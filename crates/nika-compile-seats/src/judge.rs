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

/// The release a door that composes from a lent catalogue was lent, recorded beside what it
/// composed in the native record its rounds replay (`plan.document_create`): its identity, or
/// null when none was lent. An outcome with no such record is unchanged.
pub fn lent(catalog: Option<&dyn ComponentCatalog>, out: &mut CompileOutcome) {
    let release = catalog.map_or(Value::Null, |catalog| catalog.release().record());
    let record = out.provenance.plan.as_mut();
    if let Some(section) = record.and_then(|record| record.get_mut("document_create")) {
        section["catalogue"] = release;
    }
}

/// The engine facts a request may condition on, beside the state (A5): the components the door
/// composed into these bytes, by receipt (`[]`: none), and the catalogue release it was lent
/// (null: none), as the native record states them ([`lent`]): the outcome's, else the record a
/// round replays (`request.plan`), so a replay shows its judge the facts its rejection was bound
/// to. A fact the record does not state is not stated. A judge reads them as data, never as
/// instructions; they bind the context a rejection holds in, so another release or composition
/// is judged again. No such record: no facts.
pub fn authoring(base: &mut Value, request: &CompileRequest, out: &CompileOutcome) {
    fn section(record: Option<&Value>) -> Option<&Value> {
        record
            .and_then(|record| record.get("document_create"))
            .filter(|s| s.is_object())
    }
    let records = [out.provenance.plan.as_ref(), request.plan.as_ref()];
    let Some(entry) = records.into_iter().find_map(section) else {
        return;
    };
    let composed = entry.get("components").cloned();
    let mut facts = json!({"composed": composed.unwrap_or_else(|| json!([]))});
    if let Some(release) = entry.get("catalogue") {
        facts["catalogue"] = release.clone();
    }
    base["authoring"] = facts;
}

#[cfg(test)]
mod tests {
    use super::{authoring, lent};
    use crate::foundry::{Component, ComponentCatalog, ComponentRef, Release, Unresolved};
    use serde_json::{Value, json};

    /// A catalogue of one release, holding nothing a test resolves.
    struct Lent;

    impl ComponentCatalog for Lent {
        fn release(&self) -> Release {
            Release::new("r1", "11", "profile/r1")
        }
        fn resolve(&self, reference: &ComponentRef) -> Result<Component, Unresolved> {
            Err(Unresolved::Unknown(reference.id.clone()))
        }
    }

    /// An outcome whose native record holds `entry` as the document door's section, or none.
    fn recorded(entry: Option<Value>) -> nika_compile::CompileOutcome {
        let mut out = nika_compile::initial();
        let section = |e: Value| json!({"strategy": "native", "document_create": e});
        out.provenance.plan = Some(entry.map_or_else(|| json!({}), section));
        out
    }

    /// The door's record gains the lent release (or null), and the judge's state reads it beside
    /// the composed receipts, from the outcome's record or else the record a round replays; with
    /// no such record nothing is stated, and a fact the record does not hold is not stated either.
    #[test]
    fn the_judge_reads_the_release_and_receipts_the_door_recorded() {
        let request = nika_compile::CompileRequest::create("r");
        let receipt = json!({"component": {"id": "block:x"}});
        let mut out = recorded(Some(json!({"components": [receipt.clone()]})));
        lent(Some(&Lent), &mut out);
        let mut base = json!({"request": "r"});
        authoring(&mut base, &request, &out);
        let release = json!({"version": "r1", "snapshot_sha256": "11", "profile": "profile/r1"});
        let facts = json!({"composed": [receipt], "catalogue": release});
        assert_eq!(base["authoring"], facts);
        let replaying = request
            .clone()
            .with_plan(out.provenance.plan.clone().unwrap_or_default());
        let mut replayed = json!({"request": "r"});
        authoring(&mut replayed, &replaying, &recorded(None));
        assert_eq!(
            replayed["authoring"], facts,
            "a replay reads the record it replays"
        );
        let mut none = recorded(Some(json!({"components": []})));
        lent(None, &mut none);
        authoring(&mut base, &request, &none);
        assert_eq!(
            base["authoring"],
            json!({"composed": [], "catalogue": null})
        );
        authoring(&mut base, &request, &recorded(Some(json!({}))));
        assert_eq!(base["authoring"], json!({"composed": []}));
        let mut untouched = json!({"request": "r"});
        let mut other = recorded(None);
        lent(Some(&Lent), &mut other);
        assert_eq!(other.provenance.plan, Some(json!({})));
        authoring(&mut untouched, &request, &other);
        assert_eq!(untouched, json!({"request": "r"}));
    }
}
