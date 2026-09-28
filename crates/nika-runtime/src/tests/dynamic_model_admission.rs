// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! C4 · E13 F1/F2: a `model:` rendered at run time is admitted as the route it
//! renders to. The host binds the Run observer, compose gives its routes the
//! bounded client, and the injected wires count every post per client, so a
//! dispatch the admission missed cannot hide.

use std::sync::Arc;
use std::sync::Mutex;

use nika_kernel::http::{HttpError, HttpPostDyn, HttpRequest, HttpResponse, HttpStreamResponse};
use nika_kernel::secret::Secret;
use nika_kernel_mock::{MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor};
use nika_providers::{InferenceAdmission, ProviderRegistry, ProvidersConfig};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_invoke::InvokeVerb;

use super::*;

const FREE: &str = "openrouter/qwen/qwen3.8-27b:free";
const OPENROUTER: &str = "https://openrouter.ai/api/v1/chat/completions";
const DEEPSEEK: &str = "https://api.deepseek.com/v1/chat/completions";

/// A wire that answers each vendor with complete usage for the model it was
/// asked for; `completion` overrides the free route's output count.
struct Wire {
    single: bool,
    completion: u64,
    posts: Mutex<Vec<String>>,
}

impl Wire {
    fn new(single: bool, completion: u64) -> Arc<Self> {
        Arc::new(Self {
            single,
            completion,
            posts: Mutex::new(Vec::new()),
        })
    }
    fn posts(&self) -> Vec<String> {
        self.posts.lock().expect("posts").clone()
    }
}

impl HttpPostDyn for Wire {
    fn supports_single_attempt(&self) -> bool {
        self.single
    }
    async fn post(&self, req: HttpRequest) -> Result<HttpResponse, HttpError> {
        self.posts.lock().expect("posts").push(req.url.clone());
        let sent: serde_json::Value =
            serde_json::from_slice(req.body.as_ref().expect("a body")).expect("json");
        let usage = if req.url == OPENROUTER {
            serde_json::json!({"prompt_tokens": 10, "completion_tokens": self.completion,
                "total_tokens": 10 + self.completion, "cost": 0, "is_byok": false})
        } else {
            serde_json::json!({"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15,
                "prompt_cache_hit_tokens": 0, "prompt_cache_miss_tokens": 10})
        };
        let body = serde_json::json!({
            "id": "dynamic-fixture", "model": sent["model"],
            "choices": [{"message": {"content": "observed"}, "finish_reason": "stop"}],
            "usage": usage
        });
        Ok(HttpResponse::new(
            200,
            std::collections::BTreeMap::new(),
            body.to_string().into(),
            req.url,
        ))
    }
    async fn send_streaming(&self, _: HttpRequest) -> Result<HttpStreamResponse, HttpError> {
        panic!("no route here streams");
    }
}

struct Wires {
    normal: Arc<Wire>,
    bounded: Arc<Wire>,
    /// The `ollama` endpoint this test owns (CI 291): the B-5 run gate dials
    /// it for real, so the host's port 11434 never decides a result.
    local: Option<String>,
}

impl Wires {
    fn new(completion: u64) -> Self {
        Self {
            normal: Wire::new(false, completion),
            bounded: Wire::new(true, completion),
            local: None,
        }
    }
}

/// A local engine this test owns. It answers the B-5 liveness `GET /` with a
/// bare 404 (as alive as a 200) and nothing else; the injected wire still
/// carries every POST (the localhost-is-shared law).
#[allow(clippy::disallowed_methods)] // test seam — the probe's own worker pattern
fn owned_local_engine() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind"); // seam-bypass-ok: test owns the real loopback liveness endpoint
    let port = listener.local_addr().expect("addr").port();
    std::thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            use std::io::Write as _;
            let _ = stream.write_all(b"HTTP/1.0 404 Not Found\r\n\r\n");
        }
    });
    format!("http://127.0.0.1:{port}/v1/chat/completions")
}

/// A loopback endpoint nothing listens on: bound, then released.
fn released_local_endpoint() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind"); // seam-bypass-ok: test reserves an unused loopback endpoint for refusal
    let port = listener.local_addr().expect("addr").port();
    drop(listener);
    format!("http://127.0.0.1:{port}/v1/chat/completions")
}

/// Run `source` with `m` bound as the operator's `--var`, over the registry
/// compose builds for a Run observer (or none), with an optional cap.
async fn run_with(
    source: &str,
    m: &str,
    account: Option<&InferenceAdmission>,
    wires: &Wires,
    cap: Option<f64>,
) -> (RunOutcome, Vec<Event>) {
    let wf = nika_schema::parse(
        source,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    let mut config = ProvidersConfig::new()
        .with_key("openrouter", Secret::new("fixture"))
        .with_key("deepseek", Secret::new("fixture"))
        .with_key("mistral", Secret::new("fixture"));
    if let Some(local) = &wires.local {
        config = config.with_base_url("ollama", local.clone());
    }
    let mut registry = ProviderRegistry::new(Arc::clone(&wires.normal), config);
    let mut runtime_config = RuntimeConfig::default();
    if let Some(account) = account {
        registry =
            registry.with_inference_admission_http(account.clone(), Arc::clone(&wires.bounded));
        runtime_config.inference_admission = Some(account.clone());
    }
    let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new())),
        Arc::clone(&invoke),
        nika_verb_infer::InferVerb::new(Arc::new(registry), "mock/echo"),
        AgentVerb::new(
            Arc::new(MockProvider::new("mock")),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            "mock/echo",
        ),
        nika_clock::DeclaredClock::system(),
        runtime_config,
    )
    .with_var_overrides([("m".to_owned(), serde_json::Value::from(m))].into())
    .with_max_cost_usd(cap);
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("the run settles");
    (outcome, sink.into_events())
}

fn source(infer: &str) -> String {
    format!(
        "nika: dynamic\ninputs:\n  m: {{ type: string, required: true }}\npermits: {{}}\ntasks:\n  ask:\n    infer: {{ prompt: hello, model: \"${{{{ inputs.m }}}}\", max_tokens: 64{infer} }}\n"
    )
}

fn receipt(events: &[Event]) -> serde_json::Value {
    let terminal = events
        .iter()
        .rfind(|e| e.is_terminal())
        .expect("a terminal frame");
    let text = terminal
        .str_field("inference_admission")
        .expect("the terminal frame carries the receipt");
    serde_json::from_str(text).expect("the receipt is JSON")
}

fn field<'e>(frame: &'e Event, key: &str) -> Option<&'e FieldValue> {
    frame.fields.iter().find(|f| f.key == key).map(|f| &f.value)
}

fn frame(events: &[Event], kind: EventKind) -> &Event {
    events
        .iter()
        .find(|e| e.kind == kind)
        .unwrap_or_else(|| panic!("a {kind:?} frame"))
}

/// A run-time value naming the declared-free route is observed: one bounded
/// attempt, closed complete usage, a zero the receipt and the frames agree on.
#[tokio::test]
async fn a_run_time_free_route_is_observed_and_settles_closed_usage() {
    let wires = Wires::new(5);
    let account = InferenceAdmission::observe_run();
    let (outcome, events) = run_with(&source(""), FREE, Some(&account), &wires, None).await;
    assert!(outcome.ok, "{outcome:?}");
    assert_eq!(wires.bounded.posts(), [OPENROUTER]);
    assert!(wires.normal.posts().is_empty());
    let observed = receipt(&events);
    assert_eq!(observed["state"], "Open", "{observed}");
    assert_eq!(observed["unknown_calls"], 0, "{observed}");
    let attempts = observed["attempts"].as_array().expect("attempts");
    assert_eq!(attempts.len(), 1, "{observed}");
    assert_eq!(attempts[0]["model"], "qwen/qwen3.8-27b:free");
    assert_eq!(attempts[0]["estimated_nano_usd"], "0");
    let done = frame(&events, EventKind::TaskCompleted);
    assert_eq!(field(done, "cost_usd"), Some(&FieldValue::Float(0.0)));
    let terminal = events.iter().rfind(|e| e.is_terminal()).expect("terminal");
    assert_eq!(field(terminal, "priced_calls"), Some(&FieldValue::Int(1)));
    assert_eq!(field(terminal, "unpriced_calls"), Some(&FieldValue::Int(0)));
}

/// Vision on the rendered free route, and a route the Run's review calls
/// unknown-cost, both refuse before any byte on either client; the account
/// stays Open (no charge) and names the refusal.
#[tokio::test]
async fn unsupported_free_shapes_and_unknown_cost_routes_never_reach_the_wire() {
    let vision = ", vision: [{ source: url, url: 'https://example.invalid/a.png' }]";
    for (infer, m, why) in [
        (vision, FREE, "text-only"),
        ("", "mistral/mistral-small-latest", "unknown USD cost"),
        (
            "",
            "openrouter/google/gemma-4-26b-a4b-it:free",
            "unknown USD cost",
        ),
    ] {
        let wires = Wires::new(5);
        let account = InferenceAdmission::observe_run();
        let (outcome, events) = run_with(&source(infer), m, Some(&account), &wires, None).await;
        assert!(!outcome.ok, "{m}: {outcome:?}");
        assert!(
            wires.normal.posts().is_empty() && wires.bounded.posts().is_empty(),
            "{m}"
        );
        let observed = receipt(&events);
        assert_eq!(observed["state"], "Open", "{m}: {observed}");
        assert!(
            observed["attempts"]
                .as_array()
                .expect("attempts")
                .is_empty()
        );
        let refusal = observed["refusal"]
            .as_str()
            .expect("the refusal is on record");
        assert!(refusal.contains(why), "{m}: {refusal}");
    }
}

/// Paid, local and mock values keep today's transport and pricing under the
/// same observer: the normal client, protocol retries, no receipt attempt.
#[tokio::test]
async fn run_time_paid_local_and_mock_routes_keep_their_composition() {
    let local = owned_local_engine();
    for (m, url) in [
        ("deepseek/deepseek-v4-pro", Some(DEEPSEEK)),
        ("ollama/llama3.2", Some(local.as_str())),
        ("mock/echo", None),
    ] {
        let mut wires = Wires::new(5);
        wires.local = Some(local.clone());
        let account = InferenceAdmission::observe_run();
        let (outcome, events) = run_with(&source(""), m, Some(&account), &wires, None).await;
        assert!(outcome.ok, "{m}: {outcome:?}");
        assert_eq!(
            wires.normal.posts(),
            url.into_iter().collect::<Vec<_>>(),
            "{m}"
        );
        assert!(wires.bounded.posts().is_empty(), "{m}");
        let observed = receipt(&events);
        assert!(
            observed["attempts"]
                .as_array()
                .expect("attempts")
                .is_empty()
        );
        assert!(observed["refusal"].is_null(), "{m}: {observed}");
    }
    let wires = Wires::new(5);
    let account = InferenceAdmission::observe_run();
    let (_, events) = run_with(
        &source(""),
        "deepseek/deepseek-v4-pro",
        Some(&account),
        &wires,
        None,
    )
    .await;
    let done = frame(&events, EventKind::TaskCompleted);
    assert!(
        matches!(field(done, "cost_usd"), Some(FieldValue::Float(c)) if *c > 0.0),
        "the paid route keeps its tariff price"
    );
}

/// CI 291's control: only the owned engine let the local route through. With
/// nothing listening on its endpoint, the B-5 run gate still refuses before
/// any wire call, whatever the host runs on 11434, and the ledger stays
/// empty: a request never handed to the transport is known-not-sent.
#[tokio::test]
async fn a_run_time_local_route_with_no_server_is_refused_before_the_wire() {
    // A released port can be re-bound by another process before the probe
    // (the probe's own test meets the same race): try a few fresh ones.
    let mut refused = false;
    for _ in 0..5 {
        let mut wires = Wires::new(5);
        wires.local = Some(released_local_endpoint());
        let account = InferenceAdmission::observe_run();
        let (outcome, events) =
            run_with(&source(""), "ollama/llama3.2", Some(&account), &wires, None).await;
        let error = outcome.records.get("ask").and_then(|r| r.error.as_ref());
        let Some(error) = error.filter(|e| e.message.contains("nothing answers there")) else {
            continue;
        };
        assert!(!outcome.ok);
        assert!(
            error.message.contains("BEFORE any wire call"),
            "{}",
            error.message
        );
        assert!(
            wires.normal.posts().is_empty(),
            "the gate stops before the wire"
        );
        assert!(wires.bounded.posts().is_empty());
        let observed = receipt(&events);
        assert!(
            observed["attempts"]
                .as_array()
                .expect("attempts")
                .is_empty(),
            "{observed}"
        );
        let terminal = events.iter().rfind(|e| e.is_terminal()).expect("terminal");
        assert_eq!(field(terminal, "unpriced_calls"), Some(&FieldValue::Int(0)));
        refused = true;
        break;
    }
    assert!(
        refused,
        "a port nothing listens on is refused by the run gate"
    );
}

/// E13 F2 · complete usage above the requested bound: the account holds an
/// unknown charge, so no frame may say priced, and a retry never re-sends.
#[tokio::test]
async fn an_over_bound_free_reply_is_unknown_on_every_frame_and_never_resent() {
    let wires = Wires::new(600);
    let account = InferenceAdmission::observe_run();
    let retried = source("").replace("    infer:", "    retry: { max_attempts: 3 }\n    infer:");
    let (outcome, events) = run_with(&retried, FREE, Some(&account), &wires, None).await;
    assert!(!outcome.ok, "{outcome:?}");
    assert_eq!(wires.bounded.posts(), [OPENROUTER], "no second dispatch");
    let observed = receipt(&events);
    assert_eq!(observed["state"], "Uncertain", "{observed}");
    assert_eq!(observed["unknown_calls"], 1, "{observed}");
    let failed = frame(&events, EventKind::TaskFailed);
    assert_eq!(field(failed, "cost_usd"), None, "never a priced zero");
    let terminal = events.iter().rfind(|e| e.is_terminal()).expect("terminal");
    assert_eq!(field(terminal, "priced_calls"), Some(&FieldValue::Int(0)));
    assert!(matches!(field(terminal, "unpriced_calls"), Some(FieldValue::Int(n)) if *n >= 1));
    assert_ne!(
        field(terminal, "cost_qualifier"),
        Some(&FieldValue::String("priced".into()))
    );
}

/// A static declared-free lane beside a run-time paid value: each route
/// keeps its own client; a numeric zero cap still admits the zero tariff.
#[tokio::test]
async fn a_mixed_run_keeps_each_route_and_a_zero_cap_admits_the_zero_tariff() {
    let mixed = format!(
        "{}  free:\n    infer: {{ prompt: hello, model: {FREE}, max_tokens: 64 }}\n",
        source("")
    );
    let wires = Wires::new(5);
    let account = InferenceAdmission::observe_run();
    let (outcome, _) = run_with(
        &mixed,
        "deepseek/deepseek-v4-pro",
        Some(&account),
        &wires,
        None,
    )
    .await;
    assert!(outcome.ok, "{outcome:?}");
    assert_eq!(wires.normal.posts(), [DEEPSEEK]);
    assert_eq!(wires.bounded.posts(), [OPENROUTER]);
    let wires = Wires::new(5);
    let account = InferenceAdmission::observe_run();
    let (outcome, _) = run_with(&source(""), FREE, Some(&account), &wires, Some(0.0)).await;
    assert!(
        outcome.ok,
        "numeric zero is not a closed account: {outcome:?}"
    );
    assert_eq!(wires.bounded.posts(), [OPENROUTER]);
}
