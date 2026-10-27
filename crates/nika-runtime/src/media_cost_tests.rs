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
//! - mock media under a cap stays unmetered and is never counted as paid;
//! - an `agent:` loop's paid media tool calls meet the same cap before their
//!   request, and a returned unpriced one is counted by the settlement;
//! - a returned media call that billed with no report, or whose output fails
//!   its `returns:` contract, still names why its spend is unpriced;
//! - the keyless `system` voice bills no provider: a capped run admits it,
//!   and it settles as unpriced local work (`local_model`), never free.

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
    media_runtime_with(http, budget, MockProvider::new("mock"))
}

/// [`media_runtime`] whose `agent:` loop asks `agent` for its turns.
fn media_runtime_with(http: &MockHttp, budget: Option<f64>, agent: MockProvider) -> MediaRuntime {
    media_runtime_voiced(http, budget, agent, None)
}

/// [`media_runtime_with`] whose speech plane also has the keyless `system`
/// provider's host synthesizer, `voice`.
fn media_runtime_voiced(
    http: &MockHttp,
    budget: Option<f64>,
    agent: MockProvider,
    voice: Option<nika_builtin::SystemVoice>,
) -> MediaRuntime {
    let images = nika_builtin::ImageKeys::new()
        .with_openai(key())
        .with_gemini(key())
        .with_xai(key());
    let speech = nika_builtin::TtsKeys::new()
        .with_openai(key())
        .with_elevenlabs(key());
    let mut dispatcher = nika_builtin::BuiltinDispatcher::new(
        Arc::new(MockFs::new()),
        Arc::new(http.clone()),
        Arc::new(nika_kernel_mock::MockClock::new()),
        Arc::new(nika_builtin::NullEmitter::default()),
        Arc::new(nika_builtin::NonInteractive::default()),
        Arc::new(nika_builtin::NoWorkflow::default()),
    )
    .with_image_plane(Arc::new(http.clone()), images)
    .with_tts_plane(Arc::new(http.clone()), speech);
    if let Some(voice) = voice {
        dispatcher = dispatcher.with_system_voice(voice);
    }
    let dispatcher = Arc::new(dispatcher);
    let invoke = Arc::new(InvokeVerb::new(Arc::clone(&dispatcher)));
    let registry = Arc::new(nika_providers::ProviderRegistry::without_http(
        nika_providers::ProvidersConfig::default(),
    ));
    Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new())),
        Arc::clone(&invoke),
        InferVerb::new(registry, "mock/echo"),
        AgentVerb::new(
            Arc::new(agent),
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
    settle_with(tool, args, "", http, budget).await.0
}

/// [`settle`] with `header` (task keys such as `returns:`) on the media task:
/// the run's own Result and its frames.
async fn settle_with(
    tool: &str,
    args: &str,
    header: &str,
    http: &MockHttp,
    budget: Option<f64>,
) -> (Result<RunOutcome, RuntimeError>, VecSink) {
    settle_on(media_runtime(http, budget), tool, args, header).await
}

/// One media task (`tool` with `args`, `header` on the task) run by
/// `runtime`, the report checked clean first: the run's own Result and its
/// frames.
async fn settle_on(
    runtime: MediaRuntime,
    tool: &str,
    args: &str,
    header: &str,
) -> (Result<RunOutcome, RuntimeError>, VecSink) {
    let yaml = format!(
        "nika: c-d3\ninputs:\n  provider: {{ type: string, default: openai }}\npermits:\n  tools: [\"{tool}\"]\n  fs: {{ write: [\"./out/**\"] }}\ntasks:\n  media:\n{header}    invoke:\n      tool: \"{tool}\"\n      args: {{ {args}, output_dir: \"./out\" }}\noutputs:\n  media: ${{{{ tasks.media.output }}}}\n"
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
    let result = runtime.run(&wf, &report, &mut stamper, &mut sink).await;
    (result, sink)
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
        text.contains("the keyless `system` voice for speech"),
        "the remedy a capped beat can follow names the keyless voice: {text}"
    );
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

/// A scripted agent turn: one `nika:image_generate` call on `provider`, then
/// a final answer. `priced` makes each turn an observed, priced inference
/// call (a priced model's loop).
fn drawing_agent(provider: &str, priced: bool) -> MockProvider {
    use nika_kernel::ai::provider::{ContentBlock, InferResponse, StopReason, TokenUsage};
    let observed = |mut response: InferResponse| {
        if priced {
            let mut call = nika_types::cost::InferenceCall::new();
            call.route = Some(nika_types::cost::InferenceRoute::new(
                "fixture".into(),
                "selected".into(),
                "https://one.example/v1/chat/completions".into(),
            ));
            call.response_model = Some("selected".into());
            call.usage = Some(response.usage.clone());
            call.usage_complete = true;
            call.pricing = Some(r#"{"kind":"test_fixture"}"#.into());
            call.estimated_usd = Some(nika_types::cost::Cost::from_micro_usd(10));
            response.inference_calls = vec![call];
        }
        response
    };
    let draw = InferResponse::new(
        vec![ContentBlock::ToolUse {
            id: "draw-1".into(),
            name: "nika:image_generate".into(),
            input: serde_json::json!({
                "provider": provider, "prompt": "a small blue circle", "output_dir": "./out"
            }),
        }],
        TokenUsage::default(),
        StopReason::ToolUse,
    );
    let done = InferResponse::new(
        vec![ContentBlock::Text {
            text: "done".into(),
        }],
        TokenUsage::default(),
        StopReason::EndTurn,
    );
    MockProvider::new("mock")
        .enqueue_response(observed(draw))
        .enqueue_response(observed(done))
}

/// One `agent:` task granted `nika:image_generate`, run under `budget`: the
/// run's own Result and its frames.
async fn settle_agent(
    agent: MockProvider,
    http: &MockHttp,
    budget: Option<f64>,
) -> (Result<RunOutcome, RuntimeError>, VecSink) {
    let yaml = "nika: c-d3-agent\nmodel: mock/echo\npermits:\n  tools: [\"nika:image_generate\"]\n  fs: { write: [\"./out/**\"] }\ntasks:\n  draw:\n    agent:\n      prompt: \"draw a small blue circle\"\n      tools: [\"nika:image_generate\"]\n";
    let wf = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "fixture checks clean: {report:#?}");
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let result = media_runtime_with(http, budget, agent)
        .run(&wf, &report, &mut stamper, &mut sink)
        .await;
    (result, sink)
}

/// The bypass the C-D3 review demonstrated: an `agent:` granted a paid media
/// builtin reached the openai endpoint under `--max-cost-usd`. The loop now
/// judges each media tool call against the cap before its request: the
/// unpriced call is refused (fed back to the model), nothing is sent.
#[tokio::test]
async fn an_agents_unpriced_media_call_under_a_cap_is_refused_before_its_request() {
    for provider in ["openai", "gemini"] {
        let http = MockHttp::new().enqueue_ok(200, openai_image_answer());
        let agent = drawing_agent(provider, true);
        let outcome = settle_agent(agent.clone(), &http, Some(5.0))
            .await
            .0
            .expect("the run starts");
        assert!(
            http.sent_requests().is_empty(),
            "{provider}: no media request left the loop"
        );
        let requests = agent.captured_requests();
        let fed_back = requests
            .last()
            .expect("the loop asked again")
            .messages
            .iter()
            .flat_map(|m| m.content.iter())
            .find_map(|block| match block {
                nika_kernel::ai::provider::ContentBlock::ToolResult {
                    content,
                    is_error: true,
                    ..
                } => Some(content.clone()),
                _ => None,
            })
            .expect("the refusal reached the model");
        assert!(
            fed_back.starts_with("NIKA-1704") && fed_back.contains("no catalog price"),
            "{provider}: {fed_back}"
        );
        assert!(
            outcome.ok,
            "{provider}: {:?}",
            outcome.records["draw"].error
        );
        assert_eq!(
            (outcome.priced_calls, outcome.unpriced_calls),
            (2, 0),
            "{provider}: two priced turns, and the refused call billed nothing"
        );
    }
}

/// Without a cap the agent's openai image is sent, and the settlement
/// counts it: a priced loop plus an unpriced media call settles
/// `partially_priced`, never `priced` (the review's `✔ $0.0600` loop).
#[tokio::test]
async fn an_agents_returned_unpriced_media_call_is_counted_by_the_settlement() {
    let http = MockHttp::new().enqueue_ok(200, openai_image_answer());
    let (result, sink) = settle_agent(drawing_agent("openai", true), &http, None).await;
    let outcome = result.expect("no cap, no refusal");
    assert!(outcome.ok, "{:?}", outcome.records["draw"].error);
    assert_eq!(http.sent_requests().len(), 1, "one paid media request");
    assert_eq!((outcome.priced_calls, outcome.unpriced_calls), (2, 1));
    assert_eq!(
        outcome.settlement.spend.qualifier,
        CostQualifier::PartiallyPriced,
        "the loop's priced turns never hide the unpriced image"
    );
    let completed = (sink.events().iter())
        .find(|e| e.kind == nika_event::EventKind::TaskCompleted)
        .expect("the agent task completed");
    assert_eq!(
        completed.str_field("cost_unpriced"),
        Some("missing_catalog_price"),
        "the frame names why part of the task's spend is missing"
    );
}

/// The media task's terminal frame field `key`.
fn media_frame_field<'a>(sink: &'a VecSink, key: &str) -> Option<&'a str> {
    (sink.events().iter())
        .find(|e| {
            matches!(
                e.kind,
                nika_event::EventKind::TaskCompleted | nika_event::EventKind::TaskFailed
            ) && e.str_field("task") == Some("media")
        })
        .and_then(|e| e.str_field(key))
}

/// A priced provider (xAI) whose answer reports no spend billed all the
/// same: the call is counted unpriced (`provider_did_not_report_usage`),
/// never a free `unmetered` run.
#[tokio::test]
async fn a_priced_media_call_that_reports_no_spend_settles_unpriced() {
    let answer = serde_json::json!({ "data": [{ "b64_json": PNG_1X1_B64, "revised_prompt": "" }] });
    let http = MockHttp::new().enqueue_ok(200, answer.to_string().into_bytes());
    let (result, sink) = settle_with(
        "nika:image_generate",
        "provider: xai, prompt: \"a small blue circle\"",
        "",
        &http,
        None,
    )
    .await;
    let outcome = result.expect("no cap");
    assert!(outcome.ok, "{:?}", outcome.records["media"].error);
    assert_eq!(http.sent_requests().len(), 1);
    assert_eq!((outcome.priced_calls, outcome.unpriced_calls), (0, 1));
    assert_eq!(outcome.settlement.spend.qualifier, CostQualifier::Unpriced);
    assert_eq!(
        media_frame_field(&sink, "cost_unpriced"),
        Some("provider_did_not_report_usage")
    );
}

/// A returned paid media call whose output fails its `returns:` contract
/// already billed: the failure keeps why its spend is unpriced, and the
/// ledger counts it.
#[tokio::test]
async fn a_contract_rejected_media_call_keeps_its_unpriced_spend() {
    let http = MockHttp::new().enqueue_ok(200, openai_image_answer());
    let (result, sink) = settle_with(
        "nika:image_generate",
        "provider: openai, prompt: \"a small blue circle\"",
        "    returns: integer\n",
        &http,
        None,
    )
    .await;
    let outcome = result.expect("no cap");
    assert!(!outcome.ok, "an object never fits `integer`");
    let error = outcome.records["media"]
        .error
        .as_ref()
        .expect("the rejection");
    assert_eq!(error.code, "NIKA-TYPE-101");
    assert_eq!(
        http.sent_requests().len(),
        1,
        "the call fired before the judgment"
    );
    assert_eq!(outcome.unpriced_calls, 1);
    assert_eq!(
        media_frame_field(&sink, "cost_unpriced"),
        Some("missing_catalog_price")
    );
}

/// A WAV the fake host synthesizer streams: 16-bit mono PCM at 22.05 kHz, a
/// tenth of a second of silence (the builtin judges the bytes' format only).
fn host_wav() -> Vec<u8> {
    let pcm = vec![0u8; 4_410];
    let data_len = u32::try_from(pcm.len()).expect("a tenth of a second");
    let mut wav = Vec::with_capacity(44 + pcm.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&1u16.to_le_bytes()); // mono
    wav.extend_from_slice(&22_050u32.to_le_bytes());
    wav.extend_from_slice(&44_100u32.to_le_bytes()); // byte rate
    wav.extend_from_slice(&2u16.to_le_bytes()); // block align
    wav.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    wav.extend_from_slice(&pcm);
    wav
}

/// The keyless `system` voice bills no provider, so no cap refuses it (a
/// beat keeps speech under its `plafond:` this way), yet its local compute
/// is unpriced, never free: a capped run, a zero cap included, starts the
/// host synthesizer once, sends no provider request and settles `unpriced`
/// with `local_model` on the task's frame, as a run with no cap does.
#[tokio::test]
async fn the_keyless_system_voice_runs_under_a_cap_and_settles_as_unpriced_local_work() {
    for cap in [Some(0.0), Some(1.0), None] {
        let http = MockHttp::new();
        let synth = MockShell::new().enqueue_result(
            nika_kernel::process::ShellResult::new(0, "", "", std::time::Duration::ZERO)
                .with_raw(host_wav(), Vec::new()),
        );
        let voice = nika_builtin::SystemVoice::new(
            Arc::new(synth.clone()),
            nika_builtin::SystemSynth::espeak_ng("/usr/bin/espeak-ng"),
        );
        let runtime = media_runtime_voiced(&http, cap, MockProvider::new("mock"), Some(voice));
        let (result, sink) = settle_on(
            runtime,
            "nika:tts_generate",
            "provider: system, text: \"Budget zero.\"",
            "",
        )
        .await;
        let outcome = result.expect("no cap refuses a voice that bills no provider");
        assert!(outcome.ok, "{cap:?}: {:?}", outcome.records["media"].error);
        let started = synth.executed_commands();
        assert_eq!(started.len(), 1, "{cap:?}: the host synthesizer spoke once");
        assert_eq!(started[0].program, "/usr/bin/espeak-ng", "{cap:?}");
        assert!(
            http.sent_requests().is_empty(),
            "{cap:?}: no provider request"
        );
        assert_eq!(
            (outcome.priced_calls, outcome.unpriced_calls),
            (0, 1),
            "{cap:?}: one unpriced local call"
        );
        assert_eq!(outcome.total_cost_usd, None, "{cap:?}: never a priced zero");
        assert_eq!(
            outcome.settlement.spend.qualifier,
            CostQualifier::Unpriced,
            "{cap:?}: local work is unpriced, never unmetered"
        );
        assert_eq!(
            media_frame_field(&sink, "cost_unpriced"),
            Some("local_model"),
            "{cap:?}: the frame names why the spend is absent"
        );
    }
}
