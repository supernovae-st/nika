use super::*;
use nika_providers::ProvidersConfig;
use serde_json::json;

fn mock_verb() -> InferVerb {
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    InferVerb::new(registry, "mock/echo")
}

#[tokio::test]
async fn plain_text_round_trip() {
    let out = mock_verb()
        .run(InferInput::new("Say hello."))
        .await
        .expect("mock infer succeeds");
    // The mock echoes the last user message, prefixed.
    match &out.output {
        InferValue::Text(text) => {
            assert!(
                text.contains("Say hello."),
                "echo carries the prompt: {text}"
            );
        }
        other => panic!("expected text output, got {other:?}"),
    }
    assert_eq!(out.model_resolved, "mock/echo");
    assert!(out.usage.output_tokens > 0, "mock reports usage");
}

#[tokio::test]
async fn per_task_model_override_wins() {
    let mut input = InferInput::new("ping");
    input.model = Some("mock/other".to_owned());
    let out = mock_verb().run(input).await.expect("mock resolves");
    assert_eq!(out.model_resolved, "mock/other");
}

#[tokio::test]
async fn empty_prompt_is_rejected_before_any_call() {
    let err = mock_verb()
        .run(InferInput::new("   "))
        .await
        .expect_err("empty prompt rejected");
    assert!(matches!(
        err,
        VerbInferError::InvalidParam {
            param: "prompt",
            ..
        }
    ));
}

#[tokio::test]
async fn out_of_range_temperature_is_rejected() {
    let mut input = InferInput::new("hi");
    input.temperature = Some(3.5);
    let err = mock_verb().run(input).await.expect_err("temp rejected");
    assert!(matches!(
        err,
        VerbInferError::InvalidParam {
            param: "temperature",
            ..
        }
    ));
}

#[tokio::test]
async fn unknown_provider_is_a_resolution_error() {
    let mut input = InferInput::new("hi");
    input.model = Some("ghost/model".to_owned());
    let err = mock_verb().run(input).await.expect_err("ghost rejected");
    assert!(matches!(err, VerbInferError::ModelResolution { .. }));
}

#[tokio::test]
async fn structured_mock_synthesizes_a_conformant_instance() {
    // F3: mock + `schema:` returns a SYNTHESIZED conformant instance
    // (the echo could never satisfy a schema — every structured
    // workflow on mock/echo died NIKA-INFER-002 · no offline CI).
    let mut input = InferInput::new("extract the person");
    input.schema = Some(json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" },
            "age": { "type": "integer", "minimum": 0 }
        },
        "required": ["name", "age"]
    }));
    let out = mock_verb().run(input).await.expect("valid structured");
    match out.output {
        InferValue::Structured(v) => {
            assert_eq!(v["name"], "mock");
            assert_eq!(v["age"], 0);
        }
        other => panic!("expected structured, got {other:?}"),
    }
}

#[tokio::test]
async fn structured_mock_handles_atlas_style_schemas() {
    // The field-report class (payload-review · geo-audit): enum
    // severity + bounded integers + arrays of typed objects must
    // dry-run green offline — the F3 acceptance shape.
    let mut input = InferInput::new("review the payload");
    input.schema = Some(json!({
        "type": "object",
        "required": ["verdict", "score", "findings"],
        "properties": {
            "verdict": { "type": "string", "enum": ["P0", "P1", "P2", "P3"] },
            "score": { "type": "integer", "minimum": 0, "maximum": 12 },
            "findings": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["severity", "detail"],
                    "properties": {
                        "severity": { "type": "string", "enum": ["P0", "P1"] },
                        "detail": { "type": "string" }
                    }
                }
            }
        }
    }));
    let out = mock_verb().run(input).await.expect("dry-runs offline");
    match out.output {
        InferValue::Structured(v) => {
            assert_eq!(v["verdict"], "P0", "enum → first entry");
            assert_eq!(v["score"], 0, "bounded integer → minimum");
            assert_eq!(v["findings"][0]["severity"], "P0");
        }
        other => panic!("expected structured, got {other:?}"),
    }
}

#[tokio::test]
async fn schema_retry_exhaustion_reports_attempts() {
    // A `pattern` is outside the mock generator's vocabulary — the
    // synthesized "mock" never validates → budget exhausted (the
    // retry loop itself stays covered post-F3).
    let mut input = InferInput::new("give me a year");
    input.schema = Some(json!({ "type": "string", "pattern": "^\\d{4}$" }));
    let err = mock_verb().run(input).await.expect_err("never validates");
    match err {
        VerbInferError::SchemaValidation { attempts, .. } => {
            // initial call + DEFAULT_SCHEMA_RETRY_BUDGET retries
            assert_eq!(attempts, 1 + u32::from(DEFAULT_SCHEMA_RETRY_BUDGET));
        }
        other => panic!("expected SchemaValidation, got {other:?}"),
    }
}

#[tokio::test]
async fn zero_retry_budget_is_single_shot() {
    let mut input = InferInput::new("give me a year");
    input.schema = Some(json!({ "type": "string", "pattern": "^\\d{4}$" }));
    let err = mock_verb()
        .with_schema_retry_budget(0)
        .run(input)
        .await
        .expect_err("single shot fails");
    assert!(matches!(
        err,
        VerbInferError::SchemaValidation { attempts: 1, .. }
    ));
}

#[tokio::test]
async fn invalid_schema_is_rejected_before_any_call() {
    // A schema that doesn't compile is a task-authoring error: NIKA-432
    // with ZERO provider round-trips (review lenses 1+3).
    let mut input = InferInput::new("hi");
    input.schema = Some(json!({ "type": "definitely-not-a-type" }));
    let err = mock_verb().run(input).await.expect_err("schema rejected");
    assert!(matches!(
        err,
        VerbInferError::InvalidParam {
            param: "schema",
            ..
        }
    ));
}

#[test]
fn oversized_schema_render_is_capped() {
    let huge = json!({
        "type": "object",
        "description": "x".repeat(20_000),
    });
    let rendered = crate::structured::render_schema(&huge);
    assert!(rendered.len() < 5_000, "render capped: {}", rendered.len());
    assert!(rendered.ends_with("…(schema truncated)"));

    // Boundary: a render of EXACTLY the cap stays untouched (> not >=).
    let at_cap = json!("y".repeat(4096 - 2)); // 2 quotes in the render
    let exact = crate::structured::render_schema(&at_cap);
    assert_eq!(exact.len(), 4096);
    assert!(!exact.contains("truncated"));

    // A multibyte char straddling the cap: render = quote + 4094 z + é,
    // so byte 4096 lands MID-é and the cut must walk BACK to 4095
    // (never forward past the cap).
    let multibyte = json!(format!("{}éé", "z".repeat(4094)));
    let cut = crate::structured::render_schema(&multibyte);
    assert!(cut.ends_with("…(schema truncated)"));
    let body = cut.trim_end_matches("…(schema truncated)");
    assert!(
        body.len() <= 4096,
        "cut never exceeds the cap: {}",
        body.len()
    );
    assert!(body.is_char_boundary(body.len()));
}

#[test]
fn system_prompt_lands_first() {
    let mut input = InferInput::new("question");
    input.system = Some("you are terse".to_owned());
    let messages = base_messages(&input, SchemaWire::None);
    assert_eq!(messages.len(), 2);
    assert!(matches!(messages[0].role, Role::System));
    assert!(matches!(messages[1].role, Role::User));
}

#[test]
fn instruction_rides_the_prompt_on_both_fallback_wires() {
    let mut input = InferInput::new("question");
    input.schema = Some(json!({ "type": "object" }));
    let text_of = |m: &Message| match &m.content[0] {
        ContentBlock::Text { text } => text.clone(),
        _ => String::new(),
    };
    // Strict: the schema travels natively — the prompt stays clean.
    let native = base_messages(&input, SchemaWire::Strict);
    assert_eq!(text_of(&native[0]), "question");
    // Both fallbacks steer through the prompt (JSON mode only promises
    // JSON, not the shape; instruction-only promises nothing).
    for wire in [SchemaWire::JsonMode, SchemaWire::Instruction] {
        let fallback = base_messages(&input, wire);
        assert!(text_of(&fallback[0]).contains("JSON Schema"), "{wire:?}");
    }
}

// ── F2 · the schema-wire decision (ADR-098) ──────────────────────

/// The decision table: underspecified + strict wire → JSON mode;
/// fully-specified keeps today's strict path; no native support →
/// instruction; no schema → none.
#[test]
fn schema_wire_decision_table() {
    let plain = InferInput::new("q");
    assert_eq!(schema_wire(&plain, true, true), SchemaWire::None);

    let mut under = InferInput::new("q");
    under.schema = Some(json!({ "type": "object" }));
    assert_eq!(schema_wire(&under, true, true), SchemaWire::JsonMode);
    // A wire whose strict mode accepts anything (mock) keeps Strict —
    // the F3 offline synthesis depends on receiving the schema.
    assert_eq!(schema_wire(&under, true, false), SchemaWire::Strict);
    assert_eq!(schema_wire(&under, false, false), SchemaWire::Instruction);

    let mut full = InferInput::new("q");
    full.schema = Some(json!({
        "type": "object",
        "properties": { "name": { "type": "string" } },
        "required": ["name"]
    }));
    assert_eq!(schema_wire(&full, true, true), SchemaWire::Strict);
}

/// Gate 10 PARITY — pins the request-shaping behaviors the brouillon
/// verb established (`git show brouillon:tools/nika-verb-infer/src/lib.rs`
/// · read-only reference · CRAFT rewrite): system prompt becomes the
/// System message, the prompt lands verbatim as a single user Text
/// block, sampling params pass through untouched, and the response
/// text is the concatenation of Text blocks only.
#[tokio::test]
async fn gate10_parity_request_shaping_vs_brouillon() {
    let mut input = InferInput::new("What is the capital of France?");
    input.system = Some("You are terse.".to_owned());
    input.temperature = Some(0.3);
    input.max_tokens = Some(128);
    let messages = base_messages(&input, SchemaWire::None);
    // Brouillon shape: [System, User] · prompt verbatim · single block.
    assert_eq!(messages.len(), 2);
    assert!(matches!(messages[0].role, Role::System));
    assert_eq!(messages[1].content.len(), 1);
    assert!(matches!(
        &messages[1].content[0],
        ContentBlock::Text { text } if text == "What is the capital of France?"
    ));
    let req = build_request(&input, "echo", messages, SchemaWire::None);
    assert_eq!(req.temperature, Some(0.3));
    assert_eq!(req.max_tokens, Some(128));
    // End-to-end on the deterministic mock: echo carries the prompt,
    // usage is word-count arithmetic (brouillon mock contract).
    let out = mock_verb()
        .run(input)
        .await
        .expect("parity round-trip succeeds");
    match &out.output {
        InferValue::Text(t) => assert!(t.contains("capital of France")),
        other => panic!("expected text, got {other:?}"),
    }
}

#[test]
fn request_carries_the_task_timeout() {
    // F1: the task `timeout:` must reach the provider transport
    // deadline — unset stays None (the adapter's per-provider
    // default governs).
    let budget = std::time::Duration::from_secs(420);
    let mut input = InferInput::new("q");
    input.timeout = Some(budget);
    let req = build_request(
        &input,
        "m",
        base_messages(&input, SchemaWire::None),
        SchemaWire::None,
    );
    assert_eq!(req.timeout, Some(budget));

    let unset = InferInput::new("q");
    let req = build_request(
        &unset,
        "m",
        base_messages(&unset, SchemaWire::None),
        SchemaWire::None,
    );
    assert_eq!(req.timeout, None, "no budget → adapter default governs");
}

#[test]
fn thinking_budget_reaches_the_infer_request() {
    // #1135 sibling: `thinking.budget_tokens` used to die at dispatch and
    // never reach InferRequest even when InferInput already had the field.
    let mut input = InferInput::new("q");
    input.thinking_budget = Some(2048);
    let req = build_request(
        &input,
        "m",
        base_messages(&input, SchemaWire::None),
        SchemaWire::None,
    );
    assert_eq!(req.thinking_budget, Some(2048));
}

// ── F2 · the adapter-path proof (the http seam mocked) ───────────

use nika_kernel::http::{HttpError, HttpRequest, HttpResponse, HttpStreamResponse};
use nika_kernel::secret::Secret;
use nika_providers::ProviderRegistry as Registry;

/// A canned-response http seam: serves queued JSON bodies · captures
/// every request it saw (the dividend of the kernel http seam — the
/// real openai adapter runs with zero network).
struct SeamHttp {
    responses: std::sync::Mutex<std::collections::VecDeque<String>>,
    captured: std::sync::Mutex<Vec<HttpRequest>>,
}

impl SeamHttp {
    fn with_json(bodies: &[&str]) -> Arc<Self> {
        Arc::new(Self {
            responses: std::sync::Mutex::new(bodies.iter().map(|b| (*b).to_owned()).collect()),
            captured: std::sync::Mutex::new(Vec::new()),
        })
    }

    fn captured(&self) -> Vec<HttpRequest> {
        self.captured.lock().expect("seam lock").clone()
    }
}

impl HttpPostDyn for SeamHttp {
    async fn post(&self, request: HttpRequest) -> Result<HttpResponse, HttpError> {
        self.captured
            .lock()
            .expect("seam lock")
            .push(request.clone());
        let body = self
            .responses
            .lock()
            .expect("seam lock")
            .pop_front()
            .ok_or_else(|| HttpError::Other {
                reason: "SeamHttp: no canned response queued".to_owned(),
            })?;
        Ok(HttpResponse::new(
            200,
            std::collections::BTreeMap::new(),
            bytes::Bytes::from(body),
            request.url,
        ))
    }

    async fn send_streaming(&self, _request: HttpRequest) -> Result<HttpStreamResponse, HttpError> {
        Err(HttpError::Other {
            reason: "SeamHttp: streaming not modelled".to_owned(),
        })
    }
}

fn openai_verb(seam: &Arc<SeamHttp>) -> InferVerb<SeamHttp> {
    let registry = Registry::new(
        Arc::clone(seam),
        ProvidersConfig::new().with_key("openai", Secret::new("sk-test")),
    );
    InferVerb::new(Arc::new(registry), "openai/gpt-4o-mini")
}

/// An ollama-routed verb whose endpoint is a real loopback address —
/// the B-5 gate dials THAT (a real `TcpStream`), while the adapter's
/// own wire stays on the canned seam.
fn ollama_verb(seam: &Arc<SeamHttp>, base_url: String) -> InferVerb<SeamHttp> {
    let registry = Registry::new(
        Arc::clone(seam),
        ProvidersConfig::new().with_base_url("ollama", base_url),
    );
    InferVerb::new(Arc::new(registry), "ollama/qwen3.5:4b")
}

/// A one-shot loopback server: `speak` answers a bare 404 (a live
/// engine), `!speak` holds the socket mute (the gauntlet hang).
#[allow(clippy::disallowed_methods)] // test seam — the probe's own worker pattern
fn spawn_stub_server(speak: bool) -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    std::thread::spawn(move || {
        while let Ok((mut stream, _)) = listener.accept() {
            std::thread::spawn(move || {
                if speak {
                    use std::io::Write as _;
                    let _ = stream.write_all(b"HTTP/1.0 404 Not Found\r\n\r\n");
                } else {
                    std::thread::sleep(std::time::Duration::from_secs(30));
                    drop(stream);
                }
            });
        }
    });
    port
}

/// B-5: a silent local endpoint refuses in milliseconds — BEFORE any
/// wire call — naming the cause and the exits (the gauntlet's
/// « still running » until kill).
#[tokio::test]
async fn a_silent_local_endpoint_refuses_before_any_wire_call() {
    let dead = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        drop(listener);
        port
    };
    let seam = SeamHttp::with_json(&[]);
    let verb = ollama_verb(&seam, format!("http://127.0.0.1:{dead}"));
    let started = std::time::Instant::now();
    let err = verb
        .run(InferInput::new("hi"))
        .await
        .expect_err("the gate refuses a silent endpoint");
    let text = err.to_string();
    assert!(text.contains("no server listening"), "{text}");
    assert!(text.contains(&format!("127.0.0.1:{dead}")), "{text}");
    assert!(
        text.contains("mock/echo"),
        "the keyless exit is named · {text}"
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "a refusal, never a hang"
    );
    assert!(
        seam.captured().is_empty(),
        "zero wire calls — the gate fired before the transport"
    );
}

/// B-5's edge: a MUTE server (accepts, never speaks) is named as a
/// stuck server — a connect-only probe would pass it and hang.
#[tokio::test]
async fn a_mute_local_endpoint_names_the_stuck_server() {
    let port = spawn_stub_server(false);
    let seam = SeamHttp::with_json(&[]);
    let verb = ollama_verb(&seam, format!("http://127.0.0.1:{port}"));
    let started = std::time::Instant::now();
    let err = verb
        .run(InferInput::new("hi"))
        .await
        .expect_err("the gate refuses a mute endpoint");
    let text = err.to_string();
    assert!(text.contains("never answers HTTP"), "{text}");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "≤ the speak cap, never the old forever"
    );
    assert!(seam.captured().is_empty(), "still zero wire calls");
}

/// The gate's non-regression: a LIVE local engine passes and the one
/// real call goes through (the probe costs milliseconds).
#[tokio::test]
async fn a_live_local_endpoint_passes_the_gate() {
    let port = spawn_stub_server(true);
    let seam = SeamHttp::with_json(&[
        r#"{"choices":[{"message":{"content":"ok"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1}}"#,
    ]);
    let verb = ollama_verb(&seam, format!("http://127.0.0.1:{port}"));
    let out = verb
        .run(InferInput::new("hi"))
        .await
        .expect("a live server clears the gate");
    assert!(matches!(out.output, InferValue::Text(_)));
    assert_eq!(seam.captured().len(), 1, "the one real call went through");
}

/// The captured wire body of the seam's one request.
fn wire_body(seam: &Arc<SeamHttp>) -> serde_json::Value {
    let captured = seam.captured();
    assert_eq!(captured.len(), 1, "one round-trip");
    serde_json::from_slice(captured[0].body.as_ref().expect("a POST body"))
        .expect("the wire body is JSON")
}

/// F2 acceptance: `{type: object}` — the field repro that 400'd on
/// `OpenAI` strict — now rides JSON MODE on the real openai adapter and
/// lands green, validated locally.
#[tokio::test]
async fn underspecified_schema_rides_json_mode_on_the_openai_path() {
    let seam = SeamHttp::with_json(&[
        r#"{"choices":[{"message":{"content":"{\"head\":{\"x\":1},\"sections\":[]}"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":5}}"#,
    ]);
    let mut input = InferInput::new("translate the payload");
    input.schema = Some(json!({ "type": "object" }));
    let out = openai_verb(&seam)
        .run(input)
        .await
        .expect("green — the strict-mode 400 class is gone");
    assert!(matches!(out.output, InferValue::Structured(_)));

    // The wire proof: JSON mode requested — NOT the strict schema
    // the provider would reject.
    let body = wire_body(&seam);
    assert_eq!(
        body["response_format"],
        json!({ "type": "json_object" }),
        "{body}"
    );
    // The shape is steered through the prompt + enforced locally.
    let prompt = body["messages"][0]["content"]
        .as_str()
        .expect("prompt text");
    assert!(prompt.contains("JSON Schema"), "{prompt}");
}

/// deepseek is the ONE cloud whose API has no `json_schema`
/// (`response_format` enum = `text`|`json_object` · out-of-enum → 4xx ·
/// api-docs.deepseek.com · 2026-07-08): a fully-specified schema takes
/// the INSTRUCTION wire there — no `response_format` on the body at
/// all, the schema riding the prompt, validation local. Before the
/// per-profile capability correction this request died at the wire.
#[tokio::test]
async fn deepseek_schema_takes_the_instruction_wire() {
    let seam = SeamHttp::with_json(&[
        r#"{"choices":[{"message":{"content":"{\"name\":\"Ada\",\"age\":36}"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":5}}"#,
    ]);
    let registry = Registry::new(
        Arc::clone(&seam),
        ProvidersConfig::new().with_key("deepseek", Secret::new("sk-test")),
    );
    let verb = InferVerb::new(Arc::new(registry), "deepseek/deepseek-chat");
    let mut input = InferInput::new("extract the person");
    input.schema = Some(json!({
        "type": "object",
        "properties": { "name": { "type": "string" }, "age": { "type": "integer" } },
        "required": ["name", "age"],
        "additionalProperties": false
    }));
    let out = verb.run(input).await.expect("the instruction path lands");
    assert!(matches!(out.output, InferValue::Structured(_)));
    let body = wire_body(&seam);
    assert!(
        body.get("response_format").is_none(),
        "no out-of-enum json_schema may reach deepseek: {body}"
    );
    let prompt = body["messages"][0]["content"].as_str().expect("prompt");
    assert!(
        prompt.contains("JSON Schema"),
        "the schema rides the prompt"
    );
}

/// A retried structured task bills EVERY round-trip: the first reply
/// misses the schema (10+5 tokens), the retry conforms (20+7) — the
/// task total is the sum, while `response.usage` stays the final
/// round-trip alone. Before the fix the first call's tokens vanished
/// (the cost-undercount finding · deep review 2026-07-07).
#[tokio::test]
async fn retried_structured_task_bills_every_round_trip() {
    let seam = SeamHttp::with_json(&[
        r#"{"choices":[{"message":{"content":"{\"name\":\"Ada\"}"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":5}}"#,
        r#"{"choices":[{"message":{"content":"{\"name\":\"Ada\",\"age\":36}"},"finish_reason":"stop"}],"usage":{"prompt_tokens":20,"completion_tokens":7}}"#,
    ]);
    let mut input = InferInput::new("extract the person");
    input.schema = Some(json!({
        "type": "object",
        "properties": { "name": { "type": "string" }, "age": { "type": "integer" } },
        "required": ["name", "age"],
        "additionalProperties": false
    }));
    let out = openai_verb(&seam)
        .run(input)
        .await
        .expect("the retry conforms");
    assert!(matches!(out.output, InferValue::Structured(_)));
    assert_eq!(out.usage.input_tokens, 30, "task total sums both calls");
    assert_eq!(out.usage.output_tokens, 12);
    assert_eq!(
        out.response.usage.input_tokens, 20,
        "the final round-trip alone stays on response.usage"
    );
    assert_eq!(seam.captured().len(), 2, "exactly two round-trips");
}

/// A structured reply cut off at the token limit reports the TRUNCATION
/// as the cause, not a bare schema mismatch (review lens 5 · finding #5).
/// `finish_reason: "length"` → `StopReason::MaxTokens` on the openai
/// adapter; the terminal error must name the real fix.
#[tokio::test]
async fn truncated_structured_reply_names_the_token_limit() {
    // Valid JSON, but the required `age` never arrived — the reply was
    // cut off. Budget 0 → single shot → straight to the terminal error.
    let seam = SeamHttp::with_json(&[
        r#"{"choices":[{"message":{"content":"{\"name\":\"Ada\"}"},"finish_reason":"length"}],"usage":{"prompt_tokens":10,"completion_tokens":5}}"#,
    ]);
    let mut input = InferInput::new("extract the person");
    input.schema = Some(json!({
        "type": "object",
        "properties": { "name": { "type": "string" }, "age": { "type": "integer" } },
        "required": ["name", "age"],
        "additionalProperties": false
    }));
    let err = openai_verb(&seam)
        .with_schema_retry_budget(0)
        .run(input)
        .await
        .expect_err("a truncated reply that misses the schema must error");
    let msg = err.to_string();
    assert!(
        msg.contains("token limit"),
        "the cause must be named: {msg}"
    );
}

/// The truncation FAST-FAIL: a reply cut at `max_tokens` is terminal on
/// FIRST sight even with retry budget remaining — the identical request
/// would cut again (same budget · longer prompt), so every re-ask is a
/// paid call spent on a failure class whose remedy is the budget, not
/// the schema. The scripted SECOND reply must never be requested.
#[tokio::test]
async fn truncated_reply_fails_fast_without_burning_the_retry_budget() {
    let seam = SeamHttp::with_json(&[
        r#"{"choices":[{"message":{"content":"{\"name\":\"Ada\"}"},"finish_reason":"length"}],"usage":{"prompt_tokens":10,"completion_tokens":5}}"#,
        r#"{"choices":[{"message":{"content":"{\"name\":\"Ada\",\"age\":36}"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":5}}"#,
    ]);
    let mut input = InferInput::new("extract the person");
    input.schema = Some(json!({
        "type": "object",
        "properties": { "name": { "type": "string" }, "age": { "type": "integer" } },
        "required": ["name", "age"],
        "additionalProperties": false
    }));
    let err = openai_verb(&seam)
        .with_schema_retry_budget(3)
        .run(input)
        .await
        .expect_err("truncation is terminal on first sight");
    match &err {
        VerbInferError::SchemaValidation {
            attempts,
            detail,
            spend,
        } => {
            assert_eq!(*attempts, 1, "no blind re-ask at the same budget");
            assert!(
                spend.has_signal(),
                "the round-trip reported usage — the billed call is honestly metered"
            );
            assert!(
                detail.contains("token limit"),
                "the real fix named: {detail}"
            );
        }
        other => panic!("expected SchemaValidation, got {other:?}"),
    }
    assert_eq!(
        seam.captured().len(),
        1,
        "exactly one paid round-trip — the budget was NOT burned"
    );
}

/// The retry message is a NUMBERED repair list with the localized-edit
/// framing (« fix exactly these · keep everything else identical ») —
/// not a prose dump. Pinned at the wire: the SECOND request's last user
/// message carries the list, the framing and the failed path.
#[tokio::test]
async fn retry_message_is_a_numbered_repair_list_at_the_wire() {
    let seam = SeamHttp::with_json(&[
        r#"{"choices":[{"message":{"content":"{\"name\":\"Ada\"}"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":5}}"#,
        r#"{"choices":[{"message":{"content":"{\"name\":\"Ada\",\"age\":36}"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":5}}"#,
    ]);
    let mut input = InferInput::new("extract the person");
    input.schema = Some(json!({
        "type": "object",
        "properties": { "name": { "type": "string" }, "age": { "type": "integer" } },
        "required": ["name", "age"],
        "additionalProperties": false
    }));
    let out = openai_verb(&seam).run(input).await.expect("retry conforms");
    assert!(matches!(out.output, InferValue::Structured(_)));

    let captured = seam.captured();
    assert_eq!(captured.len(), 2, "one miss · one repair");
    let second: serde_json::Value =
        serde_json::from_slice(captured[1].body.as_ref().expect("retry body")).expect("json");
    let messages = second["messages"].as_array().expect("messages");
    let retry_prompt = messages
        .last()
        .and_then(|m| m["content"].as_str())
        .expect("the retry user message");
    assert!(
        retry_prompt.contains("Repair instructions"),
        "{retry_prompt}"
    );
    assert!(
        retry_prompt.contains("\n1. "),
        "numbered list: {retry_prompt}"
    );
    assert!(
        retry_prompt.contains("keep everything else identical"),
        "localized-edit framing: {retry_prompt}"
    );
    assert!(
        retry_prompt.contains("\"age\""),
        "the failed path is named: {retry_prompt}"
    );
}

/// The SAP-lite rescue deletes a paid retry: a reply whose only sin is
/// STRING-ENCODED scalars ("36" where an integer is declared · a
/// case-drifted enum) is repaired locally and lands in ONE round-trip —
/// the scripted second reply must never be requested.
#[tokio::test]
async fn coercible_reply_lands_in_one_round_trip() {
    let seam = SeamHttp::with_json(&[
        r#"{"choices":[{"message":{"content":"{\"name\":\"Ada\",\"age\":\"36\",\"field\":\" Mathematics \"}"},"finish_reason":"stop"}],"usage":{"prompt_tokens":9,"completion_tokens":4}}"#,
        r#"{"choices":[{"message":{"content":"{\"name\":\"Ada\",\"age\":36,\"field\":\"mathematics\"}"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":5}}"#,
    ]);
    let mut input = InferInput::new("extract the person");
    input.schema = Some(json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" },
            "age": { "type": "integer" },
            "field": { "type": "string", "enum": ["physics", "mathematics"] }
        },
        "required": ["name", "age", "field"],
        "additionalProperties": false
    }));
    let out = openai_verb(&seam)
        .run(input)
        .await
        .expect("the rescue repairs locally");
    match &out.output {
        InferValue::Structured(v) => {
            assert_eq!(v["age"], 36, "string-encoded integer coerced");
            assert_eq!(v["field"], "mathematics", "enum case-snapped");
        }
        other => panic!("expected structured, got {other:?}"),
    }
    assert_eq!(
        seam.captured().len(),
        1,
        "the coercion DELETED the retry round-trip"
    );
    assert_eq!(out.usage.input_tokens, 9, "one round-trip billed");
}

/// A miss the ladder cannot repair (a missing required member) still
/// takes the ordinary retry path — the rescue never masks real gaps.
#[tokio::test]
async fn uncoercible_miss_still_retries() {
    let seam = SeamHttp::with_json(&[
        r#"{"choices":[{"message":{"content":"{\"name\":\"Ada\"}"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":5}}"#,
        r#"{"choices":[{"message":{"content":"{\"name\":\"Ada\",\"age\":36}"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":5}}"#,
    ]);
    let mut input = InferInput::new("extract the person");
    input.schema = Some(json!({
        "type": "object",
        "properties": { "name": { "type": "string" }, "age": { "type": "integer" } },
        "required": ["name", "age"],
        "additionalProperties": false
    }));
    let out = openai_verb(&seam).run(input).await.expect("retry conforms");
    assert!(matches!(out.output, InferValue::Structured(_)));
    assert_eq!(seam.captured().len(), 2, "a real gap still costs a retry");
}

/// A NORMAL stop that fails the schema stays a plain schema mismatch —
/// no truncation hint bolted onto an ordinary validation failure.
#[tokio::test]
async fn a_normal_stop_carries_no_truncation_hint() {
    let seam = SeamHttp::with_json(&[
        r#"{"choices":[{"message":{"content":"{\"name\":\"Ada\"}"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":5}}"#,
    ]);
    let mut input = InferInput::new("extract the person");
    input.schema = Some(json!({
        "type": "object",
        "properties": { "name": { "type": "string" }, "age": { "type": "integer" } },
        "required": ["name", "age"],
        "additionalProperties": false
    }));
    let err = openai_verb(&seam)
        .with_schema_retry_budget(0)
        .run(input)
        .await
        .expect_err("missing required field must still error");
    assert!(
        !err.to_string().contains("token limit"),
        "a clean stop must not claim truncation: {err}"
    );
}

/// F2 non-regression: a fully-specified schema keeps today's strict
/// path on the SAME adapter — forwarded verbatim as `json_schema`.
#[tokio::test]
async fn fully_specified_schema_keeps_the_strict_path_on_openai() {
    let seam = SeamHttp::with_json(&[
        r#"{"choices":[{"message":{"content":"{\"name\":\"Ada\",\"age\":36}"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":5}}"#,
    ]);
    let mut input = InferInput::new("extract the person");
    input.schema = Some(json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" },
            "age": { "type": "integer" }
        },
        "required": ["name", "age"],
        "additionalProperties": false
    }));
    let out = openai_verb(&seam)
        .run(input)
        .await
        .expect("the strict path stays green");
    assert!(matches!(out.output, InferValue::Structured(_)));

    let body = wire_body(&seam);
    assert_eq!(body["response_format"]["type"], "json_schema", "{body}");
}

/// F2 non-regression for F3: the mock's "strict mode" SYNTHESIZES
/// from ANY schema — an underspecified schema must keep the Strict
/// wire there (a JSON-mode fallback would hand mock/echo prose and
/// break every offline golden).
#[tokio::test]
async fn underspecified_schema_still_synthesizes_on_mock() {
    let mut input = InferInput::new("free-form");
    input.schema = Some(json!({ "type": "object" }));
    let out = mock_verb().run(input).await.expect("mock stays green");
    assert!(matches!(out.output, InferValue::Structured(_)));
}

// ── #651 · the empty-answer gate (OBS-E promoted) ─────────────────

/// The issue's exact repro: a thinking model eats the whole budget on
/// its reasoning trace and concludes BLANK — the reply carries a real
/// spend and an empty visible answer. Green-over-"" is over: the task
/// fails typed, the teaching rides the message.
#[tokio::test]
async fn empty_answer_with_reasoning_spend_fails_typed() {
    use nika_error::traits::NikaErrorCode as _;

    let seam = SeamHttp::with_json(&[
        r#"{"choices":[{"message":{"content":""},"finish_reason":"length"}],"usage":{"prompt_tokens":7,"completion_tokens":84,"completion_tokens_details":{"reasoning_tokens":84}}}"#,
    ]);
    let err = openai_verb(&seam)
        .run(InferInput::new("brief me"))
        .await
        .expect_err("a blank answer with spend fails, never green");
    match &err {
        VerbInferError::EmptyAnswer {
            model,
            detail,
            spend,
        } => {
            assert_eq!(model, "openai/gpt-4o-mini");
            assert!(
                detail.contains("max_tokens"),
                "the likely fix is named: {detail}"
            );
            assert!(
                detail.contains("84"),
                "the reasoning spend is reported: {detail}"
            );
            assert!(
                spend.has_signal(),
                "the billed empty round-trip is honestly metered"
            );
        }
        other => panic!("expected EmptyAnswer, got {other:?}"),
    }
    assert_eq!(err.spec_code(), "NIKA-INFER-004", "the wire code");
    assert_eq!(err.nika_code(), nika_error::codes::NIKA_435);
    assert!(!err.is_transient(), "retrying the same budget re-fails");
    let msg = err.to_string();
    assert!(
        msg.contains("infer produced an empty answer"),
        "the warn's words survive the promotion: {msg}"
    );
}

/// The #410 shape: no reasoning split reported (the ollama class strips
/// the think block upstream) — one undifferentiated output count and a
/// blank visible answer. The gate keys off the spend, not the split.
#[tokio::test]
async fn empty_answer_with_undifferentiated_spend_fails_typed() {
    let seam = SeamHttp::with_json(&[
        r#"{"choices":[{"message":{"content":"  "},"finish_reason":"stop"}],"usage":{"prompt_tokens":7,"completion_tokens":512}}"#,
    ]);
    let err = openai_verb(&seam)
        .run(InferInput::new("brief me"))
        .await
        .expect_err("whitespace-only + spend is the same footgun");
    match &err {
        VerbInferError::EmptyAnswer { detail, .. } => {
            assert!(detail.contains("512"), "the spend is named: {detail}");
            assert!(detail.contains("max_tokens"), "the fix is named: {detail}");
            assert!(
                detail.contains("no-think"),
                "the alternative is named: {detail}"
            );
        }
        other => panic!("expected EmptyAnswer, got {other:?}"),
    }
}

/// The carve-out (preserved from the warn): a blank answer with ZERO
/// tokens of any kind is a plain empty completion — nothing was spent,
/// nothing to teach, the task stays green.
#[tokio::test]
async fn empty_answer_with_zero_spend_stays_green() {
    let seam = SeamHttp::with_json(&[
        r#"{"choices":[{"message":{"content":""},"finish_reason":"stop"}],"usage":{"prompt_tokens":0,"completion_tokens":0}}"#,
    ]);
    let out = openai_verb(&seam)
        .run(InferInput::new("say nothing"))
        .await
        .expect("a zero-spend empty completion is not the footgun");
    assert!(matches!(out.output, InferValue::Text(ref t) if t.is_empty()));
}

/// The gate's predicate, pure: blank + spend evidence fires, anything
/// else passes (the warn's detection table, promoted verbatim).
#[test]
fn blank_answer_gate_detection_table() {
    let reasoning = |t: u64| {
        let mut u = TokenUsage::new(7, 0);
        u.thinking_tokens = Some(t);
        u
    };
    // Blank + reported reasoning split → the max_tokens teaching.
    let err = refuse_blank_answer("", &reasoning(84), "m/x").expect_err("thinking spend fires");
    assert!(err.to_string().contains("84"), "{err}");
    // Blank + undifferentiated output spend → fires too.
    assert!(refuse_blank_answer("  ", &TokenUsage::new(7, 512), "m/x").is_err());
    // Blank + zero spend → silent (a plain empty completion).
    assert!(refuse_blank_answer("", &TokenUsage::new(7, 0), "m/x").is_ok());
    // Content of any kind → never the footgun.
    assert!(refuse_blank_answer("Paris", &reasoning(84), "m/x").is_ok());
    assert!(refuse_blank_answer("Paris", &TokenUsage::new(7, 50), "m/x").is_ok());
}

#[test]
fn request_carries_params_and_the_schema_wire() {
    let mut input = InferInput::new("q");
    input.temperature = Some(0.7);
    input.max_tokens = Some(64);
    input.schema = Some(json!({
        "type": "object",
        "properties": { "name": { "type": "string" } }
    }));
    let req = build_request(
        &input,
        "claude-x",
        base_messages(&input, SchemaWire::Strict),
        SchemaWire::Strict,
    );
    assert_eq!(req.model, "claude-x");
    assert_eq!(req.temperature, Some(0.7));
    assert_eq!(req.max_tokens, Some(64));
    assert!(matches!(req.response_format, ResponseFormat::JsonSchema(_)));
    // F2 · the JSON-mode fallback asks the wire for JSON, not a shape.
    let req_json_mode = build_request(
        &input,
        "m",
        base_messages(&input, SchemaWire::JsonMode),
        SchemaWire::JsonMode,
    );
    assert!(matches!(
        req_json_mode.response_format,
        ResponseFormat::Json
    ));
    let req_no_native = build_request(
        &input,
        "m",
        base_messages(&input, SchemaWire::Instruction),
        SchemaWire::Instruction,
    );
    assert!(matches!(
        req_no_native.response_format,
        ResponseFormat::Text
    ));
}

const OPENAI_OK: &str = r#"{"id":"cc","model":"m",
        "choices":[{"message":{"content":"ok"},"finish_reason":"stop"}],
        "usage":{"prompt_tokens":1,"completion_tokens":1}}"#;

/// #1135 · the filed 0.111.0 body was text-only
/// `{"messages":[{"content":"MARKER...","role":"user"}]}`. A URL vision
/// part MUST appear as `image_url` on the openai-compat wire.
#[tokio::test]
async fn url_vision_appears_as_an_image_url_part_on_the_wire() {
    let seam = SeamHttp::with_json(&[OPENAI_OK]);
    let verb = openai_verb(&seam);
    let mut input = InferInput::new("MARKER-PROMPT-XYZ");
    input.max_tokens = Some(16);
    input
        .vision
        .push(VisionPart::url("http://127.0.0.1:8731/x.png"));
    verb.run(input).await.expect("infer succeeds");
    let captured = seam.captured();
    assert_eq!(captured.len(), 1, "one provider round-trip");
    let body: serde_json::Value =
        serde_json::from_slice(captured[0].body.as_ref().expect("body")).expect("json");
    let content = &body["messages"][0]["content"];
    assert!(
        content.is_array(),
        "multimodal content is an array, not a text string: {content}"
    );
    let parts = content.as_array().expect("parts");
    assert!(
        parts.iter().any(|p| {
            p["type"] == "image_url" && p["image_url"]["url"] == "http://127.0.0.1:8731/x.png"
        }),
        "the URL vision part rides as image_url: {content}"
    );
    assert!(
        parts
            .iter()
            .any(|p| p["type"] == "text" && p["text"] == "MARKER-PROMPT-XYZ"),
        "the prompt still rides: {content}"
    );
}

/// #1135 · a missing local file used to run green. It must fail closed
/// (`InvalidParam` param:"vision") with zero provider calls.
#[tokio::test]
async fn missing_local_vision_file_fails_before_any_provider_call() {
    let seam = SeamHttp::with_json(&[]);
    let verb = openai_verb(&seam);
    let mut input = InferInput::new("MARKER-FILE-PROBE");
    input
        .vision
        .push(VisionPart::file("./this-file-does-not-exist.png"));
    let err = verb
        .run(input)
        .await
        .expect_err("missing vision file refuses");
    assert!(
        matches!(
            err,
            VerbInferError::InvalidParam {
                param: "vision",
                ..
            }
        ),
        "typed vision param: {err}"
    );
    assert!(
        err.to_string().contains("cannot read image"),
        "the missing path is named: {err}"
    );
    assert!(
        seam.captured().is_empty(),
        "zero wire calls — the file gate fires first"
    );
}

/// A present local file inlines as a `data:image/...;base64,...` URL so
/// openai-compat (and anthropic, after the same fence lift) can carry it.
#[tokio::test]
async fn local_vision_file_becomes_a_data_url_part() {
    let dir = std::env::temp_dir();
    let path = dir.join("nika-s2-1135-vision.png");
    // 1×1 PNG (67 bytes) — magic is what names the mime.
    let png: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90,
        0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8,
        0xCF, 0xC0, 0x00, 0x00, 0x00, 0x03, 0x00, 0x01, 0x18, 0xDD, 0x8D, 0xB4, 0x00, 0x00, 0x00,
        0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];
    std::fs::write(&path, png).expect("write fixture");
    let seam = SeamHttp::with_json(&[OPENAI_OK]);
    let verb = openai_verb(&seam);
    let mut input = InferInput::new("look");
    input.vision.push(VisionPart::file(path.to_string_lossy()));
    verb.run(input).await.expect("infer succeeds");
    let body: serde_json::Value =
        serde_json::from_slice(seam.captured()[0].body.as_ref().expect("body")).expect("json");
    let parts = body["messages"][0]["content"].as_array().expect("parts");
    let url = parts
        .iter()
        .find(|p| p["type"] == "image_url")
        .and_then(|p| p["image_url"]["url"].as_str())
        .expect("image_url part");
    assert!(
        url.starts_with("data:image/png;base64,"),
        "file inlines as a data URL: {url}"
    );
    let _ = std::fs::remove_file(&path);
}
