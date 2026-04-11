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
    emit_provider_responded(
        event_log,
        &input.task_id,
        response.request_id.clone(),
        response.usage.input_tokens,
        response.usage.output_tokens,
        response.usage.cache_read_tokens.unwrap_or(0),
        response.ttft_ms,
        stop_reason_to_finish_reason(&response.stop_reason),
        response.cost_usd.unwrap_or(0.0),
    );

    Ok(InferOutput { text, response })
}

/// Map the kernel [`StopReason`] to the event-log [`FinishReason`].
///
/// The two enums overlap but have different variant names because the
/// kernel stays neutral to event-log types. This function centralizes
/// the cross-layer mapping so there is only one place to update if
/// either enum grows a variant.
fn stop_reason_to_finish_reason(reason: &StopReason) -> FinishReason {
    match reason {
        StopReason::EndTurn => FinishReason::EndTurn,
        StopReason::MaxTokens => FinishReason::MaxTokens,
        StopReason::StopSequence => FinishReason::StopSequence,
        StopReason::ToolUse => FinishReason::ToolUse,
        StopReason::ContentFilter => FinishReason::Other("content_filter".to_string()),
        StopReason::Unknown(s) => FinishReason::Other(s.clone()),
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
