// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What each task of a judged candidate touches, as the checker infers it: a task invoking a
//! quiet tool that reads only paths the request names and writes only the output a part the
//! judge carried states (a path no task reads) is settled; every other task stays open, a write
//! through a constant resolved as the runtime resolves it.

use super::{EFFECTS, Effects};
use serde_json::{Value, json};

/// The request: read a source, keep some rows, save the output it names.
const INTENT: &str =
    "Read ./in/rows.json, keep only the open rows, save ./out/open.json as a list.";
/// The request's parts, as the verifier asks them alone.
const PARTS: [&str; 3] = [
    "Read ./in/rows.json",
    "keep only the open rows",
    "save ./out/open.json as a list",
];

/// A candidate whose write goes through the constant `output` (`./out/open.json` or another
/// path), as the drafts the native door accepts write it.
fn candidate(output: &str) -> String {
    format!(
        r#"nika: open-rows
const:
  source: ./in/rows.json
  output: {output}
permits:
  tools: ["nika:read", "nika:jq", "nika:write"]
  fs: {{ read: ["./in/rows.json"], write: ["{output}"] }}
tasks:
  load:
    invoke: {{ tool: "nika:read", args: {{ path: "${{{{ const.source }}}}" }} }}
  keep:
    with: {{ rows: "${{{{ tasks.load.output }}}}" }}
    invoke: {{ tool: "nika:jq", args: {{ input: "${{{{ with.rows }}}}", expression: "fromjson | map(select(.open))" }} }}
  save:
    with: {{ rows: "${{{{ tasks.keep.output }}}}" }}
    invoke: {{ tool: "nika:write", args: {{ path: "${{{{ const.output }}}}", content: "${{{{ with.rows }}}}", overwrite: true, create_dirs: true }} }}
"#
    )
}

/// The verdict's records so far: each part of [`PARTS`] answered as `answers` says, each record
/// naming its part as the verifier annotates it.
fn answered(answers: [&str; 3]) -> Vec<Value> {
    (PARTS.iter().zip(answers).enumerate())
        .map(|(k, (part, choice))| {
            json!({"question": format!("verify-part-{k}"), "choice": choice,
                "clause": {"text": part, "restricts": false}})
        })
        .collect()
}

/// A candidate reading the request's source and writing, through a constant, the output a
/// carried part states leaves no task open: the read is named, the jq program has no effect,
/// and the write is that part's output, a path no task reads. What settles each is stated.
#[test]
fn a_constant_write_of_the_output_a_carried_part_states_is_settled() {
    let effects = Effects::of(&candidate("./out/open.json"), INTENT).expect("it parses");
    let carried = answered(["carried"; 3]);
    assert_eq!(effects.open(&carried), Vec::<String>::new());
    let settled = effects.settled(&carried);
    assert_eq!(settled["question"], "verify-extra");
    assert_eq!(settled["settled"], "only_requested");
    assert_eq!(settled["by"], "engine");
    let expected = json!([
        {"task": "load", "reads": ["./in/rows.json"], "writes": [],
            "settled": {"reads": ["./in/rows.json"], "writes": []}},
        {"task": "keep", "reads": [], "writes": [], "settled": {"reads": [], "writes": []}},
        {"task": "save", "reads": [], "writes": ["./out/open.json"],
            "settled": {"reads": [], "writes": [{"path": "./out/open.json", "part": PARTS[2]}]}},
    ]);
    assert_eq!(settled["effects"], expected);
}

/// The write stays open when no part the judge carried states its output: the part asking it
/// answered otherwise, or never asked.
#[test]
fn a_write_no_carried_part_states_stays_open() {
    let effects = Effects::of(&candidate("./out/open.json"), INTENT).expect("it parses");
    for answers in [
        ["carried", "carried", "no_operation"],
        ["carried", "carried", "superseded"],
        ["carried", "carried", "missing"],
    ] {
        assert_eq!(effects.open(&answered(answers)), ["save"], "{answers:?}");
    }
    assert_eq!(effects.open(&[]), ["save"], "no part answered yet");
}

/// A write through a constant to a path the request never names is open: the checker resolves
/// the constant, so the write is never mistaken for one with no effect. So is a write over a
/// path a task reads (the request's own source), even one the request names.
#[test]
fn an_unnamed_constant_write_and_a_write_over_a_read_path_stay_open() {
    let carried = answered(["carried"; 3]);
    let elsewhere = Effects::of(&candidate("./out/elsewhere.json"), INTENT).expect("it parses");
    assert_eq!(elsewhere.open(&carried), ["save"]);
    let mut state = json!({});
    let _ = elsewhere.show(&mut state, &carried, "");
    assert_eq!(
        state["effects"][2]["writes"],
        json!(["./out/elsewhere.json"])
    );
    assert_eq!(state["effects"][2]["settled"], Value::Null);
    let over = Effects::of(&candidate("./in/rows.json"), INTENT).expect("it parses");
    assert_eq!(over.open(&carried), ["save"], "the source is read");
}

/// Another verb, a nested workflow, a tool with an effect of its own (a gate asking a person,
/// the network) and a path known only at run time each leave their task open.
#[test]
fn another_verb_an_effectful_tool_or_a_computed_path_stays_open() {
    let candidate = r#"nika: open-tasks
model: mock/echo
inputs:
  destination: { type: string }
permits:
  tools: ["nika:read", "nika:prompt", "nika:fetch", "nika:write"]
  fs: { read: ["./in/rows.json"], write: ["./out/**"] }
  net: { http: ["api.example.test"] }
  exec: ["ls"]
tasks:
  load:
    invoke: { tool: "nika:read", args: { path: "./in/rows.json" } }
  ask:
    invoke: { tool: "nika:prompt", args: { mode: confirm, message: "Proceed?" } }
  fetch:
    invoke: { tool: "nika:fetch", args: { url: "https://api.example.test/rows" } }
  list:
    exec: { command: ["ls"] }
  think:
    infer: { prompt: "Summarize the rows" }
  save:
    invoke: { tool: "nika:write", args: { path: "${{ inputs.destination }}", content: "x" } }
"#;
    let effects = Effects::of(candidate, INTENT).expect("it parses");
    let carried = answered(["carried"; 3]);
    let open = ["ask", "fetch", "list", "think", "save"];
    assert_eq!(effects.open(&carried), open);
}

/// The facts are shown as data beside the question, with what they mean; a candidate that does
/// not parse gives none.
#[test]
fn the_facts_are_shown_with_what_they_mean_and_none_without_a_parse() {
    let effects = Effects::of(&candidate("./out/open.json"), INTENT).expect("it parses");
    let mut state = json!({"request": INTENT});
    let told = effects.show(&mut state, &answered(["carried"; 3]), "Name the task.");
    assert_eq!(told, format!("Name the task. {EFFECTS}"));
    assert_eq!(state["request"], INTENT);
    let tasks: Vec<&Value> = (state["effects"].as_array().into_iter().flatten())
        .map(|task| &task["task"])
        .collect();
    assert_eq!(tasks, [&json!("load"), &json!("keep"), &json!("save")]);
    assert!(Effects::of("tasks: [", INTENT).is_none());
}
