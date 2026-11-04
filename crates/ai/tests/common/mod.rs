//! A scriptable OpenAI-compatible endpoint, on localhost, in-process.
//!
//! The provider adapter is only really tested when something speaks HTTP at it:
//! a canned `CompletionResponse` proves the parsing helpers, not the wire. This
//! server answers `/v1/chat` (and any other path) with whatever a test scripts —
//! JSON, a chunked SSE stream, a 429, a 500, or nothing at all — and records the
//! request bodies so a test can assert what was actually sent.
//!
//! Nothing here touches the network beyond `127.0.0.1`, and the port is chosen
//! by the OS, so suites can run concurrently.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// One scripted reply.
#[derive(Debug, Clone)]
pub enum Reply {
    /// A JSON body with 200.
    Json(String),
    /// A JSON body with a chosen status.
    Status(u16, String),
    /// A chunked `text/event-stream` body of complete `data:` frames.
    Sse(Vec<String>),
    /// Accept the request and never answer, so the client's timeout fires.
    Hang,
    /// Accept the request and close the socket without a response.
    Drop,
}

impl Reply {
    /// A completion whose content is `content`.
    pub fn content(content: &str) -> Self {
        Self::Json(
            format!(
                r#"{{"id":"mock","choices":[{{"message":{{"content":{}}},"finish_reason":"stop"}}],"usage":{{"prompt_tokens":10,"completion_tokens":2}}}}"#,
                serde_json::to_string(content).expect("json")
            ),
        )
    }

    /// A completion that also carries reasoning.
    pub fn content_with_reasoning(content: &str, reasoning: &str) -> Self {
        Self::Json(format!(
            r#"{{"id":"mock","choices":[{{"message":{{"content":{},"reasoning_content":{}}},"finish_reason":"stop"}}],"usage":{{"prompt_tokens":10,"completion_tokens":5}}}}"#,
            serde_json::to_string(content).expect("json"),
            serde_json::to_string(reasoning).expect("json")
        ))
    }

    /// A 429 with a provider-shaped error body.
    pub fn rate_limited() -> Self {
        Self::Status(
            429,
            r#"{"error":{"message":"slow down","type":"rate_limit_exceeded"}}"#.to_string(),
        )
    }

    /// A 500 with a provider-shaped error body.
    pub fn server_error() -> Self {
        Self::Status(
            500,
            r#"{"error":{"message":"upstream exploded","type":"server_error"}}"#.to_string(),
        )
    }

    /// A completion that asks for a tool call.
    pub fn tool_call(name: &str, arguments: serde_json::Value) -> Self {
        Self::Json(format!(
            r#"{{"id":"mock","choices":[{{"message":{{"content":null,"tool_calls":[{{"id":"call_1","type":"function","function":{{"name":{},"arguments":{}}}}}]}},"finish_reason":"tool_calls"}}],"usage":{{"prompt_tokens":10,"completion_tokens":4}}}}"#,
            serde_json::to_string(name).expect("json"),
            serde_json::to_string(&arguments.to_string()).expect("json")
        ))
    }

    /// An SSE stream that delivers `content` in `deltas`, then a usage frame.
    pub fn stream(content: &str, deltas: usize) -> Self {
        let mut frames = Vec::new();
        let chunk = (content.len() / deltas.max(1)).max(1);
        let mut rest = content;
        while !rest.is_empty() {
            let take = chunk.min(rest.len());
            let (head, tail) = rest.split_at(take);
            frames.push(format!(
                r#"{{"id":"mock","choices":[{{"index":0,"delta":{{"content":{}}}}}]}}"#,
                serde_json::to_string(head).expect("json")
            ));
            rest = tail;
        }
        frames.push(
            r#"{"id":"mock","choices":[],"usage":{"prompt_tokens":12,"completion_tokens":4}}"#
                .to_string(),
        );
        Self::Sse(frames)
    }
}

/// A request the server received.
#[derive(Debug, Clone)]
pub struct Recorded {
    pub path: String,
    pub body: String,
}

/// The scripted server.
pub struct MockServer {
    port: u16,
    requests: Arc<Mutex<Vec<Recorded>>>,
}

impl MockServer {
    /// Start a server that answers with `replies` in order, then repeats the last.
    pub fn start(replies: Vec<Reply>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&requests);
        std::thread::spawn(move || {
            let mut index = 0usize;
            for stream in listener.incoming() {
                let Ok(stream) = stream else { break };
                let reply = replies
                    .get(index.min(replies.len().saturating_sub(1)))
                    .cloned()
                    .unwrap_or(Reply::Drop);
                index += 1;
                let seen = Arc::clone(&seen);
                std::thread::spawn(move || {
                    serve(stream, reply, seen);
                });
            }
        });
        Self { port, requests }
    }

    /// The URL an `OpenAiProvider` should be pointed at.
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}/v1/chat", self.port)
    }

    /// What the server has been asked so far.
    pub fn requests(&self) -> Vec<Recorded> {
        self.requests.lock().expect("requests").clone()
    }

    /// How many requests arrived.
    pub fn count(&self) -> usize {
        self.requests.lock().expect("requests").len()
    }
}

fn serve(mut stream: TcpStream, reply: Reply, seen: Arc<Mutex<Vec<Recorded>>>) {
    let Some(request) = read_request(&mut stream) else {
        return;
    };
    seen.lock().expect("requests").push(request);

    match reply {
        Reply::Drop => {}
        Reply::Hang => {
            // Hold the connection open until the client gives up. The write
            // never happens, so the client's own timeout decides.
            std::thread::sleep(Duration::from_secs(30));
        }
        Reply::Json(body) => write_response(&mut stream, 200, "application/json", &body),
        Reply::Status(code, body) => write_response(&mut stream, code, "application/json", &body),
        Reply::Sse(frames) => {
            let head = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\n\
                        transfer-encoding: chunked\r\nconnection: close\r\n\r\n";
            if stream.write_all(head.as_bytes()).is_err() {
                return;
            }
            for frame in frames {
                let payload = format!("data: {frame}\n\n");
                let chunk = format!("{:x}\r\n{payload}\r\n", payload.len());
                if stream.write_all(chunk.as_bytes()).is_err() {
                    return;
                }
                let _ = stream.flush();
            }
            let done = "data: [DONE]\n\n";
            let chunk = format!("{:x}\r\n{done}\r\n", done.len());
            let _ = stream.write_all(chunk.as_bytes());
            let _ = stream.write_all(b"0\r\n\r\n");
            let _ = stream.flush();
        }
    }
}

fn read_request(stream: &mut TcpStream) -> Option<Recorded> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line).ok()?;
    let path = request_line.split_whitespace().nth(1)?.to_string();

    let mut length = 0usize;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).ok()? == 0 {
            break;
        }
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().unwrap_or(0);
            }
        }
    }
    let mut body = vec![0u8; length];
    if length > 0 {
        reader.read_exact(&mut body).ok()?;
    }
    Some(Recorded {
        path,
        body: String::from_utf8_lossy(&body).to_string(),
    })
}

fn write_response(stream: &mut TcpStream, status: u16, content_type: &str, body: &str) {
    let head = format!(
        "HTTP/1.1 {status} X\r\ncontent-type: {content_type}\r\n\
         content-length: {}\r\nconnection: close\r\n\r\n",
        body.len()
    );
    if stream.write_all(head.as_bytes()).is_ok() {
        let _ = stream.write_all(body.as_bytes());
        let _ = stream.flush();
    }
}
