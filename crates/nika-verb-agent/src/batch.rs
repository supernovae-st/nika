// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One dispatched batch's shapes (ADR-097) — the per-call resolve output
//! and the batch fold's product — split from `lib.rs` at the 1,500-line
//! file cap when the effect memory (#1470) rode onto the resolve.

use nika_kernel::ai::provider::ContentBlock;

use crate::guard::{EffectGate, Guard, turn_signature};
use crate::intrinsic::ComposeOutcome;
use crate::observe::{AgentEvent, AgentObserver};
use crate::router::ToolRouter;

/// One resolved tool call (phase-1 output · ADR-097): the result block
/// plus what the fold needs (name + args for the signature/router · the
/// compose outcome for its telemetry).
pub(crate) struct Resolved {
    pub(crate) block: ContentBlock,
    pub(crate) name: String,
    pub(crate) args: serde_json::Value,
    /// Real spend the tool reported (top-level `cost_usd` in its
    /// structured output) — summed into the batch.
    pub(crate) cost_usd: Option<f64>,
    pub(crate) compose: Option<ComposeOutcome>,
    /// The effect memory's verdict on this call (#1470) — what the fold
    /// remembers (`Fresh`) or reports as refused (`Replay`).
    pub(crate) gate: EffectGate,
}

impl Resolved {
    pub(crate) fn new(
        block: ContentBlock,
        name: String,
        args: serde_json::Value,
        cost_usd: Option<f64>,
        compose: Option<ComposeOutcome>,
        gate: EffectGate,
    ) -> Self {
        Self {
            block,
            name,
            args,
            cost_usd,
            compose,
            gate,
        }
    }
}

/// What one dispatched batch produced (results + the guard's evidence).
pub(crate) struct BatchOutcome {
    /// The tool-result blocks, in dispatch order.
    pub(crate) results: Vec<ContentBlock>,
    /// Σ of the batch's tool-reported real spend (0.0 = none reported).
    pub(crate) tools_cost_usd: f64,
    /// Turn signature over actions + observations (see `guard`).
    pub(crate) signature: u64,
    /// A bounded digest of the observations, for the next routing query.
    pub(crate) observations_digest: String,
    /// Whether EVERY call in the batch errored.
    pub(crate) all_errors: bool,
}

/// The batch fold's accumulators (phase 2 · ADR-097): what the
/// SEQUENTIAL pass over the resolved calls collects before
/// [`BatchFold::finish`] seals it into the [`BatchOutcome`].
pub(crate) struct BatchFold {
    results: Vec<ContentBlock>,
    sig_calls: Vec<(String, serde_json::Value)>,
    sig_results: Vec<(String, bool)>,
    /// The error STREAK counts real tool failures only — a compose
    /// verdict of `invalid` is the EXPECTED feedback of the draft→repair
    /// loop, never a tool fault, so it must not arm the error-streak
    /// nudge (which would spend the one reflection budget during normal
    /// repair). Tracked separately from the per-block `is_error`.
    all_dispatch_errors: bool,
    had_dispatch: bool,
    tools_cost_usd: f64,
}

impl BatchFold {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            results: Vec::with_capacity(capacity),
            sig_calls: Vec::with_capacity(capacity),
            sig_results: Vec::with_capacity(capacity),
            all_dispatch_errors: true,
            had_dispatch: false,
            tools_cost_usd: 0.0,
        }
    }

    /// Fold ONE resolved call into the batch (phase-2 unit — request
    /// order): the spend, the effect memory, the telemetry, the guard
    /// signature parts, the router's recency ledger.
    pub(crate) fn accept(
        &mut self,
        observer: &dyn AgentObserver,
        turn: u32,
        r: Resolved,
        guard: &mut Guard,
        router: &mut ToolRouter,
    ) {
        if let Some(cost) = r.cost_usd {
            self.tools_cost_usd += cost;
        }
        // An intrinsic reports ComposeChecked; a real dispatch reports
        // ToolCompleted. They are NOT both — `nika:compose` is
        // loop-served, never a tool invocation, so it must not surface
        // as one on the stream (a `tool_invoked` for a call that never
        // hit the executor would mislead every reader).
        // An effectful call that settled (either way) is remembered; a
        // refused replay is a decision, never a dispatch (no
        // `tool_invoked` for a call that never hit the executor).
        match r.gate {
            EffectGate::Fresh(sig) => guard.remember_effect(sig),
            EffectGate::Replay => observer.on_event(&AgentEvent::EffectReplayRefused {
                turn,
                name: r.name.clone(),
            }),
            EffectGate::Free => {}
        }
        if let Some(outcome) = r.compose {
            observer.on_event(&AgentEvent::ComposeChecked {
                turn,
                valid: outcome.valid,
                violations: outcome.violations,
            });
        } else if r.gate == EffectGate::Replay {
            // refused in place · the block still shapes the turn signature
        } else if let ContentBlock::ToolResult { is_error, .. } = &r.block {
            self.had_dispatch = true;
            self.all_dispatch_errors &= *is_error;
            observer.on_event(&AgentEvent::ToolCompleted {
                turn,
                name: r.name.clone(),
                is_error: *is_error,
            });
        }
        // The guard signature reads EVERY observation (compose
        // included — a repeating compose draft is still a no-progress
        // loop) regardless of which event reported it.
        if let ContentBlock::ToolResult {
            content, is_error, ..
        } = &r.block
        {
            self.sig_results.push((content.clone(), *is_error));
        }
        router.note_used(&r.name, turn);
        self.sig_calls.push((r.name, r.args));
        self.results.push(r.block);
    }

    /// Seal the batch: the turn signature, the bounded observations
    /// digest, the error-streak verdict.
    pub(crate) fn finish(self) -> BatchOutcome {
        // ' '-joined so adjacent results don't fuse into phantom seam tokens
        // ("…statusfetch…") in the next turn's BM25 query.
        let observations_digest = self
            .sig_results
            .iter()
            .flat_map(|(content, _)| content.chars().take(512).chain(std::iter::once(' ')))
            .take(2048)
            .collect();
        BatchOutcome {
            signature: turn_signature(&self.sig_calls, &self.sig_results),
            results: self.results,
            tools_cost_usd: self.tools_cost_usd,
            observations_digest,
            // No real dispatch this turn (compose-only) ⇒ no error streak.
            all_errors: self.had_dispatch && self.all_dispatch_errors,
        }
    }
}
