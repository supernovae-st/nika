// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A loopback author on the OpenAI-compatible wire, scripted per scenario. It recognizes what
//! each request is — a turn of the author agent (a request that offers tools), the verifier's
//! closed choice, a one-shot document round, the bounded reading of a reply, or a plain reply —
//! answers each from its own script, and keeps every request body it received with the kind it
//! recognized. A double: it scripts what an intelligence answers, never what Nika decides.
//! A held step keeps the author's request under way until the child's cue, so the child can
//! act from a host's thread while it is.

use std::io::{Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

/// One step of the scripted author agent.
#[derive(Clone, Debug)]
pub(crate) enum Step {
    /// One tool call, with the text the assistant says beside it, if any.
    Call {
        name: String,
        args: Value,
        text: Option<String>,
    },
    /// A reply that ends the agent's turn.
    Say(String),
    /// A reply with reasoning and no content: no word and no call.
    Think(String),
    /// Hold the request under way: leave the marker under the markers directory, wait (30 s
    /// at most) for the child's cue `<marker>.go`, then answer it with the next step.
    Hold(String),
}

/// A tool call.
pub(crate) fn call(name: &str, args: Value) -> Step {
    Step::Call {
        name: name.to_owned(),
        args,
        text: None,
    }
}

/// A tool call with words beside it (an explanation before asking again).
pub(crate) fn call_saying(text: &str, name: &str, args: Value) -> Step {
    Step::Call {
        name: name.to_owned(),
        args,
        text: Some(text.to_owned()),
    }
}

/// A reply that ends the turn.
pub(crate) fn say(text: &str) -> Step {
    Step::Say(text.to_owned())
}

/// A reply that only reasons.
pub(crate) fn think(thought: &str) -> Step {
    Step::Think(thought.to_owned())
}

/// Hold the next request until the child's cue.
pub(crate) fn hold(marker: &str) -> Step {
    Step::Hold(marker.to_owned())
}

/// Leave `marker` under `markers`, then wait (bounded) for the cue `<marker>.go`.
fn held(markers: &Path, marker: &str) {
    std::fs::write(markers.join(marker), b"").unwrap();
    let cue = markers.join(format!("{marker}.go"));
    let deadline = Instant::now() + Duration::from_secs(30);
    while !cue.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// What the peer answers, per kind of request: the agent's steps in order, and the plain replies.
#[derive(Clone, Debug, Default)]
pub(crate) struct Script {
    pub(crate) agent: Vec<Step>,
    pub(crate) replies: Vec<String>,
    /// What the Session's reading of an answer picks, one per reading: an offered key,
    /// `DELEGATE` or `NONE` (the default when the queue is empty).
    pub(crate) picks: Vec<String>,
    /// What the verifier's judge answers the whole-request question, one per verdict asked
    /// (`faithful` when the queue is empty); a doubt that follows is never located.
    pub(crate) verdicts: Vec<String>,
}

/// The kinds of request the peer recognizes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A turn of the author agent: the request offers tools.
    Agent,
    /// The verifier's closed choice (approved, unless the script rejects the whole request).
    Judge,
    /// A one-shot document round (the compile door's own call).
    Document,
    /// The bounded reading of a reply (« copy the value or say NONE »).
    Reading,
    /// The Session's reading of which offer an answer picks (« one word only »).
    Pick,
    /// Anything else: a plain reply.
    Reply,
}

/// One request the peer received.
#[derive(Clone, Debug)]
pub(crate) struct Seen {
    pub(crate) kind: Kind,
    pub(crate) body: Value,
}

impl Seen {
    /// Every text the request carries (its messages' contents, tool results included).
    pub(crate) fn text(&self) -> String {
        (self.body["messages"].as_array().into_iter().flatten())
            .map(|message| match &message["content"] {
                Value::String(text) => text.clone(),
                Value::Array(parts) => (parts.iter())
                    .filter_map(|part| part["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n"),
                _ => String::new(),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The text of the request's last message: the person's line, or the result of the tool
    /// the author called last.
    pub(crate) fn last(&self) -> String {
        let last = (self.body["messages"].as_array().into_iter().flatten()).next_back();
        match last.map(|message| &message["content"]) {
            Some(Value::String(text)) => text.clone(),
            Some(Value::Array(parts)) => (parts.iter())
                .filter_map(|part| part["text"].as_str())
                .collect::<Vec<_>>()
                .join("\n"),
            _ => String::new(),
        }
    }

    /// The names of the tools the author called in the request's last assistant message.
    pub(crate) fn called(&self) -> Vec<String> {
        let messages = self.body["messages"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let last = (messages.iter()).rev().find(|m| m["role"] == "assistant");
        (last
            .and_then(|m| m["tool_calls"].as_array())
            .into_iter()
            .flatten())
        .filter_map(|call| call["function"]["name"].as_str().map(str::to_owned))
        .collect()
    }

    /// The names of the tools the request offers.
    pub(crate) fn tools(&self) -> Vec<String> {
        (self.body["tools"].as_array().into_iter().flatten())
            .filter_map(|tool| tool["function"]["name"].as_str().map(str::to_owned))
            .collect()
    }
}

/// The options a verifier question offers when it approves.
const APPROVALS: [&str; 5] = [
    "faithful",
    "carried",
    "consistent",
    "only_requested",
    "no_task",
];

/// The option a verifier's doubt question offers when its judge locates nothing.
const UNLOCATED: &str = "unlocated";

/// The kind of a request, read from its body.
fn kind_of(body: &Value) -> Kind {
    if body["tools"]
        .as_array()
        .is_some_and(|tools| !tools.is_empty())
    {
        return Kind::Agent;
    }
    let properties = &body["response_format"]["json_schema"]["schema"]["properties"];
    let choices = properties["choice"]["enum"].as_array();
    let judged = |k: &Value| k == UNLOCATED || APPROVALS.iter().any(|a| k == a);
    if choices.is_some_and(|keys| keys.iter().any(judged)) {
        return Kind::Judge;
    }
    if properties.get("candidate").is_some() {
        return Kind::Document;
    }
    let last = (body["messages"].as_array().into_iter().flatten())
        .next_back()
        .and_then(|message| message["content"].as_str())
        .unwrap_or_default();
    if last.contains("asked a human for one value") || last.contains("asked a human to choose") {
        return Kind::Reading;
    }
    if last.contains("One word only, nothing else.") {
        return Kind::Pick;
    }
    Kind::Reply
}

/// The scripts the peer answers from, consumed in order per kind.
struct Queues {
    agent: Vec<Step>,
    replies: Vec<String>,
    picks: Vec<String>,
    verdicts: Vec<String>,
    calls: usize,
    markers: PathBuf,
}

impl Queues {
    fn next_step(&mut self) -> Step {
        if self.agent.is_empty() {
            say("(the scripted author has nothing more to say)")
        } else {
            self.agent.remove(0)
        }
    }

    fn answer(&mut self, kind: Kind, body: &Value) -> String {
        match kind {
            Kind::Agent => match self.next_step() {
                Step::Hold(marker) => {
                    held(&self.markers, &marker);
                    self.answer(kind, body)
                }
                Step::Call { name, args, text } => {
                    self.calls += 1;
                    completion(
                        &json!({"role": "assistant", "content": text, "tool_calls": [{
                            "id": format!("call_{}", self.calls), "type": "function",
                            "function": {"name": name, "arguments": args.to_string()}}]}),
                        "tool_calls",
                    )
                }
                Step::Say(text) => {
                    self.calls += 1;
                    text_completion(&text)
                }
                Step::Think(thought) => {
                    self.calls += 1;
                    let message =
                        json!({"role": "assistant", "content": null, "reasoning_content": thought});
                    completion(&message, "stop")
                }
            },
            Kind::Judge => {
                let keys = &body["response_format"]["json_schema"]["schema"]["properties"]["choice"]
                    ["enum"];
                let offers =
                    |key: &str| keys.as_array().is_some_and(|k| k.iter().any(|k| k == key));
                // A scripted verdict answers the first question that offers it.
                let scripted = (self.verdicts.first()).is_some_and(|verdict| offers(verdict));
                let choice = if scripted {
                    self.verdicts.remove(0)
                } else {
                    let approve = APPROVALS.into_iter().find(|a| offers(a));
                    let unlocated = offers(UNLOCATED).then_some(UNLOCATED);
                    approve.or(unlocated).unwrap_or("none").to_owned()
                };
                text_completion(&json!({"choice": choice}).to_string())
            }
            // The one-shot door is not how these scenarios author: it gets no document.
            Kind::Document => text_completion(
                &json!({"candidate": "", "candidate_lines": [], "operations": [],
                    "questions": [], "gaps": [], "notes": "no document"})
                .to_string(),
            ),
            Kind::Reading => text_completion("NONE"),
            Kind::Pick => {
                let pick = if self.picks.is_empty() {
                    "NONE".to_owned()
                } else {
                    self.picks.remove(0)
                };
                text_completion(&pick)
            }
            Kind::Reply => {
                let text = if self.replies.is_empty() {
                    "D'accord.".to_owned()
                } else {
                    self.replies.remove(0)
                };
                text_completion(&text)
            }
        }
    }
}

fn completion(message: &Value, finish: &str) -> String {
    json!({"id": "chatcmpl-open-ux", "object": "chat.completion", "created": 0,
        "model": "oux-author",
        "choices": [{"index": 0, "message": message, "finish_reason": finish}],
        "usage": {"prompt_tokens": 1000, "completion_tokens": 200, "total_tokens": 1200}})
    .to_string()
}

fn text_completion(text: &str) -> String {
    completion(&json!({"role": "assistant", "content": text}), "stop")
}

/// The loopback peer: every request it received, in order.
pub(crate) struct Peer {
    port: u16,
    seen: Arc<Mutex<Vec<Seen>>>,
    stop: Arc<Mutex<bool>>,
}

impl Peer {
    /// The peer answering `script`, its held steps marked under `markers`.
    pub(crate) fn start(script: Script, markers: &Path) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(Mutex::new(false));
        let (kept, halt) = (Arc::clone(&seen), Arc::clone(&stop));
        let markers = markers.to_path_buf();
        std::thread::spawn(move || {
            let mut queues = Queues {
                agent: script.agent,
                replies: script.replies,
                picks: script.picks,
                verdicts: script.verdicts,
                calls: 0,
                markers,
            };
            for stream in listener.incoming() {
                if *halt.lock().unwrap() {
                    break;
                }
                let Ok(mut stream) = stream else { continue };
                let Some(body) = read_request(&mut stream) else {
                    continue;
                };
                let kind = kind_of(&body);
                let answer = queues.answer(kind, &body);
                kept.lock().unwrap().push(Seen { kind, body });
                respond(&mut stream, &answer);
            }
        });
        Self { port, seen, stop }
    }

    /// The base URL a local engine's override takes (`NIKA_VLLM_BASE_URL`).
    pub(crate) fn base(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }

    /// The requests received so far.
    pub(crate) fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }

    /// The author agent's requests received so far.
    pub(crate) fn agent(&self) -> Vec<Seen> {
        (self.seen().into_iter())
            .filter(|seen| seen.kind == Kind::Agent)
            .collect()
    }

    pub(crate) fn shutdown(&self) {
        *self.stop.lock().unwrap() = true;
        let _ = TcpStream::connect(("127.0.0.1", self.port));
    }
}

impl Drop for Peer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn read_request(stream: &mut TcpStream) -> Option<Value> {
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(30)))
        .ok()?;
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 8192];
    let header_end = loop {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            return None;
        }
        buffer.extend_from_slice(&chunk[..n]);
        if let Some(at) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            break at + 4;
        }
    };
    let head = String::from_utf8_lossy(&buffer[..header_end]).to_lowercase();
    let length: usize = head
        .lines()
        .find_map(|line| line.strip_prefix("content-length:"))
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0);
    while buffer.len() < header_end + length {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..n]);
    }
    serde_json::from_slice(&buffer[header_end..header_end + length]).ok()
}

fn respond(stream: &mut TcpStream, body: &str) {
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body.as_bytes());
    let _ = stream.flush();
}
