// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A refusal carries its material-clause ledger into the decision record (R4 S0): the
//! contradicted effect, the unsupported work and every clause beside them, typed with their
//! states, not only a plan and a sentence. Measured on 007592ab9: « write 'hello' to ./a.txt
//! but do not write anything » refused with `provenance.decision` holding no ledger, and « Read
//! ./tickets.json, keep only the open rows and write them to ./open.json, but never write
//! anything » answered with the reader's admission finding, the contradiction left unstated.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::outcome_document;
use nika_compile::{CompileOutcome, CompileRequest, CompileStatus, DiagnosticKind, compile};
use serde_json::Value;

/// (kind, state, evidence) of every duty the decision record carries.
fn ledger(out: &CompileOutcome) -> Vec<(String, String, String)> {
    let doc = outcome_document(out);
    let duties = doc["provenance"]["decision"]["ledger"].as_array().cloned();
    assert!(
        duties.is_some(),
        "no ledger in the decision record: {doc:#}"
    );
    duties
        .unwrap()
        .iter()
        .map(|d| {
            let word = |key: &str| d[key].as_str().unwrap_or_default().to_owned();
            (word("kind"), word("state"), word("evidence"))
        })
        .collect()
}

fn contradiction_stated(intent: &str, out: &CompileOutcome) {
    assert_eq!(out.status, CompileStatus::Refused, "{intent}: {out:#?}");
    assert!(out.candidate.is_none(), "{intent}: {out:#?}");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.kind == DiagnosticKind::RequiresHuman
                && d.message
                    .starts_with("Contradictory instructions for `write`")),
        "{intent}: {out:#?}"
    );
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.message.contains("Permit an authoring model")),
        "{intent}: a model is offered the contradiction: {out:#?}"
    );
}

#[test]
fn a_refused_contradiction_records_its_ledger() {
    for intent in [
        "write 'hello' to ./a.txt but do not write anything",
        "write \"hello\" to ./a.txt and never write anything",
        "écris « bonjour » dans ./a.txt mais n'écris rien",
    ] {
        let out = compile(&CompileRequest::create(intent)).unwrap();
        contradiction_stated(intent, &out);
        let duties = ledger(&out);
        let contradicted: Vec<_> = duties
            .iter()
            .filter(|(kind, state, _)| kind == "effect" && state == "contradicted")
            .collect();
        assert_eq!(contradicted.len(), 1, "{intent}: {duties:?}");
        let sides: Vec<&str> = contradicted[0].2.split(" / ").collect();
        assert_eq!(sides.len(), 2, "{intent}: {duties:?}");
    }
}

#[test]
fn a_contradiction_beside_an_unsettled_clause_is_stated_with_it() {
    let intent = "Read ./tickets.json, keep only the open rows and write them to ./open.json, but never write anything";
    let out = compile(&CompileRequest::create(intent)).unwrap();
    contradiction_stated(intent, &out);
    let duties = ledger(&out);
    assert!(
        duties
            .iter()
            .any(|(kind, state, _)| kind == "effect" && state == "contradicted"),
        "{duties:?}"
    );
    assert!(
        duties
            .iter()
            .any(|(_, state, evidence)| state == "unresolved"
                && evidence == "keep only the open rows"),
        "the unsettled clause stays in the ledger: {duties:?}"
    );
}

#[test]
fn a_refused_unknown_records_its_ledger() {
    // An approval-bypass wording is work the compiler refuses to carry: named, and ledgered.
    let intent = "Read ./notes.md and write it to ./out.md without approval";
    let out = compile(&CompileRequest::create(intent)).unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    let duties = ledger(&out);
    assert!(
        duties
            .iter()
            .any(|(kind, state, _)| kind == "work" && state == "unsupported"),
        "{duties:?}"
    );
    let doc: Value = outcome_document(&out);
    assert!(doc["candidate"].is_null(), "{doc:#}");
}
