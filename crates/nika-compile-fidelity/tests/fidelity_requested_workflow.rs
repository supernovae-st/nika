// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The workflow file a request names for the workflow it asks for (`requested_workflow`): read
//! from the request, kept apart from the files the candidate's own tasks call or name, whatever
//! `nika:` the author writes.

use std::collections::BTreeMap;

use nika_compile_fidelity::fidelity::{requested_workflow, requested_workflow_answered};
use serde_json::json;

/// A request that asks to create, save and run `bilan.nika`.
const NAMED: &str = "À partir de commandes.csv, prépare un bilan des ventes. Après mon choix, crée, \
                     sauvegarde et lance bilan.nika, avec le total dans out/bilan.md.";

/// A candidate named `id` that reads the orders and writes the total, a text it writes mentioning
/// the workflow file, with `extra` task lines.
fn candidate(id: &str, extra: &str) -> String {
    let tasks = "tasks:\n  read:\n    invoke: { tool: \"nika:read\", args: { path: \"commandes.csv\" } }\n  total:\n    invoke: { tool: \"nika:write\", args: { path: \"out/bilan.md\", content: \"Total de bilan.nika\" } }\n";
    format!(
        "nika: {id}\npermits:\n  tools: [\"nika:read\", \"nika:write\"]\n  fs:\n    read: [\"commandes.csv\"]\n    write: [\"out/bilan.md\"]\n{tasks}{extra}"
    )
}

/// The request names `bilan.nika`; the author's `bilan-ventes` changes nothing, nor does a text
/// the candidate writes that mentions the file. A folder the request gives is kept.
#[test]
fn the_workflow_file_a_request_names_is_requested() {
    let plain = candidate("bilan-ventes", "");
    assert_eq!(
        requested_workflow(NAMED, &plain).as_deref(),
        Some("bilan.nika")
    );
    let nested = "À partir de commandes.csv, prépare un bilan des ventes, puis crée et sauvegarde le \
                  workflow dans workflows/bilan.nika, avec le total dans out/bilan.md.";
    assert_eq!(
        requested_workflow(nested, &plain).as_deref(),
        Some("workflows/bilan.nika")
    );
}

/// One file the request writes with and without its leading `./` is one destination, requested
/// once; two distinct names stay two, with or without the prefix.
#[test]
fn one_file_named_with_and_without_its_dot_slash_is_requested_once() {
    let plain = candidate("bilan-ventes", "");
    let twice = "À partir de commandes.csv, crée bilan.nika avec le total dans out/bilan.md, puis \
                 sauvegarde ./bilan.nika et lance-le.";
    assert_eq!(
        requested_workflow(twice, &plain).as_deref(),
        Some("bilan.nika")
    );
    for distinct in [
        "Crée bilan.nika puis ./relance.nika.",
        "Crée ./bilan.nika puis ./relance.nika.",
    ] {
        assert_eq!(requested_workflow(distinct, &plain), None, "{distinct}");
    }
}

/// A file the candidate calls as its child, or a task's own file, is not the one it is saved as;
/// a request naming none, or two, or a path outside the project, requests nothing.
#[test]
fn a_child_a_task_file_two_files_or_an_outside_path_request_nothing() {
    let call = "  child:\n    invoke: { workflow: \"./bilan.nika\" }\n";
    assert_eq!(
        requested_workflow(NAMED, &candidate("bilan-ventes", call)),
        None
    );
    let write = "  keep:\n    invoke: { tool: \"nika:write\", args: { path: \"bilan.nika\", content: \"x\" } }\n";
    assert_eq!(
        requested_workflow(NAMED, &candidate("bilan-ventes", write)),
        None
    );
    let plain = candidate("bilan-ventes", "");
    for intent in [
        "Lis commandes.csv et écris out/bilan.md.",
        "Crée bilan.nika puis relance.nika.",
        "Crée ../bilan.nika depuis commandes.csv.",
    ] {
        assert_eq!(requested_workflow(intent, &plain), None, "{intent}");
    }
}

/// A candidate counting the lines of the component at `path` (a literal, or a const it declares
/// in `consts`) into `out/total.md`.
fn reading(consts: &str, path: &str) -> String {
    let read =
        format!("  read:\n    invoke: {{ tool: \"nika:read\", args: {{ path: \"{path}\" }} }}\n");
    format!(
        "nika: count-lines\n{consts}permits:\n  tools: [\"nika:read\", \"nika:write\"]\n  fs:\n    read: [\"./components/base.nika\"]\n    write: [\"out/total.md\"]\ntasks:\n{read}  total:\n    invoke: {{ tool: \"nika:write\", args: {{ path: \"out/total.md\", content: \"ok\" }} }}\n"
    )
}

/// A component the candidate reads through its `const:` is a file its task uses, never the
/// workflow's own, exactly as when it reads that component literally: a request naming the
/// component alone requests nothing, and one naming another file for the workflow requests that
/// file.
#[test]
fn a_component_a_task_reads_through_a_const_is_never_the_workflows_own_file() {
    let request =
        "Compte les lignes de ./components/base.nika et écris le total dans out/total.md.";
    let saved = "Compte les lignes de ./components/base.nika et écris le total dans out/total.md, \
                 puis enregistre le workflow dans comptage.nika.";
    let declared = "const:\n  component_path: ./components/base.nika\n";
    let through_const = reading(declared, "${{ const.component_path }}");
    let literal = reading("", "./components/base.nika");
    for candidate in [&through_const, &literal] {
        assert_eq!(requested_workflow(request, candidate), None, "{candidate}");
        assert_eq!(
            requested_workflow(saved, candidate).as_deref(),
            Some("comptage.nika"),
            "{candidate}"
        );
    }
}

/// A const no task uses excludes nothing, even one holding the file the request names.
#[test]
fn a_const_no_task_uses_excludes_nothing() {
    let unused = candidate("bilan-ventes", "").replacen(
        "permits:",
        "const:\n  copy: bilan.nika\npermits:",
        1,
    );
    assert_eq!(
        requested_workflow(NAMED, &unused).as_deref(),
        Some("bilan.nika")
    );
}

/// A message workflow writing the text `bonjour.nika`, the person's reply to the author's question.
const MESSAGE: &str = "nika: message\npermits:\n  tools: [\"nika:write\"]\n  fs:\n    write: [\"out/message.txt\"]\ntasks:\n  message:\n    invoke: { tool: \"nika:write\", args: { path: \"out/message.txt\", content: \"bonjour.nika\" } }\n";

/// The person's reply `bonjour.nika` to the question about the text, as a Session describes it,
/// located at the request's lines `lines`.
fn replied(lines: [u64; 2]) -> BTreeMap<String, String> {
    let context = json!({"question_key": "message_text", "question": "Quel texte ?",
        "human_citation": "u2", "selected_options": [], "human_reply": "bonjour.nika",
        "human_lines": lines});
    let answer = json!({"bound_values": "bonjour.nika", "bindings": [], "choice_context": context});
    BTreeMap::from([("message_text".to_owned(), answer.to_string())])
}

/// A reply to a Session's question is that question's answer: a workflow file its line holds as
/// business content is not the file the request names. The request's own words naming one still
/// do beside the same reply, and a location that is not the reply as typed leaves every word.
#[test]
fn a_reply_to_a_question_never_names_the_workflow_file() {
    let request = "Écris un court message dans out/message.txt.";
    let words = format!("{request}\nbonjour.nika");
    assert_eq!(
        requested_workflow(&words, MESSAGE).as_deref(),
        Some("bonjour.nika")
    );
    assert_eq!(
        requested_workflow_answered(&words, MESSAGE, &replied([1, 2])),
        None
    );
    let named = format!("{request} Enregistre le workflow dans bonjour.nika.\nbonjour.nika");
    assert_eq!(
        requested_workflow_answered(&named, MESSAGE, &replied([1, 2])).as_deref(),
        Some("bonjour.nika")
    );
    assert_eq!(
        requested_workflow_answered(&words, MESSAGE, &replied([0, 1])).as_deref(),
        Some("bonjour.nika")
    );
}
