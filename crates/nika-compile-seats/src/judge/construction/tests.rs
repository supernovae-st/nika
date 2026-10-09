// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The construction facts a door records beside its offer, and the localization alternatives a
//! judged state turns them into: the current-byte witness and the resolved contract are facts;
//! fit stays the judge's, and an offer the catalogue cannot resolve keeps it unknown.

use super::{
    ALTERNATIVES, CONSTRUCTION, Construction, Construed, HELD_ALTERNATIVE, HISTORY, HOLDING, assess,
};
use crate::foundry::component::pinned;
use crate::foundry::{Component, ComponentCatalog, ComponentRef, Release, Unresolved};
use nika_compile::surface::sha256;
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
    assert_eq!(told, format!("Say why. {CONSTRUCTION} {ALTERNATIVES}"));
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

/// A later question over the same state is shown each standing `no_fit` of the judge as its own
/// history, bound to the judged bytes and the lent catalogue, beside the construction context.
/// A fit left unknown, a named component, no choice, or a finding over other statuses (another
/// witness of the same offer) is never one; with none, the state and instructions stay as they
/// are.
#[test]
fn a_standing_no_fit_is_recalled_as_history_and_nothing_else_is() {
    let mut offered = offer(&["block:fit", "block:other"]);
    assess(&Lent, &mut offered, &[]);
    let construction = Construction::of(&judged(&offered));
    let bytes = "nika: w\ntasks: {}\n";
    let release = json!({"version": "r1", "snapshot_sha256": "11", "profile": "profile/r1"});
    let shown = || {
        let mut state = judged(&offered);
        state["candidate_nika"] = json!(bytes);
        state["authoring"]["catalogue"] = release.clone();
        state
    };
    let asked = |id: &str, answer: Option<&str>| {
        let mut record = json!({"question": id, "choice": answer, "clause": {"text": id}});
        construction.annotate(Some(&mut record), answer);
        record
    };
    let standing = asked("verify-point-6", Some("no_fit"));
    let mut unknown = asked("verify-point-1", Some("no_fit"));
    unknown["construction"]["no_fit"]["alternative_stands"] = json!(false);
    let mut elsewhere = asked("verify-point-2", Some("no_fit"));
    elsewhere["construction"]["no_fit"]["offered"][0]["construction"]["held"] = json!("expanded");
    let others = [
        unknown,
        elsewhere,
        asked("verify-point-3", Some("component-0")),
        asked("verify-point-4", None),
    ];
    let mut state = shown();
    let mut records = others.to_vec();
    records.push(standing.clone());
    let told = construction.recall(&mut state, &records, "Judge it.".to_owned());
    assert_eq!(told, format!("Judge it. {CONSTRUCTION} {HISTORY}"));
    let finding = json!({"question": "verify-point-6", "clause": "verify-point-6",
        "choice": "no_fit", "basis": standing["construction"]["no_fit"]["offered"]});
    let history = json!({"candidate_sha256": sha256(bytes), "catalogue": release,
        "construction": [finding]});
    assert_eq!(state["history"], history);
    let mut untouched = shown();
    let told = construction.recall(&mut untouched, &others, "Judge it.".to_owned());
    assert_eq!(told, "Judge it.");
    assert_eq!(untouched, shown());
    let mut bare = json!({"request": "r"});
    let told = Construction::of(&bare).recall(&mut bare, &records, "Judge it.".to_owned());
    assert_eq!(told, "Judge it.");
    assert_eq!(bare, json!({"request": "r"}));
}

/// The bytes a judged state shows.
const BYTES: &str = "nika: w\ntasks: {}\n";

/// The lent release's identity, as the door records it.
fn release() -> Value {
    json!({"version": "r1", "snapshot_sha256": "11", "profile": "profile/r1"})
}

/// A receipt of `id` in the lent release witnessed `verdict` on the judged bytes, binding its
/// path hole `to` a literal, as the door records it (`judge::lent`).
fn receipt(id: &str, verdict: &str, to: &str) -> Value {
    let mut receipt = seen(id, LENT, verdict);
    receipt["bindings"] = json!([{"path": "const.path", "bound": to}]);
    receipt
}

/// The judged state of [`BYTES`] whose engine facts hold `offered`, the lent release and what
/// the door composed.
fn holding(offered: &Value, composed: &[Value]) -> Value {
    let mut state = judged(offered);
    state["candidate_nika"] = json!(BYTES);
    state["authoring"]["catalogue"] = release();
    state["authoring"]["composed"] = json!(composed);
    state
}

/// What a question is shown of a component the bytes hold: its place in the offer, identity,
/// title, witness and bindings.
fn shown(k: usize, id: &str, witness: &str, to: &str) -> Value {
    json!({"offer": k, "component": {"id": id, "version": "r1"}, "title": format!("the {id} block"),
        "witness": witness, "bindings": [{"path": "const.path", "bound": to}]})
}

/// A question judging the bytes is told each offered component they hold as admitted and whose
/// contract resolves, `expanded` and `invoked` each with its own witness and bindings, bound to
/// these bytes and the lent release, beside the construction context and what holding means
/// (each witness said apart). One held whose contract does not resolve is not among them.
#[test]
fn a_question_judging_the_bytes_is_told_each_component_they_hold() {
    let mut offered = offer(&["block:fit", "block:other", "block:gone"]);
    let composed = [
        receipt("block:fit", "expanded", "./in/x.json"),
        receipt("block:other", "invoked", "./in/y.json"),
        receipt("block:gone", "expanded", "./in/z.json"),
    ];
    assess(&Lent, &mut offered, &composed);
    let mut state = holding(&offered, &composed);
    let construction = Construction::of(&state);
    let told = construction.holding(&mut state, "Judge it.".to_owned());
    assert_eq!(told, format!("Judge it. {CONSTRUCTION} {HOLDING}"));
    let held = [
        shown(0, "block:fit", "expanded", "./in/x.json"),
        shown(1, "block:other", "invoked", "./in/y.json"),
    ];
    let bound_to = json!({"candidate_sha256": sha256(BYTES), "catalogue": release(),
        "held": held});
    assert_eq!(state["construction"], bound_to);
    assert_eq!(
        state["authoring"]["composed"],
        json!(composed),
        "the facts kept"
    );
    for witness in [
        "`expanded`: its admitted nodes",
        "`invoked`: a task of these bytes calls",
    ] {
        assert!(HOLDING.contains(witness), "{witness}");
    }
}

/// Bytes that hold no offered component as admitted (none composed, one revised, one a rewrite
/// left behind, one held whose contract does not resolve), facts recorded before construction
/// statuses, or no facts: a question is told nothing more and its state stays as it is.
#[test]
fn nothing_held_tells_a_question_nothing_more() {
    let state_of = |verdict: Option<&str>, id: &str| {
        let mut offered = offer(&["block:fit", "block:gone"]);
        let composed: Vec<Value> = (verdict.into_iter())
            .map(|verdict| receipt(id, verdict, "./in/x.json"))
            .collect();
        assess(&Lent, &mut offered, &composed);
        holding(&offered, &composed)
    };
    let states = [
        state_of(None, "block:fit"),
        state_of(Some("revised"), "block:fit"),
        state_of(Some("absent"), "block:fit"),
        state_of(Some("expanded"), "block:gone"),
        holding(
            &offer(&["block:fit"]),
            &[receipt("block:fit", "expanded", "./in/x.json")],
        ),
        json!({"request": "r", "candidate_nika": BYTES}),
    ];
    for state in states {
        let mut asked = state.clone();
        let told = Construction::of(&state).holding(&mut asked, "Judge it.".to_owned());
        assert_eq!(told, "Judge it.", "{state:#}");
        assert_eq!(asked, state);
    }
}

/// A localization over bytes holding a component offers it as `held-<k>` (its witness and
/// bindings in the option) beside the runtime alternatives, the offers the bytes lack and
/// `no_fit`, told what each means; `held-<k>` reads as that component held, never as a defect or
/// a fallback, and its record keeps that basis. `no_fit` keeps its meaning: an offer that could
/// not be examined keeps the fit unknown.
#[test]
fn a_localization_offers_a_held_component_beside_the_ones_lacking() {
    let mut offered = offer(&["block:fit", "block:other", "block:gone"]);
    let composed = [receipt("block:fit", "expanded", "./in/x.json")];
    assess(&Lent, &mut offered, &composed);
    let mut state = holding(&offered, &composed);
    let construction = Construction::of(&state);
    let tasks = ["read".to_owned(), "keep".to_owned()];
    let instructions = "Say why.".to_owned();
    let (told, options) = construction.localization((&tasks, true), &mut state, instructions);
    let keys: Vec<&str> = options.iter().map(|option| option.key.as_str()).collect();
    let alternatives = [
        "task-read",
        "task-keep",
        "omitted",
        "component-1",
        "no_fit",
        "held-0",
        "no_task",
    ];
    assert_eq!(keys, alternatives);
    assert_eq!(
        options[5].description,
        "`block:fit` (release r1) · the block:fit block: these bytes hold it as admitted (expanded; bound: const.path = \"./in/x.json\")"
    );
    let meaning = format!("Say why. {CONSTRUCTION} {HOLDING} {ALTERNATIVES} {HELD_ALTERNATIVE}");
    assert_eq!(told, meaning);
    let held = json!([shown(0, "block:fit", "expanded", "./in/x.json")]);
    assert_eq!(state["construction"]["held"], held);
    assert_eq!(construction.read("held-0"), Some(Construed::Held));
    for key in ["held-1", "held-2", "held-x", "component-0"] {
        assert_eq!(construction.read(key), None, "{key}");
    }
    assert_eq!(construction.read("no_fit"), Some(Construed::Undecided));
    let mut record = json!({"question": "verify-point-6"});
    construction.annotate(Some(&mut record), Some("held-0"));
    let basis = json!({"component": offered[0]["component"],
        "construction": offered[0]["construction"]});
    assert_eq!(record["construction"], json!({"held": basis}));
}

/// With no component held, a localization is the one a missing clause always had: each task,
/// `omitted` when the clause may ask an operation of its own, the construction alternatives, then
/// `no_task`, told what those alternatives mean, its state unchanged.
#[test]
fn a_localization_without_a_held_component_is_unchanged() {
    let mut offered = offer(&["block:fit", "block:other"]);
    let composed = [receipt("block:fit", "revised", "./in/x.json")];
    assess(&Lent, &mut offered, &composed);
    let state = holding(&offered, &composed);
    let construction = Construction::of(&state);
    let tasks = ["read".to_owned()];
    for (omittable, runtime) in [
        (true, vec!["task-read", "omitted"]),
        (false, vec!["task-read"]),
    ] {
        let mut asked = state.clone();
        let (told, options) =
            construction.localization((&tasks, omittable), &mut asked, "Say why.".to_owned());
        let mut expected = runtime;
        expected.extend(["component-0", "component-1", "no_fit", "no_task"]);
        let keys: Vec<&str> = options.iter().map(|option| option.key.as_str()).collect();
        assert_eq!(keys, expected);
        assert_eq!(options[0].description, "the task `read`");
        assert_eq!(told, construction.told("Say why.".to_owned()));
        assert_eq!(asked, state);
    }
    let mut bare = json!({"request": "r"});
    let (told, options) =
        Construction::default().localization((&tasks, false), &mut bare, "Say why.".to_owned());
    assert_eq!(told, "Say why.");
    let keys: Vec<&str> = options.iter().map(|option| option.key.as_str()).collect();
    assert_eq!(keys, ["task-read", "no_task"]);
}

/// The whole request over a trial run is told what the bytes hold as every judging question is,
/// beside the judge's own standing `no_fit` history when there is one.
#[test]
fn a_later_question_is_told_what_the_bytes_hold_beside_its_history() {
    let mut offered = offer(&["block:fit", "block:other"]);
    let composed = [receipt("block:fit", "invoked", "./in/x.json")];
    assess(&Lent, &mut offered, &composed);
    let construction = Construction::of(&holding(&offered, &composed));
    let mut standing = json!({"question": "verify-point-6", "choice": "no_fit",
        "clause": {"text": "use a component when one applies"}});
    construction.annotate(Some(&mut standing), Some("no_fit"));
    let held = json!([shown(0, "block:fit", "invoked", "./in/x.json")]);
    let mut state = holding(&offered, &composed);
    let told = construction.recall(&mut state, &[], "Judge it.".to_owned());
    assert_eq!(told, format!("Judge it. {CONSTRUCTION} {HOLDING}"));
    assert_eq!(state["construction"]["held"], held);
    assert_eq!(state.get("history"), None);
    let mut state = holding(&offered, &composed);
    let told = construction.recall(&mut state, &[standing], "Judge it.".to_owned());
    assert_eq!(
        told,
        format!("Judge it. {CONSTRUCTION} {HOLDING} {HISTORY}")
    );
    assert_eq!(state["construction"]["held"], held);
    assert_eq!(state["history"]["construction"][0]["choice"], "no_fit");
}

/// What a question judging the bytes is shown (the whole request among them): a copy of the
/// judged state and its instructions, both told what the bytes hold when they hold a component,
/// `invoked` kept apart from `expanded`; both as they are when nothing is held.
#[test]
fn a_question_is_shown_a_copy_of_the_state_told_what_the_bytes_hold() {
    for witness in ["expanded", "invoked"] {
        let mut offered = offer(&["block:fit", "block:other"]);
        let composed = [receipt("block:fit", witness, "./in/x.json")];
        assess(&Lent, &mut offered, &composed);
        let state = holding(&offered, &composed);
        let (asked, told) = Construction::shown(&state, "Judge it.");
        assert_eq!(told, format!("Judge it. {CONSTRUCTION} {HOLDING}"));
        let held = json!([shown(0, "block:fit", witness, "./in/x.json")]);
        assert_eq!(asked["construction"]["held"], held, "{witness}");
        let mut rest = asked.clone();
        rest.as_object_mut()
            .expect("a state")
            .remove("construction");
        assert_eq!(rest, state, "otherwise a copy of the judged state");
    }
    let mut offered = offer(&["block:fit"]);
    assess(&Lent, &mut offered, &[]);
    for state in [holding(&offered, &[]), json!({"request": "r"})] {
        let asked = Construction::shown(&state, "Judge it.");
        assert_eq!(asked, (state.clone(), "Judge it.".to_owned()));
    }
}
