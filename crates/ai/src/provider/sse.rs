//! Server-sent-event parsing.
//!
//! The wire format is line oriented: `field: value` lines accumulate into one
//! event, and a blank line dispatches it. This module implements the parts a
//! chat endpoint actually uses:
//!
//! - `data` (multi-line values are joined with `\n`),
//! - `event` and `id`,
//! - comment lines (`: keep-alive`) are ignored,
//! - unknown fields are ignored,
//! - a final event is dispatched at end of stream even without a blank line,
//!   because some servers close the socket instead of writing the separator.
//!
//! The parser is fed arbitrary byte boundaries: a chunk may end in the middle
//! of a line, or in the middle of a multi-byte character. Nothing is decoded
//! until a full line is available, so a split character is never corrupted.

/// One dispatched event.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SseEvent {
    /// The `event:` field, when the server sent one.
    pub event: Option<String>,
    /// The `id:` field, when the server sent one.
    pub id: Option<String>,
    /// The joined `data:` value. Empty for a bare keep-alive.
    pub data: String,
}

impl SseEvent {
    /// Whether this event carries data worth parsing.
    pub fn has_data(&self) -> bool {
        !self.data.trim().is_empty()
    }
}

/// An incremental parser. Feed it chunks, collect complete events.
#[derive(Debug, Default)]
pub struct SseParser {
    /// Bytes of the current, incomplete line.
    pending: Vec<u8>,
    event: Option<String>,
    id: Option<String>,
    data: Option<String>,
    /// Counts dispatched events, used as a cheap sanity bound by callers.
    dispatched: u64,
}

impl SseParser {
    /// A parser with no buffered input.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many events have been dispatched so far.
    pub fn dispatched(&self) -> u64 {
        self.dispatched
    }

    /// Feed a chunk of bytes and return every event it completed.
    pub fn push(&mut self, chunk: &[u8]) -> Vec<SseEvent> {
        let mut events = Vec::new();
        // Resume the line that the previous chunk left unfinished.
        let mut line = std::mem::take(&mut self.pending);
        for &byte in chunk {
            if byte == b'\n' {
                self.push_line(&line, &mut events);
                line.clear();
            } else {
                line.push(byte);
            }
        }
        // Keep the unterminated tail for the next chunk.
        self.pending = line;
        events
    }

    /// Feed a UTF-8 chunk.
    pub fn push_str(&mut self, chunk: &str) -> Vec<SseEvent> {
        self.push(chunk.as_bytes())
    }

    /// Dispatch any event still buffered when the stream ends.
    pub fn finish(&mut self) -> Option<SseEvent> {
        if self.pending.is_empty() {
            return self.dispatch();
        }
        let tail = std::mem::take(&mut self.pending);
        let mut events = Vec::new();
        self.push_line(&tail, &mut events);
        let mut out = events.into_iter().next();
        if out.is_none() {
            out = self.dispatch();
        }
        out
    }

    fn push_line(&mut self, line: &[u8], events: &mut Vec<SseEvent>) {
        // A `\r\n` line ending leaves the CR at the end of `line`.
        let line = match line.split_last() {
            Some((b'\r', rest)) => rest,
            _ => line,
        };
        if line.is_empty() {
            if let Some(event) = self.dispatch() {
                events.push(event);
            }
            return;
        }
        // A line starting with a colon is a comment or keep-alive.
        if line[0] == b':' {
            return;
        }
        let text = String::from_utf8_lossy(line);
        let (field, value) = match text.split_once(':') {
            Some((field, value)) => {
                // Exactly one optional space after the colon is trimmed.
                (field, value.strip_prefix(' ').unwrap_or(value))
            }
            None => (text.as_ref(), ""),
        };
        match field {
            "data" => match &mut self.data {
                Some(existing) => {
                    existing.push('\n');
                    existing.push_str(value);
                }
                None => self.data = Some(value.to_string()),
            },
            "event" => self.event = Some(value.to_string()),
            "id" => self.id = Some(value.to_string()),
            // `retry` and any vendor field are not used by chat streams.
            _ => {}
        }
    }

    fn dispatch(&mut self) -> Option<SseEvent> {
        if self.data.is_none() && self.event.is_none() && self.id.is_none() {
            return None;
        }
        let event = SseEvent {
            event: self.event.take(),
            id: self.id.take(),
            data: self.data.take().unwrap_or_default(),
        };
        self.dispatched += 1;
        Some(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_all(chunks: &[&str]) -> Vec<SseEvent> {
        let mut parser = SseParser::new();
        let mut out = Vec::new();
        for chunk in chunks {
            out.extend(parser.push_str(chunk));
        }
        out.extend(parser.finish());
        out
    }

    #[test]
    fn sse_parses_one_event_per_blank_line() {
        let events = parse_all(&["data: {\"a\":1}\n\ndata: {\"b\":2}\n\n"]);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].data, "{\"a\":1}");
        assert_eq!(events[1].data, "{\"b\":2}");
        assert_eq!(events[0].event, None);
    }

    #[test]
    fn sse_joins_multiline_data_and_keeps_event_and_id() {
        let events = parse_all(&["event: message\nid: 7\ndata: line one\ndata: line two\n\n"]);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event.as_deref(), Some("message"));
        assert_eq!(events[0].id.as_deref(), Some("7"));
        assert_eq!(events[0].data, "line one\nline two");
    }

    #[test]
    fn sse_ignores_comments_and_unknown_fields() {
        let events = parse_all(&[": keep-alive\nretry: 500\nfoo: bar\ndata: x\n\n"]);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "x");
    }

    #[test]
    fn sse_handles_crlf_line_endings() {
        let events = parse_all(&["data: one\r\n\r\ndata: two\r\n\r\n"]);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].data, "one");
        assert_eq!(events[1].data, "two");
    }

    #[test]
    fn sse_reassembles_lines_split_across_chunks() {
        let events = parse_all(&["da", "ta: split", " value\n", "\n"]);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "split value");
    }

    #[test]
    fn sse_does_not_split_a_multibyte_character() {
        // "é" is two bytes; the chunk boundary falls between them.
        let mut parser = SseParser::new();
        let bytes = "data: é\n\n".as_bytes();
        let mut out = parser.push(&bytes[..7]);
        out.extend(parser.push(&bytes[7..]));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].data, "é");
    }

    #[test]
    fn sse_dispatches_a_trailing_event_without_a_blank_line() {
        let events = parse_all(&["data: {\"last\":true}"]);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "{\"last\":true}");
    }

    #[test]
    fn sse_keeps_a_data_field_without_a_value() {
        let events = parse_all(&["data:\n\n"]);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "");
        assert!(!events[0].has_data());
    }

    #[test]
    fn sse_does_not_emit_anything_for_an_empty_stream() {
        assert!(parse_all(&["", "\n\n", ": hi\n\n"]).is_empty());
    }
}
