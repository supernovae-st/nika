// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The OpenAI-compatible chat-completions wire contract.
//!
//! This is the surface the sidecar SERVES and the engine's existing
//! `OpenAiCompat` client (in `nika-providers`) already speaks — so wiring the
//! sidecar in needs zero new dispatch code. We model the subset a sovereign
//! local chat sidecar needs; unknown request fields are ignored (forward-
//! compat), unset optional fields are omitted on the wire (`skip_serializing_if`)
//! so our output matches what the client parser expects.
//!
//! The contract is validated end-to-end by a self-consistency test: a
//! [`ChatResponse`] we build, serialized, must parse back through the same
//! client that talks to ollama/openai today (added when the crate is wired
//! into the workspace alongside `nika-providers`).

use serde::{Deserialize, Serialize};

/// A chat message role. Closed set per the `OpenAI` chat schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum Role {
    /// System / developer instruction.
    System,
    /// End-user turn.
    User,
    /// Model turn.
    Assistant,
    /// Tool result turn.
    Tool,
}

/// One chat message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    /// Who authored the message.
    pub role: Role,
    /// The text content. (Multimodal parts are a later extension.)
    pub content: String,
}

impl Message {
    /// Construct a message.
    pub fn new(role: Role, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
        }
    }
}

/// A chat-completions request (the subset we serve).
///
/// Sampling knobs are `Option` so an unset field is omitted on the wire and
/// the backend applies its canonical default. `seed` is first-class because
/// determinism is required for golden tests (a SOTA-checklist invariant).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct ChatRequest {
    /// `provider/name` is resolved upstream; here it is the bare local model id.
    pub model: String,
    /// The conversation. Must be non-empty (validated by the backend).
    pub messages: Vec<Message>,
    /// Sampling temperature (0.0–2.0). `None` → backend default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Nucleus sampling. `None` → backend default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Hard cap on generated tokens. `None` → backend default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Stop sequences — generation halts when any is produced.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub stop: Vec<String>,
    /// RNG seed — set for reproducible (testable) output.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    /// Stream tokens as SSE deltas when true.
    pub stream: bool,
}

impl ChatRequest {
    /// Construct a request with the required fields; sampling knobs start
    /// unset (backend defaults) and are plain `pub` — set them directly.
    /// (INV-019: every `#[non_exhaustive]` struct ships a constructor —
    /// without one, external consumers cannot build a request at all.)
    #[must_use]
    pub fn new(model: impl Into<String>, messages: Vec<Message>) -> Self {
        Self {
            model: model.into(),
            messages,
            temperature: None,
            top_p: None,
            max_tokens: None,
            stop: Vec::new(),
            seed: None,
            stream: false,
        }
    }
}

/// Why a generation stopped. Maps to the `OpenAI` `finish_reason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum FinishReason {
    /// Hit the natural end-of-sequence token.
    Stop,
    /// Hit `max_tokens`.
    Length,
}

/// Token accounting, as `OpenAI` reports it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Usage {
    /// Tokens in the (templated) prompt.
    pub prompt_tokens: u32,
    /// Tokens generated.
    pub completion_tokens: u32,
    /// Sum of the two.
    pub total_tokens: u32,
}

impl Usage {
    /// Construct from the two counted parts; the total is derived — it can
    /// never disagree with the sum (INV-019 constructor). Saturating, so a
    /// degenerate token count can never overflow-panic — the bound is local,
    /// not merely a consequence of the request-body size cap upstream.
    #[must_use]
    pub fn new(prompt_tokens: u32, completion_tokens: u32) -> Self {
        Self {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens.saturating_add(completion_tokens),
        }
    }
}

/// One completion choice.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Choice {
    /// Position in the `choices` array (always 0 for single-completion).
    pub index: u32,
    /// The produced message.
    pub message: Message,
    /// Why it stopped.
    pub finish_reason: FinishReason,
}

/// A non-streamed chat-completions response (`OpenAI` shape).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ChatResponse {
    /// Opaque completion id.
    pub id: String,
    /// Always `"chat.completion"`.
    pub object: String,
    /// The model that produced this.
    pub model: String,
    /// The completions (we emit exactly one).
    pub choices: Vec<Choice>,
    /// Token accounting.
    pub usage: Usage,
}

impl ChatResponse {
    /// Build a single-choice response — the shape the engine's `OpenAiCompat`
    /// client parses. `id`/`object` carry the canonical constants.
    pub fn single(
        id: impl Into<String>,
        model: impl Into<String>,
        content: impl Into<String>,
        finish_reason: FinishReason,
        usage: Usage,
    ) -> Self {
        Self {
            id: id.into(),
            object: "chat.completion".to_owned(),
            model: model.into(),
            choices: vec![Choice {
                index: 0,
                message: Message::new(Role::Assistant, content),
                finish_reason,
            }],
            usage,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_omits_unset_optionals_on_the_wire() {
        // A minimal request must serialize WITHOUT temperature/top_p/etc. —
        // the client parser treats absence as "use the default", so emitting
        // explicit nulls would diverge from the OpenAI shape.
        let req = ChatRequest {
            model: "qwen3".to_owned(),
            messages: vec![Message::new(Role::User, "hi")],
            ..Default::default()
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(
            !json.contains("temperature"),
            "unset temperature must be omitted"
        );
        assert!(!json.contains("top_p"), "unset top_p must be omitted");
        assert!(!json.contains("seed"), "unset seed must be omitted");
        assert!(
            json.contains("\"stream\":false"),
            "stream is always present"
        );
    }

    #[test]
    fn request_roundtrips_with_all_knobs() {
        let req = ChatRequest {
            model: "qwen3".to_owned(),
            messages: vec![
                Message::new(Role::System, "be terse"),
                Message::new(Role::User, "2+2?"),
            ],
            temperature: Some(0.2),
            top_p: Some(0.9),
            max_tokens: Some(64),
            stop: vec!["\n\n".to_owned()],
            seed: Some(42),
            stream: true,
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: ChatRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.model, "qwen3");
        assert_eq!(back.messages.len(), 2);
        assert_eq!(back.seed, Some(42));
        assert_eq!(back.stop, vec!["\n\n".to_owned()]);
        assert!(back.stream);
    }

    #[test]
    fn request_tolerates_unknown_fields() {
        // Forward-compat: a client sending a field we don't model (e.g.
        // `presence_penalty`) must not break deserialization.
        let json =
            r#"{"model":"m","messages":[{"role":"user","content":"x"}],"presence_penalty":0.5}"#;
        let req: ChatRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.model, "m");
        assert_eq!(req.messages[0].role, Role::User);
    }

    #[test]
    fn response_has_canonical_object_and_one_choice() {
        let resp = ChatResponse::single(
            "cmpl-1",
            "qwen3",
            "4",
            FinishReason::Stop,
            Usage {
                prompt_tokens: 5,
                completion_tokens: 1,
                total_tokens: 6,
            },
        );
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"object\":\"chat.completion\""));
        let back: ChatResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.choices.len(), 1);
        assert_eq!(back.choices[0].message.role, Role::Assistant);
        assert_eq!(back.choices[0].finish_reason, FinishReason::Stop);
        assert_eq!(back.usage.total_tokens, 6);
    }
}
