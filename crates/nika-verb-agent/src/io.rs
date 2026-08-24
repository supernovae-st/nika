// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The verb's input/output DTOs — descended from `lib.rs` at the
//! 1500-line file cap (the `task/failed.rs` precedent · the B4 harness
//! seat was the straw). Every public path is preserved by the `lib.rs`
//! re-export; nothing here changed shape in the move.

use nika_kernel::ai::provider::TokenUsage;
use nika_kernel::runtime::agent::AgentStopReason;
use nika_types::blame::BlamePolarity;

/// The `agent` task input — spec fields, already CEL-resolved.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct AgentInput {
    /// The initial user message (required · spec §agent).
    pub prompt: String,
    /// Optional system prompt.
    pub system: Option<String>,
    /// Optional model override of the verb's default.
    pub model: Option<String>,
    /// Tool whitelist · gitignore-style globs · DEFAULT-DENY (empty =
    /// pure conversation, no tools).
    pub tools: Vec<String>,
    /// Turn budget (default [`crate::DEFAULT_MAX_TURNS`]).
    pub max_turns: Option<u32>,
    /// Cumulative token budget across all turns.
    pub max_tokens_total: Option<u64>,
    /// Sampling temperature (0-2 · validated here).
    pub temperature: Option<f32>,
    /// JSON Schema validating the FINAL output (spec `schema:`).
    pub schema: Option<serde_json::Value>,
    /// The workflow's declared `permits:` boundary (P3 B5) — the
    /// harness permission bridge judges every ask against it. `None`
    /// on the native path (the dispatch layer gates effects there) or
    /// under an undeclared block (zero authority · F-O8).
    pub permits: Option<nika_schema::types::Permits>,
    /// The operator's bound `--answer` for THIS task on a resumed run
    /// (P3 B5) — the harness gate's human verdict. `None` on a fresh
    /// run (an out-of-grants ask then pauses for the operator).
    pub gate_answer: Option<serde_json::Value>,
}

impl AgentInput {
    /// A tool-less conversation with every optional field unset.
    #[must_use]
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            system: None,
            model: None,
            tools: Vec::new(),
            max_turns: None,
            max_tokens_total: None,
            temperature: None,
            schema: None,
            permits: None,
            gate_answer: None,
        }
    }
}

/// The resolved turn budget with its F-P22 (NEP-0017) blame polarity —
/// computed ONCE at arm time as a match on `input.max_turns`: a
/// `max_turns:` the task wrote is imputed « by the caller » (F-A5); the
/// [`crate::DEFAULT_MAX_TURNS`] the engine applies on an absent key is imputed
/// « by the contract » that DECLARES it (spec 02-verbs.md §agent ·
/// `max_turns` default 10) — the failure's Display names that faulty
/// contract, so the journal's `task_failed` detail attests it.
#[derive(Clone, Copy)]
pub(crate) struct TurnBudget {
    /// The turn budget the loop enforces.
    pub(crate) max_turns: u32,
    /// Who an exhausted budget is imputed to.
    pub(crate) blame: BlamePolarity,
    /// The faulty contract the blame names.
    pub(crate) blame_source: &'static str,
}

/// The verb's result value — text, or the validated JSON value when the
/// task carried a `schema:` (or `nika:done` carried a `result:`).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum AgentValue {
    /// Free-form final assistant text.
    Text(String),
    /// Schema-validated structured output.
    Structured(serde_json::Value),
}

/// The `agent` verb output.
#[derive(Debug)]
#[non_exhaustive]
pub struct AgentOutput {
    /// The shaped output value (`.output` in spec terms).
    pub output: AgentValue,
    /// Why the loop ended (success reasons only — failures are errors).
    pub stop_reason: AgentStopReason,
    /// How many model turns ran.
    pub turns: u32,
    /// Cumulative tokens across all turns (input + output).
    pub total_tokens: u64,
    /// Real spend reported by the loop's TOOLS (a tool whose structured
    /// output carries a top-level numeric `cost_usd` — the image builtin
    /// on tick-billed providers · future paid tools). `None` when no tool
    /// reported spend — never a fake zero. The LLM turns' own cost is
    /// priced by the DISPATCH layer from [`Self::usage`].
    pub tools_cost_usd: Option<f64>,
    /// The absorbed usage split across every provider round-trip of the
    /// loop (turns + schema re-asks) — the input/output/cache meters the
    /// cost layer prices with the same resolver `infer` uses. Zero-valued
    /// on harness-built outputs that never touched a provider.
    pub usage: TokenUsage,
    /// The model the loop resolved and ran (`provider/name`) — the
    /// pricing/attribution key. `None` on harness-built outputs.
    pub model_resolved: Option<String>,
}

impl AgentOutput {
    /// Construct an output (INV-019 · per-crate `new()` on every
    /// `#[non_exhaustive]` struct so L3/test harnesses can build one).
    #[must_use]
    pub fn new(
        output: AgentValue,
        stop_reason: AgentStopReason,
        turns: u32,
        total_tokens: u64,
    ) -> Self {
        Self {
            output,
            stop_reason,
            turns,
            total_tokens,
            tools_cost_usd: None,
            usage: TokenUsage::default(),
            model_resolved: None,
        }
    }

    /// Attach the loop's accumulated tool spend (builder — `new()` stays
    /// stable for existing callers).
    #[must_use]
    pub fn with_tools_cost_usd(mut self, cost: Option<f64>) -> Self {
        self.tools_cost_usd = cost;
        self
    }

    /// Attach the loop's absorbed usage split + resolved model (builder —
    /// decorated once at the loop's single return site).
    #[must_use]
    pub fn with_spend_identity(mut self, usage: TokenUsage, model_resolved: String) -> Self {
        self.usage = usage;
        self.model_resolved = Some(model_resolved);
        self
    }
}
