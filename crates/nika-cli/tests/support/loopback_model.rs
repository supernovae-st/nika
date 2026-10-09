// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A loopback model for the Session door witnesses: the OpenAI-compatible wire a local engine's
//! base URL override reaches (`NIKA_VLLM_BASE_URL`), every request body kept, each answered with
//! the same words. No key, no network beyond 127.0.0.1.

use std::io::{Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use serde_json::{Value, json};

/// What the loopback model says to every request.
pub(crate) const SAID: &str = "Here to help with your automation.";

/// The kept choice of a local engine's model served by this loopback (`vllm`).
pub(crate) const CHOICE: &str = r#"{"kind":{"kind":"local","provider":"vllm"},"model":"vllm/s03-seat","chosen_at":"2026-10-09T00:00:00Z"}"#;

/// A loopback model on the OpenAI-compatible wire (the vLLM route's base URL override): it keeps
/// every request body it receives and answers each with [`SAID`]. No key, no network beyond
/// 127.0.0.1; dropping it stops and joins its thread.
pub(crate) struct Model {
    port: u16,
    bodies: Arc<Mutex<Vec<Value>>>,
    stop: Arc<AtomicBool>,
    server: Option<JoinHandle<()>>,
}

impl Model {
    // The synchronous fixture owns this blocking socket thread and joins it on drop.
    #[allow(clippy::disallowed_methods)]
    pub(crate) fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback model");
        let port = listener.local_addr().expect("address").port();
        let bodies = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (seen, halt) = (Arc::clone(&bodies), Arc::clone(&stop));
        let server = std::thread::spawn(move || {
            for stream in listener.incoming() {
                if halt.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(mut stream) = stream else { continue };
                if let Some(body) = read_body(&mut stream) {
                    seen.lock().expect("model log").push(body);
                    answer(&mut stream);
                }
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

    /// This model kept as the intelligence of every conversation opened with `home`.
    pub(crate) fn chosen_in(home: &std::path::Path) {
        std::fs::create_dir_all(home.join(".nika")).expect("home");
        std::fs::write(home.join(".nika/session-intelligence.json"), CHOICE).expect("the choice");
    }

    pub(crate) fn bodies(&self) -> Vec<Value> {
        self.bodies.lock().expect("model log").clone()
    }
}

impl Drop for Model {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(server) = self.server.take() {
            let _ = server.join();
        }
    }
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

/// One chat completion carrying [`SAID`].
fn answer(stream: &mut TcpStream) {
    let body = json!({
        "id": "chatcmpl-machine", "object": "chat.completion",
        "choices": [{"index": 0, "message": {"role": "assistant", "content": SAID}, "finish_reason": "stop"}],
        "usage": {"prompt_tokens": 100, "completion_tokens": 10, "total_tokens": 110},
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
