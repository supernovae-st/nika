// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! The Session's two document doors read a question's answer within its scope: the presented door
//! (`verify_document_presented_with`, the one a proposal takes) and the laws-only door of its desk
//! (`judge_document_answered`). A pick or a free entry settles the open name its question quotes;
//! an answer bound to an option the person did not take, or given for another name, settles none,
//! even with the person's reply line among the words, as a Session's verifier words hold it.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use nika_compile::{CompileOutcome, CompileRequest};
use nika_compile_cognition::{
    Cognition, NoProvider, judge_document_answered, verify_document_presented_with,
};
use serde_json::{Value, json};

/// Two outputs in two folders, so where the JSON output's name starts stays open.
const REQUEST: &str = "Prépare un workflow qui lit articles.csv et ne garde que les articles en rupture. Il faudra générer out/stock.json avec le nombre et la liste des références, puis un court résumé dans notes/stock.md.";
/// The same request with a second name that shares the path alone.
const TWO: &str = "Prépare un workflow qui lit articles.csv. Il faudra générer out/stock.json avec le total, puis il faudra relire out/stock.json pour un court résumé dans notes/stock.md.";
/// The open name the question asks about, and the question quoting it, with its key.
const NAME: &str = "faudra générer out/stock.json";
const ASKED: &str = "Quel fichier désigne « faudra générer out/stock.json » ?";
const KEY: &str = "const.stock_path";

/// A document reading each of `reads` and writing each of `writes`, each permitted exactly.
fn document(reads: &[&str], writes: &[&str]) -> String {
    let mut source = "nika: stock\npermits:\n  fs:\n".to_owned();
    let _ = writeln!(source, "    read: {reads:?}\n    write: {writes:?}");
    source.push_str("  tools: [\"nika:read\", \"nika:write\"]\ntasks:\n");
    for (k, path) in reads.iter().enumerate() {
        let _ = writeln!(
            source,
            "  read_{k}:\n    invoke: {{ tool: \"nika:read\", args: {{ path: \"{path}\" }} }}"
        );
    }
    for (k, path) in writes.iter().enumerate() {
        let _ = writeln!(
            source,
            "  write_{k}:\n    invoke: {{ tool: \"nika:write\", args: {{ path: \"{path}\", content: \"ok\" }} }}"
        );
    }
    source
}

/// The Session's description of an answer to the question: a pick of `option`, or, with none,
/// the person's own words (a free entry), binding `value` from the line `u2`.
fn answer(option: Option<&str>, value: &str) -> Value {
    let (kind, selected) = match option {
        Some(option) => {
            let provenance =
                json!({"kind": "offered", "message": "u2", "question": KEY, "option": option});
            let offer = json!({"key": option, "label": value, "provenance": provenance,
                "values": [{"role": "value", "value": value}]});
            (provenance, vec![offer])
        }
        None => (
            json!({"kind": "answered", "message": "u2", "excerpt": value}),
            Vec::new(),
        ),
    };
    let reply = if option.is_some() {
        "le chemin seul"
    } else {
        value
    };
    json!({"bound_values": value,
        "bindings": [{"key": KEY, "role": "value", "value": value, "provenance": kind}],
        "choice_context": {"question_key": KEY, "question": ASKED, "human_citation": "u2",
            "selected_options": selected, "human_reply": reply, "human_lines": [1, 2]}})
}

/// The diagnostics an outcome states for `target`.
fn findings(out: &CompileOutcome, target: &str) -> Vec<String> {
    (out.diagnostics.iter())
        .filter(|d| d.target == target)
        .map(|d| d.message.clone())
        .collect()
}

/// The paths the presented door and the laws-only door find unrealized in `source` for
/// `stated`, with `answer` as the person's answer to the question when there is one.
async fn owed_at_both(
    stated: &str,
    source: &str,
    answer: Option<&Value>,
) -> (Vec<String>, Vec<String>) {
    let mut request = CompileRequest::create(stated);
    let mut answers = BTreeMap::new();
    if let Some(answer) = answer {
        request = request.answer(KEY, answer.to_string());
        answers.insert(KEY.to_owned(), answer.to_string());
    }
    let none = Cognition::<NoProvider> {
        provider: None,
        seat: None,
    };
    let presentation = json!({"kind": "proposal", "pending_choices": [], "selected": null});
    let presented = verify_document_presented_with(
        &request,
        source,
        (&[], &[]),
        &presentation,
        &[],
        none,
        None,
    )
    .await
    .unwrap();
    let door = (findings(&presented, "conversation").into_iter())
        .filter(|message| message.starts_with("UNREALIZED PATH"))
        .collect();
    let laws = (judge_document_answered(stated, source, (&[], &[]), &answers, None))
        .into_iter()
        .filter(|law| law.starts_with("UNREALIZED PATH"))
        .collect();
    (door, laws)
}

/// Whether `owed` names exactly the one path `path`.
fn names(owed: &[String], path: &str) -> bool {
    owed.len() == 1 && owed[0].contains(&format!("`{path}`"))
}

/// A question's answer counts on both document doors within its scope. Unanswered, the open name
/// is owed whole; the person's pick of the path alone, or their own words for it with no option,
/// settles it at both doors, and the Markdown output stays owed on its own. A pick bound to an
/// option the person did not take settles it at neither, and an answer given for one name never
/// settles another name that shares the reading, its reply line among the words or not.
#[tokio::test]
async fn a_question_answer_counts_on_both_document_doors_within_its_scope() {
    let both = document(&["articles.csv"], &["out/stock.json", "notes/stock.md"]);
    let (door, laws) = owed_at_both(REQUEST, &both, None).await;
    assert!(
        names(&door, NAME) && names(&laws, NAME),
        "{door:#?} {laws:#?}"
    );
    for settled in [
        answer(Some("reading-3"), "out/stock.json"),
        answer(None, "out/stock.json"),
    ] {
        let (door, laws) = owed_at_both(REQUEST, &both, Some(&settled)).await;
        assert_eq!((door.len(), laws.len()), (0, 0), "{door:#?} {laws:#?}");
    }
    let json_only = document(&["articles.csv"], &["out/stock.json"]);
    let picked = answer(Some("reading-3"), "out/stock.json");
    let (door, laws) = owed_at_both(REQUEST, &json_only, Some(&picked)).await;
    let markdown = "notes/stock.md";
    assert!(
        names(&door, markdown) && names(&laws, markdown),
        "{door:#?} {laws:#?}"
    );
    let mut other = picked.clone();
    other["bindings"][0]["provenance"]["option"] = json!("reading-1");
    let (door, laws) = owed_at_both(REQUEST, &both, Some(&other)).await;
    assert!(
        names(&door, NAME) && names(&laws, NAME),
        "{door:#?} {laws:#?}"
    );
    let read_too = document(
        &["articles.csv", "out/stock.json"],
        &["out/stock.json", "notes/stock.md"],
    );
    let typed = answer(None, "out/stock.json");
    let (door, laws) = owed_at_both(TWO, &read_too, Some(&typed)).await;
    let relire = "faudra relire out/stock.json";
    assert!(
        names(&door, relire) && names(&laws, relire),
        "{door:#?} {laws:#?}"
    );
    let replied = format!("{TWO}\nout/stock.json");
    let (door, laws) = owed_at_both(&replied, &read_too, Some(&typed)).await;
    assert!(
        names(&door, relire) && names(&laws, relire),
        "{door:#?} {laws:#?}"
    );
}
