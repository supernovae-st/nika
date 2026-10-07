// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;
use serde_json::json;

const INTENT: &str = "Read ./calendar.json and the files in ./reports/, then save ./out/r.json";

fn row(extra: &Value) -> Value {
    let mut row = json!({"path": "./calendar.json", "state": "observed", "kind": "json"});
    if let (Some(row), Some(extra)) = (row.as_object_mut(), extra.as_object()) {
        row.extend(extra.clone());
    }
    json!({ "observed": [row] })
}

#[test]
fn a_path_the_request_does_not_state_is_refused() {
    let world = |path: &str| json!({"observed": [{"path": path, "state": "absent"}]});
    assert_eq!(
        admit_observation(INTENT, &world("./reports/july.csv")),
        Ok(())
    );
    assert_eq!(admit_observation(INTENT, &world("./out/r.json")), Ok(()));
    for unstated in [
        "./secrets.json",
        "./reports/../x.csv",
        "./reports/a/b.csv",
        "./reports/notes.md",
        "/calendar.json",
    ] {
        assert_eq!(
            admit_observation(INTENT, &world(unstated)),
            Err(Refusal::Unstated),
            "{unstated}"
        );
    }
}

#[test]
fn rows_and_values_beyond_the_observers_shape_are_refused() {
    assert_eq!(admit_observation(INTENT, &row(&json!({}))), Ok(()));
    for bad in [
        json!({"rows": [{"id": "a"}]}),
        json!({"state": "shouted"}),
        json!({"kind": "xml"}),
        json!({"columns": [1]}),
        json!({"values": {"note": ["a long free-text value well past thirty-two chars"]}}),
        json!({"values": {"s": [{"nested": true}]}}),
        json!({"peek_sha256": "not-a-digest"}),
    ] {
        assert_eq!(
            admit_observation(INTENT, &row(&bad)),
            Err(Refusal::Shape),
            "{bad}"
        );
    }
    let empty = json!({"observed": [], "kinds": {}});
    assert_eq!(admit_observation(INTENT, &empty), Err(Refusal::Shape));
    let stray = json!({"observed": [{"path": "./calendar.json", "state": "absent"}],
        "kinds": {"./other.json": {}}});
    assert_eq!(admit_observation(INTENT, &stray), Err(Refusal::Shape));
    let twice = json!({"observed": [{"path": "./calendar.json", "state": "absent"},
        {"path": "./calendar.json", "state": "absent"}]});
    assert_eq!(admit_observation(INTENT, &twice), Err(Refusal::Shape));
}

#[test]
fn an_oversized_observation_is_refused() {
    let wide: Vec<String> = (0..20_000).map(|n| format!("column_{n}")).collect();
    let world = row(&json!({ "columns": wide }));
    assert_eq!(admit_observation(INTENT, &world), Err(Refusal::Oversize));
    let many: Vec<Value> = (0..=OBSERVATION_ROWS)
        .map(|n| json!({"path": format!("./reports/{n}.csv"), "state": "absent"}))
        .collect();
    assert_eq!(
        admit_observation(INTENT, &json!({ "observed": many })),
        Err(Refusal::Oversize)
    );
}

#[test]
fn only_the_observed_files_within_the_bound_are_trial_inputs() {
    let world = json!({"observed": [
        {"path": "./a.json", "state": "observed"},
        {"path": "./gone.json", "state": "absent"}]});
    let file = |path: &str| json!({"files": [{"path": path, "text": "[]"}]});
    assert_eq!(admit_trial(&world, &file("./a.json")), Ok(()));
    for refused in ["./gone.json", "./b.json", "../a.json", "/a.json"] {
        assert_eq!(
            admit_trial(&world, &file(refused)),
            Err(Refusal::TrialShape),
            "{refused}"
        );
    }
    let twice = json!({"files": [{"path": "./a.json", "text": ""},
        {"path": "./a.json", "text": ""}]});
    assert_eq!(admit_trial(&world, &twice), Err(Refusal::TrialShape));
    let extra = json!({"files": [{"path": "./a.json", "text": "", "mode": 7}]});
    assert_eq!(admit_trial(&world, &extra), Err(Refusal::TrialShape));
    let bound = usize::try_from(TRIAL_BYTES).unwrap();
    let big = json!({"files": [{"path": "./a.json", "text": "x".repeat(bound + 1)}]});
    assert_eq!(admit_trial(&world, &big), Err(Refusal::TrialOversize));
}

/// The door's one entry: JSON texts, a repeated key refused at any depth, both laws applied,
/// and each refusal carrying the code a door answers.
#[test]
fn observed_admits_the_texts_and_names_each_refusal_code() {
    let world = r#"{"observed":[{"path":"./calendar.json","state":"observed"}]}"#;
    let trial = r#"{"files":[{"path":"./calendar.json","text":"{}"}]}"#;
    let admitted = Observed::admit(INTENT, world, Some(trial)).unwrap();
    assert_eq!(admitted.world["observed"][0]["path"], "./calendar.json");
    assert!(admitted.trial.is_some());
    let repeated = r#"{"observed":[{"path":"./calendar.json","state":"a","state":"b"}]}"#;
    assert_eq!(
        Observed::admit(INTENT, repeated, None),
        Err(Refusal::Malformed)
    );
    assert_eq!(Refusal::Malformed.code(), "malformed_compile_request");
    assert_eq!(Refusal::Unstated.code(), "compile_observation_refused");
    assert_eq!(Refusal::TrialShape.code(), "compile_trial_inputs_refused");
    assert!(repeats_a_key(r#"{"a":[{"b":1,"b":2}]}"#));
    assert!(!repeats_a_key(r#"{"a":[{"b":1},{"b":2}]}"#));
}
