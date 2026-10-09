// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The localization asked of a doubt nothing located: shown its own answer to each part and what
//! each task touches, the judge may name a part, an open task or nothing, never carry the request.

use super::{DOUBT, Doubt, Located, UNLOCATED};
use crate::judge::Effects;
use serde_json::{Value, json};

/// The request and its parts.
const INTENT: &str =
    "Read ./in/rows.json, keep only the open rows, save ./out/open.json as a list.";
const PARTS: [&str; 3] = [
    "Read ./in/rows.json",
    "keep only the open rows",
    "save ./out/open.json as a list",
];
/// A candidate whose `send` posts the rows: the one task the facts leave open.
const CANDIDATE: &str = r#"nika: open-rows
permits:
  tools: ["nika:read", "nika:jq", "nika:write", "nika:fetch"]
  fs: { read: ["./in/rows.json"], write: ["./out/open.json"] }
  net: { http: ["api.example.test"] }
tasks:
  load:
    invoke: { tool: "nika:read", args: { path: "./in/rows.json" } }
  keep:
    with: { rows: "${{ tasks.load.output }}" }
    invoke: { tool: "nika:jq", args: { input: "${{ with.rows }}", expression: "fromjson" } }
  save:
    with: { rows: "${{ tasks.keep.output }}" }
    invoke: { tool: "nika:write", args: { path: "./out/open.json", content: "${{ with.rows }}" } }
  send:
    invoke: { tool: "nika:fetch", args: { url: "https://api.example.test/rows" } }
"#;

/// Every part answered carried, as the verifier records each.
fn carried() -> Vec<Value> {
    (PARTS.iter().enumerate())
        .map(|(k, part)| {
            json!({"question": format!("verify-part-{k}"), "choice": "carried",
                "clause": {"text": part, "restricts": false}})
        })
        .collect()
}

/// The question shows the judge its own answer to each part and what each task touches, and
/// offers each part, each open task and `unlocated`: never an option carrying the request.
#[test]
fn the_question_shows_its_own_answers_and_offers_parts_open_tasks_or_nothing() {
    let effects = Effects::of(CANDIDATE, INTENT).expect("it parses");
    let records = carried();
    let open = effects.open(&records);
    assert_eq!(open, ["send"]);
    let doubt = Doubt::new(PARTS.map(str::to_owned).to_vec(), open);
    let state = json!({"request": INTENT, "candidate_nika": CANDIDATE});
    let (shown, told, options) = doubt.question(&state, &effects, &records);
    let keys: Vec<&str> = options.iter().map(|option| option.key.as_str()).collect();
    assert_eq!(
        keys,
        ["part-0", "part-1", "part-2", "task-send", "unlocated"]
    );
    assert_eq!(options[1].description, PARTS[1]);
    assert_eq!(options[4].description, UNLOCATED);
    let parts = json!([
        {"part": 0, "text": PARTS[0], "answer": "carried"},
        {"part": 1, "text": PARTS[1], "answer": "carried"},
        {"part": 2, "text": PARTS[2], "answer": "carried"},
    ]);
    assert_eq!(shown["parts"], parts);
    assert_eq!(shown["effects"][3]["task"], "send");
    assert_eq!(shown["effects"][3]["settled"], Value::Null);
    assert_eq!(shown["request"], INTENT);
    assert!(told.starts_with(DOUBT), "{told}");
    assert!(told.contains("`effects` lists what each task"), "{told}");
    assert!(!keys.contains(&"faithful") && !keys.contains(&"carried"));
}

/// What an answer locates: a part of the request, an open task, or nothing; a task the facts
/// settle, a part out of range or any other key is no answer of this question.
#[test]
fn an_answer_locates_a_part_an_open_task_or_nothing() {
    let doubt = Doubt::new(PARTS.map(str::to_owned).to_vec(), vec!["send".to_owned()]);
    assert_eq!(doubt.read("part-2"), Some(Located::Part(2)));
    assert_eq!(
        doubt.read("task-send"),
        Some(Located::Task("send".to_owned()))
    );
    assert_eq!(doubt.read("unlocated"), Some(Located::Nothing));
    for refused in [
        "part-3",
        "part-x",
        "task-save",
        "carried",
        "faithful",
        "none",
    ] {
        assert_eq!(doubt.read(refused), None, "{refused}");
    }
}
