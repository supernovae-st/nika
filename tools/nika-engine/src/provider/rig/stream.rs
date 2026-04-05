//! Streaming types and response consumer for rig provider inference.
//!
//! Contains `StreamChunk` (TUI communication), `StreamResult` (complete response),
//! and `consume_rig_stream()` (shared streaming loop for all rig-core providers).

use futures::StreamExt;
use rig::completion::GetTokenUsage;
use rig::streaming::StreamedAssistantContent;
use std::time::Instant;
use tokio::sync::mpsc;
use tokio::time::timeout;

use super::error::RigInferError;
use crate::util::STREAM_CHUNK_TIMEOUT;

// =============================================================================
// StreamChunk - Communication type for streaming responses
// =============================================================================

/// Chunk of streaming response for real-time display
#[derive(Debug, Clone)]
pub enum StreamChunk {
    /// Text token from the model
    Token(String),
    /// Thinking/reasoning content (Claude extended thinking)
    Thinking(String),
    /// Stream completed successfully with final text
    Done(String),
    /// Stream failed with error
    Error(String),
    /// Token usage metrics (sent after completion)
    Metrics {
        input_tokens: u64,
        output_tokens: u64,
    },
    /// MCP server connected successfully
    McpConnected(String),
    /// MCP server connection failed
    McpError { server_name: String, error: String },
    // ═══════════════════════════════════════════════════════════════════════════
    // Chat Inline Box Events
    // ═══════════════════════════════════════════════════════════════════════════
    /// MCP tool call started (for inline visualization)
    McpCallStart {
        tool: String,
        server: String,
        params: String,
    },
    /// MCP tool call completed successfully
    McpCallComplete { result: String },
    /// MCP tool call failed
    McpCallFailed { error: String },
    /// Infer stream started (for inline visualization)
    InferStart {
        model: String,
        /// The user prompt text (for TaskBox::Infer display)
        prompt: String,
        prompt_tokens: u32,
        max_tokens: u32,
    },
    /// Infer stream token count update
    InferTokens { output_tokens: u32 },
    /// Infer stream completed
    InferComplete,
    // ═══════════════════════════════════════════════════════════════════════════
    // Activity Events for /exec, /fetch, /agent
    // ═══════════════════════════════════════════════════════════════════════════
    /// Shell command started (for activity stack)
    ExecStart { command: String },
    /// Shell command completed
    ExecComplete,
    /// HTTP fetch started (for activity stack)
    FetchStart { url: String, method: String },
    /// HTTP fetch completed
    FetchComplete,
    /// Agent loop started (for activity stack)
    AgentStart { goal: String },
    /// Agent loop completed
    AgentComplete,
    // ═══════════════════════════════════════════════════════════════════════════
    // Connection Verification Events
    // ═══════════════════════════════════════════════════════════════════════════
    /// Provider verification started
    ProviderVerifying { provider: String, model: String },
    /// Provider verification succeeded
    ProviderVerified {
        provider: String,
        model: String,
        latency_ms: u64,
    },
    /// Provider verification failed
    ProviderVerifyFailed { provider: String, error: String },
    /// Provider not configured (no API key set)
    ProviderNotConfigured { provider: String },
    /// MCP server ping started
    McpPinging { server: String },
    /// MCP server ping succeeded
    McpPinged {
        server: String,
        latency_ms: u64,
        tool_count: usize,
    },
    /// All provider verifications timed out (no providers available)
    ProviderVerificationTimeout,
    // ═══════════════════════════════════════════════════════════════════════════
    // Native Model Events
    // ═══════════════════════════════════════════════════════════════════════════
    /// Native model pull started
    NativeModelPullStarted { model: String },
    /// Native model pull progress update
    NativeModelPullProgress {
        model: String,
        status: String,
        completed: u64,
        total: u64,
    },
    /// Native model pull completed successfully
    NativeModelPulled {
        model: String,
        path: String,
        size: u64,
    },
    /// Native model pull failed
    NativeModelPullFailed { model: String, error: String },
    /// Native model deleted
    NativeModelDeleted { model: String },
    /// Native model delete failed
    NativeModelDeleteFailed { model: String, error: String },
    /// Native models list refreshed
    NativeModelsRefreshed { count: usize },
}

// =============================================================================
// StreamResult - Complete streaming response with token usage
// =============================================================================

/// Complete streaming response with text and token usage metrics
#[derive(Debug, Clone, Default)]
pub struct StreamResult {
    /// The complete response text
    pub text: String,
    /// Number of input tokens used
    pub input_tokens: u64,
    /// Number of output tokens generated
    pub output_tokens: u64,
    /// Total tokens (input + output)
    pub total_tokens: u64,
    /// Cached input tokens (from prompt caching)
    pub cached_input_tokens: u64,
    /// Time to first token in milliseconds (None if not captured)
    pub ttft_ms: Option<u64>,
    /// API request ID from response headers (None if not available)
    pub request_id: Option<String>,
    /// Why the model stopped generating (extracted from provider response)
    pub finish_reason: Option<nika_event::FinishReason>,
}

impl StreamResult {
    /// Create a new StreamResult with just text (zero tokens)
    pub fn from_text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Default::default()
        }
    }
}

/// Consume a rig-core streaming response, forwarding chunks to the channel.
///
/// Shared streaming loop for all rig-core providers. Handles:
/// - Per-chunk timeout via `STREAM_CHUNK_TIMEOUT`
/// - Token text forwarding via `StreamChunk::Token`
/// - Optional thinking/reasoning capture (Claude only)
/// - Token usage extraction from `Final` response
///
/// Returns early on timeout or stream error.
pub(crate) async fn consume_rig_stream<R>(
    stream: &mut rig::streaming::StreamingCompletionResponse<R>,
    tx: &mpsc::Sender<StreamChunk>,
    response_buf: &mut String,
    result: &mut StreamResult,
    capture_thinking: bool,
    stream_start: Instant,
) -> Result<(), RigInferError>
where
    R: Clone + Unpin + GetTokenUsage + serde::Serialize + serde::de::DeserializeOwned,
{
    loop {
        let chunk_result = match timeout(STREAM_CHUNK_TIMEOUT, stream.next()).await {
            Ok(Some(result)) => result,
            Ok(None) => break,
            Err(_elapsed) => {
                let _ = tx.try_send(StreamChunk::Error(format!(
                    "Stream timeout: no chunk received for {}s",
                    STREAM_CHUNK_TIMEOUT.as_secs()
                )));
                return Err(RigInferError::Timeout {
                    duration_ms: STREAM_CHUNK_TIMEOUT.as_millis() as u64,
                });
            }
        };

        match chunk_result {
            Ok(content) => match content {
                StreamedAssistantContent::Text(text) => {
                    // Capture time-to-first-token on first text chunk
                    if result.ttft_ms.is_none() {
                        result.ttft_ms = Some(stream_start.elapsed().as_millis() as u64);
                    }
                    response_buf.push_str(&text.text);
                    let _ = tx.try_send(StreamChunk::Token(text.text));
                }
                StreamedAssistantContent::ReasoningDelta { reasoning, .. } if capture_thinking => {
                    let _ = tx.try_send(StreamChunk::Thinking(reasoning));
                }
                StreamedAssistantContent::Final(response) => {
                    if let Some(usage) = response.token_usage() {
                        result.input_tokens = usage.input_tokens;
                        result.output_tokens = usage.output_tokens;
                        result.total_tokens = usage.total_tokens;
                        result.cached_input_tokens = usage.cached_input_tokens;
                    }
                    // Extract finish_reason from the serialized provider response.
                    // Anthropic uses "stop_reason", OpenAI uses "finish_reason".
                    if let Ok(json) = serde_json::to_value(&response) {
                        let reason = json
                            .get("stop_reason")
                            .or_else(|| {
                                json.get("choices")
                                    .and_then(|c| c.get(0))
                                    .and_then(|c| c.get("finish_reason"))
                            })
                            .and_then(|v| v.as_str());
                        if let Some(r) = reason {
                            result.finish_reason = Some(nika_event::FinishReason::from(r));
                        }
                    }
                }
                _ => {}
            },
            Err(e) => {
                let _ = tx.try_send(StreamChunk::Error(e.to_string()));
                return Err(RigInferError::PromptError(e.to_string()));
            }
        }
    }
    Ok(())
}
