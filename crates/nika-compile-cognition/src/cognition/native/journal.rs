// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The journal of a native conversation: the cold round's report, one entry per judged round,
//! the record of the whole conversation and the line a human reads. Data, never authority.

use super::{Answer, Cold, Talk, knowledge};
use crate::fidelity::Diagnostic;
use crate::{CompileOutcome, CompileRequest, CompileStatus, DiagnosticKind};
use serde_json::{Value, json};

/// The status and the open questions of the cold round, kept in the record as data.
pub(super) fn cold_report(out: &CompileOutcome) -> Value {
    let status = match out.status {
        CompileStatus::Ready => "ready",
        CompileStatus::Incomplete => "incomplete",
        CompileStatus::Refused => "refused",
        _ => "other",
    };
    json!({
        "status": status,
        "questions": out.questions.iter().map(|q| q.key.clone()).collect::<Vec<_>>(),
        "diagnostics": out.diagnostics.iter().filter(|d| d.kind != DiagnosticKind::Applied).map(|d| d.message.clone()).collect::<Vec<_>>(),
    })
}

/// One judged round: the candidate by digest and in full, what the seat asked and left out, the
/// judge's diagnostics, and both texts of a dual answer apart from the judged source.
pub(super) fn judged(round: u32, answer: &Answer, diagnostics: &[Diagnostic]) -> Value {
    let mut entry = json!({
        "round": round,
        "candidate_sha256": knowledge::sha256(&answer.candidate),
        "candidate": answer.candidate,
        "questions": answer.questions.iter().map(|q| q.key.clone()).collect::<Vec<_>>(),
        "gaps": answer.gaps.clone(),
        "notes": answer.notes.clone(),
        "diagnostics": diagnostics.iter().map(|d| json!({"kind": d.kind, "message": d.message})).collect::<Vec<_>>(),
    });
    if let Some(dual) = &answer.dual {
        entry["transport"] = dual.record();
    }
    entry
}

/// The native record of a conversation: the identity, the knowledge pack, the references sent,
/// every round, whether a candidate was accepted (else the last refused text), the revision.
pub(in crate::cognition) fn record(
    out: &mut CompileOutcome,
    request: &CompileRequest,
    cold: &Cold,
    talk: &Talk,
    sent: &[Value],
    accepted: Option<&Answer>,
    revision: Option<(&str, &str)>,
) {
    let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
    decision["cold"] = cold.report.clone();
    decision["native"] = json!({
        "identity": knowledge::identity(),
        "knowledge": request.authoring_knowledge.as_ref().map(|pack| json!({
            "identity": pack.identity,
            "selection": pack.selection,
        })),
        "references": sent,
        "rounds": talk.rounds.clone(),
        "accepted": accepted.is_some(),
        "refused_source": accepted.is_none().then(|| talk.refused.clone()).flatten(),
        "revision": revision.map(|(source, words)| json!({
            "base_sha256": knowledge::sha256(source),
            "change": words,
            "delta": accepted.and_then(|answer| {
                let base = crate::edit::literal_projection(source)?;
                let revised = crate::edit::literal_projection(&answer.candidate)?;
                Some(nika_compile_fidelity::candidate::delta(&base, &revised))
            }),
        })),
    });
    out.provenance.decision = Some(decision);
}

/// One line a human reads: what each round of the conversation decided and why.
pub(super) fn line(rounds: &[Value]) -> String {
    let mut parts = Vec::new();
    for round in rounds {
        let n = round["round"].as_u64().unwrap_or_default();
        if let Some(call) = round["call"].as_str() {
            parts.push(format!("round {n}: the call {call}"));
        } else if let Some(answer) = round["answer"].as_str() {
            parts.push(format!("round {n}: {answer}"));
        } else {
            let diagnostics = round["diagnostics"].as_array().cloned().unwrap_or_default();
            let asked = round["asked"].as_array().map(Vec::len);
            if let (true, Some(questions)) = (diagnostics.is_empty(), asked) {
                parts.push(format!(
                    "round {n}: asked {questions} question(s), no candidate"
                ));
            } else if diagnostics.is_empty() {
                parts.push(format!("round {n}: accepted"));
            } else {
                let heads: Vec<String> = diagnostics
                    .iter()
                    .take(3)
                    .map(|d| {
                        d["message"]
                            .as_str()
                            .unwrap_or_default()
                            .chars()
                            .take(90)
                            .collect()
                    })
                    .collect();
                parts.push(format!(
                    "round {n}: refused ({} diagnostic(s): {})",
                    diagnostics.len(),
                    heads.join(" · ")
                ));
            }
        }
    }
    let judged = rounds
        .iter()
        .filter(|r| r.get("candidate_sha256").is_some() || r.get("sketch_sha256").is_some())
        .count();
    format!(
        "The authoring conversation recorded {} round(s), including {judged} candidate or sketch judgment(s): {}.",
        rounds.len(),
        parts.join("; ")
    )
}
