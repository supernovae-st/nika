// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The public round journal of the native door and of the sketch's fills, over the existing
//! revision route: what the seat wrote freely (its notes; a refused round's question keys and
//! gaps) is kept by digest, what the laws admitted stays readable, and a decode error outside
//! the answer schema is stated by its class and position while the seat's own repair still reads
//! the whole error. Scripted providers only; no network, no key.
//!
//! Known public paths outside this boundary stay where they are and are not claimed here: the
//! structural and admission diagnostics (which name what they refuse) and a refused native
//! candidate, kept whole.

use super::*;

/// A revision in words the constant door cannot settle: the native door revises the base.
fn revision(repairs: u32) -> CompileRequest {
    CompileRequest::edit(
        REVISION_BASE,
        "Finalement, résume le texte avant de l'écrire.",
    )
    .with_original_intent("Copie entree.txt dans a.txt.")
    .with_authoring_policy(policy(NativeMode::Escalate, repairs))
}

/// A native answer with these fields over an empty answer.
fn native(fields: &Value) -> String {
    let mut answer = json!({"candidate": "", "candidate_lines": [], "questions": [], "gaps": [],
        "notes": ""});
    for (key, value) in fields.as_object().unwrap() {
        answer[key] = value.clone();
    }
    answer.to_string()
}

fn question(key: &str) -> Value {
    json!({"key": key, "label": "Quel ton garder ?", "answer_type": "text",
        "why": "La demande ne le dit pas."})
}

/// The withheld form of a text the journal keeps by digest.
fn assert_withheld(kept: &Value, text: &str, what: &str) {
    assert_eq!(kept["withheld"], true, "{what}: {kept}");
    assert_eq!(kept["sha256"], sha(text), "{what}");
    assert_eq!(kept["bytes"], text.len(), "{what}");
}

/// The withheld form of a list the journal keeps by digest, with how many items it held.
fn assert_listed(kept: &Value, values: &Value, what: &str) {
    assert_withheld(kept, &values.to_string(), what);
    let items = values.as_array().unwrap().len();
    assert_eq!(
        kept["shape"],
        json!({"type": "array", "items": items}),
        "{what}"
    );
}

fn assert_absent(out: &CompileOutcome, sentinels: &[&str]) {
    let document = outcome_document(out).to_string();
    for sentinel in sentinels {
        assert!(!document.contains(sentinel), "{sentinel}: {document}");
    }
}

/// A refused ask (more than eight questions, a refusal that names none of them) keeps its notes,
/// its question keys and its gaps by digest; no call is added.
#[tokio::test]
async fn a_refused_native_ask_keeps_its_notes_keys_and_gaps_by_digest() {
    let keys: Vec<String> = (0..9)
        .map(|n| format!("const.ask_key_sentinel_{n}"))
        .collect();
    let reply = native(&json!({
        "questions": keys.iter().map(|k| question(k)).collect::<Vec<_>>(),
        "gaps": ["ASK-GAP-SENTINEL"], "notes": "ASK-NOTES-SENTINEL"}));
    let provider = Script::texts(std::slice::from_ref(&reply));
    let out = compile_with_provider(&revision(0), &provider)
        .await
        .unwrap();
    assert_eq!(provider.calls(), 1);
    assert_eq!(context(&out)[0]["response"]["sha256"], sha(&reply));
    assert_absent(
        &out,
        &["ASK-NOTES-SENTINEL", "ASK-GAP-SENTINEL", "ask_key_sentinel"],
    );
    let rounds = native_rounds(&out);
    let ask = &rounds[0];
    assert!(
        ask["diagnostics"]
            .to_string()
            .contains("at most eight questions"),
        "{ask:#}"
    );
    assert_withheld(&ask["notes"], "ASK-NOTES-SENTINEL", "notes");
    assert_listed(&ask["asked"], &json!(keys), "asked");
    assert_listed(&ask["gaps"], &json!(["ASK-GAP-SENTINEL"]), "gaps");
}

/// A genuine ask is answered by the human first: its admitted question and its gap stay in the
/// outcome and in its round by key and clause; only its notes are kept by digest.
#[tokio::test]
async fn an_admitted_native_ask_keeps_its_questions_and_gaps_readable() {
    let clause = "la longueur du résumé";
    let reply = native(
        &json!({"questions": [question("const.tone")], "gaps": [clause],
        "notes": "ACCEPTED-ASK-NOTES-SENTINEL"}),
    );
    let provider = Script::texts(std::slice::from_ref(&reply));
    let out = compile_with_provider(&revision(0), &provider)
        .await
        .unwrap();
    assert_eq!(provider.calls(), 1);
    let asked: Vec<&str> = out.questions.iter().map(|q| q.key.as_str()).collect();
    assert!(asked.contains(&"const.tone"), "{out:#?}");
    assert!(asked.iter().any(|key| key.starts_with("gap.")), "{asked:?}");
    let ask = &native_rounds(&out)[0];
    assert_eq!(ask["diagnostics"], json!([]), "{ask:#}");
    assert_eq!(ask["asked"], json!(["const.tone"]));
    assert_eq!(ask["gaps"], json!([clause]));
    assert_withheld(&ask["notes"], "ACCEPTED-ASK-NOTES-SENTINEL", "notes");
    assert_absent(&out, &["ACCEPTED-ASK-NOTES-SENTINEL"]);
}

/// A judged native round the laws refuse keeps the seat's notes, and its question keys and gaps,
/// by digest; the refused candidate itself stays the known whole-candidate path.
#[tokio::test]
async fn a_refused_judged_native_round_keeps_its_notes_keys_and_gaps_by_digest() {
    let reply = native(&json!({"candidate": "nika: x\ntasks: {}\n",
        "questions": [question("const.judged_key_sentinel")], "gaps": ["JUDGED-GAP-SENTINEL"],
        "notes": "JUDGED-NOTES-SENTINEL"}));
    let provider = Script::texts(std::slice::from_ref(&reply));
    let out = compile_with_provider(&revision(0), &provider)
        .await
        .unwrap();
    assert_eq!(provider.calls(), 1);
    let rounds = native_rounds(&out);
    let judged = &rounds[0];
    assert!(judged.get("candidate_sha256").is_some(), "{judged:#}");
    assert!(
        !judged["diagnostics"].as_array().unwrap().is_empty(),
        "{judged:#}"
    );
    assert_withheld(&judged["notes"], "JUDGED-NOTES-SENTINEL", "notes");
    assert_listed(&judged["gaps"], &json!(["JUDGED-GAP-SENTINEL"]), "gaps");
    assert_listed(
        &judged["questions"],
        &json!(["const.judged_key_sentinel"]),
        "questions",
    );
    assert_absent(&out, &["JUDGED-NOTES-SENTINEL", "JUDGED-GAP-SENTINEL"]);
    // The key is gone from its round; the outcome names it only where a refusal or the whole
    // refused candidate does (the known paths this witness does not claim).
    let journal = json!(rounds).to_string();
    let elsewhere = journal.matches("judged_key_sentinel").count();
    let named: usize = judged["diagnostics"]
        .to_string()
        .matches("judged_key_sentinel")
        .count();
    assert_eq!(elsewhere, named, "only a diagnostic may name it: {journal}");
}

/// A native answer outside its schema is refused by class and position, the seat's key or value
/// never repeated; a JSON syntax error keeps its words, and its repair still reads them.
#[tokio::test]
async fn a_native_answer_outside_its_schema_is_refused_by_class_and_position() {
    for (reply, sentinel) in [
        (
            native(&json!({"candidate": "nika: x\ntasks: {}\n", "native_key_sentinel": 1})),
            "native_key_sentinel",
        ),
        (
            native(&json!({"questions": "NATIVE-VALUE-SENTINEL"})),
            "NATIVE-VALUE-SENTINEL",
        ),
    ] {
        let provider = Script::texts(std::slice::from_ref(&reply));
        let out = compile_with_provider(&revision(1), &provider)
            .await
            .unwrap();
        assert_absent(&out, &[sentinel]);
        assert_eq!(
            provider.calls(),
            1,
            "a schema error buys no repair: {reply}"
        );
        let round = &native_rounds(&out)[0];
        assert_eq!(round["failure_class"], "ANSWER_SCHEMA", "{round}");
        assert_eq!(round["response_sha256"], sha(&reply));
        let column = round["decode_error"]["column"].as_u64().unwrap();
        let reason = format!("the answer schema, line 1, column {column}");
        assert_eq!(round["answer"], format!("not a native answer: {reason}"));
        assert!(
            out.diagnostics
                .iter()
                .any(|d| d.target == "authoring_native"
                    && d.message
                        .contains(&format!("not a native answer ({reason})"))),
            "{:#?}",
            out.diagnostics
        );
    }
    // A syntax error quotes no answer text: the round says what serde says, and the paid repair
    // sends those words back to the seat.
    let broken = r#"{"candidate": !}"#.to_owned();
    let provider = Script::texts(&[broken, native_answer()]);
    let out = compile_with_provider(&revision(1), &provider)
        .await
        .unwrap();
    assert_eq!(provider.calls(), 2);
    let round = &native_rounds(&out)[0];
    assert_eq!(round["failure_class"], "ANSWER_JSON_SYNTAX", "{round}");
    let said = round["answer"].as_str().unwrap();
    assert!(said.contains("expected value"), "{said}");
    let repair = &provider.seen.lock().unwrap()[1].user;
    assert!(repair.contains("expected value"), "{repair}");
}

/// A fill's notes are kept by digest, on an accepted fill round and on a refused one.
#[tokio::test]
async fn fill_notes_are_kept_by_digest_accepted_or_refused() {
    let mut ghost = valid_fills();
    ghost.push(json!({"task": "ghost", "field": "prompt", "value": "x"}));
    for (fills, accepted) in [(valid_fills(), true), (ghost, false)] {
        let notes = if accepted {
            "FILL-NOTES-ACCEPTED-SENTINEL"
        } else {
            "FILL-NOTES-REFUSED-SENTINEL"
        };
        let provider = Script::texts(&[
            recap_sketch().to_string(),
            json!({"fills": fills, "notes": notes}).to_string(),
        ]);
        let request = CompileRequest::create(SKETCH_INTENT)
            .with_authoring_policy(policy(NativeMode::Sketch, 0));
        let out = compile_with_provider(&request, &provider).await.unwrap();
        assert_eq!(provider.calls(), 2);
        let rounds = native_rounds(&out);
        let fill = &rounds[1];
        assert_eq!(fill["phase"], "fill", "{fill:#}");
        assert_eq!(
            fill["diagnostics"].as_array().unwrap().is_empty(),
            accepted,
            "{fill:#}"
        );
        assert_withheld(&fill["notes"], notes, "fill notes");
        assert_absent(&out, &[notes]);
    }
}
