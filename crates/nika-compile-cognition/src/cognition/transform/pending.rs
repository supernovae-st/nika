// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! An answered field resumes authoring through the existing bounded call and verifier.
use super::{
    AuthoringPolicy, CompileOutcome, DiagnosticKind, PendingTransform, ProposedTransform,
    ProviderInferDyn, Refusal,
};
use crate::{CompileError, CompileRequest, CompileStatus};
use serde_json::json;

pub(crate) async fn resume<P: ProviderInferDyn>(
    intent: &str,
    request: &CompileRequest,
    policy: &AuthoringPolicy,
    provider: &P,
    mut out: CompileOutcome,
) -> Result<CompileOutcome, CompileError> {
    let Some(record) = out.provenance.plan.clone() else {
        return Ok(out);
    };
    let Ok((mut pending, mut plan)) = PendingTransform::load(intent, &record, request) else {
        return Ok(out);
    };
    if !out.questions.is_empty() || !pending.begin_attempt() {
        return Ok(out);
    }
    // These are the deterministic pending requirement, now being fulfilled by this call.
    out.diagnostics
        .retain(|d| d.target != "authoring_transform");
    let answer = super::propose(policy, provider, pending.context(), &mut out).await;
    let program = answer.as_ref().map(|proposed| proposed.jq.clone()).ok();
    // The clause's own words bind the observed spellings its program must honor (R4 A11).
    let context = pending.context();
    let evidence = context["clause"].as_str().unwrap_or_default();
    let world = nika_compile::surface::observed::world(request);
    let spelled = super::spelling::Spelled::of(world, intent, evidence);
    let verdict = answer.and_then(|proposed| regenerated(intent, &pending, &spelled, proposed));
    // The same repair as the first synthesis, within this request's allowance (R4 A11).
    let mut repairs = super::domain::Repairs::granted(policy, &out);
    let verdict = match (verdict, program) {
        (Err(why), Some(jq)) => {
            let (state, clause) = (pending.context(), pending.detail().to_owned());
            repairs
                .repair(policy, provider, &state, &clause, (jq, why), &mut out)
                .await
                .and_then(|repaired| regenerated(intent, &pending, &spelled, repaired))
        }
        (verdict, _) => verdict,
    };
    match verdict {
        Ok(proposed) => {
            // What the spelling law could not judge rides the record and the judges' state.
            let unjudged = spelled.qualify(&proposed, &mut out);
            let rule = crate::rules::Rule::program(
                pending.detail(),
                proposed.jq.trim(),
                proposed.columns_read,
            );
            plan.rules.push(rule.clone());
            let mut verified = pending.verified_record(&plan, &rule);
            if let Some(world) = nika_compile::surface::observed::world(request) {
                verified["observed_world"] = world.clone();
            }
            let mut assembly_request = request.clone();
            assembly_request.plan = Some(verified.clone());
            // Assembly consumes the verified field receipt; it never executes the workflow.
            out.provenance.plan = None;
            // The first candidate of the seat's plan (its first round suspended before any):
            // the whole request is judged with the remainder, by the authoring provider
            // through the journaled call (R4 A11).
            let judges = (None, Some((policy, provider)));
            let verify = super::super::verify::replayed;
            out = verify(intent, &verified, &assembly_request, judges, true, out).await?;
            let regeneration = kept_or_refused(&mut out, &verified);
            let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
            decision["transform_regeneration"] = regeneration;
            if !unjudged.is_empty() {
                decision["transform_regeneration"]["unjudged"] = json!(unjudged);
            }
            out.provenance.decision = Some(decision);
        }
        Err(Refusal(why)) => {
            crate::finding(
                &mut out,
                DiagnosticKind::Unknown,
                "authoring_transform",
                format!("Program regeneration remains pending: {why}."),
            );
            pending.suspend(&plan, &mut out);
            let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
            decision["transform_regeneration"] = json!({"accepted":false,"why":why});
            out.provenance.decision = Some(decision);
        }
    }
    Ok(out)
}

/// The laws a regenerated program is held to: it reads only the observed fields, it uses the
/// answered ones, and it passes every law of a computation clause's program.
fn regenerated(
    intent: &str,
    pending: &PendingTransform,
    spelled: &super::spelling::Spelled,
    proposed: ProposedTransform,
) -> Result<ProposedTransform, Refusal> {
    if proposed
        .columns_read
        .iter()
        .any(|c| !pending.columns().contains(c))
    {
        return Err(Refusal(
            "the regenerated program reads an unobserved field".into(),
        ));
    }
    if !pending.uses_answers(&proposed.columns_read) {
        return Err(Refusal(
            "the regenerated program does not use the answered fields".into(),
        ));
    }
    let (detail, columns) = (pending.detail(), pending.columns());
    super::verified(intent, detail, columns, spelled, &proposed).map(|()| proposed)
}

/// The regeneration's verdict once the replay of its verified record ran: the program is
/// accepted only when the replay kept that record. A replay that refused it keeps its own
/// findings; nothing claims the program, restores the record, or pretends the spent call away.
fn kept_or_refused(out: &mut CompileOutcome, verified: &serde_json::Value) -> serde_json::Value {
    let refused = out
        .diagnostics
        .iter()
        .find(|d| matches!(d.target.as_str(), "pending_transform" | "recorded_plan"))
        .map(|d| d.message.clone())
        .or_else(|| {
            let kept = out.provenance.plan.is_some();
            (!kept).then(|| "the replay kept no verified record".to_owned())
        });
    match refused {
        None => {
            if let Some(record) = out.provenance.plan.as_mut() {
                record["verified_transform"] = verified["verified_transform"].clone();
            }
            crate::finding(
                out,
                DiagnosticKind::Applied,
                "authoring_transform",
                "The field answer regenerated a program through one bounded provider call and the existing transform verifier.",
            );
            json!({"accepted":true})
        }
        Some(why) => {
            out.status = CompileStatus::Incomplete;
            out.candidate = None;
            out.check_preview = None;
            out.provenance.plan = None;
            crate::finding(
                out,
                DiagnosticKind::Unknown,
                "authoring_transform",
                format!(
                    "The regenerated program was verified, but its replay refused it ({why}); the provider call is spent. Compile the request afresh."
                ),
            );
            json!({"accepted":false,"why":why})
        }
    }
}
