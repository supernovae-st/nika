// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::unwrap_used)]
use super::*;
use crate::admission::{HardMonetaryCap, InferenceAdmission, UnknownCostChoice, UnknownCostPolicy};
use nika_kernel::ai::provider::{ContentBlock, InferResponse, StopReason, TokenUsage};
use std::time::Duration;
const URL: &str = "https://gateway.example/v1/chat/completions";
fn completed(invocation: &str) -> Value {
    let choice = UnknownCostChoice::new(
        "candidate".into(),
        invocation.into(),
        "openai".into(),
        "unpriced".into(),
        URL.into(),
        2,
        1024,
        Duration::from_secs(10),
    )
    .unwrap();
    let policy = UnknownCostPolicy::new(
        true,
        HardMonetaryCap::Absent,
        HardMonetaryCap::Absent,
        HardMonetaryCap::Absent,
        None,
        None,
    );
    let account = InferenceAdmission::new_unknown(choice, policy)
        .unwrap()
        .for_scope("candidate", invocation)
        .unwrap();
    let mut attempt = account.reserve("openai", "unpriced", URL, 1024).unwrap();
    attempt.sent().unwrap();
    let mut response = InferResponse::new(
        vec![ContentBlock::Text {
            text: "answer".into(),
        }],
        TokenUsage::new(10, 20),
        StopReason::EndTurn,
    );
    response.gen_ai.response_model = Some("unpriced".into());
    attempt.settle(&response).unwrap();
    account.close("done").unwrap();
    account.snapshot().unwrap().durable_observation()
}
#[test]
fn completed_report_is_versioned_bound_and_never_a_numeric_account() {
    let observations = vec![completed("a"), completed("b")];
    let report = CompletedCostReport::read(&observations).unwrap();
    let checkpoint = report.checkpoint(b"project-a");
    assert_eq!(checkpoint["schema"], SCHEMA);
    assert!(report.matches_checkpoint(&checkpoint, b"project-a"));
    assert!(!report.matches_checkpoint(&checkpoint, b"project-b"));
    assert!(InferenceAdmission::from_checkpoint(&checkpoint, b"project-a").is_err());
    assert!(report.summary().contains("2 scope(s), 2 request(s)"));
    assert!(report.summary().contains("2 unpriced call(s)"));
    let old = Value::String(
        super::super::denied("only a complete strict numeric account can be checkpointed")
            .to_string(),
    );
    assert!(report.matches_checkpoint(&old, b"project-a"));
    assert!(!report.matches_checkpoint(&json!("other codec failure"), b"project-a"));
    assert!(!report.matches_checkpoint(&Value::Null, b"project-a"));
    let other = CompletedCostReport::read(&[completed("c")]).unwrap();
    assert!(!other.matches_checkpoint(&checkpoint, b"project-a"));
}
#[test]
fn active_uncertain_incomplete_duplicate_and_corrupt_scopes_refuse() {
    let mutations: [fn(&mut Value); 11] = [
        |v| v["state"] = json!("Open"),
        |v| v["state"] = json!("Uncertain"),
        |v| v["unknown_attempts"][0]["sent"] = json!(false),
        |v| v["unknown_attempts"][0]["note"] = json!("possibly billed; no automatic retry"),
        |v| v["unknown_attempts"][0]["usage"] = Value::Null,
        |v| v["unknown_attempts"][0]["id"] = json!(9),
        |v| v["unknown_attempts"][0]["response_model"] = json!("another"),
        |v| v["unknown_attempts"][0]["choice"]["invocation"] = json!("another"),
        |v| v["unknown_calls"] = json!(0),
        |v| v["known_subtotal_nano_usd"] = json!("1"),
        |v| v["schema"] = json!("nika/inference-cost-observation@9"),
    ];
    for mutate in mutations {
        let mut observation = completed("a");
        mutate(&mut observation);
        assert!(CompletedCostReport::read(&[observation]).is_err());
    }
    let one = completed("a");
    assert!(CompletedCostReport::read(&[one.clone(), one]).is_err());
    assert!(CompletedCostReport::read(&[]).is_err());
    let legacy: Value = serde_json::from_str(include_str!("../legacy/fixture.json")).unwrap();
    assert!(CompletedCostReport::read(&[legacy, completed("a")]).is_err());
}

#[test]
fn other_observation_families_are_never_silently_ignored() {
    let closed = completed("a");
    for state in ["Open", "Closed", "Uncertain"] {
        // The decision owner keeps even settled/refused journals Open. This reader must
        // not interpret another owner's state or turn its unknown invoice into zero.
        let decision = json!({"schema":"nika/session-decision-seat@1",
            "kind":"decision_seat", "seat":"typesafe/jev-1.13.0",
            "unbudgeted":true, "state":state, "calls_sent":0, "unknown_calls":0,
            "attempts":[{"question":"routing", "sent":false, "outcome":"refused",
                "error":"not consulted: unknown-cost scope"}]});
        assert!(CompletedCostReport::read(&[closed.clone(), decision]).is_err());
    }
    let mut unbudgeted = closed.clone();
    unbudgeted["unbudgeted"] = json!(true);
    assert!(CompletedCostReport::read(&[closed, unbudgeted]).is_err());
}

#[test]
fn a_completed_report_never_masks_a_present_numeric_account() {
    let observations = vec![completed("prior")];
    let account = InferenceAdmission::new(Cost::new(1_000_000_000)).unwrap();
    let kept =
        crate::admission::accounting_checkpoint(Some(&account), &observations, b"project").unwrap();
    assert_eq!(
        kept["account"]["schema"],
        "nika/inference-admission-checkpoint@1"
    );
    let report = crate::admission::accounting_checkpoint(None, &observations, b"project").unwrap();
    assert_eq!(report["schema"], SCHEMA);
    assert_eq!(observations[0]["unknown_calls"], 1);
}
