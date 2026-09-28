// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! C2 · an exact declared-free route observed per Run (E4): the host binds
//! a scoped observer, compose gives only its routes a single-attempt client,
//! and the run's terminal frame carries the receipt. Injected transport:
//! the wire counts every post, so a second dispatch cannot hide.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use nika_kernel::http::{HttpError, HttpPostDyn, HttpRequest, HttpResponse, HttpStreamResponse};
use nika_kernel::secret::Secret;
use nika_kernel::tool_executor::ToolResult;
use nika_kernel_mock::{MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor};
use nika_providers::{InferenceAdmission, ProviderRegistry, ProvidersConfig};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_invoke::InvokeVerb;

use super::*;

const FREE: &str = "openrouter/qwen/qwen3.8-27b:free";

/// An `OpenRouter` wire that answers every post as `served`; `single` is the
/// client's single-attempt promise (compose's bounded client makes it).
struct Wire {
    single: bool,
    served: &'static str,
    posts: AtomicUsize,
}

impl Wire {
    fn new(single: bool, served: &'static str) -> Arc<Self> {
        Arc::new(Self {
            single,
            served,
            posts: AtomicUsize::new(0),
        })
    }
    fn posts(&self) -> usize {
        self.posts.load(Ordering::SeqCst)
    }
}

impl HttpPostDyn for Wire {
    fn supports_single_attempt(&self) -> bool {
        self.single
    }
    async fn post(&self, req: HttpRequest) -> Result<HttpResponse, HttpError> {
        self.posts.fetch_add(1, Ordering::SeqCst);
        assert_eq!(req.url, "https://openrouter.ai/api/v1/chat/completions");
        let body = serde_json::json!({
            "id": "free-fixture", "model": self.served,
            "choices": [{"message": {"content": "observed"}, "finish_reason": "stop"}],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15,
                      "cost": 0, "is_byok": false}
        });
        Ok(HttpResponse::new(
            200,
            std::collections::BTreeMap::new(),
            body.to_string().into(),
            req.url,
        ))
    }
    async fn send_streaming(&self, _: HttpRequest) -> Result<HttpStreamResponse, HttpError> {
        panic!("the declared-free route is non-streaming");
    }
}

/// Run `source` over the registry compose builds: an account that observes
/// declared-free routes only gets its own bounded client beside the normal one.
async fn run_free(
    source: &str,
    account: Option<&InferenceAdmission>,
    (normal, bounded): (&Arc<Wire>, &Arc<Wire>),
    tools: MockToolExecutor,
) -> (RunOutcome, Vec<Event>) {
    let wf = nika_schema::parse(
        source,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "fixture passes the ladder: {report:?}");
    let config = ProvidersConfig::new().with_key("openrouter", Secret::new("fixture"));
    let mut registry = ProviderRegistry::new(Arc::clone(normal), config);
    let mut runtime_config = RuntimeConfig::default();
    if let Some(account) = account {
        assert!(account.observes_declared_free_only());
        registry = registry.with_inference_admission_http(account.clone(), Arc::clone(bounded));
        runtime_config = runtime_config
            .with_inference_admission(account, "declared-free observation", "run-1")
            .expect("an observer binds without an unknown choice");
    }
    let invoke = Arc::new(InvokeVerb::new(Arc::new(tools)));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new())),
        Arc::clone(&invoke),
        nika_verb_infer::InferVerb::new(Arc::new(registry), FREE),
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
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("the run settles");
    (outcome, sink.into_events())
}

/// The terminal frame's receipt, parsed; `None` when the frame has none.
fn receipt(events: &[Event]) -> Option<serde_json::Value> {
    let terminal = events
        .iter()
        .rfind(|e| e.is_terminal())
        .expect("a terminal frame");
    let text = terminal.str_field("inference_admission")?;
    Some(serde_json::from_str(text).expect("the receipt is JSON"))
}

/// A bounded text call on the exact route settles with a receipt: one sent
/// attempt priced at the pinned zero tariff, never billed, and the receipt
/// names its scope. Only the terminal frame carries it, and without an
/// account the same run carries none and keeps today's transport.
#[tokio::test]
async fn a_declared_free_text_run_stamps_its_scoped_receipt_on_the_terminal_frame() {
    let source = format!(
        "nika: free-observed\nmodel: {FREE}\npermits: {{}}\ntasks:\n  ask:\n    infer: {{ prompt: hello, max_tokens: 64 }}\n"
    );
    let (normal, bounded) = (
        Wire::new(false, "unused"),
        Wire::new(true, "qwen/qwen3.8-27b:free"),
    );
    let account = InferenceAdmission::observe_declared_free();
    let (outcome, events) = run_free(
        &source,
        Some(&account),
        (&normal, &bounded),
        MockToolExecutor::new(),
    )
    .await;
    assert!(outcome.ok, "{outcome:?}");
    assert_eq!(
        (normal.posts(), bounded.posts()),
        (0, 1),
        "one bounded attempt"
    );
    let observed = receipt(&events).expect("the terminal frame carries the receipt");
    assert_eq!(observed["scoped_to_declared_free"], true, "{observed}");
    assert_eq!(observed["state"], "Open", "{observed}");
    assert_eq!(observed["unknown_calls"], 0, "{observed}");
    assert!(
        observed["billed_nano_usd"].is_null(),
        "no invoice: {observed}"
    );
    let attempts = observed["attempts"].as_array().expect("attempts");
    assert_eq!(attempts.len(), 1, "{observed}");
    assert_eq!(attempts[0]["sent"], true);
    assert_eq!(attempts[0]["estimated_nano_usd"], "0");
    assert_eq!(attempts[0]["model"], "qwen/qwen3.8-27b:free");
    let stamped = events
        .iter()
        .filter(|e| e.field("inference_admission").is_some());
    assert_eq!(stamped.count(), 1, "only the terminal frame");

    let (plain, unobserved) = (
        Wire::new(false, "qwen/qwen3.8-27b:free"),
        Wire::new(true, "x"),
    );
    let (outcome, events) = run_free(
        &source,
        None,
        (&plain, &unobserved),
        MockToolExecutor::new(),
    )
    .await;
    assert!(outcome.ok, "{outcome:?}");
    assert_eq!((plain.posts(), unobserved.posts()), (1, 0));
    assert_eq!(receipt(&events), None, "no account, no receipt");
}

/// A response that names another model leaves the charge unknown: the task
/// fails, the receipt says Uncertain with no estimate (never a known zero),
/// the next call on the same account is refused before the wire, and a
/// local effect still runs under its own permits (billing is not effects).
#[tokio::test]
async fn a_mismatched_identity_stays_unknown_and_closes_the_next_call_but_not_effects() {
    let source = format!(
        "nika: free-uncertain\nmodel: {FREE}\npermits: {{ tools: ['nika:write'], fs: {{ write: ['./note.txt'] }} }}\ntasks:\n  ask:\n    infer: {{ prompt: hello, max_tokens: 64 }}\n  again:\n    after: {{ ask: terminal }}\n    infer: {{ prompt: again, max_tokens: 64 }}\n  note:\n    after: {{ again: terminal }}\n    invoke: {{ tool: 'nika:write', args: {{ path: ./note.txt, content: local }} }}\n"
    );
    let (normal, bounded) = (
        Wire::new(false, "unused"),
        Wire::new(true, "qwen/qwen3.8-27b"),
    );
    let tools = MockToolExecutor::new().enqueue_ok(ToolResult::success("tc1", "written"));
    let account = InferenceAdmission::observe_declared_free();
    let (outcome, events) =
        run_free(&source, Some(&account), (&normal, &bounded), tools.clone()).await;
    assert!(!outcome.ok, "{outcome:?}");
    assert_eq!(
        (normal.posts(), bounded.posts()),
        (0, 1),
        "no second dispatch"
    );
    let observed = receipt(&events).expect("a failed run still carries the receipt");
    assert_eq!(observed["state"], "Uncertain", "{observed}");
    assert_eq!(observed["unknown_calls"], 1, "{observed}");
    assert_eq!(observed["scoped_to_declared_free"], true, "{observed}");
    let attempts = observed["attempts"].as_array().expect("attempts");
    assert_eq!(
        attempts.len(),
        1,
        "the refused call reserved nothing: {observed}"
    );
    assert!(attempts[0]["estimated_nano_usd"].is_null(), "{observed}");
    let calls = tools.captured_calls();
    assert_eq!(calls.len(), 1, "the local effect ran");
    assert_eq!(calls[0].name, "nika:write");
}
