// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Pure accounting round trips; no provider, credentials or filesystem.
#![allow(clippy::expect_used, clippy::unwrap_used)]
use super::*;
use crate::admission::{Attempt, HardMonetaryCap, UnknownCostChoice, UnknownCostPolicy};
use nika_kernel::ai::provider::{ContentBlock, InferResponse, StopReason, UsageCompleteness};
const PROJECT: &[u8] = b"canonical/project/a";
const ENDPOINT: &str = "https://api.deepseek.com/v1/chat/completions";
fn funded() -> InferenceAdmission {
    InferenceAdmission::new(Cost::new(3_000_000_000)).unwrap()
}
fn reserve(a: &InferenceAdmission) -> Attempt {
    a.reserve("deepseek", "deepseek-v4-pro", ENDPOINT, 512)
        .unwrap()
}
fn response() -> InferResponse {
    let mut r = InferResponse::new(
        vec![ContentBlock::Text { text: "ok".into() }],
        TokenUsage::new(100, 20),
        StopReason::EndTurn,
    );
    r.usage_completeness = UsageCompleteness::Complete;
    r.gen_ai.response_model = Some("deepseek-v4-pro".into());
    r.request_id = Some("request-identity-kept".into());
    r
}
fn rehash(v: &mut Value) {
    let account: Checkpoint = serde_json::from_value(v["account"].clone()).unwrap();
    v["digest"] = Value::String(hash(&bytes(&account).unwrap()));
}
#[test]
fn closed_reopening_preserves_settlement_identity_and_amends_only_the_total() {
    let a = funded();
    let mut call = reserve(&a);
    call.sent().unwrap();
    call.settle(&response()).unwrap();
    drop(call);
    let old = a.snapshot().unwrap();
    let raw = a.checkpoint(PROJECT).unwrap();
    assert!(!raw.to_string().contains("/v1/chat/completions"));
    let (again, observed) = InferenceAdmission::from_checkpoint(&raw, PROJECT).unwrap();
    assert_eq!(observed, old.durable_observation());
    let kept = again.snapshot().unwrap();
    assert_eq!(kept.state, AdmissionState::Closed);
    assert_eq!(kept.attempts, old.attempts);
    assert_eq!(kept.estimated, old.estimated);
    assert!(
        again
            .reserve("deepseek", "deepseek-v4-pro", ENDPOINT, 512)
            .is_err()
    );
    assert!(again.amend(Cost::new(old.estimated.nano_usd - 1)).is_err());
    assert_eq!(again.snapshot().unwrap().estimated, old.estimated);
    again.amend(Cost::new(4_000_000_000)).unwrap();
    let mut next = reserve(&again);
    next.sent().unwrap();
    next.settle(&response()).unwrap();
    drop(next);
    let now = again.snapshot().unwrap();
    assert_eq!(now.limit.nano_usd, 4_000_000_000);
    assert_eq!(now.estimated.nano_usd, old.estimated.nano_usd * 2);
    assert_eq!(now.attempts[1].id, 1);
    assert_eq!(
        raw["account"]["identity"],
        again.checkpoint(PROJECT).unwrap()["account"]["identity"]
    );
}
#[test]
fn active_and_unknown_reservations_never_reopen_even_when_their_price_is_zero() {
    for free in [false, true] {
        for sent in [false, true] {
            let a = funded();
            let mut call = if free {
                a.reserve(
                    "openrouter",
                    "qwen/qwen3.8-27b:free",
                    "https://openrouter.ai/api/v1/chat/completions",
                    512,
                )
                .unwrap()
            } else {
                reserve(&a)
            };
            if sent {
                call.sent().unwrap();
            }
            let raw = a.checkpoint(PROJECT).unwrap();
            let (again, _) = InferenceAdmission::from_checkpoint(&raw, PROJECT).unwrap();
            assert_eq!(again.snapshot().unwrap().state, AdmissionState::Uncertain);
            assert_eq!(
                again.snapshot().unwrap().active,
                a.snapshot().unwrap().active
            );
            assert!(again.amend(Cost::new(9_000_000_000)).is_err());
            drop(call);
            let (closed, _) =
                InferenceAdmission::from_checkpoint(&a.checkpoint(PROJECT).unwrap(), PROJECT)
                    .unwrap();
            if sent {
                assert_eq!(
                    closed.snapshot().unwrap().held_unknown,
                    a.snapshot().unwrap().held_unknown
                );
                assert_eq!(closed.snapshot().unwrap().state, AdmissionState::Uncertain);
                assert!(closed.amend(Cost::new(9_000_000_000)).is_err());
            } else {
                assert_eq!(closed.snapshot().unwrap().active, Cost::zero());
                closed.amend(Cost::new(3_000_000_000)).unwrap();
            }
        }
    }
}
#[test]
fn legacy_corruption_divergent_routes_and_inconsistent_exposure_refuse() {
    let a = funded();
    let mut call = reserve(&a);
    call.sent().unwrap();
    call.settle(&response()).unwrap();
    drop(call);
    let raw = a.checkpoint(PROJECT).unwrap();
    assert!(InferenceAdmission::from_checkpoint(&raw, b"other/project").is_err());
    assert!(
        InferenceAdmission::from_checkpoint(&a.snapshot().unwrap().durable_observation(), PROJECT)
            .is_err()
    );
    for (path, changed) in [
        (
            "schema",
            Value::String("nika/inference-admission-checkpoint@99".into()),
        ),
        ("estimated", Value::String("0".into())),
        ("active", Value::String("1".into())),
        ("held", Value::String("1".into())),
        ("limit", Value::String("-1".into())),
    ] {
        let mut wrong = raw.clone();
        wrong["account"][path] = changed;
        assert!(InferenceAdmission::from_checkpoint(&wrong, PROJECT).is_err());
        rehash(&mut wrong);
        assert!(InferenceAdmission::from_checkpoint(&wrong, PROJECT).is_err());
    }
    for (path, changed) in [
        ("id", Value::from(1)),
        ("route_tariff", Value::String("changed".into())),
        ("max_output_tokens", Value::from(0)),
        ("phase", Value::String("released".into())),
    ] {
        let mut wrong = raw.clone();
        wrong["account"]["attempts"][0][path] = changed;
        rehash(&mut wrong);
        assert!(InferenceAdmission::from_checkpoint(&wrong, PROJECT).is_err());
    }
    assert!(
        InferenceAdmission::unbudgeted()
            .checkpoint(PROJECT)
            .is_err()
    );
}
#[test]
fn private_metadata_is_not_silently_lost_or_written() {
    let a = funded();
    let mut call = reserve(&a);
    call.sent().unwrap();
    let mut r = response();
    r.request_id = Some(format!("private {ENDPOINT}"));
    call.settle(&r).unwrap();
    drop(call);
    assert!(a.checkpoint(PROJECT).is_err());
}

#[test]
fn unknown_cost_choice_cannot_become_a_numeric_checkpoint() {
    let choice = UnknownCostChoice::new(
        "candidate".into(),
        "invocation".into(),
        "deepseek".into(),
        "deepseek-v4-pro".into(),
        ENDPOINT.into(),
        2,
        512,
        std::time::Duration::from_secs(10),
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
    let a = InferenceAdmission::new_unknown(choice, policy)
        .unwrap()
        .for_scope("candidate", "invocation")
        .unwrap();
    assert!(a.checkpoint(PROJECT).is_err());
}
