// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A document a Session wrote, qualified before it becomes a proposal. The strict parser,
//! pure Check, stated selections and fidelity laws apply to its exact bytes. With both an
//! authoring provider and a decision seat, the provider reads the exact request and scoped
//! answers without the candidate, then independently audits that reading. The seat qualifies
//! its concrete program obligations against the original context, source and observations;
//! model dataflow, extra effects, removals and trials still apply. This program-only claim
//! does not certify preparation acts or grant Save/Run authority.
//!
//! Seat-only and provider-only callers retain the whole-request verification route. A terminal
//! identical refusal may be carried without calls; a carried reading is otherwise audited
//! anew and cannot carry positive program judgments. Failed or incomplete interpretation
//! stops before the trial or seat and stays retryable. Located defects keep a preview for
//! repair; unresolved judgments stay unjudged; no permitted judge means no READY.

pub(super) mod interpretation;

use super::verify::{self, Judge};
use super::{Cognition, admitted, forensic, policy_bounded, rehearsal};
use crate::fidelity::resolution::Resolution;
use crate::rehearse::Rehearse;
use crate::types::Input;
use crate::{CompileOutcome, CompileRequest, CompileStatus, DiagnosticKind, lexicon};
pub use nika_compile_seats::judge::Removal;
use nika_compile_seats::judge::Witness;
use nika_kernel::ai::provider::ProviderInferDyn;
use serde_json::{Value, json};

/// The route step every outcome of this door starts with.
const DOOR: &str = "conversation: document";
/// The finding target of a refusal the laws make before any judge reads the bytes.
const REFUSED: &str = "conversation";
/// The finding target of a removal claim the judge did not confirm.
const REMOVAL: &str = "removal";
/// What a revision handed to this door is told: the judge frames a revision by its base.
const NOT_CREATED: &str = "A conversation's document is verified against the request it creates: a revision of a saved workflow is judged with its base by the revision door, so nothing was judged here and nothing is READY.";
/// What a candidate no judge settled says (R4 A11): nothing is READY on its bytes.
const UNJUDGED: &str = "No judge was permitted to verify this conversation's candidate: no law reads what its programs do, and no verdict settled the whole request against these bytes, so nothing is READY. Permit a judge (the authoring route or a decision seat) and verify it again.";

/// Qualify a conversation's document against its request. With both intelligences, the
/// provider reads and audits the need, and the seat qualifies the resulting program duties;
/// with one judge, its whole-request verdict applies. READY covers the program, never the
/// completion of preparation acts or permission to Save/Run.
///
/// `request` is what a compile round of the same conversation would send: the person's stated
/// request as a creation (`CompileRequest::create`), the world its host observed for it
/// (`with_knowledge`: the observation the one-shot door reads, so the laws that read the project
/// fire), the verdicts its earlier rounds kept (`with_declined`: a judge is never asked again on
/// bytes it rejected) and the authoring policy its provider judges under. `selections` are the
/// values the author states on the person's words and the ones the host verified itself
/// ([`judge_document`](crate::judge_document)). `host`, when lent, runs the READY bytes once in
/// its room and the verdict reads that run; the room's account excludes the endpoint routes the
/// request states from the files it observes (`fidelity::stated_routes`).
///
/// # Errors
/// None today: every refusal is an outcome; the `Result` matches the other compile entries.
pub async fn verify_document<P: ProviderInferDyn>(
    request: &CompileRequest,
    candidate: &str,
    selections: (&[Resolution], &[Resolution]),
    cognition: Cognition<'_, P>,
    host: Option<&dyn Rehearse>,
) -> Result<CompileOutcome, crate::CompileError> {
    verify_document_with(request, candidate, selections, &[], cognition, host).await
}

/// [`verify_document`] for a revision that drops values a proposal the person saw bound: each
/// claim of `removals` is asked of the judge before the whole request (`verify-removed-<k>`,
/// recorded as `decision.removals`), and a claim it does not confirm keeps the document from
/// READY, never proposed, with why (a `removal` finding).
///
/// # Errors
/// None today: every refusal is an outcome; the `Result` matches the other compile entries.
pub async fn verify_document_with<P: ProviderInferDyn>(
    request: &CompileRequest,
    candidate: &str,
    selections: (&[Resolution], &[Resolution]),
    removals: &[Removal],
    cognition: Cognition<'_, P>,
    host: Option<&dyn Rehearse>,
) -> Result<CompileOutcome, crate::CompileError> {
    verify_presented(
        request, candidate, selections, removals, None, cognition, host,
    )
    .await
}

/// Verify a document with the host's still-open proposal choices as untrusted presentation
/// data. They are not human answers, runtime obligations or Save/Run authorization.
/// Existing document callers retain the same behavior without presentation data.
///
/// A proposal that carries a schedule beside its candidate presents it as `schedule`: the
/// person's line it cites (`message`), their words that say when the work runs (`excerpt`)
/// and the zone they follow (`zone`), as the host accepted them. The trigger is deployment,
/// never the candidate's bytes (#1720): a program obligation that only those words evidence
/// is carried beside the candidate and never asked of the judge.
///
/// # Errors
/// Every refusal is an outcome, as with [`verify_document`].
pub async fn verify_document_presented<P: ProviderInferDyn>(
    request: &CompileRequest,
    candidate: &str,
    selections: (&[Resolution], &[Resolution]),
    presentation: &Value,
    cognition: Cognition<'_, P>,
    host: Option<&dyn Rehearse>,
) -> Result<CompileOutcome, crate::CompileError> {
    verify_document_presented_with(
        request,
        candidate,
        selections,
        presentation,
        &[],
        cognition,
        host,
    )
    .await
}

/// [`verify_document_presented`] with the same removal claims as [`verify_document_with`].
///
/// # Errors
/// Every refusal is an outcome, as with [`verify_document_with`].
pub async fn verify_document_presented_with<P: ProviderInferDyn>(
    request: &CompileRequest,
    candidate: &str,
    selections: (&[Resolution], &[Resolution]),
    presentation: &Value,
    removals: &[Removal],
    cognition: Cognition<'_, P>,
    host: Option<&dyn Rehearse>,
) -> Result<CompileOutcome, crate::CompileError> {
    verify_presented(
        request,
        candidate,
        selections,
        removals,
        Some(presentation),
        cognition,
        host,
    )
    .await
}

async fn verify_presented<P: ProviderInferDyn>(
    request: &CompileRequest,
    candidate: &str,
    selections: (&[Resolution], &[Resolution]),
    removals: &[Removal],
    presentation: Option<&Value>,
    cognition: Cognition<'_, P>,
    host: Option<&dyn Rehearse>,
) -> Result<CompileOutcome, crate::CompileError> {
    let caller = match nika_compile::surface::semantic::caller(request) {
        Ok(caller) => caller,
        Err(refused) => return Ok(*refused),
    };
    let admitted::Money {
        reading,
        record,
        closed,
    } = match admitted::read(request) {
        Ok(money) => money,
        Err(refused) => return Ok(*refused),
    };
    let offered = cognition.provider.is_some() || cognition.seat.is_some();
    let seats = if closed.is_some() {
        Cognition::default()
    } else {
        cognition
    };
    let serves = rehearsal::Serves {
        caller,
        raw: request.clone(),
        reading: reading.clone(),
    };
    let mut rehearsals = rehearsal::Rehearsals::new(host).serving(serves);
    let words = (candidate, selections, removals, presentation);
    let mut out = judged(&reading, words, seats, &mut rehearsals).await;
    rehearsals.finish(&reading, &mut out).await;
    if let Some(money) = record {
        let closed = closed.as_deref().filter(|_| offered);
        admitted::record(request, money, closed, &mut out);
    }
    nika_compile::surface::observed::record(request, &mut out);
    forensic::record(request, offered, &mut out);
    Ok(out)
}

type Document<'a> = (
    &'a str,
    (&'a [Resolution], &'a [Resolution]),
    &'a [Removal],
    Option<&'a Value>,
);

fn presentation(out: &mut CompileOutcome, offered: Option<&Value>) -> bool {
    let Some(offered) = offered else {
        return true;
    };
    let (schedule, money) = (offered.get("schedule"), offered.get("money"));
    let keys = 3 + usize::from(schedule.is_some()) + usize::from(money.is_some());
    let valid = offered
        .as_object()
        .is_some_and(|object| object.len() == keys)
        && offered["kind"] == "proposal"
        && offered.get("selected").is_some_and(Value::is_null)
        && offered["pending_choices"].is_array()
        && schedule.is_none_or(scheduled)
        && money.is_none_or(ceiling);
    if !valid {
        crate::finding(
            out,
            DiagnosticKind::Refused,
            "presentation",
            "Proposal presentation must contain only kind, pending_choices, selected:null and, when the proposal carries them, its schedule (message, excerpt, zone) and its Run ceiling (excerpt, usd).",
        );
        out.status = CompileStatus::Incomplete;
        return false;
    }
    if let Some(decision) = out.provenance.decision.as_mut() {
        decision["presentation"] = offered.clone();
    }
    true
}

/// Whether a presented schedule holds exactly the cited line, the words that say when the work
/// runs (some) and the zone they follow, each as text.
fn scheduled(schedule: &Value) -> bool {
    let text = |key: &str| schedule[key].as_str();
    schedule.as_object().is_some_and(|object| object.len() == 3)
        && text("message").is_some()
        && text("zone").is_some()
        && text("excerpt").is_some_and(|words| !words.trim().is_empty())
}

/// Whether a presented Run ceiling holds exactly the words of the request that state it (some)
/// and its amount in USD, finite and not negative: the money binding the proposal carries beside
/// its bytes.
fn ceiling(money: &Value) -> bool {
    money.as_object().is_some_and(|object| object.len() == 2)
        && money["excerpt"]
            .as_str()
            .is_some_and(|words| !words.trim().is_empty())
        && money["usd"]
            .as_f64()
            .is_some_and(|usd| usd.is_finite() && usd >= 0.0)
}

/// The laws, the finish, the run and the verdict over one document, with the exit each leaves.
async fn judged<P: ProviderInferDyn>(
    reading: &CompileRequest,
    (candidate, selections, removals, offered): Document<'_>,
    seats: Cognition<'_, P>,
    rehearsals: &mut rehearsal::Rehearsals<'_>,
) -> CompileOutcome {
    let mut out = crate::initial();
    verify::route(&mut out, DOOR);
    if !presentation(&mut out, offered) {
        return out;
    }
    let Input::Create(words) = &reading.input else {
        crate::finding(&mut out, DiagnosticKind::Refused, REFUSED, NOT_CREATED);
        out.status = CompileStatus::Incomplete;
        return out;
    };
    let intent = lexicon::fold_apostrophes(words);
    let observed = reading.knowledge.as_ref();
    // The person's answers reach the laws within their scope: an answered reading is owed.
    let answers = &reading.answers;
    let refusals =
        super::native::document_refusals(&intent, candidate, selections, answers, observed);
    if !refusals.is_empty() {
        for refusal in refusals {
            crate::finding(&mut out, DiagnosticKind::Refused, REFUSED, refusal.message);
        }
        out.status = CompileStatus::Incomplete;
        return out;
    }
    nika_compile::surface::finish(candidate.to_owned(), &mut out);
    // A stated property no check of the bytes carries holds them before any judge or run.
    if out.status != CompileStatus::Ready || Witness::of(&intent, candidate).hold(&mut out) {
        return out;
    }
    let provider = (reading.authoring.as_ref())
        .filter(|policy| policy_bounded(policy))
        .zip(seats.provider);
    let judge = match (seats.seat, provider) {
        (Some(seat), _) => Judge::Seat(seat),
        (None, Some((policy, provider))) => Judge::Provider(policy, provider),
        // A run serves only a verdict: with no judge, the room runs nothing.
        (None, None) => return pending(&intent, reading, out),
    };
    if !removals_confirmed(&judge, removals, &intent, &mut out).await {
        return out;
    }
    // A carried terminal rejection is repeated before any new author or trial call. Carried
    // readings cannot grant an acceptance: every other candidate receives a fresh audit.
    if let Some((policy, provider)) = provider.filter(|_| seats.seat.is_some()) {
        let input = interpretation::input(&intent, reading);
        let earlier = interpretation::earlier(&input, policy, reading);
        if let Some(record) = earlier.as_ref()
            && let Some(rejected) =
                verify::repeat_document_rejection(&intent, reading, &judge, &out, record)
        {
            return rejected;
        }
        // A reading this process already audited for these very words is reused with no call.
        let record = match interpretation::audited(&input, policy, reading) {
            Some(record) => record,
            None => {
                interpretation::read(&input, policy, provider, earlier.as_ref(), &mut out).await
            }
        };
        let covered = interpretation::covered(&record);
        if let Some(decision) = out.provenance.decision.as_mut() {
            decision["request_interpretation"] = record;
        }
        // A transport, parse or audit failure cannot start a run or a decision call behind it.
        if !covered {
            return verify::uninterpreted(&intent, &judge, out);
        }
    }
    let run = rehearsals.trial(reading, &out).await;
    let read = lexicon::read(&intent);
    // Boxed where it is built, as the native door's verdict is: its state machine is large.
    let observation = run.as_ref();
    match Box::pin(verify::verdict_by(
        &intent,
        &read,
        judge,
        reading,
        out,
        0,
        observation,
    ))
    .await
    {
        Ok(ready) => ready,
        Err(judged) => {
            let (mut judged, mut verdict) = *judged;
            // Why no run of these bytes exists, when the room refused them (A4).
            let refused = (judged.candidate.as_deref()).and_then(|c| rehearsals.refused(c));
            if let Some(refused) = refused {
                verify::unobserved(&mut judged, &mut verdict, refused);
            }
            if verdict.defects.is_empty() {
                verify::preserve_unjudged(judged, &verdict)
            } else {
                verify::kept(judged, &verdict, 0)
            }
        }
    }
}

/// Qualify every removal before interpretation or trial; an unconfirmed claim holds the document.
async fn removals_confirmed<P: ProviderInferDyn>(
    judge: &Judge<'_, P>,
    removals: &[Removal],
    intent: &str,
    out: &mut CompileOutcome,
) -> bool {
    if removals.is_empty() {
        return true;
    }
    // Boxed where it is built: its questions are the verdict's, at the verdict's size.
    let claims = verify::removals::unconfirmed(judge, removals, intent, out);
    let held = Box::pin(claims).await;
    if held.is_empty() {
        return true;
    }
    for why in held {
        crate::finding(out, DiagnosticKind::Refused, REMOVAL, why);
    }
    out.status = CompileStatus::Incomplete;
    false
}

/// No judge was permitted: the whole request stays pending on these bytes, recorded as the core
/// keeps a native candidate its round has not judged, and nothing is READY.
fn pending(intent: &str, request: &CompileRequest, mut out: CompileOutcome) -> CompileOutcome {
    let candidate = out.candidate.clone().unwrap_or_default();
    let mut stated = lexicon::read(intent).plan;
    crate::shape::promote_stated_rules(&mut stated, intent);
    let bound = nika_compile::surface::Binding::of(intent, request, &stated, &candidate);
    let open = json!([{"clause": intent, "witness": Value::Null, "spans": [[0, intent.len()]]}]);
    let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
    decision["pending"] =
        json!({"candidate_sha256": bound.candidate, "plan_sha256": bound.plan, "open": open});
    out.provenance.decision = Some(decision);
    let target = "semantic_verification";
    crate::finding(&mut out, DiagnosticKind::Unknown, target, UNJUDGED);
    out.status = CompileStatus::Incomplete;
    out
}
