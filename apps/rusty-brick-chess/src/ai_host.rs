//! The tool host an assistant session runs against.
//!
//! It is the join between the two halves of the tool surface: board and study
//! writes go to [`BoardWriteHost`] (revision-checked, mutability-checked), and
//! everything else goes to [`ReadHost`] (position, engine, sources). The agent
//! loop only ever sees one [`ToolHost`].

use rbc_ai::agent::ToolHost;
use rbc_ai::approval::{ApprovalStore, Changeset};
use rbc_ai::board::{BoardContent, BoardWriteHost};
use rbc_ai::policy::Origin;
use rbc_ai::protocol::{ToolCall, ToolResult};
use rbc_ai::reads::{Engine, EngineLine, ReadHost};
use rbc_db::Db;

/// Tools the board host owns: the ones that write, plus reading boards and
/// studies back. Everything else is served by [`ReadHost`].
const BOARD_TOOLS: &[&str] = &[
    "play_move",
    "draw_arrow",
    "draw_circle",
    "clear_shapes",
    "annotate",
    "write_note",
    "create_board",
    "copy_board",
    "lock_board",
    "unlock_board",
    "create_study",
    // Board and study reads live with the writes so there is one
    // implementation of each, over one repository handle.
    "list_boards",
    "get_board",
    "list_studies",
    "get_study",
];

/// A host over the repository, with an engine when one is configured.
pub(crate) struct SessionHost<'a> {
    db: &'a Db,
    board: BoardWriteHost<'a>,
    reads: ReadHost<'a>,
    approvals: ApprovalStore<'a>,
    session_id: String,
    origin: Origin,
}

impl<'a> SessionHost<'a> {
    /// Build the host; `engine` is optional because a session may run without one.
    pub fn new(
        db: &'a Db,
        actor: &'a str,
        engine: Option<&'a dyn Engine>,
        session_id: impl Into<String>,
        origin: Origin,
    ) -> Self {
        let reads = match engine {
            Some(engine) => ReadHost::new(db).with_engine(engine),
            None => ReadHost::new(db),
        };
        Self {
            db,
            board: BoardWriteHost::new(db, actor),
            reads,
            approvals: ApprovalStore::new(db),
            session_id: session_id.into(),
            origin,
        }
    }
}

impl ToolHost for SessionHost<'_> {
    fn call(&mut self, call: &ToolCall) -> ToolResult {
        if call.name == "approval_status" {
            return self.approval_status(call);
        }
        if BOARD_TOOLS.contains(&call.name.as_str()) {
            self.board.call(call)
        } else {
            self.reads.call(call)
        }
    }

    /// Queue the write for a person instead of performing it.
    ///
    /// The changeset shows the board as it stands and the call that wants to
    /// change it. It does not yet show the *result* of the write: computing
    /// that needs a dry run of each tool, which lands with the approval UI.
    fn request_approval(&mut self, call: &ToolCall, summary: String) -> Option<String> {
        let board_id = call
            .arguments
            .get("board_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        let (title, revision, before, start_fen) = match self.db.board(&board_id) {
            Ok(Some(board)) => (
                board.title.clone(),
                board.revision,
                BoardContent::parse(&board.content_json).unwrap_or_default(),
                board.start_fen.clone(),
            ),
            _ => (String::new(), 0, BoardContent::default(), String::new()),
        };
        // Show the board as it would be where that is cheap to compute; a
        // preview that cannot be built falls back to the current board rather
        // than guessing.
        let after = match call.name.as_str() {
            "play_move" => call
                .arguments
                .get("move")
                .and_then(|mv| mv.as_str())
                .and_then(|text| before.with_move(&start_fen, text).ok()),
            _ => None,
        }
        .unwrap_or_else(|| before.clone());
        let mut changeset =
            Changeset::for_board(&board_id, title, revision, call.name.clone(), before, after)
                .with_arguments(call.arguments.clone());
        changeset.summary = summary;
        if changeset.notes == ["no visible change"] {
            // Nothing to diff; say what was asked instead.
            changeset.notes = vec![format!(
                "requested {} {}",
                call.name,
                serde_json::to_string(&call.arguments).unwrap_or_default()
            )];
        }
        self.approvals
            .request(
                self.origin,
                Some(&self.session_id),
                call,
                &changeset,
                now_ms(),
            )
            .ok()
            .map(|request| request.id)
    }
}

/// The wall clock, in Unix milliseconds.
pub(crate) fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

/// A local UCI engine, kept alive for the length of one turn.
///
/// The assistant's `analyze` and `evaluate` tools need a real engine, and the
/// fingerprint in the answer is what makes a stored turn reproducible: it names
/// the binary that produced the numbers.
pub(crate) struct LocalEngine {
    engine: std::sync::Mutex<knightlm_engine::Engine>,
    label: String,
}

impl LocalEngine {
    /// Start an engine at `path`.
    pub fn spawn(path: &str, options: knightlm_engine::EngineOptions) -> anyhow::Result<Self> {
        let engine = knightlm_engine::Engine::spawn_with(path, 60, options)?;
        let label = format!("{} {}", engine.name(), engine.version())
            .trim()
            .to_string();
        Ok(Self {
            engine: std::sync::Mutex::new(engine),
            label,
        })
    }
}

impl Engine for LocalEngine {
    fn analyse(&self, fen: &str, depth: u32) -> Result<EngineLine, String> {
        let mut engine = self
            .engine
            .lock()
            .map_err(|_| "the engine session was lost".to_string())?;
        let analysis = engine
            .analyse(fen, depth)
            .map_err(|error| format!("the engine failed: {error}"))?;
        Ok(EngineLine {
            best_move: (!analysis.best_move.trim().is_empty()).then_some(analysis.best_move),
            score_cp: analysis.score_cp,
            mate_in: analysis.mate,
            pv: analysis.pv,
            depth: analysis.depth,
        })
    }

    fn fingerprint(&self) -> String {
        self.label.clone()
    }
}

impl<'a> SessionHost<'a> {
    /// Report one approval's state, for a client polling after
    /// `approval_required`.
    fn approval_status(&self, call: &ToolCall) -> ToolResult {
        let Some(id) = call.arguments.get("approval_id").and_then(|v| v.as_str()) else {
            return rbc_ai::result::refused(
                call,
                rbc_ai::protocol::result_code::INVALID_ARGUMENTS,
                "approval_status needs an approval_id",
            );
        };
        match self.approvals.get(id) {
            Ok(Some(request)) => {
                let status = match request.status {
                    rbc_ai::protocol::ApprovalStatus::Pending => "pending",
                    rbc_ai::protocol::ApprovalStatus::Approved => "approved",
                    rbc_ai::protocol::ApprovalStatus::Denied => "denied",
                    rbc_ai::protocol::ApprovalStatus::Expired => "expired",
                };
                rbc_ai::result::ok(
                    call,
                    format!("approval {status}"),
                    serde_json::json!({
                        "approval_id": request.id,
                        "status": status,
                        "tool": request.tool,
                        "changeset_summary": request.changeset_summary,
                        "revision": null,
                    }),
                )
            }
            Ok(None) => rbc_ai::result::refused(
                call,
                rbc_ai::protocol::result_code::NOT_FOUND,
                format!("no approval {id}"),
            ),
            Err(error) => rbc_ai::result::refused(
                call,
                rbc_ai::protocol::result_code::INTERNAL,
                error.to_string(),
            ),
        }
    }
}
