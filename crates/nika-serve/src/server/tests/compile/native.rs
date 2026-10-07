// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Native authoring on the compile door over real loopback (S06): the operator's seat through
//! the typed builder, a controlled seat on the OpenAI-compatible wire that keeps every body it
//! received (`calls()` is the P of the S09 matrix), a Foundry knowledge release the strict door
//! admits, and the answer rounds the server keeps. The core document is compared to what the
//! seat really received; nothing here reads the environment.

use std::io::{Read as _, Write as _};
use std::sync::mpsc;

use nika_cli_host::compile::knowledge::{Snapshot, TrustedIdentity};
use nika_event::source_id::sha256_hex;
use nika_onboard::compile::{AuthoringKnowledge, CompileRequest, revise_intent};
use nika_onboard::knowledge::fixture::{self, Payload};
use nika_providers::ProvidersConfig;

use super::*;
use crate::NativeAuthoring;

mod authority;
mod judged;
mod lifecycle;
mod openapi;
mod reasoning;
mod refusals;
mod replayed;
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
/// The verifier's closed choice, approved (native step 1, R4 A11): the explicit answer
/// scripted at the judge's position, after a candidate READY in its authoring round. The
/// judge's call is a real request, counted like any other.
pub(super) const JUDGE_APPROVES: &str = r#"{"choice":"faithful"}"#;

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

/// A legacy whole-source reply retained for refused or interrupted rounds. Successful
/// creation and source revision use typed answers, never this form.
pub(super) fn native_answer(candidate: &str) -> String {
    json!({"candidate": candidate, "questions": [], "gaps": [], "notes": "read, transform, write"})
        .to_string()
}

/// A source revision states an addition and the exact base write it copies; the compiler
/// owns the resulting source. The separate judge answer is scripted at its own call.
fn copy_revision_answer() -> String {
    json!({"supersedes": [], "adds": [CHANGE], "like": "./b.md", "notes": "copy the result"})
        .to_string()
}

/// The revision seat reads the exact base and both requests, not a changed run model.
fn assert_revision_opening(
    received: &Value,
    document: &Value,
    base: &str,
    original: &str,
    change: &str,
) {
    assert_eq!(
        document["provenance"]["decision"]["native"]["revision"]["base_sha256"],
        sha256_hex(base.as_bytes())
    );
    let opening: Value = serde_json::from_str(&message(received, "user")).expect("opening");
    let revised = revise_intent(&CompileRequest::edit(base, change).with_original_intent(original))
        .expect("a revision in words");
    assert_eq!(opening["request"], revised.as_str(), "the whole meaning");
    assert_eq!(opening["change"], change);
    assert_eq!(opening["base_candidate"], base);
}

/// The words of [`INTENT`] its draft step cites.
pub(super) const DRAFT: &str = "do something clever with it";
/// A part of [`INTENT`] the plan leaves open: the private plan hands the request to the sketch
/// door.
pub(super) const OPEN: &str = "which model runs the rewrite";

/// The private plan's answer for [`INTENT`] (its closed schema): the stated read, a draft whose
/// detail is `draft`, the stated write, and the parts the plan leaves open. The compiler, never
/// the seat, assembles the candidate from it.
pub(super) fn plan_answer(draft: &str, unknowns: &[&str]) -> String {
    json!({"steps": [
        {"op": "read", "detail": "./a.md", "evidence": "Read ./a.md"},
        {"op": "draft", "detail": draft, "evidence": DRAFT},
    ], "effects": [
        {"verb": "write", "target": "./b.md", "policy": "automatic", "evidence": "then write ./b.md"},
    ], "obligations": [], "constraints": [], "unknowns": unknowns, "regions": [],
       "approval_bypass": {"present": false}})
    .to_string()
}

/// The sketch door's graph for [`INTENT`]: read the stated source, one infer, write the stated
/// destination. Structure only; the compiler emits the document and derives its permits.
pub(super) fn sketch_answer() -> String {
    json!({"name": "clever-rewrite", "tasks": [
        {"id": "read_source", "verb": "invoke", "tool": "nika:read", "purpose": "read",
         "reads": ["./a.md"]},
        {"id": "transform", "verb": "infer", "purpose": "rewrite cleverly",
         "with": [{"name": "text", "from": "read_source"}]},
        {"id": "write_result", "verb": "invoke", "tool": "nika:write", "purpose": "write",
         "writes": ["./b.md"], "with": [{"name": "text", "from": "transform"}]},
    ], "questions": [], "gaps": [], "notes": "read, transform, write"})
    .to_string()
}

/// The sketch's one typed hole, filled.
pub(super) fn fills_answer() -> String {
    json!({"fills": [{"task": "transform", "field": "prompt",
        "value": "Rewrite this text in a clever way, inventing nothing: ${{ with.text }}"}],
        "notes": "one hole"})
    .to_string()
}

/// A kept native question round: the plan leaves a part open, the sketch and its fill follow,
/// and the compiler asks for the run model — three requests, the round and its token kept.
pub(super) fn question_round() -> Vec<Reply> {
    vec![
        Reply::Text(plan_answer(DRAFT, &[OPEN])),
        Reply::Text(sketch_answer()),
        Reply::Text(fills_answer()),
    ]
}

/// A Foundry knowledge release on disk: its root, which is the directory the operator names
/// (`snapshot`, the word every door's flag still takes), and the identity the operator's host,
/// which sealed it, trusts for its latest seal.
pub(super) struct Foundry {
    pub(super) root: std::path::PathBuf,
    pub(super) snapshot: std::path::PathBuf,
    sealed: std::cell::RefCell<Option<TrustedIdentity>>,
}

/// The block the release ships: its first line is a marker a test finds in what the seat read.
pub(super) const BLOCK_FILE: &str = "blocks/s06-transform.nika";
pub(super) const BLOCK_TEXT: &str = "# S06-BLOCK-MARKER\nnika: read-transform-write\ntasks: {}\n";

/// The release: a policy-R payload the strict door admits, every row synthetic.
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
    let mut family = row("family", "family:s06-text-rewrite", "Text rewrite", "NONE");
    family["need"] =
        json!("Read a text file, do something clever with it, write the result to a file");
    family["facets"] = json!({});
    let pack = row("pattern_pack", "pack:s06-rewrite", "Rewrite pack", "NONE");
    let mut pattern = row(
        "pattern",
        "pattern:s06-transform-text",
        "Transform text",
        "NONE",
    );
    pattern["purpose"] =
        json!("S06-PATTERN-MARKER one infer transforms the text read, then the result is written");
    pattern["notes"] = json!("");
    let file_sha = sha256_hex(BLOCK_TEXT.as_bytes());
    let mut block = row(
        "block",
        "block:s06-transform",
        "Read, transform, write",
        "CHECKED",
    );
    block["purpose"] = json!("read a file, one infer, write the result");
    block["file"] = json!(BLOCK_FILE);
    block["file_sha256"] = json!(file_sha);
    for list in [
        "holes",
        "effects",
        "authority",
        "interfaces",
        "callables",
        "known_failure_modes",
    ] {
        block[list] = json!([]);
    }
    block["check_receipt"] = json!({
        "verifier_sha256": fixture::VERIFIER, "spec_sha": fixture::SPEC, "sha256": file_sha,
        "verdict": "CURRENT_CHECKED",
    });
    let mut repair = row(
        "repair_principle",
        "repair:S06_PATH",
        "stated paths only",
        "NONE",
    );
    repair["strategy"] = json!("read and write exactly the paths the request states");
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
        fixture::edge("family:s06-text-rewrite", "RECOMMENDS", "pack:s06-rewrite"),
        fixture::edge("pack:s06-rewrite", "CONTAINS", "pattern:s06-transform-text"),
        fixture::edge(
            "block:s06-transform",
            "REALIZES",
            "pattern:s06-transform-text",
        ),
        fixture::edge(
            "diagnostic:NIKA-PARSE-022",
            "SUGGESTS_REPAIR",
            "repair:S06_PATH",
        ),
    ];
    payload
}

impl Foundry {
    /// A synthetic release under `base`, sealed at the fixture's version.
    pub(super) fn create(base: &std::path::Path) -> Self {
        let root = base.join("release");
        let foundry = Self {
            root: root.clone(),
            snapshot: root,
            sealed: std::cell::RefCell::new(None),
        };
        foundry.reseal(VERSION);
        foundry
    }

    /// The identity of the latest seal, from the bytes this host wrote (never read back).
    pub(super) fn identity(&self) -> TrustedIdentity {
        self.sealed.borrow().clone().expect("sealed")
    }

    /// Write the release again in place, sealed at `version`.
    pub(super) fn reseal(&self, version: &str) {
        let mut files = release().render();
        let mut manifest = Payload::manifest(&files);
        manifest["knowledge_version"] = json!(version);
        files.insert(
            fixture::manifest_path().to_owned(),
            fixture::manifest_bytes(&manifest),
        );
        *self.sealed.borrow_mut() = fixture::identity_of(&files);
        if self.root.exists() {
            std::fs::remove_dir_all(&self.root).expect("replaced");
        }
        fixture::write_files(&self.root, &files).expect("a release");
    }

    /// The pack the one knowledge door composes for `intent` from this release.
    pub(super) fn pack(&self, intent: &str) -> AuthoringKnowledge {
        Snapshot::open(&self.snapshot, Some(&self.identity()))
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

/// A native finish a deterministicOnly answer round held for its judge (native step 2): the
/// whole request still open and the finding naming the judge a round can permit.
fn assert_held(document: &Value) {
    let open = &document["provenance"]["decision"]["pending"]["open"];
    assert!(
        open.as_array().is_some_and(|open| !open.is_empty()),
        "{document:#}"
    );
    assert!(
        document["diagnostics"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|d| d.to_string().contains("semantic_verification")),
        "{document:#}"
    );
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
/// the snapshot's hashes and none of its host paths. `sent` is the round's first request (the
/// private plan's), `calls` the requests the round sent.
#[cfg(unix)] // the disk form is defined for Unix descriptors only
fn assert_first_round(
    foundry: &Foundry,
    world: &TestWorld,
    (sent, calls): (&Value, u64),
    document: &Value,
) {
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
    // The private plan's opening: the request itself, byte for byte.
    assert_eq!(message(sent, "user"), INTENT);
    let provenance = &document["provenance"];
    assert_eq!(provenance["cognition"], "explicitProvider");
    assert_eq!(provenance["strategy"], "native");
    let receipt = &provenance["authoring"];
    assert_eq!(receipt["model"], SEAT);
    assert_eq!(receipt["calls"], calls);
    // Each completion reports 1000 in and 200 out: the receipt sums what was sent.
    assert_eq!(receipt["input_tokens"], 1000 * calls);
    assert_eq!(receipt["output_tokens"], 200 * calls);
    assert_eq!(
        receipt["context"][0]["instruction_sha256"],
        sha256_hex(system.as_bytes()),
        "the receipt names the instruction the seat really read"
    );
    assert_eq!(receipt["backend"]["kind"], "direct_api");
    assert_eq!(receipt["backend"]["provider"], "vllm");
    assert_eq!(receipt["backend"]["requested_model"], SEAT);
    assert_eq!(
        receipt["backend"]["authority"]["http_requests"]["sent"],
        calls
    );
    assert_eq!(
        receipt["backend"]["cost_basis"],
        "unpriced; billing_unverified"
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
#[cfg(unix)] // the disk form is defined for Unix descriptors only
async fn a_native_round_reads_the_pinned_pack_under_the_operators_seat_and_its_answer_round_calls_no_one()
 {
    let world = TestWorld::new();
    let foundry = Foundry::create(&world.root.path().join("knowledge"));
    let seat = Seat::start(question_round());
    let authoring = NativeAuthoring::new(SEAT, seat.providers())
        .with_knowledge_release(&foundry.snapshot, foundry.identity())
        .with_max_tokens(4096)
        // The plan, the sketch, its fill and the judgment; repairs stay the default preference.
        .with_max_calls(4);
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
    // The plan, the sketch and its fill: a question round, its judgment not yet due.
    assert_eq!(seat.calls(), 3);
    assert_first_round(&foundry, &world, (&seat.bodies()[0], 3), &document);

    // The answer round: the kept plan, this round's answers, zero calls — the same plan the
    // paid round produced, now baked and held: a deterministicOnly round permits no judge
    // (native step 2), so the finish stays INCOMPLETE, its candidate kept as the preview.
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
    assert_eq!(replayed["status"], "incomplete", "{replayed:#}");
    assert_held(&replayed);
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
    assert_eq!(seat.calls(), 3, "the answer rounds called no one");

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
    // Leave the new destination open, rather than changing the base's already chosen model.
    const ORIGINAL: &str = "Read ./a.md and do something clever with it. Write ./b.md.";
    const CHANGE_PATH: &str = "Change the destination file.";
    let world = TestWorld::new();
    let links = json!({"supersedes": [{"replaces": "Write ./b.md",
        "by": "Change the destination file"}], "adds": [], "notes": "ask the new path"});
    let seat = Seat::start(vec![Reply::Text(links.to_string())]);
    let authoring = NativeAuthoring::new(SEAT, seat.providers()).with_repairs(0);
    let (server, _backend) = start_native(&world, compile_limits(), authoring).await;
    let base = candidate(RUN_MODEL, false);
    let edit = |cognition: &str, fields: &Value| {
        let mut body = json!({
            "compile_version": 2,
            "mode": "edit",
            "cognition": cognition,
            "source": base,
            "change": {"text": CHANGE_PATH},
            "original_intent": ORIGINAL,
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
    assert_eq!(document["status"], "incomplete", "{document:#}");
    assert!(document["candidate"].is_null(), "{document:#}");
    let questions = document["questions"].as_array().expect("questions");
    assert_eq!(questions.len(), 1, "{questions:#?}");
    assert_eq!(questions[0]["key"], "revision.path");
    assert_revision_opening(&seat.bodies()[0], &document, &base, ORIGINAL, CHANGE_PATH);
    assert_eq!(seat.calls(), 1, "only the typed links are requested");

    let answers = json!({"answers": {"revision.path": "./c.md"}});
    let token_field = json!({"replay_token": token});
    let mut replay_fields = answers.clone();
    merge(&mut replay_fields, &token_field);
    let second = server
        .request(&compile_request(&edit("deterministicOnly", &replay_fields)))
        .await;
    assert_eq!(second.status, 200, "{}", second.body);
    // Held for its judge (native step 2): deterministicOnly permits none, so the replayed
    // revision stays INCOMPLETE with its candidate kept as the preview, still zero calls.
    let replayed = second.json();
    assert_eq!(replayed["status"], "incomplete", "{replayed:#}");
    assert_held(&replayed);
    let candidate_text = replayed["candidate"].as_str().expect("candidate");
    assert_eq!(candidate_text, base.replace("./b.md", "./c.md"));
    assert!(
        candidate_text.contains(RUN_MODEL),
        "the base model is unchanged"
    );
    let again = server
        .request(&compile_request(&edit("deterministicOnly", &replay_fields)))
        .await;
    assert_eq!(again.status, 200, "{}", again.body);
    assert_eq!(again.body, second.body, "exact replay, no new round");
    assert_eq!(seat.calls(), 1, "both answer rounds are zero-call replays");

    // A replay repeats its round's input exactly: another base byte, another original intent,
    // another change — each a conflict, never a substitute round.
    for changed in [
        json!({"source": format!("{base}# one more byte\n")}),
        json!({"original_intent": format!("{ORIGINAL} today")}),
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
