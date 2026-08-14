// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The CLAIMS — what a surface may print, and under which condition.
//! Ported from the studio's SSOT (its claims registry), the part written
//! after four false claims were caught in one evening.
//!
//! A claim is not only what one WRITES — it is also what one forbids
//! oneself to write. These predicates are the executable form.
//!
//! ⚠️ This paragraph used to read « a renderer `debug_assert!`s them ».
//! No renderer did. The module had ZERO consumers in `src/` while the
//! surface on the other side of the wasm door decided the same questions
//! inline, with a different rule — a law nothing applied, next to a rule
//! nobody had written down. An unwired predicate is not a law, it is a wish.
//!
//! ⚠️ And the sentence that replaced it — « the verdicts ride
//! [`crate::wasm::derive_run`] » — was itself an OVERCLAIM, caught by two
//! independent Gate-11 lenses within the hour. Two of four ride it. The
//! honest state, per predicate:
//!
//! | predicate | wired | where, and why |
//! |---|---|---|
//! | [`may_claim_chain_intact`] | ✅ | `derive_run` → `claims.chain_intact` |
//! | [`may_claim_bottleneck`] | ✅ | `derive_run` → `claims.bottleneck` |
//! | [`may_claim_check_clean`] | ❌ | its input is a CHECKER report, which
//!   this door never sees. Its consumer is the check surface, a different
//!   crate. It lives here so the law has one home, not because this crate
//!   applies it. |
//! | [`must_mark_simulated`] | ❌ | it answers `true` unconditionally — a
//!   RULE addressed to renderers («if the content is simulated, say so»),
//!   not a per-run verdict. Shipping a constant through a per-run door
//!   would be ceremony, and the run-level question it looks like is
//!   already answered by `chain_intact`. |
//!
//! Writing « two of four, and here is why the other two cannot » costs one
//! table. Writing « the verdicts ride the door » cost two reviewers.
//!
//! The two gate claims (`gate_blind` · `gate_law`) are TYPE-level, not
//! predicates: they bind the gate-question builder (a permit gate's
//! question carries the DECISION — hosts · paths · tool names — never one
//! byte of content the workflow fetched, read or received). They land with
//! the gate builder; the predicates below are complete without them.

use crate::derive::Neck;
use crate::model::Run;

/// The words a producer writes when it has nothing real to write. They are
/// checked against BOTH fields, because a fabricated run marks whichever one
/// its producer reaches for, and the claim must not depend on which.
const FABRICATED: [&str; 2] = ["synthetic", "never ran"];

fn is_fabricated(field: &str) -> bool {
    FABRICATED.iter().any(|f| field.contains(f))
}

/// `chain intact` — only on a run that carries POSITIVE evidence of having
/// happened. THE claim of the product: placing it on emptiness empties it
/// everywhere.
///
/// **The receipt must exist.** A trace id is what a verifier would ask for
/// first, so an empty `trace` refuses regardless of anything else. Then
/// NEITHER field may carry a fabrication sentinel — a producer marks
/// whichever one it reaches for, and a real id beside `when: "synthetic"`
/// is a contradiction, which refuses too.
///
/// This predicate used to read `!when.contains(sentinel)` alone, which
/// failed OPEN: a run with no trace and no stamp at all claimed an intact
/// chain, and so did any `when` nobody had thought to enumerate. Denying
/// two magic words is not evidence — it is the absence of one specific lie.
///
/// An EMPTY `when` beside a real trace still claims, deliberately: the
/// journal fold (`ingress::run_from_journal`) leaves the display stamp
/// blank while carrying a real `workflow_started` uuid. That is a missing
/// field on a real run, not a fabricated one, and refusing there would
/// have made the honest case the failing case.
///
/// The two surfaces also disagreed. This crate judged `when`; the renderer
/// on the other side of the wasm door judged the trace id. They only agreed
/// because the fixtures happened to set both. One law now, and it RIDES the
/// door — the browser reads the verdict instead of re-deriving it, the same
/// argument the `groups` projection already won.
///
/// ⚠️ Honest limit · `trace` and `when` are the presence of a receipt, not
/// its verification. Whether the hash chain actually holds is the engine
/// verifier's answer, never this crate's. This predicate gates PRINTING
/// the claim; it does not establish it.
#[must_use]
pub fn may_claim_chain_intact(run: &Run) -> bool {
    !run.trace.trim().is_empty() && !is_fabricated(&run.trace) && !is_fabricated(&run.when)
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
