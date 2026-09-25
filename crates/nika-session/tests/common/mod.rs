// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Shared fixtures for the session's knowledge route: a synthetic Foundry snapshot in the
//! exporter's own layout (a knowledge root `foundry/` beside `.local/foundry/snapshots/<v>/`,
//! a manifest pinning the sha256 of every file under the root), and a loopback seat that speaks
//! the OpenAI-compatible wire, answers from a script and keeps every request body it received.
#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]

use std::io::{Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use nika_event::source_id::sha256_hex;
use serde_json::{Value, json};

/// The request every scenario authors: work the deterministic reader reads but cannot settle.
pub(crate) const INTENT: &str = "Read ./a.md and do something clever with it, then write ./b.md";

/// The change said at the consent prompt of the revision scenario.
pub(crate) const CHANGE: &str = "also keep a copy of the result in ./c.md";

/// The model the human chose for this session (a local engine's wire name).
pub(crate) const SEAT_MODEL: &str = "vllm/s03-seat";

/// The snapshot's version.
pub(crate) const VERSION: &str = "knowledge-s03";

/// A Foundry snapshot on disk: the knowledge root and the snapshot directory.
pub(crate) struct Foundry {
    pub(crate) root: PathBuf,
    pub(crate) snapshot: PathBuf,
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// The row files the exporter copies into a snapshot.
const ROW_FILES: [&str; 8] = [
    "families.jsonl",
    "pattern_packs.jsonl",
    "patterns.jsonl",
    "blocks.jsonl",
    "examples.jsonl",
    "skills.jsonl",
    "repair_principles.jsonl",
    "relations.jsonl",
];

/// The files under the knowledge root the rows name.
const ROOT_FILES: [&str; 3] = [
    "blocks/s03-transform.nika",
    "examples/s03-rewrite/workflow.nika",
    "skills/s03-rewrite/SKILL.md",
];

impl Foundry {
    /// A synthetic snapshot under `base`, its manifest pinning every file.
    pub(crate) fn create(base: &Path) -> Self {
        let root = base.join("foundry");
        let snapshot = base.join(".local/foundry/snapshots").join(VERSION);
        let rows = [
            (
                "families.jsonl",
                vec![
                    json!({"id": "family:s03-text-rewrite", "kind": "family", "title": "Text rewrite", "need": "Read a text file, do something clever with it, write the result to a file"}),
                ],
            ),
            (
                "pattern_packs.jsonl",
                vec![
                    json!({"id": "pack:s03-rewrite", "kind": "pattern_pack", "members": ["pattern:s03-transform-text"]}),
                ],
            ),
            (
                "patterns.jsonl",
                vec![
                    json!({"id": "pattern:s03-transform-text", "kind": "pattern", "title": "Transform text", "purpose": "S03-PATTERN-MARKER one infer transforms the text read, then the result is written", "notes": "state max_tokens"}),
                ],
            ),
            (
                "blocks.jsonl",
                vec![
                    json!({"id": "block:s03-transform", "kind": "block", "title": "Read, transform, write", "purpose": "read a file, one infer, write the result", "file": "blocks/s03-transform.nika"}),
                ],
            ),
            (
                "examples.jsonl",
                vec![
                    json!({"id": "example:s03-rewrite", "kind": "example", "corpus": "dev", "intent": "Read ./notes.md and do something clever with it, then write ./out.md", "file": "examples/s03-rewrite/workflow.nika"}),
                ],
            ),
            (
                "skills.jsonl",
                vec![
                    json!({"id": "skill:s03-rewrite", "kind": "skill", "family": "family:s03-text-rewrite", "file": "skills/s03-rewrite/SKILL.md"}),
                ],
            ),
            (
                "repair_principles.jsonl",
                vec![
                    json!({"id": "repair:S03_PATH", "kind": "repair_principle", "title": "stated paths only", "strategy": "S03-REPAIR-MARKER read and write exactly the paths the request states"}),
                ],
            ),
            (
                "relations.jsonl",
                vec![
                    json!({"from": "family:s03-text-rewrite", "rel": "RECOMMENDS", "to": "pack:s03-rewrite"}),
                    json!({"from": "pack:s03-rewrite", "rel": "CONTAINS", "to": "pattern:s03-transform-text"}),
                    json!({"from": "block:s03-transform", "rel": "REALIZES", "to": "pattern:s03-transform-text"}),
                    json!({"from": "diagnostic:NIKA-PARSE-022", "rel": "SUGGESTS_REPAIR", "to": "repair:S03_PATH"}),
                ],
            ),
        ];
        for (name, lines) in rows {
            let text = lines.iter().fold(String::new(), |mut text, line| {
                use std::fmt::Write as _;
                let _ = writeln!(text, "{line}");
                text
            });
            write(&snapshot.join(name), &text);
        }
        write(
            &root.join("blocks/s03-transform.nika"),
            "# S03-BLOCK-MARKER\nnika: read-transform-write\ntasks: {}\n",
        );
        write(
            &root.join("examples/s03-rewrite/workflow.nika"),
            "# S03-EXAMPLE-MARKER\nnika: s03-rewrite\ntasks: {}\n",
        );
        write(
            &root.join("skills/s03-rewrite/SKILL.md"),
            "# S03-SKILL-MARKER\nWhen a text is read, transformed by one infer and written.\n",
        );
        let foundry = Self { root, snapshot };
        foundry.pin(VERSION, "digest-s03-a");
        foundry
    }

    /// Write the manifest the exporter writes: every row file pinned under `knowledge/`, every
    /// file of the root pinned under its own path, as they are on disk now.
    pub(crate) fn pin(&self, version: &str, digest: &str) {
        let mut files = serde_json::Map::new();
        for name in ROW_FILES {
            let bytes = std::fs::read(self.snapshot.join(name)).unwrap();
            files.insert(format!("knowledge/{name}"), json!(sha256_hex(&bytes)));
        }
        for relative in ROOT_FILES {
            let bytes = std::fs::read(self.root.join(relative)).unwrap();
            files.insert(relative.to_owned(), json!(sha256_hex(&bytes)));
        }
        write(
            &self.snapshot.join("manifest.json"),
            &json!({
                "knowledge_version": version,
                "digest": digest,
                "source_commit": "synthetic",
                "files": files,
            })
            .to_string(),
        );
    }

    /// Edit the block in the knowledge root after the export (the manifest still pins the old
    /// bytes): the snapshot is stale.
    pub(crate) fn edit_block_after_export(&self) {
        write(
            &self.root.join("blocks/s03-transform.nika"),
            "# S03-BLOCK-MARKER edited after the export\nnika: read-transform-write\ntasks: {}\n",
        );
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
