// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;
use serde_json::json;

fn pause(minted: &Value, ttl: &Value) -> Value {
    json!({"kind":"workflow_paused","fields":[
        {"key":"task","value":"approve"}, {"key":"mode","value":"confirm"},
        {"key":"approval_shown_hash","value":"shown"},
        {"key":"approval_nonce","value":"run"},
        {"key":"approval_minted_at_ms","value":minted},
        {"key":"approval_ttl_seconds","value":ttl}
    ]})
}

#[test]
fn expiry_uses_the_runtime_ticket_law_including_its_last_subsecond() {
    let raw = pause(&json!(1_000_000), &json!(900)).to_string();
    let facts = RunFacts::of(Path::new("pause.ndjson"), &raw).expect("pause");
    let ticket = nika_runtime::approval::ApprovalTicket::new(
        "shown".into(),
        "run".into(),
        "approve".into(),
        1_000_000,
        900,
    );
    for (now, expired) in [
        (1_000_000, false),
        (1_899_000, false),
        (1_899_001, true),
        (1_900_000, true),
        (2_000_000, true),
    ] {
        assert_eq!(ticket.is_expired(now), expired);
        assert_eq!(facts.pause_approval_expired(now), Some(expired));
    }
    assert_eq!(facts.pause_approval_expired(-1), None);
}

#[test]
fn unreadable_timing_and_later_fields_never_supply_a_guess() {
    for (minted, ttl) in [
        (json!(0), json!(null)),
        (json!(0), json!("900")),
        (json!(0), json!(-1)),
        (json!(0), json!(u64::MAX)),
        (json!(-1), json!(900)),
        (json!("0"), json!(900)),
    ] {
        let mut frame = pause(&minted, &ttl);
        frame["fields"]
            .as_array_mut()
            .expect("fields")
            .push(json!({"key":"approval_ttl_seconds","value":900}));
        let facts = RunFacts::of(Path::new("pause.ndjson"), &frame.to_string()).expect("pause");
        assert_eq!(facts.pause_approval_expired(2_000_000), None);
    }
    let mut missing = pause(&json!(0), &json!(900));
    missing["fields"]
        .as_array_mut()
        .expect("fields")
        .retain(|f| f["key"] != "approval_ttl_seconds");
    let facts = RunFacts::of(Path::new("pause.ndjson"), &missing.to_string()).expect("pause");
    assert_eq!(facts.pause_approval_expired(2_000_000), None);
}

#[test]
fn multiple_pauses_keep_the_question_but_do_not_claim_one_ticket_expiry() {
    let first = pause(&json!(0), &json!(900));
    let later = pause(&json!(2_000_000), &json!(900));
    let raw = format!("{first}\n{later}\n");
    let facts = RunFacts::of(Path::new("pause.ndjson"), &raw).expect("pause");
    assert_eq!(
        facts.pause_gate(),
        Some(("approve", "the run awaits your answer", "confirm"))
    );
    assert_eq!(facts.pause_approval_expired(2_000_000), None);
}
