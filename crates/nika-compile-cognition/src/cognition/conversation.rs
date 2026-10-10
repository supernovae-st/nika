// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A document a Session's conversation wrote, verified before anything is proposed (R4 A11). No
//! author is called: the bytes face what a native door's own candidate faces. First the laws,
//! over the world the host observed for the request (`CompileRequest::knowledge`, the
//! observation a compile round reads): the strict parser, the pure Check, each selection on the
//! person's own words, the fidelity laws. Then the core's finish, one run of the READY bytes when
//! the host lends a room, and the whole-request verdict of the judge the caller permits (its
//! decision seat, else its bounded authoring provider), carrying the verdicts its conversation
//! kept (`CompileRequest::declined`, R6). The outcome is READY only on that verdict. Otherwise it
//! says why, as the document door ends a candidate: a doubt no defect located is held, located
//! defects keep the bytes as the preview (what the author repairs from), a verdict that got no
//! answer leaves them unjudged, and with no judge permitted the whole request stays pending.

use super::verify::{self, Judge};
use super::{Cognition, admitted, forensic, policy_bounded, rehearsal};
use crate::fidelity::resolution::Resolution;
use crate::rehearse::Rehearse;
use crate::types::Input;
use crate::{CompileOutcome, CompileRequest, CompileStatus, DiagnosticKind, lexicon};
pub use nika_compile_seats::judge::Removal;
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

/// Verify a document a conversation wrote for `request` exactly as a native door verifies its
/// own candidate, and settle it READY only on the whole-request verdict.
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
    let words = (candidate, selections, removals);
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

/// The laws, the finish, the run and the verdict over one document, with the exit each leaves.
async fn judged<P: ProviderInferDyn>(
    reading: &CompileRequest,
    (candidate, selections, removals): (&str, (&[Resolution], &[Resolution]), &[Removal]),
    seats: Cognition<'_, P>,
    rehearsals: &mut rehearsal::Rehearsals<'_>,
) -> CompileOutcome {
    let mut out = crate::initial();
    verify::route(&mut out, DOOR);
    let Input::Create(words) = &reading.input else {
        crate::finding(&mut out, DiagnosticKind::Refused, REFUSED, NOT_CREATED);
        out.status = CompileStatus::Incomplete;
        return out;
    };
    let intent = lexicon::fold_apostrophes(words);
    let observed = reading.knowledge.as_ref();
    let refusals = super::native::document_refusals(&intent, candidate, selections, observed);
    if !refusals.is_empty() {
        for refusal in refusals {
            crate::finding(&mut out, DiagnosticKind::Refused, REFUSED, refusal.message);
        }
        out.status = CompileStatus::Incomplete;
        return out;
    }
    nika_compile::surface::finish(candidate.to_owned(), &mut out);
    if out.status != CompileStatus::Ready {
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
    if !removals.is_empty() {
        // Boxed where it is built: its questions are the verdict's, at the verdict's size.
        let claims = verify::removals::unconfirmed(&judge, removals, &intent, &mut out);
        let held = Box::pin(claims).await;
        if !held.is_empty() {
            for why in held {
                crate::finding(&mut out, DiagnosticKind::Refused, REMOVAL, why);
            }
            out.status = CompileStatus::Incomplete;
            return out;
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
