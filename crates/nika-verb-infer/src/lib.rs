// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! # nika-verb-infer — the `infer` verb executor (L2)
//!
//! One-shot LLM inference per `nika-spec spec/02-verbs.md §infer` — the
//! first of the 4 verbs (`infer · exec · invoke · agent` · locked forever
//! per D-2026-05-22-N18).
//!
//! ## Shape
//!
//! - **Resolution** — `model: provider/name` resolves through the
//!   [`nika_providers::ProviderRegistry`] (L1.5 · D-2026-05-22-N17: the
//!   provider layer lives BELOW the verbs so `infer` and `agent` share it
//!   without a sideways dep).
//! - **Request shaping** — builds the kernel
//!   [`nika_kernel::ai::provider::InferRequest`] from already-resolved
//!   task fields (`${{ }}` CEL binding happens upstream).
//! - **Structured output floor** — when the task carries a `schema:`,
//!   picks the wire per the internal `SchemaWire` decision: fully-specified
//!   schemas forward as
//!   `ResponseFormat::JsonSchema`; an UNDERSPECIFIED schema on a strict
//!   wire falls back to the provider's native JSON mode + LOCAL validation
//!   (F2 · ADR-098); profiles without native support get the instruction
//!   fallback. In every path the reply is extracted, validated locally,
//!   and retried within a bounded budget (spec-sanctioned: « MAY
//!   auto-retry validation before emitting NIKA-INFER-002 »).
//!
//! ## Fences (what this crate is NOT)
//!
//! Streaming passthrough (`verb-agent`/engine surface) · vision staging
//! (`nika-media-*` · deferred) · `${{ }}` resolution (upstream binding) ·
//! transport retry/backoff (engine scheduler policy — only the
//! schema-validation retry lives here).
//!
//! ## Example (mock · zero key · zero network)
//!
//! ```
//! use std::sync::Arc;
//! use nika_providers::{ProviderRegistry, ProvidersConfig};
//! use nika_verb_infer::{InferInput, InferVerb};
//!
//! # async fn demo() -> Result<(), Box<dyn std::error::Error>> {
//! let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
//! let verb = InferVerb::new(registry, "mock/echo");
//! let out = verb.run(InferInput::new("Say hello.")).await?;
//! # Ok(())
//! # }
//! ```

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

mod coerce;
mod errors;
mod structured;

use std::sync::Arc;

use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, Message, ProviderInferDyn, ProviderMeta,
    ResponseFormat, Role, StopReason, TokenUsage,
};
use nika_kernel::http::HttpPostDyn;
use nika_providers::ProviderRegistry;

pub use errors::VerbInferError;

/// Default schema-validation retry budget (provider re-calls AFTER the
/// initial one — 2 retries = up to 3 round-trips on a structured task).
pub const DEFAULT_SCHEMA_RETRY_BUDGET: u8 = 2;

/// The `infer` task input — spec fields, already CEL-resolved.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct InferInput {
    /// The user prompt (required · spec §infer).
    pub prompt: String,
    /// Optional system prompt.
    pub system: Option<String>,
    /// Optional `provider/name` override of the verb's default model.
    pub model: Option<String>,
    /// Sampling temperature (0-2 · validated here).
    pub temperature: Option<f32>,
    /// Maximum output tokens.
    pub max_tokens: Option<u32>,
    /// JSON Schema for structured output (spec `schema:`).
    pub schema: Option<serde_json::Value>,
    /// Extended-thinking token budget (spec `thinking.budget_tokens`).
    pub thinking_budget: Option<u32>,
    /// The task-level `timeout:` budget (spec 03) — plumbed to the
    /// provider transport deadline so the HTTP effect's fixed default
    /// cannot undercut a longer task budget (F1 · a local model
    /// routinely needs minutes). `None` → the adapter's per-provider
    /// default governs.
    pub timeout: Option<std::time::Duration>,
}

impl InferInput {
    /// A plain-text inference with every optional field unset.
    #[must_use]
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            system: None,
            model: None,
            temperature: None,
            max_tokens: None,
            schema: None,
            thinking_budget: None,
            timeout: None,
        }
    }
}

/// The verb's result value — plain text, or the validated JSON value when
/// the task carried a `schema:`.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum InferValue {
    /// Free-form text output.
    Text(String),
    /// Schema-validated structured output.
    Structured(serde_json::Value),
}

/// The `infer` verb output.
#[derive(Debug)]
#[non_exhaustive]
pub struct InferOutput {
    /// The shaped output value (`.output` in spec terms).
    pub output: InferValue,
    /// Token usage summed across EVERY provider round-trip this task spent
    /// (schema retries included) — the number the ledger bills. Before this
    /// was the FINAL round-trip alone, so a structured task that retried
    /// twice billed ~⅓ of its real spend. The final round-trip by itself
    /// stays available as `response.usage`.
    pub usage: TokenUsage,
    /// The model string that was resolved (`provider/name`).
    pub model_resolved: String,
    /// The full kernel response of the FINAL round-trip (engine surface:
    /// events · cost · trace propagation). Its `usage` is that round-trip
    /// alone — the task total lives in `self.usage`.
    pub response: InferResponse,
}

impl InferOutput {
    /// Construct from the final round-trip + the task-total usage the run
    /// loop accumulated across every round-trip (single-shot tasks pass the
    /// one response's usage — total and final coincide).
    #[must_use]
    pub fn new(
        output: InferValue,
        model_resolved: String,
        response: InferResponse,
        usage: TokenUsage,
    ) -> Self {
        Self {
            output,
            usage,
            model_resolved,
            response,
        }
    }
}

/// One-shot LLM inference — the `infer` verb executor.
#[derive(Debug)]
pub struct InferVerb<H = nika_providers::NoHttp> {
    registry: Arc<ProviderRegistry<H>>,
    default_model: String,
    schema_retry_budget: u8,
}

impl<H> InferVerb<H> {
    /// Create the verb over a registry with the workflow's default model.
    #[must_use]
    pub fn new(registry: Arc<ProviderRegistry<H>>, default_model: impl Into<String>) -> Self {
        Self {
            registry,
            default_model: default_model.into(),
            schema_retry_budget: DEFAULT_SCHEMA_RETRY_BUDGET,
        }
    }

    /// Override the schema-validation retry budget (0 = single shot).
    #[must_use]
    pub fn with_schema_retry_budget(mut self, budget: u8) -> Self {
        self.schema_retry_budget = budget;
        self
    }
}

impl<H> InferVerb<H>
where
    H: HttpPostDyn + Send + Sync + 'static,
{
    /// Execute the `infer` task.
    ///
    /// CANCEL SAFETY: cancel-safe at the provider transport (kernel
    /// contract) — no state is mutated; dropping the future mid-call
    /// abandons the request.
    ///
    /// # Errors
    ///
    /// [`VerbInferError::InvalidParam`] on an empty prompt or out-of-range
    /// temperature · [`VerbInferError::ModelResolution`] when the model
    /// string resolves to no profile · [`VerbInferError::ProviderCall`]
    /// when the provider round-trip fails ·
    /// [`VerbInferError::SchemaValidation`] when a `schema:` task exhausts
    /// the retry budget without a conforming reply.
    pub async fn run(&self, input: InferInput) -> Result<InferOutput, VerbInferError> {
        validate_params(&input)?;

        // Compile the task schema ONCE, before any provider call — a schema
        // that doesn't compile is a task-authoring error (NIKA-432), not a
        // validation failure, and must not burn paid round-trips (review
        // lenses 1+3 · P1).
        let validator = match input.schema.as_ref() {
            Some(schema) => Some(structured::compile_schema(schema).map_err(|detail| {
                VerbInferError::InvalidParam {
                    param: "schema",
                    detail,
                }
            })?),
            None => None,
        };

        let model = input.model.as_deref().unwrap_or(&self.default_model);
        let provider =
            self.registry
                .resolve(model)
                .map_err(|source| VerbInferError::ModelResolution {
                    model: model.to_owned(),
                    source,
                })?;
        let wire = schema_wire(
            &input,
            provider.supports_response_format(),
            provider.strict_schema_rejects_underspecified(),
        );

        let mut messages = base_messages(&input, wire);
        // u32 counter: a u8 would saturate at budget = u8::MAX and loop
        // forever on paid calls (review lens 1 · P1).
        let mut attempts: u32 = 0;
        // EVERY round-trip's usage folds in — a schema retry is a real paid
        // call and the ledger bills `InferOutput::usage`. Keeping only the
        // final response's usage under-billed retried tasks by up to
        // budget+1 × (the cost-undercount finding · deep review 2026-07-07).
        let mut usage_total = TokenUsage::default();
        loop {
            attempts += 1;
            let request = build_request(&input, provider.name(), messages.clone(), wire);
            let response = provider
                .infer(request)
                .await
                .map_err(|source| VerbInferError::ProviderCall { source })?;
            usage_total.absorb(&response.usage);
            let text = response_text(&response);

            let (Some(schema), Some(validator)) = (input.schema.as_ref(), validator.as_ref())
            else {
                return Ok(InferOutput::new(
                    InferValue::Text(text),
                    model.to_owned(),
                    response,
                    usage_total,
                ));
            };

            match structured::extract_and_validate(&text, validator, schema) {
                structured::Validation::Valid(value) => {
                    return Ok(InferOutput::new(
                        InferValue::Structured(value),
                        model.to_owned(),
                        response,
                        usage_total,
                    ));
                }
                structured::Validation::Invalid(errors) => {
                    // Terminal on either: (a) the retry budget is spent, or
                    // (b) TRUNCATION fast-fail — a reply cut at `max_tokens`
                    // cannot be repaired by re-asking at the SAME budget (the
                    // identical request cuts again, and the retry message
                    // makes the prompt LONGER), so every re-ask is a paid
                    // call spent on a known failure class whose remedy is
                    // the budget, not the schema. Providers document
                    // length-exhaustion as its own escape hatch (structured
                    // output explicitly does NOT hold across it); the
                    // stop_reason_hint prints the actionable fix either way.
                    let truncated = matches!(response.stop_reason, StopReason::MaxTokens);
                    if truncated || attempts > u32::from(self.schema_retry_budget) {
                        let detail = format!(
                            "{}{}",
                            errors.join("; "),
                            stop_reason_hint(&response.stop_reason)
                        );
                        return Err(VerbInferError::SchemaValidation { attempts, detail });
                    }
                    messages.push(Message::text(Role::Assistant, text));
                    messages.push(Message::text(
                        Role::User,
                        structured::retry_message(&errors, schema),
                    ));
                }
            }
        }
    }
}

/// Spec-level parameter validation (NIKA-432 class).
fn validate_params(input: &InferInput) -> Result<(), VerbInferError> {
    if input.prompt.trim().is_empty() {
        return Err(VerbInferError::InvalidParam {
            param: "prompt",
            detail: "prompt must be a non-empty string".to_owned(),
        });
    }
    if let Some(t) = input.temperature
        && !(0.0..=2.0).contains(&t)
    {
        return Err(VerbInferError::InvalidParam {
            param: "temperature",
            detail: format!("{t} is outside the spec range 0-2"),
        });
    }
    Ok(())
}

/// How the task `schema:` reaches the provider wire (F2 · ADR-098).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SchemaWire {
    /// No `schema:` on the task — plain text.
    None,
    /// Fully-specified schema on a wire with native structured support —
    /// forward verbatim as `ResponseFormat::JsonSchema` (today's path).
    Strict,
    /// UNDERSPECIFIED schema (an object without `properties` · an array
    /// without `items` anywhere in the tree) on a strict wire that would
    /// 400 on it — request the provider's native JSON mode instead, put
    /// the schema instruction in the prompt, validate LOCALLY.
    JsonMode,
    /// No native support on this wire at all — instruction-only prompt
    /// (+ the lenient extraction strips prose/code fences), local
    /// validation as everywhere.
    Instruction,
}

/// Pick the schema wire once per run — the F2 decision, made from the
/// resolved provider's two capability answers.
fn schema_wire(input: &InferInput, native: bool, strict_rejects: bool) -> SchemaWire {
    let Some(schema) = &input.schema else {
        return SchemaWire::None;
    };
    if !native {
        return SchemaWire::Instruction;
    }
    if strict_rejects && structured::is_underspecified(schema) {
        return SchemaWire::JsonMode;
    }
    SchemaWire::Strict
}

/// The opening conversation: optional system, then the user prompt (with
/// the schema instruction appended whenever the schema does NOT travel
/// natively as `JsonSchema` — the JSON-mode and instruction fallbacks
/// both steer the model through the prompt).
fn base_messages(input: &InferInput, wire: SchemaWire) -> Vec<Message> {
    let mut messages = Vec::with_capacity(2);
    if let Some(system) = &input.system {
        messages.push(Message::text(Role::System, system.clone()));
    }
    let prompt = match (&input.schema, wire) {
        (Some(schema), SchemaWire::JsonMode | SchemaWire::Instruction) => format!(
            "{prompt}\n\nReply with ONLY a JSON value that satisfies this \
             JSON Schema, no prose, no code fences:\n{rendered}",
            prompt = input.prompt,
            rendered = structured::render_schema(schema)
        ),
        _ => input.prompt.clone(),
    };
    messages.push(Message::text(Role::User, prompt));
    messages
}

/// Shape the kernel request for one round-trip.
fn build_request(
    input: &InferInput,
    wire_model: &str,
    messages: Vec<Message>,
    wire: SchemaWire,
) -> InferRequest {
    let mut request = InferRequest::new(wire_model, messages);
    request.temperature = input.temperature;
    request.max_tokens = input.max_tokens;
    request.thinking_budget = input.thinking_budget;
    // The task `timeout:` rides every round-trip of this task (schema
    // retries included) — the OUTER attempt-loop budget still enforces
    // the real total; this only stops the transport from undercutting it.
    request.timeout = input.timeout;
    match (&input.schema, wire) {
        (Some(schema), SchemaWire::Strict) => {
            request.response_format = ResponseFormat::JsonSchema(schema.clone());
        }
        // The F2 fallback: ask for JSON (which the wire CAN promise) ·
        // the user schema is enforced by the LOCAL validation layer.
        (Some(_), SchemaWire::JsonMode) => {
            request.response_format = ResponseFormat::Json;
        }
        _ => {}
    }
    request
}

/// An actionable suffix when a structured reply failed AND the provider
/// stopped for a reason that EXPLAINS the failure — truncation and content
/// filtering otherwise masquerade as a bare schema mismatch, hiding the real
/// cause. Empty for a normal stop (the schema genuinely went unmet).
fn stop_reason_hint(stop: &StopReason) -> &'static str {
    match stop {
        StopReason::MaxTokens => {
            " · the reply was cut off at the token limit before it could \
             complete — raise `max_tokens` or simplify the schema"
        }
        StopReason::ContentFilter => " · the provider's content filter blocked the reply",
        _ => "",
    }
}

/// Concatenated text blocks of the response content.
fn response_text(response: &InferResponse) -> String {
    response
        .content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

#[cfg(test)]
mod tests {
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

        async fn send_streaming(
            &self,
            _request: HttpRequest,
        ) -> Result<HttpStreamResponse, HttpError> {
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
            r#"{"choices":[{"message":{"content":"{\"head\":{\"x\":1},\"sections\":[]}"},"finish_reason":"stop"}],"usage":{}}"#,
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
            r#"{"choices":[{"message":{"content":"{\"name\":\"Ada\",\"age\":36}"},"finish_reason":"stop"}],"usage":{}}"#,
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
            r#"{"choices":[{"message":{"content":"{\"name\":\"Ada\"}"},"finish_reason":"length"}],"usage":{}}"#,
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
            r#"{"choices":[{"message":{"content":"{\"name\":\"Ada\"}"},"finish_reason":"length"}],"usage":{}}"#,
            r#"{"choices":[{"message":{"content":"{\"name\":\"Ada\",\"age\":36}"},"finish_reason":"stop"}],"usage":{}}"#,
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
            VerbInferError::SchemaValidation { attempts, detail } => {
                assert_eq!(*attempts, 1, "no blind re-ask at the same budget");
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
            r#"{"choices":[{"message":{"content":"{\"name\":\"Ada\"}"},"finish_reason":"stop"}],"usage":{}}"#,
            r#"{"choices":[{"message":{"content":"{\"name\":\"Ada\",\"age\":36}"},"finish_reason":"stop"}],"usage":{}}"#,
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
            r#"{"choices":[{"message":{"content":"{\"name\":\"Ada\",\"age\":36,\"field\":\"mathematics\"}"},"finish_reason":"stop"}],"usage":{}}"#,
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
            r#"{"choices":[{"message":{"content":"{\"name\":\"Ada\"}"},"finish_reason":"stop"}],"usage":{}}"#,
            r#"{"choices":[{"message":{"content":"{\"name\":\"Ada\",\"age\":36}"},"finish_reason":"stop"}],"usage":{}}"#,
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
            r#"{"choices":[{"message":{"content":"{\"name\":\"Ada\"}"},"finish_reason":"stop"}],"usage":{}}"#,
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
            r#"{"choices":[{"message":{"content":"{\"name\":\"Ada\",\"age\":36}"},"finish_reason":"stop"}],"usage":{}}"#,
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
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// Temperature acceptance matches the spec interval exactly.
        #[test]
        fn temperature_validation_matches_spec_interval(t in -10.0f32..10.0) {
            let mut input = InferInput::new("p");
            input.temperature = Some(t);
            let ok = validate_params(&input).is_ok();
            prop_assert_eq!(ok, (0.0..=2.0).contains(&t));
        }

        /// The balanced-span extractor never panics on arbitrary text and,
        /// when it extracts, the candidate is real JSON.
        #[test]
        fn extraction_total_on_arbitrary_text(s in ".{0,400}") {
            // Total function — must not panic (the coercion pass included).
            let schema = serde_json::json!({ "type": "object" });
            let v = crate::structured::compile_schema(&schema)
                .expect("trivial schema compiles");
            let _ = crate::structured::extract_and_validate(&s, &v, &schema);
        }
    }
}
