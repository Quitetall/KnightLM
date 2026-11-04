//! The approval queue: what a write would have done, and who said yes.
//!
//! A tool call that the policy does not auto-approve becomes a **request**, not
//! an action. The request carries a [`Changeset`] — the board before, the board
//! after, and a line per difference — so a person can judge it without the model
//! being involved again.
//!
//! The lifecycle is deliberately small:
//!
//! ```text
//! pending --approve--> approved
//! pending --deny-----> denied
//! pending --deadline->  expired
//! ```
//!
//! Only `pending` moves. Resolving twice, or resolving something the deadline
//! already passed, is refused by name: a queue that silently accepts a second
//! answer is a queue that can be raced into applying a write nobody approved.
//!
//! Auto-approval lives in [`crate::policy`], not here: a scope the profile
//! grants means the call never becomes a request at all, which is why no record
//! exists for it.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::board::BoardContent;
use crate::protocol::{ApprovalRequest, ApprovalStatus, Origin, ToolCall};
use rbc_db::{AiApproval, AiApprovalInput, Db};

/// How long an untouched request stays actionable: fifteen minutes.
pub const DEFAULT_TTL_MS: i64 = 15 * 60 * 1000;

/// One board write, as it stands and as it would stand.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoardChange {
    pub board_id: String,
    pub title: String,
    /// The revision the write would carry; a stale one is a conflict, not an
    /// approval.
    pub expected_revision: i64,
    pub action: String,
    pub before: BoardContent,
    pub after: BoardContent,
}

/// What a pending request would change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Changeset {
    /// One line for the queue row.
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub board: Option<BoardChange>,
    /// One line per difference, so the queue can show a diff without parsing
    /// the two contents.
    pub notes: Vec<String>,
    /// The arguments the approved call is replayed with, so answering "yes"
    /// can execute exactly what was reviewed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<serde_json::Value>,
}

impl Changeset {
    /// A board write: the stored content and the proposed one.
    pub fn for_board(
        board_id: impl Into<String>,
        title: impl Into<String>,
        expected_revision: i64,
        action: impl Into<String>,
        before: BoardContent,
        after: BoardContent,
    ) -> Self {
        let action = action.into();
        let notes = note_changes(&before, &after);
        let summary = match notes.first() {
            Some(first) => format!("{action}: {first}"),
            None => action.clone(),
        };
        Self {
            summary,
            board: Some(BoardChange {
                board_id: board_id.into(),
                title: title.into(),
                expected_revision,
                action,
                before,
                after,
            }),
            notes,
            arguments: None,
        }
    }

    /// Record the arguments to replay when the request is approved.
    pub fn with_arguments(mut self, arguments: serde_json::Value) -> Self {
        self.arguments = Some(arguments);
        self
    }

    /// The stored JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }
}

/// One line per field that differs between two board contents.
fn note_changes(before: &BoardContent, after: &BoardContent) -> Vec<String> {
    let mut notes = Vec::new();
    if before.moves.len() != after.moves.len() {
        let added = after
            .moves
            .get(before.moves.len().min(after.moves.len())..)
            .unwrap_or_default();
        notes.push(if added.is_empty() {
            format!("moves {} -> {}", before.moves.len(), after.moves.len())
        } else {
            format!(
                "moves {} -> {} ({})",
                before.moves.len(),
                after.moves.len(),
                added.join(" ")
            )
        });
    }
    if before.arrows.len() != after.arrows.len() {
        notes.push(format!(
            "arrows {} -> {}",
            before.arrows.len(),
            after.arrows.len()
        ));
    }
    if before.circles.len() != after.circles.len() {
        notes.push(format!(
            "circles {} -> {}",
            before.circles.len(),
            after.circles.len()
        ));
    }
    if before.annotations.len() != after.annotations.len() {
        notes.push(format!(
            "annotations {} -> {}",
            before.annotations.len(),
            after.annotations.len()
        ));
    }
    if before.notes_md != after.notes_md {
        notes.push(format!(
            "notes {} -> {} chars",
            before.notes_md.chars().count(),
            after.notes_md.chars().count()
        ));
    }
    if notes.is_empty() {
        notes.push("no visible change".to_string());
    }
    notes
}

/// Why a lifecycle change was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalError {
    /// No request with that id.
    NotFound(String),
    /// It was already answered.
    AlreadyResolved(ApprovalStatus),
    /// The deadline passed; the request is now `expired`.
    Expired,
}

impl std::fmt::Display for ApprovalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "no approval {id}"),
            Self::AlreadyResolved(status) => {
                write!(f, "the approval was already resolved as {status:?}")
            }
            Self::Expired => f.write_str("the approval expired before it was answered"),
        }
    }
}

impl std::error::Error for ApprovalError {}

/// The queue over one database.
pub struct ApprovalStore<'a> {
    db: &'a Db,
    ttl_ms: i64,
}

impl<'a> ApprovalStore<'a> {
    /// A queue whose requests live for [`DEFAULT_TTL_MS`].
    pub fn new(db: &'a Db) -> Self {
        Self {
            db,
            ttl_ms: DEFAULT_TTL_MS,
        }
    }

    /// A queue with a deadline of its own, for tests and for a short-lived job.
    pub fn with_ttl(db: &'a Db, ttl_ms: i64) -> Self {
        Self { db, ttl_ms }
    }

    /// Queue a request for a call the policy would not auto-approve.
    pub fn request(
        &self,
        origin: Origin,
        session_id: Option<&str>,
        call: &ToolCall,
        changeset: &Changeset,
        now: i64,
    ) -> Result<ApprovalRequest> {
        let (target_kind, target_id) = match &changeset.board {
            Some(board) => ("board", board.board_id.clone()),
            None => ("", String::new()),
        };
        let row = self.db.create_ai_approval(&AiApprovalInput {
            origin: origin.as_str().to_string(),
            session_id: session_id.map(str::to_string),
            target_kind: target_kind.to_string(),
            target_id,
            action: changeset
                .board
                .as_ref()
                .map(|board| board.action.clone())
                .unwrap_or_else(|| call.name.clone()),
            tool: call.name.clone(),
            summary: changeset.summary.clone(),
            changeset_json: changeset.to_json(),
            created: now,
            expires: Some(now + self.ttl_ms.max(0)),
        })?;
        Ok(to_request(row))
    }

    /// Read one request.
    pub fn get(&self, id: &str) -> Result<Option<ApprovalRequest>> {
        Ok(self.db.ai_approval(id)?.map(to_request))
    }

    /// The status of one request, for an MCP client that polls.
    pub fn status(&self, id: &str) -> Result<Option<ApprovalStatus>> {
        Ok(self
            .db
            .ai_approval(id)?
            .map(|row| parse_status(&row.status)))
    }

    /// Everything still actionable, oldest first, after expiring what is due.
    pub fn pending(&self, now: i64) -> Result<Vec<ApprovalRequest>> {
        self.expire_due(now)?;
        Ok(self
            .db
            .ai_approvals("pending")?
            .into_iter()
            .map(to_request)
            .collect())
    }

    /// Answers a pending request.
    pub fn approve(&self, id: &str, now: i64) -> Result<ApprovalRequest, ApprovalError> {
        self.resolve(id, "approved", now)
    }

    /// Refuses a pending request.
    pub fn deny(&self, id: &str, now: i64) -> Result<ApprovalRequest, ApprovalError> {
        self.resolve(id, "denied", now)
    }

    /// Expire every request whose deadline has passed.
    pub fn expire_due(&self, now: i64) -> Result<usize> {
        Ok(self.db.expire_ai_approvals(now)?)
    }

    fn resolve(&self, id: &str, status: &str, now: i64) -> Result<ApprovalRequest, ApprovalError> {
        let row = self
            .db
            .ai_approval(id)
            .map_err(|_| ApprovalError::NotFound(id.to_string()))?
            .ok_or_else(|| ApprovalError::NotFound(id.to_string()))?;
        if row.status != "pending" {
            return Err(ApprovalError::AlreadyResolved(parse_status(&row.status)));
        }
        if row.expires.is_some_and(|expires| now >= expires) {
            let _ = self.expire_due(now);
            return Err(ApprovalError::Expired);
        }
        // The update only matches a pending row, so a request answered by
        // someone else in the meantime is reported rather than overwritten.
        match self
            .db
            .resolve_ai_approval(id, status, now)
            .map_err(|_| ApprovalError::NotFound(id.to_string()))?
        {
            Some(resolved) => Ok(to_request(resolved)),
            None => {
                let current = self
                    .db
                    .ai_approval(id)
                    .ok()
                    .flatten()
                    .map(|row| parse_status(&row.status))
                    .unwrap_or(ApprovalStatus::Pending);
                Err(ApprovalError::AlreadyResolved(current))
            }
        }
    }
}

fn to_request(row: AiApproval) -> ApprovalRequest {
    ApprovalRequest {
        id: row.id,
        origin: match row.origin.as_str() {
            "mcp" => Origin::Mcp,
            _ => Origin::InApp,
        },
        session_id: row.session_id,
        target_kind: (!row.target_kind.is_empty()).then_some(row.target_kind),
        target_id: (!row.target_id.is_empty()).then_some(row.target_id),
        action: row.action,
        tool: row.tool,
        changeset_summary: row.summary,
        changeset: serde_json::from_str(&row.changeset_json).ok(),
        status: parse_status(&row.status),
        created_at: row.created,
        resolved_at: row.resolved,
        expires_at: row.expires,
    }
}

fn parse_status(status: &str) -> ApprovalStatus {
    match status {
        "approved" => ApprovalStatus::Approved,
        "denied" => ApprovalStatus::Denied,
        "expired" => ApprovalStatus::Expired,
        _ => ApprovalStatus::Pending,
    }
}

/// A changeset read back from storage.
pub fn changeset_of(request: &ApprovalRequest) -> Option<Changeset> {
    request
        .changeset
        .as_ref()
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .with_context(|| format!("approval {} has an unreadable changeset", request.id))
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content(moves: &[&str]) -> BoardContent {
        BoardContent {
            moves: moves.iter().map(|mv| mv.to_string()).collect(),
            ..BoardContent::default()
        }
    }

    #[test]
    fn approvals_changeset_notes_the_move_that_would_be_played() {
        let changeset = Changeset::for_board(
            "b1",
            "Board",
            3,
            "play_move",
            content(&["e4"]),
            content(&["e4", "e5"]),
        );
        assert_eq!(changeset.notes, vec!["moves 1 -> 2 (e5)"]);
        assert_eq!(changeset.summary, "play_move: moves 1 -> 2 (e5)");
        let board = changeset.board.expect("board change");
        assert_eq!(board.expected_revision, 3);
        assert_eq!(board.action, "play_move");
    }

    #[test]
    fn approvals_changeset_says_when_nothing_visible_changes() {
        let changeset = Changeset::for_board(
            "b1",
            "Board",
            1,
            "clear_shapes",
            content(&["e4"]),
            content(&["e4"]),
        );
        assert_eq!(changeset.notes, vec!["no visible change"]);
    }

    #[test]
    fn approvals_changeset_counts_arrows_and_notes() {
        let mut after = content(&[]);
        after.arrows.push(crate::board::Arrow {
            from: "e2".into(),
            to: "e4".into(),
            color: "green".into(),
        });
        after.notes_md = "a plan".into();
        let changeset = Changeset::for_board("b1", "Board", 1, "draw_arrow", content(&[]), after);
        assert!(
            changeset
                .notes
                .iter()
                .any(|note| note.contains("arrows 0 -> 1"))
        );
        assert!(
            changeset
                .notes
                .iter()
                .any(|note| note.contains("notes 0 -> 6 chars"))
        );
    }

    #[test]
    fn approvals_status_strings_round_trip() {
        assert_eq!(parse_status("pending"), ApprovalStatus::Pending);
        assert_eq!(parse_status("approved"), ApprovalStatus::Approved);
        assert_eq!(parse_status("denied"), ApprovalStatus::Denied);
        assert_eq!(parse_status("expired"), ApprovalStatus::Expired);
        assert_eq!(parse_status("something new"), ApprovalStatus::Pending);
    }
}
