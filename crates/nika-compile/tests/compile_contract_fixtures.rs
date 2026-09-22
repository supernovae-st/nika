// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The contract fixtures the product lane reads: one recorded compile document per state
//! the wire really supports (ready · incomplete with a typed question · incomplete with a
//! clarification · refused · a schedule beside the candidate with its binding questions ·
//! a gated outbound effect · a skeleton hole · an edit). Each fixture holds the request that
//! produced it and the exact document; this test recompiles the request and refuses any
//! drift, so the fixtures are always the wire of this very compiler.
//!
//! Regenerate after a deliberate wire change with
//! `NIKA_CONTRACT_FIXTURES=write cargo test -p nika-compile --test compile_contract_fixtures`
//! and review the diff: a fixture is the contract, never a convenience.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]
use nika_compile::{CompileRequest, compile, outcome_document};
use serde_json::{Value, json};
use std::path::PathBuf;

struct Case {
    name: &'static str,
    state: &'static str,
    request: fn() -> CompileRequest,
}

const CASES: &[Case] = &[
    Case {
        name: "ready-filter-json",
        state: "ready · deterministic door · a filter and a write",
        request: || {
            CompileRequest::create(
                "Read ./tickets.json, keep only the rows whose status is open and write them to ./open.json",
            )
        },
    },
    Case {
        name: "ready-gated-send",
        state: "ready · an outbound send dominated by a human gate (nika:prompt)",
        request: || {
            CompileRequest::create(
                "Lis ./report.md et envoie-le à http://127.0.0.1:8768/notify, mais demande-moi avant d'envoyer",
            )
        },
    },
    Case {
        name: "incomplete-model",
        state: "incomplete · the one mandatory `model` question of a language step",
        request: || {
            CompileRequest::create(
                "Read ./notes/brief.md and write a 3-bullet summary to ./out/summary.md",
            )
        },
    },
    Case {
        name: "incomplete-typed-question",
        state: "incomplete · a typed hole (`const.source_glob`): a directory is never read as one file",
        request: || {
            CompileRequest::create(
                "Read each file in ./invoices, extract the vendor and the total from each and write the records to ./totals.json",
            )
        },
    },
    Case {
        name: "incomplete-clarification",
        state: "incomplete · the catch-all `intent.clarification` for a clause the reader cannot settle",
        request: || {
            CompileRequest::create(
                "Read ./tickets.json, count the open tickets and write the count to ./count.json",
            )
        },
    },
    Case {
        name: "refused-conflict",
        state: "refused · one effect both requested and prohibited",
        request: || {
            CompileRequest::create(
                "Extrais les coordonnées de chaque candidature et prépare un accusé de réception. Envoie ensuite une invitation. Il est aussi absolument interdit d'envoyer une invitation.",
            )
        },
    },
    Case {
        name: "schedule-trigger",
        state: "ready · a schedule stated beside the candidate (requested_trigger) with its four optional binding questions (two of them choices)",
        request: || {
            CompileRequest::create(
                "Every weekday at 8, read ./tickets.json, keep only the rows whose status is open and write them to ./open.json",
            )
        },
    },
    Case {
        name: "schedule-trigger-answered",
        state: "ready · the same schedule with its binding values answered and echoed",
        request: || {
            CompileRequest::create(
                "Every weekday at 8, read ./tickets.json, keep only the rows whose status is open and write them to ./open.json",
            )
            .answer("trigger.timezone", r#""Europe/Paris""#)
            .answer("trigger.missed", r#""rattraper-une-fois""#)
            .answer("trigger.overlap", r#""sauter""#)
            .answer("trigger.ceiling", "0.10")
        },
    },
    Case {
        name: "skeleton-question",
        state: "incomplete · an exact skeleton with its literal hole (`const.request`)",
        request: || CompileRequest::create("classify-and-route"),
    },
    Case {
        name: "edit-set-constant",
        state: "ready · an accepted source with one constant changed",
        request: || {
            let base = compile(
                &CompileRequest::create("classify-and-route")
                    .answer("const.request", r#""An outage affects our customers.""#),
            )
            .unwrap()
            .candidate
            .unwrap();
            CompileRequest::set_constant(base, "request", r#""One customer cannot log in.""#)
        },
    },
];

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("contract")
}

#[test]
fn every_contract_fixture_is_the_wire_of_this_compiler() {
    let write = std::env::var("NIKA_CONTRACT_FIXTURES").as_deref() == Ok("write");
    let dir = fixture_dir();
    if write {
        std::fs::create_dir_all(&dir).unwrap();
    }
    let mut drifted = Vec::new();
    for case in CASES {
        let out = compile(&(case.request)()).unwrap();
        let document = outcome_document(&out);
        let recorded = json!({
            "fixture": case.name,
            "state": case.state,
            "compile_version": document["compile_version"],
            "document": document,
        });
        let path = dir.join(format!("{}.json", case.name));
        if write {
            let mut text = serde_json::to_string_pretty(&recorded).unwrap();
            text.push('\n');
            std::fs::write(&path, text).unwrap();
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .expect("a recorded fixture (regenerate with NIKA_CONTRACT_FIXTURES=write)");
        let on_disk: Value = serde_json::from_str(&text).expect("a JSON fixture");
        if on_disk != recorded {
            drifted.push(case.name);
        }
    }
    assert!(
        drifted.is_empty(),
        "contract fixtures drifted from the wire: {drifted:?} (review, then regenerate with NIKA_CONTRACT_FIXTURES=write)"
    );
}

/// The fixtures cover every state the product lane renders, and each one names its state.
#[test]
fn the_fixture_set_covers_every_wire_state() {
    let statuses: Vec<&str> = CASES
        .iter()
        .map(|c| c.state.split(" · ").next().unwrap())
        .collect();
    for status in ["ready", "incomplete", "refused"] {
        assert!(statuses.contains(&status), "no fixture in state {status}");
    }
    assert!(CASES.iter().any(|c| c.name == "schedule-trigger"));
    assert!(CASES.iter().any(|c| c.name == "ready-gated-send"));
}
