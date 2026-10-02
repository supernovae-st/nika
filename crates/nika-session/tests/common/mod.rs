// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Shared fixtures for the session's knowledge route: a synthetic Foundry knowledge release the
//! strict door admits (built with the knowledge door's own test support, every row invented),
//! and a loopback seat that speaks the OpenAI-compatible wire, answers from a script and keeps
//! every request body it received.
#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]

use std::cell::RefCell;
use std::io::{Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use nika_event::source_id::sha256_hex;
use nika_onboard::knowledge::TrustedIdentity;
use nika_onboard::knowledge::fixture::{self, Payload};
use serde_json::{Value, json};

/// The request every scenario authors: work the deterministic reader reads but cannot settle.
pub(crate) const INTENT: &str = "Read ./a.md and do something clever with it, then write ./b.md";

/// The change said at the consent prompt of the revision scenario.
pub(crate) const CHANGE: &str = "also keep a copy of the result in ./c.md";

/// The model the human chose for this session (a local engine's wire name).
pub(crate) const SEAT_MODEL: &str = "vllm/s03-seat";

/// The release's version.
pub(crate) const VERSION: &str = "knowledge-s03";

/// The block the release ships: its first line is a marker a test finds in a seat's instruction.
const BLOCK_FILE: &str = "blocks/s03-transform.nika";
const BLOCK_TEXT: &str = "# S03-BLOCK-MARKER\nnika: read-transform-write\ntasks: {}\n";

/// A Foundry knowledge release on disk, at a root the session is told to read (`snapshot`, the
/// name every door's flag still takes; `root`, the same directory), and the identity the test,
/// as the host that sealed it, trusts for its latest seal.
pub(crate) struct Foundry {
    pub(crate) root: PathBuf,
    pub(crate) snapshot: PathBuf,
    sealed: RefCell<Option<TrustedIdentity>>,
}

/// The release: a policy-R payload the strict door admits — a family, its pack, the pattern a
/// checked block realizes, a repair principle a diagnostic suggests — every row synthetic.
fn release() -> Payload {
    let mut payload = Payload::minimal();
    for kind in [
        "family",
        "pattern_pack",
        "pattern",
        "block",
        "repair_principle",
    ] {
        payload.kind(kind).clear();
    }
    payload.files.remove(fixture::BLOCK_FILE);
    payload
        .files
        .insert(BLOCK_FILE.to_owned(), BLOCK_TEXT.as_bytes().to_vec());
    let row = |kind: &str, id: &str, title: &str, proof: &str| {
        fixture::row(kind, id, title, "EXPERIMENTAL", proof)
    };
    let mut family = row("family", "family:s03-text-rewrite", "Text rewrite", "NONE");
    family["need"] =
        json!("Read a text file, do something clever with it, write the result to a file");
    family["facets"] = json!({});
    let pack = row("pattern_pack", "pack:s03-rewrite", "Rewrite pack", "NONE");
    let mut pattern = row(
        "pattern",
        "pattern:s03-transform-text",
        "Transform text",
        "NONE",
    );
    pattern["purpose"] =
        json!("S03-PATTERN-MARKER one infer transforms the text read, then the result is written");
    pattern["notes"] = json!("state max_tokens");
    let file_sha = sha256_hex(BLOCK_TEXT.as_bytes());
    let mut block = row(
        "block",
        "block:s03-transform",
        "Read, transform, write",
        "CHECKED",
    );
    block["purpose"] = json!("read a file, one infer, write the result");
    block["file"] = json!(BLOCK_FILE);
    block["file_sha256"] = json!(file_sha);
    for list in ["holes", "authority", "interfaces", "known_failure_modes"] {
        block[list] = json!([]);
    }
    block["effects"] = json!(["fs.read", "fs.write"]);
    block["callables"] = json!(["nika:read", "nika:write"]);
    block["check_receipt"] = json!({
        "verifier_sha256": fixture::VERIFIER, "spec_sha": fixture::SPEC, "sha256": file_sha,
        "verdict": "CURRENT_CHECKED",
    });
    let mut repair = row(
        "repair_principle",
        "repair:S03_PATH",
        "stated paths only",
        "NONE",
    );
    repair["strategy"] =
        json!("S03-REPAIR-MARKER read and write exactly the paths the request states");
    for (kind, value) in [
        ("family", family),
        ("pattern_pack", pack),
        ("pattern", pattern),
        ("block", block),
        ("repair_principle", repair),
    ] {
        payload.kind(kind).push(value);
    }
    payload.relations = vec![
        fixture::edge("family:s03-text-rewrite", "RECOMMENDS", "pack:s03-rewrite"),
        fixture::edge("pack:s03-rewrite", "CONTAINS", "pattern:s03-transform-text"),
        fixture::edge(
            "block:s03-transform",
            "REALIZES",
            "pattern:s03-transform-text",
        ),
        fixture::edge(
            "diagnostic:NIKA-PARSE-022",
            "SUGGESTS_REPAIR",
            "repair:S03_PATH",
        ),
    ];
    payload
}

impl Foundry {
    /// A synthetic release under `base`, sealed at [`VERSION`].
    pub(crate) fn create(base: &Path) -> Self {
        let root = base.join("release");
        let foundry = Self {
            root: root.clone(),
            snapshot: root,
            sealed: RefCell::new(None),
        };
        foundry.reseal(VERSION, "fixture-producer");
        foundry
    }

    /// The identity of the latest seal, from the bytes this host wrote (never read back).
    pub(crate) fn identity(&self) -> TrustedIdentity {
        self.sealed.borrow().clone().expect("sealed")
    }

    /// Write the release again, sealed at `version` by the producer `tool`: the same rows under
    /// another version, or the same rows and version under another manifest.
    pub(crate) fn reseal(&self, version: &str, tool: &str) {
        let mut files = release().render();
        let mut manifest = Payload::manifest(&files);
        manifest["knowledge_version"] = json!(version);
        manifest["tool"]["id"] = json!(tool);
        files.insert(
            fixture::manifest_path().to_owned(),
            fixture::manifest_bytes(&manifest),
        );
        *self.sealed.borrow_mut() = fixture::identity_of(&files);
        if self.root.exists() {
            std::fs::remove_dir_all(&self.root).unwrap();
        }
        fixture::write_files(&self.root, &files).unwrap();
    }

    /// Edit the block in the release after it was sealed (its manifest still pins the old bytes):
    /// the strict door refuses it.
    pub(crate) fn edit_block_after_export(&self) {
        std::fs::write(
            self.root.join(BLOCK_FILE),
            "# S03-BLOCK-MARKER edited after the export\nnika: read-transform-write\ntasks: {}\n",
        )
        .unwrap();
    }
}

/// A candidate that realizes [`INTENT`]: read the stated source, one infer, write the stated
/// destination; `model` is the placeholder the compiler asks for, or the seat's own model.
pub(crate) fn candidate(model: &str, copy: bool) -> String {
    let copy_task = if copy {
        "\n  write_copy:\n    with: { content: \"${{ tasks.transform.output }}\" }\n    invoke:\n      tool: \"nika:write\"\n      args: { path: \"./c.md\", content: \"${{ with.content }}\" }"
    } else {
        ""
    };
    let writes = if copy {
        "[\"./b.md\", \"./c.md\"]"
    } else {
        "[\"./b.md\"]"
    };
    format!(
        "nika: clever-rewrite\nmodel: {model}\npermits:\n  tools: [\"nika:read\", \"nika:write\"]\n  fs:\n    read: [\"./a.md\"]\n    write: {writes}\ntasks:\n  read_source:\n    invoke:\n      tool: \"nika:read\"\n      args: {{ path: \"./a.md\" }}\n  transform:\n    with: {{ text: \"${{{{ tasks.read_source.output }}}}\" }}\n    infer:\n      max_tokens: 600\n      prompt: \"Rewrite this text in a clever way, inventing nothing: ${{{{ with.text }}}}\"\n  write_result:\n    with: {{ content: \"${{{{ tasks.transform.output }}}}\" }}\n    invoke:\n      tool: \"nika:write\"\n      args: {{ path: \"./b.md\", content: \"${{{{ with.content }}}}\" }}{copy_task}\n"
    )
}

/// The native answer the seat returns for a candidate.
pub(crate) fn native_answer(candidate: &str) -> String {
    json!({"candidate": candidate, "questions": [], "gaps": [], "notes": "read, transform, write"})
        .to_string()
}

/// A loopback seat on the OpenAI-compatible wire: every request body it received, in order;
/// each answered with the next scripted text (the last repeats).
pub(crate) struct LoopbackSeat {
    pub(crate) port: u16,
    pub(crate) requests: Arc<Mutex<Vec<Value>>>,
    stop: Arc<Mutex<bool>>,
}

impl LoopbackSeat {
    pub(crate) fn start(script: Vec<String>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(Mutex::new(false));
        let (seen, halt) = (Arc::clone(&requests), Arc::clone(&stop));
        std::thread::spawn(move || {
            let mut next = 0_usize;
            for stream in listener.incoming() {
                if *halt.lock().unwrap() {
                    break;
                }
                let Ok(mut stream) = stream else { continue };
                let Some(body) = read_request(&mut stream) else {
                    continue;
                };
                seen.lock().unwrap().push(body);
                let text = script
                    .get(next)
                    .or_else(|| script.last())
                    .cloned()
                    .unwrap_or_default();
                next += 1;
                respond(&mut stream, &text);
            }
        });
        Self {
            port,
            requests,
            stop,
        }
    }

    /// The base URL a local engine's override takes (`NIKA_VLLM_BASE_URL`).
    pub(crate) fn base(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }

    /// The bodies received so far.
    pub(crate) fn bodies(&self) -> Vec<Value> {
        self.requests.lock().unwrap().clone()
    }

    pub(crate) fn shutdown(&self) {
        *self.stop.lock().unwrap() = true;
        let _ = TcpStream::connect(("127.0.0.1", self.port));
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

fn respond(stream: &mut TcpStream, text: &str) {
    let body = json!({
        "id": "chatcmpl-s03",
        "object": "chat.completion",
        "choices": [{"index": 0, "message": {"role": "assistant", "content": text}, "finish_reason": "stop"}],
        "usage": {"prompt_tokens": 1000, "completion_tokens": 200, "total_tokens": 1200},
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

/// The text of a message of a captured body, by role (the first one).
pub(crate) fn message(body: &Value, role: &str) -> String {
    body["messages"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|m| m["role"] == role)
        .and_then(|m| m["content"].as_str())
        .unwrap_or_default()
        .to_owned()
}

/// sha256 of a text, lowercase hex.
pub(crate) fn sha256(text: &str) -> String {
    sha256_hex(text.as_bytes())
}
