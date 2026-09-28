// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;
use crate::compile::{CompileRequest, compile};

/// A public synthetic sentinel, never a real credential.
const SENTINEL: &str = "sk-c7SENTINEL0000000000000000000000";

/// The test's redactor: it removes the sentinel, and leaves every other text whole.
fn redact(text: &str) -> String {
    text.replace(SENTINEL, "[redacted]")
}

/// The questions the deterministic core really asks for an exact skeleton.
fn questions() -> Vec<CompileQuestion> {
    let out = compile(&CompileRequest::create("bounded-batch")).expect("compiles");
    assert!(!out.questions.is_empty(), "the skeleton asks its holes");
    out.questions
}

fn answers(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

fn usable(value: Value) -> RoundRecord {
    match RoundReading::from_raw(value) {
        RoundReading::Usable { record, .. } => *record,
        RoundReading::Unreadable { why, .. } => panic!("unreadable: {why}"),
    }
}

fn unreadable(value: &Value) -> String {
    match RoundReading::from_raw(value.clone()) {
        RoundReading::Unreadable { raw, why } => {
            assert_eq!(&raw, value, "an unreadable value is kept byte for byte");
            why
        }
        RoundReading::Usable { .. } => panic!("read as usable: {value}"),
    }
}

/// A plain kept round: one question waiting, nothing else.
fn plain() -> Value {
    Capture::new("x", &redact)
        .questions(&questions())
        .finish()
        .expect("kept")
}

#[test]
fn a_round_is_kept_and_read_back_exactly() {
    let questions = questions();
    let plan = json!({"strategy": "hot", "observed_world": {"observed": []}});
    let mut receipt = AuthoringReceipt::new("mock/echo");
    receipt.calls = 1;
    receipt.input_tokens = Some(12);
    let knowledge = json!({"pack_sha256": "abc", "presented": true});
    let kept = Capture::new("Read ./data/orders.csv and post the totals", &redact)
        .answers(&answers(&[(
            "rule.1",
            "\"the total of the amount column\"",
        )]))
        .questions(&questions)
        .reasons(&["a reason".to_owned()])
        .restatements(1)
        .continuation(Some(&plan))
        .knowledge(Some(&knowledge))
        .authoring_receipt(Some(&receipt))
        .finish()
        .expect("within the bound");
    let reading = RoundReading::from_raw(kept.clone());
    assert_eq!(
        reading.raw(),
        &kept,
        "the exact value is what the host re-saves"
    );
    let record = usable(kept);
    assert_eq!(record.continuable(), Ok(()));
    assert!(record.request.is_exact());
    assert_eq!(
        record.pending().map(|q| q.key.as_str()),
        Some(questions[0].key.as_str())
    );
    assert_eq!(record.questions.len(), questions.len());
    assert_eq!(
        record.answer_map(),
        answers(&[("rule.1", "\"the total of the amount column\"")])
    );
    assert_eq!(
        record.continuation.as_ref().and_then(|p| p.value.clone()),
        Some(plan)
    );
    assert_eq!(record.knowledge, Some(knowledge));
    let back = record.authoring_receipt().expect("receipt kept");
    assert_eq!(
        (back.model.as_str(), back.calls, back.input_tokens),
        ("mock/echo", 1, Some(12))
    );
    assert_eq!(record.restatements, 1);
}

#[test]
fn a_kept_round_carries_no_authority_field() {
    let kept = plain();
    let keys: Vec<&String> = kept.as_object().expect("an object").keys().collect();
    for forbidden in [
        "money",
        "account",
        "consent",
        "review",
        "identity",
        "question_id",
        "observed",
    ] {
        assert!(
            !keys.iter().any(|k| k.as_str() == forbidden),
            "{forbidden} in {keys:?}"
        );
    }
}

#[test]
fn a_crafted_authority_or_money_field_makes_the_whole_round_unreadable() {
    // A field this schema does not know — a monetary span, a consent, an account, a question
    // identity, an r1 `observed` row — is refused whole: nothing of it reaches a request.
    for (field, value) in [
        ("money", json!([[0, 5]])),
        ("consent", json!(true)),
        ("account", json!({"spent_usd": 0})),
        ("question_id", json!("q-1")),
        ("observed", json!([{"path": "data/orders.csv"}])),
    ] {
        let mut kept = plain();
        kept[field] = value;
        let why = unreadable(&kept);
        assert!(
            why.starts_with("a malformed round record"),
            "{field}: {why}"
        );
        assert!(why.contains(field), "{field}: {why}");
    }
    // The same inside a nested record: an answer that claims an admitted span.
    let mut kept = Capture::new("x", &redact)
        .answers(&answers(&[("budget", "\"$5\"")]))
        .questions(&questions())
        .finish()
        .expect("kept");
    kept["answers"][0]["money"] = json!([0, 2]);
    assert!(unreadable(&kept).contains("money"));
}

#[test]
fn a_redacted_text_is_kept_as_displayed_and_never_continued() {
    let secret_request = format!("post to https://h.example/?token={SENTINEL}");
    let kept = Capture::new(&secret_request, &redact)
        .questions(&questions())
        .finish()
        .expect("kept");
    assert!(
        !kept.to_string().contains(SENTINEL),
        "the sentinel never lands in the record"
    );
    let record = usable(kept);
    assert_eq!(record.continuable(), Err(Unusable::Redacted("request")));
    assert_eq!(record.request.sha256, sha256_hex(secret_request.as_bytes()));

    let kept = Capture::new("post the totals", &redact)
        .answers(&answers(&[("endpoint", &format!("\"{SENTINEL}\""))]))
        .questions(&questions())
        .finish()
        .expect("kept");
    assert!(!kept.to_string().contains(SENTINEL));
    assert_eq!(
        usable(kept).continuable(),
        Err(Unusable::Redacted("answer"))
    );

    // An unrelated business secret in the compiler's own continuation and evidence: never
    // kept, and the round says its continuation was withheld.
    let plan = json!({"observed_world": {"values": [SENTINEL]}});
    let kept = Capture::new("post the totals", &redact)
        .questions(&questions())
        .continuation(Some(&plan))
        .knowledge(Some(&json!({"note": SENTINEL})))
        .finish()
        .expect("kept");
    assert!(
        !kept.to_string().contains(SENTINEL),
        "a business value the redactor names never lands"
    );
    let record = usable(kept);
    assert_eq!(
        record.knowledge, None,
        "evidence the redactor would change is not kept"
    );
    assert_eq!(
        record.continuable(),
        Err(Unusable::Withheld("redacted".to_owned()))
    );
}

#[test]
fn a_text_altered_after_it_was_kept_is_never_continued() {
    let mut kept = Capture::new("post the totals to https://h.example/in", &redact)
        .answers(&answers(&[("rule.1", "\"amount\"")]))
        .questions(&questions())
        .finish()
        .expect("kept");
    let mut request = kept.clone();
    request["request"]["text"] = json!("post the totals to https://evil.example/in");
    assert_eq!(
        usable(request).continuable(),
        Err(Unusable::Redacted("request"))
    );
    kept["answers"][0]["literal"]["text"] = json!("\"price\"");
    assert_eq!(
        usable(kept).continuable(),
        Err(Unusable::Redacted("answer"))
    );
    let said = Unusable::Redacted("answer").to_string();
    assert!(said.contains("altered since"), "{said}");
}

#[test]
fn an_oversized_round_withholds_its_plan_and_a_huge_request_is_not_kept() {
    let plan = json!({"observed_world": {"blob": "x".repeat(ROUND_LIMIT + 1)}});
    let kept = Capture::new("post the totals", &redact)
        .questions(&questions())
        .continuation(Some(&plan))
        .knowledge(Some(&json!({"k": 1})))
        .finish()
        .expect("kept without its plan");
    assert!(kept.to_string().len() <= ROUND_LIMIT);
    let record = usable(kept);
    assert_eq!(
        record.continuable(),
        Err(Unusable::Withheld("over_bound".to_owned()))
    );
    assert_eq!(record.knowledge, None);
    let huge = "y".repeat(ROUND_LIMIT + 1);
    assert_eq!(Capture::new(&huge, &redact).finish(), None);
}

#[test]
fn unreadable_values_are_named_kept_and_never_used() {
    let cases = [
        (json!("not a record"), "a round record without a schema"),
        (json!({"schema": 2}), "round schema 2; this engine reads 1"),
        (json!({"schema": 1}), "a malformed round record"),
    ];
    for (raw, why) in cases {
        let said = unreadable(&raw);
        assert!(said.starts_with(why), "{said}");
    }
    // A record cut short (a field missing) is malformed, never read with a default.
    let mut truncated = plain();
    truncated
        .as_object_mut()
        .expect("an object")
        .remove("answers");
    assert!(unreadable(&truncated).contains("missing field `answers`"));
    // A value of another type is malformed too.
    for (field, value) in [
        ("restatements", json!(300)),
        ("knowledge", json!("a note")),
        ("revises", json!(7)),
        ("request", json!("x")),
    ] {
        let mut kept = plain();
        kept[field] = value;
        let why = unreadable(&kept);
        assert!(why.contains(field), "{field}: {why}");
    }
}

#[test]
fn a_round_with_too_many_answers_or_no_question_is_not_continued() {
    let many: BTreeMap<String, String> = (0..=MAX_ANSWERS)
        .map(|i| (format!("k{i:03}"), "1".to_owned()))
        .collect();
    let kept = Capture::new("x", &redact)
        .answers(&many)
        .questions(&questions())
        .finish()
        .expect("kept");
    assert_eq!(
        usable(kept).continuable(),
        Err(Unusable::TooManyAnswers(MAX_ANSWERS + 1))
    );
    let kept = Capture::new("x", &redact).finish().expect("kept");
    assert_eq!(usable(kept).continuable(), Err(Unusable::NoQuestion));
}

#[test]
fn an_altered_continuation_is_refused() {
    let kept = Capture::new("x", &redact)
        .questions(&questions())
        .continuation(Some(&json!({"strategy": "hot"})))
        .finish()
        .expect("kept");
    let mut edited = kept.clone();
    edited["continuation"]["value"]["strategy"] = json!("native");
    assert_eq!(
        usable(edited).continuable(),
        Err(Unusable::Altered("continuation"))
    );
    let mut emptied = kept;
    emptied["continuation"] = json!({"sha256": "0", "value": null, "withheld": null});
    assert_eq!(
        usable(emptied).continuable(),
        Err(Unusable::Altered("continuation"))
    );
}

#[test]
fn a_round_is_named_in_words_and_knows_the_base_it_revises() {
    let base = "nika: kept\ntasks: {}\n";
    let kept = Capture::new("post the totals", &redact)
        .edit(None, base, "post the totals", None)
        .answers(&answers(&[("model", "\"mock/echo\""), ("limit", "5")]))
        .questions(&questions())
        .revises(Some("prop-1"))
        .finish()
        .expect("kept");
    let record = usable(kept);
    let words = record.summary();
    assert!(
        words.starts_with("« post the totals » · settled"),
        "{words}"
    );
    assert!(
        words.contains("model → mock/echo") && words.contains("limit → 5"),
        "a string literal is read without its quotes: {words}"
    );
    assert!(words.contains(" · waiting: « "), "{words}");
    assert!(words.ends_with(" · it revises proposal prop-1"), "{words}");
    assert!(record.revises_base(base));
    assert!(!record.revises_base("nika: other\n"));
    let unrevised = usable(plain());
    assert!(unrevised.summary().contains("no answer settled"));
    assert!(!unrevised.revises_base("x"), "a round that revises nothing");
}

#[test]
fn a_revision_keeps_its_exact_base_change_and_path() {
    let base = "nika: kept\ntasks: {}\n";
    let kept = Capture::new("add a step", &redact)
        .edit(
            Some("flows/kept.nika"),
            base,
            "add a step",
            Some("the first request"),
        )
        .questions(&questions())
        .revises(Some("prop-1"))
        .finish()
        .expect("kept");
    let record = usable(kept);
    let edit = record.edit.as_ref().expect("an edit");
    assert_eq!(
        edit.path.as_ref().map(|p| p.text.as_str()),
        Some("flows/kept.nika")
    );
    assert_eq!(edit.base.text, base);
    assert_eq!(edit.base.sha256, sha256_hex(base.as_bytes()));
    assert!(edit.change.is_exact());
    assert_eq!(record.revises.as_deref(), Some("prop-1"));
    assert_eq!(record.continuable(), Ok(()));
    // A redacted base, or a redacted path, can never be the EDIT's exact base again.
    let secret_base = format!("nika: kept\nconst: {{token: {SENTINEL}}}\n");
    let kept = Capture::new("add a step", &redact)
        .edit(None, &secret_base, "add a step", None)
        .questions(&questions())
        .finish()
        .expect("kept");
    assert_eq!(
        usable(kept).continuable(),
        Err(Unusable::Redacted("revision"))
    );
    let secret_path = format!("flows/{SENTINEL}.nika");
    let kept = Capture::new("add a step", &redact)
        .edit(Some(&secret_path), base, "add a step", None)
        .questions(&questions())
        .finish()
        .expect("kept");
    assert_eq!(
        usable(kept).continuable(),
        Err(Unusable::Redacted("revision"))
    );
}
