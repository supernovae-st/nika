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
//! Streaming passthrough (`verb-agent`/engine surface) · CAS vision
//! staging (`nika-media-*` · deferred; file/url refs ARE wired) ·
//! `${{ }}` resolution (upstream binding) · transport retry/backoff
//! (engine scheduler policy — only the schema-validation retry lives
//! here).
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
mod vision;

use nika_types::cost::SpendOnFailure;
use std::sync::Arc;

use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, Message, ProviderError, ProviderInferDyn,
    ProviderMeta, ResponseFormat, Role, StopReason, TokenUsage,
};
use nika_kernel::http::HttpPostDyn;
use nika_providers::ProviderRegistry;

pub use errors::VerbInferError;
pub use vision::VisionPart;

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
    /// `infer.vision:` — local files are inlined as `data:` URLs; remote
    /// URLs stay URLs. Empty means text-only (the historical path).
    pub vision: Vec<VisionPart>,
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
            vision: Vec::new(),
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

/// A subscription-seat result. It deliberately has no usage, price, or
/// responding-model field: the terminal receipt records only the seat and
/// the model the workflow requested.
#[cfg(feature = "access-harness")]
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct HarnessInferOutput {
    /// Text or locally schema-validated JSON in the runtime value plane.
    pub output: serde_json::Value,
    /// The author-requested model identity, not a claim about the responder.
    pub requested_model: String,
}

#[cfg(feature = "access-harness")]
impl HarnessInferOutput {
    /// Construct without any numeric meter or responder identity.
    #[must_use]
    pub fn new(output: serde_json::Value, requested_model: impl Into<String>) -> Self {
        Self {
            output,
            requested_model: requested_model.into(),
        }
    }
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

    /// The EFFECTIVE default a model-less task runs on (`--model` ||
    /// envelope `model:`) — the runtime keys its access lane on it.
    #[must_use]
    pub fn default_model(&self) -> &str {
        &self.default_model
    }

    /// Execute one `infer:` through a proved subscription harness seat.
    ///
    /// This is a single-shot lane: schema failure never retries and no
    /// numeric usage leaves the harness adapter.
    ///
    /// # Errors
    ///
    /// Invalid task parameters, an unattested seat, adapter execution
    /// failure, or a final value that violates the task schema.
    #[cfg(feature = "access-harness")]
    pub async fn run_on_harness(
        &self,
        seat_id: &str,
        input: InferInput,
    ) -> Result<HarnessInferOutput, VerbInferError> {
        validate_params(&input)?;
        let validator = match input.schema.as_ref() {
            Some(schema) => Some(structured::compile_schema(schema).map_err(|detail| {
                VerbInferError::InvalidParam {
                    param: "schema",
                    detail,
                }
            })?),
            None => None,
        };
        let need = if input.schema.is_some() {
            nika_harness::StructuredOutputGrade::JsonSchema
        } else {
            nika_harness::StructuredOutputGrade::Text
        };
        let seat = nika_harness::meet_infer_grade(seat_id, need).map_err(|err| {
            VerbInferError::HarnessAccess {
                detail: err.to_string(),
            }
        })?;
        let requested_model = input
            .model
            .clone()
            .unwrap_or_else(|| self.default_model.clone());
        let request = nika_harness::HarnessInferRequest::new(input.prompt, &requested_model)
            .with_system(input.system)
            .with_schema(input.schema.clone())
            .with_timeout(input.timeout);
        let outcome = seat
            .run(request)
            .await
            .map_err(|err| VerbInferError::HarnessAccess {
                detail: err.to_string(),
            })?;
        debug_assert_eq!(outcome.requested_model, requested_model);
        debug_assert!(outcome.usage_observed);
        let output = match (input.schema.as_ref(), validator.as_ref()) {
            (Some(schema), Some(validator)) => {
                match structured::extract_and_validate(&outcome.output, validator, schema) {
                    structured::Validation::Valid(value) => value,
                    structured::Validation::Invalid(errors) => {
                        return Err(VerbInferError::SchemaValidation {
                            attempts: 1,
                            detail: errors.join("; "),
                            spend: Box::default(),
                        });
                    }
                }
            }
            _ => serde_json::Value::String(outcome.output),
        };
        Ok(HarnessInferOutput::new(output, requested_model))
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
    /// [`VerbInferError::InvalidParam`] on an empty prompt, out-of-range
    /// temperature, or a missing `vision:` file ·
    /// [`VerbInferError::ModelResolution`] when the model string resolves
    /// to no profile · [`VerbInferError::ProviderCall`]
    /// when the provider round-trip fails ·
    /// [`VerbInferError::SchemaValidation`] when a `schema:` task exhausts
    /// the retry budget without a conforming reply ·
    /// [`VerbInferError::EmptyAnswer`] when the provider spent tokens yet
    /// the visible answer is blank (NIKA-INFER-004 · #651).
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
        // B-5: a silent/mute local server refuses HERE — before any wire call.
        refuse_unlive_local(&self.registry, model)?;
        let wire = schema_wire(
            &input,
            provider.supports_response_format(),
            provider.strict_schema_rejects_underspecified(),
        );

        let mut messages = base_messages(&input, wire);
        attach_vision(&mut messages, &input.vision)?;
        // u32 counter: a u8 would saturate at budget = u8::MAX and loop
        // forever on paid calls (review lens 1 · P1).
        let mut attempts: u32 = 0;
        // EVERY round-trip's usage folds in — a schema retry is a real paid
        // call and the ledger bills `InferOutput::usage`. Keeping only the
        // final response's usage under-billed retried tasks by up to
        // budget+1 × (the cost-undercount finding · deep review 2026-07-07).
        let mut usage_total = TokenUsage::default();
        // Failure decoration — billed round-trips ride the error.
        let incurred =
            |u: &TokenUsage| Box::new(SpendOnFailure::new(u.clone(), None, Some(model.to_owned())));
        loop {
            attempts += 1;
            let request = build_request(&input, provider.name(), messages.clone(), wire);
            let response =
                provider
                    .infer(request)
                    .await
                    .map_err(|source| VerbInferError::ProviderCall {
                        source,
                        spend: incurred(&usage_total),
                    })?;
            usage_total.absorb(&response.usage);
            // R3-F1 (2026-07-29 audit · run 3 · the agent loop's own
            // `NIKA-AGENT-005` sibling): the usage-absence gate.
            refuse_unmetered(&response, model, incurred(&usage_total))?;
            let text = response_text(&response);

            let (Some(schema), Some(validator)) = (input.schema.as_ref(), validator.as_ref())
            else {
                return finish_text_lane(text, model, response, usage_total);
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
                    let truncated = matches!(response.stop_reason, StopReason::MaxTokens);
                    if truncated || attempts > u32::from(self.schema_retry_budget) {
                        return Err(schema_failure(
                            attempts,
                            &errors,
                            &response.stop_reason,
                            incurred(&usage_total),
                        ));
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

/// The B-5 gate (the sibling of [`refuse_unmetered`] — a named refusal
/// BEFORE the hang, never a silent one): probe the local endpoint a
/// server-backed keyless model would call, and refuse FAST when it is
/// silent (nothing listening · DNS stalled · blackholed) or mute
/// (accepts, never speaks). `None` from the gate — a keyed cloud
/// provider, `mock`, an unknown provider, a probe that itself failed —
/// never blocks a run: the gate is a diagnostic, the transport's own
/// errors stay the fallback truth.
///
/// The call is blocking IO on the executor (this crate is deliberately
/// tokio-free in prod): milliseconds against a live engine, ≤1.5s on
/// the dead path — the cold path that used to cost the user a kill.
fn refuse_unlive_local<H>(
    registry: &Arc<ProviderRegistry<H>>,
    model: &str,
) -> Result<(), VerbInferError>
where
    H: HttpPostDyn + Send + Sync + 'static,
{
    use nika_providers::probe::LocalLiveness;

    let (addr, cause) = match nika_providers::probe::local_run_gate(registry, model) {
        None | Some(LocalLiveness::Live(_)) => return Ok(()),
        Some(LocalLiveness::Mute(addr)) => (
            addr,
            "accepts connections but never answers HTTP (1s cap) — a stuck server",
        ),
        Some(LocalLiveness::Silent(addr)) => (
            addr,
            "nothing answers there (300ms cap) — no server listening",
        ),
    };
    Err(VerbInferError::ProviderCall {
        source: ProviderError::Other {
            reason: format!(
                "local endpoint {addr}: {cause} — start the engine (e.g. `ollama serve`) \
                 or rehearse keyless with `mock/echo`; the run stopped BEFORE any wire call"
            ),
        },
        // Zero billed, zero signal — the gate fired before any round-trip.
        spend: Box::default(),
    })
}

/// The R3-F1 gate (extracted under the fn-length law · the agent loop's
/// `NIKA-AGENT-005` sibling): a priced backend that omits the usage
/// block would bill this task $0 in the ledger while charging real
/// money — fail CLOSED; a mock/local zero is a TRUE zero (the
/// documented unmetered carve-out), never an invented number.
fn refuse_unmetered(
    response: &InferResponse,
    model: &str,
    spend: Box<SpendOnFailure>,
) -> Result<(), VerbInferError> {
    if !response.usage_reported && nika_catalog::find_pricing_for(model).is_some() {
        return Err(VerbInferError::UsageUnmetered {
            model: model.to_owned(),
            spend,
        });
    }
    Ok(())
}

/// The plain-text lane's exit — the #651 gate lives on this path (OBS-E
/// promoted): a blank visible answer PAIRED with token spend settles
/// FAILED (NIKA-INFER-004) — the warn-only era let the run finish green
/// over `""`. Split from `run` under the 100-line fn ratchet.
fn finish_text_lane(
    text: String,
    model: &str,
    response: InferResponse,
    usage: TokenUsage,
) -> Result<InferOutput, VerbInferError> {
    refuse_blank_answer(&text, &usage, model)?;
    Ok(InferOutput::new(
        InferValue::Text(text),
        model.to_owned(),
        response,
        usage,
    ))
}

/// The #651 gate (OBS-E promoted from a runtime warn to a typed failure):
/// a thinking model (gemini-2.5-flash · openai o-series · anthropic
/// extended-thinking) under a tight `max_tokens` can spend the whole
/// budget on its reasoning trace and conclude with a BLANK visible
/// answer — the task used to settle green over `""` and every downstream
/// `${{ tasks.X.output }}` silently resolved to nothing.
///
/// The signal is the **blank visible answer paired with token spend**,
/// under either report shape:
///
/// - a REPORTED reasoning split (`thinking_tokens` · anthropic
///   extended-thinking; `reasoning_tokens` · `OpenAI` o-series; the
///   gemini wire folds `thoughtsTokenCount` INTO `output_tokens`, so its
///   heavy-think empty answer reports `output_tokens == thoughts > 0`), or
/// - NO split at all with `output_tokens > 0` (#410 · the ollama path
///   strips the think block upstream and reports one undifferentiated
///   count: 512 tokens consumed, 2 bytes visible — the exact footgun).
///
/// A blank answer with ZERO tokens of any kind stays GREEN — nothing was
/// spent, so this is a plain empty completion, not the thinking footgun
/// (the warn's carve-out, preserved). The gate sits on the plain-text
/// lane: under a `schema:`, an empty REPLY already dies NIKA-INFER-002 at
/// extraction, and a schema-VALIDATED empty container (`[]` · `{}`) is a
/// legitimate conformant answer, never the footgun.
fn refuse_blank_answer(text: &str, usage: &TokenUsage, model: &str) -> Result<(), VerbInferError> {
    if !text.trim().is_empty() {
        return Ok(());
    }
    let reasoning = usage
        .thinking_tokens
        .unwrap_or(0)
        .saturating_add(usage.reasoning_tokens.unwrap_or(0));
    let detail = if reasoning > 0 {
        format!(
            "max_tokens likely too low for a thinking model (reasoning consumed {reasoning} tokens)"
        )
    } else if usage.output_tokens > 0 {
        format!(
            "the provider consumed {} tokens yet the visible answer is empty — a thinking model \
             may have spent the budget inside its think block (raise max_tokens, or use a \
             no-think variant)",
            usage.output_tokens
        )
    } else {
        return Ok(());
    };
    Err(VerbInferError::EmptyAnswer {
        model: model.to_owned(),
        detail,
        spend: Box::new(SpendOnFailure::new(
            usage.clone(),
            None,
            Some(model.to_owned()),
        )),
    })
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

/// Append loaded image blocks onto the user turn. A missing local file
/// fails here — before the provider loop spends anything.
fn attach_vision(messages: &mut [Message], parts: &[VisionPart]) -> Result<(), VerbInferError> {
    if parts.is_empty() {
        return Ok(());
    }
    let images = vision::vision_blocks(parts)?;
    let Some(user) = messages.last_mut() else {
        return Err(VerbInferError::InvalidParam {
            param: "vision",
            detail: "internal: user message missing before vision attach".to_owned(),
        });
    };
    user.content.extend(images);
    Ok(())
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
/// The terminal schema failure. Reached on either: (a) the retry budget
/// is spent, or (b) TRUNCATION fast-fail — a reply cut at `max_tokens`
/// cannot be repaired by re-asking at the SAME budget (the identical
/// request cuts again, and the retry message makes the prompt LONGER),
/// so every re-ask is a paid call spent on a known failure class whose
/// remedy is the budget, not the schema. Providers document
/// length-exhaustion as its own escape hatch (structured output
/// explicitly does NOT hold across it); the `stop_reason_hint` prints
/// the actionable fix either way. The billed round-trips ride the error
/// as `spend` — a failed task is not a refunded task.
fn schema_failure(
    attempts: u32,
    errors: &[String],
    stop: &StopReason,
    spend: Box<SpendOnFailure>,
) -> VerbInferError {
    let detail = format!("{}{}", errors.join("; "), stop_reason_hint(stop));
    VerbInferError::SchemaValidation {
        attempts,
        detail,
        spend,
    }
}

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
mod tests;

#[cfg(test)]
mod proptests;
