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
    assert!(reading.record().is_ok() && reading.continuable().is_some());
    let words = reading.words().expect("read");
    assert!(words.summary.starts_with("« Read ./data/orders.csv"));
    assert_eq!(words.asked.as_deref(), Some(questions[0].why.as_str()));
    assert_eq!(words.blocked, None, "a continuable round is not blocked");
    let unreadable = RoundReading::from_raw(json!({"schema": 2}));
    assert!(unreadable.record().is_err() && unreadable.continuable().is_none());
    assert_eq!(
        unreadable.words(),
        Err("round schema 2; this engine reads 1"),
        "an unreadable round says why, never a summary"
    );
    let redacted = RoundReading::from_raw(
        Capture::new(&format!("post {SENTINEL}"), &redact)
            .questions(&questions)
            .finish()
            .expect("kept"),
    );
    assert!(redacted.record().is_ok() && redacted.continuable().is_none());
    let blocked = redacted.words().expect("read").blocked.expect("blocked");
    assert_eq!(blocked, Unusable::Redacted("request").to_string());
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

/// C11 · R2 · a request is named as typed and as rebuilt only when a restatement rebuilt the
/// sentence the human typed; a trailing line break is never shown inside « »; a revision is named
/// by its own request whatever was typed.
#[test]
fn a_request_is_named_as_typed_only_when_a_restatement_rebuilt_it() {
    assert_eq!(as_typed(None, "Log hi\n"), "« Log hi »");
    assert_eq!(as_typed(Some("Log hi\n"), "Log hi"), "« Log hi »");
    assert_eq!(as_typed(Some(" \n"), "Log hi"), "« Log hi »");
    assert_eq!(
        as_typed(Some("sum it\n"), "sum the amount column\n"),
        "« sum it » as you typed it · rebuilt as « sum the amount column »"
    );
    let revision = usable(
        Capture::new("post the totals", &redact)
            .edit(None, "nika: kept\ntasks: {}\n", "post the totals", None)
            .questions(&questions())
            .finish()
            .expect("kept"),
    );
    assert_eq!(
        revision.request_as_typed(Some("log hi")),
        "« post the totals »"
    );
    let created = usable(plain());
    let typed = format!("{} in other words", created.request.text.trim_end());
    let words = created.summary_as_typed(Some(&typed));
    assert!(
        words.starts_with(&format!("« {typed} » as you typed it · rebuilt as « ")),
        "{words}"
    );
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

/// C10 · D-R · the live round's law beside its codec: the request a round is (an EDIT with its
/// original request, every answer, the plan, the admitted spans carried as data), what an
/// outcome settled, the questions it leaves, a re-anchored plan and a carried receipt.
#[test]
fn the_live_round_law_lives_beside_the_codec() {
    let edit = (
        "nika: base\n".to_owned(),
        "also post it".to_owned(),
        Some("the first request".to_owned()),
    );
    let plan = json!({"strategy": "hot", "observed_world": {"observed": []}});
    let settled_answers = answers(&[("rule.1", "\"all rows\"")]);
    let admitted = 0..5;
    let spans = std::slice::from_ref(&admitted);
    let edited = request("x", Some(&edit), &settled_answers, Some(&plan), spans);
    assert_eq!(edited.original_intent.as_deref(), Some("the first request"));
    assert_eq!(edited.answers, settled_answers);
    assert_eq!(edited.plan, Some(plan.clone()));
    assert_eq!(edited.money, spans, "admitted spans ride as data");
    let created = request("x", None, &BTreeMap::new(), None, &[]);
    assert!(created.plan.is_none() && created.money.is_empty() && created.answers.is_empty());

    let mut out = compile(&CompileRequest::create("bounded-batch")).expect("compiles");
    out.provenance.strategy = None;
    assert_eq!(settled(&out), None, "no strategy settled: no continuation");
    out.provenance.strategy = Some(crate::compile::Strategy::Native);
    out.provenance.plan = Some(plan.clone());
    let mut receipt = AuthoringReceipt::new("claude-code/default");
    receipt.backend = Some(json!({"kind": "harness_infer"}));
    out.provenance.authoring = Some(receipt.clone());
    let kept = settled(&out).expect("settled");
    assert_eq!((kept.plan, kept.receipt), (plan.clone(), Some(receipt)));
    out.provenance
        .authoring
        .as_mut()
        .expect("a receipt")
        .backend = None;
    assert_eq!(
        settled(&out).expect("settled").receipt,
        None,
        "only a harness receipt"
    );

    let mandatory = out.questions.iter().filter(|q| q.mandatory).count();
    assert!(mandatory > 0, "the skeleton asks");
    let mut clarification = out.questions[0].clone();
    clarification.key = CLARIFICATION_KEY.to_owned();
    let mut gap = out.questions[0].clone();
    (gap.key, gap.mandatory) = ("gap.1".to_owned(), false);
    out.questions.extend([clarification, gap]);
    let (asked, _) = open(&out, false);
    assert_eq!(
        asked.len(),
        mandatory + 1,
        "a CREATE asks the clarification, never a gap"
    );
    let (asked, _) = open(&out, true);
    assert!(
        asked.iter().any(|q| q.key == "gap.1"),
        "a revision asks its dispositions"
    );
    assert!(
        !asked.iter().any(|q| q.key == CLARIFICATION_KEY),
        "never a replacement"
    );

    let recorded = json!({"observed_world": {"observed": ["old"]}});
    assert_eq!(reanchored(Some(&recorded), &out), Some(plan.clone()));
    assert_eq!(reanchored(Some(&plan), &out), None, "nothing moved");
    let approved = json!({"observed_world": 1, "verified_transform": {}});
    assert_eq!(
        reanchored(Some(&approved), &out),
        None,
        "an approval stays bound"
    );

    let mut replay = compile(&CompileRequest::create("bounded-batch")).expect("compiles");
    let mut carried = AuthoringReceipt::new("claude-code/default");
    carried.backend = Some(json!({"kind": "harness_infer"}));
    carry_receipt(&mut replay, Some(&carried));
    let named = replay.provenance.authoring.clone().expect("carried");
    assert_eq!(
        named.backend.expect("backend")["carried_from_authoring_round"],
        true
    );
    let own = replay.provenance.authoring.clone();
    carry_receipt(&mut replay, Some(&AuthoringReceipt::new("other/model")));
    assert_eq!(
        replay.provenance.authoring, own,
        "an outcome's own receipt stays"
    );

    let kept = Capture::new("also post it", &redact)
        .edit(None, &edit.0, &edit.1, edit.2.as_deref())
        .questions(&questions())
        .finish()
        .expect("kept");
    assert_eq!(usable(kept).edit.expect("an edit").texts(), edit);
}

/// A kept round whose subscription receipt is `receipt` as a host kept it.
fn with_receipt(receipt: Value) -> RoundRecord {
    let mut kept = plain();
    kept["authoring_receipt"] = receipt;
    usable(kept)
}

/// C10 · a kept receipt is read back only as it was written: a generated receipt and its
/// nullable fields left null come back exactly; a field of another type, a negative or
/// fractional count, or a missing key is never read as zero, empty or absent — the whole
/// receipt is refused, so a continued round never claims a receipt nobody measured.
#[test]
fn a_malformed_kept_receipt_is_refused_never_read_as_zero() {
    let mut generated = AuthoringReceipt::new("claude-code/default");
    generated.calls = 3;
    generated.input_tokens = Some(12);
    generated.output_tokens = Some(34);
    generated.elapsed_ms = 56;
    generated.context = vec![json!("brief.md")];
    generated.backend = Some(json!({"kind": "harness_infer"}));
    let kept = Capture::new("x", &redact)
        .questions(&questions())
        .authoring_receipt(Some(&generated))
        .finish()
        .expect("kept");
    let receipt = kept["authoring_receipt"].clone();
    assert_eq!(usable(kept).authoring_receipt(), Some(generated.clone()));
    let mut nullable = receipt.clone();
    for key in ["input_tokens", "output_tokens", "backend"] {
        nullable[key] = Value::Null;
    }
    let mut legacy = generated.clone();
    (legacy.input_tokens, legacy.output_tokens, legacy.backend) = (None, None, None);
    assert_eq!(with_receipt(nullable).authoring_receipt(), Some(legacy));
    for (key, value) in [
        ("calls", json!("7")),
        ("calls", json!(-1)),
        ("calls", json!(1.5)),
        ("calls", json!(u64::from(u32::MAX) + 1)),
        ("elapsed_ms", json!(null)),
        ("elapsed_ms", json!("56")),
        ("input_tokens", json!("12")),
        ("output_tokens", json!(-3)),
        ("context", json!("brief.md")),
        ("context", json!(null)),
        ("model", json!(7)),
    ] {
        let mut malformed = receipt.clone();
        malformed[key] = value.clone();
        assert_eq!(
            with_receipt(malformed).authoring_receipt(),
            None,
            "{key} = {value}"
        );
    }
    for key in ["calls", "elapsed_ms", "input_tokens", "context", "backend"] {
        let mut missing = receipt.clone();
        missing.as_object_mut().expect("an object").remove(key);
        assert_eq!(
            with_receipt(missing).authoring_receipt(),
            None,
            "{key} missing"
        );
    }
}

/// C10 · B15 · an EDIT's change carries its own admitted directives: the money law's spans of
/// the exact change it holds (bytes, whatever Unicode the line held before it), the business
/// clause left the change's own, none when the gate admitted no money in the line, and never a
/// span shifted from the whole line.
#[test]
fn a_change_carries_its_own_admitted_directives_as_the_law_reads_it() {
    let line = "\u{3000}\u{a0} add a step that logs bye, budget 0 USD  ";
    let change = line.trim();
    let spans = change_money(change, true);
    assert_eq!(spans.len(), 1, "{spans:?}");
    assert_eq!(change[spans[0].clone()].trim(), "budget 0 USD");
    assert!(
        !change[spans[0].clone()].contains("logs bye"),
        "the business clause stays the change's own"
    );
    let in_line = line.find("budget").expect("stated");
    assert!(
        spans[0].end <= change.len() && spans[0].start < in_line,
        "offsets are the change's own, never the line's"
    );
    assert!(
        change_money(change, false).is_empty(),
        "no money admitted in the line: no span"
    );
    assert!(change_money("add a step that logs bye", true).is_empty());
}

/// C10 · the request a Ready outcome is kept with is the one its compile round read: the round's
/// own request, its answer and the plan it continued (with that plan's earlier observation), and
/// the observation this round was given, kept apart. A round the host gave no observation keeps
/// none; an attached observation the outcome did not record exactly cannot be rebuilt.
#[test]
fn a_compiled_request_keeps_its_own_observation_apart_from_the_one_it_continued() {
    let intent =
        "read ./orders.csv, keep only the rows whose status is open and write them to ./out.json";
    let absent = |path: &str| json!({"path": path, "state": "absent", "complete": false});
    let continued = json!({"observed": [absent("./orders.csv"), absent("./out.json")]});
    let given = json!({"observed": [absent("./orders.csv"),
        {"path": "./out.json", "state": "empty", "kind": "json", "complete": false}]});
    let asked = compile(&CompileRequest::create(intent).with_knowledge(continued.clone()))
        .expect("the question round");
    let key = asked
        .questions
        .iter()
        .map(|q| q.key.clone())
        .find(|key| key.starts_with("const.rule_field_"))
        .expect("the field is asked");
    let round = CompileRequest::create(intent)
        .with_plan(asked.provenance.plan.clone().expect("a question plan"))
        .answer(key.as_str(), "\"status\"");
    let ready = compile(&round.clone().with_knowledge(given.clone())).expect("the answer round");
    assert_eq!(
        ready.status,
        crate::compile::CompileStatus::Ready,
        "{ready:?}"
    );
    let stamped = crate::knowledge::pin::observed_in(ready.clone(), Some(&given));
    let kept = compiled(round.clone(), &stamped).expect("rebuilt");
    assert_eq!(kept.knowledge.as_ref(), Some(&given), "this round's own");
    assert_eq!(
        kept.plan.as_ref().map(|plan| &plan["observed_world"]),
        Some(&continued),
        "the continued plan's, untouched"
    );
    assert_eq!(kept.answers, round.answers);
    assert_eq!(
        compiled(round.clone(), &ready).map(|request| request.knowledge),
        Some(None),
        "a round given none keeps none"
    );
    let mut unrecorded = stamped.clone();
    unrecorded.provenance.plan = None;
    assert!(compiled(round.clone(), &unrecorded).is_none());
    let mut other = stamped;
    other.provenance.plan.as_mut().expect("a plan")["observed_world"] = continued;
    assert!(
        compiled(round, &other).is_none(),
        "never an observation it was not given"
    );
}

/// C10 · a reviewed counterexample: a world of the same shape as the one the host attached (same
/// paths, states, kinds and column counts: `[status]` read as `[state]`, or a sample now called
/// complete) is never taken for it. A real compile of `orders.csv` observed with its `status`
/// column, stamped with that world; the attached world itself is kept (the control).
#[test]
fn a_same_shape_world_is_never_taken_for_the_one_the_host_attached() {
    let intent =
        "read ./orders.csv, keep only the rows whose status is open and write them to ./out.json";
    let row = json!({"path": "./orders.csv", "state": "observed", "complete": false,
        "kind": "csv", "columns": ["id", "status"]});
    let world = json!({"observed": [row]});
    let request = CompileRequest::create(intent);
    let out = compile(&request.clone().with_knowledge(world.clone())).expect("compiles");
    assert!(
        out.provenance
            .plan
            .as_ref()
            .is_some_and(|plan| plan["observed_world"] == world),
        "the compiler records the world it read: {:?}",
        out.provenance.plan
    );
    let stamped = crate::knowledge::pin::observed_in(out, Some(&world));
    let kept = compiled(request.clone(), &stamped).expect("the attached world is kept");
    assert_eq!(kept.knowledge.as_ref(), Some(&world));
    for (field, other) in [
        ("columns", json!(["id", "state"])),
        ("complete", json!(true)),
    ] {
        let mut same_shape = stamped.clone();
        same_shape.provenance.plan.as_mut().expect("a plan")["observed_world"]["observed"][0]
            [field] = other;
        assert!(
            compiled(request.clone(), &same_shape).is_none(),
            "a world of the same shape with other {field} is not the attached one"
        );
    }
    // A record naming no identity (an older one) is never taken as exact.
    let mut legacy = stamped;
    let record = legacy
        .provenance
        .decision
        .as_mut()
        .expect("a decision record");
    let observed = record["session"]["observed"]
        .as_object_mut()
        .expect("the host's record");
    assert!(observed.remove("world_sha256").is_some());
    assert!(compiled(request, &legacy).is_none());
}

/// A semantic record keeps no observation of its own: its request is rebuilt from the one the
/// host's record discloses only when its full identity matches the host receipt and its scoped
/// identity matches the reading. A changed relevant fact or missing identity rebuilds nothing.
#[test]
fn a_semantic_record_requires_both_the_full_receipt_and_the_scoped_reading_identity() {
    fn observed(out: &mut CompileOutcome) -> &mut Value {
        &mut out.provenance.decision.as_mut().expect("a decision record")["session"]["observed"]
    }
    let intent = "read ./inventory.json, keep the items whose stock is under 8";
    let world = json!({"observed": [{"path": "./inventory.json", "state": "observed",
        "complete": false, "kind": "json", "columns": ["sku", "stock"]}]});
    let identity = nika_compile_fidelity::observed::basis::of_request(Some(&world), intent);
    let request = CompileRequest::create(intent);
    let mut out = compile(&request.clone().with_knowledge(world.clone())).expect("compiles");
    out.provenance.plan = Some(json!({"semantic_record": 1,
        "basis": {"read": {"world_sha256": identity, "effective": intent}}}));
    let stamped = crate::knowledge::pin::observed_in(out, Some(&world));
    let kept = compiled(request.clone(), &stamped).expect("the attached world is kept");
    assert_eq!(kept.knowledge.as_ref(), Some(&world));
    // An extra destination is irrelevant to the original reading but belongs to the host's
    // complete receipt. The rebuilt request retains that full observation, never a summary.
    let mut expanded = world.clone();
    expanded["observed"]
        .as_array_mut()
        .expect("rows")
        .push(json!({"path": "./next.json", "state": "absent"}));
    let restamped = crate::knowledge::pin::observed_in(stamped.clone(), Some(&expanded));
    assert_eq!(
        compiled(request.clone(), &restamped)
            .expect("same reading")
            .knowledge,
        Some(expanded)
    );
    let mut unnamed = stamped.clone();
    unnamed.provenance.plan.as_mut().expect("a plan")["basis"]["read"]
        .as_object_mut()
        .expect("reading")
        .remove("effective");
    assert!(compiled(request.clone(), &unnamed).is_none());
    // Read under another world.
    let mut other = stamped.clone();
    other.provenance.plan.as_mut().expect("a plan")["basis"]["read"]["world_sha256"] =
        json!("0".repeat(64));
    assert!(compiled(request.clone(), &other).is_none());
    // A disclosed world of the same shape, or none at all.
    let mut same_shape = stamped.clone();
    observed(&mut same_shape)["world"]["observed"][0]["columns"] = json!(["sku", "qty"]);
    assert!(compiled(request.clone(), &same_shape).is_none());
    let mut missing = stamped.clone();
    let record = observed(&mut missing).as_object_mut().expect("an object");
    assert!(record.remove("world").is_some());
    assert!(compiled(request.clone(), &missing).is_none());
    // A semantic record never falls back to an `observed_world` it does not keep.
    let mut decorated = stamped;
    decorated.provenance.plan.as_mut().expect("a plan")["observed_world"] = world;
    let record = observed(&mut decorated).as_object_mut().expect("an object");
    assert!(record.remove("world").is_some());
    assert!(compiled(request, &decorated).is_none());
}
