// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A scripted author on the OpenAI-compatible wire a local engine's base URL override reaches
//! (`NIKA_VLLM_BASE_URL`): each request that offers the Session's tools takes the next step of
//! its script, a tool call (with words beside it, if any) or a reply; a request that offers no
//! tool is answered as the Session's own readings expect (a verifier's closed choice approved,
//! anything else a plain word). Every request body is kept. A double: it scripts what an author
//! decides, never what Nika checks. No key, no network beyond 127.0.0.1.

use std::io::{Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use serde_json::{Value, json};

/// The options a verifier's closed choice offers when it approves.
const APPROVALS: [&str; 5] = [
    "faithful",
    "carried",
    "consistent",
    "only_requested",
    "no_task",
];

/// The scripted author: its port, every body it received, and its thread.
pub(crate) struct Author {
    port: u16,
    bodies: Arc<Mutex<Vec<Value>>>,
    stop: Arc<AtomicBool>,
    server: Option<JoinHandle<()>>,
}

impl Author {
    /// An author that answers each request offering tools with the next step of `script`:
    /// `{"tool": name, "args": {…}}` calls a tool (`"say"` beside it adds its words),
    /// `{"say": words}` replies; past its last step, a plain word.
    // The synchronous fixture owns this blocking socket thread and joins it on drop.
    #[allow(clippy::disallowed_methods)]
    pub(crate) fn start(script: Vec<Value>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback author");
        let port = listener.local_addr().expect("address").port();
        let bodies = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (seen, halt) = (Arc::clone(&bodies), Arc::clone(&stop));
        let server = std::thread::spawn(move || {
            let mut steps = script.into_iter();
            for (calls, stream) in listener.incoming().enumerate() {
                if halt.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(mut stream) = stream else { continue };
                let Some(body) = read_body(&mut stream) else {
                    continue;
                };
                let agent = body["tools"].as_array().is_some_and(|t| !t.is_empty());
                let plain = reply(&body);
                seen.lock().expect("author log").push(body);
                let scripted = if agent { steps.next() } else { None };
                let message = match scripted {
                    Some(call) if call.get("tool").is_some() => json!({
                        "role": "assistant", "content": call.get("say").cloned(),
                        "tool_calls": [{"id": format!("call_{calls}"), "type": "function",
                            "function": {"name": call["tool"], "arguments": call["args"].to_string()}}]}),
                    Some(said) => json!({"role": "assistant", "content": said["say"]}),
                    None => plain,
                };
                answer(&mut stream, &message);
            }
        });
        Self {
            port,
            bodies,
            stop,
            server: Some(server),
        }
    }

    /// The base URL a local engine's override takes (`NIKA_VLLM_BASE_URL`).
    pub(crate) fn base(&self) -> String {
        format!("http://127.0.0.1:{}/v1", self.port)
    }

    /// Every request body received, in order.
    pub(crate) fn bodies(&self) -> Vec<Value> {
        self.bodies.lock().expect("author log").clone()
    }
}

impl Drop for Author {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(server) = self.server.take() {
            let _ = server.join();
        }
    }
}

/// What the author answers a request that offers no tool: a verifier's closed choice approved,
/// anything else a plain word.
fn reply(body: &Value) -> Value {
    let keys = &body["response_format"]["json_schema"]["schema"]["properties"]["choice"]["enum"];
    let offered = |key: &&str| keys.as_array().is_some_and(|k| k.iter().any(|k| k == *key));
    if let Some(approve) = APPROVALS.into_iter().find(offered) {
        let choice = json!({"choice": approve}).to_string();
        return json!({"role": "assistant", "content": choice});
    }
    json!({"role": "assistant", "content": "D'accord."})
}

/// One request's JSON body: the head up to its blank line, then `content-length` bytes.
fn read_body(stream: &mut TcpStream) -> Option<Value> {
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(30)))
        .ok()?;
    let (mut buffer, mut chunk) = (Vec::new(), [0_u8; 8192]);
    let end = loop {
        let n = stream.read(&mut chunk).ok().filter(|n| *n > 0)?;
        buffer.extend_from_slice(&chunk[..n]);
        if let Some(at) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            break at + 4;
        }
    };
    let head = String::from_utf8_lossy(&buffer[..end]).to_lowercase();
    let length: usize = (head.lines())
        .find_map(|line| line.strip_prefix("content-length:"))
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0);
    while buffer.len() < end + length {
        let n = stream.read(&mut chunk).ok().filter(|n| *n > 0)?;
        buffer.extend_from_slice(&chunk[..n]);
    }
    serde_json::from_slice(&buffer[end..end + length]).ok()
}

/// One chat completion carrying `message`.
fn answer(stream: &mut TcpStream, message: &Value) {
    let finish = if message["tool_calls"].is_array() {
        "tool_calls"
    } else {
        "stop"
    };
    let body = json!({
        "id": "chatcmpl-author", "object": "chat.completion", "model": "s03-seat",
        "choices": [{"index": 0, "message": message, "finish_reason": finish}],
        "usage": {"prompt_tokens": 1000, "completion_tokens": 100, "total_tokens": 1100},
    })
    .to_string();
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body.as_bytes());
    let _ = stream.flush();
}
