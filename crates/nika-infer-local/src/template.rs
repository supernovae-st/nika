// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Per-family chat templating — messages → a single prompt string.
//!
//! Getting the chat template wrong produces *garbage*, not a crash (a SOTA
//! must-not-forget). candle's own examples hardcode the family template; we do
//! the same for the **supported** families (deterministic, no Jinja surprises,
//! golden-testable), which is the v1 posture per ADR-091 research. A v2
//! `minijinja` renderer reading `tokenizer_config.json`'s `chat_template` is
//! the escape hatch for arbitrary GGUFs (noted, not built — it needs custom
//! filter shims like `raise_exception`).
//!
//! This module is **candle-free** (pure string assembly) so it compiles + is
//! fully tested in the default build, independent of the `local-infer` feature.

use crate::protocol::{Message, Role};

/// A chat-template family. Closed for the models the sidecar ships v1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ChatFamily {
    /// Qwen2/Qwen3 `ChatML` — `<|im_start|>role\n…<|im_end|>\n`.
    Qwen,
    /// Llama-3 — `<|start_header_id|>role<|end_header_id|>\n\n…<|eot_id|>`.
    Llama3,
    /// Phi-3 — `<|role|>\n…<|end|>\n`.
    Phi3,
}

impl ChatFamily {
    /// Render a conversation into the family's prompt string, including the
    /// trailing assistant-generation prefix (`add_generation_prompt = true`).
    #[must_use]
    pub fn render(self, messages: &[Message]) -> String {
        match self {
            Self::Qwen => Self::render_qwen(messages),
            Self::Llama3 => Self::render_llama3(messages),
            Self::Phi3 => Self::render_phi3(messages),
        }
    }

    fn tag(role: Role) -> &'static str {
        match role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        }
    }

    fn render_qwen(messages: &[Message]) -> String {
        let mut out = String::new();
        for m in messages {
            out.push_str("<|im_start|>");
            out.push_str(Self::tag(m.role));
            out.push('\n');
            out.push_str(&m.content);
            out.push_str("<|im_end|>\n");
        }
        out.push_str("<|im_start|>assistant\n");
        out
    }

    fn render_llama3(messages: &[Message]) -> String {
        let mut out = String::from("<|begin_of_text|>");
        for m in messages {
            out.push_str("<|start_header_id|>");
            out.push_str(Self::tag(m.role));
            out.push_str("<|end_header_id|>\n\n");
            out.push_str(&m.content);
            out.push_str("<|eot_id|>");
        }
        out.push_str("<|start_header_id|>assistant<|end_header_id|>\n\n");
        out
    }

    fn render_phi3(messages: &[Message]) -> String {
        let mut out = String::new();
        for m in messages {
            out.push_str("<|");
            out.push_str(Self::tag(m.role));
            out.push_str("|>\n");
            out.push_str(&m.content);
            out.push_str("<|end|>\n");
        }
        out.push_str("<|assistant|>\n");
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn convo() -> Vec<Message> {
        vec![
            Message::new(Role::System, "be terse"),
            Message::new(Role::User, "hi"),
        ]
    }

    #[test]
    fn qwen_wraps_chatml_and_opens_assistant_turn() {
        let p = ChatFamily::Qwen.render(&convo());
        assert_eq!(
            p,
            "<|im_start|>system\nbe terse<|im_end|>\n\
             <|im_start|>user\nhi<|im_end|>\n\
             <|im_start|>assistant\n"
        );
    }

    #[test]
    fn llama3_uses_header_ids_and_eot() {
        let p = ChatFamily::Llama3.render(&convo());
        assert!(p.starts_with("<|begin_of_text|>"));
        assert!(p.contains("<|start_header_id|>user<|end_header_id|>\n\nhi<|eot_id|>"));
        assert!(p.ends_with("<|start_header_id|>assistant<|end_header_id|>\n\n"));
    }

    #[test]
    fn phi3_uses_role_tags_and_end() {
        let p = ChatFamily::Phi3.render(&convo());
        assert!(p.contains("<|user|>\nhi<|end|>\n"));
        assert!(p.ends_with("<|assistant|>\n"));
    }

    #[test]
    fn every_family_opens_an_assistant_turn() {
        // The generation prefix is what makes the model continue as assistant;
        // forgetting it is a classic garbage-output bug.
        for fam in [ChatFamily::Qwen, ChatFamily::Llama3, ChatFamily::Phi3] {
            let p = fam.render(&convo());
            assert!(
                p.contains("assistant"),
                "{fam:?} must open an assistant turn"
            );
        }
    }
}
