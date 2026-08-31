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
        "skipped",
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
fn versioned_dialect_detection_stops_after_valid_genesis() {
    let (genesis, hash) = line(1, "disarmed", None, r#"{"slot":null}"#, None);
    let corrupt_tail = format!("{genesis}\nnot-json\n");

    assert_eq!(
        classify_journal(&corrupt_tail),
        Some(JournalFormat::Versioned)
    );
    assert!(first_line_is_versioned(&corrupt_tail));
    assert_eq!(scan_chain(&corrupt_tail), (1, Some(hash), 1));
    assert!(replay_projection([(&*corrupt_tail, true)]).is_none());

    let blank_tail = format!("{genesis}\n\n");
    assert!(classify_journal(&blank_tail).is_none());
    assert!(!first_line_is_versioned(&blank_tail));
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
fn rotation_is_only_the_first_null_predecessor_lifecycle_event() {
    let (first, first_hash) = line(1, "disarmed", None, r#"{"slot":null}"#, None);
    let forged_payload = r#"{"from":"history-w2.ndjson","lines":1,"archives":1,"archives_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#;
    let (rotated, rotated_hash) = unchecked_ledger_line(
        2,
        ts("2026-08-19T03:03:00Z"),
        "rotated",
        None,
        forged_payload,
        Some(&first_hash),
    );
    let live = format!("{first}\n{rotated}\n");
    let forged_head = render_chain_anchor(2, Some(&rotated_hash)).expect("shape-valid head");

    assert!(
        verify_line(&rotated, 2, Some(&first_hash)).is_none(),
        "a rotation cannot appear after another lifecycle event"
    );
    assert_eq!(scan_chain(&live), (1, Some(first_hash), 1));
    assert!(replay_projection([(&*live, true)]).is_none());
    assert!(!journal_snapshot_matches(
        Some(&forged_head),
        &[("history.ndjson", &live, true)]
    ));

    let mut lifecycle = LifecycleValidator::default();
    let first_doc: serde_json::Value = serde_json::from_str(&first).expect("first doc");
    let rotated_doc: serde_json::Value = serde_json::from_str(&rotated).expect("rotation doc");
    assert!(lifecycle.accept(&first_doc));
    assert!(
        !lifecycle.accept(&rotated_doc),
        "the typed lifecycle authority rejects a non-first rotation"
    );
}

#[test]
fn snapshot_rejects_an_invalid_suffix_beyond_a_lagging_head() {
    let (first, first_hash) = line(1, "disarmed", None, r#"{"slot":null}"#, None);
    let payload = r#"{"from":"history-w2.ndjson","lines":1,"archives":1,"archives_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#;
    let (invalid_rotation, _) = unchecked_ledger_line(
        2,
        ts("2026-08-19T03:03:00Z"),
        "rotated",
        None,
        payload,
        Some(&first_hash),
    );
    let live = format!("{first}\n{invalid_rotation}\n");
    let lagging_head = render_chain_anchor(1, Some(&first_hash)).expect("head");

    assert!(
        chain_anchor_matches(&live, Some(&lagging_head)),
        "a head may intentionally lag a longer fully verified chain"
    );
    assert!(
        !journal_snapshot_matches(Some(&lagging_head), &[("history.ndjson", &live, true)]),
        "snapshot validation must reject invalid bytes after that head"
    );
}

#[test]
fn sequence_one_rotation_still_requires_a_null_predecessor() {
    let payload = r#"{"from":"history-w2.ndjson","lines":1,"archives":1,"archives_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#;
    let foreign = "b".repeat(64);
    let (linked_genesis, _) = unchecked_ledger_line(
        1,
        ts("2026-08-19T03:02:00Z"),
        "rotated",
        None,
        payload,
        Some(&foreign),
    );
    assert!(verify_line(&linked_genesis, 1, Some(&foreign)).is_none());
}

#[test]
fn durable_head_codec_accepts_only_a_matching_verified_prefix() {
    let (first, first_hash) = line(1, "skipped", None, r#"{"slot":null}"#, None);
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

    let forged_hash = "b".repeat(64);
    let forged_head = render_chain_anchor(3, Some(&forged_hash)).expect("forged head shape");
    let invalid_middle = format!("{first}\nbroken\n{{\"hash\":\"{forged_hash}\"}}\n");
    assert!(!chain_anchor_matches(&invalid_middle, Some(&forged_head)));
    assert!(render_chain_anchor(0, Some(&first_hash)).is_none());
    assert!(render_chain_anchor(1, None).is_none());
}

#[test]
fn scan_chain_reports_the_exact_prefix_identity() {
    let (first, first_hash) = line(1, "skipped", None, r#"{"slot":null}"#, None);
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
        execution: None,
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
        execution: None,
    };
    assert_eq!(
        claim_payload(&claim, 9).expect("claim payload"),
        r#"{"attempt":1,"deadline":"2026-08-20T03:00:00Z","fencing":9,"gen":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#
    );
    assert!(claim_payload(&claim, 0).is_none());
}

#[test]
fn public_constructors_start_with_conservative_defaults() {
    let slot = ts("2026-08-19T03:00:00Z");
    let decided_at = ts("2026-08-19T03:02:00Z");
    let entry = HistoryEntry::new(Some(slot), decided_at, DecisionKind::Fired);
    assert_eq!(entry.slot, Some(slot));
    assert_eq!(entry.decided_at, decided_at);
    assert_eq!(entry.kind, DecisionKind::Fired);
    assert!(entry.reason.is_none());
    assert!(entry.trace.is_none());
    assert!(entry.exit.is_none());
    assert!(entry.slots.is_none());
    assert!(entry.slot_id.is_none());
    assert!(entry.fencing.is_none());
    assert!(entry.generation.is_none());

    let slot_id = SlotId::from_wire(&"a".repeat(64)).expect("slot id");
    let deadline = ts("2026-08-20T03:00:00Z");
    let claim = Claim::new(slot_id.clone(), deadline, decided_at);
    assert_eq!(claim.slot_id, slot_id);
    assert_eq!(claim.deadline, deadline);
    assert_eq!(claim.decided_at, decided_at);
    assert!(claim.generation.is_none());

    assert_eq!(
        RecordOutcome::new(7, 2),
        RecordOutcome {
            seq: 7,
            repaired: 2
        }
    );
}

#[test]
fn legacy_receipt_adapter_accepts_only_consistent_bare_terminals() {
    let entry = |kind, exit| HistoryEntry {
        slot: Some(ts("2026-08-19T03:00:00Z")),
        decided_at: ts("2026-08-19T03:02:00Z"),
        kind,
        reason: None,
        trace: None,
        exit,
        slots: None,
        slot_id: None,
        fencing: None,
        generation: None,
        execution: None,
    };
    for (kind, exit, word) in [
        (DecisionKind::Fired, Some(0), "fired"),
        (DecisionKind::Paused, Some(4), "paused"),
        (DecisionKind::Failed, Some(1), "failed"),
        (DecisionKind::Failed, Some(5), "failed"),
    ] {
        let payload = legacy_receipt_payload(&entry(kind, exit)).expect("legacy terminal");
        assert!(payload.ends_with(",\"legacy\":true}"), "{word}: {payload}");
        let (line, _) = ledger_line(1, ts("2026-08-19T03:02:00Z"), word, None, &payload, None)
            .expect("explicit legacy line");
        assert_eq!(scan_chain(&format!("{line}\n")).2, 1, "{word}");
    }
    for (kind, exit) in [
        (DecisionKind::Fired, Some(7)),
        (DecisionKind::Fired, None),
        (DecisionKind::Paused, Some(0)),
        (DecisionKind::Paused, Some(7)),
        (DecisionKind::Failed, Some(0)),
        (DecisionKind::Failed, Some(4)),
        (DecisionKind::Failed, None),
        (DecisionKind::Skipped, Some(0)),
    ] {
        assert!(legacy_receipt_payload(&entry(kind, exit)).is_none());
    }
    let mut named = entry(DecisionKind::Fired, Some(0));
    named.slot_id = SlotId::from_wire(&"a".repeat(64));
    assert!(legacy_receipt_payload(&named).is_none());
    named.slot_id = None;
    named.fencing = Some(FencingToken::new(1));
    assert!(legacy_receipt_payload(&named).is_none());
    named.fencing = None;
    named.execution = ExecutionLink::new(
        "exe-018f1f6e-7b8c-7d9e-8fab-0123456789ab",
        "018f1f6e7b8c7d9e8fab0123456789ab",
    );
    assert!(legacy_receipt_payload(&named).is_none());
}

#[test]
fn keyless_slotful_explicit_legacy_receipt_projects() {
    let slot = ts("2026-08-19T03:00:00Z");
    let decided_at = ts("2026-08-19T03:02:00Z");
    let mut entry = HistoryEntry::new(Some(slot), decided_at, DecisionKind::Paused);
    entry.exit = Some(4);
    let payload = legacy_receipt_payload(&entry).expect("explicit legacy payload");
    let (receipt, _) =
        ledger_line(1, decided_at, "paused", None, &payload, None).expect("versioned envelope");

    let (last, watermark) = replay_projection([(&*receipt, true)]).expect("valid replay");
    let last = last.expect("projected receipt");
    assert_eq!(last.slot, slot);
    assert_eq!(last.kind, DecisionKind::Paused);
    assert_eq!(watermark, Some(decided_at));
}

#[test]
fn explicit_legacy_shape_and_terminal_lifecycle_are_both_strict() {
    let payload = |exit: u8, legacy: serde_json::Value| {
        serde_json::json!({
            "slot": "2026-08-19T03:00:00Z",
            "reason": null,
            "trace": null,
            "exit": exit,
            "slots": null,
            "fencing": null,
            "gen": null,
            "legacy": legacy,
        })
    };
    assert!(verify_payload("paused", None, &payload(4, true.into()), 1).is_some());
    assert!(verify_payload("failed", None, &payload(2, true.into()), 1).is_some());
    assert!(verify_payload("paused", None, &payload(4, false.into()), 1).is_none());
    assert!(verify_payload("skipped", None, &payload(0, true.into()), 1).is_none());
    assert!(verify_payload("disarmed", None, &payload(0, true.into()), 1).is_none());

    let versioned = |kind: &str, exit: u8, slot_id: Option<&str>, fencing: Option<u64>| {
        let mut payload = payload(exit, true.into());
        payload["fencing"] = fencing.map_or(serde_json::Value::Null, serde_json::Value::from);
        ledger_line(
            1,
            ts("2026-08-19T03:02:00Z"),
            kind,
            slot_id,
            &serde_json::to_string(&payload).expect("payload"),
            None,
        )
        .expect("shape-valid line")
        .0
    };
    for (kind, exit) in [("paused", 4), ("failed", 1), ("failed", 5)] {
        let line = versioned(kind, exit, None, None);
        assert_eq!(scan_chain(&format!("{line}\n")).2, 1, "{kind}/{exit}");
    }
    for (kind, exit) in [("fired", 7), ("paused", 0), ("failed", 0), ("failed", 4)] {
        let line = versioned(kind, exit, None, None);
        assert_eq!(scan_chain(&format!("{line}\n")).2, 0, "{kind}/{exit}");
    }
    let slot = "a".repeat(64);
    assert_eq!(
        scan_chain(&format!("{}\n", versioned("paused", 4, Some(&slot), None))).2,
        0
    );
    assert_eq!(
        scan_chain(&format!("{}\n", versioned("paused", 4, None, Some(1)))).2,
        0
    );

    for payload in [
        r#"{"slot":null,"fencing":1,"gen":null}"#,
        r#"{"slot":null,"fencing":null,"gen":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#,
    ] {
        let (disarmed, _) = line(1, "disarmed", None, payload, None);
        assert_eq!(scan_chain(&format!("{disarmed}\n")).2, 0);
    }

    for payload in [
        r#"{"slot":null,"fencing":1,"gen":null}"#,
        r#"{"slot":null,"fencing":null,"gen":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#,
    ] {
        let (skipped, _) = line(1, "skipped", None, payload, None);
        assert_eq!(scan_chain(&format!("{skipped}\n")).2, 0);
    }
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
    assert!(
        unsettled(&text)
            .expect("valid journal")
            .collect::<Vec<_>>()
            .is_empty()
    );
}

#[test]
fn keyed_slotless_receipt_still_settles_its_claim() {
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
        r#"{"slot":null,"trace":null,"exit":0,"fencing":1,"gen":null}"#,
        Some(&hash),
    );
    let text = format!("{claim}\n{receipt}\n");
    let replayed = replay_core([(&*text, true)]).expect("valid replay");
    assert_eq!(
        fold_replay(&replayed, &ts("2026-08-21T03:00:00Z"))
            .expect("fold")
            .0,
        FiringState::Succeeded
    );
    assert!(replayed.last.is_none());
}

#[test]
fn typed_receipt_derives_kind_and_binds_the_claim() {
    let claim = Claim {
        slot_id: SlotId::from_wire(&"a".repeat(64)).expect("slot id"),
        generation: ArmGeneration::from_wire(&"b".repeat(64)),
        deadline: ts("2026-08-20T03:00:00Z"),
        decided_at: ts("2026-08-19T03:01:00Z"),
        execution: None,
    };
    for (exit, kind) in [
        (0, DecisionKind::Fired),
        (4, DecisionKind::Paused),
        (2, DecisionKind::Failed),
    ] {
        let receipt = Receipt::for_claim(
            &claim,
            FencingToken::new(7),
            ts("2026-08-19T03:00:00Z"),
            ts("2026-08-19T03:02:00Z"),
            None,
            exit,
            None,
        );
        let entry = receipt.history_entry();
        assert_eq!(receipt.kind(), kind);
        assert_eq!(entry.kind, kind);
        assert_eq!(entry.slot_id.as_ref(), Some(&claim.slot_id));
        assert_eq!(entry.fencing, Some(FencingToken::new(7)));
        assert_eq!(entry.generation, claim.generation);
    }
}

#[test]
fn execution_link_is_direct_strict_and_survives_claim_replay() {
    let execution_id = "exe-018f1f6e-7b8c-7d9e-8fab-0123456789ab";
    let trace_id = "018f1f6e7b8c7d9e8fab0123456789ab";
    let link = ExecutionLink::new(execution_id, trace_id).expect("direct identity pair");
    assert_eq!(link.execution_id(), execution_id);
    assert_eq!(link.trace_id(), trace_id);
    assert!(ExecutionLink::new(execution_id, "0".repeat(32)).is_none());
    assert!(ExecutionLink::new("job-not-an-execution", trace_id).is_none());
    assert!(
        parse_last(
            r#"{"slot":"2026-08-19T03:00:00Z","fired_at":"2026-08-19T03:02:00Z","trace":null,"exit":0,"kind":"fired","gen":null,"execution_id":"exe-018f1f6e-7b8c-7d9e-8fab-0123456789ab"}"#,
        )
        .is_none()
    );

    let mut claim = Claim::new(
        SlotId::from_wire(&"a".repeat(64)).expect("slot id"),
        ts("2026-08-20T03:00:00Z"),
        ts("2026-08-19T03:01:00Z"),
    );
    claim.execution = Some(link.clone());
    let claim_payload = claim_payload(&claim, 1).expect("claim payload");
    let (claim_line, claim_hash) = ledger_line(
        1,
        claim.decided_at,
        "claimed",
        Some(claim.slot_id.as_str()),
        &claim_payload,
        None,
    )
    .expect("claim line");
    let unsettled_claim = unsettled(&format!("{claim_line}\n"))
        .expect("valid claim")
        .next()
        .expect("unsettled");
    assert_eq!(unsettled_claim.execution, Some(link.clone()));

    let receipt = Receipt::for_claim(
        &claim,
        FencingToken::new(1),
        ts("2026-08-19T03:00:00Z"),
        ts("2026-08-19T03:02:00Z"),
        Some(".nika/traces/exact.ndjson".to_owned()),
        0,
        None,
    );
    let receipt_entry = receipt.history_entry();
    let receipt_payload = decision_payload(&receipt_entry);
    let (receipt_line, _) = ledger_line(
        2,
        receipt_entry.decided_at,
        "fired",
        Some(claim.slot_id.as_str()),
        &receipt_payload,
        Some(&claim_hash),
    )
    .expect("receipt line");
    let text = format!("{claim_line}\n{receipt_line}\n");
    assert_eq!(scan_chain(&text).2, 2);
    assert!(unsettled(&text).expect("valid chain").next().is_none());

    let forged = receipt_payload.replace(trace_id, &"0".repeat(32));
    assert!(
        ledger_line(
            2,
            receipt_entry.decided_at,
            "fired",
            Some(claim.slot_id.as_str()),
            &forged,
            Some(&claim_hash),
        )
        .is_none()
    );
}

#[test]
fn execution_link_binds_canonical_normal_run_id() {
    let run_id = "d9428888-122b-4aa7-8f71-342450b49c5f";
    let execution_id = "exe-018f1f6e-7b8c-7d9e-8fab-0123456789ab";
    let trace_id = "018f1f6e7b8c7d9e8fab0123456789ab";
    let link = ExecutionLink::for_run(run_id, execution_id, trace_id).expect("run link");
    assert_eq!(link.run_id(), Some(run_id));
    assert!(ExecutionLink::for_run("not-a-run", execution_id, trace_id).is_none());
    assert!(ExecutionLink::for_run(run_id, execution_id, "0".repeat(32)).is_none());

    let mut claim = Claim::new(
        SlotId::from_wire(&"c".repeat(64)).expect("slot id"),
        ts("2026-08-20T03:00:00Z"),
        ts("2026-08-19T03:01:00Z"),
    );
    claim.execution = Some(link);
    let payload = claim_payload(&claim, 4).expect("claim payload");
    let payload: serde_json::Value = serde_json::from_str(&payload).expect("payload json");
    assert_eq!(
        payload.get("run_id").and_then(serde_json::Value::as_str),
        Some(run_id)
    );
    assert_eq!(
        execution_link(&payload).and_then(|link| link.run_id().map(str::to_owned)),
        Some(run_id.to_owned())
    );
}

#[test]
fn receipts_refuse_contradictions_mismatches_duplicates_and_future_claims() {
    let slot = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let other = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let generation = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
    let other_generation = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
    let (claim, claim_hash) = line(
        1,
        "claimed",
        Some(slot),
        &format!(r#"{{"deadline":"2026-08-20T03:00:00Z","fencing":1,"gen":"{generation}"}}"#),
        None,
    );
    let receipt = |kind: &str, id: &str, fence: u64, receipt_gen: &str, exit: u8| {
        line(
            2,
            kind,
            Some(id),
            &format!(r#"{{"slot":"2026-08-19T03:00:00Z","fencing":{fence},"gen":"{receipt_gen}","exit":{exit}}}"#),
            Some(&claim_hash),
        )
        .0
    };
    for invalid in [
        receipt("fired", slot, 1, generation, 7),
        receipt("failed", slot, 1, generation, 0),
        receipt("fired", other, 1, generation, 0),
        receipt("fired", slot, 2, generation, 0),
        receipt("fired", slot, 1, other_generation, 0),
    ] {
        assert_eq!(scan_chain(&format!("{claim}\n{invalid}\n")).2, 1);
    }

    let good = receipt("fired", slot, 1, generation, 0);
    let good_hash = verify_line(&good, 2, Some(&claim_hash)).expect("receipt hash");
    let (duplicate, _) = line(
        3,
        "fired",
        Some(slot),
        &format!(r#"{{"slot":"2026-08-19T03:00:00Z","fencing":1,"gen":"{generation}","exit":0}}"#),
        Some(&good_hash),
    );
    assert_eq!(scan_chain(&format!("{claim}\n{good}\n{duplicate}\n")).2, 2);

    let named_without_claim = line(
        1,
        "failed",
        Some(slot),
        &format!(r#"{{"slot":"2026-08-19T03:00:00Z","fencing":1,"gen":"{generation}","exit":2}}"#),
        None,
    )
    .0;
    assert_eq!(scan_chain(&named_without_claim).2, 0);
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
    let (first_claim, claim_hash) = line(
        1,
        "claimed",
        Some(slot_a),
        r#"{"deadline":"2026-08-20T03:00:00Z","fencing":1,"gen":null}"#,
        None,
    );
    let (receipt, hash) = line(
        2,
        "fired",
        Some(slot_a),
        r#"{"slot":"2026-08-19T03:00:00Z","trace":null,"exit":0,"fencing":1,"gen":null}"#,
        Some(&claim_hash),
    );
    let (claim, _) = line(
        3,
        "claimed",
        Some(slot_b),
        r#"{"deadline":"2026-08-20T03:00:00Z","fencing":3,"gen":null}"#,
        Some(&hash),
    );
    let text = format!("{first_claim}\n{receipt}\n{claim}\n");
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
    assert_eq!(unsettled(&claim).expect("valid journal").count(), 1);
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
    assert!(unsettled(&forged_receipt).is_none());
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
        r#"{"slot":null,"exit":0,"fencing":1}"#,
        Some(&claim_hash),
    );
    assert!(unsettled(&format!("{claim}\n{wrong_slot}\n")).is_none());
    assert!(unsettled(&format!("{claim}\n{wrong_fence}\n")).is_none());
    assert!(
        unsettled(&format!("{claim}\n{receipt}\n"))
            .expect("valid journal")
            .collect::<Vec<_>>()
            .is_empty()
    );
}

#[test]
fn unsettled_keeps_an_earlier_claim_when_only_a_later_claim_settles() {
    let slot_a = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let slot_b = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let (claim_a, first_hash) = line(
        1,
        "claimed",
        Some(slot_a),
        r#"{"deadline":"2026-08-20T03:00:00Z","fencing":1,"gen":null}"#,
        None,
    );
    let (claim_b, second_hash) = line(
        2,
        "claimed",
        Some(slot_b),
        r#"{"deadline":"2026-08-20T03:00:00Z","fencing":2,"gen":null}"#,
        Some(&first_hash),
    );
    let (receipt_b, _) = line(
        3,
        "fired",
        Some(slot_b),
        r#"{"slot":null,"exit":0,"fencing":2,"gen":null}"#,
        Some(&second_hash),
    );
    let text = format!("{claim_a}\n{claim_b}\n{receipt_b}\n");
    let unsettled = unsettled(&text).expect("valid journal").collect::<Vec<_>>();
    assert_eq!(unsettled.len(), 1);
    assert_eq!(unsettled[0].slot_id.as_str(), slot_a);
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
    assert_eq!(tallies([(&*forged_tail, true)]), None);

    let (versioned_first, first_hash) = line(1, "skipped", None, r#"{"slot":null}"#, None);
    let slot = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let (versioned_claim, claim_hash) = line(
        2,
        "claimed",
        Some(slot),
        r#"{"deadline":"2026-08-20T03:00:00Z","fencing":2,"gen":null}"#,
        Some(&first_hash),
    );
    let (versioned_receipt, _) = line(
        3,
        "fired",
        Some(slot),
        r#"{"slot":"2026-08-19T03:00:00Z","exit":0,"fencing":2,"gen":null}"#,
        Some(&claim_hash),
    );
    let versioned = format!("{versioned_first}\n{versioned_claim}\n{versioned_receipt}\n");
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
fn only_an_explicit_legacy_bare_receipt_projects() {
    let (receipt, _) = line_at(
        1,
        "2026-08-19T03:02:00Z",
        "paused",
        None,
        r#"{"slot":"2026-08-19T03:00:00Z","trace":null,"exit":4,"fencing":null,"gen":null}"#,
        None,
    );
    assert!(replay_projection([(&*receipt, true)]).is_none());
    let receipt = r#"{"slot":"2026-08-19T03:00:00Z","decided_at":"2026-08-19T03:02:00Z","kind":"paused","trace":null,"exit":4}"#;
    let (last, watermark) = replay_projection([(receipt, false)]).expect("explicit legacy receipt");
    let last = last.expect("projection");
    assert_eq!(last.kind, DecisionKind::Paused);
    assert_eq!(watermark, Some(ts("2026-08-19T03:02:00Z")));
    let (state, _, lifecycle_slot) =
        replay_state([(receipt, false)], &ts("2026-08-19T03:03:00Z")).expect("state");
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
