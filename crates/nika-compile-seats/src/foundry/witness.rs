// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The reuse witness (C13): what a candidate holds of an expansion, re-derived from the
//! candidate's own bytes — never from the knowledge an author was shown, never from lines the
//! candidate shares with a reference. A receipt names the digest of every node its expansion
//! produced and every bound literal; the witness parses the candidate and compares. A candidate
//! that dropped the expansion is `absent` even when the component's text still sits in the
//! author's context or inside one of the candidate's prompts. A shown reference no receipt names
//! is `consulted`: shown, its use unobservable here.
//!
//! A revision of a bound value ([`revise`]) is the same bounded literal edit on the candidate,
//! judged by the component's own binding laws, with the receipt carried to the new bytes.

use nika_compile::KnowledgeReference;
use nika_compile::surface::{literal_projection, sha256};
use nika_compile_fidelity::literal::literal_at;
use serde_json::{Map, Value, json};

use super::bind::{Binding, BindingError, edit_literal, judge};
use super::component::Component;
use super::instance::{MERGED, nodes};

/// The law the reuse record states.
pub const REUSE: &str = "reuse: a component is expanded when every node its receipt names is re-derived, digest for digest, from the candidate's own bytes, and every bound literal holds; a shown reference no receipt names is consulted; lexical overlap never decides";

/// What one candidate holds of one expansion receipt: `expanded` (every node and bound literal as
/// receipted), `revised` (every node present, some changed since), `absent` (a node missing), or
/// `unreadable` (the candidate's literals cannot be read).
#[must_use]
pub fn witness(receipt: &Value, candidate: &str) -> Value {
    let id = receipt["component"]["id"].clone();
    let Some(mut document) = literal_projection(candidate) else {
        return json!({"component": id, "verdict": "unreadable"});
    };
    let (mut kept, mut changed, mut missing) = (Vec::new(), Vec::new(), Vec::new());
    for section in MERGED {
        let Some(entries) = receipt["nodes"][section].as_object() else {
            continue;
        };
        for (name, digest) in entries {
            let path = format!("{section}.{name}");
            match document.get(section).and_then(|s| s.get(name)) {
                None => missing.push(path),
                Some(node) if json!(sha256(&node.to_string())) == *digest => kept.push(path),
                Some(_) => changed.push(path),
            }
        }
    }
    let mut unbound = Vec::new();
    for binding in receipt["bindings"].as_array().into_iter().flatten() {
        let path = binding["path"].as_str().unwrap_or_default();
        if literal_at(&mut document, path).is_none_or(|held| *held != binding["bound"]) {
            unbound.push(path.to_owned());
        }
    }
    let verdict = if !missing.is_empty() || kept.is_empty() && changed.is_empty() {
        "absent"
    } else if changed.is_empty() && unbound.is_empty() {
        "expanded"
    } else {
        "revised"
    };
    json!({
        "component": id,
        "release": receipt["component"]["release"],
        "verdict": verdict,
        "candidate_sha256": sha256(candidate),
        "receipt_candidate_sha256": receipt["candidate_sha256"],
        "nodes": {"kept": kept, "changed": changed, "missing": missing},
        "bindings_not_held": unbound,
    })
}

/// The reuse record of a candidate: each shown reference, `consulted` unless an expansion receipt
/// names it, and each receipt's witness ([`witness`]). The counts say how many components the
/// candidate's bytes actually hold.
#[must_use]
pub fn reuse(shown: &[KnowledgeReference], receipts: &[Value], candidate: Option<&str>) -> Value {
    let witnessed: Vec<Value> = (receipts.iter())
        .map(|receipt| {
            candidate.map_or_else(
                || json!({"component": receipt["component"]["id"], "verdict": "absent"}),
                |c| witness(receipt, c),
            )
        })
        .collect();
    let mut rows: Vec<Value> = (shown.iter())
        .filter(|r| !witnessed.iter().any(|w| w["component"] == r.id.as_str()))
        .map(|r| json!({"id": r.id, "kind": r.kind, "use": "consulted"}))
        .collect();
    rows.extend(witnessed.iter().map(
        |w| json!({"id": w["component"], "kind": "block", "use": w["verdict"], "witness": w}),
    ));
    let count = |word: &str| rows.iter().filter(|r| r["use"] == word).count();
    json!({
        "law": REUSE,
        "expanded": count("expanded"),
        "revised": count("revised"),
        "absent": count("absent"),
        "consulted": count("consulted"),
        "references": rows,
    })
}

/// Read the reuse a knowledge-qualification record states, new or legacy. A record written
/// before the reuse witness carries only a lexical `trace`, whose `instantiated` and `adapted`
/// counted shared lines: it is read as consultation with its lexical counts kept as data, never
/// as reuse.
#[must_use]
pub fn reuse_of(record: &Value) -> Value {
    if let Some(reuse) = record.get("reuse") {
        return reuse.clone();
    }
    match record.get("trace") {
        Some(trace) => json!({
            "law": "legacy: a lexical trace of shared lines, never evidence of reuse",
            "expanded": 0,
            "consulted": trace["references"].as_array().map_or(0, Vec::len),
            "legacy_lexical_trace": trace,
        }),
        None => Value::Null,
    }
}

/// A revision of bound values on an expanded candidate: each change judged by the component's
/// binding laws, applied by a bounded literal edit on the candidate, every other byte kept; the
/// receipt carried to the new bytes (its bindings, node digests and candidate digest, and the
/// digest it revises). The receipt must hold on `candidate` first: a stale receipt revises
/// nothing.
///
/// # Errors
/// [`BindingError`]: a stale receipt (as `Unproven`), a refused binding, or an unproven edit.
pub fn revise(
    candidate: &str,
    receipt: &Value,
    component: &Component,
    changes: &[Binding],
) -> Result<(String, Value), BindingError> {
    let stale = || {
        BindingError::Unproven(super::bind::EditRefusal::Unlocated(
            "the receipt does not hold on this candidate".to_owned(),
        ))
    };
    if witness(receipt, candidate)["verdict"] != "expanded"
        || receipt["component"]["file_sha256"] != json!(component.file_sha256)
    {
        return Err(stale());
    }
    judge(component, changes)?;
    let mut revised = candidate.to_owned();
    for change in changes {
        if !(receipt["bindings"].as_array().into_iter().flatten())
            .any(|b| b["path"] == change.path.as_str())
        {
            return Err(BindingError::UnknownHole(change.path.clone()));
        }
        revised =
            edit_literal(&revised, &change.path, &change.value).map_err(BindingError::Unproven)?;
    }
    let mut carried = receipt.clone();
    for row in carried["bindings"].as_array_mut().into_iter().flatten() {
        if let Some(change) = changes.iter().find(|c| row["path"] == c.path.as_str()) {
            row["bound"] = change.value.clone();
        }
    }
    let document = literal_projection(&revised).ok_or_else(stale)?;
    let produced: Map<String, Value> = (MERGED.iter())
        .filter_map(|section| {
            let names = receipt["nodes"][*section].as_object()?;
            let entries: Map<String, Value> = names
                .keys()
                .map(|name| (name.clone(), Value::Null))
                .collect();
            Some(((*section).to_owned(), Value::Object(entries)))
        })
        .collect();
    carried["nodes"] = nodes(&document, &Value::Object(produced));
    carried["revises"] = receipt["candidate_sha256"].clone();
    carried["candidate_sha256"] = json!(sha256(&revised));
    let (_, checked, needed) = super::instance::check(&revised);
    carried["check"] = checked;
    carried["authority"]["document_needs"] = needed;
    Ok((revised, carried))
}
