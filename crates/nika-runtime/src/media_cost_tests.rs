// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! C-D3 · paid media spend under `--max-cost-usd` and in the settlement,
//! through the REAL parse → check → run chain and the real builtin
//! dispatcher. The provider endpoint is a recording fake (`MockHttp` on the
//! image and speech planes): no request leaves the process, and the fake's
//! ledger proves which calls were sent.
//!
//! - under a cap, a paid media call the catalog cannot price (openai
//!   images, openai and elevenlabs speech) refuses before the prologue,
//!   NIKA-1709, zero requests: an unpriced text model's refusal;
//! - a templated provider only the run decides refuses before its request
//!   (NIKA-1704), zero requests;
//! - with no cap the call runs and settles UNPRICED, never "priced";
//! - xAI images carry a catalog rate: metered against the cap;
//! - mock media under a cap stays unmetered and is never counted as paid.

#![allow(clippy::expect_used, clippy::panic)]

use super::*;
use nika_event::settlement::CostQualifier;
use nika_kernel::secret::Secret;
use nika_kernel_mock::{MockFs, MockHttp, MockProvider, MockShell};

type MediaDispatcher = nika_builtin::BuiltinDispatcher<
    MockFs,
    MockHttp,
    nika_kernel_mock::MockClock,
    nika_builtin::NullEmitter,
    nika_builtin::NonInteractive,
    nika_builtin::NoWorkflow,
>;

type MediaRuntime = Runtime<
    MockShell,
    MediaDispatcher,
    nika_providers::NoHttp,
    MockProvider,
    MediaDispatcher,
    nika_kernel_mock::MockClock,
>;

fn key() -> Secret {
    Secret::new("sk-fake-endpoint-only".to_owned())
}

/// The real builtin dispatcher, its image and speech planes wired to the
/// recording fake `http` with a key for every paid provider.
fn media_runtime(http: &MockHttp, budget: Option<f64>) -> MediaRuntime {
    let images = nika_builtin::ImageKeys::new()
        .with_openai(key())
        .with_gemini(key())
        .with_xai(key());
    let speech = nika_builtin::TtsKeys::new()
        .with_openai(key())
        .with_elevenlabs(key());
    let dispatcher = Arc::new(
        nika_builtin::BuiltinDispatcher::new(
            Arc::new(MockFs::new()),
            Arc::new(http.clone()),
            Arc::new(nika_kernel_mock::MockClock::new()),
            Arc::new(nika_builtin::NullEmitter::default()),
            Arc::new(nika_builtin::NonInteractive::default()),
            Arc::new(nika_builtin::NoWorkflow::default()),
        )
        .with_image_plane(Arc::new(http.clone()), images)
        .with_tts_plane(Arc::new(http.clone()), speech),
    );
    let invoke = Arc::new(InvokeVerb::new(Arc::clone(&dispatcher)));
    let registry = Arc::new(nika_providers::ProviderRegistry::without_http(
        nika_providers::ProvidersConfig::default(),
    ));
    Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new())),
        Arc::clone(&invoke),
        InferVerb::new(registry, "mock/echo"),
        AgentVerb::new(
            Arc::new(MockProvider::new("mock")),
            invoke,
            Arc::clone(&dispatcher),
            "mock/echo",
        ),
        nika_kernel_mock::MockClock::new(),
        RuntimeConfig::default().with_max_cost_usd(budget),
    )
}

/// One media task (`tool` with `args`) under `budget`, the report checked
/// clean first; the run's own Result (a refused admission is data here).
async fn settle(
    tool: &str,
    args: &str,
    http: &MockHttp,
    budget: Option<f64>,
) -> Result<RunOutcome, RuntimeError> {
    let yaml = format!(
        "nika: c-d3\ninputs:\n  provider: {{ type: string, default: openai }}\npermits:\n  tools: [\"{tool}\"]\n  fs: {{ write: [\"./out/**\"] }}\ntasks:\n  media:\n    invoke:\n      tool: \"{tool}\"\n      args: {{ {args}, output_dir: \"./out\" }}\noutputs:\n  media: ${{{{ tasks.media.output }}}}\n"
    );
    let wf = nika_schema::parse(
        &yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "fixture checks clean: {report:#?}");
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    media_runtime(http, budget)
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
}

/// A 1×1 PNG, the payload an image endpoint answers with.
const PNG_1X1_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

/// What an `OpenAI`-shaped image endpoint answers: one image, token usage, no USD.
fn openai_image_answer() -> Vec<u8> {
    serde_json::json!({
        "created": 1,
        "data": [{ "b64_json": PNG_1X1_B64 }],
        "usage": { "input_tokens": 10, "output_tokens": 272, "total_tokens": 282 }
    })
    .to_string()
    .into_bytes()
}

/// What the xAI image endpoint answers: one image and its exact spend
/// (2e8 ticks = $0.02).
fn xai_image_answer() -> Vec<u8> {
    serde_json::json!({
        "data": [{ "b64_json": PNG_1X1_B64, "revised_prompt": "" }],
        "usage": { "cost_in_usd_ticks": 200_000_000u64 }
    })
    .to_string()
    .into_bytes()
}

/// The refusal a run's launch gates owe an unpriced media call under a cap:
/// the `BudgetFloor` family (NIKA-1709) naming the call, before any request.
fn assert_refused_at_admission(result: Result<RunOutcome, RuntimeError>, http: &MockHttp) {
    let err = result.expect_err("an armed cap refuses unknown media spend");
    assert!(
        matches!(err, RuntimeError::BudgetFloor { .. }),
        "the unpriced text model's refusal family: {err:?}"
    );
    let text = err.to_string();
    assert!(text.starts_with("NIKA-1709"), "{text}");
    assert!(text.contains("no catalog price for task `media`"), "{text}");
    assert!(
        http.sent_requests().is_empty(),
        "refused before any request reached the endpoint"
    );
}

#[tokio::test]
async fn unpriced_speech_under_a_cap_is_refused_before_any_request() {
    for args in [
        "provider: openai, text: \"Budget zero.\"",
        "provider: elevenlabs, text: \"Budget zero.\"",
        "model: gpt-4o-mini-tts, text: \"Budget zero.\"",
    ] {
        let http = MockHttp::new().enqueue_ok(200, b"ID3\x04fake-mp3".to_vec());
        for cap in [0.0, 1.0] {
            let result = settle("nika:tts_generate", args, &http, Some(cap)).await;
            assert_refused_at_admission(result, &http);
        }
    }
}

#[tokio::test]
async fn unpriced_images_under_a_cap_are_refused_before_any_request() {
    for args in [
        "provider: openai, prompt: \"a small blue circle\"",
        "model: gpt-image-1-mini, prompt: \"a small blue circle\"",
        "provider: gemini, prompt: \"a small blue circle\"",
    ] {
        let http = MockHttp::new().enqueue_ok(200, openai_image_answer());
        let result = settle("nika:image_generate", args, &http, Some(5.0)).await;
        assert_refused_at_admission(result, &http);
    }
}

/// A provider only the run decides passes the launch gates (they cannot
/// judge a template) and refuses before its own request, NIKA-1704.
#[tokio::test]
async fn a_run_decided_unpriced_provider_is_refused_before_its_request() {
    let http = MockHttp::new().enqueue_ok(200, b"ID3\x04fake-mp3".to_vec());
    let args = "provider: \"${{ inputs.provider }}\", text: \"hi\"";
    let outcome = settle("nika:tts_generate", args, &http, Some(1.0))
        .await
        .expect("the run starts: the provider is decided at run");
    assert!(!outcome.ok);
    let err = outcome.records["media"]
        .error
        .as_ref()
        .expect("the task carries the refusal");
    assert_eq!(err.code, "NIKA-1704");
    assert!(
        err.message.contains("refused before the media request")
            && err.message.contains("`openai`"),
        "{}",
        err.message
    );
    assert!(!err.transient, "a refusal is never retried");
    assert!(http.sent_requests().is_empty(), "no request was sent");
}

/// With no cap the paid call runs, and the settlement counts it as
/// unpriced: never `priced`, never `unmetered`, never a $0 total.
#[tokio::test]
async fn without_a_cap_unpriced_media_settles_unpriced() {
    let cases = [
        (
            "nika:tts_generate",
            "provider: openai, text: \"Hello.\"",
            b"ID3\x04fake-mp3".to_vec(),
        ),
        (
            "nika:image_generate",
            "provider: openai, prompt: \"a small blue circle\"",
            openai_image_answer(),
        ),
    ];
    for (tool, args, answer) in cases {
        let http = MockHttp::new().enqueue_ok(200, answer);
        let outcome = settle(tool, args, &http, None)
            .await
            .expect("no cap, no admission refusal");
        assert!(outcome.ok, "{tool}: {:?}", outcome.records["media"].error);
        assert_eq!(http.sent_requests().len(), 1, "{tool}: one paid request");
        assert_eq!(outcome.unpriced_calls, 1, "{tool}: counted unpriced");
        assert_eq!(outcome.priced_calls, 0, "{tool}");
        assert_eq!(outcome.total_cost_usd, None, "{tool}: no fake zero");
        assert_eq!(
            outcome.settlement.spend.qualifier,
            CostQualifier::Unpriced,
            "{tool}: the run never reports priced while it holds unknown spend"
        );
    }
}

/// xAI images carry a catalog rate: the call is admitted under a cap that
/// covers it and metered against it with the spend the endpoint reported.
#[tokio::test]
async fn a_priced_media_call_is_metered_against_the_cap() {
    let http = MockHttp::new().enqueue_ok(200, xai_image_answer());
    let outcome = settle(
        "nika:image_generate",
        "provider: xai, prompt: \"a small blue circle\"",
        &http,
        Some(1.0),
    )
    .await
    .expect("the $0.02 floor fits a $1 cap");
    assert!(outcome.ok, "{:?}", outcome.records["media"].error);
    assert_eq!(http.sent_requests().len(), 1);
    assert_eq!(outcome.priced_calls, 1);
    assert_eq!(outcome.unpriced_calls, 0);
    let spent = outcome.total_cost_usd.expect("metered");
    assert!((spent - 0.02).abs() < 1e-12, "{spent}");
    assert_eq!(outcome.settlement.spend.qualifier, CostQualifier::Priced);

    let http = MockHttp::new().enqueue_ok(200, xai_image_answer());
    let result = settle(
        "nika:image_generate",
        "provider: xai, prompt: \"a small blue circle\"",
        &http,
        Some(0.01),
    )
    .await;
    let err = result.expect_err("the $0.02 floor exceeds a $0.01 cap");
    assert!(matches!(err, RuntimeError::BudgetFloor { .. }), "{err:?}");
    assert!(http.sent_requests().is_empty());
}

/// Mock media under any cap stays legal, unmetered and never paid.
#[tokio::test]
async fn mock_media_under_a_cap_is_unmetered() {
    for (tool, args) in [
        ("nika:tts_generate", "provider: mock, text: \"hi\""),
        ("nika:image_generate", "provider: mock, prompt: \"a star\""),
    ] {
        let http = MockHttp::new();
        let outcome = settle(tool, args, &http, Some(0.0))
            .await
            .expect("a rehearsal is admitted under a zero cap");
        assert!(outcome.ok, "{tool}: {:?}", outcome.records["media"].error);
        assert!(http.sent_requests().is_empty());
        assert_eq!(outcome.priced_calls, 0, "{tool}");
        assert_eq!(outcome.unpriced_calls, 0, "{tool}: never counted as paid");
        assert_eq!(
            outcome.settlement.spend.qualifier,
            CostQualifier::Unmetered,
            "{tool}"
        );
    }
}
