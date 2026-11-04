//! The replayable event log behind a session or match stream.
//!
//! A turn emits normalized events as it runs ([`AiEventPayload`]). Persisting
//! them gives the client two things it cannot get from the transcript alone:
//! a cursor (`after`) to reconnect from, and the exact order in which deltas,
//! tool calls and usage arrived.
//!
//! Ids are assigned by the store, not by the caller, so two writers on one
//! stream can never hand a client two different events under the same id.

use anyhow::{Context as _, Result};

use crate::protocol::{AiEvent, AiEventPayload};
use rbc_db::Db;

/// What a stream belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamKind {
    /// An assistant session.
    Session,
    /// A match.
    Match,
}

impl StreamKind {
    /// The stable spelling stored in the database.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::Match => "match",
        }
    }
}

/// One stream: a kind and an id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamKey {
    pub kind: StreamKind,
    pub id: String,
}

impl StreamKey {
    /// The stream of an assistant session.
    pub fn session(id: impl Into<String>) -> Self {
        Self {
            kind: StreamKind::Session,
            id: id.into(),
        }
    }

    /// The stream of a match.
    pub fn game(id: impl Into<String>) -> Self {
        Self {
            kind: StreamKind::Match,
            id: id.into(),
        }
    }
}

/// The event log of one database.
pub struct EventLog<'a> {
    db: &'a Db,
}

impl<'a> EventLog<'a> {
    /// Read and write the log in `db`.
    pub fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Append one event and return it with the id it was given.
    pub fn append(&self, stream: &StreamKey, payload: &AiEventPayload) -> Result<AiEvent> {
        let json = serde_json::to_string(payload).context("serializing an event payload")?;
        let row = self
            .db
            .append_ai_event(stream.kind.as_str(), &stream.id, &json)?;
        Ok(AiEvent::new(row.event_id as u64, payload.clone()))
    }

    /// Append a whole turn's events, in order.
    ///
    /// This is what a finished turn does: the client is watching live, so a
    /// failure to record one event must not abort the turn, and the caller
    /// gets back what was actually written.
    pub fn append_all(&self, stream: &StreamKey, payloads: &[AiEventPayload]) -> Vec<AiEvent> {
        let mut written = Vec::with_capacity(payloads.len());
        for payload in payloads {
            match self.append(stream, payload) {
                Ok(event) => written.push(event),
                Err(_) => break,
            }
        }
        written
    }

    /// Every event after `after`, in order.
    pub fn replay(&self, stream: &StreamKey, after: u64) -> Result<Vec<AiEvent>> {
        let rows = self
            .db
            .ai_events(stream.kind.as_str(), &stream.id, after as i64)?;
        rows.into_iter()
            .map(|row| {
                let payload: AiEventPayload = serde_json::from_str(&row.payload_json)
                    .with_context(|| format!("event {} is not a known payload", row.event_id))?;
                Ok(AiEvent::new(row.event_id as u64, payload))
            })
            .collect()
    }

    /// The last id written on the stream, or 0 when it is empty.
    ///
    /// A client that has been away asks for this and then reconnects at it, so
    /// it neither misses events nor replays ones it has already seen.
    pub fn latest(&self, stream: &StreamKey) -> Result<u64> {
        Ok(self
            .db
            .latest_ai_event_id(stream.kind.as_str(), &stream.id)? as u64)
    }

    /// Whether the stream has anything after `after`.
    pub fn has_more(&self, stream: &StreamKey, after: u64) -> Result<bool> {
        Ok(self.latest(stream)? > after)
    }
}
