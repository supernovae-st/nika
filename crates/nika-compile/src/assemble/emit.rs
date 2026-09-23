// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The assembler's emission: the permits derived from what the tasks reach, the fidelity laws
//! over the emitted document, the literal round trip. Beside `assemble.rs` at the 1,500-line
//! file cap; the assembler re-exports it.
use std::collections::BTreeMap;

use serde_json::json;

use super::Doc;
use crate::plan::Plan;
use crate::{CompileError, CompileOutcome, DiagnosticKind, QuestionType};

/// The question a fidelity refusal asks: a request no door could realize as stated.
const FIDELITY_QUESTION: &str = "Supply a complete replacement request that names what to read and where to write. It explicitly replaces the earlier intent.";

/// What the fidelity laws hold every emitted candidate to: the request, its plan, the
/// values the human answered (never invented literals).
pub(crate) struct Laws<'a> {
    pub(crate) intent: &'a str,
    pub(crate) plan: &'a Plan,
    pub(crate) answers: &'a BTreeMap<String, String>,
}

/// The permits are exactly what the tasks reach: the tools invoked, the paths read and
/// written, the hosts fetched — derived, never written by hand.
fn emit_permits(d: &mut Doc) {
    d.root["permits"]["tools"] = json!(d.tools.iter().copied().collect::<Vec<_>>());
    if !d.reads.is_empty() || !d.writes.is_empty() {
        let mut fs = json!({});
        if !d.reads.is_empty() {
            fs["read"] = json!(d.reads);
        }
        if !d.writes.is_empty() {
            fs["write"] = json!(d.writes);
        }
        d.root["permits"]["fs"] = fs;
    }
    if !d.hosts.is_empty() {
        d.root["permits"]["net"] = json!({"http": d.hosts});
    }
}

pub(crate) fn emit(
    mut d: Doc,
    laws: &Laws<'_>,
    out: &mut CompileOutcome,
) -> Result<(), CompileError> {
    emit_permits(&mut d);
    for key in ["const", "inputs"] {
        if d.root[key]
            .as_object()
            .is_some_and(serde_json::Map::is_empty)
            && let Some(map) = d.root.as_object_mut()
        {
            map.remove(key);
        }
    }
    if d.root["outputs"]
        .as_object()
        .is_some_and(serde_json::Map::is_empty)
    {
        if let Some(fact) = d.facts.last() {
            d.root["outputs"][fact.name] = json!(fact.template);
        } else if d.item {
            d.root["outputs"]["item"] = json!("${{ inputs.item }}");
        }
    }
    // READY is against the request, not only against the Check: the fidelity laws refuse a
    // candidate that drops a stated path, skips a stated approval, performs a prohibited
    // effect or invents a path or host, at every door (recorded as `decision.fidelity`). A
    // candidate this compile refused earlier (a door tried before this one) is superseded by
    // this emission: its refusal and its question go, this candidate is judged afresh.
    out.diagnostics.retain(|d| d.target != "fidelity");
    out.questions.retain(|q| q.label != FIDELITY_QUESTION);
    let waived: Vec<String> = out
        .diagnostics
        .iter()
        .filter(|d| {
            d.kind == DiagnosticKind::Applied
                && d.message.ends_with("is not written, by explicit answer.")
        })
        .filter_map(|d| d.message.split('`').nth(1).map(str::to_owned))
        .collect();
    let mut refusals = Vec::new();
    crate::fidelity::laws(
        laws.intent,
        laws.plan,
        &d.root,
        &crate::fidelity::allowed_values(laws.answers),
        &waived,
        &mut refusals,
    );
    refusals.dedup();
    if !refusals.is_empty() {
        for refusal in &refusals {
            crate::finding(
                out,
                DiagnosticKind::Refused,
                "fidelity",
                format!(
                    "The assembled candidate fails the request: {}",
                    refusal.message
                ),
            );
        }
        let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
        decision["fidelity"] = json!({
            "refused": true,
            "diagnostics": refusals.iter().map(|r| json!({"kind": r.kind, "message": r.message})).collect::<Vec<_>>(),
        });
        out.provenance.decision = Some(decision);
        crate::question(
            out,
            "intent.clarification",
            FIDELITY_QUESTION,
            QuestionType::Text,
        );
        return Ok(());
    }
    let source = serde_yaml_bw::to_string(&d.root).map_err(CompileError::representation)?;
    if crate::edit::literal_projection(&source).as_ref() != Some(&d.root) {
        crate::finding(
            out,
            DiagnosticKind::Refused,
            "candidate",
            "The emitted candidate did not preserve literal data.",
        );
        out.status = crate::CompileStatus::Refused;
        return Ok(());
    }
    crate::finish(source, out);
    Ok(())
}
