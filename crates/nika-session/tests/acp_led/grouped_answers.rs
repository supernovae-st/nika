// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;

/// A question in the same group keeps its identity after the agent finishes an intervening
/// turn without calling ask again. The repeated option key answers only the named question.
#[test]
fn bound_grouped_answers_survive_an_intervening_reply_without_ask() {
    let question = |key: &str, source: &str| {
        json!({"key": key, "question": format!("Use {source}?"), "free_text": false,
            "options": [{"key": "use", "label": "Use this source",
                "values": [{"role": "read_source", "value": source}]}]})
    };
    let asks = json!({"questions": [
        question("first_source", HACKER_NEWS), question("second_source", TECHCRUNCH)
    ]});
    let world = World::new(Some(&json!([
        [call("ask_sources", "ask", &asks)],
        [{"say": "The first source is selected."}],
        [{"say": "Both sources are selected."}]
    ])));
    let report = world.run("grouped_answers");
    let steps = report["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 3);
    let first_questions = &steps[0]["shown"]["work"]["questions"];
    assert_eq!(first_questions.as_array().unwrap().len(), 2);
    let second_id = &first_questions[1]["id"];
    let partial = &steps[1]["shown"]["work"];
    let remaining: Vec<_> = partial["questions"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|q| q["state"] == "open")
        .collect();
    assert_eq!(remaining.len(), 1, "{partial:#}");
    assert_eq!(remaining[0]["key"], "second_source");
    assert_eq!(&remaining[0]["id"], second_id, "no replacement ask");
    let first = partial["bindings"].as_array().unwrap();
    assert_eq!(
        first.len(),
        1,
        "the first answer cannot bind both questions"
    );
    assert_eq!(first[0]["value"], HACKER_NEWS);
    assert_eq!(first[0]["provenance"]["question"], "first_source");
    assert_eq!(first[0]["provenance"]["message"], "u2");

    let complete = &steps[2]["shown"]["work"];
    let bindings = complete["bindings"].as_array().unwrap();
    assert_eq!(bindings.len(), 2, "{complete:#}");
    for (value, question, message) in [
        (HACKER_NEWS, "first_source", "u2"),
        (TECHCRUNCH, "second_source", "u3"),
    ] {
        let bound = bindings.iter().find(|b| b["value"] == value).unwrap();
        assert_eq!(bound["role"], "read_source");
        assert_eq!(
            bound["provenance"],
            json!({"kind": "offered",
            "message": message, "question": question, "option": "use"})
        );
    }
    // Work omits an empty question list from its wire; no waiting identity survives it.
    assert_eq!(complete.get("questions"), None);
    assert_eq!(steps[2]["shown"]["waiting"], json!({"kind": "free"}));
    assert_eq!(steps[2]["shown"]["question_id"], Value::Null);
    assert_eq!(steps[2]["outcome"]["text"], "Both sources are selected.");
    assert_eq!(world.observed("prompts").len(), 3);
    assert_eq!(
        world
            .observed("replies")
            .iter()
            .filter(|reply| reply["call"] == "ask")
            .count(),
        1
    );
    assert!(workflows(&world.path("project")).is_empty());
}
