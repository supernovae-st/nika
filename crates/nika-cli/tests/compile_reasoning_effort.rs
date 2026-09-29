// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::disallowed_types
)]

//! An explicit reasoning effort at the real doors (R4 B16), three kinds of evidence kept apart.
//!
//! The binary: `nika compile` and `nika serve` against a loopback recorder standing where the
//! `DeepSeek` endpoint would (the operator's base-URL override: a gateway the catalog never
//! qualifies). The flag outranks the environment, an unknown word refuses before any request or
//! before the listener binds, a named level is recorded and refused with nothing sent, stated
//! money closes every seat whatever the level, and each control proves the recorder sees the
//! attempt a door makes.
//!
//! The seam: the shared Onboard producer and the compiler's calls through the actual provider
//! registry, its kernel HTTP effect a double on the provider's own endpoint that keeps every
//! request it is handed: the bytes a qualified route dispatches, never a network capture.
//!
//! No independent network capture exists here. Scripted completions prove mechanics; they
//! qualify no provider reasoning.

use nika_kernel::http::{HttpError, HttpPostDyn, HttpRequest, HttpResponse, HttpStreamResponse};
use nika_kernel::secret::Secret;
use nika_onboard::compile::decide::{ChoiceOption, ChoiceQuestion, DecisionSeat, ProviderChoice};
use nika_onboard::compile::{Cognition, CompileOutcome, CompileRequest, compile_with_cognition};
use nika_onboard::compile_config as config;
use nika_providers::{ProviderRegistry, ProvidersConfig, ResolvedProvider};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::io::{Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// The one model the catalog lists effort levels for (low · high · max).
const PRO: &str = "deepseek/deepseek-v4-pro";

/// Work the deterministic reader cannot settle alone: a named seat is asked.
const FREE: &str = "Review this customer request and harmonise the tone of the support reply";

/// Work over a stated file, with the facts the host observed about it.
const TICKETS: &str =
    "Read ./data/tickets.csv, harmonise the tone of every reply, then write ./out/replies.csv";

/// A finite lexical ambiguity the decision seat settles (WARM), every other clause explicit.
const WARM: &str =
    "Cherche les éléments demandés, puis propose par écrit trois créneaux compatibles.";

/// A scripted completion: an empty object, the model named, usage with its reasoning tokens.
const COMPLETION: &str = r#"{"id":"b16","object":"chat.completion","model":"deepseek-v4-pro","choices":[{"index":0,"message":{"role":"assistant","content":"{}"},"finish_reason":"stop"}],"usage":{"prompt_tokens":120,"completion_tokens":60,"total_tokens":180,"prompt_cache_hit_tokens":0,"prompt_cache_miss_tokens":120,"completion_tokens_details":{"reasoning_tokens":42}}}"#;

/// Where the schema instruction starts on a JSON-object seat's last user turn.
const SCHEMA_INSTRUCTION: &str = "\n\nReply with ONLY a JSON value";

// ── The seam: the shared producer and the compiler's calls, dispatched bytes kept ──

/// The kernel HTTP effect on the provider's own endpoint: every request the adapter dispatched,
/// each answered with [`COMPLETION`].
#[derive(Default)]
struct Capture {
    requests: Mutex<Vec<HttpRequest>>,
}

impl Capture {
    /// Each dispatched request's URL and JSON body, in order.
    fn sent(&self) -> Vec<(String, Value)> {
        self.requests
            .lock()
            .expect("requests")
            .iter()
            .map(|r| {
                let body = r.body.as_deref().expect("a body");
                (r.url.clone(), serde_json::from_slice(body).expect("JSON"))
            })
            .collect()
    }
}

impl HttpPostDyn for Capture {
    fn supports_single_attempt(&self) -> bool {
        true
    }

    async fn post(&self, request: HttpRequest) -> Result<HttpResponse, HttpError> {
        let url = request.url.clone();
        self.requests.lock().expect("requests").push(request);
        let body = COMPLETION.as_bytes().to_vec();
        Ok(HttpResponse::new(
            200,
            std::collections::BTreeMap::new(),
            body.into(),
            url,
        ))
    }

    async fn send_streaming(&self, _: HttpRequest) -> Result<HttpStreamResponse, HttpError> {
        Err(HttpError::Other {
            reason: "every authoring call is buffered".to_owned(),
        })
    }
}

/// The `DeepSeek` route as the registry resolves it with no override: its direct endpoint.
fn direct(capture: &Arc<Capture>) -> ResolvedProvider<Capture> {
    let providers = ProvidersConfig::new().with_key("deepseek", Secret::new("fixture"));
    ProviderRegistry::new(Arc::clone(capture), providers)
        .resolve(PRO)
        .expect("the direct route resolves")
}

/// The shared producer's configuration for `strategy` and `word`, none from the environment.
fn configured(strategy: &str, word: Option<&str>) -> config::AuthoringConfig {
    let mut named = config::AuthoringSettings::none().with_strategy(strategy);
    if let Some(word) = word {
        named = named.with_reasoning(word);
    }
    config::resolve(&named, &config::AuthoringSettings::none()).expect("a valid configuration")
}

/// What the host observed about the stated file: a header, never a row.
fn observed() -> Value {
    json!({"observed": [{"path": "./data/tickets.csv", "kind": "csv", "columns": ["id", "customer", "reply"]}]})
}

/// One compile of `intent` under the producer's policy for `config`, the facts beside it.
async fn authored(
    capture: &Arc<Capture>,
    config: &config::AuthoringConfig,
    intent: &str,
    cap: u32,
) -> CompileOutcome {
    let policy = config
        .policy(PRO, cap, Duration::from_secs(30))
        .expect("bounded")
        .with_repairs(0);
    let request = CompileRequest::create(intent)
        .with_knowledge(observed())
        .with_authoring_policy(policy);
    let provider = direct(capture);
    let cognition = Cognition {
        provider: Some(&provider),
        seat: None,
    };
    Box::pin(compile_with_cognition(&request, cognition))
        .await
        .expect("compiles")
}

/// The text of the first message of a role in a dispatched body.
fn text_of(body: &Value, role: &str) -> String {
    body["messages"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|m| m["role"] == role)
        .and_then(|m| m["content"].as_str())
        .unwrap_or_default()
        .to_owned()
}

fn sha256(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

/// The receipt's journal of calls.
fn context(outcome: &CompileOutcome) -> Vec<Value> {
    outcome
        .provenance
        .authoring
        .as_ref()
        .map(|receipt| receipt.context.clone())
        .unwrap_or_default()
}

/// A named max leaves the direct route as the two structural keys under the declared cap, small
/// or large, beside the request and the facts the host observed; the receipt names the
/// instruction and the schema the body carried, and reads the level back from those bytes.
#[tokio::test]
async fn a_named_max_leaves_with_the_request_its_facts_and_its_identities() {
    for cap in [2048, 16_384] {
        let capture = Arc::new(Capture::default());
        let outcome = authored(&capture, &configured("only", Some("max")), TICKETS, cap).await;
        let sent = capture.sent();
        assert_eq!(sent.len(), 1, "{cap}: {outcome:#?}");
        let (url, body) = &sent[0];
        assert!(url.starts_with("https://api.deepseek.com/"), "{url}");
        assert_eq!(body["model"], "deepseek-v4-pro");
        assert_eq!(body["max_tokens"].as_u64(), Some(u64::from(cap)));
        assert_eq!(body["thinking"], json!({"type": "enabled"}), "{cap}");
        assert_eq!(body["reasoning_effort"], "max", "{cap}: never low");
        let call = &context(&outcome)[0];
        assert_eq!(call["instruction_sha256"], sha256(&text_of(body, "system")));
        let user = text_of(body, "user");
        let (opening, instruction) = user
            .split_once(SCHEMA_INSTRUCTION)
            .expect("the schema instruction on the user turn");
        let opening: Value = serde_json::from_str(opening).expect("the opening's JSON");
        assert_eq!(opening["request"], TICKETS, "the request, whole");
        assert_eq!(opening["observed_world"], observed(), "the observed facts");
        let (_, schema) = instruction
            .split_once("schema:\n")
            .expect("the schema, verbatim");
        assert_eq!(call["schema_sha256"], sha256(schema), "{schema}");
        assert_eq!(
            call["reasoning"],
            json!({
                "configured": "max",
                "transmitted": {"thinking": "enabled", "effort": "max"},
                "served": "unknown",
                "reasoning_tokens": 42,
                "response_model": "deepseek-v4-pro",
            }),
            "{cap}"
        );
    }
}

/// The escalating strategy asks max of its plan call and of the native call after it; the plan
/// call carries the request as written.
#[tokio::test]
async fn every_call_of_an_escalating_round_asks_the_named_level() {
    let capture = Arc::new(Capture::default());
    let outcome = authored(&capture, &configured("escalate", Some("max")), FREE, 8192).await;
    let sent = capture.sent();
    assert_eq!(
        sent.len(),
        2,
        "the plan, then the native call: {outcome:#?}"
    );
    for (_, body) in &sent {
        assert_eq!(body["thinking"], json!({"type": "enabled"}), "{body}");
        assert_eq!(body["reasoning_effort"], "max", "{body}");
        assert_eq!(body["max_tokens"].as_u64(), Some(8192), "{body}");
    }
    let plan = text_of(&sent[0].1, "user");
    assert_eq!(plan.split_once(SCHEMA_INSTRUCTION).map(|p| p.0), Some(FREE));
    let calls: Vec<Value> = context(&outcome)
        .iter()
        .map(|call| call["call"].clone())
        .collect();
    assert_eq!(calls, [json!("plan"), json!("native")]);
}

/// Without a level the route keeps its own bytes (the bounded low under 8192 tokens, nothing
/// above) and the receipt reads those bytes back, apart from the unset configuration.
#[tokio::test]
async fn without_a_level_the_direct_route_keeps_its_own_bytes() {
    for (cap, effort) in [(2048, json!("low")), (16_384, Value::Null)] {
        let capture = Arc::new(Capture::default());
        let outcome = authored(&capture, &configured("only", None), TICKETS, cap).await;
        let sent = capture.sent();
        assert_eq!(sent.len(), 1, "{cap}");
        let body = &sent[0].1;
        assert!(body.get("thinking").is_none(), "{cap}: {body}");
        let sent_effort = body.get("reasoning_effort").cloned().unwrap_or(Value::Null);
        assert_eq!(sent_effort, effort, "{cap}: {body}");
        let call = &context(&outcome)[0];
        assert_eq!(call["reasoning"]["configured"], Value::Null, "{cap}");
        assert_eq!(
            call["reasoning"]["transmitted"],
            json!({"thinking": null, "effort": effort}),
            "{cap}"
        );
    }
}

/// The decision call asks the producer's level under the declared authoring cap, and keeps its
/// own compact call (256 tokens, the route's low) when none is named.
#[tokio::test]
async fn the_decision_call_asks_the_level_under_the_declared_cap_on_the_wire() {
    let question = ChoiceQuestion::new(
        "clause-0",
        "Which operation does this clause of the request ask for?",
        json!({"request": WARM}),
        vec![ChoiceOption::new("lookup", "find one record")],
    );
    let level = configured("escalate", Some("max"))
        .reasoning
        .expect("named");
    let capture = Arc::new(Capture::default());
    let provider = direct(&capture);
    let choice =
        ProviderChoice::new(&provider, PRO, Duration::from_secs(5)).with_reasoning(level, 16_384);
    let _answer = choice.choose(&question).await;
    let sent = capture.sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].1["max_tokens"].as_u64(), Some(16_384));
    assert_eq!(sent[0].1["thinking"], json!({"type": "enabled"}));
    assert_eq!(sent[0].1["reasoning_effort"], "max");

    let capture = Arc::new(Capture::default());
    let provider = direct(&capture);
    let choice = ProviderChoice::new(&provider, PRO, Duration::from_secs(5));
    let _answer = choice.choose(&question).await;
    let sent = capture.sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].1["max_tokens"].as_u64(), Some(256), "compatibility");
    assert_eq!(sent[0].1["reasoning_effort"], "low");
    assert!(sent[0].1.get("thinking").is_none());
}

// ── The binary: the CLI and Serve doors against a loopback recorder ──

/// `nika` in a clean room: no keychain, no run key, the `DeepSeek` key a fixture, nothing else.
fn command(room: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_nika"));
    cmd.env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", room)
        .env("NIKA_KEYCHAIN", "off")
        .env("NIKA_RUN_KEY_FILE", room.join("absent-run.key"))
        .env("NIKA_RUN_PUB_FILE", room.join("absent-run.pub"))
        .env("NO_COLOR", "1")
        .env("DEEPSEEK_API_KEY", "fixture-not-a-key")
        .current_dir(room)
        .stdin(Stdio::null());
    cmd
}

/// The authoring seat named: one call, a short wait.
const SEATED: [&str; 6] = [
    "--authoring-model",
    PRO,
    "--authoring-timeout",
    "5",
    "--authoring-repairs",
    "0",
];

/// `nika compile <request> --json` with `flags`, the environment's `ambient` effort word and the
/// `DeepSeek` endpoint the recorder's.
fn compile(
    room: &Path,
    recorder: &Recorder,
    request: &str,
    flags: &[&str],
    ambient: Option<&str>,
) -> Output {
    let mut cmd = command(room);
    cmd.env("NIKA_DEEPSEEK_BASE_URL", recorder.url())
        .args(["compile", request, "--json"])
        .args(flags);
    if let Some(word) = ambient {
        cmd.env("NIKA_AUTHORING_REASONING", word);
    }
    cmd.output().expect("compile")
}

/// `flags` with the effort flag naming `word`, when given.
fn with_word<'a>(flags: &[&'a str], word: Option<&'a str>) -> Vec<&'a str> {
    let mut all = flags.to_vec();
    if let Some(word) = word {
        all.extend(["--authoring-reasoning", word]);
    }
    all
}

fn document(out: &Output) -> Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "{e}: stdout={} stderr={}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// The authoring requests the host counted before transport (absent when none was prepared).
fn prepared(doc: &Value) -> u64 {
    doc["provenance"]["authoring"]["backend"]["authority"]["http_requests"]["sent"]
        .as_u64()
        .unwrap_or(0)
}

/// The authoring calls the receipt journals (none without a receipt).
fn calls(doc: &Value) -> u64 {
    doc["provenance"]["authoring"]["calls"]
        .as_u64()
        .unwrap_or(0)
}

fn says(doc: &Value, text: &str) -> bool {
    doc["diagnostics"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|d| d["message"].as_str().is_some_and(|m| m.contains(text)))
}

/// A loopback stand-in for the `DeepSeek` endpoint: it counts every accepted connection, keeps
/// every complete request body apart, and answers each with [`COMPLETION`]. Dropping it stops
/// and joins.
struct Recorder {
    port: u16,
    accepts: Arc<AtomicUsize>,
    bodies: Arc<Mutex<Vec<Value>>>,
    stop: Arc<AtomicBool>,
    server: Option<JoinHandle<()>>,
}

impl Recorder {
    // The synchronous fixture owns this blocking socket thread and joins it on drop.
    #[allow(clippy::disallowed_methods)]
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback endpoint");
        let port = listener.local_addr().expect("address").port();
        let accepts = Arc::new(AtomicUsize::new(0));
        let bodies = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (seen, kept, halt) = (Arc::clone(&accepts), Arc::clone(&bodies), Arc::clone(&stop));
        let server = std::thread::spawn(move || {
            for stream in listener.incoming() {
                if halt.load(Ordering::SeqCst) {
                    break;
                }
                seen.fetch_add(1, Ordering::SeqCst);
                let Ok(mut stream) = stream else { continue };
                if let Some(body) = read_body(&mut stream) {
                    kept.lock().expect("bodies").push(body);
                }
                let head = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    COMPLETION.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(COMPLETION.as_bytes());
            }
        });
        Self {
            port,
            accepts,
            bodies,
            stop,
            server: Some(server),
        }
    }

    /// The complete endpoint URL, as `NIKA_DEEPSEEK_BASE_URL` takes it.
    fn url(&self) -> String {
        format!("http://127.0.0.1:{}/v1/chat/completions", self.port)
    }

    /// Accepted connections and complete request bodies, apart.
    fn counts(&self) -> (usize, usize) {
        (
            self.accepts.load(Ordering::SeqCst),
            self.bodies.lock().expect("bodies").len(),
        )
    }

    fn bodies(&self) -> Vec<Value> {
        self.bodies.lock().expect("bodies").clone()
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        // The flag ends the accept loop at its next connection; the wake connection is that one.
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(server) = self.server.take() {
            let _ = server.join();
        }
    }
}

/// One whole request's JSON body: its headers, then every byte its length declares.
fn read_body(stream: &mut TcpStream) -> Option<Value> {
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .ok()?;
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        let n = stream.read(&mut chunk).ok()?;
        if n == 0 || buffer.len() > 4 << 20 {
            return None;
        }
        buffer.extend_from_slice(&chunk[..n]);
        let Some(end) = buffer.windows(4).position(|w| w == b"\r\n\r\n") else {
            continue;
        };
        let head = String::from_utf8_lossy(&buffer[..end]).to_lowercase();
        let length = head
            .lines()
            .find_map(|line| line.strip_prefix("content-length:"))
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(0);
        if buffer.len() >= end + 4 + length {
            return serde_json::from_slice(&buffer[end + 4..end + 4 + length]).ok();
        }
    }
}

/// The control: no level named, the request reaches the recorder with the route's own bytes and
/// the receipt reads them back.
#[test]
fn without_a_level_the_cli_sends_the_request_with_the_routes_own_bytes() {
    let recorder = Recorder::start();
    let room = tempfile::tempdir().expect("room");
    let doc = document(&compile(room.path(), &recorder, FREE, &SEATED, None));
    let bodies = recorder.bodies();
    assert!(!bodies.is_empty(), "{doc}");
    assert_eq!(bodies.len() as u64, prepared(&doc), "{doc}");
    for body in &bodies {
        assert_eq!(body["model"], "deepseek-v4-pro", "{body}");
        assert_eq!(body["reasoning_effort"], "low", "{body}");
        assert!(body.get("thinking").is_none(), "{body}");
    }
    assert!(
        text_of(&bodies[0], "user").starts_with(FREE),
        "{}",
        bodies[0]
    );
    let call = &doc["provenance"]["authoring"]["context"][0];
    assert_eq!(call["reasoning"]["configured"], Value::Null, "{doc}");
    assert_eq!(
        call["reasoning"]["transmitted"],
        json!({"thinking": null, "effort": "low"}),
        "{doc}"
    );
}

/// A level named by the flag or the environment (the flag first) is the level asked and
/// recorded; the override is no route the catalog binds, so the call refuses before a byte
/// leaves: no connection, no body, nothing counted.
#[test]
fn a_named_level_is_asked_and_refused_before_a_byte_leaves_an_unqualified_route() {
    let room = tempfile::tempdir().expect("room");
    for (flag, ambient, asked) in [
        (Some("max"), None, "max"),
        (None, Some("max"), "max"),
        (Some("high"), Some("max"), "high"),
        (Some("max"), Some("medium"), "max"),
        (Some("low"), None, "low"),
    ] {
        let recorder = Recorder::start();
        let flags = with_word(&SEATED, flag);
        let doc = document(&compile(room.path(), &recorder, FREE, &flags, ambient));
        let case = format!("{flag:?} over {ambient:?}");
        assert_eq!(recorder.counts(), (0, 0), "{case}: {doc}");
        assert_eq!(prepared(&doc), 0, "{case}: {doc}");
        let call = &doc["provenance"]["authoring"]["context"][0];
        assert_eq!(call["reasoning"]["configured"], asked, "{case}: {doc}");
        assert_eq!(call["reasoning"]["transmitted"], "unobserved", "{case}");
        assert_eq!(
            call["result"]["failure_kind"], "admission_refused",
            "{case}"
        );
        let reason = format!(
            "the explicit reasoning effort `{asked}` is not qualified for deepseek/deepseek-v4-pro"
        );
        assert!(says(&doc, &reason), "{case}: {doc}");
        assert!(says(&doc, "nothing was sent"), "{case}: {doc}");
    }
}

/// A word outside low · high · max refuses the configuration before any request, whichever
/// source names it and never falling back to the other; a word with no seat to ask it is a
/// usage error, and the deterministic door never reads the environment's word.
#[test]
fn an_unknown_word_refuses_before_any_request() {
    let room = tempfile::tempdir().expect("room");
    let recorder = Recorder::start();
    for (flag, ambient, word) in [
        (Some("maximum"), None, "maximum"),
        (Some("MAX"), None, "MAX"),
        (Some("medium"), Some("max"), "medium"),
        (None, Some("medium"), "medium"),
        (None, Some("x-high"), "x-high"),
    ] {
        let flags = with_word(&SEATED, flag);
        let out = compile(room.path(), &recorder, FREE, &flags, ambient);
        let said = text(&out);
        assert_eq!(out.status.code(), Some(3), "{word}: {said}");
        let named = format!("`{word}` is not a reasoning effort");
        assert!(said.contains(&named), "{word}: {said}");
    }
    assert_eq!(recorder.counts(), (0, 0));
    // An incomplete compile exits 2 too: the usage error is the one that writes no document
    // and names the seats the word needs.
    let unseated = command(room.path())
        .args(["compile", FREE, "--json", "--authoring-reasoning", "max"])
        .output()
        .expect("usage");
    let said = text(&unseated);
    assert_eq!(unseated.status.code(), Some(2), "{said}");
    assert!(unseated.stdout.is_empty(), "no document: {said}");
    assert!(
        said.contains("--authoring-model") && said.contains("--decision-model"),
        "{said}"
    );
    let hello = command(room.path())
        .env("NIKA_AUTHORING_REASONING", "medium")
        .args(["compile", "hello", "--json"])
        .output()
        .expect("hello");
    assert_eq!(document(&hello)["status"], "ready", "{}", text(&hello));
}

/// Stated money closes every seat whatever the level (B15): a zero, a positive and an
/// inconsistent ceiling record no call and send nothing. The same request with no money reaches
/// the wire and is refused there, at max: the money law, not the route, stopped the others.
#[test]
fn stated_money_opens_no_seat_whatever_the_level() {
    let room = tempfile::tempdir().expect("room");
    let recorder = Recorder::start();
    let flags = with_word(&SEATED, Some("max"));
    for request in [
        format!("{FREE}, budget 0 USD"),
        format!("{FREE}. Budget: 2 USD."),
        format!("{FREE}. Budget: 2 USD. Budget: 3 USD."),
    ] {
        let doc = document(&compile(room.path(), &recorder, &request, &flags, None));
        assert_eq!(calls(&doc), 0, "{request}: {doc}");
        assert_eq!(prepared(&doc), 0, "{request}: {doc}");
        assert_ne!(doc["status"], "ready", "{request}: {doc}");
    }
    assert_eq!(recorder.counts(), (0, 0));
    let doc = document(&compile(room.path(), &recorder, FREE, &flags, None));
    assert!(calls(&doc) >= 1, "{doc}");
    let call = &doc["provenance"]["authoring"]["context"][0];
    assert_eq!(call["reasoning"]["configured"], "max", "{doc}");
    assert_eq!(call["result"]["failure_kind"], "admission_refused", "{doc}");
}

/// The decision seat alone asks the named level; the route refuses it with nothing sent. The
/// control, with no level, sends the seat's own compact call: the decision seat is reached.
#[test]
fn the_decision_seat_asks_the_named_level_or_keeps_its_own_call() {
    let room = tempfile::tempdir().expect("room");
    let control = Recorder::start();
    let decision = ["--decision-model", PRO];
    let doc = document(&compile(room.path(), &control, WARM, &decision, None));
    let bodies = control.bodies();
    assert_eq!(bodies.len(), 1, "{doc}");
    assert_eq!(bodies[0]["max_tokens"].as_u64(), Some(256), "{doc}");
    assert_eq!(bodies[0]["reasoning_effort"], "low", "{doc}");
    assert!(bodies[0].get("thinking").is_none(), "{doc}");
    let named = Recorder::start();
    let flags = with_word(&decision, Some("max"));
    let doc = document(&compile(room.path(), &named, WARM, &flags, None));
    assert_eq!(named.counts(), (0, 0), "{doc}");
    let refused = "the explicit reasoning effort `max` is not qualified";
    assert!(doc.to_string().contains(refused), "{doc}");
}

// ── Serve: the operator's flags over the environment, judged before the listener binds ──

#[cfg(unix)]
const REGISTRY: &str = "nika: proj\narm:\n  - workflow: workflows/doctor.nika\n    cadence: \"TZ=UTC 0 3 * * *\"\n    plafond: 0.05\n    manqué: sauter\n";
#[cfg(unix)]
const TRUE: &str =
    "nika: armed-true\npermits: { exec: true }\ntasks:\n  ok:\n    exec: { shell: \"true\" }\n";
#[cfg(unix)]
const TOKEN: &str = "test-only-credential-material-0123456789";
#[cfg(unix)]
const NATIVE_INTENT: &str = "Read ./a.md and do something clever with it, then write ./b.md";

/// A project `nika serve` can bind: the registry, one workflow, the owner-only Bearer file.
#[cfg(unix)]
fn project(room: &Path) {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::create_dir_all(room.join("workflows")).expect("workflows");
    std::fs::write(room.join("nika.yaml"), REGISTRY).expect("registry");
    std::fs::write(room.join("workflows/doctor.nika"), TRUE).expect("workflow");
    let token = room.join("token");
    std::fs::write(&token, TOKEN).expect("token");
    std::fs::set_permissions(&token, std::fs::Permissions::from_mode(0o600)).expect("mode");
}

/// `nika serve` seating the `DeepSeek` model at the recorder, with `flags` and `ambient`.
#[cfg(unix)]
fn serve(
    room: &Path,
    address: &str,
    recorder: &Recorder,
    flags: &[&str],
    ambient: Option<&str>,
) -> Command {
    let mut cmd = command(room);
    cmd.env("NIKA_DEEPSEEK_BASE_URL", recorder.url())
        .args(["serve", "--bind", address, "--workflows", "workflows"])
        .args(["--token-file", "token", "--authoring-model", PRO])
        .args(["--authoring-repairs", "0"])
        .args(flags);
    if let Some(word) = ambient {
        cmd.env("NIKA_AUTHORING_REASONING", word);
    }
    cmd
}

/// The one-line HTTP exchange: status and body.
#[cfg(unix)]
fn http(address: &str, request: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(address).expect("connect serve");
    stream.write_all(request.as_bytes()).expect("request");
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).expect("response");
    let text = String::from_utf8(bytes).expect("UTF-8 response");
    let (head, body) = text.split_once("\r\n\r\n").expect("HTTP boundary");
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .expect("status");
    (status, body.to_owned())
}

/// Wait for the listener's public health answer.
#[cfg(unix)]
fn healthy(address: &str, child: &mut std::process::Child) {
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    loop {
        if TcpStream::connect(address).is_ok() {
            let health = "GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";
            if http(address, health).0 == 200 {
                return;
            }
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            panic!("serve never became healthy");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// SIGTERM, then the exit code of a clean stop.
#[cfg(unix)]
fn terminate(child: &mut std::process::Child) -> Option<i32> {
    let pid = nix::unistd::Pid::from_raw(i32::try_from(child.id()).expect("pid"));
    nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGTERM).expect("kill -TERM");
    let stop = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(status) = child.try_wait().expect("wait") {
            return status.code();
        }
        if std::time::Instant::now() >= stop {
            let _ = child.kill();
            panic!("serve ignored SIGTERM");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// A word outside low · high · max, from the flag or the environment, refuses the seat before
/// the listener binds (the port stays occupied: reaching bind would be another refusal).
#[cfg(unix)]
#[test]
fn serve_refuses_an_unknown_word_before_its_listener_binds() {
    let room = tempfile::tempdir().expect("room");
    project(room.path());
    let recorder = Recorder::start();
    let canary = TcpListener::bind("127.0.0.1:0").expect("port canary");
    let address = canary.local_addr().expect("address").to_string();
    for (flag, ambient, word) in [
        (Some("maximum"), None, "maximum"),
        (None, Some("medium"), "medium"),
        (Some("MAX"), Some("max"), "MAX"),
    ] {
        let flags = with_word(&[], flag);
        let out = serve(room.path(), &address, &recorder, &flags, ambient)
            .output()
            .expect("serve refusal");
        let said = text(&out);
        assert_eq!(out.status.code(), Some(1), "{word}: {said}");
        assert!(said.contains("native authoring refused"), "{word}: {said}");
        let named = format!("`{word}` is not a reasoning effort");
        assert!(said.contains(&named), "{word}: {said}");
        assert!(
            !said.contains("listener failed"),
            "never reached bind: {said}"
        );
    }
    drop(canary);
    assert_eq!(recorder.counts(), (0, 0));
}

/// The flag outranks the environment and either reaches every round: the receipt of a caller's
/// round names the level the operator asked, refused before a byte leaves the override. The
/// control, with none named, sends the route's own bytes to the recorder.
#[cfg(unix)]
#[test]
fn serve_asks_the_flag_over_the_environment_on_every_round() {
    let room = tempfile::tempdir().expect("room");
    project(room.path());
    let fresh = json!({
        "compile_version": 2, "mode": "create", "cognition": "explicitProvider",
        "intent": NATIVE_INTENT,
    })
    .to_string();
    let post = format!(
        "POST /v1/compile HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nAuthorization: Bearer {TOKEN}\r\nContent-Length: {}\r\n\r\n{fresh}",
        fresh.len()
    );
    for (flag, ambient, asked) in [
        (None, None, None),
        (Some("max"), Some("medium"), Some("max")),
        (None, Some("max"), Some("max")),
        (Some("high"), Some("max"), Some("high")),
    ] {
        let recorder = Recorder::start();
        let probe = TcpListener::bind("127.0.0.1:0").expect("free port");
        let address = probe.local_addr().expect("address").to_string();
        drop(probe);
        let flags = with_word(&[], flag);
        let mut child = serve(room.path(), &address, &recorder, &flags, ambient)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("serve");
        healthy(&address, &mut child);
        let (status, body) = http(&address, &post);
        assert_eq!(status, 200, "{body}");
        let doc: Value = serde_json::from_str(&body).expect("document");
        let call = &doc["provenance"]["authoring"]["context"][0];
        let case = format!("{flag:?} over {ambient:?}");
        if let Some(word) = asked {
            assert_eq!(call["reasoning"]["configured"], word, "{case}: {doc}");
            assert_eq!(
                call["result"]["failure_kind"], "admission_refused",
                "{case}"
            );
            assert_eq!(recorder.counts(), (0, 0), "{case}: {doc}");
        } else {
            assert_eq!(call["reasoning"]["configured"], Value::Null, "{doc}");
            let bodies = recorder.bodies();
            assert_eq!(bodies.len(), 1, "{doc}");
            assert_eq!(bodies[0]["reasoning_effort"], "low", "{doc}");
            assert!(bodies[0].get("thinking").is_none(), "{doc}");
        }
        assert_eq!(terminate(&mut child), Some(0), "{case}");
    }
}
