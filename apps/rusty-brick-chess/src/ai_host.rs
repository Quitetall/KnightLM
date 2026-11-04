//! The tool host an assistant session runs against.
//!
//! It is the join between the two halves of the tool surface: board and study
//! writes go to [`BoardWriteHost`] (revision-checked, mutability-checked), and
//! everything else goes to [`ReadHost`] (position, engine, sources). The agent
//! loop only ever sees one [`ToolHost`].

use rbc_ai::agent::ToolHost;
use rbc_ai::board::BoardWriteHost;
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
    board: BoardWriteHost<'a>,
    reads: ReadHost<'a>,
}

impl<'a> SessionHost<'a> {
    /// Build the host; `engine` is optional because a session may run without one.
    pub fn new(db: &'a Db, actor: &'a str, engine: Option<&'a dyn Engine>) -> Self {
        let reads = match engine {
            Some(engine) => ReadHost::new(db).with_engine(engine),
            None => ReadHost::new(db),
        };
        Self {
            board: BoardWriteHost::new(db, actor),
            reads,
        }
    }
}

impl ToolHost for SessionHost<'_> {
    fn call(&mut self, call: &ToolCall) -> ToolResult {
        if BOARD_TOOLS.contains(&call.name.as_str()) {
            self.board.call(call)
        } else {
            self.reads.call(call)
        }
    }
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
