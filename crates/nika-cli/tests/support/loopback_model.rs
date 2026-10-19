// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A loopback model for the Session door witnesses: the OpenAI-compatible wire a local engine's
//! base URL override reaches (`NIKA_VLLM_BASE_URL`), every request body kept. By default each is
//! answered with the same words. A scripted loopback ([`Model::scripted`]) reads what each request
//! asks instead ([`Asked`]): the Session's routing request receives the next label of its script,
//! the judge's closed choice its whole-request approval. No key, no network beyond 127.0.0.1.

use std::collections::VecDeque;
use std::io::{Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use serde_json::{Value, json};

/// What the loopback model says to every request it does not read.
pub(crate) const SAID: &str = "Here to help with your automation.";

/// The kept choice of a local engine's model served by this loopback (`vllm`).
pub(crate) const CHOICE: &str = r#"{"kind":{"kind":"local","provider":"vllm"},"model":"vllm/s03-seat","chosen_at":"2026-10-09T00:00:00Z"}"#;

/// The words that open the Session's routing request: one bounded label for one line.
const ROUTE: &str = "You route ONE line a human typed";

/// The judge's closed choice, answered with the whole request's approval.
const APPROVED: &str = r#"{"choice":"faithful","model_parts":[]}"#;

/// What one request asked, read from the words of its messages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Asked {
    /// The Session's route of this line, as its prompt quotes it.
    Route(String),
    /// A judge's closed choice among these keys.
    Choice(Vec<String>),
    /// Anything else.
    Other,
}

impl Asked {
    /// What `body` asks.
    pub(crate) fn of(body: &Value) -> Self {
        let said: Vec<&str> = (body["messages"].as_array().into_iter().flatten())
            .filter_map(|message| message["content"].as_str())
            .collect();
        if let Some(prompt) = said.iter().find(|words| words.contains(ROUTE)) {
            let line = (prompt.split_once("The human's line: «"))
                .and_then(|(_, rest)| rest.split_once("».\nLabels"))
                .map_or_else(String::new, |(line, _)| line.to_owned());
            return Self::Route(line);
        }
        if let Some(choice) = said.iter().find_map(|words| words.strip_prefix("STATE:\n")) {
            // The route carries the choice's schema in its own JSON schema (`response_format`, as
            // the vLLM route sends it); a JSON mode that folds it into the turn, on its last line.
            let format = &body["response_format"];
            let carried = [&format["json_schema"]["schema"], &format["schema"]]
                .into_iter()
                .find(|schema| schema.is_object())
                .cloned();
            let schema = carried
                .or_else(|| {
                    (choice.lines().last()).and_then(|line| serde_json::from_str(line).ok())
                })
                .unwrap_or_default();
            let keys = (schema["properties"]["choice"]["enum"]
                .as_array()
                .into_iter()
                .flatten())
            .filter_map(|key| key.as_str().map(str::to_owned))
            .collect();
            return Self::Choice(keys);
        }
        Self::Other
    }
}

/// A loopback model on the OpenAI-compatible wire (the vLLM route's base URL override): it keeps
/// every request body it receives and answers each with [`SAID`], or as its script reads it
/// ([`Model::scripted`]). No key, no network beyond 127.0.0.1; dropping it stops and joins its
/// thread.
pub(crate) struct Model {
    port: u16,
    bodies: Arc<Mutex<Vec<Value>>>,
    stop: Arc<AtomicBool>,
    server: Option<JoinHandle<()>>,
}

impl Model {
    /// A loopback model that answers every request with [`SAID`].
    pub(crate) fn start() -> Self {
        Self::serve(None)
    }

    /// A scripted loopback model: each routing request receives the next of `labels` (`UNKNOWN`
    /// once they run out), a judge's closed choice offering `faithful` its whole-request
    /// approval, any other request [`SAID`].
    pub(crate) fn scripted(labels: &[&str]) -> Self {
        Self::serve(Some(
            labels.iter().map(|label| (*label).to_owned()).collect(),
        ))
    }

    // The synchronous fixture owns this blocking socket thread and joins it on drop.
    #[allow(clippy::disallowed_methods)]
    fn serve(mut script: Option<VecDeque<String>>) -> Self {
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
                    let said = (script.as_mut())
                        .map_or_else(|| SAID.to_owned(), |labels| reply(&body, labels));
                    seen.lock().expect("model log").push(body);
                    answer(&mut stream, &said);
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

    /// What each request received so far asked, in order ([`Asked::of`]).
    pub(crate) fn asked(&self) -> Vec<Asked> {
        self.bodies().iter().map(Asked::of).collect()
    }
}

/// The scripted answer to `body`: the next label for a route, the approval for a closed choice
/// that offers `faithful`, else [`SAID`].
fn reply(body: &Value, labels: &mut VecDeque<String>) -> String {
    match Asked::of(body) {
        Asked::Route(_) => labels.pop_front().unwrap_or_else(|| "UNKNOWN".to_owned()),
        Asked::Choice(keys) if keys.iter().any(|key| key == "faithful") => APPROVED.to_owned(),
        Asked::Choice(_) | Asked::Other => SAID.to_owned(),
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

/// One chat completion carrying `said`.
fn answer(stream: &mut TcpStream, said: &str) {
    let body = json!({
        "id": "chatcmpl-machine", "object": "chat.completion",
        "choices": [{"index": 0, "message": {"role": "assistant", "content": said}, "finish_reason": "stop"}],
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
