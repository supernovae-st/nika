// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! NIK-266 · the `harness` hint and the bridged drift, judged through the ONE door.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::Value;

use super::*;

fn audit(source: &str) -> Audit {
    audit_source(source, "w.nika", None, None, AuditOptions::default()).expect("the fixture parses")
}

/// The NIK-266 qualification workflow (P4): an `agent:` task the run declares on Codex over
/// ACP, beside two builtin consumers. A tools-whitelisted twin is native-only.
const HARNESS_AGENT: &str = "nika: codex-image\nmodel: openai/gpt-6-sol\nrun:\n  access:\n    \
    via: codex\n    protocol: acp\npermits:\n  fs:\n    read: [\"./out/codex-image.png\"]\n    \
    write: [\"./out/codex-image.png\"]\n  exec: [\"cp\", \"shasum\"]\n  tools: [\"nika:read\"]\n\
    tasks:\n  draw:\n    agent:\n      prompt: \"copy the image to ./out\"\n      max_turns: 6\n  \
    digest:\n    after: { draw: success }\n    \
    exec: { command: [\"shasum\", \"-a\", \"256\", \"./out/codex-image.png\"] }\n  \
    measure:\n    after: { draw: success }\n    \
    invoke: { tool: \"nika:read\", args: { path: \"./out/codex-image.png\" } }\n";

/// NIK-266 · the harness agent task is told what the boundary covers and what it does not,
/// on the typed verdict, the machine rows and the rendered report; the drift pass keeps the
/// `cp` grant the bridge judges (it used to say "remove the entry"); a native-only twin
/// gets no harness hint and its `cp` drift stands.
#[test]
fn a_harness_agent_task_is_told_its_boundary_and_keeps_its_bridged_grants() {
    let seated = audit(HARNESS_AGENT);
    let hints: Vec<(&str, &str, &str)> = (seated.verdict.harness.iter())
        .map(|h| (h.kind, h.task.as_str(), h.advice.as_str()))
        .collect();
    assert_eq!(hints.len(), 1, "{hints:?}");
    assert_eq!((hints[0].0, hints[0].1), ("harness", "draw"));
    assert_eq!(
        hints[0].2,
        nika_types::access::HarnessBoundary::CODEX_ACP.check_advice(true)
    );
    assert!(
        hints[0].2.contains("in mode `read-only`")
            && hints[0].2.contains("reads any file")
            && hints[0].2.contains("not only the fs.write paths"),
        "{}",
        hints[0].2
    );
    assert!(
        !seated.verdict.drift.iter().any(|d| d.contains("`cp`")),
        "{:?}",
        seated.verdict.drift
    );
    let obj = audit_json(
        &seated.wf,
        &seated.report,
        &seated.skills,
        &seated.verdict,
        Lanes::default(),
    )
    .expect("serializes");
    let rows: Vec<&Value> = (obj["hints"].as_array().expect("hints"))
        .iter()
        .filter(|row| row["kind"] == "harness")
        .collect();
    assert_eq!(rows.len(), 1, "{:?}", obj["hints"]);
    assert_eq!(rows[0]["task"], "draw");
    assert_eq!(rows[0]["advice"], hints[0].2);
    let shown = seated.verdict.report_with_hints(&seated.report);
    assert_eq!(shown.hints.len(), seated.report.hints.len() + 1);
    assert!(
        shown
            .hints
            .iter()
            .any(|h| h.kind == "harness" && h.task == "draw")
    );

    let native = audit(&HARNESS_AGENT.replace(
        "      max_turns: 6\n",
        "      max_turns: 6\n      tools: [\"nika:read\"]\n",
    ));
    assert!(
        native.verdict.harness.is_empty(),
        "a whitelist is native-only"
    );
    assert!(
        native
            .verdict
            .drift
            .iter()
            .any(|d| d.contains("`cp` admits a program no task runs")),
        "{:?}",
        native.verdict.drift
    );
}
