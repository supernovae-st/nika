// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

//! B11 · the `unwind` cleanup lane rides the run's own authority, seen from an
//! embedder. A scripted provider wire counts every physical request it takes,
//! and the run outcome must hold each of them on the ledger exactly once: a
//! served request is priced, a request that failed after it was sent is an
//! unknown charge, and a request the cleanup's own timer abandoned is an
//! unknown charge too. Never zero, never twice. A cleanup stays best-effort:
//! none of this fails its run.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use std::time::Duration;

use nika_event::{Event, EventKind};
use nika_kernel::http::{HttpError, HttpPostDyn, HttpRequest, HttpResponse, HttpStreamResponse};
use nika_kernel::secret::Secret;
use nika_kernel_mock::{MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_runtime::{DeterministicStamper, RunOutcome, Runtime, RuntimeConfig, VecSink};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;

const PAID: &str = "deepseek/deepseek-v4-pro";

/// How the provider answers the cleanup's request.
#[derive(Clone, Copy)]
enum Answer {
    /// 200 with complete usage.
    Serve,
    /// 500 after the request arrived: sent, never answered with usage.
    Fail,
    /// Never answers: only the cleanup's own timer ends it.
    Hang,
}

/// The provider as the cleanup meets it: each request is counted when it
/// arrives, and a hung one again when the runtime drops it. A served answer
/// reports `prompt` input tokens (a size the launch floor cannot see).
struct Wire {
    answer: Answer,
    prompt: u64,
    posts: AtomicUsize,
    dropped: AtomicUsize,
}

impl Wire {
    fn new(answer: Answer) -> Arc<Self> {
        Self::with_prompt(answer, 10)
    }
    fn with_prompt(answer: Answer, prompt: u64) -> Arc<Self> {
        Arc::new(Self {
            answer,
            prompt,
            posts: AtomicUsize::new(0),
            dropped: AtomicUsize::new(0),
        })
    }
}

/// Counts a hung request whose future was dropped.
struct Pending<'a>(&'a AtomicUsize);

impl Drop for Pending<'_> {
    fn drop(&mut self) {
        self.0.fetch_add(1, SeqCst);
    }
}

impl HttpPostDyn for Wire {
    async fn post(&self, req: HttpRequest) -> Result<HttpResponse, HttpError> {
        self.posts.fetch_add(1, SeqCst);
        let (status, body) = match self.answer {
            Answer::Serve => (
                200,
                serde_json::json!({
                    "id": "cleanup", "model": "deepseek-v4-pro",
                    "choices": [{"message": {"content": "tidied"}, "finish_reason": "stop"}],
                    "usage": {"prompt_tokens": self.prompt, "completion_tokens": 5,
                        "total_tokens": self.prompt + 5,
                        "prompt_cache_hit_tokens": 0, "prompt_cache_miss_tokens": self.prompt}
                }),
            ),
            Answer::Fail => (
                500,
                serde_json::json!({"error": {"message": "upstream failed"}}),
            ),
            Answer::Hang => {
                let _pending = Pending(&self.dropped);
                return std::future::pending().await;
            }
        };
        Ok(HttpResponse::new(
            status,
            BTreeMap::new(),
            body.to_string().into(),
            req.url,
        ))
    }
    async fn send_streaming(&self, _: HttpRequest) -> Result<HttpStreamResponse, HttpError> {
        panic!("no cleanup here streams");
    }
}

/// `work` is an `exec` (no provider, nothing on the ledger); `tidy` is its
/// `unwind` cleanup on `seat` (at the reasoning seat's 256-token floor),
/// bounded by its own 300 ms timer.
fn cleanup(seat: &str) -> String {
    format!(
        "nika: tidy\npermits: {{ exec: [\"true\"] }}\ntasks:\n  work:\n    exec: {{ command: [\"true\"] }}\n  tidy:\n    after: {{ work: unwind }}\n    timeout: \"300ms\"\n    infer: {{ prompt: bye, model: \"{seat}\", max_tokens: 256 }}\n"
    )
}

async fn run(source: &str, wire: &Arc<Wire>, cap: Option<f64>) -> RunOutcome {
    launch(source, wire, cap, MockShell::new().enqueue_ok("done"))
        .await
        .0
}

/// As [`run`], on `shell`, keeping the events the run wrote.
async fn launch(
    source: &str,
    wire: &Arc<Wire>,
    cap: Option<f64>,
    shell: MockShell,
) -> (RunOutcome, Vec<Event>) {
    let wf = nika_schema::parse(
        source,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "fixture passes the ladder: {report:?}");
    let config = ProvidersConfig::new().with_key("deepseek", Secret::new("fixture"));
    let registry = ProviderRegistry::new(Arc::clone(wire), config);
    let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(shell)),
        Arc::clone(&invoke),
        InferVerb::new(Arc::new(registry), "mock/echo"),
        AgentVerb::new(
            Arc::new(MockProvider::new("mock")),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            "mock/echo",
        ),
        nika_clock::DeclaredClock::system(),
        RuntimeConfig::default(),
    )
    .with_max_cost_usd(cap);
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let running = runtime.run(&wf, &report, &mut stamper, &mut sink);
    let outcome = tokio::time::timeout(Duration::from_secs(10), running)
        .await
        .expect("the run settles within its cleanup timer")
        .expect("the run starts");
    (outcome, sink.into_events())
}

/// A served cleanup request is priced on the run's ledger, once.
#[tokio::test]
async fn a_served_cleanup_request_is_priced_once() {
    let wire = Wire::new(Answer::Serve);
    let outcome = run(&cleanup(PAID), &wire, None).await;
    assert!(outcome.ok, "{outcome:?}");
    assert_eq!(wire.posts.load(SeqCst), 1);
    assert_eq!(
        (outcome.priced_calls, outcome.unpriced_calls),
        (1, 0),
        "{outcome:?}"
    );
    assert!(
        outcome.total_cost_usd.is_some_and(|usd| usd > 0.0),
        "the cleanup's spend is the run's: {outcome:?}"
    );
}

/// A cleanup request that failed after it was sent is one unknown charge:
/// its own failure carries it, and nothing counts it again.
#[tokio::test]
async fn a_failed_cleanup_request_is_one_unknown_charge() {
    let wire = Wire::new(Answer::Fail);
    let outcome = run(&cleanup(PAID), &wire, None).await;
    assert!(outcome.ok, "a cleanup never fails its run: {outcome:?}");
    assert_eq!(wire.posts.load(SeqCst), 1, "a 500 is never re-sent");
    assert_eq!(
        (outcome.priced_calls, outcome.unpriced_calls),
        (0, 1),
        "{outcome:?}"
    );
}

/// A cleanup its own timer abandons keeps the request it sent: one unknown
/// charge (the dispatch journal), never zero and never twice.
#[tokio::test]
async fn a_cleanup_its_timer_drops_is_one_unknown_charge() {
    let wire = Wire::new(Answer::Hang);
    let outcome = run(&cleanup(PAID), &wire, None).await;
    assert!(outcome.ok, "a cleanup never fails its run: {outcome:?}");
    assert_eq!(
        (wire.posts.load(SeqCst), wire.dropped.load(SeqCst)),
        (1, 1),
        "sent, then dropped by the cleanup's timer"
    );
    assert_eq!(
        (outcome.priced_calls, outcome.unpriced_calls),
        (0, 1),
        "{outcome:?}"
    );
}

/// `ask` then its two cleanups: `paid` can spend, `sweep` is housekeeping.
const CROSSED: &str = "nika: crossed\npermits: { exec: [\"true\"] }\ntasks:\n  ask:\n    infer: { prompt: hi, model: \"deepseek/deepseek-v4-pro\", max_tokens: 256 }\n  paid:\n    after: { ask: unwind }\n    infer: { prompt: bye, model: \"deepseek/deepseek-v4-pro\", max_tokens: 256 }\n  sweep:\n    after: { ask: unwind }\n    exec: { command: [\"true\"] }\n";

/// Once the run's budget is crossed, a cleanup that can spend does not start:
/// it is refused before dispatch and journaled, and nothing more reaches the
/// wire, as the main lane starts no task after a trip. A housekeeping cleanup
/// of the same task still runs. The cap (0.0025) admits both calls' floors
/// (2 × 256 tokens); `ask`'s served answer then reports a million input tokens
/// the floor could not see and crosses it. The same run under a small prompt
/// is the control: the paid cleanup is sent.
#[tokio::test]
async fn after_the_budget_is_crossed_only_housekeeping_cleanups_run() {
    for (prompt, crossed) in [(1_000_000, true), (10, false)] {
        let wire = Wire::with_prompt(Answer::Serve, prompt);
        let shell = MockShell::new().enqueue_ok("swept");
        let probe = shell.clone();
        let (outcome, events) = launch(CROSSED, &wire, Some(0.0025), shell).await;
        assert_eq!(outcome.budget_exceeded, crossed, "{outcome:?}");
        assert_eq!(
            probe.executed_commands().len(),
            1,
            "prompt {prompt}: the housekeeping cleanup runs either way"
        );
        let journal: Vec<(String, String)> = events
            .iter()
            .filter(|e| {
                e.kind == EventKind::PermitChecked && e.str_field("plane") == Some("on_finally")
            })
            .map(|e| {
                let text = |key| e.str_field(key).unwrap_or_default().to_owned();
                (text("decision"), text("why"))
            })
            .collect();
        let refused = journal.iter().any(|(decision, why)| {
            decision == "failure" && why.contains("NIKA-1704") && why.contains("already crossed")
        });
        if crossed {
            assert_eq!(wire.posts.load(SeqCst), 1, "only `ask` reached the wire");
            assert!(refused, "{journal:?}");
            assert_eq!(outcome.priced_calls, 1, "{outcome:?}");
        } else {
            assert_eq!(wire.posts.load(SeqCst), 2, "the paid cleanup is sent");
            assert!(!refused, "{journal:?}");
            assert_eq!(outcome.priced_calls, 2, "{outcome:?}");
        }
    }
}

/// The same request as the run's own task is the control: the cleanup lane
/// now counts exactly what the main lane counts.
#[tokio::test]
async fn the_main_lane_twin_counts_the_same() {
    let twin = cleanup(PAID).replace("    after: { work: unwind }\n", "");
    for (answer, counts) in [
        (Answer::Serve, (1, 0)),
        (Answer::Fail, (0, 1)),
        (Answer::Hang, (0, 1)),
    ] {
        let wire = Wire::new(answer);
        let outcome = run(&twin, &wire, None).await;
        assert_eq!(wire.posts.load(SeqCst), 1);
        assert_eq!(
            (outcome.priced_calls, outcome.unpriced_calls),
            counts,
            "{outcome:?}"
        );
    }
}
