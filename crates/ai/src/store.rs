//! SQLite-backed session/turn store for the assistant.
//!
//! The tables live in the schema-v4 database (see `rbc-db`). This module is the
//! typed boundary: protocol DTOs in, protocol DTOs out, JSON columns inside.

use anyhow::Result;

use crate::policy::PermissionProfile;
use crate::protocol::{
    AiSession, AiTurn, ContextSpecRef, ErrorInfo, ModelRef, SessionMode, TurnRole, Usage,
};

/// How a session talks: the model it uses, the context it compiles with, and
/// what it may do without asking.
///
/// Three separate references that always travel together; passing them as
/// consecutive arguments made the call sites a guessing game.
#[derive(Debug, Clone, Copy)]
pub struct SessionTalk<'a> {
    pub model: &'a ModelRef,
    pub context_spec: &'a ContextSpecRef,
    pub permissions: &'a PermissionProfile,
}

pub struct Store {
    db: rbc_db::Db,
}

impl Store {
    pub fn open(path: &str) -> Result<Self> {
        Ok(Self {
            db: rbc_db::Db::open(path)?,
        })
    }

    pub fn in_memory() -> Result<Self> {
        Ok(Self {
            db: rbc_db::Db::open_memory()?,
        })
    }

    pub fn db(&self) -> &rbc_db::Db {
        &self.db
    }

    pub fn create_session(
        &self,
        title: &str,
        mode: SessionMode,
        home_board_id: Option<&str>,
        target_board_id: Option<&str>,
        talk: SessionTalk<'_>,
    ) -> Result<AiSession> {
        let SessionTalk {
            model,
            context_spec,
            permissions,
        } = talk;
        let model_json = serde_json::to_string(model)?;
        let context_spec_json = serde_json::to_string(context_spec)?;
        let permissions_json = serde_json::to_string(permissions)?;
        let row = self.db.create_ai_session(
            title,
            mode.as_str(),
            home_board_id,
            target_board_id,
            rbc_db::SessionProfile {
                model_json: &model_json,
                context_spec_json: &context_spec_json,
                permissions_json: &permissions_json,
            },
        )?;
        Ok(session_from_row(row))
    }

    pub fn sessions(&self) -> Result<Vec<AiSession>> {
        Ok(self
            .db
            .ai_sessions()?
            .into_iter()
            .map(session_from_row)
            .collect())
    }

    pub fn session(&self, id: &str) -> Result<Option<AiSession>> {
        Ok(self.db.ai_session(id)?.map(session_from_row))
    }

    pub fn rename(&self, id: &str, title: &str) -> Result<AiSession> {
        Ok(session_from_row(self.db.rename_ai_session(id, title)?))
    }

    pub fn archive(&self, id: &str) -> Result<()> {
        self.db.archive_ai_session(id)
    }

    pub fn set_target(&self, id: &str, board_id: Option<&str>) -> Result<AiSession> {
        Ok(session_from_row(
            self.db.set_ai_session_target(id, board_id)?,
        ))
    }

    pub fn append_turn(&self, session_id: &str, turn: &AiTurn) -> Result<AiTurn> {
        let input = rbc_db::AiTurnInput {
            role: turn.role.as_str().into(),
            content: turn.content.clone(),
            reasoning: turn.reasoning.clone(),
            tool_calls_json: serde_json::to_string(&turn.tool_calls)?,
            tool_results_json: serde_json::to_string(&turn.tool_results)?,
            context_json: turn.context_json.clone(),
            context_sha256: turn.context_sha256.clone(),
            spec_version: turn.spec_version.clone(),
            template_version: turn.template_version.clone(),
            tool_schema_version: turn.tool_schema_version.clone(),
            engine_fingerprint: turn.engine_fingerprint.clone(),
            model_json: serde_json::to_string(&turn.model)?,
            tokens_in: turn.usage.tokens_in as i64,
            tokens_out: turn.usage.tokens_out as i64,
            latency_ms: turn.usage.latency_ms as i64,
            error_json: turn.error.as_ref().map(serde_json::to_string).transpose()?,
        };
        Ok(turn_from_row(self.db.append_ai_turn(session_id, &input)?))
    }

    pub fn turns(&self, session_id: &str) -> Result<Vec<AiTurn>> {
        Ok(self
            .db
            .ai_turns(session_id)?
            .into_iter()
            .map(turn_from_row)
            .collect())
    }
}

fn session_from_row(row: rbc_db::AiSession) -> AiSession {
    AiSession {
        id: row.id,
        title: row.title,
        mode: match row.mode.as_str() {
            "play" => SessionMode::Play,
            "watch" => SessionMode::Watch,
            _ => SessionMode::Chat,
        },
        home_board_id: row.home_board_id,
        target_board_id: row.target_board_id,
        model: serde_json::from_str(&row.model_json)
            .unwrap_or_else(|_| ModelRef::new("unknown", "unknown")),
        context_spec: serde_json::from_str(&row.context_spec_json)
            .unwrap_or_else(|_| ContextSpecRef::new("unknown", "unknown")),
        permissions: serde_json::from_str(&row.permissions_json).unwrap_or_default(),
        created_at: row.created,
        updated_at: row.updated,
        archived: row.archived,
    }
}

fn turn_from_row(row: rbc_db::AiTurn) -> AiTurn {
    AiTurn {
        id: row.id,
        session_id: row.session_id,
        seq: row.seq as u64,
        role: match row.role.as_str() {
            "assistant" => TurnRole::Assistant,
            "tool" => TurnRole::Tool,
            "system" => TurnRole::System,
            _ => TurnRole::User,
        },
        content: row.content,
        reasoning: row.reasoning,
        tool_calls: serde_json::from_str(&row.tool_calls_json).unwrap_or_default(),
        tool_results: serde_json::from_str(&row.tool_results_json).unwrap_or_default(),
        context_json: row.context_json,
        context_sha256: row.context_sha256,
        spec_version: row.spec_version,
        template_version: row.template_version,
        tool_schema_version: row.tool_schema_version,
        engine_fingerprint: row.engine_fingerprint,
        model: serde_json::from_str(&row.model_json)
            .unwrap_or_else(|_| ModelRef::new("unknown", "unknown")),
        usage: Usage {
            tokens_in: row.tokens_in as u32,
            tokens_out: row.tokens_out as u32,
            latency_ms: row.latency_ms as u64,
            cost_estimate: None,
        },
        error: row
            .error_json
            .as_deref()
            .and_then(|json| serde_json::from_str::<ErrorInfo>(json).ok()),
        created_at: row.created,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(session: &str, role: TurnRole, content: &str) -> AiTurn {
        AiTurn {
            id: String::new(),
            session_id: session.into(),
            seq: 0,
            role,
            content: content.into(),
            reasoning: None,
            tool_calls: Vec::new(),
            tool_results: Vec::new(),
            context_json: "{}".into(),
            context_sha256: "hash".into(),
            spec_version: "chat-v1".into(),
            template_version: "t1".into(),
            tool_schema_version: "1".into(),
            engine_fingerprint: None,
            model: ModelRef::new("test", "model"),
            usage: Usage::default(),
            error: None,
            created_at: 0,
        }
    }

    #[test]
    fn sessions_and_turns_round_trip() {
        let store = Store::in_memory().unwrap();
        let session = store
            .create_session(
                "Test",
                SessionMode::Chat,
                None,
                None,
                SessionTalk {
                    model: &ModelRef::new("test", "model"),
                    context_spec: &ContextSpecRef::new("chat-v1", "1"),
                    permissions: &PermissionProfile::default(),
                },
            )
            .unwrap();
        let first = store
            .append_turn(&session.id, &turn(&session.id, TurnRole::User, "hi"))
            .unwrap();
        assert_eq!(first.seq, 1);
        let second = store
            .append_turn(
                &session.id,
                &turn(&session.id, TurnRole::Assistant, "hello"),
            )
            .unwrap();
        assert_eq!(second.seq, 2);
        let turns = store.turns(&session.id).unwrap();
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[1].content, "hello");
        assert_eq!(turns[1].role, TurnRole::Assistant);
        assert_eq!(store.sessions().unwrap().len(), 1);
        store.rename(&session.id, "Renamed").unwrap();
        assert_eq!(
            store.session(&session.id).unwrap().unwrap().title,
            "Renamed"
        );
    }
}
