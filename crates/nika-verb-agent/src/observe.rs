// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! [`AgentObserver`] — the loop's telemetry seam (ADR-093).
//!
//! Every internal DECISION the loop takes — which tools the model was
//! shown, a reflection injected, a stall detected, a compose draft
//! checked, a budget snapshot — is reported through this seam as a typed
//! [`AgentEvent`]. Per `AgentOps` (Dong · Lu · Zhu 2024,
//! arxiv.org/abs/2411.05285) agent observability must expose the agent's
//! decisions, not just its I/O; per TRAIL (Deshpande et al. 2025,
//! arxiv.org/abs/2505.08638) traces are only debuggable when the failure
//! evidence (the cycle period, the repeat count) rides IN the trace.
//!
//! The L3 runtime maps these payloads 1:1 onto the `nika-event` kinds
//! (`agent_tools_selected` · `agent_nudge` · `agent_stalled` ·
//! `agent_compose_checked` · `agent_budget_checkpoint`); the verb crate
//! itself stays event-log-agnostic (INV-024: ONE emission site per verb
//! path — that site is the runtime adapter, not the loop).
//!
//! Callbacks are synchronous and MUST be cheap (enqueue, don't block):
//! the loop calls them between awaits on its hot path.

/// Where a tool definition came from, classified by its closed namespace
/// prefix. The router reports per-source counts so MCP-heavy universes
/// are visible at a glance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ToolSource {
    /// `nika:` — the engine's builtin stdlib.
    Builtin,
    /// `mcp:` — Model Context Protocol servers.
    Mcp,
    /// `skill:` — stored-workflow skills (forward seam: routed + counted
    /// today, served by definition providers when the pack layer ships).
    Skill,
    /// `agent:` — loop-intrinsic capabilities (compose · done sentinel
    /// lives under `nika:` for spec-compat).
    Intrinsic,
    /// Any other prefix (an MCP alias, a future namespace).
    Other,
}

impl ToolSource {
    /// Classify a tool name by its namespace prefix.
    #[must_use]
    pub fn classify(name: &str) -> Self {
        if name.starts_with("nika:") {
            Self::Builtin
        } else if name.starts_with("mcp:") {
            Self::Mcp
        } else if name.starts_with("skill:") {
            Self::Skill
        } else if name.starts_with("agent:") {
            Self::Intrinsic
        } else {
            Self::Other
        }
    }
}

/// Per-source definition counts for one routing decision.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct SourceCounts {
    /// `nika:` builtins offered.
    pub builtin: u32,
    /// `mcp:` tools offered.
    pub mcp: u32,
    /// `skill:` workflows offered.
    pub skill: u32,
    /// `agent:` intrinsics offered.
    pub intrinsic: u32,
    /// Everything else.
    pub other: u32,
}

impl SourceCounts {
    /// Tally a set of tool names.
    #[must_use]
    pub fn tally<'a>(names: impl Iterator<Item = &'a str>) -> Self {
        let mut counts = Self::default();
        for name in names {
            match ToolSource::classify(name) {
                ToolSource::Builtin => counts.builtin += 1,
                ToolSource::Mcp => counts.mcp += 1,
                ToolSource::Skill => counts.skill += 1,
                ToolSource::Intrinsic => counts.intrinsic += 1,
                ToolSource::Other => counts.other += 1,
            }
        }
        counts
    }
}

/// Why a reflection message was injected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NudgeReason {
    /// The same action+observation cycle repeated `nudge_after` times.
    RepeatedActions,
    /// Every tool call failed for `error_streak_nudge` consecutive turns.
    ErrorStreak,
}

/// One observable loop decision. Payloads are small and owned — an
/// observer may hold them past the callback.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum AgentEvent {
    /// The loop started: resolved model + the whitelisted universe size.
    RunStarted {
        /// Resolved model id.
        model: String,
        /// Whitelisted definitions available (after filtering, before
        /// any per-turn routing).
        universe: u32,
        /// Whether per-turn routing is active for this run.
        routed: bool,
    },
    /// A model turn is about to be requested.
    TurnStarted {
        /// 1-based turn counter.
        turn: u32,
    },
    /// The router decided which definitions this turn's request carries.
    ToolsSelected {
        /// 1-based turn counter.
        turn: u32,
        /// Definitions offered this turn.
        offered: u32,
        /// Whitelisted universe size.
        universe: u32,
        /// Per-source breakdown of the offered set.
        by_source: SourceCounts,
    },
    /// A whitelisted tool call was dispatched and completed.
    ToolCompleted {
        /// 1-based turn counter.
        turn: u32,
        /// The tool's full name (whitelist-admitted, safe to log).
        name: String,
        /// Whether the result was an error fed back to the model.
        is_error: bool,
    },
    /// An `agent:compose` draft was statically checked.
    ComposeChecked {
        /// 1-based turn counter.
        turn: u32,
        /// Whether the draft parsed AND passed Core conformance.
        valid: bool,
        /// Conformance violation count (0 when valid).
        violations: u32,
    },
    /// A corrective reflection message was injected.
    Nudged {
        /// 1-based turn counter.
        turn: u32,
        /// What triggered it.
        reason: NudgeReason,
        /// Cycle period in turns (0 for an error streak).
        period: u32,
    },
    /// The stall threshold was crossed; the loop stops as NIKA-467.
    Stalled {
        /// 1-based turn counter.
        turn: u32,
        /// Detected cycle period in turns.
        period: u32,
        /// How many times the cycle repeated.
        repeats: u32,
    },
    /// Per-turn budget snapshot (after the turn's spend was counted).
    BudgetCheckpoint {
        /// 1-based turn counter.
        turn: u32,
        /// Cumulative tokens (input + output) across all turns so far.
        total_tokens: u64,
        /// The task's `max_tokens_total`, when set.
        budget: Option<u64>,
    },
    /// The loop ended successfully.
    Finished {
        /// Total turns run.
        turns: u32,
        /// Total tokens spent.
        total_tokens: u64,
    },
}

/// The observer seam. Implementations MUST be cheap and non-blocking.
pub trait AgentObserver: Send + Sync {
    /// One loop decision happened.
    fn on_event(&self, event: &AgentEvent);
}

/// The default observer: drops everything (zero overhead when unwired).
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopObserver;

impl AgentObserver for NoopObserver {
    fn on_event(&self, _event: &AgentEvent) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_classification_covers_the_closed_namespaces() {
        assert_eq!(ToolSource::classify("nika:read"), ToolSource::Builtin);
        assert_eq!(ToolSource::classify("mcp:server/tool"), ToolSource::Mcp);
        assert_eq!(ToolSource::classify("skill:audit-site"), ToolSource::Skill);
        assert_eq!(ToolSource::classify("agent:compose"), ToolSource::Intrinsic);
        assert_eq!(ToolSource::classify("custom-thing"), ToolSource::Other);
        // Prefixes are exact: a bare namespace word is not its namespace.
        assert_eq!(ToolSource::classify("nika"), ToolSource::Other);
    }

    #[test]
    fn source_counts_tally_each_bucket() {
        let names = [
            "nika:read",
            "nika:write",
            "mcp:gh/issues",
            "skill:weekly-report",
            "agent:compose",
            "weird",
        ];
        let counts = SourceCounts::tally(names.iter().copied());
        assert_eq!(
            counts,
            SourceCounts {
                builtin: 2,
                mcp: 1,
                skill: 1,
                intrinsic: 1,
                other: 1,
            }
        );
    }
}
