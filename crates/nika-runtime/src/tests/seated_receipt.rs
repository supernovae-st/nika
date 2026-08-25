// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Subscription-seat terminal receipt regressions.

use super::*;

/// A subscription seat is quota-backed, not free. Its terminal receipt
/// identifies the seat and requested model without inventing metered spend,
/// token usage, or the identity of the model that actually answered.
#[test]
fn seated_infer_receipt_exposes_no_numeric_or_responder_identity() {
    const NOTE: &str = "infer · seat codex · requested anthropic/claude-sonnet-4-6";
    let ran = task::RanTask {
        decisions: Vec::new(),
        note: NOTE.to_owned(),
        retries: Vec::new(),
        agent_events: Vec::new(),
        evidence: None,
        duration_ms: 0,
        result: task::RunResult::Success {
            value: Value::String("hi".to_owned()),
            tokens: None,
            recovered_from: None,
            warning: None,
            child: None,
            cost_usd: None,
            cost_unpriced: Some(nika_types::cost::UnpricedReason::SubscriptionQuota),
            model: None,
        },
    };
    let mut ok = true;
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    settle::settle_ran(
        "ask",
        ran,
        None,
        &TRUSTED,
        &[],
        None,
        &mut ok,
        &mut stamper,
        &mut sink,
    );
    let completed = sink
        .events()
        .iter()
        .find(|event| event.kind == EventKind::TaskCompleted)
        .expect("a TaskCompleted frame");
    let sfield = |key: &str| {
        completed
            .fields
            .iter()
            .find(|field| field.key == key)
            .and_then(|field| match &field.value {
                FieldValue::String(value) => Some(value.as_str()),
                _ => None,
            })
    };

    assert_eq!(sfield("note"), Some(NOTE));
    assert_eq!(sfield("access"), Some("harness"));
    assert_eq!(sfield("billing"), Some("unknown"));
    assert_eq!(sfield("cost_unpriced"), Some("subscription_quota"));
    for forbidden in ["tokens", "cost_usd", "model", "provider"] {
        assert!(
            !completed.fields.iter().any(|field| field.key == forbidden),
            "subscription receipts must omit {forbidden}"
        );
    }
}
