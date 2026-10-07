// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The public round journal of the sketch door, its graph and its fills: what the seat wrote
//! freely (its notes; a refused round's question keys and gaps) is kept by digest, what the laws
//! admitted stays readable, and a decode error is stated by its class and position. Scripted
//! providers only; no network, no key.
//!
//! The retired native door's own rounds (a whole-source answer, its ask and its judged round)
//! are gone with it; their journal laws hold on the sketch door: a refused round's notes, keys
//! and gaps by digest in `a_refused_sketch_keeps_its_free_text_and_question_keys_off_the_public_document`,
//! an answer outside the schema by class and position in
//! `an_answer_outside_the_sketch_schema_is_refused_by_class_and_position_unechoed`, a revision's
//! notes by digest in the semantic revision suite.
//!
//! Known public paths outside this boundary stay where they are and are not claimed here: the
//! structural and admission diagnostics, which name what they refuse.

use super::*;

/// The withheld form of a text the journal keeps by digest.
fn assert_withheld(kept: &Value, text: &str, what: &str) {
    assert_eq!(kept["withheld"], true, "{what}: {kept}");
    assert_eq!(kept["sha256"], sha(text), "{what}");
    assert_eq!(kept["bytes"], text.len(), "{what}");
}

fn assert_absent(out: &CompileOutcome, sentinels: &[&str]) {
    let document = outcome_document(out).to_string();
    for sentinel in sentinels {
        assert!(!document.contains(sentinel), "{sentinel}: {document}");
    }
}

/// A genuine ask is answered by the human first: the accepted sketch's question and its gap stay
/// in the outcome and in its round by key and clause; only its notes are kept by digest.
#[tokio::test]
async fn an_admitted_sketch_ask_keeps_its_questions_and_gaps_readable() {
    let clause = "la longueur du résumé";
    let mut asking = recap_sketch();
    asking["questions"] = json!([{"key": "const.audience", "label": "Pour quel public ?",
        "answer_type": "text", "why": "La demande ne le dit pas."}]);
    asking["gaps"] = json!([clause]);
    asking["notes"] = json!("ACCEPTED-ASK-NOTES-SENTINEL");
    let mut fills = valid_fills();
    fills[1]["value"] = json!(
        "Résume ces tickets pour ${{ const.audience }} sans rien inventer: ${{ with.tickets }}"
    );
    let provider = Script::texts(&[
        asking.to_string(),
        json!({"fills": fills, "notes": "fills"}).to_string(),
    ]);
    let request =
        CompileRequest::create(SKETCH_INTENT).with_authoring_policy(policy(NativeMode::Sketch, 0));
    let out = compile_with_provider(&request, &provider).await.unwrap();
    assert_eq!(provider.calls(), 2);
    let asked: Vec<&str> = out.questions.iter().map(|q| q.key.as_str()).collect();
    assert!(asked.contains(&"const.audience"), "{out:#?}");
    let round = &native_rounds(&out)[0];
    assert_eq!(round["diagnostics"], json!([]), "{round:#}");
    assert_eq!(round["questions"], json!(["const.audience"]), "{round:#}");
    assert_eq!(round["gaps"], json!([clause]), "{round:#}");
    assert_withheld(&round["notes"], "ACCEPTED-ASK-NOTES-SENTINEL", "notes");
    assert_absent(&out, &["ACCEPTED-ASK-NOTES-SENTINEL"]);
}

/// A sketch answer that is not JSON quotes no answer text: the round says what serde says, by
/// class, and the sketch door buys no repair for it.
#[tokio::test]
async fn a_sketch_syntax_error_keeps_the_decoders_words_and_buys_no_repair() {
    let broken = r#"{"tasks": !}"#.to_owned();
    let provider = Script::texts(&[broken.clone(), recap_sketch().to_string()]);
    let request =
        CompileRequest::create(SKETCH_INTENT).with_authoring_policy(policy(NativeMode::Sketch, 1));
    let out = compile_with_provider(&request, &provider).await.unwrap();
    assert_eq!(provider.calls(), 1);
    let round = &native_rounds(&out)[0];
    assert_eq!(round["failure_class"], "ANSWER_JSON_SYNTAX", "{round}");
    assert_eq!(round["response_sha256"], sha(&broken));
    let said = round["answer"].as_str().unwrap();
    assert!(said.contains("expected value"), "{said}");
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
