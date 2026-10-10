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
/// asked for; `completion` is the output count on every route.
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
            serde_json::json!({"prompt_tokens": 10, "completion_tokens": self.completion,
                "total_tokens": 10 + self.completion,
                "prompt_cache_hit_tokens": 0, "prompt_cache_miss_tokens": 10})
        };
        let body = if req.url.ends_with("/api/chat") {
            serde_json::json!({
                "model": sent["model"], "message": {"role": "assistant", "content": "observed"},
                "done": true, "done_reason": "stop",
                "prompt_eval_count": 10, "eval_count": self.completion
            })
        } else {
            serde_json::json!({
                "id": "dynamic-fixture", "model": sent["model"],
                "choices": [{"message": {"content": "observed"}, "finish_reason": "stop"}],
                "usage": usage
            })
        };
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
    format!("http://127.0.0.1:{port}/api/chat")
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
    let (outcome, events) = launch(source, Some(m), account, wires, cap).await;
    (outcome.expect("the run settles"), events)
}

/// As [`run_with`], keeping a launch refusal: `m` is bound only when given,
/// and one `exec` result is queued for a prior step.
async fn launch(
    source: &str,
    m: Option<&str>,
    account: Option<&InferenceAdmission>,
    wires: &Wires,
    cap: Option<f64>,
) -> (Result<RunOutcome, RuntimeError>, Vec<Event>) {
    launch_with(source, m, None, account, wires, cap).await
}

/// As [`launch`], with the operator's `--model` envelope override.
async fn launch_with(
    source: &str,
    m: Option<&str>,
    model_override: Option<&str>,
    account: Option<&InferenceAdmission>,
    wires: &Wires,
    cap: Option<f64>,
) -> (Result<RunOutcome, RuntimeError>, Vec<Event>) {
    let shell = MockShell::new().enqueue_ok("noted");
    let runtime = runtime_on(shell, (m, model_override), account, wires, cap);
    run_on(&runtime, source).await
}

/// The runtime every launch here builds: the wires' registry (the bounded
/// client when an account observes the Run), `shell` for `exec`, the
/// operator's `--var m` and `--model`, and the cap.
fn runtime_on(
    shell: MockShell,
    (m, model_override): (Option<&str>, Option<&str>),
    account: Option<&InferenceAdmission>,
    wires: &Wires,
    cap: Option<f64>,
) -> MockRuntime {
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
    Runtime::new(
        ExecVerb::new(Arc::new(shell)),
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
    .with_var_overrides(
        m.map(|m| ("m".to_owned(), serde_json::Value::from(m)))
            .into_iter()
            .collect(),
    )
    .with_model_override(model_override.map(str::to_owned))
    .with_max_cost_usd(cap)
}

type MockRuntime = Runtime<
    MockShell,
    MockToolExecutor,
    Wire,
    MockProvider,
    MockToolDefinitionProvider,
    nika_clock::DeclaredClock,
>;

async fn run_on(
    runtime: &MockRuntime,
    source: &str,
) -> (Result<RunOutcome, RuntimeError>, Vec<Event>) {
    let wf = nika_schema::parse(
        source,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime.run(&wf, &report, &mut stamper, &mut sink).await;
    (outcome, sink.into_events())
}

/// B9 · E17-g02 twins: the same workflow with its paid `model:` written
/// literally, or rendered from an input the operator's `--var` decides.
/// A prior `exec` step stands for any earlier workflow effect.
fn twin(model: &str) -> String {
    format!(
        "nika: twin\ninputs:\n  m: {{ type: string, required: false, default: \"mock/echo\" }}\npermits: {{ exec: [\"true\"] }}\ntasks:\n  note:\n    exec: {{ command: [\"true\"] }}\n  ask:\n    after: {{ note: success }}\n    infer: {{ prompt: hello, model: \"{model}\", max_tokens: 512 }}\n"
    )
}

/// A [`twin`] whose `ask` declares `max_tokens`.
fn capped(model: &str, max_tokens: u32) -> String {
    twin(model).replace("max_tokens: 512", &format!("max_tokens: {max_tokens}"))
}

/// B9 · every source that decides a `model:` before any effect meets the
/// floor under a zero cap: a declared default, a const, and a task's own
/// rendered model under the operator's `--model` (the task keeps winning).
/// The operator's `--model` replacing a rendered envelope is the control:
/// the envelope's value never seats, so the run proceeds on `mock/echo`.
#[tokio::test]
async fn every_pre_effect_model_source_meets_the_floor() {
    const PAID: &str = "deepseek/deepseek-v4-pro";
    let default = twin("${{ inputs.m }}").replace(
        "default: \"mock/echo\"",
        "default: \"deepseek/deepseek-v4-pro\"",
    );
    let constant = twin("${{ const.m }}").replace(
        "permits:",
        "const:\n  m: \"deepseek/deepseek-v4-pro\"\npermits:",
    );
    let envelope = twin("mock/echo")
        .replace("permits:", "model: \"${{ inputs.m }}\"\npermits:")
        .replace("model: \"mock/echo\", ", "");
    for (name, source, m, model_override, refused) in [
        ("default", default.as_str(), None, None, true),
        ("const", constant.as_str(), None, None, true),
        (
            "task over --model",
            &twin("${{ inputs.m }}"),
            Some(PAID),
            Some("mock/echo"),
            true,
        ),
        (
            "--model over envelope",
            envelope.as_str(),
            Some(PAID),
            Some("mock/echo"),
            false,
        ),
    ] {
        let wires = Wires::new(5);
        let (result, events) =
            launch_with(source, m, model_override, None, &wires, Some(0.0)).await;
        if refused {
            assert!(
                matches!(result, Err(RuntimeError::BudgetFloor { .. })),
                "{name}: {result:?}"
            );
            assert!(
                events.is_empty() && wires.normal.posts().is_empty(),
                "{name}"
            );
        } else {
            let outcome = result.expect("the run settles");
            assert!(outcome.ok, "{name}: {outcome:?}");
            assert!(wires.normal.posts().is_empty(), "{name}: mock/echo seats");
        }
    }
}

/// B9 · E17-g02: a known-paid route the inputs decide meets its literal
/// twin's budget floor ($0.002028 at 512 output tokens) before the prologue.
/// Below it both refuse with zero events and zero posts; above it, or with
/// no cap, both run to the same business output.
#[tokio::test]
async fn a_rendered_paid_route_meets_its_literal_twins_budget_floor() {
    const PAID: &str = "deepseek/deepseek-v4-pro";
    for (cap, runs) in [
        (Some(0.0), false),
        (Some(0.001), false),
        (Some(0.01), true),
        (None, true),
    ] {
        for (source, m) in [(twin(PAID), None), (twin("${{ inputs.m }}"), Some(PAID))] {
            let wires = Wires::new(5);
            let (result, events) = launch(&source, m, None, &wires, cap).await;
            let twin = if m.is_some() { "rendered" } else { "literal" };
            if runs {
                let outcome = result.expect("the run settles");
                assert!(outcome.ok, "{twin} {cap:?}: {outcome:?}");
                assert_eq!(outcome.records["ask"].output, "observed", "{twin} {cap:?}");
                assert_eq!(wires.normal.posts(), [DEEPSEEK], "{twin} {cap:?}");
            } else {
                assert!(
                    matches!(result, Err(RuntimeError::BudgetFloor { .. })),
                    "{twin} {cap:?}: {result:?}"
                );
                assert!(events.is_empty(), "{twin} {cap:?}: no prologue, no note");
                assert!(
                    wires.normal.posts().is_empty(),
                    "{twin} {cap:?}: no request"
                );
            }
        }
    }
}

/// B9 · the MODELS rung `nika check` applies to a literal seat judges the
/// seat a binding renders, at the embedder door, before the prologue: a
/// reasoning seat under its cap floor, a cap above the seat's output window,
/// an id the resolver cannot name. With no cap, or one above the floor, both
/// twins refuse NIKA-1707 with the rung's own words, with zero events (no
/// note) and zero posts. (Under a cap the unnameable id meets the floor's
/// #1368 arm first.) Mock at the same tiny cap runs, both spellings.
#[tokio::test]
async fn a_rendered_seat_meets_its_literal_twins_models_rung() {
    for (seat, max_tokens, law, caps) in [
        (
            "deepseek/deepseek-v4-pro",
            64,
            "too small for reasoning seat",
            &[None, Some(1.0)][..],
        ),
        (
            "openai/gpt-5.2",
            200_000,
            "can emit in one answer",
            &[None, Some(100.0)][..],
        ),
        ("acme/model-x", 64, "`acme/model-x` · ", &[None][..]),
    ] {
        for &cap in caps {
            for (source, m) in [
                (capped(seat, max_tokens), None),
                (capped("${{ inputs.m }}", max_tokens), Some(seat)),
            ] {
                let wires = Wires::new(5);
                let (result, events) = launch(&source, m, None, &wires, cap).await;
                let twin = if m.is_some() { "rendered" } else { "literal" };
                let Err(RuntimeError::ReportMismatch { detail }) = &result else {
                    panic!("{seat} {twin} {cap:?}: {result:?}");
                };
                assert!(detail.contains(law) && detail.contains(seat), "{detail}");
                assert!(events.is_empty(), "{seat} {twin} {cap:?}: no note");
                assert!(wires.normal.posts().is_empty(), "{seat} {twin} {cap:?}");
            }
        }
    }
    for (source, m) in [
        (capped("mock/echo", 64), None),
        (capped("${{ inputs.m }}", 64), Some("mock/echo")),
    ] {
        let wires = Wires::new(5);
        let (result, _) = launch(&source, m, None, &wires, Some(0.0)).await;
        let outcome = result.expect("the mock runs");
        assert!(outcome.ok, "{outcome:?}");
        assert_eq!(outcome.records["ask"].output, "mock(echo) · hello");
    }
}

fn source(infer: &str) -> String {
    format!(
        "nika: dynamic\ninputs:\n  m: {{ type: string, required: true }}\npermits: {{}}\ntasks:\n  ask:\n    infer: {{ prompt: hello, model: \"${{{{ inputs.m }}}}\", max_tokens: 64{infer} }}\n"
    )
}

/// [`source`] at the reasoning seat's cap floor (256): a paid reasoning route
/// under it is the MODELS rung's refusal, rendered or literal (B9).
fn reasoning_source() -> String {
    source("").replace("max_tokens: 64", "max_tokens: 256")
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
/// same observer: the normal client (one attempt per request), no receipt attempt.
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
        let (outcome, events) =
            run_with(&reasoning_source(), m, Some(&account), &wires, None).await;
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
        &reasoning_source(),
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
    assert_eq!(
        field(failed, "cost_unpriced"),
        Some(&FieldValue::String("usage_rejected".into())),
        "E17-F4: the tariff exists; the reply broke its bound"
    );
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
        reasoning_source()
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

// ─── B9 phase C · the pre-send guard for a seat only the run decides ─────

const PAID_ROUTE: &str = "deepseek/deepseek-v4-pro";

/// `pick` (an `exec` whose output names the model) runs first; `ask` then
/// renders its `model:` from that output, so only the run decides it.
fn picked(max_tokens: u32, ask: &str) -> String {
    format!(
        "nika: picked\npermits: {{ exec: [\"true\"] }}\ntasks:\n  pick:\n    exec: {{ command: [\"true\"] }}\n  ask:\n{ask}    with: {{ m: \"${{{{ tasks.pick.output }}}}\" }}\n    infer: {{ prompt: hello, model: \"${{{{ with.m }}}}\", max_tokens: {max_tokens} }}\n"
    )
}

/// Run `source` with `pick` answering `seat`, under `cap`.
async fn launch_picked(
    source: &str,
    seat: &str,
    account: Option<&InferenceAdmission>,
    wires: &Wires,
    cap: Option<f64>,
) -> (RunOutcome, Vec<Event>) {
    let shell = MockShell::new().enqueue_ok(seat);
    let runtime = runtime_on(shell, (None, None), account, wires, cap);
    let (result, events) = run_on(&runtime, source).await;
    (
        result.expect("launch cannot judge a run-decided seat"),
        events,
    )
}

/// The one-call floor launch would price for `seat` at `max_tokens`.
fn one_call_floor(seat: &str, max_tokens: u32) -> f64 {
    let source = format!(
        "nika: one\ntasks:\n  ask:\n    infer: {{ prompt: hello, model: \"{seat}\", max_tokens: {max_tokens} }}\n"
    );
    let wf = nika_schema::parse(
        &source,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    nika_check::check(&wf).cost.min_path_total_usd
}

/// The refused task's record: failed on `code`, one attempt, nothing sent.
fn refused<'o>(outcome: &'o RunOutcome, task: &str, code: &str) -> &'o str {
    let record = &outcome.records[task];
    let error = record.error.as_ref().expect("the refusal is on record");
    assert_eq!(error.code, code, "{}", error.message);
    assert!(!error.transient, "{}", error.message);
    assert_eq!(record.attempts, Some(1), "never retried");
    assert!(
        error
            .message
            .contains("refused before the provider request"),
        "{}",
        error.message
    );
    &error.message
}

/// A paid route only the run decides meets the floor at dispatch: under a
/// cap below it the request is refused before any byte (NIKA-1704, one
/// attempt under an authored `retry:`), while `pick`, which ran first,
/// stands. Above the floor, or with no cap, it is sent.
#[tokio::test]
async fn a_task_output_route_meets_its_floor_before_its_request() {
    let retried = "    retry: { max_attempts: 3 }\n";
    for (cap, sent) in [
        (Some(0.0), false),
        (Some(0.001), false),
        (Some(0.01), true),
        (None, true),
    ] {
        let wires = Wires::new(5);
        let (outcome, events) =
            launch_picked(&picked(512, retried), PAID_ROUTE, None, &wires, cap).await;
        assert_eq!(
            outcome.records["pick"].status,
            TaskStatus::Success,
            "{cap:?}"
        );
        if sent {
            assert!(outcome.ok, "{cap:?}: {outcome:?}");
            assert_eq!(outcome.records["ask"].output, "observed");
            assert_eq!(wires.normal.posts(), [DEEPSEEK], "{cap:?}");
            continue;
        }
        assert!(!outcome.ok);
        let why = refused(&outcome, "ask", "NIKA-1704");
        assert!(why.contains("never a reservation"), "{why}");
        assert!(wires.normal.posts().is_empty() && wires.bounded.posts().is_empty());
        let terminal = events.iter().rfind(|e| e.is_terminal()).expect("terminal");
        assert_eq!(field(terminal, "priced_calls"), Some(&FieldValue::Int(0)));
        assert_eq!(field(terminal, "unpriced_calls"), Some(&FieldValue::Int(0)));
    }
}

/// The MODELS rung judges a seat only the run decides at dispatch: a
/// reasoning seat under its cap floor (NIKA-INFER-004, the failure it
/// prevents) and a cap above a seat's output window (NIKA-INFER-001) are
/// refused before any request, with the checker's own words; `pick` stands.
/// The refusal is never replayed, even when `on_codes:` names its code.
#[tokio::test]
async fn a_task_output_seat_the_models_rung_refuses_never_reaches_the_wire() {
    let retried = "    retry: { max_attempts: 3, on_codes: [NIKA-INFER-004, NIKA-INFER-001] }\n";
    for (seat, max_tokens, code, law) in [
        (
            PAID_ROUTE,
            64,
            "NIKA-INFER-004",
            "too small for reasoning seat",
        ),
        (
            "openai/gpt-5.2",
            200_000,
            "NIKA-INFER-001",
            "can emit in one answer",
        ),
    ] {
        let wires = Wires::new(5);
        let source = picked(max_tokens, retried);
        let (outcome, _) = launch_picked(&source, seat, None, &wires, None).await;
        assert_eq!(outcome.records["pick"].status, TaskStatus::Success);
        let why = refused(&outcome, "ask", code);
        assert!(why.contains(law) && why.contains(seat), "{why}");
        assert!(wires.normal.posts().is_empty(), "{seat}");
    }
}

/// The one-call projection keeps the task's own declarations: a `thinking:`
/// block on a seat the catalog knows cannot reason is the thinking law's
/// refusal at dispatch (NIKA-INFER-001), judged on the rendered seat; the
/// same block on a reasoning seat at an ample cap is sent.
#[tokio::test]
async fn the_guard_judges_the_thinking_the_task_declares() {
    let thinking = picked(512, "").replace(
        "max_tokens: 512 }",
        "max_tokens: 512, thinking: { enabled: true } }",
    );
    let wires = Wires::new(5);
    let seat = "deepseek/deepseek-chat";
    let (outcome, _) = launch_picked(&thinking, seat, None, &wires, None).await;
    assert_eq!(outcome.records["pick"].status, TaskStatus::Success);
    let why = refused(&outcome, "ask", "NIKA-INFER-001");
    assert!(why.contains("cannot reason") && why.contains(seat), "{why}");
    assert!(wires.normal.posts().is_empty());
    let wires = Wires::new(5);
    let (outcome, _) = launch_picked(&thinking, PAID_ROUTE, None, &wires, None).await;
    assert!(
        outcome.ok,
        "a reasoning seat keeps its thinking: {outcome:?}"
    );
    assert_eq!(wires.normal.posts(), [DEEPSEEK]);
}

/// Mock and local routes only the run decides run under a zero cap: the
/// mock is a proven zero, a local seat is unmetered (never free, never the
/// cap's to refuse).
#[tokio::test]
async fn local_and_mock_task_output_routes_run_under_a_zero_cap() {
    let local = owned_local_engine();
    for (seat, url, answer) in [
        ("mock/echo", None, "mock(echo) · hello"),
        ("ollama/llama3.2", Some(local.as_str()), "observed"),
    ] {
        let mut wires = Wires::new(5);
        wires.local = Some(local.clone());
        let (outcome, _) = launch_picked(&picked(512, ""), seat, None, &wires, Some(0.0)).await;
        assert!(outcome.ok, "{seat}: {outcome:?}");
        assert_eq!(outcome.records["ask"].output, answer, "{seat}");
        assert_eq!(
            wires.normal.posts(),
            url.into_iter().collect::<Vec<_>>(),
            "{seat}"
        );
    }
}

/// The item table's statuses, in input order.
fn item_statuses(events: &[Event]) -> Vec<String> {
    let text = events
        .iter()
        .filter(|e| matches!(e.kind, EventKind::TaskFailed | EventKind::TaskCompleted))
        .find_map(|e| e.str_field("items"))
        .expect("an inline item table");
    let rows: Vec<serde_json::Value> = serde_json::from_str(text).expect("rows");
    rows.iter()
        .map(|row| row["status"].as_str().expect("a status").to_owned())
        .collect()
}

/// An item's route is judged per item, whatever the order: the paid item
/// under a cap below its floor is refused before its request, the mock
/// item beside it runs.
#[tokio::test]
async fn each_item_route_is_judged_when_it_dispatches() {
    for (items, statuses) in [
        (["deepseek/deepseek-v4-pro", "mock/echo"], ["failed", "ok"]),
        (["mock/echo", "deepseek/deepseek-v4-pro"], ["ok", "failed"]),
    ] {
        let source = format!(
            "nika: items\npermits: {{}}\ntasks:\n  ask:\n    for_each: {{ items: [\"{}\", \"{}\"], max_parallel: 1, fail_fast: false }}\n    infer: {{ prompt: hello, model: \"${{{{ item }}}}\", max_tokens: 512 }}\n",
            items[0], items[1]
        );
        let wires = Wires::new(5);
        let runtime = runtime_on(MockShell::new(), (None, None), None, &wires, Some(0.001));
        let (result, events) = run_on(&runtime, &source).await;
        let outcome = result.expect("launch cannot judge an item seat");
        assert_eq!(item_statuses(&events), statuses, "{items:?}");
        let error = outcome.records["ask"]
            .error
            .as_ref()
            .expect("the item failure");
        assert_eq!(error.code, "NIKA-1704", "{}", error.message);
        assert!(wires.normal.posts().is_empty(), "{items:?}");
    }
}

/// Earlier KNOWN spend narrows the snapshot a later run-decided route meets:
/// under the same cap the route alone is sent, and after a paid literal
/// task spent it is refused. The earlier request and its price stand.
#[tokio::test]
async fn earlier_known_spend_narrows_what_a_later_route_meets() {
    let first = "  first:\n    infer: { prompt: first, model: \"deepseek/deepseek-v4-pro\", max_tokens: 256 }\n";
    let spent = {
        let wires = Wires::new(5);
        let runtime = runtime_on(MockShell::new(), (None, None), None, &wires, None);
        let (result, _) = run_on(&runtime, &format!("nika: first\ntasks:\n{first}")).await;
        let outcome = result.expect("settles");
        outcome.total_cost_usd.expect("the paid call is priced")
    };
    let cap = one_call_floor(PAID_ROUTE, 512) + spent / 2.0;
    let alone = Wires::new(5);
    let (outcome, _) = launch_picked(&picked(512, ""), PAID_ROUTE, None, &alone, Some(cap)).await;
    assert!(outcome.ok, "the route alone fits: {outcome:?}");
    assert_eq!(alone.normal.posts(), [DEEPSEEK]);
    let after = picked(512, "    after: { first: success }\n")
        .replace("tasks:\n", &format!("tasks:\n{first}"));
    let wires = Wires::new(5);
    let (outcome, _) = launch_picked(&after, PAID_ROUTE, None, &wires, Some(cap)).await;
    assert_eq!(outcome.records["first"].status, TaskStatus::Success);
    assert_eq!(
        outcome.total_cost_usd,
        Some(spent),
        "the earlier price stands"
    );
    refused(&outcome, "ask", "NIKA-1704");
    assert_eq!(wires.normal.posts(), [DEEPSEEK], "only the earlier request");
}

/// Precedence: a task's own run-decided seat wins over a rendered envelope
/// and over `--model`, so the guard judges THAT seat; an envelope's paid
/// literal never seats a task that names its own route.
#[tokio::test]
async fn the_seat_a_task_names_is_the_seat_the_guard_judges() {
    let envelope = |model: &str| {
        picked(512, "").replace(
            "permits:",
            &format!(
                "model: \"{model}\"\ninputs:\n  e: {{ type: string, required: false, default: \"mock/echo\" }}\npermits:"
            ),
        )
    };
    for model_override in [None, Some("mock/echo")] {
        let wires = Wires::new(5);
        let shell = MockShell::new().enqueue_ok(PAID_ROUTE);
        let runtime = runtime_on(shell, (None, model_override), None, &wires, Some(0.001));
        let (result, _) = run_on(&runtime, &envelope("${{ inputs.e }}")).await;
        let outcome = result.expect("the envelope renders to mock at launch");
        refused(&outcome, "ask", "NIKA-1704");
        assert!(wires.normal.posts().is_empty(), "{model_override:?}");
    }
    let wires = Wires::new(5);
    let (outcome, _) =
        launch_picked(&envelope(PAID_ROUTE), "mock/echo", None, &wires, Some(0.0)).await;
    assert!(outcome.ok, "{outcome:?}");
    assert_eq!(outcome.records["ask"].output, "mock(echo) · hello");
    assert!(wires.normal.posts().is_empty());
}

/// An unknown charge is the observed account's to close: after an
/// over-bound free reply (`usage_rejected`), a run-decided free route is
/// refused by the account before any byte. The guard's snapshot never saw
/// that charge (it has no USD), so a paid run-decided route that fits the
/// cap is still sent: the snapshot is an upper bound, never a reservation.
#[tokio::test]
async fn an_unknown_charge_closes_the_account_and_leaves_the_snapshot_an_upper_bound() {
    let first =
        format!("  first:\n    infer: {{ prompt: first, model: {FREE}, max_tokens: 64 }}\n");
    let source = picked(512, "")
        .replace("tasks:\n", &format!("tasks:\n{first}"))
        .replace("  pick:\n", "  pick:\n    after: { first: failure }\n");
    let wires = Wires::new(600);
    let account = InferenceAdmission::observe_run();
    let (outcome, events) = launch_picked(&source, FREE, Some(&account), &wires, None).await;
    assert!(!outcome.ok);
    assert_eq!(
        wires.bounded.posts(),
        [OPENROUTER],
        "the closed account sends nothing more"
    );
    assert!(outcome.records["ask"].error.is_some(), "{outcome:?}");
    let observed = receipt(&events);
    assert_eq!(observed["unknown_calls"], 1, "{observed}");
    let cap = one_call_floor(PAID_ROUTE, 512) * 1.5;
    let wires = Wires::new(600);
    let account = InferenceAdmission::observe_run();
    let (outcome, _) = launch_picked(&source, PAID_ROUTE, Some(&account), &wires, Some(cap)).await;
    assert_eq!(wires.bounded.posts(), [OPENROUTER]);
    assert_eq!(
        wires.normal.posts(),
        [DEEPSEEK],
        "the snapshot never counted the unknown charge"
    );
    assert_eq!(outcome.records["ask"].status, TaskStatus::Success);
}

/// Internal retries belong to the verb and are counted where they happen: a
/// structured task whose seat only the run decides passes the guard ONCE (its
/// floor is one call's, as launch's is), and each schema re-ask is its own
/// priced request on the ledger. The guard never claims to bound them.
#[tokio::test]
async fn schema_reasks_after_the_guard_are_each_on_the_ledger() {
    let schema = picked(512, "").replace(
        "max_tokens: 512 }",
        "max_tokens: 512, schema: { type: object, required: [x] } }",
    );
    let one = {
        let wires = Wires::new(5);
        let (outcome, _) = launch_picked(&picked(512, ""), PAID_ROUTE, None, &wires, None).await;
        outcome.total_cost_usd.expect("one priced request")
    };
    let wires = Wires::new(5);
    let cap = one_call_floor(PAID_ROUTE, 512) * 1.1;
    let (outcome, _) = launch_picked(&schema, PAID_ROUTE, None, &wires, Some(cap)).await;
    let error = outcome.records["ask"]
        .error
        .as_ref()
        .expect("no reply fits");
    assert_eq!(error.code, "NIKA-INFER-002", "{}", error.message);
    assert_eq!(
        wires.normal.posts(),
        [DEEPSEEK, DEEPSEEK, DEEPSEEK],
        "the ask and two schema re-asks, each sent"
    );
    let spent = outcome.total_cost_usd.expect("each re-ask is priced");
    assert!(
        (spent - 3.0 * one).abs() < 1e-12,
        "three priced requests: {spent} vs {one}"
    );
}

/// The cleanup lane's journal: each `on_finally` decision with its reason.
fn cleanup_journal(events: &[Event]) -> Vec<(String, String)> {
    events
        .iter()
        .filter(|e| {
            e.kind == EventKind::PermitChecked && e.str_field("plane") == Some("on_finally")
        })
        .map(|e| {
            let text = |key| e.str_field(key).unwrap_or_default().to_owned();
            (text("decision"), text("why"))
        })
        .collect()
}

/// Whether the cleanup lane journaled a refusal before the provider request.
fn refused_in_cleanup(events: &[Event], code: &str) -> bool {
    cleanup_journal(events).iter().any(|(decision, why)| {
        decision == "failure"
            && why.contains(code)
            && why.contains("refused before the provider request")
    })
}

/// The cleanup lane rides the run's authority (B11). This replaces B9's
/// deliberate baseline pin, under which this very cleanup was sent unguarded
/// and unbudgeted under a zero cap. An `unwind` cleanup whose seat only the run
/// decides now meets the main lane's pre-send guard, and what it sends is on
/// the run's own ledger. Under a cap below the call's floor the request is
/// refused before any byte and the refusal is journaled on the cleanup lane
/// (best-effort: the run still succeeds); above the floor, or with no cap, it
/// is sent and priced once.
#[tokio::test]
async fn the_cleanup_lane_meets_the_guard_and_the_ledger() {
    let source = "nika: cleanup\npermits: { exec: [\"true\"] }\ntasks:\n  work:\n    exec: { command: [\"true\"] }\n  tidy:\n    after: { work: unwind }\n    with: { m: \"${{ tasks.work.output }}\" }\n    infer: { prompt: hello, model: \"${{ with.m }}\", max_tokens: 512 }\n";
    let floor = one_call_floor(PAID_ROUTE, 512);
    for (cap, sent) in [
        (Some(0.0), false),
        (Some(floor / 2.0), false),
        (Some(0.01), true),
        (None, true),
    ] {
        let wires = Wires::new(5);
        let shell = MockShell::new().enqueue_ok(PAID_ROUTE);
        let runtime = runtime_on(shell, (None, None), None, &wires, cap);
        let (result, events) = run_on(&runtime, source).await;
        let outcome = result.expect("the run starts");
        assert!(outcome.ok, "cap {cap:?}: a cleanup never fails its run");
        assert_eq!(outcome.records["work"].status, TaskStatus::Success);
        let counts = (outcome.priced_calls, outcome.unpriced_calls);
        if sent {
            assert_eq!(wires.normal.posts(), [DEEPSEEK], "cap {cap:?}");
            assert_eq!(counts, (1, 0), "cap {cap:?}: priced once");
            assert!(
                outcome.total_cost_usd.is_some_and(|usd| usd > 0.0),
                "cap {cap:?}: the cleanup's spend is the run's: {outcome:?}"
            );
            continue;
        }
        assert!(wires.normal.posts().is_empty(), "cap {cap:?}: no byte");
        assert_eq!(counts, (0, 0), "cap {cap:?}: nothing was sent");
        assert!(
            refused_in_cleanup(&events, "NIKA-1704"),
            "cap {cap:?}: {:?}",
            cleanup_journal(&events)
        );
    }
}

/// Law 6 in the cleanup lane (B11): a cleanup child runs under the run's
/// remaining budget like any other call (it ran with none before), so under a
/// cap below the child's floor its run-decided route is refused before its
/// request; above it, the child's spend is the parent's.
#[tokio::test]
async fn a_cleanup_child_meets_the_budget_its_parent_hands_down() {
    let parent = "nika: parent\ntasks:\n  work:\n    infer: { prompt: hi, model: mock/echo, max_tokens: 8 }\n  tidy:\n    after: { work: unwind }\n    invoke:\n      workflow: \"./child.nika\"\n";
    for (cap, sent) in [(0.001, false), (0.01, true)] {
        let wires = Wires::new(5);
        let child = ChildOnWires {
            normal: Arc::clone(&wires.normal),
            bounded: Arc::clone(&wires.bounded),
            seat: PAID_ROUTE.to_owned(),
        };
        let runtime = runtime_on(MockShell::new(), (None, None), None, &wires, Some(cap))
            .with_child_runner(Arc::new(child));
        let (result, events) = run_on(&runtime, parent).await;
        let outcome = result.expect("the parent starts");
        assert!(outcome.ok, "a cleanup never fails its run: {outcome:?}");
        if sent {
            assert_eq!(wires.normal.posts(), [DEEPSEEK]);
            assert!(
                outcome.total_cost_usd.is_some_and(|usd| usd > 0.0),
                "the cleanup child's spend is the parent's: {outcome:?}"
            );
            continue;
        }
        assert!(
            wires.normal.posts().is_empty(),
            "the cleanup child sent nothing"
        );
        assert!(
            cleanup_journal(&events)
                .iter()
                .any(|(decision, why)| decision == "failure"
                    && why.contains("refused before the provider request")),
            "{:?}",
            cleanup_journal(&events)
        );
    }
}

/// A child run on the same wires, launched with the parent's remaining
/// budget (law 6): its `pick` answers `seat`.
struct ChildOnWires {
    normal: Arc<Wire>,
    bounded: Arc<Wire>,
    seat: String,
}

type ChildRun<'a> = std::pin::Pin<
    Box<
        dyn std::future::Future<
                Output = Result<crate::child::ChildOutcome, crate::child::ChildRunRefusal>,
            > + 'a,
    >,
>;

impl crate::child::ChildRunner for ChildOnWires {
    fn run_child(&self, call: crate::child::ChildCall) -> ChildRun<'_> {
        Box::pin(async move {
            let wires = Wires {
                normal: Arc::clone(&self.normal),
                bounded: Arc::clone(&self.bounded),
                local: None,
            };
            let shell = MockShell::new().enqueue_ok(&self.seat);
            let runtime = runtime_on(shell, (None, None), None, &wires, call.remaining_budget_usd);
            let (result, _) = run_on(&runtime, &picked(512, "")).await;
            let outcome = result.map_err(|err| crate::child::ChildRunRefusal {
                code: "NIKA-COMP-001".to_owned(),
                message: err.to_string(),
            })?;
            let failure = outcome
                .records
                .values()
                .find_map(|r| r.error.as_ref())
                .map(|e| (e.code.clone(), e.message.clone()));
            Ok(crate::child::ChildOutcome {
                ok: outcome.ok,
                outputs: outcome.outputs,
                cost_usd: outcome.total_cost_usd,
                trace: None,
                failure,
            })
        })
    }
}

/// Nested runs: the child inherits the parent's remaining (law 6) and meets
/// the same guard against its OWN ledger. Under a parent cap below the
/// floor the child's run-decided route is refused before its request; above
/// it the request is sent and the child's spend is the parent's.
#[tokio::test]
async fn a_nested_route_meets_the_budget_its_parent_hands_down() {
    let parent = "nika: parent\ntasks:\n  call:\n    invoke:\n      workflow: \"./child.nika\"\n";
    for (cap, sent) in [(0.001, false), (0.01, true)] {
        let wires = Wires::new(5);
        let child = ChildOnWires {
            normal: Arc::clone(&wires.normal),
            bounded: Arc::clone(&wires.bounded),
            seat: PAID_ROUTE.to_owned(),
        };
        let runtime = runtime_on(MockShell::new(), (None, None), None, &wires, Some(cap))
            .with_child_runner(Arc::new(child));
        let (result, _) = run_on(&runtime, parent).await;
        let outcome = result.expect("the parent starts");
        if sent {
            assert!(outcome.ok, "{outcome:?}");
            assert_eq!(wires.normal.posts(), [DEEPSEEK]);
            assert!(outcome.total_cost_usd.is_some_and(|usd| usd > 0.0));
            continue;
        }
        assert!(!outcome.ok);
        let error = outcome.records["call"]
            .error
            .as_ref()
            .expect("the child failure");
        assert!(
            error
                .message
                .contains("refused before the provider request"),
            "{error:?}"
        );
        assert!(wires.normal.posts().is_empty(), "the child sent nothing");
    }
}
