// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A revision check over a search reruns the search just before the action (eco-60 E09B on
//! the binary of 2026-09-22: « retrouve les passages pertinents du guide interne … Vérifie
//! de nouveau la version courante juste avant cette action » was refused for want of a
//! lookup). The hits are the version the answer was drafted from; changed hits are a
//! changed version. The plan below is replayed through the assembler with zero calls.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileRequest, compile};
use serde_json::Value;

mod common;

const INTENT: &str = "Quand une question arrive dans Slack, retrouve les passages pertinents du guide interne et rédige une réponse avec références. Poste ensuite la réponse sur https://hooks.example.invalid/faq. Mais cette action finale exige la validation humaine de ce dossier précis, avant son exécution. Vérifie de nouveau la version courante juste avant cette action.";

const PLAN: &str = r#"{"bindings": [{"literal": "https://hooks.example.invalid/faq", "role": "url"}], "constraints": [], "effects": [{"evidence": "Poste ensuite la réponse sur https://hooks.example.invalid/faq.", "policy": "human_first", "policy_literal": "cette action finale exige la validation humaine de ce dossier précis, avant son exécution", "target": "https://hooks.example.invalid/faq", "verb": "send"}], "obligations": [{"kind": "revision_check", "evidence": "Vérifie de nouveau la version courante juste avant cette action"}], "operations": [{"categories": [], "detail": "les passages pertinents du guide interne", "evidence": "retrouve les passages pertinents du guide interne", "op": "search"}, {"categories": [], "detail": "une réponse avec références", "evidence": "rédige une réponse avec références", "op": "draft"}], "rules": [], "strategy": "cold", "trigger": "Quand une question arrive dans Slack", "unknowns": []}"#;

#[test]
fn a_revision_check_over_a_search_reruns_the_search_before_the_action() {
    let plan: Value = serde_json::from_str(PLAN).unwrap();
    let out = compile(
        &CompileRequest::create(INTENT)
            .with_plan(plan)
            .answer("const.search_root", r#""./guide""#)
            .answer("model", r#""mock/echo""#),
    )
    .unwrap();
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.message.contains("no retrievable source")),
        "the search is the retrievable source: {out:#?}"
    );
    let source = out.candidate.as_deref().expect("a candidate is assembled");
    let doc: Value = serde_yaml_bw::from_str(source).unwrap();
    let reread = &doc["tasks"]["revision_reread"];
    assert_eq!(reread["invoke"]["tool"], "nika:grep", "{source}");
    assert_eq!(
        reread["invoke"]["args"]["path"], "${{ const.search_root }}",
        "{source}"
    );
    let stable = &doc["tasks"]["revision_stable"];
    assert!(
        stable["with"]["before"]
            .as_str()
            .unwrap_or_default()
            .contains("search_hits"),
        "the first hits are compared with the rerun: {source}"
    );
    assert_eq!(
        doc["tasks"]["revision_admit"]["invoke"]["tool"], "nika:assert",
        "{source}"
    );
    assert!(
        source.contains("revision_admit: success"),
        "the action waits for the recheck: {source}"
    );
}
