// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A seat read step that absorbs a second requested operation: the region it
//! quotes is not produced, so the COLD merge leaves the work asked instead of assembling a
//! whole-file copy. A genuine copy and a separate lookup step keep their reading. All seats
//! are hermetic doubles; the first fixture is the recorded seat answer of a historical run.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{AuthoringPolicy, CompileRequest, CompileStatus, HotPolicy, NativeMode};
use nika_compile_cognition::compile_with_provider;
use serde_json::{Value, json};
use std::time::Duration;

mod common;
use common::{Rotating, keys};

const FIND: &str = "Read ./tickets.json, find ticket 42 and write it to ./ticket-42.json";

fn cold_only() -> AuthoringPolicy {
    AuthoringPolicy::new("mock/authoring", 4096, Duration::from_secs(2))
        .with_native(NativeMode::Off)
}

/// A recorded provider plan that copied the entire input instead of selecting the ticket.
const RECORDED_ABSORBED: &str = include_str!("fixtures/absorbed_read_plan.json");

fn absorbed_findings(out: &nika_compile::CompileOutcome) -> Vec<String> {
    out.diagnostics
        .iter()
        .filter(|d| d.message.contains("a read performs no such work"))
        .map(|d| d.message.clone())
        .collect()
}

#[tokio::test]
async fn the_recorded_absorbed_read_is_not_a_whole_file_copy() {
    let seat = Rotating::new(vec![RECORDED_ABSORBED.to_owned()]);
    let request = CompileRequest::create(FIND).with_authoring_policy(cold_only());
    let out = compile_with_provider(&request, &seat).await.unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    let findings = absorbed_findings(&out);
    assert!(
        findings.iter().any(|m| m.contains("find ticket 42")),
        "the absorbed clause is named: {out:#?}"
    );
    // The existing unresolved-work contract answers it: the request is handed back, and under
    // the default escalate strategy this very question opens the native door.
    assert!(keys(&out).contains(&"intent.clarification"), "{out:#?}");
}

fn separate_lookup() -> Value {
    json!({
        "steps": [
            {"op": "read", "detail": "./tickets.json", "evidence": "Read ./tickets.json"},
            {"op": "lookup", "detail": "ticket 42", "evidence": "find ticket 42"}
        ],
        "effects": [{"verb": "write", "target": "./ticket-42.json", "policy": "automatic",
                     "evidence": "write it to ./ticket-42.json"}],
        "obligations": [], "constraints": [], "unknowns": [],
        "regions": [
            {"role": "operation", "text": "Read ./tickets.json"},
            {"role": "operation", "text": "find ticket 42"},
            {"role": "effect", "text": "write it to ./ticket-42.json"}
        ]
    })
}

#[tokio::test]
async fn a_separate_lookup_step_realizes_the_clause_the_read_quotes() {
    // Same request; the read even quotes the whole sentence, but a lookup step and the write
    // effect realize everything after its path: nothing is absorbed.
    let mut plan = separate_lookup();
    plan["steps"][0]["evidence"] = json!(FIND);
    let seat = Rotating::new(vec![plan.to_string()]);
    let request = CompileRequest::create(FIND).with_authoring_policy(cold_only());
    let out = compile_with_provider(&request, &seat).await.unwrap();
    assert!(absorbed_findings(&out).is_empty(), "{out:#?}");
}

#[tokio::test]
async fn a_genuine_copy_keeps_its_read_and_write() {
    // HOT off forces the seat's reading of a plain copy; the read quotes the write clause,
    // which the write effect realizes.
    let copy = "Read ./tickets.json and write it to ./tickets-copy.json";
    let plan = json!({
        "steps": [{"op": "read", "detail": "./tickets.json", "evidence": "Read ./tickets.json"}],
        "effects": [{"verb": "write", "target": "./tickets-copy.json", "policy": "automatic",
                     "evidence": "write it to ./tickets-copy.json"}],
        "obligations": [], "constraints": [], "unknowns": [],
        "regions": [
            {"role": "operation", "text": "Read ./tickets.json"},
            {"role": "effect", "text": "write it to ./tickets-copy.json"}
        ]
    });
    for evidence in ["Read ./tickets.json", copy] {
        let mut plan = plan.clone();
        plan["steps"][0]["evidence"] = json!(evidence);
        let seat = Rotating::new(vec![plan.to_string()]);
        let request = CompileRequest::create(copy)
            .with_hot_policy(HotPolicy::Off)
            .with_authoring_policy(cold_only());
        let out = compile_with_provider(&request, &seat).await.unwrap();
        assert!(absorbed_findings(&out).is_empty(), "{evidence}: {out:#?}");
    }
}

#[tokio::test]
async fn an_absorbed_detail_is_caught_even_when_the_evidence_is_clean() {
    // The residue may ride the detail instead of the evidence (detail
    // « ./tickets.json, find ticket 42 »).
    let mut plan = separate_lookup();
    plan["steps"].as_array_mut().unwrap().remove(1);
    plan["steps"][0]["detail"] = json!("./tickets.json, find ticket 42");
    plan["regions"] = json!([
        {"role": "operation", "text": "Read ./tickets.json, find ticket 42"},
        {"role": "effect", "text": "write it to ./ticket-42.json"}
    ]);
    let seat = Rotating::new(vec![plan.to_string()]);
    let request = CompileRequest::create(FIND).with_authoring_policy(cold_only());
    let out = compile_with_provider(&request, &seat).await.unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(
        absorbed_findings(&out)
            .iter()
            .any(|m| m.contains("find ticket 42")),
        "{out:#?}"
    );
}
