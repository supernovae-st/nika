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
//!   injects `ResponseFormat::JsonSchema` (instruction fallback for
//!   profiles without native support), extracts a JSON candidate from the
//!   reply, validates, and retries within a bounded budget
//!   (spec-sanctioned: « MAY auto-retry validation before emitting
//!   NIKA-INFER-002 »).
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

mod errors;
mod structured;

use std::sync::Arc;

use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, Message, ProviderInferDyn, ProviderMeta,
    ResponseFormat, Role, TokenUsage,
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
    /// Token usage from the (final) provider round-trip — a convenience
    /// duplicate of `response.usage` (intentional · ergonomic access).
    pub usage: TokenUsage,
    /// The model string that was resolved (`provider/name`).
    pub model_resolved: String,
    /// The full kernel response of the final round-trip (engine surface:
    /// events · cost · trace propagation).
    pub response: InferResponse,
}

impl InferOutput {
    /// Construct from the final round-trip pieces.
    #[must_use]
    pub fn new(output: InferValue, model_resolved: String, response: InferResponse) -> Self {
        Self {
            output,
            usage: response.usage.clone(),
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
        let native_schema = provider.supports_response_format();

        let mut messages = base_messages(&input, native_schema);
        // u32 counter: a u8 would saturate at budget = u8::MAX and loop
        // forever on paid calls (review lens 1 · P1).
        let mut attempts: u32 = 0;
        loop {
            attempts += 1;
            let request = build_request(&input, provider.name(), messages.clone(), native_schema);
            let response = provider
                .infer(request)
                .await
                .map_err(|source| VerbInferError::ProviderCall { source })?;
            let text = response_text(&response);

            let (Some(schema), Some(validator)) = (input.schema.as_ref(), validator.as_ref())
            else {
                return Ok(InferOutput::new(
                    InferValue::Text(text),
                    model.to_owned(),
                    response,
                ));
            };

            match structured::extract_and_validate(&text, validator) {
                structured::Validation::Valid(value) => {
                    return Ok(InferOutput::new(
                        InferValue::Structured(value),
                        model.to_owned(),
                        response,
                    ));
                }
                structured::Validation::Invalid(detail) => {
                    if attempts > u32::from(self.schema_retry_budget) {
                        return Err(VerbInferError::SchemaValidation { attempts, detail });
                    }
                    messages.push(Message::text(Role::Assistant, text));
                    messages.push(Message::text(
                        Role::User,
                        structured::retry_message(&detail, schema),
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

/// The opening conversation: optional system, then the user prompt (with
/// the schema instruction appended when the profile lacks native
/// `response_format` support).
fn base_messages(input: &InferInput, native_schema: bool) -> Vec<Message> {
    let mut messages = Vec::with_capacity(2);
    if let Some(system) = &input.system {
        messages.push(Message::text(Role::System, system.clone()));
    }
    let prompt = match (&input.schema, native_schema) {
        (Some(schema), false) => format!(
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
    native_schema: bool,
) -> InferRequest {
    let mut request = InferRequest::new(wire_model, messages);
    request.temperature = input.temperature;
    request.max_tokens = input.max_tokens;
    request.thinking_budget = input.thinking_budget;
    if let Some(schema) = &input.schema
        && native_schema
    {
        request.response_format = ResponseFormat::JsonSchema(schema.clone());
    }
    request
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
    async fn structured_happy_path_through_the_noisy_echo() {
        // mock echoes `mock(echo) · {prompt}` — the prompt IS the JSON, the
        // balanced-span extraction digs it out of the prefix noise.
        let mut input = InferInput::new(r#"{"name":"Ada","age":36}"#);
        input.schema = Some(json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "age": { "type": "integer" }
            },
            "required": ["name", "age"]
        }));
        let out = mock_verb().run(input).await.expect("valid structured");
        match out.output {
            InferValue::Structured(v) => assert_eq!(v["age"], 36),
            other => panic!("expected structured, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn schema_retry_exhaustion_reports_attempts() {
        // The echo of a prose prompt never validates → budget exhausted.
        let mut input = InferInput::new("just prose, no json here");
        input.schema = Some(json!({ "type": "object", "required": ["x"] }));
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
        let mut input = InferInput::new("prose");
        input.schema = Some(json!({ "type": "object", "required": ["x"] }));
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
    }

    #[test]
    fn system_prompt_lands_first() {
        let mut input = InferInput::new("question");
        input.system = Some("you are terse".to_owned());
        let messages = base_messages(&input, true);
        assert_eq!(messages.len(), 2);
        assert!(matches!(messages[0].role, Role::System));
        assert!(matches!(messages[1].role, Role::User));
    }

    #[test]
    fn instruction_fallback_only_without_native_support() {
        let mut input = InferInput::new("question");
        input.schema = Some(json!({ "type": "object" }));
        let native = base_messages(&input, true);
        let fallback = base_messages(&input, false);
        let text_of = |m: &Message| match &m.content[0] {
            ContentBlock::Text { text } => text.clone(),
            _ => String::new(),
        };
        assert_eq!(text_of(&native[0]), "question");
        assert!(text_of(&fallback[0]).contains("JSON Schema"));
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
        let messages = base_messages(&input, true);
        // Brouillon shape: [System, User] · prompt verbatim · single block.
        assert_eq!(messages.len(), 2);
        assert!(matches!(messages[0].role, Role::System));
        assert_eq!(messages[1].content.len(), 1);
        assert!(matches!(
            &messages[1].content[0],
            ContentBlock::Text { text } if text == "What is the capital of France?"
        ));
        let req = build_request(&input, "echo", messages, true);
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
    fn request_carries_params_and_native_schema() {
        let mut input = InferInput::new("q");
        input.temperature = Some(0.7);
        input.max_tokens = Some(64);
        input.schema = Some(json!({ "type": "object" }));
        let req = build_request(&input, "claude-x", base_messages(&input, true), true);
        assert_eq!(req.model, "claude-x");
        assert_eq!(req.temperature, Some(0.7));
        assert_eq!(req.max_tokens, Some(64));
        assert!(matches!(req.response_format, ResponseFormat::JsonSchema(_)));
        let req_no_native = build_request(&input, "m", base_messages(&input, false), false);
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
            // Total function — must not panic.
            let v = crate::structured::compile_schema(&serde_json::json!({ "type": "object" }))
                .expect("trivial schema compiles");
            let _ = crate::structured::extract_and_validate(&s, &v);
        }
    }
}
