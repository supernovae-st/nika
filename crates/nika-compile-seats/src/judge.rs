// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The untrusted state a judging seat reads (R4 A11): what every question of a verdict and its
//! repair show the judge, apart from the compiler-owned reference they also carry. The verifier
//! of `nika-compile-cognition` asks the questions and weighs the answers; this state moved here
//! at that crate's size cap (2026-10-08). The engine facts a request may condition on (the
//! catalogue a door was lent, what it composed) ride the same state as data ([`authoring`]).

use nika_compile::surface::{EditChange, Input};
use nika_compile::{CompileOutcome, CompileRequest};
use serde_json::{Value, json};

use crate::foundry::ComponentCatalog;
use crate::foundry::witness::witness;

/// The state every question and every repair carries: the request as compiled and as first
/// stated, its answers, the observed world and the candidate's bytes. The first statement is
/// the one the binding holds ([`Binding::of`](nika_compile::surface::Binding::of)): the
/// preserved original request, else the submitted text the compiled request was folded or
/// clarified from. A revision in words is judged on the request it resolves (`intent`); the
/// request of the base it revises, which the change partly supersedes, is never shown as its
/// first statement: it stays apart as history (`revision.base_request`) beside the change as
/// the human stated it (`revision.change`).
#[must_use]
pub fn state(intent: &str, request: &CompileRequest, candidate: &str) -> Value {
    let (submitted, change) = match &request.input {
        Input::Create(text) if text != intent => (Some(text.clone()), None),
        Input::Edit {
            change: EditChange::Text(words),
            ..
        } => (None, Some(words)),
        _ => (None, None),
    };
    let first = request.original_intent.clone().or(submitted);
    // A revision judged on the earlier request with the change appended (`… Change: …`) is
    // told so: its replaced clauses are still in the words, superseded by the change.
    let appended = request.original_intent.is_some()
        && nika_compile::revise_intent(request).is_some_and(|resolved| resolved == intent);
    let unknown = first.is_none();
    let (first, revision) = match change {
        Some(change) => {
            let mut revision = json!({"change": change, "base_request": first});
            if appended {
                revision["appended"] = json!(true);
            }
            // A base whose request is unknown is shown whole: the change is judged over it.
            if let (true, Input::Edit { source, .. }) = (unknown, &request.input) {
                revision["base_nika"] = json!(source);
            }
            (None, revision)
        }
        None => (first, Value::Null),
    };
    let mut state = json!({
        "request": intent,
        "original_request": first,
        "answers": request.answers,
        "observed": request.knowledge,
        "candidate_nika": candidate,
    });
    if !revision.is_null() {
        state["revision"] = revision;
    }
    state
}

/// A revision the compiler applied over the complete document (its record says so) is judged as
/// the base with exactly the change: the base shown whole, whatever request it answers. A round
/// replaying that revision's record reads the same mode from the record, so its judge is shown
/// the same context the rejection the record carries was bound to (A3).
pub fn over_document(base: &mut Value, request: &CompileRequest, out: &CompileOutcome) {
    let revised = |record: &Value| record.get("document_revision").is_some();
    let applied = out.provenance.decision.as_ref().is_some_and(revised)
        || request.plan.as_ref().is_some_and(revised);
    if let (true, Some(revision), Input::Edit { source, .. }) =
        (applied, base.get_mut("revision"), &request.input)
    {
        revision["base_nika"] = json!(source);
        revision["over_document"] = json!(true);
    }
}

/// How many offered components the facts name; the rest are counted, never shown.
const OFFERED: usize = 24;

/// The engine facts of a door that composes from a lent catalogue, recorded once per settled
/// attempt in the native record its rounds replay (`plan.document_create.facts`), so every round
/// that judges these bytes shows the same ones ([`authoring`]):
/// - `catalogue`: the identity of the release the door was lent, or null when none was;
/// - `offered`: the admitted components it offered its author (`document::components`), each by
///   identity, title, purpose, holes and effects, at most `OFFERED` of them, with their `total`
///   (0: none offered, or no catalogue);
/// - `composed`: each receipt the record holds, witnessed on the bytes this attempt made
///   (`expanded`, `revised`, `invoked`, `absent`, `unreadable`); a receipt a whole rewrite left
///   behind is `absent`, never current composition. Not stated when no bytes were made.
///
/// The record keeps its receipts as they were (lineage). An outcome with no such record is
/// unchanged.
pub fn lent(catalog: Option<&dyn ComponentCatalog>, out: &mut CompileOutcome) {
    let release = catalog.map_or(Value::Null, |catalog| catalog.release().record());
    let offered = crate::foundry::document::components(catalog);
    let rows = offered.as_array().map_or(&[][..], Vec::as_slice);
    let mut facts = json!({
        "catalogue": release,
        "offered": {"total": rows.len(), "components": &rows[..rows.len().min(OFFERED)]},
    });
    let candidate = out.candidate.clone();
    let record = out.provenance.plan.as_mut();
    let Some(section) = record.and_then(|record| record.get_mut("document_create")) else {
        return;
    };
    if let Some(bytes) = candidate.as_deref() {
        let receipts = section["components"]
            .as_array()
            .map_or(&[][..], Vec::as_slice);
        let composed: Vec<Value> = (receipts.iter())
            .map(|receipt| {
                let seen = witness(receipt, bytes);
                json!({"component": seen["component"], "release": seen["release"],
                    "verdict": seen["verdict"]})
            })
            .collect();
        facts["composed"] = json!(composed);
    }
    section["facts"] = facts;
}

/// The engine facts a request may condition on, beside the state (A5): exactly the facts the
/// door recorded for these bytes ([`lent`]), read from the outcome's native record, else from the
/// record a round replays (`request.plan`), so a replay shows its judge the facts its rejection
/// was bound to. A judge reads them as data, never as instructions; they bind the context a
/// rejection holds in, so another release, offer or composition is judged again. No recorded
/// facts: none stated.
pub fn authoring(base: &mut Value, request: &CompileRequest, out: &CompileOutcome) {
    fn facts(record: Option<&Value>) -> Option<&Value> {
        (record.and_then(|record| record.get("document_create")))
            .and_then(|section| section.get("facts"))
            .filter(|facts| facts.is_object())
    }
    let recorded = facts(out.provenance.plan.as_ref()).or_else(|| facts(request.plan.as_ref()));
    if let Some(facts) = recorded {
        base["authoring"] = facts.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::{authoring, lent};
    use crate::foundry::{Component, ComponentCatalog, ComponentRef, Release, Unresolved};
    use serde_json::{Value, json};

    /// A catalogue of one release offering one admitted block, resolving none.
    struct Lent;

    impl ComponentCatalog for Lent {
        fn release(&self) -> Release {
            Release::new("r1", "11", "profile/r1")
        }
        fn resolve(&self, reference: &ComponentRef) -> Result<Component, Unresolved> {
            Err(Unresolved::Unknown(reference.id.clone()))
        }
        fn entries(&self) -> Vec<Value> {
            vec![
                json!({"id": "block:x", "kind": "block", "title": "X", "purpose": "do x",
                "holes": [], "effects": ["fs.write"]}),
            ]
        }
    }

    /// An outcome showing `bytes`, its native record holding `entry` as the door's section.
    fn recorded(entry: Option<Value>, bytes: Option<&str>) -> nika_compile::CompileOutcome {
        let mut out = nika_compile::initial();
        out.candidate = bytes.map(str::to_owned);
        let section = |e: Value| json!({"strategy": "native", "document_create": e});
        out.provenance.plan = Some(entry.map_or_else(|| json!({}), section));
        out
    }

    /// The door records the lent release, what it offered and each receipt witnessed on the
    /// attempt's own bytes (a receipt whose nodes these bytes lack is absent, never composed),
    /// keeping the receipts as they were; the judge shows exactly those facts, from the outcome's
    /// record or else from the record a round replays. No catalogue: null and nothing offered;
    /// no bytes: no composition stated; no door record: no facts at all.
    #[test]
    fn the_judge_reads_the_facts_the_door_recorded_on_its_own_bytes() {
        let request = nika_compile::CompileRequest::create("r");
        let receipt = json!({"component": {"id": "block:x", "release": {"version": "r1"}},
            "nodes": {"tasks": {"x": "digest"}}});
        let bytes = "nika: w\ntasks: {}\n";
        let mut out = recorded(Some(json!({"components": [receipt.clone()]})), Some(bytes));
        lent(Some(&Lent), &mut out);
        let section = &out.provenance.plan.as_ref().expect("record")["document_create"];
        assert_eq!(
            section["components"],
            json!([receipt]),
            "the lineage is kept"
        );
        let facts = &section["facts"];
        let release = json!({"version": "r1", "snapshot_sha256": "11", "profile": "profile/r1"});
        assert_eq!(facts["catalogue"], release);
        assert_eq!(facts["offered"]["total"], 1);
        assert_eq!(
            facts["offered"]["components"][0]["component"]["id"],
            "block:x"
        );
        let absent =
            json!([{"component": "block:x", "release": {"version": "r1"}, "verdict": "absent"}]);
        assert_eq!(facts["composed"], absent);
        let mut base = json!({"request": "r"});
        authoring(&mut base, &request, &out);
        assert_eq!(base["authoring"], *facts);
        let replaying = request
            .clone()
            .with_plan(out.provenance.plan.clone().unwrap_or_default());
        let mut replayed = json!({"request": "r"});
        authoring(&mut replayed, &replaying, &recorded(None, None));
        assert_eq!(
            replayed["authoring"], *facts,
            "a replay reads the record it replays"
        );
        let mut none = recorded(Some(json!({"components": []})), None);
        lent(None, &mut none);
        authoring(&mut base, &request, &none);
        let nothing = json!({"catalogue": null, "offered": {"total": 0, "components": []}});
        assert_eq!(base["authoring"], nothing);
        let mut untouched = json!({"request": "r"});
        let mut other = recorded(None, Some(bytes));
        lent(Some(&Lent), &mut other);
        assert_eq!(other.provenance.plan, Some(json!({})));
        authoring(&mut untouched, &request, &other);
        assert_eq!(untouched, json!({"request": "r"}));
    }
}
