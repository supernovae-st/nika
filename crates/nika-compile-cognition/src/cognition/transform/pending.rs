// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! An answered field resumes authoring through the existing bounded call and verifier.
use super::{
    AuthoringPolicy, CompileOutcome, DiagnosticKind, PendingTransform, ProviderInferDyn, Refusal,
};
use crate::{CompileError, CompileRequest};
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
    let verdict = super::propose(policy, provider, pending.context(), &mut out)
        .await
        .and_then(|proposed| {
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
            super::verify(intent, pending.columns(), &proposed).map(|()| proposed)
        });
    match verdict {
        Ok(proposed) => {
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
            crate::replay(intent, &verified, &assembly_request, &mut out)?;
            if let Some(record) = out.provenance.plan.as_mut() {
                record["verified_transform"] = verified["verified_transform"].clone();
            }
            crate::finding(
                &mut out,
                DiagnosticKind::Applied,
                "authoring_transform",
                "The field answer regenerated a program through one bounded provider call and the existing transform verifier.",
            );
            let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
            decision["transform_regeneration"] = json!({"accepted":true});
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
