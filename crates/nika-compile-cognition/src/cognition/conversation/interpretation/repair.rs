// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Repair the interpretation, never the request or program. Progress is diagnostic evidence,
//! not a turn quota; transport, interruption and repeated feedback cannot trigger more work.

use std::collections::BTreeSet;

use nika_kernel::ai::provider::ProviderInferDyn;
use serde_json::{Value, json};

use super::{attempt, covered, diagnostics, journal, knowledge};
use crate::{AuthoringPolicy, CompileOutcome};

pub(super) async fn read<P: ProviderInferDyn>(
    input: &Value,
    policy: &AuthoringPolicy,
    provider: &P,
    mut proposed: Option<&Value>,
    out: &mut CompileOutcome,
) -> Value {
    let before = journal(out).len();
    let mut attempts = Vec::new();
    let mut seen = BTreeSet::new();
    let mut state = None;
    let mut record = loop {
        let mut record = attempt(input, policy, provider, proposed, state.as_ref(), out).await;
        proposed = None;
        let feedback = feedback(input, &mut record);
        attempts.push(record.clone());
        let Some(feedback) = feedback else {
            break record;
        };
        let progress = progress(&feedback);
        if !seen.insert(progress.clone()) {
            record["repair_stop"] =
                json!({"reason":"repeated_feedback","progress_sha256":progress});
            break record;
        }
        state = Some(
            json!({"source":input,"previous_interpretation":record["reading"],
            "repair_feedback":feedback}),
        );
    };
    record["attempts"] = json!(attempts);
    record["calls"] = json!(&journal(out)[before.min(journal(out).len())..]);
    record
}

fn feedback(input: &Value, record: &mut Value) -> Option<Value> {
    if covered(record) {
        return None;
    }
    if let Some(failure) = record.get("failure") {
        // A response or transport problem is not evidence of a wrong interpretation.
        // Unresolved meaning may be repaired only from the same sources; otherwise the
        // repeated unresolved location ends the cycle without a judgment or trial.
        return (failure["stage"] == "interpret_request"
            && matches!(
                failure["kind"].as_str(),
                Some("invalid_structure" | "evidence" | "unresolved")
            ))
        .then(|| failure.clone());
    }
    if record["audit"]["status"] != "defect" {
        return None;
    }
    let defects = record["audit"]["defects"].as_array()?;
    let spans = input["spans"].as_array()?;
    for (index, defect) in defects.iter().enumerate() {
        let evidence = defect["evidence"].as_str().unwrap_or_default();
        if evidence.is_empty()
            || !spans.iter().any(|span| {
                span["text"]
                    .as_str()
                    .is_some_and(|text| text.contains(evidence))
            })
        {
            record["failure"] = diagnostics::failure(
                "audit_interpretation",
                "evidence",
                &format!("audit.defects[{index}].evidence"),
                "The audit evidence is not an exact nonempty quote from a preserved source span; it cannot justify reading repair.",
            );
            return None;
        }
    }
    Some(record["audit"].clone())
}

/// Changing model-written notes, requirement IDs, ordering or duplicate defects is not progress.
/// Exact source evidence and defect kinds bind a semantic problem; structural diagnostics
/// already name their stable failing location. Repeated feedback can only hold, never admit.
fn progress(feedback: &Value) -> String {
    let identity = if let Some(defects) = feedback["defects"].as_array() {
        let mut defects: Vec<_> = defects
            .iter()
            .map(|defect| {
                json!([
                    defect["kind"],
                    defect["evidence"]
                        .as_str()
                        .unwrap_or_default()
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                ])
                .to_string()
            })
            .collect();
        defects.sort();
        defects.dedup();
        json!(defects)
    } else {
        json!([feedback["stage"], feedback["kind"], feedback["path"]])
    };
    knowledge::sha256(&identity.to_string())
}
