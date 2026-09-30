// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Law 21 follows the reader. Once « request human confirmation before writing » reads as the
//! named gate it is (rebuilt here on synthetic readings), the seat's confirm prompt over the
//! write is the stated approval guarding it, never an invented one, and a candidate that drops
//! it is refused by the approval law.
use nika_compile_fidelity::fidelity::{Diagnostic, laws};
use nika_compile_reader::lexicon;
use serde_json::{Value, json};

const INTENT: &str = "Read ./data/readings.csv. Sum integer celsius of valid rows. If the total is below 100, finish without a report; otherwise display the total and request human confirmation before writing. The report written to ./out/result.json is a JSON object with integer total.";

/// read → convert → total → (review →) write, the write guarded by the review's yes when
/// `gated`.
fn candidate(gated: bool) -> Value {
    let mut doc = json!({
        "nika": "gated-total",
        "permits": {"fs": {"read": ["./data/readings.csv"], "write": ["./out/result.json"]},
                    "tools": ["nika:read", "nika:convert", "nika:jq", "nika:prompt", "nika:write"]},
        "tasks": {
            "read_source": {"invoke": {"tool": "nika:read", "args": {"path": "./data/readings.csv"}}},
            "parse_source": {"with": {"text": "${{ tasks.read_source.output }}"},
                "invoke": {"tool": "nika:convert", "args": {"input": "${{ with.text }}", "from": "csv", "to": "json"}}},
            "total": {"with": {"rows": "${{ tasks.parse_source.output }}"},
                "invoke": {"tool": "nika:jq", "args": {"input": "${{ with.rows }}", "expression": "{total: ([.[] | (.celsius | tonumber)] | add // 0)}"}}},
            "review": {"with": {"total": "${{ tasks.total.output }}"},
                "invoke": {"tool": "nika:prompt", "args": {"message": "Write the report? ${{ with.total }}"}}},
            "write_report": {"with": {"approved": "${{ tasks.review.output }}", "total": "${{ tasks.total.output }}"},
                "when": "${{ with.approved == true }}",
                "invoke": {"tool": "nika:write", "args": {"path": "./out/result.json", "content": "${{ with.total }}"}}}
        }
    });
    if !gated && let Some(tasks) = doc["tasks"].as_object_mut() {
        tasks.remove("review");
        tasks.insert(
            "write_report".to_owned(),
            json!({"with": {"total": "${{ tasks.total.output }}"},
                "invoke": {"tool": "nika:write", "args": {"path": "./out/result.json", "content": "${{ with.total }}"}}}),
        );
    }
    doc
}

/// The heads of the gate findings the laws return over `doc`, judged against the reader's
/// own plan of the request.
fn gate_heads(doc: &Value) -> Vec<String> {
    let plan = lexicon::read(INTENT).plan;
    let mut out: Vec<Diagnostic> = Vec::new();
    laws(INTENT, &plan, doc, &[], &[], &[], &mut out);
    let head = |d: &Diagnostic| d.message.split(':').next().unwrap_or_default().to_owned();
    out.iter().filter(|d| d.kind == "gate").map(head).collect()
}

#[test]
fn the_seats_gate_over_the_write_is_the_stated_approval() {
    assert_eq!(gate_heads(&candidate(true)), Vec::<String>::new());
}

#[test]
fn a_candidate_that_drops_the_gate_is_refused() {
    assert_eq!(gate_heads(&candidate(false)), ["MISSING APPROVAL"]);
}
