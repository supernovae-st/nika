// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The shared effect law consumes the reader's scope, whatever door made the candidate.
use nika_compile_fidelity::fidelity::dropped_effects;
use nika_compile_reader::lexicon;
use serde_json::json;

#[test]
fn an_authoring_frame_does_not_require_an_effect_task() {
    let reading = lexicon::read("Create a new workflow named config-values");
    let document = json!({"tasks": {
        "report": {"invoke": {"tool": "nika:jq", "args": {"filter": "."}}}
    }});
    let mut findings = Vec::new();
    dropped_effects(&reading.plan, &document, &mut findings);
    assert_eq!(findings.len(), 0, "{findings:?}");
    assert_eq!(
        reading.unresolved,
        ["Create a new workflow named config-values"]
    );
}

#[test]
fn a_creation_inside_the_authored_program_is_still_owed() {
    for (intent, evidence) in [
        ("Create an invoice", "Create an invoice"),
        ("Create a user", "Create a user"),
        ("Create a file", "Create a file"),
        (
            "Create a workflow that creates an invoice",
            "creates an invoice",
        ),
        ("Crée un workflow qui crée une facture", "crée une facture"),
    ] {
        let mut findings = Vec::new();
        dropped_effects(
            &lexicon::read(intent).plan,
            &json!({"tasks": {}}),
            &mut findings,
        );
        assert_eq!(findings.len(), 1, "{intent}: {findings:?}");
        assert_eq!(findings[0].kind, "effect");
        assert_eq!(
            findings[0].message,
            format!(
                "DROPPED EFFECT: the request states `create` (« {evidence} ») and no task carries it — no `nika:fetch` beyond GET, no `nika:notify`, no `nika:emit`, no `mcp:` tool. Realize it (a destination the request leaves open is ONE `const.<name>_endpoint` question), never drop it."
            )
        );
    }
}
