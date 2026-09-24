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
//!
//! The same carrier rides the TRANSPORT's account of the call (the
//! product-convergence war room · L4): a seat that answered only after
//! the provider layer's bounded backoff (429 · 503 · 529 · `Retry-After`)
//! used to be invisible in the sealed trace — the verb reported it, the
//! frame did not. `attempts` · `waited_ms` · `retried_on` now ride the
//! terminal beside the meters they explain (a 3-second task with one
//! completion token is a rate-limited seat, not a slow one).

use nika_providers::TransportReport;

use crate::{FieldValue, i, s};

/// The provider-reported split of ONE metered task (summed across the
/// task's round-trips, exactly like the `tokens` it rides beside) plus
/// the responder's own identity when the wire returned it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct UsageSplit {
    /// Individual invocation evidence; routes are never merged for pricing.
    pub inference_calls: Vec<nika_types::cost::InferenceCall>,
    /// Exact route and tariff observation for new receipts; historic frames
    /// retain their original pricing identity and are never repriced here.
    pub pricing: Option<String>,
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
    /// Round-trips the transport sent for this task (1 = the seat
    /// answered first time · summed across a `schema:` task's re-asks
    /// like the meters). `None` = the verb reported no transport (a
    /// harness seat · an agent loop · a builtin).
    pub attempts: Option<u32>,
    /// Backoff slept between round-trips, in milliseconds — the seat's
    /// `Retry-After` or the layer's 1 s · 2 s · 4 s schedule.
    pub waited_ms: Option<u64>,
    /// The HTTP status of every answer the transport waited on, in
    /// order (`[429, 429]` = two rate-limits before the answer).
    pub retried_on: Vec<u16>,
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
            inference_calls: Vec::new(),
            pricing: None,
            input: usage.input_tokens,
            output: usage.output_tokens,
            cache_read: usage.cache_read_tokens,
            cache_write,
            reasoning,
            model_served: None,
            response_id: None,
            attempts: None,
            waited_ms: None,
            retried_on: Vec::new(),
        }
    }

    /// Preserve observations across authored task retries. Debits occur before
    /// this presentation-only fold, so no invocation is charged twice.
    pub(crate) fn join_calls(
        target: &mut Option<Box<Self>>,
        prior: &[nika_types::cost::InferenceCall],
        append_current: bool,
    ) {
        if prior.is_empty() {
            return;
        }
        let split = target.get_or_insert_with(Box::default);
        let mut calls = prior.to_vec();
        if append_current {
            calls.extend_from_slice(&split.inference_calls);
        }
        split.inference_calls = calls;
        // A single-route summary cannot describe observations from several attempts.
        split.pricing = None;
    }

    pub(crate) fn with_calls(mut self, calls: &[nika_types::cost::InferenceCall]) -> Self {
        self.inference_calls = calls.to_vec();
        self
    }

    /// Invocation count excluded from the known subtotal; None for older producers.
    pub(crate) fn unknown_calls(&self) -> Option<u32> {
        (!self.inference_calls.is_empty()).then(|| {
            u32::try_from(
                self.inference_calls
                    .iter()
                    .filter(|c| c.known_estimate().is_none())
                    .count(),
            )
            .unwrap_or(u32::MAX)
        })
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

    /// Stamp the transport's account of the call (the provider layer's
    /// [`TransportReport`] · every round-trip summed). A report that
    /// sent nothing (`attempts == 0`) stamps nothing: the frame never
    /// claims a round-trip the wire did not make.
    pub(crate) fn transported(mut self, report: &TransportReport) -> Self {
        self.inference_calls.clone_from(&report.inference_calls);
        self.pricing = report.inference_calls.first().and_then(|first| {
            report
                .inference_calls
                .iter()
                .all(|c| c.pricing == first.pricing)
                .then(|| first.pricing.clone())
                .flatten()
        });
        if report.attempts == 0 {
            return self;
        }
        self.attempts = Some(report.attempts);
        // `u128` ms → the frame's `u64`, saturating (a corrupt clock must
        // not wrap the receipt).
        self.waited_ms = Some(u64::try_from(report.waited.as_millis()).unwrap_or(u64::MAX));
        self.retried_on.clone_from(&report.statuses);
        self
    }

    /// Whether the transport re-sent at least once.
    pub(crate) fn retried(&self) -> bool {
        !self.retried_on.is_empty()
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

    /// A split worth carrying — a metered call with signal, a wire that
    /// named the responder, or a transport that sent a round-trip.
    pub(crate) fn carried(self) -> Option<Box<Self>> {
        let named = self.model_served.is_some() || self.response_id.is_some();
        (self.has_signal() || named || self.attempts.is_some() || !self.inference_calls.is_empty())
            .then(|| Box::new(self))
    }
}

/// Push the additive split onto a terminal frame's fields — `tokens_in`
/// · `tokens_out` always when a split rides, the subsets only when the
/// provider reported them, the responder's identity only when the wire
/// returned it, the transport's `attempts` whenever the verb reported
/// one and `waited_ms` · `retried_on` only when it re-sent (a first-time
/// answer reads `attempts: 1` and nothing else — zero waits are not a
/// fact worth a field).
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
    if let Some(count) = split.unknown_calls() {
        fields.push(("cost_unknown_calls", i(i64::from(count))));
        if let Ok(calls) = serde_json::to_string(&split.inference_calls) {
            fields.push(("inference_calls", s(&calls)));
        }
    }
    if let Some(pricing) = &split.pricing {
        fields.push(("pricing_route", s(pricing)));
    }
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
    if let Some(attempts) = split.attempts {
        fields.push(("attempts", i(i64::from(attempts))));
    }
    if split.retried() {
        if let Some(ms) = split.waited_ms {
            fields.push(("waited_ms", n(ms)));
        }
        let statuses = split
            .retried_on
            .iter()
            .map(u16::to_string)
            .collect::<Vec<_>>()
            .join(" · ");
        fields.push(("retried_on", s(&statuses)));
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

    /// A seat that answered after one 429 (the measured Gemini shape
    /// under the product matrix's parallel load): the frame says two
    /// round-trips, the second-long wait, and what was waited on.
    #[test]
    fn a_retried_call_stamps_attempts_wait_and_statuses() {
        let mut report = TransportReport::new();
        report.attempts = 2;
        report.waited = std::time::Duration::from_millis(2000);
        report.statuses = vec![429];
        let split = UsageSplit::of(&TokenUsage::new(7, 3)).transported(&report);
        let mut fields = Vec::new();
        push_usage_fields(&mut fields, Some(&split));
        assert_eq!(field(&fields, "attempts"), Some(&i(2)));
        assert_eq!(field(&fields, "waited_ms"), Some(&i(2000)));
        assert_eq!(field(&fields, "retried_on"), Some(&s("429")));
    }

    /// The first-time answer — the common case — reads `attempts: 1`
    /// and no wait fields: a zero wait is not a fact worth a field, and
    /// a reader greps `retried_on` for exactly the retried calls.
    #[test]
    fn a_first_time_answer_reads_one_attempt_and_no_wait() {
        let mut report = TransportReport::new();
        report.attempts = 1;
        let split = UsageSplit::of(&TokenUsage::new(7, 3)).transported(&report);
        let mut fields = Vec::new();
        push_usage_fields(&mut fields, Some(&split));
        assert_eq!(field(&fields, "attempts"), Some(&i(1)));
        assert_eq!(field(&fields, "waited_ms"), None);
        assert_eq!(field(&fields, "retried_on"), None);
    }

    /// A report that sent nothing stamps nothing — and a split whose
    /// ONLY fact is the transport is still carried (a seat that reported
    /// no meters but was retried is exactly the call a reader asks about).
    #[test]
    #[allow(clippy::expect_used)]
    fn an_unsent_report_stamps_nothing_and_a_transport_alone_is_carried() {
        let split = UsageSplit::of(&TokenUsage::new(7, 3)).transported(&TransportReport::new());
        assert_eq!(split.attempts, None);
        assert!(!split.retried());

        let mut report = TransportReport::new();
        report.attempts = 3;
        report.statuses = vec![503, 429];
        report.waited = std::time::Duration::from_secs(3);
        let carried = UsageSplit::of(&TokenUsage::new(0, 0))
            .transported(&report)
            .carried()
            .expect("the transport alone is worth carrying");
        let mut fields = Vec::new();
        push_usage_fields(&mut fields, Some(&carried));
        assert_eq!(field(&fields, "tokens_in"), None, "no signal, no meters");
        assert_eq!(field(&fields, "attempts"), Some(&i(3)));
        assert_eq!(field(&fields, "waited_ms"), Some(&i(3000)));
        assert_eq!(field(&fields, "retried_on"), Some(&s("503 · 429")));
    }
}
