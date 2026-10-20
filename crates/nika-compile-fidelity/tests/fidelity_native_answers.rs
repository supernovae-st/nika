// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A Session's answer to a question it asked reaches Law 1 within its scope (`laws_answered`): the
//! open name the question quotes, the line that answers it, the role the question asked, and the
//! option picked, or, for a free entry, the person's own words for that question. Out of that
//! scope it settles nothing, and its value never reads another name. A typed value outside any
//! question keeps the historical value reading.

use std::collections::{BTreeMap, BTreeSet};

use nika_compile_fidelity::fidelity::{Diagnostic, laws_answered};
use nika_compile_reader::lexicon;
use serde_json::{Value, json};

/// A change that takes an answer out of its scope, named for the failure message.
type OutOfScope = (&'static str, fn(&mut Value));

/// Two outputs in two folders: where the JSON output's name starts stays open.
const REQUEST: &str = "Prépare un workflow qui lit articles.csv et ne garde que les articles en \
                       rupture. Il faudra générer out/stock.json avec le nombre et la liste des \
                       références, puis un court résumé dans notes/stock.md.";
/// The open name, as the request writes it.
const NAME: &str = "faudra générer out/stock.json";
/// The question about where it starts, quoting it, and its key.
const ASKED: &str = "Quel fichier désigne « faudra générer out/stock.json » ?";
const KEY: &str = "const.stock_path";
/// The line that answers it.
const LINE: &str = "u2";
/// The request with a second name sharing the JSON output's path alone.
const TWO: &str = "Prépare un workflow qui lit articles.csv. Il faudra générer out/stock.json avec le \
                   total, puis il faudra relire out/stock.json pour un court résumé dans \
                   notes/stock.md.";
/// The second name, which the first name's answer never settles.
const RELIRE: &str = "faudra relire out/stock.json";

/// A candidate reading `reads` and writing `writes`, each permitted exactly.
fn candidate(reads: &[&str], writes: &[&str]) -> Value {
    let mut tasks = serde_json::Map::new();
    for (k, path) in reads.iter().enumerate() {
        let read = json!({"invoke": {"tool": "nika:read", "args": {"path": path}}});
        tasks.insert(format!("read_{k}"), read);
    }
    for (k, path) in writes.iter().enumerate() {
        let args = json!({"path": path, "content": "{}"});
        tasks.insert(
            format!("write_{k}"),
            json!({"invoke": {"tool": "nika:write", "args": args}}),
        );
    }
    json!({"nika": "stock", "tasks": tasks, "permits": {"fs": {"read": reads,
        "write": writes}, "tools": ["nika:read", "nika:write"]}})
}

/// The articles read, `writes` written.
fn writing(writes: &[&str]) -> Value {
    candidate(&["articles.csv"], writes)
}

/// The Session's description of a pick (`Conversation::answers`): the bound value, the binding
/// the pick made, the question, the answering line and the option picked with its value.
fn pick(option: &str, value: &str) -> Value {
    let provenance = json!({"kind": "offered", "message": LINE, "question": KEY, "option": option});
    json!({
        "bound_values": value,
        "bindings": [{"key": KEY, "role": "value", "value": value, "provenance": provenance}],
        "choice_context": {"question_key": KEY, "question": ASKED, "human_citation": LINE,
            "selected_options": [{"key": option, "label": value,
                "values": [{"role": "value", "value": value}], "provenance": provenance}],
            "human_reply": "le chemin seul"},
    })
}

/// The Session's description of a free entry (`Conversation::answers` after `bind_answer`): the
/// person's own words for the question `question`, no option picked.
fn typed(question: &str, value: &str) -> Value {
    let provenance = json!({"kind": "answered", "message": LINE, "excerpt": value});
    json!({
        "bound_values": value,
        "bindings": [{"key": KEY, "role": "value", "value": value, "provenance": provenance}],
        "choice_context": {"question_key": KEY, "question": question, "human_citation": LINE,
            "selected_options": [], "human_reply": value, "human_lines": [1, 2]},
    })
}

/// The findings of `kind` the laws state for `intent` over `doc` with `answers`: the first
/// backticked word of each, in order.
fn found(kind: &str, intent: &str, doc: &Value, answers: &[(&str, String)]) -> Vec<String> {
    let answers: BTreeMap<String, String> = (answers.iter())
        .map(|(key, raw)| ((*key).to_owned(), raw.clone()))
        .collect();
    let plan = lexicon::read(intent).plan;
    let mut out: Vec<Diagnostic> = Vec::new();
    laws_answered(intent, &plan, doc, (&answers, &[]), None, &mut out);
    (out.iter())
        .filter(|d| d.kind == kind)
        .filter_map(|d| d.message.split('`').nth(1).map(str::to_owned))
        .collect()
}

/// The paths Law 1 finds unrealized in `doc` for `intent` with `answers`, in its order.
fn unrealized(intent: &str, doc: &Value, answers: &[(&str, String)]) -> Vec<String> {
    found("path", intent, doc, answers)
}

/// Unanswered, the open name is owed whole. The person's pick of the path alone, quoting the
/// name, settles it: the document writing `out/stock.json` owes nothing, and the Markdown output
/// stays owed on its own (a document dropping it owes exactly `notes/stock.md`).
#[test]
fn a_native_pick_settles_the_name_its_question_quotes() {
    let both = writing(&["out/stock.json", "notes/stock.md"]);
    assert_eq!(unrealized(REQUEST, &both, &[]), [NAME]);
    let answered = [(KEY, pick("reading-3", "out/stock.json").to_string())];
    assert_eq!(unrealized(REQUEST, &both, &answered), [""; 0]);
    let json_only = writing(&["out/stock.json"]);
    assert_eq!(
        unrealized(REQUEST, &json_only, &answered),
        ["notes/stock.md"]
    );
}

/// A pick out of its scope settles nothing: another question's binding, a question quoting no
/// open name of this request, a binding resting on another line, a request that no longer states
/// the name, a pick no binding stands behind, a binding of an option the person did not pick or
/// of a value that option does not carry, and a binding of another role.
#[test]
fn a_pick_out_of_its_scope_settles_nothing() {
    let both = writing(&["out/stock.json", "notes/stock.md"]);
    let out_of_scope: [OutOfScope; 8] = [
        ("foreign question", |p| {
            p["bindings"][0]["key"] = json!("const.other");
        }),
        ("foreign occurrence", |p| {
            p["choice_context"]["question"] = json!("Où écrire le résultat ?");
        }),
        ("stale line", |p| {
            p["bindings"][0]["provenance"]["message"] = json!("u1");
        }),
        ("withdrawn", |p| {
            p["bindings"] = json!([]);
        }),
        ("different option", |p| {
            p["bindings"][0]["provenance"]["option"] = json!("reading-1");
        }),
        ("value the option does not carry", |p| {
            p["choice_context"]["selected_options"][0]["values"][0]["value"] = json!(NAME);
        }),
        ("other role", |p| {
            p["bindings"][0]["role"] = json!("run_model");
        }),
        ("other key", |p| {
            p["choice_context"]["question_key"] = json!("const.other");
        }),
    ];
    for (why, mutate) in out_of_scope {
        let mut answer = pick("reading-3", "out/stock.json");
        mutate(&mut answer);
        let answered = [(KEY, answer.to_string())];
        assert_eq!(unrealized(REQUEST, &both, &answered), [NAME], "{why}");
    }
    let changed = REQUEST.replace("générer out/stock.json", "produire out/stock.json");
    let answered = [(KEY, pick("reading-3", "out/stock.json").to_string())];
    let stale = unrealized(&changed, &both, &answered);
    assert_eq!(stale, ["faudra produire out/stock.json"], "stale request");
}

/// The person's own words for the question, with no option picked, settle the name it quotes as a
/// pick does.
#[test]
fn a_free_entry_settles_the_name_its_question_quotes() {
    let both = writing(&["out/stock.json", "notes/stock.md"]);
    let answered = [(KEY, typed(ASKED, "out/stock.json").to_string())];
    assert_eq!(unrealized(REQUEST, &both, &answered), [""; 0]);
}

/// The same typed value, answered to the question about one open name, never settles another
/// name that shares the reading. A typed value outside any question, as the direct compile API
/// takes it, still reads both: the historical reading, kept.
#[test]
fn a_free_entry_for_one_name_never_settles_another() {
    let doc = candidate(
        &["articles.csv", "out/stock.json"],
        &["out/stock.json", "notes/stock.md"],
    );
    let answered = [(KEY, typed(ASKED, "out/stock.json").to_string())];
    assert_eq!(unrealized(TWO, &doc, &answered), [RELIRE]);
    let historical = [(KEY, json!("out/stock.json").to_string())];
    assert_eq!(unrealized(TWO, &doc, &historical), [""; 0]);
}

/// A Session's verifier words hold every line of the person's, their reply to the question among
/// them, located by its citation (`human_lines`). That located reply settles no other name there
/// by its folder. Every other line keeps its words: the same text written again as an ordinary
/// line after the reply, or a line of the original request, settles by its folder as the person's
/// own words always did. A location that is not that reply, or the request's first line, leaves
/// every word.
#[test]
fn a_located_reply_settles_no_other_name_through_the_request_words() {
    let doc = candidate(
        &["articles.csv", "out/stock.json"],
        &["out/stock.json", "notes/stock.md"],
    );
    let reply = |lines: [usize; 2]| {
        let mut answer = typed(ASKED, "out/stock.json");
        answer["choice_context"]["human_lines"] = json!(lines);
        [(KEY, answer.to_string())]
    };
    let replied = format!("{TWO}\nout/stock.json");
    assert_eq!(unrealized(&replied, &doc, &reply([1, 2])), [RELIRE]);
    assert_eq!(unrealized(&replied, &doc, &[]), [""; 0]);
    let restated = format!("{replied}\nout/stock.json");
    assert_eq!(unrealized(&restated, &doc, &reply([1, 2])), [""; 0]);
    assert_eq!(unrealized(&restated, &doc, &reply([2, 3])), [""; 0]);
    assert_eq!(unrealized(&replied, &doc, &reply([0, 1])), [""; 0]);
    let mut elsewhere = typed(ASKED, "out/stock.json");
    elsewhere["choice_context"]["human_reply"] = json!("notes/stock.md");
    let elsewhere = [(KEY, elsewhere.to_string())];
    assert_eq!(unrealized(&replied, &doc, &elsewhere), [""; 0]);
}

/// A free entry out of its scope settles nothing: an option no pick carries, another question in
/// its provenance, a pick's kind, a stale line, a role the question did not ask, a public source,
/// and a line another question recorded (bound under this key, it keeps that question).
#[test]
fn a_free_entry_out_of_its_scope_settles_nothing() {
    let both = writing(&["out/stock.json", "notes/stock.md"]);
    let out_of_scope: [OutOfScope; 7] = [
        ("an option no pick carries", |a| {
            a["bindings"][0]["provenance"]["option"] = json!("reading-3");
        }),
        ("another question", |a| {
            a["bindings"][0]["provenance"]["question"] = json!("const.other");
        }),
        ("a pick's kind", |a| {
            a["bindings"][0]["provenance"]["kind"] = json!("offered");
        }),
        ("a stale line", |a| {
            a["bindings"][0]["provenance"]["message"] = json!("u1");
        }),
        ("a role the question did not ask", |a| {
            a["choice_context"]["role"] = json!("output_path");
        }),
        ("a public source", |a| {
            a["bindings"][0]["role"] = json!("read_source");
        }),
        ("a line another question recorded", |a| {
            a["choice_context"]["question_key"] = json!("const.other");
        }),
    ];
    for (why, mutate) in out_of_scope {
        let mut answer = typed(ASKED, "out/stock.json");
        mutate(&mut answer);
        let answered = [(KEY, answer.to_string())];
        assert_eq!(unrealized(REQUEST, &both, &answered), [NAME], "{why}");
    }
}

/// An explicit fact outside any question, a typed JSON literal with no question context (as the
/// direct compile API takes it, or a Session states a value no question asked), keeps the
/// historical value reading: it settles the name it is a reading of.
#[test]
fn an_explicit_fact_outside_any_question_keeps_its_value_reading() {
    let both = writing(&["out/stock.json", "notes/stock.md"]);
    let typed = [(KEY, json!("out/stock.json").to_string())];
    assert_eq!(unrealized(REQUEST, &both, &typed), [""; 0]);
}

/// A question's answer is registered once: the literal it bound is the person's (Law 2 never
/// calls it invented), and a path only its question's words carry is not theirs.
#[test]
fn an_answers_literal_is_the_persons_and_its_questions_words_are_not() {
    let question = format!("{ASKED} Par exemple ./autre/stock.json.");
    let doc = writing(&["./exports/stock.json", "./autre/stock.json"]);
    let answered = [(KEY, typed(&question, "./exports/stock.json").to_string())];
    let invented: BTreeSet<String> = found("literal", REQUEST, &doc, &answered)
        .into_iter()
        .collect();
    assert_eq!(invented, BTreeSet::from(["./autre/stock.json".to_owned()]));
}
