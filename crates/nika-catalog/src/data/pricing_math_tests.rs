// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Cache-aware pricing math — the Cost Intelligence test surface
//! (`usd_for_split` · `estimate_cost_usage_for` · live-catalog properties),
//! split from `models.rs` per the 1500-LOC file cap.

use super::ALL_PRICING;
use super::models::{
    estimate_cost_for, estimate_cost_usage_for, find_pricing, find_pricing_for, usd_for_split,
};
use crate::types::model::ModelPricing;

#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod cache_aware {
    use super::*;

    // ── usd_for_split / estimate_cost_usage_for (cache-aware math) ──

    /// A synthetic row with disclosed cache rates (anthropic-shaped:
    /// read = 0.1× input · write = 1.25× input).
    fn cached_pricing() -> ModelPricing {
        ModelPricing::new(
            "demo",
            "demo-cached",
            3.0,
            15.0,
            Some(3.75),
            Some(0.30),
            None,
            None,
            // The 4 upstream model facts and the 3 research facts play
            // no part in cost math.
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
    }

    #[test]
    fn split_prices_cache_subsets_at_their_rates() {
        // 1000 input = 600 uncached + 300 read + 100 write · 200 output.
        let p = cached_pricing();
        let usd = usd_for_split(&p, 1000, 200, 300, 100);
        let expected = (600.0 * 3.0 + 300.0 * 0.30 + 100.0 * 3.75 + 200.0 * 15.0) / 1e6;
        assert!((usd - expected).abs() < 1e-12, "got {usd}, want {expected}");
    }

    #[test]
    fn split_cache_reads_are_cheaper_writes_dearer() {
        // The two directions of the pre-split lie: reads billed at the
        // full input rate OVERCOUNTED · writes not billed UNDERCOUNTED.
        let p = cached_pricing();
        let blind = usd_for_split(&p, 1000, 0, 0, 0);
        assert!(usd_for_split(&p, 1000, 0, 1000, 0) < blind, "read discount");
        assert!(usd_for_split(&p, 1000, 0, 0, 1000) > blind, "write premium");
    }

    #[test]
    fn split_without_disclosed_rates_falls_back_to_input_rate() {
        // No cache rates in the row → the subsets bill at the input rate:
        // exactly the cache-blind figure, never a silent different number.
        let p = ModelPricing::new(
            "demo",
            "demo-plain",
            3.0,
            15.0,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        let blind = usd_for_split(&p, 1000, 50, 0, 0);
        let split = usd_for_split(&p, 1000, 50, 700, 300);
        assert!((split - blind).abs() < 1e-12);
    }

    #[test]
    fn split_clamps_oversized_subsets() {
        // A wire bug reporting subsets larger than the whole input must
        // clamp the full-rate portion at zero — never negative math.
        let p = cached_pricing();
        let usd = usd_for_split(&p, 100, 0, 200, 0);
        let expected = (200.0 * 0.30) / 1e6;
        assert!((usd - expected).abs() < 1e-12, "got {usd}, want {expected}");
    }

    #[test]
    fn estimate_cost_usage_for_zero_subsets_matches_estimate_cost_for() {
        let a = estimate_cost_usage_for("openai/gpt-4o-mini", 1000, 500, 0, 0).expect("priced");
        let b = estimate_cost_for("openai/gpt-4o-mini", 1000, 500).expect("priced");
        assert!((a.usd - b.usd).abs() < 1e-15, "zero split = the same math");
        assert_eq!(a.model, b.model);
        assert_eq!(a.provider, b.provider);
    }

    #[test]
    fn estimate_cost_usage_for_unpriced_is_none() {
        assert!(estimate_cost_usage_for("mock/mock-echo", 100, 50, 10, 0).is_none());
        assert!(estimate_cost_usage_for("ollama/qwen3.5:4b", 100, 50, 10, 0).is_none());
    }

    /// B20 / issue 1297: the W0-D canary is a Gemini cloud id the
    /// snapshot does not price. A row appearing here would make the
    /// resolved-seat admission walk a no-op.
    #[test]
    fn gemini_b20_unpriced_canary_has_no_snapshot_row() {
        assert!(
            find_pricing_for("gemini/nika-b20-unpriced-canary").is_none(),
            "the canary must stay unpriced so --max-cost-usd can refuse it"
        );
        assert!(
            find_pricing_for("gemini/gemini-2.5-flash").is_some(),
            "flash stays the priced control"
        );
    }

    #[test]
    fn live_catalog_cache_read_discount_holds() {
        // Integration over the REAL snapshot: anthropic discloses cache
        // rates upstream — a fully-cached-read prompt must cost LESS than
        // the same prompt uncached (rate ordering, not pinned dollars —
        // the born-stale law).
        let p = find_pricing_for("anthropic/claude-sonnet-4-5").expect("priced");
        let read_rate = p
            .cache_read_per_million
            .expect("anthropic discloses cache_read");
        assert!(read_rate < p.input_per_million, "read is a discount");
    }

    #[test]
    fn gpt4p1_variants_differentiated() {
        let nano = find_pricing("gpt-4.1-nano").unwrap();
        let mini = find_pricing("gpt-4.1-mini").unwrap();
        let base = find_pricing("gpt-4.1").unwrap();
        assert!(nano.input_per_million < mini.input_per_million);
        assert!(mini.input_per_million < base.input_per_million);
    }

    #[test]
    fn family_variants_price_differently() {
        // The lookup distinguishes same-family variants — the SUBJECT is
        // no-cross-contamination, not any specific price (derived law).
        let versatile = find_pricing("llama-3.3-70b-versatile").unwrap();
        let instant = find_pricing("llama-3.1-8b-instant").unwrap();
        assert!(
            (versatile.output_per_million - instant.output_per_million).abs() > f64::EPSILON,
            "distinct variants must not collapse onto one rule"
        );
    }

    #[test]
    fn all_pricing_entries_have_non_empty_fields() {
        for p in ALL_PRICING {
            assert!(!p.provider.is_empty());
            assert!(!p.model_pattern.is_empty());
        }
    }
}

/// The reasoning-rate ratchet (Q01 · lot 1). `reasoning_tokens_per_million`
/// is DECLARED on `ModelPricing` and read by nothing: today reasoning
/// prices at the output rate, which is exact as long as no row discloses a
/// different one. When one does, this test fires — and `usd_for_split`
/// grows the arm in the same commit, so the number and the doc never drift
/// apart. It must never be "fixed" by deleting the assertion.
#[test]
#[cfg(feature = "pricing")]
fn no_row_declares_a_reasoning_rate_yet() {
    let declared: Vec<&str> = ALL_PRICING
        .iter()
        .filter(|p| p.reasoning_tokens_per_million.is_some())
        .map(|p| p.model_pattern)
        .collect();
    assert!(
        declared.is_empty(),
        "these rows disclose a reasoning rate usd_for_split does not price yet: {declared:?}"
    );
}
