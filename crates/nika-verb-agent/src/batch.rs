// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One dispatched batch's shapes (ADR-097) — the per-call resolve output
//! and the batch fold's product — split from `lib.rs` at the 1,500-line
//! file cap when the effect memory (#1470) rode onto the resolve.

use nika_kernel::ai::provider::ContentBlock;

use crate::guard::EffectGate;
use crate::intrinsic::ComposeOutcome;

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
