// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The document door's own laws (R5 · C13): what the author of a fresh CREATE makes of the
//! complete document, and the record that follows it. The author writes the whole `.nika`, or
//! states the operations of [`apply`](crate::foundry::document::apply) over its own text (its envelope at least) or over
//! the last document the door made: an admitted component is composed by identity, bound at its
//! holes and receipted, never retyped. The door that asks the author, judges, settles and
//! examines the document lives in `nika-compile-cognition`; this module only makes and records,
//! and calls no provider.
//!
//! The record keeps three facts apart: the bytes the author wrote (no preservation claimed), each
//! component an operation expanded (its receipt witnessed on the bytes, [`reuse`](crate::foundry::reuse)), and the
//! knowledge only shown (consulted, never reused). Reuse grants nothing: Check judges the
//! boundary the document states.
//!
//! One record follows a created document: the native record its answer rounds replay, with the
//! door's section beside it (`plan.document_create`). Once the document is READY,
//! [`bind`](crate::foundry::document::create::bind) binds
//! that record to the final bytes (answers baked, model seated, caps filled) with the request
//! they answer and each component's receipt (`plan.document`): what a later change in words
//! revises the document from.

use super::apply;
use crate::foundry::{ComponentCatalog, reuse};
use nika_compile::surface::sha256;
use nika_compile::{CompileOutcome, CompileStatus, Strategy};
use serde_json::{Value, json};

/// The route step of the document door.
pub const ROUTE: &str = "native: document";

const NO_DOCUMENT: &str = "the answer states no document: write the whole workflow in `candidate` (or `candidate_lines`), or operations over the last document";
const NO_BASE: &str = "operations need a document to apply to: write it in `candidate` (its envelope, `nika:` and `permits:`, at least, when components carry its tasks)";

/// What one answer made of the document, and how.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Made {
    /// The document the answer states.
    pub source: String,
    /// `written`: the author wrote the whole document; `composed`: operations applied over the
    /// author's own document (this round's, or the last one the door made).
    pub mode: &'static str,
    /// The digest of the document the operations applied to, when they did.
    pub base_sha256: Option<String>,
    /// How many operations the answer stated.
    pub operations: usize,
    /// The node paths and components the operations changed, in their order.
    pub changed: Vec<String>,
    /// The receipts of the components the document holds (new, rebound or carried).
    pub receipts: Vec<Value>,
    /// How many document edits the editor proved.
    pub verified: usize,
    /// How many component operations were made by construction.
    pub constructed: usize,
}

/// The document an answer states: its `written` text taken whole, or its `operations` applied in
/// order over that text, else over the `last` document the door made.
///
/// # Errors
/// Every reason the answer makes no document: none stated, operations with nothing to apply to,
/// or each refused operation as [`super::apply`] names it (nothing of a refused answer applies).
pub fn made(
    written: Option<String>,
    operations: &[Value],
    last: Option<&Made>,
    catalog: Option<&dyn ComponentCatalog>,
) -> Result<Made, Vec<String>> {
    let carried = last.map_or(&[][..], |last| &last.receipts[..]);
    if operations.is_empty() {
        let source = written.ok_or_else(|| vec![NO_DOCUMENT.to_owned()])?;
        return Ok(Made {
            source,
            mode: "written",
            base_sha256: None,
            operations: 0,
            changed: Vec::new(),
            receipts: carried.to_vec(),
            verified: 0,
            constructed: 0,
        });
    }
    let base = match (written, last) {
        (Some(text), _) => text,
        (None, Some(last)) => last.source.clone(),
        (None, None) => return Err(vec![NO_BASE.to_owned()]),
    };
    let applied = apply(&base, (operations, None), catalog, carried)?;
    Ok(Made {
        source: applied.source,
        mode: "composed",
        base_sha256: Some(sha256(&base)),
        operations: operations.len(),
        changed: applied.changed,
        receipts: applied.receipts,
        verified: applied.verified,
        constructed: applied.constructed,
    })
}

/// What the record claims of the document's bytes: what each step proved, never more.
fn preservation(made: &Made) -> &'static str {
    match (made.mode, made.verified, made.constructed) {
        ("written", _, _) => "none claimed: the author wrote the whole document",
        (_, _, 0) => {
            "edits verified: each re-read by the strict parser, every byte outside its span the author's document"
        }
        (_, 0, _) => {
            "by construction: each component's entries inserted into the author's document; not re-verified byte by byte"
        }
        _ => "edits verified byte by byte; each component's entries inserted by construction",
    }
}

/// The door's record of one settled attempt on `out`: `decision.document_create`, how the
/// document was made and each component it holds, every receipt witnessed on the outcome's
/// candidate (`expanded` only when its nodes are there), or on the door's document while a
/// question holds the candidate back; and, on the native record its answer rounds replay, the
/// door's section `intent` and the making are restated in, for [`bind`].
pub fn record(intent: &str, made: &Made, out: &mut CompileOutcome) {
    let candidate = out.candidate.as_deref();
    let witnessed = candidate.unwrap_or(&made.source);
    let entry = json!({
        "route": ROUTE,
        "mode": made.mode,
        "base_sha256": made.base_sha256,
        "operations": made.operations,
        "changed": made.changed,
        "preservation": preservation(made),
        "components": made.receipts,
        "reuse": reuse(&[], &made.receipts, Some(witnessed)),
        "candidate_sha256": candidate.map(sha256),
    });
    let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
    decision["document_create"] = entry;
    out.provenance.decision = Some(decision);
    let native = |plan: &&mut Value| plan["strategy"] == Strategy::Native.word();
    if let Some(plan) = out.provenance.plan.as_mut().filter(native) {
        plan["document_create"] = json!({
            "route": ROUTE,
            "mode": made.mode,
            "resolved": intent,
            "base_sha256": made.base_sha256,
            "operations": made.operations,
            "changed": made.changed,
            "preservation": preservation(made),
            "components": made.receipts,
        });
    }
}

/// At the compile's entry, once every door, replay and rehearsal returned: a READY outcome whose
/// native record the document door produced is bound to its final bytes (answers baked, model
/// seated, caps filled). `plan.document` names their digest, the request they answer, no base,
/// how they were made and each component's receipt; the native record stays the one its answer
/// rounds replay (an optional question may still be answered). The door's decision is restated
/// on the same bytes, their digest and each receipt witnessed on them; an answer round, which
/// replays the record and never runs the door, restates it from the record's section. With a
/// mandatory question open there are no final bytes yet and nothing is bound. Any other outcome
/// is unchanged.
pub fn bind(out: &mut CompileOutcome) {
    if out.status != CompileStatus::Ready {
        return;
    }
    let Some(candidate) = out.candidate.clone() else {
        return;
    };
    let digest = sha256(&candidate);
    let created = |plan: &&mut Value| {
        plan["strategy"] == Strategy::Native.word() && plan.get("document_create").is_some()
    };
    let Some(plan) = out.provenance.plan.as_mut().filter(created) else {
        return;
    };
    let section = plan["document_create"].clone();
    let receipts = section["components"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    plan["document"] = json!({
        "version": 1,
        "candidate_sha256": digest,
        "request": section["resolved"],
        "base_sha256": null,
        "mode": section["mode"],
        "components": receipts,
    });
    let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
    let mut entry = match decision.get("document_create") {
        Some(entry) if entry.is_object() => entry.clone(),
        _ => json!({
            "route": section["route"],
            "mode": section["mode"],
            "base_sha256": section["base_sha256"],
            "operations": section["operations"],
            "changed": section["changed"],
            "preservation": section["preservation"],
            "components": receipts,
        }),
    };
    entry["candidate_sha256"] = json!(digest);
    entry["reuse"] = reuse(&[], &receipts, Some(&candidate));
    decision["document_create"] = entry;
    out.provenance.decision = Some(decision);
}

/// The expansion receipts the document door's outcome holds: what a qualification record's
/// reuse is witnessed from. Empty for any other door.
#[must_use]
pub fn receipts(out: &CompileOutcome) -> Vec<Value> {
    (out.provenance.decision.as_ref())
        .and_then(|decision| decision["document_create"]["components"].as_array())
        .cloned()
        .unwrap_or_default()
}

/// The language's complete schema as the author reads it: the Spec's workflow schema, compact.
#[must_use]
pub fn language() -> String {
    let text = nika_pack::schema_json();
    serde_json::from_str::<Value>(text)
        .map_or_else(|_| text.to_owned(), |schema| schema.to_string())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests;
