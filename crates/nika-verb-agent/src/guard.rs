// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Stall detection — windowed cycle scan over TURN SIGNATURES (ADR-096).
//!
//! A turn's signature hashes its tool calls **and their observations**
//! (key-order-canonical, so a model re-emitting `{a,b}` as `{b,a}` still
//! matches). The consequence is the false-positive killer: legitimate
//! polling — the same call with a CHANGING result — produces distinct
//! signatures and never trips the detector; only byte-identical
//! action+outcome cycles do. Repetitive-action loops are the dominant
//! agentic failure class in trace taxonomies (TRAIL · Deshpande et al.
//! 2025, arxiv.org/abs/2505.08638); the windowed period scan is
//! elementary on purpose — over the default window of 25 signatures it is
//! at most W²/4 ≈ 156 u64 compares per turn, deterministic and zero-LLM.
//!
//! Escalation is a two-step ladder per [`crate::config::GuardConfig`]:
//! `nudge_after` occurrences → ONE bounded verbal reflection (Reflexion ·
//! Shinn et al. 2023, arxiv.org/abs/2303.11366: verbal feedback in the
//! transcript is the cheapest effective corrective) · `stall_after`
//! occurrences → stop the run as NIKA-467 (spending more turns on a
//! proven no-progress cycle is pure budget burn).

use std::collections::VecDeque;
use std::hash::{DefaultHasher, Hash, Hasher};

use crate::config::GuardConfig;
use crate::observe::NudgeReason;

/// The longest byte-identical action cycle the stall guard is GUARANTEED to
/// stop. A period-`p` cycle reaches `stall_after` repeats only once the
/// window holds `p · stall_after` signatures, so [`Guard::new`] floors the
/// window at `stall_after · MAX_TRACKED_PERIOD` (REACH INVARIANT). Cycles
/// longer than this fall through to `max_turns`; a byte-identical ≥9-step
/// ritual is rare (models drift within a longer loop) and the per-turn scan
/// is `window²/4`, so the bound trades a vanishing tail for a smaller scan.
///
/// Raised from an implicit 5 to 8 (2026-07 review): at ×5 a period-6 cycle
/// nudged once then ran to `max_turns` — the exact class the period-4 fix
/// targeted, one period further out.
pub(crate) const MAX_TRACKED_PERIOD: usize = 8;

/// What the guard tells the loop after observing one turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GuardVerdict {
    /// No pattern — keep going.
    Proceed,
    /// Inject ONE corrective reflection message (reason + cycle period;
    /// period is 0 for an error streak).
    Nudge(NudgeReason, u32),
    /// Stop the run: cycle `period` repeated `repeats` times.
    Stall {
        /// Cycle length in turns.
        period: u32,
        /// Full occurrences observed.
        repeats: u32,
    },
}

/// The per-run stall guard. Owns the signature window + the reflection
/// budget + the error streak counter.
#[derive(Debug)]
pub(crate) struct Guard {
    cfg: GuardConfig,
    window: VecDeque<u64>,
    reflections_used: u32,
    error_streak: u32,
}

impl Guard {
    pub(crate) fn new(mut cfg: GuardConfig) -> Self {
        // REACH INVARIANT clamp (config.rs · GuardConfig): a window smaller
        // than the periods it must hold silently disarms detection. Floor
        // the window at `stall_after · MAX_TRACKED_PERIOD` so every cycle up
        // to that period can actually reach the stop — a too-small configured
        // window can never make the guard a no-op.
        cfg.window = cfg
            .window
            .max(cfg.stall_after as usize * MAX_TRACKED_PERIOD)
            .max(2);
        let capacity = cfg.window;
        Self {
            cfg,
            window: VecDeque::with_capacity(capacity),
            reflections_used: 0,
            error_streak: 0,
        }
    }

    /// Observe one completed tool turn (its signature + whether EVERY
    /// dispatched call errored) and decide.
    ///
    /// Detection order: stall (the hard stop is never masked by a nudge
    /// opportunity) → cycle nudge → error-streak nudge. Nudges consume
    /// the shared reflection budget; once spent, only the stall threshold
    /// remains armed.
    pub(crate) fn observe_turn(&mut self, signature: u64, all_errors: bool) -> GuardVerdict {
        // `cfg.window` is already clamped ≥ 2 in `new` (REACH INVARIANT).
        if self.window.len() == self.cfg.window {
            self.window.pop_front();
        }
        self.window.push_back(signature);

        self.error_streak = if all_errors {
            self.error_streak.saturating_add(1)
        } else {
            0
        };

        // Clamp at use site: stall strictly above nudge so the ladder
        // always has a rung between "warn" and "stop".
        let stall_after = self
            .cfg
            .stall_after
            .max(self.cfg.nudge_after.saturating_add(1));

        if let Some((period, repeats)) = self.dominant_cycle() {
            if repeats >= stall_after {
                return GuardVerdict::Stall { period, repeats };
            }
            if repeats >= self.cfg.nudge_after && self.take_reflection() {
                return GuardVerdict::Nudge(NudgeReason::RepeatedActions, period);
            }
        }

        if self.error_streak >= self.cfg.error_streak_nudge && self.take_reflection() {
            self.error_streak = 0;
            return GuardVerdict::Nudge(NudgeReason::ErrorStreak, 0);
        }

        GuardVerdict::Proceed
    }

    /// The dominant trailing cycle: the period whose contiguous trailing
    /// repetition count is HIGHEST (ties → smallest period), when any
    /// period repeats at least twice.
    ///
    /// Max-repeats selection matters: a `[x,y,x]` 3-cycle fed five times
    /// also exhibits an incidental period-1 double at its seam — picking
    /// the first period found would report (1, 2) and mask the true
    /// (3, 5). A uniform `[x,x,x]` run still resolves to period 1 (its
    /// repeat count dominates every longer alias of itself).
    fn dominant_cycle(&self) -> Option<(u32, u32)> {
        let sigs: Vec<u64> = self.window.iter().copied().collect();
        let n = sigs.len();
        let mut best: Option<(u32, u32)> = None;
        for period in 1..=n / 2 {
            let last = &sigs[n - period..];
            let mut repeats: u32 = 1;
            let mut offset = period;
            while offset + period <= n && sigs[n - offset - period..n - offset] == *last {
                repeats += 1;
                offset += period;
            }
            if repeats >= 2 && best.is_none_or(|(_, r)| repeats > r) {
                #[allow(clippy::cast_possible_truncation)] // period ≤ window/2 (16/2)
                let period = period as u32;
                best = Some((period, repeats));
            }
        }
        best
    }

    fn take_reflection(&mut self) -> bool {
        if self.reflections_used < self.cfg.max_reflections {
            self.reflections_used += 1;
            true
        } else {
            false
        }
    }
}

/// The corrective reflection injected on a guard nudge (Reflexion-style
/// verbal feedback · Shinn et al. 2023, arxiv.org/abs/2303.11366 —
/// deterministic text, bounded by `GuardConfig::max_reflections`).
pub(crate) fn nudge_text(reason: NudgeReason, period: u32) -> String {
    match reason {
        NudgeReason::RepeatedActions => format!(
            "[loop-guard] Your last actions repeated a {period}-turn cycle with \
             identical results. Repeating them again will not produce new \
             information. State what you learned from the results you already \
             have, then either take a DIFFERENT action or finish now with your \
             best answer."
        ),
        NudgeReason::ErrorStreak => "[loop-guard] Every tool call in your recent \
             turns failed. Re-read the error messages, state the likely root \
             cause, then either fix the arguments, choose a different tool, or \
             finish now with your best answer."
            .to_owned(),
    }
}

/// One turn's signature: every tool call (name + key-order-canonical
/// args) and every observation (result content + error flag), hashed in
/// dispatch order with type tags so adjacent fields can't collide by
/// concatenation.
pub(crate) fn turn_signature(
    calls: &[(String, serde_json::Value)],
    results: &[(String, bool)],
) -> u64 {
    let mut hasher = DefaultHasher::new();
    0xA6_u8.hash(&mut hasher); // domain tag
    calls.len().hash(&mut hasher);
    for (name, args) in calls {
        1_u8.hash(&mut hasher);
        name.hash(&mut hasher);
        hash_canonical(args, &mut hasher);
    }
    results.len().hash(&mut hasher);
    for (content, is_error) in results {
        2_u8.hash(&mut hasher);
        content.hash(&mut hasher);
        is_error.hash(&mut hasher);
    }
    hasher.finish()
}

/// Hash a JSON value with OBJECT KEYS SORTED at every depth — the model
/// re-ordering keys between turns must not defeat repeat detection.
fn hash_canonical(value: &serde_json::Value, hasher: &mut DefaultHasher) {
    match value {
        serde_json::Value::Null => 0_u8.hash(hasher),
        serde_json::Value::Bool(b) => {
            1_u8.hash(hasher);
            b.hash(hasher);
        }
        serde_json::Value::Number(n) => {
            2_u8.hash(hasher);
            n.to_string().hash(hasher);
        }
        serde_json::Value::String(s) => {
            3_u8.hash(hasher);
            s.hash(hasher);
        }
        serde_json::Value::Array(items) => {
            4_u8.hash(hasher);
            items.len().hash(hasher);
            for item in items {
                hash_canonical(item, hasher);
            }
        }
        serde_json::Value::Object(map) => {
            5_u8.hash(hasher);
            map.len().hash(hasher);
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_unstable();
            for key in keys {
                key.hash(hasher);
                if let Some(v) = map.get(key) {
                    hash_canonical(v, hasher);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn guard(nudge_after: u32, stall_after: u32, max_reflections: u32) -> Guard {
        let mut cfg = GuardConfig::new();
        cfg.nudge_after = nudge_after;
        cfg.stall_after = stall_after;
        cfg.max_reflections = max_reflections;
        Guard::new(cfg)
    }

    #[test]
    fn identical_turns_nudge_then_stall() {
        let mut g = guard(3, 5, 1);
        assert_eq!(g.observe_turn(7, false), GuardVerdict::Proceed);
        assert_eq!(g.observe_turn(7, false), GuardVerdict::Proceed);
        // 3rd identical → period-1 cycle × 3 → nudge.
        assert_eq!(
            g.observe_turn(7, false),
            GuardVerdict::Nudge(NudgeReason::RepeatedActions, 1)
        );
        // 4th → repeats 4, nudge budget spent, below stall → proceed.
        assert_eq!(g.observe_turn(7, false), GuardVerdict::Proceed);
        // 5th → stall.
        assert_eq!(
            g.observe_turn(7, false),
            GuardVerdict::Stall {
                period: 1,
                repeats: 5
            }
        );
    }

    #[test]
    fn alternating_cycle_detects_period_two() {
        let mut g = guard(3, 5, 1);
        for verdict in [
            g.observe_turn(1, false), // a
            g.observe_turn(2, false), // b
            g.observe_turn(1, false), // a  (ab ×2 → repeats 2)
            g.observe_turn(2, false), // b
            g.observe_turn(1, false), // a  (ab ×3? trailing [b,a]… see below)
        ] {
            assert_eq!(verdict, GuardVerdict::Proceed);
        }
        // 6th: window [1,2,1,2,1,2] → trailing block (1,2) ×3 → nudge p=2.
        assert_eq!(
            g.observe_turn(2, false),
            GuardVerdict::Nudge(NudgeReason::RepeatedActions, 2)
        );
    }

    #[test]
    fn a_period_four_cycle_reaches_the_stall_under_defaults() {
        // THE review-caught bug: at window 16 a 4-step byte-identical cycle
        // repeats at most floor(16/4) = 4 < stall_after 5 → never stops,
        // burning ~990 of 1000 turns. With the default window (stall·5 =
        // 25) it must stall. Drive the guard with production defaults.
        let mut g = Guard::new(GuardConfig::new());
        let cycle = [11_u64, 22, 33, 44];
        let mut stalled = None;
        'outer: for rep in 0..8 {
            for &s in &cycle {
                if let GuardVerdict::Stall { period, repeats } = g.observe_turn(s, false) {
                    stalled = Some((period, repeats));
                    let _ = rep;
                    break 'outer;
                }
            }
        }
        assert_eq!(
            stalled,
            Some((4, 5)),
            "a period-4 cycle MUST reach NIKA-467 under default config"
        );
    }

    #[test]
    fn a_period_eight_cycle_reaches_the_stall_under_defaults() {
        // The 2026-07 review-caught bug, one period further out than the
        // period-4 fix: at window 25 (stall·5) a period-6+ byte-identical
        // cycle repeats at most floor(25/6) = 4 < stall_after 5 → nudges once
        // then runs to max_turns. With MAX_TRACKED_PERIOD = 8 the default
        // window is 40 (stall·8), so a period-8 cycle MUST reach the stall.
        let mut g = Guard::new(GuardConfig::new());
        let cycle = [11_u64, 22, 33, 44, 55, 66, 77, 88];
        let mut stalled = None;
        'outer: for _ in 0..8 {
            for &s in &cycle {
                if let GuardVerdict::Stall { period, repeats } = g.observe_turn(s, false) {
                    stalled = Some((period, repeats));
                    break 'outer;
                }
            }
        }
        assert_eq!(
            stalled,
            Some((8, 5)),
            "a period-8 cycle MUST reach NIKA-467 under default config"
        );
    }

    #[test]
    fn a_tiny_configured_window_cannot_disarm_detection() {
        // Misconfig: window 3 with stall_after 5 — the clamp must lift the
        // window so period-1 still stops (the second prong of the bug).
        let mut cfg = GuardConfig::new();
        cfg.window = 3;
        cfg.max_reflections = 0;
        let mut g = Guard::new(cfg);
        let mut last = GuardVerdict::Proceed;
        for _ in 0..6 {
            last = g.observe_turn(7, false);
        }
        assert!(
            matches!(last, GuardVerdict::Stall { period: 1, .. }),
            "the clamp must keep detection armed: {last:?}"
        );
    }

    #[test]
    fn changing_observations_never_trip_the_detector() {
        // The polling pattern: same call, different result each turn.
        let mut g = guard(3, 5, 1);
        for i in 0..20_u64 {
            let sig = turn_signature(
                &[("nika:fetch".to_owned(), serde_json::json!({"url": "x"}))],
                &[(format!("status {i}"), false)],
            );
            assert_eq!(
                g.observe_turn(sig, false),
                GuardVerdict::Proceed,
                "turn {i}"
            );
        }
    }

    #[test]
    fn error_streak_nudges_once_then_only_stall_remains() {
        let mut g = guard(3, 5, 1);
        assert_eq!(g.observe_turn(1, true), GuardVerdict::Proceed);
        assert_eq!(
            g.observe_turn(2, true),
            GuardVerdict::Nudge(NudgeReason::ErrorStreak, 0)
        );
        // Budget spent: a later streak cannot nudge again.
        assert_eq!(g.observe_turn(3, true), GuardVerdict::Proceed);
        assert_eq!(g.observe_turn(4, true), GuardVerdict::Proceed);
    }

    #[test]
    fn stall_is_never_masked_by_an_available_nudge() {
        // nudge_after == stall_after misconfig → clamp keeps stall above;
        // with nudge budget 0 the ladder goes straight to stall.
        let mut g = guard(3, 3, 0);
        assert_eq!(g.observe_turn(9, false), GuardVerdict::Proceed);
        assert_eq!(g.observe_turn(9, false), GuardVerdict::Proceed);
        assert_eq!(g.observe_turn(9, false), GuardVerdict::Proceed); // repeats 3 < clamped 4
        assert_eq!(
            g.observe_turn(9, false),
            GuardVerdict::Stall {
                period: 1,
                repeats: 4
            }
        );
    }

    #[test]
    fn equal_repeat_ties_report_the_smallest_period() {
        // Window a,b,b,a,b,b: the trailing run is BOTH a period-1 double
        // (b,b · repeats 2) and a period-3 double (a,b,b ×2 · repeats 2).
        // Equal repeats → the SMALLEST period is the honest evidence
        // (kills the `>` → `>=` tie-break mutant, under which the later
        // period overwrites). Asserted on the detector directly:
        // thresholds parked high so no verdict fires mid-stream.
        let mut cfg = GuardConfig::new();
        cfg.nudge_after = 90;
        cfg.stall_after = 99;
        cfg.max_reflections = 0;
        let mut g = Guard::new(cfg);
        for s in [10_u64, 20, 20, 10, 20, 20] {
            assert_eq!(g.observe_turn(s, false), GuardVerdict::Proceed);
        }
        assert_eq!(
            g.dominant_cycle(),
            Some((1, 2)),
            "ties between equal-repeat periods must report the smallest"
        );
    }

    #[test]
    fn signature_is_sensitive_to_args_values_alone() {
        // Two turns differing ONLY in an argument VALUE must not collide —
        // kills the replace-hash_canonical-with-() mutant (under which
        // every args payload hashes identically).
        let a = turn_signature(
            &[("t".to_owned(), serde_json::json!({"x": 1}))],
            &[("ok".to_owned(), false)],
        );
        let b = turn_signature(
            &[("t".to_owned(), serde_json::json!({"x": 2}))],
            &[("ok".to_owned(), false)],
        );
        assert_ne!(a, b, "argument values are part of the action signature");
    }

    #[test]
    fn signature_is_key_order_canonical_and_observation_sensitive() {
        let a = turn_signature(
            &[("t".to_owned(), serde_json::json!({"x": 1, "y": 2}))],
            &[("ok".to_owned(), false)],
        );
        let b = turn_signature(
            &[("t".to_owned(), serde_json::json!({"y": 2, "x": 1}))],
            &[("ok".to_owned(), false)],
        );
        assert_eq!(a, b, "key order must not matter");
        let c = turn_signature(
            &[("t".to_owned(), serde_json::json!({"x": 1, "y": 2}))],
            &[("different".to_owned(), false)],
        );
        assert_ne!(a, c, "observations are part of the signature");
        let d = turn_signature(
            &[("t".to_owned(), serde_json::json!({"x": 1, "y": 2}))],
            &[("ok".to_owned(), true)],
        );
        assert_ne!(a, d, "the error flag is part of the signature");
    }

    proptest! {
        /// Injected periodic suffixes are detected; random aperiodic
        /// prefixes never mask them. Cycle length 1..=8 — the post-fix reach
        /// (window 40 = stall·MAX_TRACKED_PERIOD holds 5 periods up to period
        /// 8), which the old window-25 default could not (period-6+ ran to
        /// max_turns).
        #[test]
        fn injected_cycles_are_detected(
            prefix in proptest::collection::vec(0u64..1000, 0..6),
            cycle in proptest::collection::vec(0u64..8, 1..9),
            extra in 0u32..3,
        ) {
            // Default config: window is clamped to stall_after·5 = 25, big
            // enough for 5 periods of a period-≤5 cycle.
            let mut cfg = GuardConfig::new();
            cfg.max_reflections = 0; // no nudge budget → pure detection via stall
            let mut g = Guard::new(cfg);
            let mut last = GuardVerdict::Proceed;
            for &s in &prefix {
                last = g.observe_turn(s.wrapping_add(10_000), false);
            }
            // Feed 5 + extra full cycles — MUST stall by the end (5
            // occurrences ≥ stall_after, and the window now holds them).
            let mut stalled = false;
            for _ in 0..(5 + extra) {
                for &s in &cycle {
                    last = g.observe_turn(s, false);
                    if matches!(last, GuardVerdict::Stall { .. }) {
                        stalled = true;
                    }
                }
                if stalled { break; }
            }
            prop_assert!(stalled, "a {}-cycle fed 5+ times must stall (last: {:?})", cycle.len(), last);
        }

        /// Distinct-signature streams (all unique) never fire anything.
        #[test]
        fn unique_streams_never_fire(n in 2usize..40) {
            let mut g = guard(3, 5, 1);
            for i in 0..n as u64 {
                prop_assert_eq!(g.observe_turn(i, false), GuardVerdict::Proceed);
            }
        }
    }
}
