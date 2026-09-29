// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

//! B12 · a reviewed unknown-cost fan seen from an embedder. A scripted
//! provider wire counts every physical request; the reviewed account (total,
//! in-flight bound, authored-retry law) decides what the real runtime may
//! send. A request the runtime's own timer abandons is held once and stops
//! every sibling; a received 429 is followed by the task's authored retry
//! inside the total, while a 500 stops every further request.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use std::time::Duration;

use nika_kernel::http::{HttpError, HttpPostDyn, HttpRequest, HttpResponse, HttpStreamResponse};
use nika_kernel::secret::Secret;
use nika_kernel_mock::{MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor};
use nika_providers::admission::{CostHostEvidence, CostReview, CostRoute};
use nika_providers::{AdmissionState, InferenceAdmission, ProviderRegistry, ProvidersConfig};
use nika_runtime::{DeterministicStamper, RunOutcome, Runtime, RuntimeConfig, VecSink};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;

const MODEL: &str = "deepseek/b12-fan-fixture";

/// The provider: request `n` (1-based) answers `statuses[n - 1]` (200 when
/// absent); a prompt naming `hang` never answers, and its dropped future is
/// counted once.
struct Wire {
    statuses: Vec<u16>,
    hang: Option<&'static str>,
    posts: AtomicUsize,
    dropped: AtomicUsize,
}

struct Pending<'a>(&'a AtomicUsize);
impl Drop for Pending<'_> {
    fn drop(&mut self) {
        self.0.fetch_add(1, SeqCst);
    }
}

impl HttpPostDyn for Wire {
    fn supports_single_attempt(&self) -> bool {
        true
    }
    async fn post(&self, req: HttpRequest) -> Result<HttpResponse, HttpError> {
        let n = self.posts.fetch_add(1, SeqCst) + 1;
        let body = String::from_utf8_lossy(req.body.as_ref().expect("body")).into_owned();
        if self.hang.is_some_and(|word| body.contains(word)) {
            let _pending = Pending(&self.dropped);
            return std::future::pending().await;
        }
        let status = self.statuses.get(n - 1).copied().unwrap_or(200);
        let answer = serde_json::json!({"id": format!("fan-{n}"), "model": "b12-fan-fixture",
            "choices": [{"message": {"content": format!("DONE-{n}")}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 10, "completion_tokens": 3, "total_tokens": 13,
                "prompt_cache_hit_tokens": 0, "prompt_cache_miss_tokens": 10}});
        Ok(HttpResponse::new(
            status,
            BTreeMap::new(),
            answer.to_string().into(),
            req.url,
        ))
    }
    async fn send_streaming(&self, _: HttpRequest) -> Result<HttpStreamResponse, HttpError> {
        panic!("an unknown-cost Run never streams");
    }
}

fn wire(statuses: &[u16], hang: Option<&'static str>) -> Arc<Wire> {
    Arc::new(Wire {
        statuses: statuses.to_vec(),
        hang,
        posts: AtomicUsize::new(0),
        dropped: AtomicUsize::new(0),
    })
}

/// The fresh, confirmed choice a host would bind for this Run.
fn reviewed(total: u32, width: u32, retry: bool) -> InferenceAdmission {
    let config = ProvidersConfig::new().with_key("deepseek", Secret::new("fixture"));
    let route = CostRoute::observe(MODEL, config).expect("route");
    let evidence = CostHostEvidence::unmanaged_interactive_local();
    let review = CostReview::new(
        "fan".into(),
        "run".into(),
        route.clone(),
        evidence,
        None,
        None,
    )
    .expect("review")
    .for_run(total)
    .expect("total")
    .with_concurrency(width)
    .expect("width")
    .with_authored_retry(retry);
    review.confirm("fan", &route).expect("fresh yes")
}

async fn run(source: &str, wire: &Arc<Wire>, account: &InferenceAdmission) -> RunOutcome {
    let wf = nika_schema::parse(
        source,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "fixture passes the ladder: {report:?}");
    let config = ProvidersConfig::new().with_key("deepseek", Secret::new("fixture"));
    let registry =
        ProviderRegistry::new(Arc::clone(wire), config).with_inference_admission(account.clone());
    let runtime_config = RuntimeConfig::new(None, 0)
        .with_inference_admission(account, "fan", "run")
        .expect("scoped");
    let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new())),
        Arc::clone(&invoke),
        InferVerb::new(Arc::new(registry), MODEL),
        AgentVerb::new(
            Arc::new(MockProvider::new("mock")),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            "mock/echo",
        ),
        nika_clock::DeclaredClock::system(),
        runtime_config,
    );
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let running = runtime.run(&wf, &report, &mut stamper, &mut sink);
    tokio::time::timeout(Duration::from_secs(10), running)
        .await
        .expect("the run settles")
        .expect("the run starts")
}

fn fan(items: &str, width: u32, extra: &str) -> String {
    format!(
        "nika: fan\nmodel: {MODEL}\npermits: {{}}\ntasks:\n  review:\n    for_each: {{ items: {items}, max_parallel: {width}, fail_fast: false }}\n{extra}    infer: {{ prompt: 'ITEM ${{{{ item }}}}', max_tokens: 32 }}\n"
    )
}

/// The runtime's own timer abandons a sent request: it is held once as an
/// unknown charge and no sibling may send after it, whatever the total.
#[tokio::test]
async fn a_timed_out_fan_item_is_held_once_and_no_sibling_sends_after_it() {
    let wire = wire(&[], Some("ITEM x"));
    let account = reviewed(3, 1, false);
    let source = fan("[x, y, z]", 1, "    timeout: \"200ms\"\n");
    let outcome = run(&source, &wire, &account).await;
    assert!(!outcome.ok, "{outcome:?}");
    assert_eq!(
        wire.posts.load(SeqCst),
        1,
        "only the abandoned request left"
    );
    assert_eq!(wire.dropped.load(SeqCst), 1, "dropped exactly once");
    let receipt = account.snapshot().expect("receipt");
    assert_eq!(receipt.state, AdmissionState::Uncertain);
    assert_eq!(receipt.unknown_attempts.len(), 1, "siblings never reserved");
    assert_eq!(receipt.unknown_calls, 1);
    assert_eq!(
        receipt.unknown_attempts[0].note,
        "possibly billed; no automatic retry"
    );
}

/// A received 429 is followed by the task's authored retry inside the total
/// (one request in flight: the answered one released its slot); a 500 leaves
/// the account Uncertain and neither the retry nor a sibling with a free slot sends.
#[tokio::test]
async fn an_authored_retry_follows_a_429_and_a_500_stops_every_request() {
    let retry = "    retry: { max_attempts: 2, backoff_ms: 1 }\n";
    let wire_429 = wire(&[429], None);
    let account = reviewed(4, 1, true);
    let outcome = run(&fan("[x, y]", 1, retry), &wire_429, &account).await;
    assert!(outcome.ok, "{outcome:?}");
    assert_eq!(wire_429.posts.load(SeqCst), 3, "x twice, y once");
    let receipt = account.snapshot().expect("receipt");
    assert_eq!(receipt.state, AdmissionState::Open);
    assert_eq!(receipt.unknown_attempts.len(), 3);
    assert_eq!(
        receipt.unknown_attempts[0].note,
        "answered HTTP 429; usage and USD cost unknown"
    );
    let wire_500 = wire(&[500], None);
    let account = reviewed(4, 2, true);
    let outcome = run(&fan("[x, y]", 2, retry), &wire_500, &account).await;
    assert!(!outcome.ok, "{outcome:?}");
    assert_eq!(wire_500.posts.load(SeqCst), 1, "nothing after the 500");
    let receipt = account.snapshot().expect("receipt");
    assert_eq!(receipt.state, AdmissionState::Uncertain);
    assert_eq!(receipt.unknown_attempts.len(), 1);
}
