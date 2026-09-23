// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Native authoring on the compile door over real loopback (S06): the operator's seat through
//! the typed builder, a controlled seat on the OpenAI-compatible wire that keeps every body it
//! received (`calls()` is the P of the S09 matrix), a Foundry snapshot in the exporter's own
//! layout, and the answer rounds the server keeps. The core document is compared to what the
//! seat really received; nothing here reads the environment.

use std::io::{Read as _, Write as _};
use std::sync::mpsc;

use nika_cli_host::compile::knowledge::Snapshot;
use nika_event::source_id::sha256_hex;
use nika_onboard::compile::{AuthoringKnowledge, CompileRequest, revise_intent};
use nika_providers::ProvidersConfig;

use super::*;
use crate::NativeAuthoring;

mod lifecycle;
mod openapi;
mod refusals;
mod withheld;

/// Work the deterministic reader reads but cannot settle.
pub(super) const INTENT: &str = "Read ./a.md and do something clever with it, then write ./b.md";
/// A revision in words of the candidate [`INTENT`] produced.
const CHANGE: &str = "also keep a copy of the result in ./c.md";
/// The operator's seat: a local engine's wire name, reached at the loopback seat.
pub(super) const SEAT: &str = "vllm/s06-seat";
/// The model the candidate's own infer runs with — candidate data, never the seat.
pub(super) const RUN_MODEL: &str = "mistral/mistral-small-latest";
const VERSION: &str = "knowledge-s06";

/// What the controlled seat answers one request with.
#[derive(Clone)]
pub(super) enum Reply {
    /// A chat completion carrying this text.
    Text(String),
    /// An HTTP status with this raw body.
    Status(u16, String),
    /// Signal arrival, wait for the release, then answer this text.
    Parked(String),
    /// 503 with `Retry-After: 0`: the transport resends the same request at once.
    Busy,
}

/// A seat on the OpenAI-compatible wire: every request body it received, in order; each
/// answered from the script (the last reply repeats).
pub(super) struct Seat {
    port: u16,
    bodies: Arc<Mutex<Vec<Value>>>,
    /// The `Authorization` header of every request, as sent (empty when absent).
    authorizations: Arc<Mutex<Vec<String>>>,
    pub(super) entered: mpsc::Receiver<()>,
    pub(super) release: mpsc::Sender<()>,
}

impl Seat {
    pub(super) fn start(script: Vec<Reply>) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("seat listener");
        let port = listener.local_addr().expect("seat address").port();
        let bodies = Arc::new(Mutex::new(Vec::new()));
        let authorizations = Arc::new(Mutex::new(Vec::new()));
        let (arrived, entered) = mpsc::channel();
        let (release, released) = mpsc::channel::<()>();
        let seen = Arc::clone(&bodies);
        let presented = Arc::clone(&authorizations);
        // The loopback seat's accept loop: a test harness thread, never production.
        #[allow(clippy::disallowed_methods)]
        std::thread::spawn(move || {
            let mut next = 0_usize;
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let Some((body, authorization)) = read_request(&mut stream) else {
                    continue;
                };
                seen.lock().expect("bodies").push(body);
                presented.lock().expect("headers").push(authorization);
                let reply = script
                    .get(next)
                    .or_else(|| script.last())
                    .cloned()
                    .unwrap_or_else(|| Reply::Text(String::new()));
                next += 1;
                match reply {
                    Reply::Text(text) => respond(&mut stream, 200, &completion(&text)),
                    Reply::Status(status, body) => respond(&mut stream, status, &body),
                    Reply::Busy => busy(&mut stream),
                    Reply::Parked(text) => {
                        let _sent = arrived.send(());
                        let _released = released.recv();
                        respond(&mut stream, 200, &completion(&text));
                    }
                }
            }
        });
        Self {
            port,
            bodies,
            authorizations,
            entered,
            release,
        }
    }

    /// The operator's provider configuration: the seat's endpoint, no key, no environment.
    pub(super) fn providers(&self) -> ProvidersConfig {
        ProvidersConfig::new().with_base_url(
            "vllm",
            format!("http://127.0.0.1:{}/v1/chat/completions", self.port),
        )
    }

    pub(super) fn bodies(&self) -> Vec<Value> {
        self.bodies.lock().expect("bodies").clone()
    }

    pub(super) fn calls(&self) -> usize {
        self.bodies.lock().expect("bodies").len()
    }

    pub(super) fn authorizations(&self) -> Vec<String> {
        self.authorizations.lock().expect("headers").clone()
    }
}

fn read_request(stream: &mut std::net::TcpStream) -> Option<(Value, String)> {
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
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
    let raw_head = String::from_utf8_lossy(&buffer[..header_end]).into_owned();
    let authorization = raw_head
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("authorization")
                .then(|| value.trim().to_owned())
        })
        .unwrap_or_default();
    let head = raw_head.to_lowercase();
    let length: usize = head
        .lines()
        .find_map(|line| line.strip_prefix("content-length:"))
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0);
    while buffer.len() < header_end + length {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..n]);
    }
    let body = serde_json::from_slice(buffer.get(header_end..header_end + length)?).ok()?;
    Some((body, authorization))
}

fn completion(text: &str) -> String {
    json!({
        "id": "chatcmpl-s06",
        "object": "chat.completion",
        "choices": [{"index": 0, "message": {"role": "assistant", "content": text}, "finish_reason": "stop"}],
        "usage": {"prompt_tokens": 1000, "completion_tokens": 200, "total_tokens": 1200},
    })
    .to_string()
}

fn busy(stream: &mut std::net::TcpStream) {
    let _written = stream.write_all(
        b"HTTP/1.1 503 S06\r\nContent-Type: application/json\r\nRetry-After: 0\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
    );
    let _flushed = stream.flush();
}

fn respond(stream: &mut std::net::TcpStream, status: u16, body: &str) {
    let head = format!(
        "HTTP/1.1 {status} S06\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _head = stream.write_all(head.as_bytes());
    let _body = stream.write_all(body.as_bytes());
    let _flushed = stream.flush();
}

/// A candidate that realizes [`INTENT`]: read the stated source, one infer, write the stated
/// destination (and a copy, for the revision); `model` is the placeholder the compiler asks
/// the human for, or a model already named.
pub(super) fn candidate(model: &str, copy: bool) -> String {
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
pub(super) fn native_answer(candidate: &str) -> String {
    json!({"candidate": candidate, "questions": [], "gaps": [], "notes": "read, transform, write"})
        .to_string()
}

/// A Foundry snapshot on disk: its knowledge root and its snapshot directory.
pub(super) struct Foundry {
    pub(super) root: std::path::PathBuf,
    pub(super) snapshot: std::path::PathBuf,
}

const ROOT_FILES: [(&str, &str); 3] = [
    (
        "blocks/s06-transform.nika",
        "# S06-BLOCK-MARKER\nnika: read-transform-write\ntasks: {}\n",
    ),
    (
        "examples/s06-rewrite/workflow.nika",
        "# S06-EXAMPLE-MARKER\nnika: s06-rewrite\ntasks: {}\n",
    ),
    (
        "skills/s06-rewrite/SKILL.md",
        "# S06-SKILL-MARKER\nWhen a text is read, transformed by one infer and written.\n",
    ),
];

fn row_files() -> Vec<(&'static str, Vec<Value>)> {
    vec![
        (
            "families.jsonl",
            vec![
                json!({"id": "family:s06-text-rewrite", "kind": "family", "title": "Text rewrite", "need": "Read a text file, do something clever with it, write the result to a file"}),
            ],
        ),
        (
            "pattern_packs.jsonl",
            vec![
                json!({"id": "pack:s06-rewrite", "kind": "pattern_pack", "members": ["pattern:s06-transform-text"]}),
            ],
        ),
        (
            "patterns.jsonl",
            vec![
                json!({"id": "pattern:s06-transform-text", "kind": "pattern", "title": "Transform text", "purpose": "S06-PATTERN-MARKER one infer transforms the text read, then the result is written"}),
            ],
        ),
        (
            "blocks.jsonl",
            vec![
                json!({"id": "block:s06-transform", "kind": "block", "title": "Read, transform, write", "purpose": "read a file, one infer, write the result", "file": "blocks/s06-transform.nika"}),
            ],
        ),
        (
            "examples.jsonl",
            vec![
                json!({"id": "example:s06-rewrite", "kind": "example", "corpus": "dev", "intent": "Read ./notes.md and do something clever with it, then write ./out.md", "file": "examples/s06-rewrite/workflow.nika"}),
            ],
        ),
        (
            "skills.jsonl",
            vec![
                json!({"id": "skill:s06-rewrite", "kind": "skill", "family": "family:s06-text-rewrite", "file": "skills/s06-rewrite/SKILL.md"}),
            ],
        ),
        (
            "repair_principles.jsonl",
            vec![
                json!({"id": "repair:S06_PATH", "kind": "repair_principle", "title": "stated paths only", "strategy": "read and write exactly the paths the request states"}),
            ],
        ),
        (
            "relations.jsonl",
            vec![
                json!({"from": "family:s06-text-rewrite", "rel": "RECOMMENDS", "to": "pack:s06-rewrite"}),
                json!({"from": "pack:s06-rewrite", "rel": "CONTAINS", "to": "pattern:s06-transform-text"}),
                json!({"from": "block:s06-transform", "rel": "REALIZES", "to": "pattern:s06-transform-text"}),
            ],
        ),
    ]
}

fn write_file(path: &std::path::Path, text: &str) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("parent dir");
    std::fs::write(path, text).expect("fixture file");
}

impl Foundry {
    /// A synthetic snapshot under `base`, its manifest pinning every file (the exporter's
    /// layout: the root `foundry/` beside `.local/foundry/snapshots/<version>/`).
    pub(super) fn create(base: &std::path::Path) -> Self {
        let root = base.join("foundry");
        let snapshot = base.join(".local/foundry/snapshots").join(VERSION);
        let mut files = serde_json::Map::new();
        for (name, rows) in row_files() {
            let text = rows.iter().fold(String::new(), |mut text, row| {
                use std::fmt::Write as _;
                let _ = writeln!(text, "{row}");
                text
            });
            write_file(&snapshot.join(name), &text);
            files.insert(
                format!("knowledge/{name}"),
                json!(sha256_hex(text.as_bytes())),
            );
        }
        for (relative, text) in ROOT_FILES {
            write_file(&root.join(relative), text);
            files.insert(relative.to_owned(), json!(sha256_hex(text.as_bytes())));
        }
        let manifest = json!({
            "knowledge_version": VERSION,
            "digest": "digest-s06-a",
            "source_commit": "synthetic",
            "files": files,
        });
        write_file(&snapshot.join("manifest.json"), &manifest.to_string());
        Self { root, snapshot }
    }

    /// The pack the one knowledge door composes for `intent` from this snapshot.
    pub(super) fn pack(&self, intent: &str) -> AuthoringKnowledge {
        Snapshot::open(&self.snapshot)
            .expect("snapshot")
            .pack(intent, None)
            .expect("pack")
    }
}

/// A listener that seats `authoring`, over the same resident authority as every other test.
pub(super) async fn start_native(
    world: &TestWorld,
    limits: ServerLimits,
    authoring: NativeAuthoring,
) -> (TestServer, Arc<TestBackend>) {
    start_native_parked(world, limits, authoring, None).await
}

/// [`start_native`], with an action run inside the first compile's blocking section.
pub(super) async fn start_native_parked(
    world: &TestWorld,
    limits: ServerLimits,
    authoring: NativeAuthoring,
    park: Option<super::super::super::test_support::CaptureAction>,
) -> (TestServer, Arc<TestBackend>) {
    let (server, backend, _state) = start_native_observed(world, limits, authoring, park).await;
    (server, backend)
}

/// [`start_native_parked`], keeping the server's own state to observe its rounds afterwards.
pub(super) async fn start_native_observed(
    world: &TestWorld,
    limits: ServerLimits,
    authoring: NativeAuthoring,
    park: Option<super::super::super::test_support::CaptureAction>,
) -> (TestServer, Arc<TestBackend>, Arc<AppState>) {
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let resident = ResidentConfig::new(&world.state).with_limits(limits);
    let authority = ResidentAuthority::open(resident, backend.clone())
        .await
        .expect("authority");
    let config = ServerConfig::new(
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        &world.workflows,
        &world.token,
    )
    .with_native_authoring(authoring);
    let bound = BoundServer::attach(config, &authority).await.expect("bind");
    *bound.state.before_compile.lock().expect("compile probe") = park;
    let state = Arc::clone(&bound.state);
    let address = bound.local_addr().expect("local address");
    let shutdown_probe = authority.state.store.shutdown_test_probe();
    let shutdown_observer = Arc::clone(&shutdown_probe);
    let (shutdown, receiver) = oneshot::channel();
    let join = tokio::spawn(authority.serve_with_http(bound, async move {
        let _result = receiver.await;
        shutdown_observer.mark_shutdown_loop_observed();
    }));
    let server = TestServer {
        address,
        shutdown: Some(shutdown),
        join,
        shutdown_probe,
    };
    (server, backend, state)
}

/// A generation-2 body: `fields` over a fresh creation of [`INTENT`].
pub(super) fn fresh(fields: &Value) -> String {
    let mut body = json!({
        "compile_version": 2,
        "mode": "create",
        "cognition": "explicitProvider",
        "intent": INTENT,
    });
    merge(&mut body, fields);
    body.to_string()
}

/// A replay of a kept round of [`INTENT`] with `fields` over it.
pub(super) fn replay(token: &str, fields: &Value) -> String {
    let mut body = json!({
        "compile_version": 2,
        "mode": "create",
        "cognition": "deterministicOnly",
        "replay_token": token,
        "intent": INTENT,
    });
    merge(&mut body, fields);
    body.to_string()
}

fn merge(body: &mut Value, fields: &Value) {
    for (key, value) in fields.as_object().expect("fields object") {
        if value.is_null() {
            body.as_object_mut().expect("body object").remove(key);
        } else {
            body[key] = value.clone();
        }
    }
}

/// The token a fresh answer carries, checked for its spelling.
pub(super) fn token_of(response: &WireResponse) -> String {
    let token = response
        .header("nika-compile-replay")
        .expect("a kept round's token")
        .to_owned();
    assert_eq!(token.len(), 64, "{token}");
    assert!(
        token
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    token
}

/// The text of the first message of a role in a body the seat received.
fn message(body: &Value, role: &str) -> String {
    body["messages"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|m| m["role"] == role)
        .and_then(|m| m["content"].as_str())
        .unwrap_or_default()
        .to_owned()
}

/// The first round: the seat received the operator's model and bound, the pack composed for
/// the request byte for byte, and the receipt names that instruction and that pack — with
/// the snapshot's hashes and none of its host paths.
fn assert_first_round(foundry: &Foundry, world: &TestWorld, sent: &Value, document: &Value) {
    assert_eq!(
        sent["model"], "s06-seat",
        "the operator's model, never another"
    );
    assert_eq!(
        sent["max_tokens"].as_u64(),
        Some(4096),
        "the operator's bound: {sent}"
    );
    assert!(
        sent["response_format"].is_object(),
        "the native answer schema"
    );
    let system = message(sent, "system");
    let pack = foundry.pack(INTENT);
    assert!(!pack.references.is_empty());
    for reference in &pack.references {
        assert!(system.contains(&reference.text), "{} is sent", reference.id);
    }
    let opening: Value = serde_json::from_str(&message(sent, "user")).expect("opening");
    assert_eq!(opening["request"], INTENT);
    let provenance = &document["provenance"];
    assert_eq!(provenance["cognition"], "explicitProvider");
    assert_eq!(provenance["strategy"], "native");
    let receipt = &provenance["authoring"];
    assert_eq!(receipt["model"], SEAT);
    assert_eq!(receipt["calls"], 1);
    assert_eq!(receipt["input_tokens"], 1000);
    assert_eq!(receipt["output_tokens"], 200);
    assert_eq!(
        receipt["context"][0]["instruction_sha256"],
        sha256_hex(system.as_bytes()),
        "the receipt names the instruction the seat really read"
    );
    assert_eq!(
        receipt["backend"],
        json!({"kind": "direct_api", "provider": "vllm", "cost_basis": "measured_by_tokens_at_catalog_price"})
    );
    let identity = &provenance["decision"]["native"]["knowledge"]["identity"];
    let expected = &pack.identity;
    for key in [
        "version",
        "digest",
        "manifest_sha256",
        "rows_sha256",
        "pack_builder",
    ] {
        assert_eq!(identity[key], expected[key], "{key}");
    }
    assert_eq!(
        identity["door"]["pack_sha256"],
        expected["door"]["pack_sha256"]
    );
    assert!(identity.get("dir").is_none(), "{identity}");
    assert!(identity["verification"].get("files_root").is_none());
    let sent_ids: Vec<&Value> = provenance["decision"]["native"]["references"]
        .as_array()
        .expect("references")
        .iter()
        .map(|r| &r["id"])
        .collect();
    for reference in &pack.references {
        assert!(sent_ids.contains(&&json!(reference.id)), "{}", reference.id);
    }
    let text = document.to_string();
    let host = world.root.path().display().to_string();
    assert!(!text.contains(&host), "no host path reaches the caller");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_native_round_reads_the_pinned_pack_under_the_operators_seat_and_its_answer_round_calls_no_one()
 {
    let world = TestWorld::new();
    let foundry = Foundry::create(&world.root.path().join("knowledge"));
    let seat = Seat::start(vec![Reply::Text(native_answer(&candidate(
        "mock/echo",
        false,
    )))]);
    let authoring = NativeAuthoring::new(SEAT, seat.providers())
        .with_knowledge(&foundry.snapshot, None)
        .with_max_tokens(4096)
        .with_repairs(1);
    let (server, backend) = start_native(&world, compile_limits(), authoring).await;
    let health = server
        .request("GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await
        .json();
    let tokens = health["supportedCapabilities"].as_array().expect("tokens");
    assert!(tokens.contains(&json!("compile")) && tokens.contains(&json!("compileNativeV2")));
    assert!(
        !health.to_string().contains("s06-seat") && !health.to_string().contains(VERSION),
        "the seat is the operator's, never public: {health}"
    );
    let before = tree(world.root.path());

    let first = server.request(&compile_request(&fresh(&json!({})))).await;
    assert_eq!(first.status, 200, "{}", first.body);
    assert_eq!(first.header("cache-control"), Some("no-store"));
    let token = token_of(&first);
    let document = first.json();
    assert_eq!(document["compile_version"], 2, "a call happened");
    assert_eq!(document["status"], "incomplete", "{document:#}");
    assert!(
        document["questions"]
            .as_array()
            .expect("questions")
            .iter()
            .any(|q| q["key"] == "model"),
        "the candidate asks for its run model: {document:#}"
    );
    assert_eq!(seat.calls(), 1);
    assert_first_round(&foundry, &world, &seat.bodies()[0], &document);

    // The answer round: the kept plan, this round's answers, zero calls — the same plan the
    // paid round produced, now baked.
    let answers = json!({"answers": {"model": RUN_MODEL}});
    let second = server
        .request(&compile_request(&replay(&token, &answers)))
        .await;
    assert_eq!(second.status, 200, "{}", second.body);
    assert!(
        second.header("nika-compile-replay").is_none(),
        "no new token"
    );
    assert_eq!(second.header("cache-control"), Some("no-store"));
    let replayed = second.json();
    assert_eq!(replayed["compile_version"], 1, "no call, no receipt");
    assert_eq!(replayed["status"], "ready", "{replayed:#}");
    assert!(
        replayed["candidate"]
            .as_str()
            .expect("candidate")
            .contains(&format!("model: {RUN_MODEL}"))
    );
    assert_eq!(replayed["provenance"]["strategy"], "native");
    assert_eq!(
        replayed["provenance"]["plan"]["intent_sha256"],
        document["provenance"]["plan"]["intent_sha256"]
    );
    // The same round again: the same document, still zero calls, the token not renewed.
    let again = server
        .request(&compile_request(&replay(&token, &answers)))
        .await;
    assert_eq!(again.body, second.body);
    assert_eq!(seat.calls(), 1, "the answer rounds called no one");

    // Review material only: no job, run, trace, file or registry entry.
    let after = tree(world.root.path());
    assert_eq!(
        before.keys().collect::<Vec<_>>(),
        after.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        before.get("state/jobs/state.json"),
        after.get("state/jobs/state.json")
    );
    assert_eq!(backend.calls(), 0);
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_revision_in_words_reads_its_base_beside_the_original_intent_and_replays_exactly() {
    let world = TestWorld::new();
    let seat = Seat::start(vec![Reply::Text(native_answer(&candidate(
        "mock/echo",
        true,
    )))]);
    let authoring = NativeAuthoring::new(SEAT, seat.providers()).with_repairs(0);
    let (server, _backend) = start_native(&world, compile_limits(), authoring).await;
    let base = candidate(RUN_MODEL, false);
    let edit = |cognition: &str, fields: &Value| {
        let mut body = json!({
            "compile_version": 2,
            "mode": "edit",
            "cognition": cognition,
            "source": base,
            "change": {"text": CHANGE},
            "original_intent": INTENT,
        });
        merge(&mut body, fields);
        body.to_string()
    };

    let first = server
        .request(&compile_request(&edit("explicitProvider", &json!({}))))
        .await;
    assert_eq!(first.status, 200, "{}", first.body);
    let token = token_of(&first);
    let document = first.json();
    assert_eq!(document["compile_version"], 2);
    assert_eq!(
        document["provenance"]["decision"]["native"]["revision"]["base_sha256"],
        sha256_hex(base.as_bytes())
    );
    let received = &seat.bodies()[0];
    let opening: Value = serde_json::from_str(&message(received, "user")).expect("opening");
    let revised =
        revise_intent(&CompileRequest::edit(base.as_str(), CHANGE).with_original_intent(INTENT))
            .expect("a revision in words");
    assert_eq!(opening["request"], revised.as_str(), "the whole meaning");
    assert_eq!(opening["change"], CHANGE);
    assert_eq!(opening["base_candidate"], base.as_str());

    let answers = json!({"answers": {"model": RUN_MODEL}});
    let token_field = json!({"replay_token": token});
    let mut replay_fields = answers.clone();
    merge(&mut replay_fields, &token_field);
    let second = server
        .request(&compile_request(&edit("deterministicOnly", &replay_fields)))
        .await;
    assert_eq!(second.status, 200, "{}", second.body);
    let replayed = second.json();
    assert_eq!(replayed["status"], "ready", "{replayed:#}");
    let candidate_text = replayed["candidate"].as_str().expect("candidate");
    assert!(candidate_text.contains("./c.md") && candidate_text.contains(RUN_MODEL));

    // A replay repeats its round's input exactly: another base byte, another original intent,
    // another change — each a conflict, never a substitute round.
    for changed in [
        json!({"source": format!("{base}# one more byte\n")}),
        json!({"original_intent": format!("{INTENT} today")}),
        json!({"change": {"text": "also keep a copy in ./d.md"}}),
    ] {
        let mut fields = replay_fields.clone();
        merge(&mut fields, &changed);
        let response = server
            .request(&compile_request(&edit("deterministicOnly", &fields)))
            .await;
        assert_eq!(response.status, 409, "{}", response.body);
        assert_eq!(
            response.json()["error"]["code"],
            "compile_replay_input_changed"
        );
        assert!(
            !response.body.contains(&token),
            "the token is never reflected"
        );
    }
    assert_eq!(
        seat.calls(),
        1,
        "one paid round, every other round zero calls"
    );
    server.stop().await.expect("clean stop");
}

#[tokio::test(flavor = "multi_thread")]
async fn generation_one_answers_byte_for_byte_alike_on_a_native_server_and_never_reaches_the_seat()
{
    let world = TestWorld::new();
    let plain = TestWorld::new();
    let seat = Seat::start(vec![Reply::Text(native_answer(&candidate(
        "mock/echo",
        false,
    )))]);
    let (native, _backend) = start_native(
        &world,
        compile_limits(),
        NativeAuthoring::new(SEAT, seat.providers()),
    )
    .await;
    let backend = Arc::new(TestBackend::completes(ExecutionDisposition::Succeeded));
    let default = plain.start(backend, compile_limits()).await;
    let fixture = fixture();
    let mut bodies: Vec<String> = fixture["cases"]
        .as_array()
        .expect("cases")
        .iter()
        .map(|case| http_body(&fixture, case))
        .collect();
    // Generation-1 refusals, a request for the provider included: the same bytes.
    bodies.extend([
        json!({"compile_version": 1, "mode": "create", "intent": INTENT, "cognition": "explicitProvider"}).to_string(),
        json!({"compile_version": 1, "mode": "create", "intent": INTENT, "model": SEAT}).to_string(),
        json!({"compile_version": 1, "mode": "create", "intent": INTENT}).to_string(),
        r#"{"compile_version":1,"compile_version":1,"mode":"create","intent":"hello"}"#.to_owned(),
        "not json".to_owned(),
    ]);
    for body in &bodies {
        let on_native = native.request(&compile_request(body)).await;
        let on_default = default.request(&compile_request(body)).await;
        assert_eq!(on_native.status, on_default.status, "{body}");
        assert_eq!(on_native.body, on_default.body, "{body}");
        assert!(on_native.header("nika-compile-replay").is_none());
    }
    // The default server never speaks generation 2 and never advertises it.
    let refused = default.request(&compile_request(&fresh(&json!({})))).await;
    assert_eq!(refused.status, 422, "{}", refused.body);
    assert_eq!(
        refused.json()["error"]["code"],
        "compile_version_unsupported"
    );
    let health = default
        .request("GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await
        .json();
    assert!(
        !health["supportedCapabilities"]
            .as_array()
            .expect("tokens")
            .contains(&json!("compileNativeV2"))
    );
    assert_eq!(seat.calls(), 0, "no old request reaches the seat");
    native.stop().await.expect("clean stop");
    default.stop().await.expect("clean stop");
}
