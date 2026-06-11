// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The device-agnostic generation seam.
//!
//! [`Backend`] is the one trait the candle backend will implement (candle
//! abstracts the device — `Cpu | Metal | Cuda` — so the serving loop is
//! written once and CUDA is a compile feature, not a rewrite · ADR-091).
//! Keeping the seam here means the server/supervisor layers depend on the
//! trait, never on candle directly.
//!
//! [`MockBackend`] is a deterministic, dependency-free implementation: it
//! makes the crate compile + test before candle is wired, and it powers the
//! self-consistency test (our [`ChatResponse`] → the engine's `OpenAiCompat`
//! parser). Determinism mirrors the `mock` provider already in the catalog.

use crate::error::InferLocalError;
use crate::protocol::{ChatRequest, ChatResponse, FinishReason, Role, Usage};

/// One streamed token delta (the SSE unit the server layer emits).
///
/// The non-streaming [`Backend::generate`] is the core contract; streaming
/// wraps it token-by-token at the HTTP boundary (a later commit). The type
/// lives here so both layers share it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationChunk {
    /// The incremental text since the previous chunk.
    pub delta: String,
    /// `Some` on the final chunk, carrying why generation stopped.
    pub finish_reason: Option<FinishReason>,
}

/// A local inference backend: turn a chat request into a completion.
///
/// One method, device-agnostic. The real (candle) impl MUST honor the
/// SOTA invariants documented on the crate root (KV-cache, per-family chat
/// template, EOS + stop halt, seeded sampling, GGUF, context cap). Implementors
/// run inside the supervised child process, so a panic here is contained.
pub trait Backend {
    /// Run one chat completion to completion (non-streaming).
    ///
    /// # Errors
    ///
    /// [`InferLocalError`] for an empty conversation, an unknown model, a
    /// context overflow, OOM, a timeout, or any backend failure.
    fn generate(
        &self,
        request: &ChatRequest,
    ) -> impl std::future::Future<Output = Result<ChatResponse, InferLocalError>> + Send;
}

/// A deterministic backend with zero model dependencies.
///
/// It does not run a network or a model — it produces a fixed, reproducible
/// completion from the last user message, so tests assert exact output and
/// the wire contract is exercised end-to-end without candle.
#[derive(Debug, Clone, Default)]
pub struct MockBackend;

impl MockBackend {
    /// The fixed model id the mock reports.
    pub const MODEL: &str = "mock-local";

    /// Naive whitespace token count — deterministic, dependency-free. (The
    /// real backend uses the model's tokenizer; the mock only needs a stable
    /// number for the `usage` field.)
    fn count_tokens(text: &str) -> u32 {
        u32::try_from(text.split_whitespace().count()).unwrap_or(u32::MAX)
    }
}

impl Backend for MockBackend {
    async fn generate(&self, request: &ChatRequest) -> Result<ChatResponse, InferLocalError> {
        // Validate the request the way the real backend will, so the contract
        // (and its error paths) are exercised by the mock too.
        let last_user = request
            .messages
            .iter()
            .rev()
            .find(|m| m.role == Role::User)
            .ok_or_else(|| InferLocalError::InvalidRequest {
                reason: "no user message in the conversation".to_owned(),
            })?;

        // Deterministic "completion": a fixed prefix + an echo of the prompt.
        // Reproducible regardless of seed (the mock has no RNG) — exactly what
        // a golden test wants.
        let full = format!("mock reply: {}", last_user.content);

        // Honor max_tokens: truncate (by whitespace token) and report Length.
        let (content, finish_reason) = match request.max_tokens {
            Some(limit) if (limit as usize) < full.split_whitespace().count() => {
                let truncated: Vec<&str> = full.split_whitespace().take(limit as usize).collect();
                (truncated.join(" "), FinishReason::Length)
            }
            _ => (full, FinishReason::Stop),
        };

        let prompt_tokens: u32 = request
            .messages
            .iter()
            .map(|m| Self::count_tokens(&m.content))
            .sum();
        let completion_tokens = Self::count_tokens(&content);

        Ok(ChatResponse::single(
            "mock-cmpl-0",
            Self::MODEL,
            content,
            finish_reason,
            Usage {
                prompt_tokens,
                completion_tokens,
                total_tokens: prompt_tokens + completion_tokens,
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Message;

    fn req(messages: Vec<Message>, max_tokens: Option<u32>) -> ChatRequest {
        ChatRequest {
            model: "mock-local".to_owned(),
            messages,
            max_tokens,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn mock_is_deterministic_for_the_same_input() {
        let r = req(vec![Message::new(Role::User, "hello world")], None);
        let a = MockBackend.generate(&r).await.unwrap();
        let b = MockBackend.generate(&r).await.unwrap();
        assert_eq!(a.choices[0].message.content, b.choices[0].message.content);
        assert_eq!(a.choices[0].message.content, "mock reply: hello world");
        assert_eq!(a.choices[0].finish_reason, FinishReason::Stop);
    }

    #[tokio::test]
    async fn mock_rejects_a_conversation_with_no_user_turn() {
        let r = req(vec![Message::new(Role::System, "be terse")], None);
        let err = MockBackend.generate(&r).await.unwrap_err();
        assert!(matches!(err, InferLocalError::InvalidRequest { .. }));
    }

    #[tokio::test]
    async fn mock_honors_max_tokens_and_reports_length() {
        // "mock reply: a b c d e" = 7 whitespace tokens; cap at 3 → Length.
        let r = req(vec![Message::new(Role::User, "a b c d e")], Some(3));
        let resp = MockBackend.generate(&r).await.unwrap();
        assert_eq!(resp.choices[0].finish_reason, FinishReason::Length);
        assert_eq!(
            resp.choices[0].message.content.split_whitespace().count(),
            3
        );
    }

    #[tokio::test]
    async fn mock_counts_usage() {
        let r = req(vec![Message::new(Role::User, "two words")], None);
        let resp = MockBackend.generate(&r).await.unwrap();
        // prompt "two words" = 2 tokens; completion "mock reply: two words" = 4.
        assert_eq!(resp.usage.prompt_tokens, 2);
        assert_eq!(resp.usage.completion_tokens, 4);
        assert_eq!(resp.usage.total_tokens, 6);
    }
}
