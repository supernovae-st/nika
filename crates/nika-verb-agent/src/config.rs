// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! [`AgentConfig`] — engine-internal tuning for the agent-loop
//! intelligence (ADR-096).
//!
//! Deliberately NOT a YAML surface: the spec's `agent:` field table is
//! the public contract and stays untouched (own-corpus law — no phantom
//! fields). The embedder (L3 runtime · tests) tunes the loop through
//! this struct; every default is chosen so an unconfigured loop behaves
//! sanely on real workloads.

/// Tuning for the whole intelligence layer. `Default` is the production
/// posture; construct + mutate fields to override (all fields public —
/// this is config, not API surface to defend).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct AgentConfig {
    /// Per-turn tool routing (the MCP-Zero-style active-discovery gate).
    pub router: RouterConfig,
    /// Stall detection + bounded reflection.
    pub guard: GuardConfig,
    /// Concurrency cap for one turn's tool batch (ADR-097 · the
    /// `LLMCompiler` direction, Kim et al. 2023, arxiv.org/abs/2312.04511:
    /// independent calls the model batched into ONE turn run
    /// concurrently). `1` restores strictly sequential dispatch; results
    /// are ALWAYS fed back in request order regardless of this cap.
    pub max_parallel_tools: usize,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            router: RouterConfig::default(),
            guard: GuardConfig::default(),
            // A turn's batch is model-sized (a handful) — 8 covers it
            // without letting a pathological 100-call turn stampede the
            // executor seam.
            max_parallel_tools: 8,
        }
    }
}

impl AgentConfig {
    /// The production defaults (INV-019).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

/// Per-turn tool-definition routing.
///
/// Below [`Self::min_universe`] definitions, routing is a no-op: the
/// model sees the full whitelisted set every turn (stable prompts are
/// provider-cache-friendly; selection only pays for itself once the
/// universe is large enough to crowd the context — per Fei · Zheng ·
/// Feng 2025, arxiv.org/abs/2506.01056, injecting full schemas for
/// large tool sets degrades both context budget and selection quality).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RouterConfig {
    /// Route only when the whitelisted universe is at least this large;
    /// smaller universes ship whole (prompt-cache stability wins).
    ///
    /// Kept strictly ABOVE the full `nika:*` builtin catalog (24 as of
    /// stdlib §Media · ADR-105) so a plain-builtin whitelist always ships
    /// whole — routing pays for itself on genuinely large (MCP-heavy)
    /// universes, never on the stdlib alone. A composition-root test in
    /// `nika-cli` pins `tool_defs().len() < min_universe` so a future
    /// builtin addition cannot silently flip full-catalog agents from
    /// deterministic passthrough to BM25 narrowing (the 23→24 near-miss
    /// of 2026-07-05).
    pub min_universe: usize,
    /// How many ranked definitions to offer per turn (pinned tools ride
    /// on top of this budget, they never crowd it).
    pub top_k: usize,
    /// A tool used within the last N turns stays offered (the model just
    /// saw its results — yanking it mid-thought confuses the transcript).
    pub recency_turns: u32,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            min_universe: 32,
            top_k: 12,
            recency_turns: 2,
        }
    }
}

impl RouterConfig {
    /// The production defaults (INV-019).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

/// Stall detection + bounded verbal reflection.
///
/// The detector watches TURN SIGNATURES — a hash of the turn's tool
/// calls AND their observations — so legitimate polling (same call,
/// CHANGING result) never trips it; only byte-identical action+outcome
/// cycles do. Thresholds are in CYCLE OCCURRENCES, period-agnostic: an
/// A,B,A,B,A,B alternation counts 3 occurrences of a period-2 cycle.
///
/// REACH INVARIANT (the review caught this): a period-`p` cycle can repeat
/// at most `floor(window / p)` times inside the window, so a cycle is only
/// detectable when `window ≥ p · stall_after`. The default `window` is
/// sized at `stall_after · MAX_TRACKED_PERIOD` so cycles up to that period
/// reach the stall threshold; `Guard::new` additionally clamps the window to
/// the same floor so a misconfigured small window can never silently disarm
/// detection. (Period > window/2 is inherently invisible — a cycle that long
/// is indistinguishable from forward progress over the window.)
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct GuardConfig {
    /// Sliding window of turn signatures the detector scans (bounds the
    /// per-turn scan at `window²/4` u64 compares — trivial at 25 ≈ 156).
    pub window: usize,
    /// Cycle occurrences that trigger ONE corrective reflection message
    /// (Reflexion-style verbal feedback · Shinn et al. 2023,
    /// arxiv.org/abs/2303.11366) before any harder stop.
    pub nudge_after: u32,
    /// Cycle occurrences that stop the run as NIKA-467 (the nudge was
    /// spent and the loop is still burning budget without progress).
    /// Clamped to at least `nudge_after + 1` at use sites.
    pub stall_after: u32,
    /// Maximum reflection messages injected per run (bounded — verbal
    /// feedback helps once; repeating it is itself a loop).
    pub max_reflections: u32,
    /// Consecutive ALL-ERROR tool turns that trigger the error-streak
    /// reflection (shares the `max_reflections` budget).
    pub error_streak_nudge: u32,
}

impl Default for GuardConfig {
    fn default() -> Self {
        Self {
            // stall_after · MAX_TRACKED_PERIOD (5 · 8 = 40) — cycles up to
            // period 8 reach the stall threshold (floor(40/8) = 5); see the
            // REACH INVARIANT above. Raised from 25 (period-5) in the 2026-07
            // review that found a period-6 cycle ran to max_turns.
            window: 40,
            nudge_after: 3,
            stall_after: 5,
            max_reflections: 1,
            error_streak_nudge: 2,
        }
    }
}

impl GuardConfig {
    /// The production defaults (INV-019).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_internally_coherent() {
        let cfg = AgentConfig::new();
        assert!(cfg.max_parallel_tools >= 1, "0 would dispatch nothing");
        assert!(cfg.guard.stall_after > cfg.guard.nudge_after);
        // REACH INVARIANT: the window must hold `stall_after` full periods
        // of a multi-step cycle, not just one — the bug the review caught
        // (window 16 made period-4 cycles unstoppable: floor(16/4) < 5).
        assert!(
            cfg.guard.window >= cfg.guard.stall_after as usize * 5,
            "window must fit stall_after periods of cycles up to period 5"
        );
        assert!(cfg.router.top_k > 0);
        assert!(cfg.router.min_universe > cfg.router.top_k);
    }
}
