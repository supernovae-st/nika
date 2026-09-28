// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use nika_kernel::http::{HttpError, HttpPostDyn, HttpRequest, HttpResponse, HttpStreamResponse};
use nika_kernel_mock::{
    MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_invoke::InvokeVerb;

use crate::{DeterministicStamper, RunOutcome, Runtime, RuntimeConfig, TaskStatus, VecSink};

/// Serves the queued canned bodies (one per round-trip) · counts every
/// provider request it saw.
struct ScriptedHttp {
    bodies: Mutex<VecDeque<&'static str>>,
    calls: Mutex<usize>,
}

impl ScriptedHttp {
    fn serving(bodies: &[&'static str]) -> Arc<Self> {
        Arc::new(Self {
            bodies: Mutex::new(bodies.iter().copied().collect()),
            calls: Mutex::new(0),
        })
    }

    fn calls(&self) -> usize {
        *self.calls.lock().expect("test mutex")
    }
}

impl HttpPostDyn for ScriptedHttp {
    async fn post(&self, request: HttpRequest) -> Result<HttpResponse, HttpError> {
        *self.calls.lock().expect("test mutex") += 1;
        let body = self
            .bodies
            .lock()
            .expect("test mutex")
            .pop_front()
            .ok_or_else(|| HttpError::Other {
                reason: "ScriptedHttp: no canned response queued".to_owned(),
            })?;
        Ok(HttpResponse::new(
            200,
            BTreeMap::new(),
            Bytes::from_static(body.as_bytes()),
            request.url,
        ))
    }

    async fn send_streaming(&self, _request: HttpRequest) -> Result<HttpStreamResponse, HttpError> {
        Err(HttpError::Unsupported {
            reason: "streaming not exercised here".to_owned(),
        })
    }
}

/// A speaking loopback stub so the B-5 liveness gate passes (the
/// localhost-is-shared law: a live/dead ollama on the host must never
/// decide this test).
#[allow(clippy::disallowed_methods)] // test seam — the probe's own worker pattern
fn spawn_stub_server() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    std::thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            use std::io::Write as _;
            let _ = stream.write_all(b"HTTP/1.0 404 Not Found\r\n\r\n");
        }
    });
    port
}

/// The blank-answer repro body: empty visible content · real billed
/// output tokens (the reasoning trace ate the budget).
const EMPTY_WITH_SPEND: &str = r#"{"choices":[{"message":{"content":""},"finish_reason":"length"}],"usage":{"prompt_tokens":7,"completion_tokens":512}}"#;

async fn run_workflow(yaml: &str, http: Arc<ScriptedHttp>) -> RunOutcome {
    let wf = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "fixture passes the ladder");
    let registry = Arc::new(ProviderRegistry::new(
        http,
        ProvidersConfig::new().with_base_url(
            "ollama",
            format!("http://127.0.0.1:{}", spawn_stub_server()),
        ),
    ));
    let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new())),
        Arc::clone(&invoke),
        nika_verb_infer::InferVerb::new(registry, "ollama/llama3.2"),
        AgentVerb::new(
            Arc::new(MockProvider::new("mock")),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            "mock/echo",
        ),
        MockClock::new(),
        RuntimeConfig::default(),
    );
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("the run completes (a workflow failure is data)")
}

/// The issue's repro: the blank answer fails the task TYPED, the run
/// verdict goes red, and the declared `retry:` does NOT fire — the
/// remedy is `max_tokens`, never a re-ask at the same budget.
#[tokio::test]
async fn empty_answer_settles_failed_typed_and_the_run_goes_red() {
    let http = ScriptedHttp::serving(&[EMPTY_WITH_SPEND]);
    let outcome = run_workflow(
        "nika: w\nmodel: ollama/llama3.2\ntasks:\n  ask:\n    retry: { max_attempts: 3, backoff_ms: 1, backoff_strategy: fixed, jitter: false }\n    infer: { prompt: \"hello\" }\n",
        Arc::clone(&http),
    )
    .await;
    assert!(!outcome.ok, "an empty answer is no longer a green run");
    let rec = &outcome.records["ask"];
    assert_eq!(rec.status, TaskStatus::Failure, "the task settles failed");
    let err = rec.error.as_ref().expect("the failure carries its record");
    assert_eq!(err.code, "NIKA-INFER-004", "the typed wire code");
    assert!(
        err.message.contains("infer produced an empty answer"),
        "the warn's teaching survives the promotion: {}",
        err.message
    );
    assert!(
        err.message.contains("max_tokens"),
        "the likely fix is named: {}",
        err.message
    );
    assert!(!err.transient, "never retry-eligible by default");
    assert_eq!(
        rec.attempts,
        Some(1),
        "the declared retry: does NOT fire on a non-transient code"
    );
    assert_eq!(http.calls(), 1, "exactly one billed round-trip");
}

/// The authored escape hatch stays bounded: `on_codes: [NIKA-INFER-004]`
/// opts into retries (same policy as every typed infer failure) — and
/// the budget caps them, never a forever-loop.
#[tokio::test]
async fn empty_answer_retry_is_opt_in_and_bounded() {
    let http = ScriptedHttp::serving(&[EMPTY_WITH_SPEND, EMPTY_WITH_SPEND, EMPTY_WITH_SPEND]);
    let outcome = run_workflow(
        "nika: w\nmodel: ollama/llama3.2\ntasks:\n  ask:\n    retry: { max_attempts: 3, backoff_ms: 1, backoff_strategy: fixed, jitter: false, on_codes: [NIKA-INFER-004] }\n    infer: { prompt: \"hello\" }\n",
        Arc::clone(&http),
    )
    .await;
    assert!(!outcome.ok);
    let rec = &outcome.records["ask"];
    assert_eq!(rec.status, TaskStatus::Failure);
    assert_eq!(
        rec.error.as_ref().expect("error record").code,
        "NIKA-INFER-004"
    );
    assert_eq!(rec.attempts, Some(3), "the authored retries ran");
    assert_eq!(
        http.calls(),
        3,
        "bounded at max_attempts — never retried forever"
    );
}

/// Non-regression: a real answer with the same wire shape settles
/// green, no error attached.
#[tokio::test]
async fn a_real_answer_still_settles_green() {
    let http = ScriptedHttp::serving(&[
        r#"{"choices":[{"message":{"content":"Paris"},"finish_reason":"stop"}],"usage":{"prompt_tokens":7,"completion_tokens":50}}"#,
    ]);
    let outcome = run_workflow(
        "nika: w\nmodel: ollama/llama3.2\ntasks:\n  ask:\n    infer: { prompt: \"capital of France?\" }\n",
        Arc::clone(&http),
    )
    .await;
    assert!(outcome.ok, "a non-empty answer stays green");
    let rec = &outcome.records["ask"];
    assert_eq!(rec.status, TaskStatus::Success);
    assert!(rec.error.is_none(), "no failure rides a real answer");
    assert_eq!(http.calls(), 1);
}
