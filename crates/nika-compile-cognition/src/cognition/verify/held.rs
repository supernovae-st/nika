// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The exits of a judged candidate that is not READY: withdrawn with its replayable record, held
//! (judged and not accepted: shown at most, never offered, never replayed to that judge, its
//! record kept with that rejection inside it), kept as
//! the preview where the document door's repairs end (as COLD keeps its own) or kept unjudged for
//! a round that replays it under a judge, with the words each exit says. Split from `verify.rs`
//! at the file-LOC cap (2026-10-07); the exits keep their paths
//! (`verify::{withdrawn, held, kept, preserve_unjudged, HELD_TARGET}`).

use super::{Verdict, blocked, route};
use crate::{CompileOutcome, CompileStatus, DiagnosticKind};

/// A judged candidate that is not READY, withdrawn with its questions, its requested boundary and
/// its replayable record; the request stays INCOMPLETE naming the part and the `repairs` made
/// from the judge's defects before it.
pub(in crate::cognition) fn withdrawn(
    mut out: CompileOutcome,
    verdict: &Verdict,
    repairs: usize,
) -> CompileOutcome {
    route(&mut out, "verify: not ready");
    out.status = CompileStatus::Incomplete;
    out.candidate = None;
    out.check_preview = None;
    out.requested_boundary = None;
    out.questions.clear();
    out.provenance.plan = None;
    blocked(&mut out, verdict, repairs);
    out
}

/// A judged candidate whose located defects end the document door's repairs, kept as COLD keeps
/// its draft: INCOMPLETE, shown as the preview, never offered, the `repairs` named; a doubted
/// verdict drops the replayable record, questions and boundary (`unreplayed`, as COLD's own).
pub(in crate::cognition) fn kept(
    mut out: CompileOutcome,
    verdict: &Verdict,
    repairs: usize,
) -> CompileOutcome {
    route(&mut out, "verify: not ready");
    out.status = CompileStatus::Incomplete;
    blocked(&mut out, verdict, repairs);
    super::unreplayed(&mut out, verdict);
    out
}

/// A candidate the judge answered and did not accept, with no defect located (R6): shown as the
/// preview, never offered (INCOMPLETE), its questions and boundary cleared, and its replayable
/// record kept with that rejection inside it ([`carry_declined`](super::carry_declined)), so
/// another verifier can judge these bytes with no authoring call while no later round asks the
/// same judge again on them. The `verify_held` finding says what can decide it.
pub(in crate::cognition) fn held(mut out: CompileOutcome, verdict: &Verdict) -> CompileOutcome {
    route(&mut out, "verify: not ready, candidate held");
    out.status = CompileStatus::Incomplete;
    out.requested_boundary = None;
    out.questions.clear();
    super::carry_declined(&mut out);
    blocked(&mut out, verdict, 0);
    crate::finding(
        &mut out,
        DiagnosticKind::Applied,
        HELD_TARGET,
        held_text(verdict),
    );
    out
}

/// The finding every outcome whose candidate the judge answered and did not accept carries:
/// its bytes are shown at most, never offered, never replayed to that judge. A record kept with
/// them carries that rejection, so a round replaying it asks that judge nothing.
pub(in crate::cognition) const HELD_TARGET: &str = "verify_held";

/// What a held candidate offers, by what held it: located defects the repairs did not settle,
/// a rejection with no defect located, or an abstention; why no trial ran, when the rehearsal
/// room refused these bytes (A4); and why the localization stopped, when a call got no answer.
pub(super) fn held_text(verdict: &Verdict) -> String {
    let held = if !verdict.defects.is_empty() {
        HELD_DEFECTS
    } else if verdict.rejected() {
        HELD
    } else {
        HELD_ABSTAINED
    };
    let mut text = held.to_owned();
    if let Some(reason) = (verdict.unobserved.as_ref()).and_then(|why| why["reason"].as_str()) {
        text = format!("{text} {UNOBSERVED} ({reason}).");
    }
    if verdict.stopped {
        text = format!("{text} {HELD_STOPPED}");
    }
    text
}

const HELD: &str = "The candidate was judged and not accepted, with no defect a repair could start from: it is shown, never offered, and nothing was written. A correction of the request or another verifier can decide it.";
const HELD_DEFECTS: &str = "The candidate was judged and not accepted: the parts named above stay missing. It is shown, never offered, and nothing was written; this verifier is not asked again on these bytes, in this compile or in a later round that carries this verdict. A correction of the request, another authoring model or another verifier can decide it.";
const HELD_ABSTAINED: &str = "The verifier read the candidate and abstained: it neither accepted nor rejected it, and located no defect. It is shown, never offered, and nothing was written; it is not asked again on these bytes in this compile. A correction of the request, another verifier, or a new round that authors again can decide it.";
const UNOBSERVED: &str =
    "No trial of these bytes ran: the rehearsal room refused them before any attempt";
const HELD_STOPPED: &str = "Locating what it lacks stopped at a judge call that got no answer (refused by the call bound, or failed).";

/// A candidate the judge could not judge (its call failed, or it chose none) and found no defect
/// in: withdrawn as [`withdrawn`] withdraws it (never READY, never a candidate), but its replayable
/// record is kept, so a later round replays the same bytes with no author call and asks its judge
/// again; the `verify_resume` finding says so, to the human and to the host that resumes it.
pub(in crate::cognition) fn preserve_unjudged(
    out: CompileOutcome,
    verdict: &Verdict,
) -> CompileOutcome {
    if verdict.doubted() {
        return held(out, verdict);
    }
    let record = out.provenance.plan.clone();
    let mut out = withdrawn(out, verdict, 0);
    if record.is_some() {
        crate::finding(&mut out, DiagnosticKind::Applied, "verify_resume", RESUME);
    }
    out.provenance.plan = record;
    route(&mut out, "verify: unjudged, record kept");
    out
}

/// What an unjudged candidate's kept record offers.
const RESUME: &str = "The candidate was not judged, so it is not offered; its bytes are kept: a round that replays this record under a judge asks it on the same candidate, with no new authoring call (a replay with no judge judges nothing).";
