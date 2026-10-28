// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! #1605 · a mock model replaces the MODEL route, never a tool. The ACCESS answer says only
//! that, and the tasks whose own effects can still reach the network are named on the typed
//! layers, under the mock lane and in the JSON verdicts.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::{Value, json};

use super::*;

/// The file as `nika check --model mock/echo` judges it.
fn rehearsal(source: &str) -> Audit {
    audit_source(
        source,
        "w.nika",
        None,
        None,
        AuditOptions::new(Some("mock/echo"), None),
    )
    .expect("the fixture parses")
}

fn verdicts(audit: &Audit) -> Value {
    let obj = audit_json(
        &audit.wf,
        &audit.report,
        &audit.skills,
        &audit.verdict,
        Lanes::default(),
    )
    .expect("serializes");
    obj["verdicts"].clone()
}

/// The #1605 shape: a fetch to a real host, then an `infer:` the rehearsal seats on the mock.
const FETCH_BRIEF: &str = "nika: fetch-brief\nmodel: openai/gpt-5.2\npermits:\n  net: { http: \
    [\"jsonplaceholder.typicode.com\"] }\n  tools: [\"nika:fetch\"]\ntasks:\n  grab:\n    \
    invoke:\n      tool: \"nika:fetch\"\n      args: { url: \
    \"https://jsonplaceholder.typicode.com/todos/1\" }\n  brief:\n    with:\n      page: \
    ${{ tasks.grab.output }}\n    infer:\n      prompt: \"Summarise ${{ with.page }}\"\n      \
    max_tokens: 200\n";

/// The ACCESS row read « mock · never dials · nothing to judge » over a fetch that dialed.
#[test]
fn a_mock_lane_says_its_route_never_dials_and_names_what_still_can() {
    let judged = rehearsal(FETCH_BRIEF);
    let layers = &judged.verdict.layers;
    assert_eq!(layers.network_tasks, ["grab"]);
    assert_eq!(layers.access_ready, Some(true), "{:?}", layers.access_lines);
    assert!(
        layers.access_lines[0].starts_with("mock/echo → mock")
            && layers.access_lines[0].contains("· mock · this model route never dials"),
        "{:?}",
        layers.access_lines
    );
    assert_eq!(
        layers.access_lines.last().map(String::as_str),
        Some(
            "a mock model stubs no tool: `grab` can still reach the network, judged on the \
             PERMITS row"
        )
    );
    assert!(
        !(layers.access_lines.iter()).any(|line| line.contains("nothing to judge")),
        "{:?}",
        layers.access_lines
    );
    let verdicts = verdicts(&judged);
    assert_eq!(verdicts["network_tasks"], json!(["grab"]), "{verdicts}");
    assert_eq!(verdicts["access_ready"], json!(true), "{verdicts}");

    // Off the mock the scope line has no subject, and the tasks are still named.
    let routed = audit_source(FETCH_BRIEF, "w.nika", None, None, AuditOptions::default())
        .expect("the fixture parses");
    assert_eq!(routed.verdict.layers.network_tasks, ["grab"]);
    assert!(
        !(routed.verdict.layers.access_lines.iter()).any(|line| line.contains("stubs no tool")),
        "{:?}",
        routed.verdict.layers.access_lines
    );
}

/// A builtin-only file (the ACCESS question is moot) still names its fetch; a documentation
/// host the transport never dials, and a write, are not network effects.
#[test]
fn a_moot_file_names_its_network_tasks_and_never_a_documentation_host() {
    let fetch = |host: &str| {
        format!(
            "nika: grab\npermits:\n  net: {{ http: [\"{host}\"] }}\n  tools: [\"nika:fetch\", \
             \"nika:write\"]\n  fs: {{ write: [\"./out/**\"] }}\ntasks:\n  grab:\n    invoke:\n      \
             tool: \"nika:fetch\"\n      args: {{ url: \"https://{host}/todos/1\" }}\n  save:\n    \
             invoke:\n      tool: \"nika:write\"\n      args: {{ path: \"./out/a.txt\", content: \
             \"hi\" }}\n"
        )
    };
    let live = rehearsal(&fetch("jsonplaceholder.typicode.com"));
    assert!(live.verdict.layers.access_moot);
    assert_eq!(live.verdict.layers.access_ready, None);
    assert_eq!(live.verdict.layers.network_tasks, ["grab"]);
    assert!(
        live.verdict.layers.access_lines.is_empty(),
        "no lane, no ACCESS row: {:?}",
        live.verdict.layers.access_lines
    );
    assert_eq!(verdicts(&live)["network_tasks"], json!(["grab"]));

    let documentation = rehearsal(&fetch("api.example.com"));
    assert!(
        documentation.verdict.layers.network_tasks.is_empty(),
        "{:?}",
        documentation.verdict.layers.network_tasks
    );
    assert_eq!(verdicts(&documentation)["network_tasks"], json!([]));
}

/// An MCP tool and a paid media call reach out on their own too; a local write does not.
#[test]
fn an_mcp_tool_and_a_paid_media_call_are_network_tasks() {
    let judged = rehearsal(
        "nika: mixed\npermits:\n  tools: [\"mcp:service/search\", \"nika:tts_generate\", \
         \"nika:write\"]\n  fs: { write: [\"./out/**\"] }\ntasks:\n  look:\n    invoke:\n      \
         tool: \"mcp:service/search\"\n      args: { query: \"nika\" }\n  say:\n    invoke:\n      \
         tool: \"nika:tts_generate\"\n      args: { model: tts-1, text: hello, output_dir: ./out \
         }\n  note:\n    invoke:\n      tool: \"nika:write\"\n      args: { path: \"./out/n.txt\", \
         content: \"hi\" }\n",
    );
    assert_eq!(judged.verdict.layers.network_tasks, ["look", "say"]);
}
