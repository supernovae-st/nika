// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
use super::*;

fn ts(value: &str) -> Timestamp {
    value.parse().expect("timestamp")
}

fn line(
    seq: u64,
    kind: &str,
    slot: Option<&str>,
    payload: &str,
    prev: Option<&str>,
) -> (String, String) {
    line_at(seq, "2026-08-19T03:02:00Z", kind, slot, payload, prev)
}

fn line_at(
    seq: u64,
    at: &str,
    kind: &str,
    slot: Option<&str>,
    payload: &str,
    prev: Option<&str>,
) -> (String, String) {
    let payload = complete_payload(kind, payload);
    ledger_line(seq, ts(at), kind, slot, &payload, prev).expect("valid test ledger line")
}

fn complete_payload(kind: &str, payload: &str) -> String {
    let Ok(mut doc) = serde_json::from_str::<serde_json::Value>(payload) else {
        return payload.to_owned();
    };
    let Some(object) = doc.as_object_mut() else {
        return payload.to_owned();
    };
    if kind == "claimed" {
        object
            .entry("attempt".to_owned())
            .or_insert(serde_json::Value::from(1));
    } else if matches!(kind, "fired" | "skipped" | "paused" | "failed" | "disarmed") {
        for key in ["slot", "reason", "trace", "exit", "slots", "fencing", "gen"] {
            object
                .entry(key.to_owned())
                .or_insert(serde_json::Value::Null);
        }
    }
    serde_json::to_string(&doc).expect("test payload")
}

#[test]
fn canonical_lines_verify_and_one_changed_byte_refuses() {
    let (first, hash) = line(
        1,
        "fired",
        Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        r#"{"slot":"2026-08-19T03:00:00Z","exit":0}"#,
        None,
    );
    assert_eq!(verify_line(&first, 1, None), Some(hash));
    assert!(verify_line(&first.replace("exit\":0", "exit\":1"), 1, None).is_none());
    assert!(verify_line(&format!("{first} "), 1, None).is_none());
    let mut suffixed = first.clone();
    suffixed.pop();
    suffixed.push_str(r#","extra":"tampered"}"#);
    assert!(verify_line(&suffixed, 1, None).is_none());
    assert_eq!(scan_chain(&format!("{first}\nbroken\n")).2, 1);
}

#[test]
fn projection_vocabulary_is_exact() {
    for (word, expected) in [
        ("fired", DecisionKind::Fired),
        ("skipped", DecisionKind::Skipped),
        ("paused", DecisionKind::Paused),
        ("failed", DecisionKind::Failed),
    ] {
        assert_eq!(DecisionKind::parse_projection(word), Some(expected));
        assert_eq!(expected.as_str(), word);
    }
    assert_eq!(DecisionKind::parse_projection("disarmed"), None);
    assert_eq!(DecisionKind::parse_projection("unknown"), None);
}

#[test]
fn schema_and_line_guards_refuse_each_independent_mismatch() {
    let (valid, hash) = line(1, "fired", None, r#"{"slot":null}"#, None);
    assert!(first_line_is_versioned(&valid));
    assert!(!first_line_is_versioned(""));
    assert!(!first_line_is_versioned(&format!(
        r#"{{"schema":"{LEDGER_SCHEMA}"}}"#
    )));
    assert!(classify_journal(&format!(r#"{{"schema":"{LEDGER_SCHEMA}"}}"#)).is_none());
    assert!(!first_line_is_versioned(
        &valid.replace(&hash, &"0".repeat(64))
    ));
    assert!(!first_line_is_versioned(r#"{"schema":"nika/arm-event@2"}"#));
    assert_eq!(verify_line(&valid, 1, None), Some(hash));
    assert!(verify_line(&valid.replace(LEDGER_SCHEMA, "nika/arm-event@2"), 1, None).is_none());
    assert!(verify_line(&valid.replace("\"v\":1", "\"v\":2"), 1, None).is_none());
    assert!(verify_line(&valid, 2, None).is_none());
    assert!(
        verify_line(
            &valid.replace("2026-08-19T03:02:00Z", "not-a-time"),
            1,
            None
        )
        .is_none()
    );

    let wrong_schema_prefix = concat!(
        r#"{"schema":"nika/arm-event@2","v":1,"seq":1,"#,
        r#""ts":"2026-08-19T03:02:00Z","kind":"fired","slot_id":null,"#,
        r#""payload":{"slot":null,"reason":null,"trace":null,"exit":null,"slots":null,"fencing":null,"gen":null},"prev_hash":null"#
    );
    let wrong_schema_hash = sha256_hex(format!("null\n{wrong_schema_prefix}").as_bytes());
    let wrong_schema = format!(r#"{wrong_schema_prefix},"hash":"{wrong_schema_hash}"}}"#);
    assert!(verify_line(&wrong_schema, 1, None).is_none());

    let (unknown_kind, _) =
        unchecked_ledger_line(1, ts("2026-08-19T03:02:00Z"), "invented", None, r"{}", None);
    assert!(verify_line(&unknown_kind, 1, None).is_none());
    let (bad_slot, _) = unchecked_ledger_line(
        1,
        ts("2026-08-19T03:02:00Z"),
        "claimed",
        Some("x"),
        r#"{"attempt":1,"deadline":"2026-08-20T03:00:00Z","fencing":1,"gen":null}"#,
        None,
    );
    assert!(verify_line(&bad_slot, 1, None).is_none());
    let (slotless_claim, _) = unchecked_ledger_line(
        1,
        ts("2026-08-19T03:02:00Z"),
        "claimed",
        None,
        r#"{"attempt":1,"deadline":"2026-08-20T03:00:00Z","fencing":1,"gen":null}"#,
        None,
    );
    assert!(verify_line(&slotless_claim, 1, None).is_none());
    let (missing_decision_shape, _) =
        unchecked_ledger_line(1, ts("2026-08-19T03:02:00Z"), "fired", None, r"{}", None);
    assert!(verify_line(&missing_decision_shape, 1, None).is_none());

    let canonical_hash = "a".repeat(64);
    let (foreign_predecessor, _) = unchecked_ledger_line(
        1,
        ts("2026-08-19T03:02:00Z"),
        "fired",
        None,
        &complete_payload("fired", r#"{"slot":null}"#),
        Some(&canonical_hash),
    );
    assert!(verify_line(&foreign_predecessor, 1, None).is_none());

    for (slot_id, slot) in [
        (None, r#""2026-08-19T03:00:00Z""#),
        (
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            "null",
        ),
    ] {
        let payload = complete_payload("disarmed", &format!(r#"{{"slot":{slot}}}"#));
        let (semantic_disarm, _) = unchecked_ledger_line(
            1,
            ts("2026-08-19T03:02:00Z"),
            "disarmed",
            slot_id,
            &payload,
            None,
        );
        assert!(verify_line(&semantic_disarm, 1, None).is_none());
    }

    for payload in [
        r#"{"from":"history-w2-02.ndjson","lines":1,"archives":1,"archives_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#,
        r#"{"from":"history-w2.ndjson","lines":0,"archives":1,"archives_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#,
        r#"{"from":"history-w2.ndjson","lines":1,"archives":0,"archives_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#,
    ] {
        let (invalid_rotation, _) = unchecked_ledger_line(
            1,
            ts("2026-08-19T03:02:00Z"),
            "rotated",
            None,
            payload,
            None,
        );
        assert!(verify_line(&invalid_rotation, 1, None).is_none());
    }
}

#[test]
fn private_wire_validators_cover_every_acceptance_boundary() {
    let exact = serde_json::json!({"a": null, "b": null});
    let exact = exact.as_object().expect("object");
    assert!(exact_keys(exact, &["a", "b"]));
    assert!(!exact_keys(exact, &["a"]));
    assert!(!exact_keys(exact, &["a", "b", "c"]));
    assert!(exact_or_subset_keys(exact, &["a", "b", "c"]));
    assert!(!exact_or_subset_keys(exact, &["a"]));

    assert!(nullable_string(None));
    assert!(nullable_string(Some(&serde_json::Value::Null)));
    assert!(nullable_string(Some(&serde_json::json!("trace"))));
    assert!(!nullable_string(Some(&serde_json::json!(7))));
    assert!(nullable_u8(Some(&serde_json::json!(255))));
    assert!(!nullable_u8(Some(&serde_json::json!(256))));
    assert!(!nullable_u8(Some(&serde_json::json!("0"))));
    assert!(nullable_u32(Some(&serde_json::json!(u32::MAX))));
    assert!(!nullable_u32(Some(&serde_json::json!(
        u64::from(u32::MAX) + 1
    ))));
    assert!(!nullable_u32(Some(&serde_json::json!("0"))));
    assert!(nullable_u64(Some(&serde_json::json!(u64::MAX))));
    assert!(!nullable_u64(Some(&serde_json::json!(-1))));
    assert!(timestamp_or_null(&serde_json::Value::Null));
    assert!(timestamp_or_null(&serde_json::json!(
        "2026-08-19T03:02:00Z"
    )));
    assert!(!timestamp_or_null(&serde_json::json!("not-a-time")));
    assert!(!timestamp_or_null(&serde_json::json!(3)));

    let generation = "a".repeat(64);
    assert!(generation_valid(Some(&serde_json::json!(generation))));
    assert!(generation_valid(Some(&serde_json::Value::Null)));
    assert!(!generation_valid(None));
    assert!(!generation_valid(Some(&serde_json::json!("short"))));
    assert!(!generation_valid(Some(&serde_json::json!(3))));
    assert!(hash_is_canonical(&"a".repeat(64)));
    assert!(!hash_is_canonical(&"A".repeat(64)));
    assert!(!hash_is_canonical(&"g".repeat(64)));
    assert!(!hash_is_canonical(&"a".repeat(63)));
}

#[test]
fn legacy_and_decision_shapes_refuse_each_wrong_field_independently() {
    let valid_legacy = serde_json::json!({
        "slot": "2026-08-19T03:00:00Z",
        "decided_at": "2026-08-19T03:02:00Z",
        "kind": "fired",
        "reason": null,
        "trace": null,
        "exit": 0,
        "slots": 1
    });
    assert!(legacy_line_valid(&valid_legacy));
    for (field, wrong) in [
        ("slot", serde_json::json!("not-a-time")),
        ("decided_at", serde_json::json!("not-a-time")),
        ("kind", serde_json::json!("invented")),
        ("reason", serde_json::json!(7)),
        ("trace", serde_json::json!(7)),
        ("exit", serde_json::json!(256)),
        ("slots", serde_json::json!(u64::from(u32::MAX) + 1)),
    ] {
        let mut invalid = valid_legacy.clone();
        invalid[field] = wrong;
        assert!(!legacy_line_valid(&invalid), "accepted wrong {field}");
    }
    let mut extra = valid_legacy;
    extra["extra"] = serde_json::Value::Null;
    assert!(!legacy_line_valid(&extra));

    let payload = serde_json::json!({
        "slot": "2026-08-19T03:00:00Z",
        "reason": null,
        "trace": null,
        "exit": 0,
        "slots": 1,
        "fencing": 1,
        "gen": null
    });
    for (field, wrong) in [
        ("slot", serde_json::json!(3)),
        ("reason", serde_json::json!(3)),
        ("trace", serde_json::json!(3)),
        ("exit", serde_json::json!(256)),
        ("slots", serde_json::json!(u64::from(u32::MAX) + 1)),
        ("fencing", serde_json::json!(-1)),
        ("gen", serde_json::json!("short")),
    ] {
        let mut invalid = payload.clone();
        invalid[field] = wrong;
        let encoded = serde_json::to_string(&invalid).expect("payload");
        let (candidate, _) =
            unchecked_ledger_line(1, ts("2026-08-19T03:02:00Z"), "fired", None, &encoded, None);
        assert!(
            verify_line(&candidate, 1, None).is_none(),
            "accepted wrong {field}"
        );
    }
    let mut extra = payload;
    extra["extra"] = serde_json::Value::Null;
    let (candidate, _) = unchecked_ledger_line(
        1,
        ts("2026-08-19T03:02:00Z"),
        "fired",
        None,
        &serde_json::to_string(&extra).expect("payload"),
        None,
    );
    assert!(verify_line(&candidate, 1, None).is_none());
}

#[test]
fn migration_snapshot_and_rotation_codecs_are_exact() {
    let rotated_at = ts("2026-08-19T03:02:00Z");
    let rendered =
        render_migration_intent("history-w2-2.ndjson", 7, &rotated_at).expect("migration intent");
    assert_eq!(
        rendered,
        "{\"archive\":\"history-w2-2.ndjson\",\"lines\":7,\"rotated_at\":\"2026-08-19T03:02:00Z\"}\n"
    );
    assert_eq!(
        parse_migration_intent(&rendered),
        Some(("history-w2-2.ndjson".to_owned(), 7, rotated_at))
    );
    assert!(render_migration_intent("history-w2-02.ndjson", 7, &rotated_at).is_none());
    assert!(parse_migration_intent("{}").is_none());
    assert!(parse_migration_intent(
        "{\"archive\":\"history-w2-2.ndjson\",\"lines\":7,\"rotated_at\":\"2026-08-19T03:02:00Z\",\"extra\":null}"
    )
    .is_none());

    assert!(rotation_payload(&[("history-w2.ndjson", "")]).is_none());
    assert!(rotation_payload(&[("history-w2-02.ndjson", "legacy\n")]).is_none());
    let empty_head = render_chain_anchor(0, None).expect("empty head");
    assert!(journal_snapshot_matches(Some(&empty_head), &[]));
}

#[test]
fn archive_bundle_commitment_covers_names_order_and_exact_bytes() {
    let archives = [
        (
            "history-w2.ndjson",
            "{\"slot\":\"2026-08-17T03:00:00Z\",\"decided_at\":\"2026-08-17T03:02:00Z\",\"kind\":\"fired\"}\n",
        ),
        (
            "history-w2-2.ndjson",
            "{\"slot\":\"2026-08-18T03:00:00Z\",\"decided_at\":\"2026-08-18T03:02:00Z\",\"kind\":\"skipped\"}\n",
        ),
    ];
    let baseline = archive_bundle_hash(archives);
    assert_eq!(baseline.len(), 64);
    assert_ne!(baseline, archive_bundle_hash([archives[1], archives[0],]));
    assert_ne!(
        baseline,
        archive_bundle_hash([
            archives[0],
            (archives[1].0, &archives[1].1.replace("skipped", "failed")),
        ])
    );
    assert_ne!(
        baseline,
        archive_bundle_hash([archives[0], ("history-w2-3.ndjson", archives[1].1),])
    );
    assert_ne!(baseline, archive_bundle_hash([archives[0]]));

    let payload = rotation_payload(&archives).expect("rotation payload");
    let (genesis, hash) = line(1, "rotated", None, &payload, None);
    let live = format!("{genesis}\n");
    assert!(archive_commitment_matches(&live, &archives));
    assert!(!archive_commitment_matches(
        &live,
        &[
            (archives[0].0, &archives[0].1.replace("fired", "failed")),
            archives[1]
        ]
    ));
    let head = render_chain_anchor(1, Some(&hash)).expect("head");
    let snapshot = [
        (archives[0].0, archives[0].1, false),
        (archives[1].0, archives[1].1, false),
        ("history.ndjson", live.as_str(), true),
    ];
    assert!(journal_snapshot_matches(Some(&head), &snapshot));
    assert!(!journal_snapshot_matches(None, &snapshot));
    assert!(!journal_snapshot_matches(
        Some(&head),
        &[snapshot[1], snapshot[0], snapshot[2]]
    ));
    assert!(!journal_snapshot_matches(None, &[snapshot[0]]));
    assert!(!journal_snapshot_matches(
        Some(&render_chain_anchor(0, None).expect("empty head")),
        &[snapshot[0], ("history.ndjson", "", true)]
    ));
}

#[test]
fn durable_head_codec_accepts_only_a_matching_verified_prefix() {
    let (first, first_hash) = line(1, "fired", None, r#"{"slot":null}"#, None);
    let (second, second_hash) = line(2, "skipped", None, r#"{"slot":null}"#, Some(&first_hash));
    let text = format!("{first}\n{second}\n");
    let first_head = render_chain_anchor(1, Some(&first_hash)).expect("head 1");
    let second_head = render_chain_anchor(2, Some(&second_hash)).expect("head 2");
    assert!(chain_anchor_matches(&text, Some(&first_head)));
    assert!(chain_anchor_matches(&text, Some(&second_head)));
    assert!(chain_anchor_matches(
        &text,
        Some(&render_chain_anchor(0, None).expect("bootstrap"))
    ));
    assert!(!chain_anchor_matches(&text, None));
    assert!(!chain_anchor_matches(
        &text,
        Some(&second_head.replace(&second_hash, &"a".repeat(64)))
    ));
    assert!(render_chain_anchor(0, Some(&first_hash)).is_none());
    assert!(render_chain_anchor(1, None).is_none());
}

#[test]
fn scan_chain_reports_the_exact_prefix_identity() {
    let (first, first_hash) = line(1, "fired", None, r#"{"slot":null}"#, None);
    let (second, second_hash) = line(2, "skipped", None, r#"{"slot":null}"#, Some(&first_hash));
    let text = format!("{first}\n{second}\nbroken\n");
    assert_eq!(scan_chain(&text), (2, Some(second_hash), 2));
}

#[test]
fn json_and_decision_payload_are_canonical() {
    assert_eq!(json_str("plain"), r#""plain""#);
    assert_eq!(json_str("a\"b\\c\n"), "\"a\\\"b\\\\c\\u000a\"");

    let entry = HistoryEntry {
        slot: Some(ts("2026-08-19T03:00:00Z")),
        decided_at: ts("2026-08-19T03:02:00Z"),
        kind: DecisionKind::Fired,
        reason: Some("quoted \"reason\"".to_owned()),
        trace: Some("trace\\path".to_owned()),
        exit: Some(7),
        slots: Some(2),
        slot_id: None,
        fencing: Some(FencingToken::new(9)),
        generation: ArmGeneration::from_wire(&"a".repeat(64)),
    };
    assert_eq!(
        decision_payload(&entry),
        r#"{"slot":"2026-08-19T03:00:00Z","reason":"quoted \"reason\"","trace":"trace\\path","exit":7,"slots":2,"fencing":9,"gen":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#
    );
    let last = last_projection(&entry).expect("slot-bearing projection");
    assert_eq!(last.slot, entry.slot.expect("slot"));
    assert_eq!(last.kind, DecisionKind::Fired);

    let claim = Claim {
        slot_id: SlotId::from_wire(&"b".repeat(64)).expect("slot id"),
        generation: entry.generation.clone(),
        deadline: ts("2026-08-20T03:00:00Z"),
        decided_at: ts("2026-08-19T03:02:00Z"),
    };
    assert_eq!(
        claim_payload(&claim, 9).expect("claim payload"),
        r#"{"attempt":1,"deadline":"2026-08-20T03:00:00Z","fencing":9,"gen":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#
    );
    assert!(claim_payload(&claim, 0).is_none());
}

#[test]
fn versioned_claim_and_receipt_replay_one_lifecycle() {
    let slot = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let (claim, hash) = line(
        1,
        "claimed",
        Some(slot),
        r#"{"deadline":"2026-08-20T03:00:00Z","fencing":1,"gen":null}"#,
        None,
    );
    let (receipt, _) = line(
        2,
        "fired",
        Some(slot),
        r#"{"slot":"2026-08-19T03:00:00Z","trace":null,"exit":0,"fencing":1,"gen":null}"#,
        Some(&hash),
    );
    let text = format!("{claim}\n{receipt}\n");
    let replayed = replay_core([(&*text, true)]).expect("valid replay");
    assert_eq!(
        replayed.last.as_ref().expect("last").kind,
        DecisionKind::Fired
    );
    assert_eq!(
        fold_replay(&replayed, &ts("2026-08-21T03:00:00Z"))
            .expect("fold")
            .0,
        FiringState::Succeeded
    );
    assert!(unsettled(&text).expect("valid journal").is_empty());
}

#[test]
fn public_replay_returns_projection_watermark_and_fold_context() {
    let slot = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let (claim, hash) = line_at(
        1,
        "2026-08-19T03:01:00Z",
        "claimed",
        Some(slot),
        r#"{"deadline":"2026-08-20T03:00:00Z","fencing":1,"gen":null}"#,
        None,
    );
    let (receipt, _) = line_at(
        2,
        "2026-08-19T03:02:00Z",
        "fired",
        Some(slot),
        r#"{"slot":"2026-08-19T03:00:00Z","trace":null,"exit":0,"fencing":1,"gen":null}"#,
        Some(&hash),
    );
    let text = format!("{claim}\n{receipt}\n");
    let (last, watermark) = replay_projection([(&*text, true)]).expect("valid journal");
    assert_eq!(last.expect("last").kind, DecisionKind::Fired);
    assert_eq!(watermark, Some(ts("2026-08-19T03:02:00Z")));
    let (state, beyond_last, lifecycle_slot) =
        replay_state([(&*text, true)], &ts("2026-08-19T03:03:00Z")).expect("state");
    assert_eq!(state, FiringState::Succeeded);
    assert!(!beyond_last);
    assert_eq!(lifecycle_slot.as_deref(), Some(slot));
}

#[test]
fn replay_keeps_interleaved_slot_groups_separate() {
    let slot_a = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let slot_b = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let (claim_a, hash_a) = line(
        1,
        "claimed",
        Some(slot_a),
        r#"{"deadline":"2026-08-20T03:00:00Z","fencing":1,"gen":null}"#,
        None,
    );
    let (claim_b, hash_b) = line(
        2,
        "claimed",
        Some(slot_b),
        r#"{"deadline":"2026-08-20T03:00:00Z","fencing":2,"gen":null}"#,
        Some(&hash_a),
    );
    let (receipt_b, _) = line(
        3,
        "fired",
        Some(slot_b),
        r#"{"slot":"2026-08-19T03:00:00Z","trace":null,"exit":0,"fencing":2,"gen":null}"#,
        Some(&hash_b),
    );
    let text = format!("{claim_a}\n{claim_b}\n{receipt_b}\n");
    let (state, beyond_last, lifecycle_slot) =
        replay_state([(&*text, true)], &ts("2026-08-19T03:03:00Z")).expect("state");
    assert_eq!(state, FiringState::Succeeded);
    assert!(!beyond_last);
    assert_eq!(lifecycle_slot.as_deref(), Some(slot_b));
}

#[test]
fn a_new_claim_after_the_projection_is_reported_as_beyond_last() {
    let slot_a = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let slot_b = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let (receipt, hash) = line(
        1,
        "fired",
        Some(slot_a),
        r#"{"slot":"2026-08-19T03:00:00Z","trace":null,"exit":0,"fencing":null,"gen":null}"#,
        None,
    );
    let (claim, _) = line(
        2,
        "claimed",
        Some(slot_b),
        r#"{"deadline":"2026-08-20T03:00:00Z","fencing":2,"gen":null}"#,
        Some(&hash),
    );
    let text = format!("{receipt}\n{claim}\n");
    let (state, beyond_last, lifecycle_slot) =
        replay_state([(&*text, true)], &ts("2026-08-19T03:03:00Z")).expect("state");
    assert_eq!(state, FiringState::Claimed);
    assert!(beyond_last);
    assert_eq!(lifecycle_slot.as_deref(), Some(slot_b));
}

#[test]
fn orphan_claim_crosses_only_the_open_deadline_boundary() {
    let slot = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let (claim, _) = line(
        1,
        "claimed",
        Some(slot),
        r#"{"deadline":"2026-08-20T03:00:00Z","fencing":1,"gen":null}"#,
        None,
    );
    let replayed = replay_core([(&*claim, true)]).expect("valid replay");
    assert_eq!(unsettled(&claim).expect("valid journal").len(), 1);
    assert_eq!(
        fold_replay(&replayed, &ts("2026-08-20T03:00:00Z"))
            .expect("fold")
            .0,
        FiringState::Claimed
    );
    assert_eq!(
        fold_replay(&replayed, &ts("2026-08-20T03:00:00.000000001Z"))
            .expect("fold")
            .0,
        FiringState::Ambiguous
    );
    let forged_receipt = format!(
        "{claim}\n{{\"kind\":\"fired\",\"slot_id\":\"{slot}\",\"payload\":{{\"fencing\":1}}}}\n"
    );
    assert_eq!(
        unsettled(&forged_receipt).expect("versioned prefix").len(),
        1
    );
}

#[test]
fn unsettled_requires_a_later_receipt_with_both_identities() {
    let slot_a = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let slot_b = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let (claim, claim_hash) = line_at(
        1,
        "2026-08-19T03:00:00Z",
        "claimed",
        Some(slot_a),
        r#"{"deadline":"2026-08-20T03:00:00Z","fencing":1,"gen":null}"#,
        None,
    );
    let (wrong_slot, _) = line(
        2,
        "fired",
        Some(slot_b),
        r#"{"slot":null,"fencing":1}"#,
        Some(&claim_hash),
    );
    let (wrong_fence, _) = line(
        2,
        "fired",
        Some(slot_a),
        r#"{"slot":null,"fencing":2}"#,
        Some(&claim_hash),
    );
    let (receipt, _) = line(
        2,
        "fired",
        Some(slot_a),
        r#"{"slot":null,"fencing":1}"#,
        Some(&claim_hash),
    );
    assert_eq!(
        unsettled(&format!("{claim}\n{wrong_slot}\n"))
            .expect("valid journal")
            .len(),
        1
    );
    assert_eq!(
        unsettled(&format!("{claim}\n{wrong_fence}\n"))
            .expect("valid journal")
            .len(),
        1
    );
    assert!(
        unsettled(&format!("{claim}\n{receipt}\n"))
            .expect("valid journal")
            .is_empty()
    );
}

#[test]
fn tallies_count_each_decision_across_journals() {
    let first = concat!(
        "{\"slot\":\"2026-08-19T03:00:00Z\",\"decided_at\":\"2026-08-19T03:01:00Z\",\"kind\":\"skipped\"}\n",
        "{\"slot\":\"2026-08-19T04:00:00Z\",\"decided_at\":\"2026-08-19T04:01:00Z\",\"kind\":\"fired\"}\n"
    );
    let second = "{\"slot\":\"2026-08-19T05:00:00Z\",\"decided_at\":\"2026-08-19T05:01:00Z\",\"kind\":\"fired\"}\n";
    assert_eq!(tallies([(first, false), (second, false)]), Some((1, 2)));

    let (verified, _) = line(1, "fired", None, r#"{"slot":null}"#, None);
    let forged_tail = format!("{verified}\n{{\"kind\":\"fired\"}}\n");
    assert_eq!(tallies([(&*forged_tail, true)]), Some((0, 1)));

    let (versioned_first, first_hash) = line(1, "skipped", None, r#"{"slot":null}"#, None);
    let (versioned_second, _) = line(2, "fired", None, r#"{"slot":null}"#, Some(&first_hash));
    let versioned = format!("{versioned_first}\n{versioned_second}\n");
    assert_eq!(tallies([(&*versioned, true)]), Some((1, 1)));
}

#[test]
fn slotless_disarm_advances_only_the_watermark() {
    let (disarmed, _) = line_at(1, "2026-08-19T04:00:00Z", "disarmed", None, r"{}", None);
    let (last, watermark) = replay_projection([(&*disarmed, true)]).expect("valid journal");
    assert!(last.is_none());
    assert_eq!(watermark, Some(ts("2026-08-19T04:00:00Z")));
    assert!(envelope_ts(&serde_json::json!({"ts": "not-a-time"})).is_none());
    assert!(envelope_ts(&serde_json::json!({})).is_none());
}

#[test]
fn versioned_direct_receipt_without_slot_id_still_projects() {
    let (receipt, _) = line_at(
        1,
        "2026-08-19T03:02:00Z",
        "paused",
        None,
        r#"{"slot":"2026-08-19T03:00:00Z","trace":null,"exit":4,"fencing":null,"gen":null}"#,
        None,
    );
    let (last, watermark) = replay_projection([(&*receipt, true)]).expect("valid journal");
    let last = last.expect("projection");
    assert_eq!(last.kind, DecisionKind::Paused);
    assert_eq!(watermark, Some(ts("2026-08-19T03:02:00Z")));
    let (state, _, lifecycle_slot) =
        replay_state([(&*receipt, true)], &ts("2026-08-19T03:03:00Z")).expect("state");
    assert_eq!(state, FiringState::Cancelled);
    assert!(lifecycle_slot.is_none());
}

#[test]
fn legacy_replay_and_projection_round_trip_stay_byte_stable() {
    let legacy = r#"{"slot":"2026-08-19T03:00:00Z","decided_at":"2026-08-19T03:02:00Z","kind":"skipped","reason":"overlap","exit":0}"#;
    let replayed = replay_core([(legacy, false)]).expect("valid legacy");
    let last = replayed.last.expect("last");
    let rendered = render_last(&last);
    let parsed = parse_last(&rendered).expect("projection");
    assert_eq!(parsed.kind, DecisionKind::Skipped);
    assert_eq!(parsed.slot, last.slot);
    assert_eq!(tallies([(legacy, false)]), Some((1, 0)));
    assert_eq!(
        replay_state([(legacy, false)], &ts("2026-08-19T03:03:00Z"))
            .expect("state")
            .0,
        FiringState::Skipped
    );
}
