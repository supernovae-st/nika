// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The CLAIMS — what a surface may print, and under which condition.
//! Ported from the studio's SSOT (its claims registry), the part written
//! after four false claims were caught in one evening.
//!
//! A claim is not only what one WRITES — it is also what one forbids
//! oneself to write. These predicates are the executable form: a renderer
//! `debug_assert!`s them, and a false claim fails LOUD in development
//! instead of eroding the product's one promise quietly.
//!
//! The two gate claims (`gate_blind` · `gate_law`) are TYPE-level, not
//! predicates: they bind the gate-question builder (a permit gate's
//! question carries the DECISION — hosts · paths · tool names — never one
//! byte of content the workflow fetched, read or received). They land with
//! the gate builder; the predicates below are complete without them.

use crate::derive::Neck;
use crate::model::Run;

/// `chain intact` — only if the trace is REAL, never on synthetic
/// timings (the run's `when` carries the declared sentinel then). THE
/// claim of the product: placing it on emptiness empties it everywhere.
#[must_use]
pub fn may_claim_chain_intact(run: &Run) -> bool {
    !run.when.contains("synthetic") && !run.when.contains("never ran")
}

/// `check · clean` — only if the checker ANSWERED (a report exists) and
/// is clean. A `clean=true` from the wasm does not mean the binary would
/// accept it; the legs are named beside the claim (the renderer's half),
/// and the answer's existence is decided here.
#[must_use]
pub fn may_claim_check_clean(report_clean: Option<bool>) -> bool {
    report_clean == Some(true)
}

/// `holds its wave on its own` — only if at least one finished step
/// waits on it. A bottleneck that costs nothing teaches one to optimize
/// where there is nothing (the derivation already refuses `blocked == 0`;
/// this is the same law at the claim seam).
#[must_use]
pub fn may_claim_bottleneck(neck: Option<&Neck>) -> bool {
    neck.is_some_and(|n| n.blocked > 0)
}

/// `⟨simulated⟩` — always claimable; the claim's job is to be PRINTED on
/// simulated content, never withheld. A bench that hides what it
/// simulates is a demo.
#[must_use]
pub const fn must_mark_simulated() -> bool {
    true
}
