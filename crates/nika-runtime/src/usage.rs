// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The metered call's usage SPLIT — the receipt the frame owes its
//! reader (the usage-split lot).
//!
//! The catalog prices a call from four numbers (input · cached input ·
//! cache writes · output) and the frame kept ONE (`tokens` = the
//! completion count). A warm-cache `OpenAI` frame at `$0.000378` and a
//! cold one at `$0.00075285` were then INDISTINGUISHABLE from a price
//! change: no reader could recompute the number from what the journal
//! carried. This carrier rides from the provider result through
//! `DispatchOk`/`FailedDispatch` to the terminal frame so a reader can.
//!
//! Additive by construction: `tokens` keeps its historical meaning (the
//! completion count) — a consumer reading it today reads the same number
//! tomorrow. Absent meters stay ABSENT: `None` is "not reported", never
//! a fabricated zero (the ledger's fake-zero law).

use crate::{FieldValue, i, s};

/// The provider-reported split of ONE metered task (summed across the
/// task's round-trips, exactly like the `tokens` it rides beside) plus
/// the responder's own identity when the wire returned it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct UsageSplit {
    /// Prompt tokens — INCLUDES the cache subsets (`OTel` `gen_ai`
    /// semantics; the wires normalize to it and the cost math subtracts
    /// the subsets to price each portion at its own rate).
    pub input: u64,
    /// Completion tokens — includes the reasoning subset.
    pub output: u64,
    /// Prompt tokens served from the provider's cache (subset of `input`).
    pub cache_read: Option<u64>,
    /// Prompt tokens written to the provider's cache (subset of `input`).
    pub cache_write: Option<u64>,
    /// Reasoning/thinking tokens (subset of `output`).
    pub reasoning: Option<u64>,
    /// `gen_ai.response.model` — the model that ANSWERED, when the wire
    /// says so. Never the requested name asserted as served (ADR-112).
    pub model_served: Option<String>,
    /// `gen_ai.response.id` — the provider's own id for the response.
    pub response_id: Option<String>,
}

impl UsageSplit {
    /// Fold a kernel `TokenUsage` into the frame's split. `cache_write`
    /// sums the two names the wires use (`cache_write_tokens` ·
    /// Anthropic's `cache_creation_tokens`) exactly as the cost math
    /// does, so the recompute line and the bill read the same number.
    pub(crate) fn of(usage: &nika_kernel::provider::TokenUsage) -> Self {
        let cache_write = match (usage.cache_write_tokens, usage.cache_creation_tokens) {
            (None, None) => None,
            (a, b) => Some(a.unwrap_or(0).saturating_add(b.unwrap_or(0))),
        };
        // Reasoning and thinking are the same output subset under two
        // provider names — the reporting side carries.
        let reasoning = match (usage.reasoning_tokens, usage.thinking_tokens) {
            (None, None) => None,
            (a, b) => Some(a.unwrap_or(0).saturating_add(b.unwrap_or(0))),
        };
        Self {
            input: usage.input_tokens,
            output: usage.output_tokens,
            cache_read: usage.cache_read_tokens,
            cache_write,
            reasoning,
            model_served: None,
            response_id: None,
        }
    }

    /// Stamp the responder's identity (the wire's `gen_ai` attrs).
    pub(crate) fn served_by(
        mut self,
        model_served: Option<String>,
        response_id: Option<String>,
    ) -> Self {
        self.model_served = model_served;
        self.response_id = response_id;
        self
    }

    /// Whether the provider reported ANY meter — an all-zero split is
    /// "did not report" and must not ride as four honest zeroes.
    pub(crate) fn has_signal(&self) -> bool {
        self.input > 0
            || self.output > 0
            || self.cache_read.is_some_and(|n| n > 0)
            || self.cache_write.is_some_and(|n| n > 0)
            || self.reasoning.is_some_and(|n| n > 0)
    }

    /// A split worth carrying — a metered call with signal, or a wire
    /// that named the responder.
    pub(crate) fn carried(self) -> Option<Box<Self>> {
        let named = self.model_served.is_some() || self.response_id.is_some();
        (self.has_signal() || named).then(|| Box::new(self))
    }
}

/// Push the additive split onto a terminal frame's fields — `tokens_in`
/// · `tokens_out` always when a split rides, the subsets only when the
/// provider reported them, the responder's identity only when the wire
/// returned it.
pub(crate) fn push_usage_fields(
    fields: &mut Vec<(&'static str, FieldValue)>,
    split: Option<&UsageSplit>,
) {
    // `u64` meter → the frame's `i64` field, saturating (a corrupt
    // provider count must not wrap the receipt).
    fn n(v: u64) -> FieldValue {
        i(i64::try_from(v).unwrap_or(i64::MAX))
    }
    let Some(split) = split else { return };
    if split.has_signal() {
        fields.push(("tokens_in", n(split.input)));
        fields.push(("tokens_out", n(split.output)));
        if let Some(v) = split.cache_read {
            fields.push(("tokens_cache_read", n(v)));
        }
        if let Some(v) = split.cache_write {
            fields.push(("tokens_cache_write", n(v)));
        }
        if let Some(v) = split.reasoning {
            fields.push(("tokens_reasoning", n(v)));
        }
    }
    if let Some(m) = &split.model_served {
        fields.push(("model_served", s(m)));
    }
    if let Some(id) = &split.response_id {
        fields.push(("response_id", s(id)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_kernel::provider::TokenUsage;

    fn field<'f>(fields: &'f [(&'static str, FieldValue)], key: &str) -> Option<&'f FieldValue> {
        fields.iter().find(|(k, _)| *k == key).map(|(_, v)| v)
    }

    #[test]
    fn the_measured_openai_usage_rides_the_frame() {
        // The measured probe: prompt 5015 of which 4992 cached, one
        // completion token — the frame that could not tell a warm cache
        // from a price change.
        let mut usage = TokenUsage::new(5015, 1);
        usage.cache_read_tokens = Some(4992);
        let split = UsageSplit::of(&usage).served_by(
            Some("gpt-4o-mini-2024-07-18".to_owned()),
            Some("chatcmpl-x".to_owned()),
        );
        let mut fields = Vec::new();
        push_usage_fields(&mut fields, Some(&split));
        assert_eq!(field(&fields, "tokens_in"), Some(&i(5015)));
        assert_eq!(field(&fields, "tokens_out"), Some(&i(1)));
        assert_eq!(field(&fields, "tokens_cache_read"), Some(&i(4992)));
        assert_eq!(
            field(&fields, "tokens_cache_write"),
            None,
            "unreported stays absent"
        );
        assert_eq!(field(&fields, "tokens_reasoning"), None);
        assert_eq!(
            field(&fields, "model_served"),
            Some(&s("gpt-4o-mini-2024-07-18"))
        );
        assert_eq!(field(&fields, "response_id"), Some(&s("chatcmpl-x")));
    }

    #[test]
    fn the_measured_gemini_usage_folds_thoughts_as_reasoning() {
        // prompt 5009 · candidates 1 · thoughts 490 (the wire folds
        // thoughts into output; the meter names them too).
        let mut usage = TokenUsage::new(5009, 491);
        usage.thinking_tokens = Some(490);
        let split = UsageSplit::of(&usage);
        let mut fields = Vec::new();
        push_usage_fields(&mut fields, Some(&split));
        assert_eq!(field(&fields, "tokens_in"), Some(&i(5009)));
        assert_eq!(field(&fields, "tokens_out"), Some(&i(491)));
        assert_eq!(field(&fields, "tokens_reasoning"), Some(&i(490)));
    }

    #[test]
    fn an_all_zero_split_is_not_carried_as_four_honest_zeroes() {
        assert!(UsageSplit::of(&TokenUsage::new(0, 0)).carried().is_none());
        let mut fields = Vec::new();
        push_usage_fields(&mut fields, Some(&UsageSplit::default()));
        assert!(fields.is_empty(), "no signal, no meters");
    }

    #[test]
    fn cache_write_sums_the_two_provider_names_like_the_cost_math() {
        let mut usage = TokenUsage::new(100, 10);
        usage.cache_write_tokens = Some(7);
        usage.cache_creation_tokens = Some(3);
        assert_eq!(UsageSplit::of(&usage).cache_write, Some(10));
    }
}
