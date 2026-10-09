// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A localization that stops, and a doubt nothing decided, held by its cause (R6): a task, an
//! extra or a part call that gets no answer stops the verdict there, nothing more asked of that
//! judge, the request unknown and never contested, and the findings say why it stopped. A doubt
//! no answer decided is held, worded by how the judge declined: a rejection, an abstention, and
//! whether locating what it lacks stopped. A contested part is named apart from the contested
//! request.

use super::super::super::{blocked, held};
use super::*;
use crate::DiagnosticKind;

/// What a candidate judged and rejected with no defect located is held with.
pub(super) const HELD: &str = "The candidate was judged and not accepted, with no defect a repair could start from: it is shown, never offered, and nothing was written. A correction of the request or another verifier can decide it.";
/// What a candidate the verifier only abstained on is held with: an abstention is never carried
/// to a later round, so a new round that authors again can decide it.
const HELD_ABSTAINED: &str = "The verifier read the candidate and abstained: it neither accepted nor rejected it, and located no defect. It is shown, never offered, and nothing was written; it is not asked again on these bytes in this compile. A correction of the request, another verifier, or a new round that authors again can decide it.";
/// What a candidate is held with when the judge rejected the whole request and nothing narrower
/// stands (unresolved): nothing is verified; `parts` were asked alone.
pub(super) fn unresolved(parts: usize) -> String {
    format!(
        "The verifier doubted the request as a whole but located nothing: asked alone, none of its parts ({parts}) was found missing, no task was found doing anything the request does not ask, and no run of these bytes decided it. Nothing is verified: the workflow is shown, never proposed, and nothing was written. Review it and describe a correction, or choose another verifier."
    )
}
/// What a held candidate adds when locating what it lacks stopped at a call with no answer.
const HELD_STOPPED: &str = "Locating what it lacks stopped at a judge call that got no answer (refused by the call bound, or failed).";
/// Why a verification left the rest of its questions unasked.
const STOPPED: &str = "The verification stopped at a judge call that got no answer (refused by the call bound, or failed: the receipt says which); nothing after it was asked of that judge. Next: another round, or a larger call bound.";
/// The field case's last part.
const WEEK: &str = "write the list to ./out/week.md";

/// What the verifier names of a part or request it could not settle.
fn unsettled(clause: &str) -> String {
    format!(
        "The judge could not settle `{clause}` against the candidate (it abstained, answered outside its options, or its call failed); nothing is READY on it. Next: a judge that answers, or a restatement the deterministic reader reads."
    )
}

/// What the verifier names of a request it rejected with no defect located and no run deciding.
fn disagreement(doubt: &str) -> String {
    format!(
        "The judge did not accept the request as carried ({doubt}) and located no defect a repair could start from; the same judge asked again decides nothing ({UNOBSERVED}). Nothing is READY on it. Next: a correction of the request, or another verifier."
    )
}

/// The findings a blocked verification of `verdict` leaves, in order.
pub(super) fn told(verdict: &Verdict) -> Vec<String> {
    let mut out = crate::initial();
    blocked(&mut out, verdict, 0);
    (out.diagnostics.iter())
        .filter(|d| d.target == "semantic_verification")
        .map(|d| d.message.clone())
        .collect()
}

/// [`CANDIDATE`] finished READY, its replayable record kept: what a door holds.
fn finished() -> CompileOutcome {
    let mut out = crate::initial();
    nika_compile::surface::finish(CANDIDATE.to_owned(), &mut out);
    assert_eq!(out.status, crate::CompileStatus::Ready, "{out:#?}");
    out.provenance.plan = Some(json!({"strategy": "native"}));
    out
}

/// A held candidate: shown as the preview (its bytes and Check kept), never offered (INCOMPLETE,
/// no question, no boundary), its record dropped; the findings the blocked verdict names, then
/// the one `verify_held` finding worded `why`.
pub(super) fn assert_held(verdict: &Verdict, why: &str) {
    let out = held(finished(), verdict);
    assert_eq!(out.status, crate::CompileStatus::Incomplete, "{out:#?}");
    assert_eq!(out.candidate.as_deref(), Some(CANDIDATE));
    assert!(out.check_preview.is_some(), "{out:#?}");
    assert!(out.requested_boundary.is_none() && out.questions.is_empty());
    assert_eq!(out.provenance.plan, None);
    let route = &out.provenance.decision.as_ref().unwrap()["route"];
    assert_eq!(route, &json!(["verify: not ready, candidate held"]));
    let semantic: Vec<String> = (out.diagnostics.iter())
        .filter(|d| d.target == "semantic_verification")
        .map(|d| d.message.clone())
        .collect();
    assert_eq!(semantic, told(verdict));
    let kept: Vec<(DiagnosticKind, &str)> = (out.diagnostics.iter())
        .filter(|d| d.target == "verify_held")
        .map(|d| (d.kind, d.message.as_str()))
        .collect();
    assert_eq!(kept, [(DiagnosticKind::Applied, why)]);
}

/// A task question that gets no answer (a failed call, or one the authority refuses before any
/// byte leaves) stops the localization: the part it asked about and every later part stay
/// unknown, then the request, never contested; no later part, no question over the run and no
/// extra question is asked. The findings name each and why nothing after was asked; the
/// rejected candidate is held, saying where locating what it lacks stopped.
#[tokio::test]
async fn a_failed_or_refused_task_question_stops_the_localization() {
    let observation = observed(true, true);
    let mut script = pointing(4, 2, Fail);
    script.truncate(5);
    let judge = Scripted::new(script);
    let policy = AuthoringPolicy::new(MODEL, 256, Duration::from_secs(2));
    let request = CompileRequest::create(CALENDAR);
    let provider = Judge::Provider(&policy, &judge);
    let shown = Some(&observation);
    let Judged { verdict, .. } = judged(CALENDAR, &request, CANDIDATE, &provider, shown).await;
    let asked = [
        "verify-request",
        "verify-part-0",
        "verify-part-1",
        "verify-part-2",
        "verify-point-2",
    ];
    assert_eq!(ids(&verdict), asked);
    let stopped = found(&[], &[ZONE, WEEK, CALENDAR], &[], &["unfaithful"], &[]);
    assert_eq!(lists(&verdict), stopped);
    assert_eq!(counts(&verdict), (5, 4, 4));
    assert!(verdict.stopped && verdict.rejected() && verdict.doubted());
    assert_eq!(judge.left(), 0);
    let named = [
        unsettled(ZONE),
        unsettled(WEEK),
        unsettled(CALENDAR),
        STOPPED.to_owned(),
    ];
    assert_eq!(told(&verdict), named);
    assert_held(&verdict, &format!("{HELD} {HELD_STOPPED}"));
    // The authority refuses the task question: the same stop, nothing sent after it.
    let inner = Scripted::new([
        (Request, Choose("unfaithful")),
        (Part, Choose("carried")),
        (Part, Choose("carried")),
        (Part, Choose("missing")),
    ]);
    let authority = Arc::new(Envelope::new(4, "authorize more judge calls"));
    let seat = Authority::new(inner, authority.clone());
    let provider = Judge::Provider(&policy, &seat);
    let Judged { verdict, out, .. } = judged(CALENDAR, &request, CANDIDATE, &provider, shown).await;
    assert_eq!(authority.account(), json!({"sent": 4, "refused": 1}));
    assert_eq!(ids(&verdict), asked);
    assert_eq!(lists(&verdict), stopped);
    assert_eq!(counts(&verdict), (5, 4, 4));
    let receipt = out.provenance.authoring.as_ref().unwrap();
    let last = &receipt.context.last().unwrap()["result"]["failure_kind"];
    assert_eq!(last, "admission_refused");
    assert_eq!(seat.inner().left(), 0);
}

/// An extra question that gets no answer stops the localization too: though a whole run of these
/// bytes was observed, no question is asked over it, and the request is unknown beside the extra
/// question, never contested; the extra question is named unanswered, never as a choice the judge
/// did not make. The rejected candidate is held, saying where locating what it lacks stopped.
#[tokio::test]
async fn a_failed_extra_question_stops_before_any_question_over_the_run() {
    let mut script = vec![(Request, Choose("unfaithful"))];
    script.extend(repeat_n((Part, Choose("carried")), 3));
    script.push((Extra, Fail));
    let judge = Scripted::new(script);
    let policy = AuthoringPolicy::new(MODEL, 256, Duration::from_secs(2));
    let provider = Judge::Provider(&policy, &judge);
    let request = CompileRequest::create(ORDERS);
    let run = Some(&observed(true, true));
    let Judged { verdict, .. } = judged(ORDERS, &request, &elsewhere(), &provider, run).await;
    assert_eq!(ids(&verdict).last(), Some(&"verify-extra"));
    let stopped = found(&[], &[EXTRA_UNANSWERED, ORDERS], &[], &["unfaithful"], &[]);
    assert_eq!(lists(&verdict), stopped);
    assert_eq!(counts(&verdict), (5, 4, 4));
    assert!(verdict.stopped && verdict.contested.is_empty());
    let named = [
        unsettled(EXTRA_UNANSWERED),
        unsettled(ORDERS),
        STOPPED.to_owned(),
    ];
    assert_eq!(told(&verdict), named);
    assert_held(&verdict, &format!("{HELD} {HELD_STOPPED}"));
    assert_eq!(judge.left(), 0);
}

/// A doubt no part locates, no task explains and no run of these bytes decides is held by how
/// the judge declined (R6): a whole-request NONE whose every part is carried only abstained (the
/// engine's facts settle every task, and an abstention is never asked where it is), so the
/// request is unknown, never contested, and the candidate is held as an abstention; the same
/// answers after « unfaithful » rejected it, and asked where that rejection is, the judge named
/// nothing: the request is unresolved, contested with why nothing decided it, and the candidate
/// held saying nothing is verified and how many parts were asked alone.
#[tokio::test]
async fn a_doubt_nothing_decided_is_held_by_how_the_judge_declined() {
    let judge = Scripted::new(undisputed("none", 3, None));
    let Judged { verdict, .. } = provided(ORDERS, &judge, None).await;
    assert_eq!(lists(&verdict), found(&[], &[ORDERS], &[], &["none"], &[]));
    assert_eq!(counts(&verdict), (4, 4, 3));
    assert_eq!(verdict.declined, Declined::Abstained);
    assert!(!verdict.rejected() && verdict.doubted() && !verdict.stopped);
    assert!(!verdict.unresolved());
    assert_eq!(told(&verdict), [unsettled(ORDERS)]);
    assert_held(&verdict, HELD_ABSTAINED);
    assert_eq!(judge.left(), 0);
    let judge = Scripted::new(undisputed(
        "unfaithful",
        3,
        Some((Locate, Choose("unlocated"))),
    ));
    let Judged { verdict, .. } = provided(ORDERS, &judge, None).await;
    let disputed = found(&[], &[], &[ORDERS], &["unfaithful"], &[UNOBSERVED]);
    assert_eq!(lists(&verdict), disputed);
    assert_eq!(verdict.declined, Declined::Rejected);
    assert!(verdict.doubted() && verdict.unresolved());
    assert_eq!(told(&verdict), [disagreement("unfaithful")]);
    assert_held(&verdict, &unresolved(3));
    assert_eq!(judge.left(), 0);
}

/// The field case (R6): the time-zone prohibition judged missing, then no task failing it, and
/// the request rejected with no run of these bytes. The findings name the contested part in its
/// own words and the contested request apart, each once: two different findings.
#[tokio::test]
async fn the_field_case_names_its_contested_part_apart_from_the_contested_request() {
    let mut script = pointing(4, 2, Choose("no_task"));
    script.push((Extra, Choose("only_requested")));
    let judge = Scripted::new(script);
    let Judged { verdict, .. } = provided(CALENDAR, &judge, None).await;
    let disputed = found(&[], &[], &[ZONE, CALENDAR], &["unfaithful"], &[UNOBSERVED]);
    assert_eq!(lists(&verdict), disputed);
    assert_eq!(verdict.request.as_deref(), Some(CALENDAR));
    let part = format!(
        "The judge found « {ZONE} » missing but then named no task that fails it and no operation it lacks: nothing decided it, and nothing is READY on it."
    );
    assert_eq!(told(&verdict), [part, disagreement("unfaithful")]);
    assert_held(&verdict, HELD);
    assert_eq!(judge.left(), 0);
}
