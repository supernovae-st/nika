// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The remote compile input laws where they now live: an object only, a present value never
//! null, literal answers kept as sent and refused when a key repeats, an observation admitted
//! against the words an input states, and the request an input states carrying exactly its
//! intent, name, observation, base and answers.

use super::*;

const READS: &str = "Read ./calendar.json and save ./out/r.json";
const WORLD: &str =
    r#"{"observed": [{"path": "./calendar.json", "state": "observed", "kind": "json"}]}"#;
const TRIAL: &str = r#"{"files": [{"path": "./calendar.json", "text": "{}"}]}"#;

#[test]
fn an_observation_is_admitted_against_the_words_an_input_states_once() {
    let create = |intent: &str| Input::Create {
        intent: intent.to_owned(),
        workflow_id: None,
        observed: None,
    };
    let observed = create(READS).observe(WORLD, Some(TRIAL));
    let Some(Ok(Input::Create {
        observed: Some(kept),
        ..
    })) = observed
    else {
        panic!("admitted: {observed:?}");
    };
    assert_eq!(kept.world["observed"][0]["path"], "./calendar.json");
    assert_eq!(
        kept.trial.as_ref().map(|t| t["files"][0]["text"].clone()),
        Some("{}".into())
    );
    let unstated = create("Save ./out/r.json").observe(WORLD, None);
    assert_eq!(
        unstated,
        Some(Err(Refusal::Unstated)),
        "a path the words do not state"
    );
    let revise = Input::Revise {
        source: "nika: r\n".to_owned(),
        change: "save ./out/r.json too".to_owned(),
        original_intent: "Read ./calendar.json".to_owned(),
        observed: None,
    };
    assert!(
        matches!(
            revise.observe(WORLD, None),
            Some(Ok(Input::Revise {
                observed: Some(_),
                ..
            }))
        ),
        "a revision states its original request and its change"
    );
    let Some(Ok(once)) = create(READS).observe(WORLD, None) else {
        panic!("admitted once");
    };
    assert_eq!(once.observe(WORLD, None), None, "an input is observed once");
    let constant = Input::Constant {
        source: "nika: r\n".to_owned(),
        name: "limit".to_owned(),
        literal: "72".to_owned(),
    };
    assert_eq!(
        constant.observe(WORLD, None),
        None,
        "a constant states no file"
    );
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Probe {
    #[serde(default, deserialize_with = "present")]
    name: Option<String>,
    #[serde(default)]
    answers: Answers,
}

fn probe(json: &str) -> Result<Probe, String> {
    serde_json::from_str::<Object<Probe>>(json)
        .map(|Object(probe)| probe)
        .map_err(|error| error.to_string())
}

#[test]
fn only_an_object_is_read_and_its_own_fields_still_judge() {
    let read = probe(r#"{"name": "brief"}"#).expect("an object");
    assert_eq!(read.name.as_deref(), Some("brief"));
    assert!(read.answers.0.is_empty(), "absent answers are none");
    assert!(probe(r#"["brief"]"#).is_err(), "a positional array");
    assert!(
        probe(r#"{"name": "a", "extra": 1}"#).is_err(),
        "an unknown field"
    );
    assert!(
        probe(r#"{"name": "a", "name": "b"}"#).is_err(),
        "a repeated field"
    );
}

#[test]
fn a_present_null_is_never_an_absent_value() {
    assert!(probe(r#"{"name": null}"#).is_err());
    assert!(probe(r#"{"name": 7}"#).is_err(), "a value of another type");
    assert!(probe("{}").expect("absent").name.is_none());
}

#[test]
fn answers_keep_the_literal_text_and_a_repeated_key_selects_neither() {
    let read = probe(r#"{"answers": {"count": 1.50, "city": "Zürich « vite »", "on": null}}"#)
        .expect("answers");
    let texts: Vec<(&str, &str)> = (read.answers.0.iter())
        .map(|(key, literal)| (key.as_str(), literal.get()))
        .collect();
    assert_eq!(
        texts,
        [
            ("city", r#""Zürich « vite »""#),
            ("count", "1.50"),
            ("on", "null")
        ],
        "each literal exactly as sent, a null literal included"
    );
    let repeated = probe(r#"{"answers": {"count": 1, "count": 2}}"#).err();
    assert!(
        repeated.is_some_and(|why| why.contains("duplicate answer key")),
        "two values for one question select neither"
    );
    assert!(
        probe(r#"{"answers": [1]}"#).is_err(),
        "answers are an object"
    );
}

fn literals(pairs: &[(&str, &str)]) -> BTreeMap<String, Box<RawValue>> {
    (pairs.iter())
        .map(|(key, text)| {
            let literal = RawValue::from_string((*text).to_owned()).expect("literal");
            ((*key).to_owned(), literal)
        })
        .collect()
}

#[test]
fn a_creation_states_its_name_observation_and_answers_unchanged() {
    let world = serde_json::json!({"rows": [{"path": "notes/brief.md", "state": "observed"}]});
    let trial = serde_json::json!({"files": [{"path": "notes/brief.md", "text": "# Brief\n"}]});
    let input = Input::Create {
        intent: "copy the brief".to_owned(),
        workflow_id: Some("copy-brief".to_owned()),
        observed: Some(Observed {
            world: world.clone(),
            trial: Some(trial.clone()),
        }),
    };
    assert_eq!(input.trial(), Some(&trial));
    let request = input.request(&literals(&[("format", r#""md""#), ("count", "1.50")]));
    assert_eq!(request.workflow_id.as_deref(), Some("copy-brief"));
    assert_eq!(request.knowledge, Some(world));
    assert_eq!(request.original_intent, None);
    let answers: Vec<(&str, &str)> = (request.answers.iter())
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    assert_eq!(answers, [("count", "1.50"), ("format", r#""md""#)]);
}

#[test]
fn a_revision_names_its_base_request_and_a_constant_carries_no_trial() {
    let revise = Input::Revise {
        source: "nika: copy\n".to_owned(),
        change: "write markdown".to_owned(),
        original_intent: "copy the brief".to_owned(),
        observed: None,
    };
    assert_eq!(revise.trial(), None);
    let request = revise.request(&BTreeMap::new());
    assert_eq!(request.original_intent.as_deref(), Some("copy the brief"));
    assert_eq!(request.knowledge, None);
    assert!(request.answers.is_empty());
    let constant = Input::Constant {
        source: "nika: copy\n".to_owned(),
        name: "limit".to_owned(),
        literal: "72".to_owned(),
    };
    assert_eq!(constant.trial(), None);
    let request = constant.request(&BTreeMap::new());
    assert_eq!(request.workflow_id, None);
    assert_eq!(request.original_intent, None);
}
