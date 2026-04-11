// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Nika `infer:` verb — LLM inference via the kernel `Provider` trait.
//!
//! This crate contains the core inference call for the `infer:` verb.
//! It receives pre-validated, pre-resolved inputs from the engine bridge
//! (prompt already spotlight-wrapped, skills already loaded, canary
//! already injected, schema instruction already appended) and dispatches
//! a single `Provider::infer(InferRequest)` call via the kernel trait.
//!
//! ## Scope for S14
//!
//! The S14 minimum-extraction handles the non-streaming, text-only
//! inference path. This is the ~90% workflow:
//!
//! - Build an `InferRequest` from the input (prompt → user message,
//!   system → system message, no tools, no vision)
//! - Call `caps.provider.infer(request)` via the kernel trait
//! - Emit a `ProviderResponded` event via `EventLog`
//! - Return the text content + token usage
//!
//! ## Out of scope for S14 (stays in engine bridge)
//!
//! - **Streaming**: `infer_stream()` path with `StreamChunk` channel
//!   and TUI-bound progress events. The engine's streaming display
//!   couples to internal types that don't belong in the verb crate.
//! - **Structured output 5-layer defense**: `StructuredOutputEngine`
//!   retry loop (Layer 2-4) stays in the engine. The verb crate
//!   returns raw text; the engine runs the retry loop on that text.
//! - **Vision**: `ContentBlock::Image` construction + staging couples
//!   to `nika-media` types. Vision path stays in the engine bridge.
//! - **Tool injection (Layer 0b)**: `DynamicSubmitTool` +
//!   `infer_with_tools` path stays in the engine.
//!
//! Session 15 extends this crate with streaming + structured output
//! retry once the engine's structured-retry types can move behind
//! the kernel trait surface.

use std::sync::Arc;
use std::time::Instant;

use nika_event::types::FinishReason;
use nika_event::EventLog;
use nika_kernel::caps::InferCaps;
use nika_kernel::provider::{
    ContentBlock, InferRequest, InferResponse, Message, ProviderExtras, ResponseFormat, Role,
    StopReason, ToolChoice,
};

mod emit;
mod error;

pub use emit::emit_provider_responded;
pub use error::VerbInferError;

/// Pre-validated, pre-resolved input for the infer verb's core call.
///
/// The engine bridge builds this after all of:
/// - template resolution
/// - spotlight wrapping (Nika Shield L2)
/// - skills injection
/// - response-format instruction
/// - canary token injection
/// - schema instruction (for structured: tasks)
/// - provider chain resolution
pub struct InferInput<'a> {
    /// Fully-resolved user prompt. Must be non-empty.
    pub prompt: &'a str,
    /// Fully-resolved system prompt (skills + canary + schema already applied).
    pub system: Option<&'a str>,
    /// Provider model identifier (e.g. `"claude-sonnet-4-6"`).
    pub model: &'a str,
    /// Sampling temperature in [0.0, 2.0]; `None` = provider default.
    pub temperature: Option<f32>,
    /// Maximum output tokens; `None` = provider default.
    pub max_tokens: Option<u32>,
    /// Extended-thinking budget in tokens; `None` = no thinking.
    pub thinking_budget: Option<u32>,
    /// Provider-specific pass-through parameters (e.g. OpenAI
    /// `response_format: json_schema` when the engine chose Layer 0a).
    pub extra: ProviderExtras,
    /// Task ID for event emission.
    pub task_id: Arc<str>,
}

/// Result of a successful infer call.
#[derive(Debug, Clone)]
pub struct InferOutput {
    /// The provider's text response (concatenation of all Text content
    /// blocks). Any `Thinking` or `ToolUse` blocks are dropped — the
    /// verb crate handles only the text path in S14.
    pub text: String,
    /// Full provider response metadata for the engine bridge to use
    /// in event emission, structured output retries, and cost
    /// accounting.
    pub response: InferResponse,
}

/// Execute a text-only inference call via the kernel `Provider` trait.
///
/// Returns [`InferOutput`] containing the extracted text and the full
/// [`InferResponse`] for the engine bridge to use in event emission and
/// downstream processing.
///
/// # Cancellation
///
/// Races the provider call against `caps.cancel.cancelled()`. On
/// cancellation returns [`VerbInferError::Cancelled`] without waiting
/// for the provider future to complete.
///
/// # Events
///
/// Emits one `ProviderResponded` event on success, capturing model,
/// token usage, request_id, cost, and duration.
pub async fn run(
    input: &InferInput<'_>,
    caps: &InferCaps<'_>,
    event_log: &EventLog,
) -> Result<InferOutput, VerbInferError> {
    // Validation mirrors the engine's own validate() checks so the
    // verb crate can be called directly from dispatch() without relying
    // on the engine bridge's pre-validation.
    if input.prompt.trim().is_empty() {
        return Err(VerbInferError::Validation {
            reason: "prompt is empty after resolution".to_string(),
        });
    }

    let start = Instant::now();

    let request = build_infer_request(input);

    // Race the provider call against cancellation.
    let response: InferResponse = tokio::select! {
        biased;
        _ = caps.cancel.cancelled() => {
            return Err(VerbInferError::Cancelled {
                task_id: input.task_id.to_string(),
            });
        }
        r = caps.provider.infer(request) => r?,
    };

    // `start` held for symmetry with the engine's timing telemetry;
    // ProviderResponded does not carry duration_ms, so we drop it.
    let _ = start;

    // Extract the concatenated text from the response content blocks.
    // `Thinking`, `ToolUse`, and `ToolResult` blocks are skipped — the
    // verb crate handles only text in S14.
    let text = extract_text(&response.content);

    // Emit ProviderResponded via the shared helper (W16-A0). `cost_usd`
    // defaults to 0.0 when `InferResponse::cost_usd` is `None` because
    // the engine bridge paths still compute cost locally — that unwrap
    // moves to the caller in W14-B2 once the engine flips to the
    // helper directly with its own cost calculation.
    //
    // W16-B3: `stop_reason_to_finish_reason` now takes the raw provider
    // string so `StopReason::ContentFilter` + `finish_reason_raw:
    // Some("policy_violation")` preserves the extra specificity instead
    // of collapsing to the hardcoded `"content_filter"` string.
    emit_provider_responded(
        event_log,
        &input.task_id,
        response.request_id.clone(),
        response.usage.input_tokens,
        response.usage.output_tokens,
        response.usage.cache_read_tokens.unwrap_or(0),
        response.ttft_ms,
        stop_reason_to_finish_reason(
            &response.stop_reason,
            response.finish_reason_raw.as_deref(),
        ),
        response.cost_usd.unwrap_or(0.0),
    );

    Ok(InferOutput { text, response })
}

/// Map the kernel [`StopReason`] + optional raw provider string to the
/// event-log [`FinishReason`].
///
/// Invariant #21 (S14) — the two enums have distinct variant names
/// because the kernel stays neutral to event-log types, and this function
/// is the single place that bridges them.
///
/// ## W16-B3 — `finish_reason_raw` consumption (option (ii))
///
/// Before W16-B3, the mapping hardcoded `"content_filter"` as the
/// `FinishReason::Other` payload for `StopReason::ContentFilter`, which
/// masked provider-specific strings (OpenAI `"length"`, Anthropic
/// `"safety"`, Gemini `"policy_violation"`, …).
///
/// The new mapping uses the explicit `StopReason` variants as the
/// authoritative path for the four typed variants (`EndTurn`,
/// `MaxTokens`, `StopSequence`, `ToolUse`) — these are specific enough
/// that the raw string would not add information. For `ContentFilter`
/// and `Unknown` — the two cases that currently lose provider specificity
/// — the helper now prefers `finish_reason_raw` when present and falls
/// back to the previous hardcoded/internal string otherwise.
///
/// This is "option (ii)" from the Phase 1 rust-async-expert audit: typed
/// safety where the kernel has a variant, string fidelity where it does
/// not. An `Other(raw)` flow-through for all six variants would drop
/// typed semantics for `MaxTokens` / `StopSequence` / `ToolUse` even when
/// providers happen to return a `"length"` or `"tool_calls"` raw string
/// — not worth the downstream ambiguity.
fn stop_reason_to_finish_reason(
    reason: &StopReason,
    finish_reason_raw: Option<&str>,
) -> FinishReason {
    match reason {
        StopReason::EndTurn => FinishReason::EndTurn,
        StopReason::MaxTokens => FinishReason::MaxTokens,
        StopReason::StopSequence => FinishReason::StopSequence,
        StopReason::ToolUse => FinishReason::ToolUse,
        StopReason::ContentFilter => {
            // Prefer the provider's verbatim string when available
            // (e.g. "policy_violation", "safety"), otherwise fall back
            // to the typed default — the pre-W16-B3 behavior.
            FinishReason::Other(
                finish_reason_raw
                    .map(str::to_string)
                    .unwrap_or_else(|| "content_filter".to_string()),
            )
        }
        StopReason::Unknown(s) => {
            // Unknown already carries a raw string in `s`, but the
            // external `finish_reason_raw` is MORE authoritative because
            // it is populated by the provider adapter directly whereas
            // `s` may have been synthesized from an internal fallback
            // during the adapter's StopReason construction.
            FinishReason::Other(
                finish_reason_raw
                    .map(str::to_string)
                    .unwrap_or_else(|| s.clone()),
            )
        }
    }
}

/// Build an `InferRequest` from a resolved `InferInput`.
///
/// System and user prompts become `Message` entries; tools and
/// response_format are currently pass-through.
fn build_infer_request(input: &InferInput<'_>) -> InferRequest {
    let mut messages = Vec::new();

    if let Some(system) = input.system {
        if !system.is_empty() {
            messages.push(Message {
                role: Role::System,
                content: vec![ContentBlock::Text {
                    text: system.to_string(),
                }],
            });
        }
    }

    messages.push(Message {
        role: Role::User,
        content: vec![ContentBlock::Text {
            text: input.prompt.to_string(),
        }],
    });

    InferRequest {
        model: input.model.to_string(),
        messages,
        temperature: input.temperature,
        max_tokens: input.max_tokens,
        tools: Vec::new(),
        tool_choice: ToolChoice::Auto,
        response_format: ResponseFormat::Text,
        stop_sequences: Vec::new(),
        thinking_budget: input.thinking_budget,
        extra: input.extra.clone(),
    }
}

/// Concatenate all `ContentBlock::Text { text }` blocks from the
/// response, ignoring thinking/tool blocks.
fn extract_text(content: &[ContentBlock]) -> String {
    let mut out = String::new();
    for block in content {
        if let ContentBlock::Text { text } = block {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(text);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_event::EventKind;
    use nika_kernel::clock::Clock;
    use nika_kernel::filesystem::FsRead;
    use nika_kernel::policy::PolicyChecker;
    use nika_kernel::provider::{Provider, ProviderError};
    use nika_kernel_mock::clock::MockClock;
    use nika_kernel_mock::filesystem::InMemoryFs;
    use nika_kernel_mock::policy::MockPolicyChecker;
    use nika_kernel_mock::MockProvider;

    fn make_caps<'a>(
        provider: Arc<dyn Provider>,
        fs: &'a dyn FsRead,
        policy: &'a dyn PolicyChecker,
        clock: &'a dyn Clock,
        cancel: &'a tokio_util::sync::CancellationToken,
    ) -> InferCaps<'a> {
        InferCaps::new(
            provider,
            fs,
            policy,
            clock,
            cancel,
            std::path::Path::new("/tmp"),
        )
    }

    fn make_input(prompt: &'static str) -> InferInput<'static> {
        InferInput {
            prompt,
            system: None,
            model: "mock-model",
            temperature: None,
            max_tokens: None,
            thinking_budget: None,
            extra: ProviderExtras::default(),
            task_id: Arc::from("test-task"),
        }
    }

    #[tokio::test]
    async fn infer_returns_text_from_mock_provider() {
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_text("Hello, Alice.");

        let fs = InMemoryFs::new();
        let policy = MockPolicyChecker::allow_all();
        let clock = MockClock::new();
        let cancel = tokio_util::sync::CancellationToken::new();
        let caps = make_caps(mock.clone(), &fs, &policy, &clock, &cancel);
        let event_log = EventLog::new();

        let input = make_input("Tell me about Alice");
        let output = run(&input, &caps, &event_log).await.unwrap();

        assert_eq!(output.text, "Hello, Alice.");
        assert_eq!(output.response.usage.output_tokens, 20);
    }

    #[tokio::test]
    async fn infer_records_request_details_on_provider() {
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_text("ok");

        let fs = InMemoryFs::new();
        let policy = MockPolicyChecker::allow_all();
        let clock = MockClock::new();
        let cancel = tokio_util::sync::CancellationToken::new();
        let caps = make_caps(mock.clone(), &fs, &policy, &clock, &cancel);
        let event_log = EventLog::new();

        let mut input = make_input("Explain Rust");
        input.system = Some("You are a helpful assistant.");
        input.temperature = Some(0.7);
        input.max_tokens = Some(512);

        run(&input, &caps, &event_log).await.unwrap();

        let captured = mock.captured_requests();
        assert_eq!(captured.len(), 1);
        let req = &captured[0];
        assert_eq!(req.model, "mock-model");
        assert_eq!(req.temperature, Some(0.7));
        assert_eq!(req.max_tokens, Some(512));
        // System + user = 2 messages.
        assert_eq!(req.messages.len(), 2);
        assert_eq!(req.messages[0].role, Role::System);
        assert_eq!(req.messages[1].role, Role::User);
    }

    #[tokio::test]
    async fn infer_emits_provider_responded_event() {
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_text("done");

        let fs = InMemoryFs::new();
        let policy = MockPolicyChecker::allow_all();
        let clock = MockClock::new();
        let cancel = tokio_util::sync::CancellationToken::new();
        let caps = make_caps(mock.clone(), &fs, &policy, &clock, &cancel);
        let event_log = EventLog::new();

        let input = make_input("ping");
        run(&input, &caps, &event_log).await.unwrap();

        // Assert concrete event fields — not a blanket matches!(.., { .. }).
        let events = event_log.events();
        let provider_responded = events
            .iter()
            .find_map(|e| match &e.kind {
                EventKind::ProviderResponded {
                    input_tokens,
                    output_tokens,
                    finish_reason,
                    ..
                } => Some((
                    *input_tokens,
                    *output_tokens,
                    finish_reason.clone(),
                )),
                _ => None,
            })
            .expect("expected ProviderResponded event");
        assert_eq!(provider_responded.0, 10);
        assert_eq!(provider_responded.1, 20);
        assert_eq!(provider_responded.2, FinishReason::EndTurn);
    }

    /// **S14-δ — golden oracle compliance (S12-G2).**
    ///
    /// `ProviderResponded` carries 8 fields. Testing only 3 of them
    /// (the previous test above) leaves 5 fields without a regression
    /// guard: `request_id`, `cache_read_tokens`, `ttft_ms`, `cost_usd`,
    /// and `task_id`. A silent breakage anywhere in the field wiring
    /// (wrong clone, wrong `.unwrap_or(0)`, wrong field name) would
    /// ship undetected.
    ///
    /// This test enqueues an explicit `InferResponse` with known
    /// values in every optional field and asserts each one on the
    /// emitted event. S12 invariant #14 ("never weaken the oracle")
    /// demands this coverage.
    #[tokio::test]
    async fn infer_emits_provider_responded_with_all_fields() {
        use nika_kernel::provider::{InferResponse, StopReason, TokenUsage};

        let mock = Arc::new(MockProvider::new("mock"));
        let mut response = InferResponse::new(
            vec![ContentBlock::Text {
                text: "synthesized".to_string(),
            }],
            TokenUsage {
                input_tokens: 42,
                output_tokens: 17,
                cache_read_tokens: Some(8),
                cache_write_tokens: Some(4),
            },
            StopReason::EndTurn,
        );
        response.request_id = Some("req_golden_01HZA".to_string());
        response.ttft_ms = Some(123);
        response.cost_usd = Some(0.004_2);
        mock.enqueue_response(response);

        let fs = InMemoryFs::new();
        let policy = MockPolicyChecker::allow_all();
        let clock = MockClock::new();
        let cancel = tokio_util::sync::CancellationToken::new();
        let caps = make_caps(mock.clone(), &fs, &policy, &clock, &cancel);
        let event_log = EventLog::new();

        let input = make_input("test");
        run(&input, &caps, &event_log).await.unwrap();

        let events = event_log.events();
        let (task_id, request_id, input_tokens, output_tokens, cache_read_tokens,
             ttft_ms, finish_reason, cost_usd) = events
            .iter()
            .find_map(|e| match &e.kind {
                EventKind::ProviderResponded {
                    task_id,
                    request_id,
                    input_tokens,
                    output_tokens,
                    cache_read_tokens,
                    ttft_ms,
                    finish_reason,
                    cost_usd,
                } => Some((
                    Arc::clone(task_id),
                    request_id.clone(),
                    *input_tokens,
                    *output_tokens,
                    *cache_read_tokens,
                    *ttft_ms,
                    finish_reason.clone(),
                    *cost_usd,
                )),
                _ => None,
            })
            .expect("expected ProviderResponded event");

        // task_id threads through from InferInput.
        assert_eq!(task_id.as_ref(), "test-task");
        // request_id comes from InferResponse.request_id verbatim.
        assert_eq!(request_id.as_deref(), Some("req_golden_01HZA"));
        // Token counts come from TokenUsage.
        assert_eq!(input_tokens, 42);
        assert_eq!(output_tokens, 17);
        // cache_read_tokens unwraps to u64 (event uses u64, not Option).
        assert_eq!(cache_read_tokens, 8);
        // ttft_ms stays Option.
        assert_eq!(ttft_ms, Some(123));
        // StopReason::EndTurn → FinishReason::EndTurn via local mapping.
        assert_eq!(finish_reason, FinishReason::EndTurn);
        // cost_usd unwraps to f64 (event uses f64, not Option). This is a
        // pure pass-through with no arithmetic, so exact equality is both
        // correct AND tighter than any epsilon comparison would be. S14-δ
        // shipped with `< f64::EPSILON` which is mathematically wrong at
        // magnitude 0.0042 (ULP ≈ 9.3e-19, not EPSILON ≈ 2.22e-16) — the
        // assertion accepted up to ~5.3e-14 relative error silently. S14.5
        // hotfix: the response.cost_usd literal and the assertion literal
        // are the same `0.004_2f64` bit pattern, so assert_eq! is exact.
        assert_eq!(
            cost_usd, 0.004_2_f64,
            "cost_usd should thread through verbatim (pure pass-through, no arithmetic)"
        );
    }

    #[tokio::test]
    async fn infer_rejects_empty_prompt() {
        let mock = Arc::new(MockProvider::new("mock"));

        let fs = InMemoryFs::new();
        let policy = MockPolicyChecker::allow_all();
        let clock = MockClock::new();
        let cancel = tokio_util::sync::CancellationToken::new();
        let caps = make_caps(mock, &fs, &policy, &clock, &cancel);
        let event_log = EventLog::new();

        let input = make_input("   ");
        let result = run(&input, &caps, &event_log).await;
        assert!(matches!(result, Err(VerbInferError::Validation { .. })));
    }

    #[tokio::test]
    async fn infer_cancelled_returns_cancelled() {
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_text("ignored");

        let fs = InMemoryFs::new();
        let policy = MockPolicyChecker::allow_all();
        let clock = MockClock::new();
        let cancel = tokio_util::sync::CancellationToken::new();
        cancel.cancel(); // pre-cancel
        let caps = make_caps(mock, &fs, &policy, &clock, &cancel);
        let event_log = EventLog::new();

        let input = make_input("Should not run");
        let result = run(&input, &caps, &event_log).await;
        assert!(matches!(result, Err(VerbInferError::Cancelled { .. })));
    }

    #[tokio::test]
    async fn infer_propagates_provider_error() {
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_error(ProviderError::RateLimited {
            retry_after_ms: 1000,
        });

        let fs = InMemoryFs::new();
        let policy = MockPolicyChecker::allow_all();
        let clock = MockClock::new();
        let cancel = tokio_util::sync::CancellationToken::new();
        let caps = make_caps(mock, &fs, &policy, &clock, &cancel);
        let event_log = EventLog::new();

        let input = make_input("test");
        let result = run(&input, &caps, &event_log).await;
        assert!(matches!(
            result,
            Err(VerbInferError::Provider(ProviderError::RateLimited { .. }))
        ));
    }

    // =========================================================================
    // W16-B3 — finish_reason_raw consumption (option (ii))
    // =========================================================================

    /// Helper — run `run()` once with a fully-controlled `InferResponse`,
    /// return the `FinishReason` emitted on the `ProviderResponded` event.
    /// Centralized so the 4 W16-B3 tests below stay focused on the
    /// `(StopReason, finish_reason_raw)` tuple under test.
    async fn run_and_capture_finish_reason(
        stop_reason: StopReason,
        finish_reason_raw: Option<&str>,
    ) -> FinishReason {
        use nika_kernel::provider::{InferResponse, TokenUsage};

        let mock = Arc::new(MockProvider::new("mock"));
        let mut response = InferResponse::new(
            vec![ContentBlock::Text {
                text: "ok".to_string(),
            }],
            TokenUsage {
                input_tokens: 5,
                output_tokens: 3,
                cache_read_tokens: None,
                cache_write_tokens: None,
            },
            stop_reason,
        );
        response.finish_reason_raw = finish_reason_raw.map(str::to_string);
        mock.enqueue_response(response);

        let fs = InMemoryFs::new();
        let policy = MockPolicyChecker::allow_all();
        let clock = MockClock::new();
        let cancel = tokio_util::sync::CancellationToken::new();
        let caps = make_caps(mock, &fs, &policy, &clock, &cancel);
        let event_log = EventLog::new();

        let input = make_input("probe");
        run(&input, &caps, &event_log).await.unwrap();

        let events = event_log.events();
        let EventKind::ProviderResponded { finish_reason, .. } = events
            .iter()
            .find_map(|e| {
                if matches!(e.kind, EventKind::ProviderResponded { .. }) {
                    Some(&e.kind)
                } else {
                    None
                }
            })
            .expect("run must emit exactly one ProviderResponded event")
        else {
            unreachable!("guarded by the find_map filter above")
        };
        finish_reason.clone()
    }

    /// `StopReason::ContentFilter` + `finish_reason_raw: Some(provider_string)`
    /// must preserve the raw string via `FinishReason::Other(provider_string)`
    /// instead of collapsing to the pre-W16-B3 hardcoded `"content_filter"`.
    /// This is the primary W16-B3 acceptance test — it failed before the
    /// `stop_reason_to_finish_reason` signature change.
    #[tokio::test]
    async fn infer_content_filter_prefers_finish_reason_raw() {
        let finish_reason = run_and_capture_finish_reason(
            StopReason::ContentFilter,
            Some("policy_violation"),
        )
        .await;
        assert_eq!(
            finish_reason,
            FinishReason::Other("policy_violation".to_string()),
            "ContentFilter should surface the raw provider string, not the hardcoded default"
        );
    }

    /// `StopReason::ContentFilter` with `finish_reason_raw: None` must
    /// fall back to the pre-W16-B3 hardcoded `"content_filter"` string
    /// so the migration is backwards-compatible for adapters that do not
    /// plumb raw strings through.
    #[tokio::test]
    async fn infer_content_filter_defaults_when_no_raw() {
        let finish_reason =
            run_and_capture_finish_reason(StopReason::ContentFilter, None).await;
        assert_eq!(
            finish_reason,
            FinishReason::Other("content_filter".to_string()),
            "ContentFilter without raw must fall back to the pre-W16-B3 default"
        );
    }

    /// `StopReason::Unknown(internal)` + `finish_reason_raw: Some(external)`
    /// must prefer the external raw string over the internal `s` payload,
    /// because the external field is populated by the provider adapter
    /// directly whereas the internal `s` may have been synthesized from
    /// an adapter fallback during `StopReason` construction.
    #[tokio::test]
    async fn infer_unknown_stop_reason_prefers_external_raw_over_internal() {
        let finish_reason = run_and_capture_finish_reason(
            StopReason::Unknown("internal_fallback".to_string()),
            Some("server_rejected_overloaded"),
        )
        .await;
        assert_eq!(
            finish_reason,
            FinishReason::Other("server_rejected_overloaded".to_string()),
            "Unknown should prefer external finish_reason_raw over the internal StopReason::Unknown payload"
        );
    }

    /// `StopReason::Unknown(internal)` with `finish_reason_raw: None`
    /// falls back to the internal `s` payload — existing behavior.
    #[tokio::test]
    async fn infer_unknown_stop_reason_uses_internal_when_no_raw() {
        let finish_reason = run_and_capture_finish_reason(
            StopReason::Unknown("only_internal".to_string()),
            None,
        )
        .await;
        assert_eq!(
            finish_reason,
            FinishReason::Other("only_internal".to_string()),
            "Unknown without raw must fall back to the internal payload"
        );
    }

    /// Typed variants (`EndTurn`, `MaxTokens`, `StopSequence`, `ToolUse`)
    /// must ignore `finish_reason_raw` entirely — the typed semantics are
    /// authoritative. W16-B3 rust-async-expert option (ii) — do NOT let a
    /// provider that happens to emit `"length"` raw override the typed
    /// `FinishReason::MaxTokens` distinction that downstream tooling uses.
    #[tokio::test]
    async fn infer_typed_stop_reasons_ignore_finish_reason_raw() {
        let cases = [
            (StopReason::EndTurn, FinishReason::EndTurn),
            (StopReason::MaxTokens, FinishReason::MaxTokens),
            (StopReason::StopSequence, FinishReason::StopSequence),
            (StopReason::ToolUse, FinishReason::ToolUse),
        ];
        for (stop, expected) in cases {
            let stop_label = format!("{stop:?}");
            // Provider raw that would falsely imply a different typed variant.
            let got =
                run_and_capture_finish_reason(stop, Some("length_or_tool_or_whatever")).await;
            assert_eq!(
                got, expected,
                "{stop_label}: typed FinishReason must win over finish_reason_raw"
            );
        }
    }

    #[test]
    fn extract_text_concatenates_text_blocks_only() {
        let content = vec![
            ContentBlock::Text {
                text: "hello".to_string(),
            },
            ContentBlock::Thinking {
                text: "should be dropped".to_string(),
            },
            ContentBlock::Text {
                text: "world".to_string(),
            },
        ];
        let result = extract_text(&content);
        assert_eq!(result, "hello\nworld");
    }

    #[test]
    fn build_infer_request_emits_user_only_when_no_system() {
        let input = make_input("just a prompt");
        let request = build_infer_request(&input);
        assert_eq!(request.messages.len(), 1);
        assert_eq!(request.messages[0].role, Role::User);
    }

    #[test]
    fn build_infer_request_includes_system_and_user() {
        let mut input = make_input("question");
        input.system = Some("You are helpful.");
        let request = build_infer_request(&input);
        assert_eq!(request.messages.len(), 2);
        assert_eq!(request.messages[0].role, Role::System);
        assert_eq!(request.messages[1].role, Role::User);
    }
}
