// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The construction facts a door records beside its offer, and the localization alternatives a
//! judged state turns them into: the current-byte witness and the resolved contract are facts;
//! fit stays the judge's, and an offer the catalogue cannot resolve keeps it unknown.

use super::{CONSTRUCTION, Construction, Construed, assess};
use crate::foundry::component::pinned;
use crate::foundry::{Component, ComponentCatalog, ComponentRef, Release, Unresolved};
use serde_json::{Value, json};

/// A catalogue of release `r1`: `block:fit` and `block:other` resolve with the callables their
/// rows declare; `block:gone` resolves to nothing.
struct Lent;

impl ComponentCatalog for Lent {
    fn release(&self) -> Release {
        Release::new("r1", "11", "profile/r1")
    }
    fn resolve(&self, reference: &ComponentRef) -> Result<Component, Unresolved> {
        pinned(reference, &self.release())?;
        let callables: &[&str] = match reference.id.as_str() {
            "block:fit" => &["nika:read", "nika:jq", "nika:write"],
            "block:other" => &["nika:fetch"],
            _ => return Err(Unresolved::Unknown(reference.id.clone())),
        };
        let source = "nika: probe\ntasks: {}\n";
        let mut component = Component::new(
            reference.id.clone(),
            self.release(),
            "blocks/x.nika",
            "",
            source,
        );
        component.callables = callables.iter().map(|c| (*c).to_owned()).collect();
        Ok(component)
    }
}

/// The offer as the door lists it: each id at `r1`, with a title, a human-owned hole, a
/// deterministic one with its producer's note, and an effect.
fn offer(ids: &[&str]) -> Value {
    let holes = json!([{"name": "const.path", "owner": "human"},
        {"name": "tasks.keep.invoke.args.expression", "owner": "deterministic",
            "note": "the filter; never a question to the human"}]);
    let rows: Vec<Value> = (ids.iter())
        .map(|id| {
            json!({"component": {"id": id, "version": "r1"}, "title": format!("the {id} block"),
                "purpose": "a purpose", "holes": holes, "effects": ["fs.read"]})
        })
        .collect();
    Value::Array(rows)
}

/// A receipt of `id` in the release `version` of digest `snapshot`, witnessed on the judged
/// bytes as `verdict`.
fn seen(id: &str, (version, snapshot): (&str, &str), verdict: &str) -> Value {
    json!({"component": id, "verdict": verdict,
        "release": {"version": version, "snapshot_sha256": snapshot, "profile": "profile/r1"}})
}

/// The lent release's exact pin.
const LENT: (&str, &str) = ("r1", "11");

/// The judged state whose engine facts hold `offered`.
fn judged(offered: &Value) -> Value {
    json!({"request": "r", "authoring": {"offered": {"total": 1, "components": offered}}})
}

fn keys(construction: &Construction) -> Vec<String> {
    (construction.options().into_iter())
        .map(|option| option.key)
        .collect()
}

/// Each offer states what the judged bytes hold of it: the strongest witness of its receipts in
/// the lent release, pinned by version and digest (the same version under another digest, or
/// another version, is another release's component), the callables its admitted row declares,
/// or why the catalogue resolves no admitted bytes for it.
#[test]
fn each_offer_states_its_current_witness_and_resolved_contract() {
    let mut offered = offer(&["block:fit", "block:other", "block:gone"]);
    let composed = [
        seen("block:fit", LENT, "absent"),
        seen("block:fit", LENT, "revised"),
        seen("block:other", ("r1", "22"), "expanded"),
        seen("block:other", ("r0", "11"), "expanded"),
        seen("block:gone", LENT, "expanded"),
    ];
    assess(&Lent, &mut offered, &composed);
    let statuses: Vec<&Value> = (offered.as_array().into_iter().flatten())
        .map(|row| &row["construction"])
        .collect();
    assert_eq!(
        statuses,
        [
            &json!({"held": "revised", "callables": ["nika:read", "nika:jq", "nika:write"]}),
            &json!({"held": null, "callables": ["nika:fetch"]}),
            &json!({"held": "expanded",
                "unresolved": "the admitted release holds no component `block:gone`"}),
        ]
    );
    assert_eq!(
        offered[0]["title"], "the block:fit block",
        "the row is kept"
    );
}

/// A localization may name each offer that resolves and that the bytes do not hold as admitted
/// (a revised one included), then `no_fit`; a named one is a defect stating the component, what
/// the bytes hold of it, its holes and its contract. With an offer the catalogue could not
/// resolve, `no_fit` decides nothing: the fit stays unknown.
#[test]
fn a_named_offer_is_a_defect_and_an_unresolved_offer_keeps_no_fit_unknown() {
    let mut offered = offer(&["block:fit", "block:other", "block:gone"]);
    assess(&Lent, &mut offered, &[seen("block:fit", LENT, "revised")]);
    let construction = Construction::of(&judged(&offered));
    assert_eq!(
        keys(&construction),
        ["component-0", "component-1", "no_fit"]
    );
    let options = construction.options();
    assert_eq!(
        options[0].description,
        "`block:fit` (release r1) · the block:fit block: these bytes hold it changed from its admitted nodes"
    );
    assert_eq!(
        options[1].description,
        "`block:other` (release r1) · the block:other block: these bytes do not hold it"
    );
    let Some(Construed::Defect(note)) = construction.read("component-0") else {
        panic!("a named offer is a defect");
    };
    assert_eq!(
        note,
        "the judge points to the admitted component `block:fit` of release r1 (the block:fit block): these bytes hold it changed from its admitted nodes; compose it by that reference, each hole bound as its owner and contract state (holes: const.path (human), tasks.keep.invoke.args.expression (deterministic: the filter; never a question to the human)) from the request, its answers, the observed world or what they establish, a question only for a value its human owner must give and none of them gives, never the component's own literal its contract does not grant; it declares the effects fs.read through nika:read, nika:jq, nika:write, which grant nothing: the document's own permits grant exactly what it reaches; a copy by hand is no composition"
    );
    assert_eq!(construction.read("no_fit"), Some(Construed::Undecided));
    for key in [
        "component-2",
        "component-3",
        "component-x",
        "task-read",
        "no_task",
    ] {
        assert_eq!(construction.read(key), None, "{key}");
    }
    let told = construction.told("Say why.".to_owned());
    assert_eq!(told, format!("Say why. {CONSTRUCTION}"));
}

/// A receipt a rewrite left behind (its nodes gone: `absent`) holds nothing: the offer stays a
/// component the judge may name, said not held.
#[test]
fn a_stale_receipt_holds_nothing() {
    let mut offered = offer(&["block:fit"]);
    assess(&Lent, &mut offered, &[seen("block:fit", LENT, "absent")]);
    assert_eq!(offered[0]["construction"]["held"], "absent");
    let construction = Construction::of(&judged(&offered));
    assert_eq!(keys(&construction), ["component-0", "no_fit"]);
    let described = &construction.options()[0].description;
    assert!(
        described.ends_with("these bytes do not hold it"),
        "{described}"
    );
}

/// Every offer examinable: the judge's `no_fit` lets the clause's own alternative stand, even
/// beside an offer the bytes hold as admitted, which is no alternative to name.
#[test]
fn no_fit_over_examinable_offers_lets_the_alternative_stand() {
    let mut offered = offer(&["block:fit", "block:other"]);
    assess(&Lent, &mut offered, &[seen("block:fit", LENT, "expanded")]);
    let construction = Construction::of(&judged(&offered));
    assert_eq!(keys(&construction), ["component-1", "no_fit"]);
    assert_eq!(construction.read("component-0"), None, "held as admitted");
    assert_eq!(construction.read("no_fit"), Some(Construed::Fallback));
}

/// No offer, or facts recorded before construction statuses: nothing more than the runtime
/// alternatives, and the instructions unchanged.
#[test]
fn no_offer_or_no_status_adds_nothing() {
    let without = [
        json!({"request": "r"}),
        judged(&json!([])),
        judged(&offer(&["block:fit"])),
    ];
    for state in without {
        let construction = Construction::of(&state);
        assert!(construction.options().is_empty(), "{state:#}");
        assert_eq!(construction.told("Say why.".to_owned()), "Say why.");
        assert_eq!(construction.read("no_fit"), None, "{state:#}");
        assert_eq!(construction.read("component-0"), None, "{state:#}");
    }
}

/// The question's record keeps the basis of a construction answer: the component it named, or,
/// for `no_fit`, every offer with what the bytes hold of it and whether the alternative stands.
#[test]
fn the_record_keeps_the_basis_of_a_construction_answer() {
    let mut offered = offer(&["block:fit", "block:gone"]);
    assess(&Lent, &mut offered, &[]);
    let construction = Construction::of(&judged(&offered));
    let mut record = json!({"question": "verify-point-6"});
    construction.annotate(Some(&mut record), Some("component-0"));
    let named = json!({"component": {"id": "block:fit", "version": "r1"},
        "construction": offered[0]["construction"]});
    assert_eq!(record["construction"], json!({"named": named}));
    construction.annotate(Some(&mut record), Some("no_fit"));
    let basis: Vec<Value> = (offered.as_array().into_iter().flatten())
        .map(|row| json!({"component": row["component"], "construction": row["construction"]}))
        .collect();
    let unknown = json!({"no_fit": {"alternative_stands": false, "offered": basis}});
    assert_eq!(record["construction"], unknown);
    let mut untouched = json!({"question": "verify-point-6"});
    for answer in [Some("task-read"), Some("no_task"), None] {
        construction.annotate(Some(&mut untouched), answer);
    }
    assert_eq!(untouched, json!({"question": "verify-point-6"}));
}
