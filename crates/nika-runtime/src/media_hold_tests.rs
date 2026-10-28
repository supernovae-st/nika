// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! C-D3 · what a capped run's paid media calls spend when they run
//! together or fail after their provider billed, through the REAL parse →
//! check → run chain and the real builtin dispatcher. The endpoint is a
//! recording fake that receives each request at once and answers after the
//! request stayed pending once, as a socket does: the calls a fan-out
//! launches are in flight together.
//!
//! - every paid media call the cap admits holds its floor until it returns,
//!   so the items of a capped `for_each` cannot launch past the cap together
//!   (six $0.211 images under a $0.25 cap used to spend $1.26);
//! - a call dropped in flight (a `fail_fast` sibling) may have billed: it is
//!   counted unpriced and its floor stays consumed;
//! - a Gemini `n:` batch whose later request fails keeps the bill of the
//!   images already rendered, the run debits it, and a retry is judged
//!   against what is left (such a retry storm used to send nine requests
//!   and settle `unmetered`);
//! - a render that fails after its provider answered keeps its bill;
//! - once a paid call billed without a figure, a call billed only from the
//!   usage it reports is refused under the cap;
//! - a rendered speech text holding `${{` is billed per character, so the
//!   cap floors it like any other.

#![allow(clippy::expect_used, clippy::panic)]

use super::*;
use nika_event::settlement::CostQualifier;
use nika_kernel::http::{HttpError, HttpGetDyn, HttpRequest, HttpResponse, HttpStreamResponse};
use nika_kernel::secret::Secret;
use nika_kernel_mock::{MockFs, MockHttp, MockProvider, MockShell};

/// The recording fake behind a socket: the request is received at once
/// (`sent_requests` sees it), its answer arrives once the call stayed
/// pending one poll.
#[derive(Clone, Default)]
struct Pending(MockHttp);

impl Pending {
    fn answering(answers: Vec<(u16, Vec<u8>)>) -> Self {
        let mut http = MockHttp::new();
        for (status, body) in answers {
            http = http.enqueue_ok(status, body);
        }
        Self(http)
    }

    fn sent(&self) -> usize {
        self.0.sent_requests().len()
    }
}

impl HttpGetDyn for Pending {
    async fn get(&self, request: HttpRequest) -> Result<HttpResponse, HttpError> {
        let answer = self.0.get(request).await;
        tokio::task::yield_now().await;
        answer
    }
}

impl HttpPostDyn for Pending {
    async fn post(&self, request: HttpRequest) -> Result<HttpResponse, HttpError> {
        let answer = self.0.post(request).await;
        tokio::task::yield_now().await;
        answer
    }

    async fn send_streaming(&self, request: HttpRequest) -> Result<HttpStreamResponse, HttpError> {
        self.0.send_streaming(request).await
    }
}

type PendingDispatcher = nika_builtin::BuiltinDispatcher<
    MockFs,
    Pending,
    nika_kernel_mock::MockClock,
    nika_builtin::NullEmitter,
    nika_builtin::NonInteractive,
    nika_builtin::NoWorkflow,
>;

type PendingRuntime = Runtime<
    MockShell,
    PendingDispatcher,
    nika_providers::NoHttp,
    MockProvider,
    PendingDispatcher,
    nika_kernel_mock::MockClock,
>;

/// The real builtin dispatcher, its image and speech planes on `http` with a
/// key for every paid provider, under `budget`.
fn pending_runtime(http: &Pending, budget: Option<f64>) -> PendingRuntime {
    let key = || Secret::new("sk-fake-endpoint-only".to_owned());
    let dispatcher = Arc::new(
        nika_builtin::BuiltinDispatcher::new(
            Arc::new(MockFs::new()),
            Arc::new(http.clone()),
            Arc::new(nika_kernel_mock::MockClock::new()),
            Arc::new(nika_builtin::NullEmitter::default()),
            Arc::new(nika_builtin::NonInteractive::default()),
            Arc::new(nika_builtin::NoWorkflow::default()),
        )
        .with_image_plane(
            Arc::new(http.clone()),
            nika_builtin::ImageKeys::new()
                .with_openai(key())
                .with_gemini(key()),
        )
        .with_tts_plane(
            Arc::new(http.clone()),
            nika_builtin::TtsKeys::new().with_openai(key()),
        ),
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

/// Run `yaml` on `runtime`, the report checked clean first: the run's own
/// Result and its frames.
async fn run_yaml(
    runtime: PendingRuntime,
    yaml: &str,
) -> (Result<RunOutcome, RuntimeError>, VecSink) {
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
    let result = runtime.run(&wf, &report, &mut stamper, &mut sink).await;
    (result, sink)
}

/// A 1×1 PNG, the payload an image endpoint answers with.
const PNG_1X1_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

/// An `OpenAI` image answer: one image billed `output_tokens` image tokens
/// (10 text input tokens, split) · `usage: false` reports no usage.
fn openai_image(output_tokens: u64, usage: bool) -> Vec<u8> {
    let mut answer = serde_json::json!({ "created": 1, "data": [{ "b64_json": PNG_1X1_B64 }] });
    if usage && let Some(map) = answer.as_object_mut() {
        map.insert(
            "usage".into(),
            serde_json::json!({
                "input_tokens": 10, "output_tokens": output_tokens,
                "total_tokens": 10 + output_tokens,
                "input_tokens_details": { "text_tokens": 10, "image_tokens": 0 }
            }),
        );
    }
    answer.to_string().into_bytes()
}

/// One `OpenAI` image call's bill at the dated rates: 10 text input tokens
/// at $5 and `output_tokens` image output tokens at $30 per 1M.
#[allow(clippy::cast_precision_loss)] // fixture token counts
fn openai_bill(output_tokens: u64) -> f64 {
    10.0 * 5.0 / 1e6 + output_tokens as f64 * 30.0 / 1e6
}

/// A Gemini answer: one image part and its usage (20 input tokens, 1,120
/// image output tokens), billed $0.06721 at the dated rates.
fn gemini_image() -> Vec<u8> {
    serde_json::json!({
        "candidates": [{ "content": { "parts": [
            { "inlineData": { "mimeType": "image/png", "data": PNG_1X1_B64 } }
        ] } }],
        "usageMetadata": {
            "promptTokenCount": 20, "candidatesTokenCount": 1120, "totalTokenCount": 1140,
            "candidatesTokensDetails": [{ "modality": "IMAGE", "tokenCount": 1120 }]
        }
    })
    .to_string()
    .into_bytes()
}

const GEMINI_BILL: f64 = 20.0 * 0.5 / 1e6 + 1_120.0 * 60.0 / 1e6;

/// What a rate-limited endpoint answers.
fn rate_limited() -> (u16, Vec<u8>) {
    (
        429,
        br#"{"error":{"code":429,"message":"quota","status":"RESOURCE_EXHAUSTED"}}"#.to_vec(),
    )
}

fn approx(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-9,
        "expected {expected}, got {actual}"
    );
}

/// The item rows of the `media` fan-out's terminal frame: `status`, and
/// `code` and `message` on a failed item.
fn media_items(sink: &VecSink) -> Vec<serde_json::Value> {
    let terminal = (sink.events().iter())
        .rfind(|e| {
            matches!(
                e.kind,
                nika_event::EventKind::TaskCompleted | nika_event::EventKind::TaskFailed
            ) && e.str_field("task") == Some("media")
        })
        .expect("the media task settled");
    serde_json::from_str(terminal.str_field("items").expect("its items")).expect("a JSON array")
}

/// The `field` of the `media` task's terminal frame, as text.
fn media_detail(sink: &VecSink) -> String {
    (sink.events().iter())
        .rfind(|e| {
            e.kind == nika_event::EventKind::TaskFailed && e.str_field("task") == Some("media")
        })
        .and_then(|e| e.str_field("detail"))
        .expect("the media task failed")
        .to_owned()
}

/// A six-item fan-out over an expression (no launch floor) drawing one
/// gpt-image-2 image each at `quality` on 1024x1024, `fail_fast` as given.
fn six_images(quality: &str, fail_fast: bool) -> String {
    format!(
        "nika: c-d3-hold\nconst:\n  prompts: [\"a\", \"b\", \"c\", \"d\", \"e\", \"f\"]\npermits:\n  tools: [\"nika:image_generate\"]\n  fs: {{ write: [\"./out/**\"] }}\ntasks:\n  media:\n    for_each: {{ items: \"${{{{ const.prompts }}}}\", fail_fast: {fail_fast} }}\n    invoke:\n      tool: \"nika:image_generate\"\n      args: {{ provider: openai, size: 1024x1024, quality: {quality}, prompt: \"${{{{ item }}}}\", output_dir: \"./out\" }}\n"
    )
}

/// C-D3 · a `for_each` with no `max_parallel` launches every item
/// before any bill lands: each admitted image holds its $0.211 floor, so
/// under a $0.25 cap one request is sent and the five others are refused
/// NIKA-1704 before theirs (the run used to send six and spend $1.26).
#[tokio::test]
async fn a_capped_fan_out_holds_each_floor_until_its_call_returns() {
    let http = Pending::answering(vec![(200, openai_image(1_000, true)); 6]);
    let (result, sink) = run_yaml(
        pending_runtime(&http, Some(0.25)),
        &six_images("high", false),
    )
    .await;
    let outcome = result.expect("an expression fan-out has no launch floor");
    assert_eq!(http.sent(), 1, "one image in flight, the rest refused");
    assert!(!outcome.budget_exceeded, "the cap holds");
    approx(outcome.total_cost_usd.expect("metered"), openai_bill(1_000));
    assert_eq!((outcome.priced_calls, outcome.unpriced_calls), (1, 0));
    assert!(!outcome.ok, "the refused items fail the fan-out");
    let items = media_items(&sink);
    let refused: Vec<&serde_json::Value> = (items.iter())
        .filter(|row| row["status"] == "failed")
        .collect();
    assert_eq!(items.len(), 6);
    assert_eq!(
        items.iter().filter(|row| row["status"] == "ok").count(),
        1,
        "{items:?}"
    );
    assert_eq!(refused.len(), 5, "{items:?}");
    for row in refused {
        assert_eq!(row["code"], "NIKA-1704", "{row}");
        let message = row["message"].as_str().expect("why");
        assert!(
            message.contains("refused before the media request")
                && message.contains("costs at least $0.211000")
                && message.contains("$0.039000 left"),
            "{message}"
        );
    }
}

/// The same fan-out under `fail_fast`: the first refusal drops the image in
/// flight, which may have rendered and billed: it is counted unpriced, never
/// free, and the cap is never crossed.
#[tokio::test]
async fn a_media_call_dropped_in_flight_is_counted_unpriced() {
    let http = Pending::answering(vec![(200, openai_image(1_000, true)); 6]);
    let (result, _sink) = run_yaml(
        pending_runtime(&http, Some(0.25)),
        &six_images("high", true),
    )
    .await;
    let outcome = result.expect("an expression fan-out has no launch floor");
    assert_eq!(http.sent(), 1, "one request left before the refusal");
    assert!(!outcome.budget_exceeded);
    assert_eq!(
        outcome.unpriced_calls, 1,
        "the dropped call may have billed"
    );
    assert_eq!(outcome.total_cost_usd, None, "no fake zero");
    assert_eq!(
        outcome.settlement.spend.qualifier,
        CostQualifier::Unpriced,
        "never a free run"
    );
}

/// C-D3 · a Gemini `n: 3` batch whose third request is rate
/// limited already rendered and billed two images: the failure keeps their
/// bill, the run debits it, and the retry is judged against what is left
/// (its $0.201 floor exceeds the $0.116 the cap leaves), so three requests
/// are sent, never nine, and the run settles `priced`, never `unmetered`.
#[tokio::test]
async fn a_failed_gemini_batch_keeps_its_bill_and_bounds_its_retries() {
    let http = Pending::answering(vec![
        (200, gemini_image()),
        (200, gemini_image()),
        rate_limited(),
        (200, gemini_image()),
        (200, gemini_image()),
        rate_limited(),
    ]);
    let yaml = "nika: c-d3-retry\npermits:\n  tools: [\"nika:image_generate\"]\n  fs: { write: [\"./out/**\"] }\ntasks:\n  media:\n    retry: { max_attempts: 3 }\n    invoke:\n      tool: \"nika:image_generate\"\n      args: { provider: gemini, n: 3, prompt: \"a star\", output_dir: \"./out\" }\n";
    let (result, sink) = run_yaml(pending_runtime(&http, Some(0.25)), yaml).await;
    let outcome = result.expect("the $0.201 floor fits a $0.25 cap");
    assert_eq!(http.sent(), 3, "one attempt: two images and the 429");
    approx(
        outcome.total_cost_usd.expect("the billed images"),
        2.0 * GEMINI_BILL,
    );
    assert_eq!(outcome.settlement.spend.qualifier, CostQualifier::Priced);
    assert!(!outcome.budget_exceeded);
    let error = outcome.records["media"].error.as_ref().expect("failed");
    assert_eq!(error.code, "NIKA-1704", "the retry is refused: {error:?}");
    let failed = (sink.events().iter())
        .rfind(|e| e.kind == nika_event::EventKind::TaskFailed)
        .expect("the task failed");
    approx(
        failed
            .float_field("cost_usd")
            .expect("the frame carries the spend"),
        2.0 * GEMINI_BILL,
    );
    assert!(media_detail(&sink).starts_with("NIKA-1704"));
}

/// C-D3 · without a cap, a speech call and then a Gemini batch
/// whose second request fails: the run's spend is both bills, `priced`,
/// and the failed task's frame carries the image it billed.
#[tokio::test]
async fn a_failed_batch_bill_reaches_the_settlement_without_a_cap() {
    let http = Pending::answering(vec![
        (200, b"ID3\x04fake-mp3".to_vec()),
        (200, gemini_image()),
        rate_limited(),
    ]);
    let yaml = "nika: c-d3-p7\npermits:\n  tools: [\"nika:tts_generate\", \"nika:image_generate\"]\n  fs: { write: [\"./out/**\"] }\ntasks:\n  speak:\n    invoke:\n      tool: \"nika:tts_generate\"\n      args: { model: tts-1, text: \"Hello.\", output_dir: \"./out\" }\n  draw:\n    after: { speak: success }\n    invoke:\n      tool: \"nika:image_generate\"\n      args: { provider: gemini, n: 2, prompt: \"a star\", output_dir: \"./out\" }\n";
    let (result, _sink) = run_yaml(pending_runtime(&http, None), yaml).await;
    let outcome = result.expect("no cap");
    assert_eq!(http.sent(), 3);
    approx(
        outcome.total_cost_usd.expect("both bills"),
        15.0 * 6.0 / 1e6 + GEMINI_BILL,
    );
    assert_eq!((outcome.priced_calls, outcome.unpriced_calls), (2, 0));
    assert_eq!(outcome.settlement.spend.qualifier, CostQualifier::Priced);
}

/// C-D3 · a render that fails after its provider answered (a payload
/// that is no image) already billed: the failure keeps the bill.
#[tokio::test]
async fn a_render_that_fails_after_its_answer_keeps_its_bill() {
    let corrupt = serde_json::json!({
        "created": 1,
        "data": [{ "b64_json": "bm90IGFuIGltYWdl" }],
        "usage": { "input_tokens": 10, "output_tokens": 272, "total_tokens": 282,
                   "input_tokens_details": { "text_tokens": 10, "image_tokens": 0 } }
    });
    let http = Pending::answering(vec![(200, corrupt.to_string().into_bytes())]);
    let yaml = "nika: c-d3-decode\npermits:\n  tools: [\"nika:image_generate\"]\n  fs: { write: [\"./out/**\"] }\ntasks:\n  media:\n    invoke:\n      tool: \"nika:image_generate\"\n      args: { provider: openai, size: 1024x1024, quality: low, prompt: \"a star\", output_dir: \"./out\" }\n";
    let (result, _sink) = run_yaml(pending_runtime(&http, Some(1.0)), yaml).await;
    let outcome = result.expect("the floor fits");
    assert!(!outcome.ok, "the payload is no image");
    assert_eq!(http.sent(), 1);
    approx(outcome.total_cost_usd.expect("billed"), openai_bill(272));
    assert_eq!(outcome.settlement.spend.qualifier, CostQualifier::Priced);
}

/// C-D3 · a priced model billed only from the usage it reports
/// (GPT Image 2.5) whose response reports none: once it billed without a
/// figure, the cap no longer knows what it leaves, and the next such call is
/// refused before its request (five used to run, unseen by a $0.01 cap).
#[tokio::test]
async fn usage_billing_is_refused_once_a_paid_call_billed_without_a_figure() {
    let http = Pending::answering(vec![(200, openai_image(0, false)); 5]);
    let yaml = "nika: c-d3-unread\nconst:\n  prompts: [\"a\", \"b\", \"c\", \"d\", \"e\"]\npermits:\n  tools: [\"nika:image_generate\"]\n  fs: { write: [\"./out/**\"] }\ntasks:\n  media:\n    for_each: { items: \"${{ const.prompts }}\", max_parallel: 1, fail_fast: false }\n    invoke:\n      tool: \"nika:image_generate\"\n      args: { model: gpt-image-2.5-flare, prompt: \"${{ item }}\", output_dir: \"./out\" }\n";
    let (result, _sink) = run_yaml(pending_runtime(&http, Some(0.01)), yaml).await;
    let outcome = result.expect("a metered model fits a cap above zero");
    assert_eq!(http.sent(), 1, "the four others are refused");
    assert_eq!(outcome.unpriced_calls, 1);
    let error = outcome.records["media"]
        .error
        .as_ref()
        .expect("refused items");
    assert!(
        error.code == "NIKA-1704" && error.message.contains("billed without a figure"),
        "{error:?}"
    );
}

/// C-D3 · a speech text the run renders to words holding `${{`
/// is billed per character all the same: 3,045 characters of `tts-1` cost
/// $0.0457, so a $0.001 cap refuses the call before its request (it used to
/// pass the guard and bill 45 times the cap).
#[tokio::test]
async fn a_rendered_text_holding_a_template_marker_meets_the_cap() {
    let http = Pending::answering(vec![(200, b"ID3\x04fake-mp3".to_vec())]);
    let yaml = "nika: c-d3-marker\ninputs:\n  script: { type: string }\npermits:\n  tools: [\"nika:tts_generate\"]\n  fs: { write: [\"./out/**\"] }\ntasks:\n  media:\n    invoke:\n      tool: \"nika:tts_generate\"\n      args: { model: tts-1, text: \"${{ inputs.script }}\", output_dir: \"./out\" }\n";
    let script = format!(
        "Write ${{{{ secrets.token }}}} in your workflow. {}",
        "x".repeat(3000)
    );
    let runtime = pending_runtime(&http, Some(0.001)).with_var_overrides(BTreeMap::from([(
        "script".to_owned(),
        Value::String(script),
    )]));
    let (result, _sink) = run_yaml(runtime, yaml).await;
    let outcome = result.expect("a templated text is judged at dispatch");
    assert_eq!(http.sent(), 0, "refused before its request");
    let error = outcome.records["media"].error.as_ref().expect("refused");
    assert_eq!(error.code, "NIKA-1704");
    assert!(
        error.message.contains("costs at least $0.0456"),
        "{}",
        error.message
    );
}
