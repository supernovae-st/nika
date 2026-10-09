// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The untrusted state a judging seat reads (R4 A11): what every question of a verdict and its
//! repair show the judge, apart from the compiler-owned reference they also carry. The verifier
//! of `nika-compile-cognition` asks the questions and weighs the answers; this state moved here
//! at that crate's size cap (2026-10-08). The engine facts a request may condition on (the
//! catalogue a door was lent, what it composed) ride the same state as data ([`authoring`]).

use nika_compile::surface::{EditChange, Input, sha256};
use nika_compile::{CompileOutcome, CompileRequest};
use serde_json::{Value, json};

use crate::foundry::ComponentCatalog;
use crate::foundry::witness::witness;

mod construction;
pub use construction::{Construction, Construed};

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

/// The sections of the native record a door that composes from a lent catalogue keeps: the
/// document door's and the document revision's.
const SECTIONS: [&str; 2] = ["document_create", "document_revision"];

/// The engine facts of a door that composes from a lent catalogue, recorded once per settled
/// attempt in its section of the native record its rounds replay (`facts`), bound to the bytes
/// they were witnessed on, so a round judging those bytes shows them ([`authoring`]) and a round
/// judging other bytes (a new revision of them) never does:
/// - `catalogue`: the identity of the release the door was lent, or null when none was;
/// - `offered`: every admitted component the door offered its author (`document::components`),
///   by identity, title, purpose, holes and effects, with their `total` (0: none offered, or no
///   catalogue); on bytes made, each with its `construction` on them ([`Construction`]): the
///   strongest witness of its receipts and the callables its admitted row declares, or why the
///   catalogue resolves no admitted bytes for it;
/// - `composed`: each receipt the section holds, witnessed on the bytes this attempt made
///   (`expanded`, `revised`, `invoked`, `absent`, `unreadable`); a receipt a rewrite left behind
///   is `absent`, never current composition; one held as admitted (`expanded`, `invoked`) also
///   states its `bindings`, each hole's path and bound literal as the receipt names them;
/// - `candidate_sha256`: those bytes. With no bytes made, no composition and no binding: no
///   round shows the facts.
///
/// The section keeps its receipts as they were (lineage). An outcome with no such section is
/// unchanged.
pub fn lent(catalog: Option<&dyn ComponentCatalog>, out: &mut CompileOutcome) {
    let release = catalog.map_or(Value::Null, |catalog| catalog.release().record());
    let offered = crate::foundry::document::components(catalog);
    let total = offered.as_array().map_or(0, Vec::len);
    let mut facts = json!({"catalogue": release,
        "offered": {"total": total, "components": offered}});
    let candidate = out.candidate.clone();
    let Some(section) = (out.provenance.plan.as_mut()).and_then(|record| {
        let key = SECTIONS.into_iter().find(|key| record[*key].is_object())?;
        record.get_mut(key)
    }) else {
        return;
    };
    if let Some(bytes) = candidate.as_deref() {
        let receipts = section["components"]
            .as_array()
            .map_or(&[][..], Vec::as_slice);
        let composed: Vec<Value> = (receipts.iter())
            .map(|receipt| {
                let seen = witness(receipt, bytes);
                let mut entry = json!({"component": seen["component"],
                    "release": seen["release"], "verdict": seen["verdict"]});
                if construction::admitted(&seen["verdict"]) {
                    entry["bindings"] = construction::bound(receipt);
                }
                entry
            })
            .collect();
        if let Some(catalog) = catalog {
            construction::assess(catalog, &mut facts["offered"]["components"], &composed);
        }
        facts["composed"] = json!(composed);
        facts["candidate_sha256"] = json!(sha256(bytes));
    }
    section["facts"] = facts;
}

/// The engine facts a request may condition on, beside the state (A5): the facts a door
/// recorded on exactly the bytes being judged ([`lent`]), read from the outcome's native record,
/// else from the record a round replays (`request.plan`), so a replay of those bytes shows the
/// facts its rejection was bound to, while a revision of them (other bytes) never shows its
/// base's facts. Shown as data, never as instructions, without the binding digest; they bind the
/// context a rejection holds in, so another release, offer or composition is judged again. No
/// facts witnessed on these bytes: none stated.
pub fn authoring(base: &mut Value, request: &CompileRequest, out: &CompileOutcome) {
    fn facts<'a>(record: Option<&'a Value>, judged: &str) -> Option<&'a Value> {
        let record = record?;
        (SECTIONS.iter())
            .filter_map(|key| record.get(*key)?.get("facts"))
            .find(|facts| facts["candidate_sha256"] == judged)
    }
    let Some(judged) = out.candidate.as_deref().map(sha256) else {
        return;
    };
    let recorded = facts(out.provenance.plan.as_ref(), &judged)
        .or_else(|| facts(request.plan.as_ref(), &judged));
    if let Some(mut shown) = recorded.cloned() {
        if let Some(fields) = shown.as_object_mut() {
            fields.remove("candidate_sha256");
        }
        base["authoring"] = shown;
    }
}

#[cfg(test)]
mod tests {
    use super::{authoring, lent};
    use crate::foundry::{Component, ComponentCatalog, ComponentRef, Release, Unresolved};
    use nika_compile::surface::sha256;
    use serde_json::{Value, json};

    /// A catalogue of one release offering `blocks` admitted blocks, the applicable `block:x`
    /// last; it resolves none.
    struct Lent {
        blocks: usize,
    }

    impl ComponentCatalog for Lent {
        fn release(&self) -> Release {
            Release::new("r1", "11", "profile/r1")
        }
        fn resolve(&self, reference: &ComponentRef) -> Result<Component, Unresolved> {
            Err(Unresolved::Unknown(reference.id.clone()))
        }
        fn entries(&self) -> Vec<Value> {
            let row = |id: String, purpose: &str| {
                json!({"id": id, "kind": "block", "title": purpose, "purpose": purpose,
                    "holes": [], "effects": ["fs.write"]})
            };
            let mut rows: Vec<Value> = (1..self.blocks)
                .map(|n| row(format!("block:decoy-{n}"), "an unrelated decoy"))
                .collect();
            rows.push(row("block:x".to_owned(), "do x"));
            rows
        }
    }

    /// An outcome showing `bytes`, its native record holding `entry` under `key`.
    fn recorded(
        key: &str,
        entry: Option<Value>,
        bytes: Option<&str>,
    ) -> nika_compile::CompileOutcome {
        let mut out = nika_compile::initial();
        out.candidate = bytes.map(str::to_owned);
        let record = |e: Value| json!({"strategy": "native", key: e});
        out.provenance.plan = Some(entry.map_or_else(|| json!({}), record));
        out
    }

    /// The door records the lent release, every component it offered (thirty here, the
    /// applicable one last: no quota), each receipt witnessed on the attempt's own bytes (absent
    /// when those bytes lack its nodes) and the digest of those bytes, keeping the receipts as
    /// they were; the judge shows exactly those facts for those bytes, from the outcome's record
    /// or else from the record a round replays, and none for other bytes.
    #[test]
    fn the_judge_reads_the_facts_the_door_recorded_on_its_own_bytes() {
        let request = nika_compile::CompileRequest::create("r");
        let receipt = json!({"component": {"id": "block:x", "release": {"version": "r1"}},
            "nodes": {"tasks": {"x": "digest"}}});
        let bytes = "nika: w\ntasks: {}\n";
        let entry = json!({"components": [receipt.clone()]});
        let mut out = recorded("document_create", Some(entry), Some(bytes));
        lent(Some(&Lent { blocks: 30 }), &mut out);
        let section = &out.provenance.plan.as_ref().expect("record")["document_create"];
        assert_eq!(
            section["components"],
            json!([receipt]),
            "the lineage is kept"
        );
        let facts = &section["facts"];
        let release = json!({"version": "r1", "snapshot_sha256": "11", "profile": "profile/r1"});
        assert_eq!(facts["catalogue"], release);
        assert_eq!(facts["offered"]["total"], 30);
        let offered = facts["offered"]["components"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        assert_eq!(offered.len(), 30, "every offered component, none cut");
        assert_eq!(
            offered[29]["component"]["id"], "block:x",
            "the applicable one, last"
        );
        // Why no offer resolves (unknown, never no fit); the receipt pins no release digest,
        // so it witnesses no offer of this release.
        let unresolved = "the admitted release holds no component `block:x`";
        assert_eq!(
            offered[29]["construction"],
            json!({"held": null, "unresolved": unresolved})
        );
        assert_eq!(offered[0]["construction"]["held"], Value::Null);
        let absent = json!([{"component": "block:x", "release": {"version": "r1"},
            "verdict": "absent"}]);
        assert_eq!(facts["composed"], absent);
        assert_eq!(facts["candidate_sha256"], json!(sha256(bytes)));
        let mut shown = facts.clone();
        shown
            .as_object_mut()
            .expect("facts")
            .remove("candidate_sha256");
        let mut base = json!({"request": "r"});
        authoring(&mut base, &request, &out);
        assert_eq!(base["authoring"], shown);
        let plan = out.provenance.plan.clone().unwrap_or_default();
        let replaying = request.clone().with_plan(plan.clone());
        let mut replayed = json!({"request": "r"});
        authoring(
            &mut replayed,
            &replaying,
            &recorded("document_create", None, Some(bytes)),
        );
        assert_eq!(
            replayed["authoring"], shown,
            "a replay of the same bytes reads them"
        );
        let mut revised = json!({"request": "r"});
        let other = recorded("document_revision", None, Some("nika: w2\ntasks: {}\n"));
        authoring(&mut revised, &request.clone().with_plan(plan), &other);
        assert_eq!(
            revised,
            json!({"request": "r"}),
            "other bytes never read a base's facts"
        );
    }

    /// A receipt witnessed as held on the bytes (its node re-derived, its bound literal there)
    /// states what it binds, each hole's path and literal; one the bytes no longer hold states
    /// none.
    #[test]
    fn a_receipt_held_on_the_bytes_states_its_bindings() {
        let bytes = "nika: w\nconst:\n  path: ./in/x.json\ntasks:\n  x:\n    invoke: { tool: \"nika:read\", args: { path: \"${{ const.path }}\" } }\n";
        let document = nika_compile::surface::literal_projection(bytes).expect("literal");
        let release = json!({"version": "r1", "snapshot_sha256": "11"});
        let binding = json!({"path": "const.path", "hole": "const.path", "owner": "human",
            "component_literal": "./data/x.json", "bound": "./in/x.json"});
        let held = json!({"component": {"id": "block:x", "release": release},
            "nodes": {"tasks": {"x": sha256(&document["tasks"]["x"].to_string())}},
            "bindings": [binding]});
        let left = json!({"component": {"id": "block:y", "release": release},
            "nodes": {"tasks": {"y": "digest"}}, "bindings": [binding]});
        let entry = json!({"components": [held, left]});
        let mut out = recorded("document_create", Some(entry), Some(bytes));
        lent(Some(&Lent { blocks: 1 }), &mut out);
        let section = &out.provenance.plan.as_ref().expect("record")["document_create"];
        let composed = &section["facts"]["composed"];
        assert_eq!(composed[0]["verdict"], "expanded", "{composed:#}");
        let bound = json!([{"path": "const.path", "bound": "./in/x.json"}]);
        assert_eq!(composed[0]["bindings"], bound);
        assert_eq!(composed[1]["verdict"], "absent", "{composed:#}");
        assert_eq!(composed[1].get("bindings"), None);
    }

    /// A revision's own section is recorded the same way and read for its bytes; no catalogue
    /// offers nothing; no bytes bind nothing; no door section, no facts at all.
    #[test]
    fn a_revision_records_its_own_facts_and_nothing_binds_without_bytes() {
        let request = nika_compile::CompileRequest::create("r");
        let bytes = "nika: v\ntasks: {}\n";
        let mut revision = recorded(
            "document_revision",
            Some(json!({"components": []})),
            Some(bytes),
        );
        lent(None, &mut revision);
        let mut base = json!({"request": "r"});
        authoring(&mut base, &request, &revision);
        let nothing = json!({"catalogue": null, "offered": {"total": 0, "components": []},
            "composed": []});
        assert_eq!(base["authoring"], nothing);
        let mut unbound = recorded("document_create", Some(json!({"components": []})), None);
        lent(None, &mut unbound);
        let section = &unbound.provenance.plan.as_ref().expect("record")["document_create"];
        assert!(
            section["facts"].get("candidate_sha256").is_none(),
            "{section:#}"
        );
        let mut untouched = json!({"request": "r"});
        let mut other = recorded("document_create", None, Some(bytes));
        lent(Some(&Lent { blocks: 1 }), &mut other);
        assert_eq!(other.provenance.plan, Some(json!({})));
        authoring(&mut untouched, &request, &other);
        assert_eq!(untouched, json!({"request": "r"}));
    }
}
