// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! A file the request asks for without naming it is a write whose exact path is asked, never a
//! READY draft that writes nothing. Witness (S98 J02, 53f8c640): « Résume mes notes dans un
//! fichier. » asked only the model and compiled READY after it — one draft, no effect, the
//! transformation ledgered as realized — through the real TUI and through the zero-provider CLI
//! alike. A named destination stays the path the request states, a file the request already has
//! stays a locative, quoted words stay content, and no answer or replayed record erases the ask.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::surface::literal_projection;
use nika_compile::{
    CompileOutcome, CompileRequest, CompileStatus, DiagnosticKind, HotPolicy, Strategy, compile,
};
use nika_compile_cognition::compile_with_provider;
use serde_json::{Value, json};
use std::sync::atomic::Ordering;

mod common;
use common::{Provider, keys, policy};

const J02: &str = "Résume mes notes dans un fichier.";
const MODEL: (&str, &str) = ("model", r#""deepseek/deepseek-chat""#);
const PATH: (&str, &str) = ("const.output_path", r#""./out/resume.md""#);

/// One round: the request, the plan its previous round recorded (an answer round replays it),
/// and every answer given so far.
fn round(intent: &str, plan: Option<&Value>, answers: &[(&str, &str)]) -> CompileOutcome {
    let mut request = CompileRequest::create(intent);
    if let Some(plan) = plan {
        request = request.with_plan(plan.clone());
    }
    for (key, literal) in answers {
        request = request.answer(*key, *literal);
    }
    compile(&request).unwrap()
}

fn recorded(out: &CompileOutcome) -> Value {
    out.provenance
        .plan
        .clone()
        .expect("every general-path round records its plan")
}

/// The writes the recorded plan states, as (target, evidence).
fn planned_writes(out: &CompileOutcome) -> Vec<(String, String)> {
    recorded(out)["effects"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|effect| effect["verb"] == "write")
        .map(|effect| {
            (
                effect["target"].as_str().unwrap_or_default().to_owned(),
                effect["evidence"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect()
}

fn mandatory(out: &CompileOutcome, key: &str) -> bool {
    out.questions.iter().any(|q| q.key == key && q.mandatory)
}

fn document(out: &CompileOutcome) -> Value {
    literal_projection(out.candidate.as_deref().expect("a candidate")).expect("a document")
}

/// The ids of the tasks that invoke `nika:write`.
fn write_tasks(doc: &Value) -> Vec<String> {
    doc["tasks"]
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(_, task)| task["invoke"]["tool"] == "nika:write")
        .map(|(id, _)| id.clone())
        .collect()
}

/// The ledger duty of one kind and evidence, as the decision record states it.
fn duty(out: &CompileOutcome, kind: &str, evidence: &str) -> Value {
    out.provenance.decision.as_ref().unwrap()["ledger"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|duty| duty["kind"] == kind && duty["evidence"] == evidence)
        .cloned()
        .unwrap_or(Value::Null)
}

#[test]
fn the_j02_request_asks_where_the_summary_goes_and_grants_nothing_before_the_answer() {
    let first = round(J02, None, &[]);
    assert_eq!(first.status, CompileStatus::Incomplete, "{first:#?}");
    assert_eq!(keys(&first), ["model", "const.output_path"], "{first:#?}");
    assert!(mandatory(&first, "const.output_path"), "{first:#?}");
    assert!(first.candidate.is_none(), "{first:#?}");
    assert!(first.requested_boundary.is_none(), "{first:#?}");
    assert_eq!(
        planned_writes(&first),
        [("un fichier".to_owned(), "dans un fichier".to_owned())]
    );
    assert_eq!(
        duty(&first, "effect", "dans un fichier")["state"],
        "unresolved",
        "{first:#?}"
    );
    // Root's zero-provider reproduction: the model answer alone, the recorded plan replayed.
    let plan = recorded(&first);
    let model = round(J02, Some(&plan), &[MODEL]);
    assert_eq!(model.status, CompileStatus::Incomplete, "{model:#?}");
    assert_eq!(keys(&model), ["const.output_path"], "{model:#?}");
    assert!(model.candidate.is_none(), "{model:#?}");
    // The answered path: one write of the drafted text to exactly that file, nothing else.
    let done = round(J02, Some(&plan), &[MODEL, PATH]);
    assert_eq!(done.status, CompileStatus::Ready, "{done:#?}");
    let doc = document(&done);
    assert_eq!(write_tasks(&doc), ["write_output"], "{doc:#}");
    assert_eq!(doc["const"]["output_path"], "./out/resume.md", "{doc:#}");
    assert_eq!(
        doc["tasks"]["write_output"]["invoke"]["args"]["path"], "${{ const.output_path }}",
        "{doc:#}"
    );
    assert_eq!(
        doc["permits"]["fs"],
        json!({"write": ["./out/resume.md"]}),
        "{doc:#}"
    );
    assert!(doc["permits"].get("net").is_none(), "{doc:#}");
    // « mes notes » stays the item each invocation supplies: no file is guessed for it.
    assert!(doc["inputs"].get("item").is_some(), "{doc:#}");
    assert_eq!(
        duty(&done, "effect", "dans un fichier")["realized_by"],
        "write_output",
        "{done:#?}"
    );
}

#[test]
fn a_file_the_request_leaves_unnamed_is_asked_in_french_and_english() {
    for (intent, target) in [
        ("Résume ./notes.md dans un fichier.", "un fichier"),
        ("Écris un haïku dans un fichier.", "un fichier"),
        (
            "Extrais les dates de ./agenda.md dans un fichier.",
            "un fichier",
        ),
        ("Summarize my notes into a file.", "a file"),
        ("Summarize my notes in a file.", "a file"),
        ("Write a haiku to a file.", "a file"),
        ("Summarize ./notes.md into a new file.", "a new file"),
    ] {
        let first = round(intent, None, &[]);
        assert_eq!(
            first.status,
            CompileStatus::Incomplete,
            "{intent}: {first:#?}"
        );
        assert!(
            mandatory(&first, "const.output_path"),
            "{intent}: {first:#?}"
        );
        assert!(first.candidate.is_none(), "{intent}: {first:#?}");
        let writes = planned_writes(&first);
        assert_eq!(writes.len(), 1, "{intent}: {writes:?}");
        assert_eq!(writes[0].0, target, "{intent}: {writes:?}");
        let plan = recorded(&first);
        let model = round(intent, Some(&plan), &[MODEL]);
        assert_eq!(keys(&model), ["const.output_path"], "{intent}: {model:#?}");
        let done = round(intent, Some(&plan), &[MODEL, PATH]);
        assert_eq!(done.status, CompileStatus::Ready, "{intent}: {done:#?}");
        let doc = document(&done);
        assert_eq!(write_tasks(&doc), ["write_output"], "{intent}: {doc:#}");
        assert_eq!(
            doc["permits"]["fs"]["write"],
            json!(["./out/resume.md"]),
            "{intent}: {doc:#}"
        );
    }
}

#[test]
fn a_named_destination_is_written_as_stated_and_no_path_is_asked() {
    for (intent, path) in [
        ("Résume ./notes.md dans ./out/resume.md.", "./out/resume.md"),
        ("Résume mes notes dans un fichier resume.md.", "resume.md"),
        (
            "Summarize ./notes.md into ./out/summary.md.",
            "./out/summary.md",
        ),
    ] {
        let first = round(intent, None, &[]);
        assert_eq!(keys(&first), ["model"], "{intent}: {first:#?}");
        let writes: Vec<String> = planned_writes(&first)
            .into_iter()
            .map(|(target, _)| target)
            .collect();
        assert_eq!(writes, [path], "{intent}");
        let done = round(intent, Some(&recorded(&first)), &[MODEL]);
        assert_eq!(done.status, CompileStatus::Ready, "{intent}: {done:#?}");
        let doc = document(&done);
        assert_eq!(doc["const"]["output_path"], path, "{intent}: {doc:#}");
        assert_eq!(
            doc["permits"]["fs"]["write"],
            json!([path]),
            "{intent}: {doc:#}"
        );
        assert_eq!(write_tasks(&doc), ["write_output"], "{intent}: {doc:#}");
    }
    // The exact zero-call copy stays READY in one round, with no question at all.
    let copy = round("Copie notes.md dans copie.md.", None, &[]);
    assert_eq!(copy.status, CompileStatus::Ready, "{copy:#?}");
    assert!(copy.questions.is_empty(), "{copy:#?}");
    assert_eq!(
        document(&copy)["permits"]["fs"]["write"],
        json!(["copie.md"])
    );
}

#[test]
fn a_file_the_request_already_has_and_quoted_words_are_never_an_output() {
    for intent in [
        // A locative over a definite, possessive or partitive file.
        "Résume les notes dans le fichier.",
        "Résume les notes dans mon fichier.",
        "Résume les notes du fichier.",
        "Summarize the notes in my file.",
        "Summarize the notes in the file.",
        // Words about a file inside quotes are the material, not an instruction.
        "Traduis « dans un fichier » en anglais.",
        "Translate \"into a file\" into French.",
        "Résume « mes notes dans un fichier ».",
        "Summarize \"my notes in a file\".",
    ] {
        let first = round(intent, None, &[]);
        assert_eq!(keys(&first), ["model"], "{intent}: {first:#?}");
        assert!(planned_writes(&first).is_empty(), "{intent}: {first:#?}");
        let done = round(intent, Some(&recorded(&first)), &[MODEL]);
        assert_eq!(done.status, CompileStatus::Ready, "{intent}: {done:#?}");
        let doc = document(&done);
        assert!(write_tasks(&doc).is_empty(), "{intent}: {doc:#}");
        // Nothing is read or written: no file is granted, and none is guessed for the source.
        assert!(doc["permits"].get("fs").is_none(), "{intent}: {doc:#}");
    }
}

#[test]
fn no_answer_and_no_replayed_record_erases_the_asked_write() {
    let first = round(J02, None, &[]);
    let plan = recorded(&first);
    // An answer that is no file keeps the question: a directory, a glob, or prose.
    for literal in [
        r#""./out/""#,
        r#""./out/*.md""#,
        r#""le fichier du résumé""#,
    ] {
        let out = round(J02, Some(&plan), &[MODEL, ("const.output_path", literal)]);
        assert_eq!(out.status, CompileStatus::Incomplete, "{literal}: {out:#?}");
        assert_eq!(keys(&out), ["const.output_path"], "{literal}: {out:#?}");
        assert!(out.candidate.is_none(), "{literal}: {out:#?}");
        assert!(
            out.diagnostics
                .iter()
                .any(|d| d.kind == DiagnosticKind::Missed && d.target == "const.output_path"),
            "{literal}: {out:#?}"
        );
    }
    // The record the 53f8c640 engine wrote for this very request (root's reproduction: one
    // draft, no effect) replays with the write restored, never READY on the model alone.
    let earlier = json!({
        "bindings": [], "constraints": [], "effects": [], "obligations": [],
        "operations": [{"categories": [], "detail": "mes notes dans un fichier",
                        "evidence": "Résume mes notes dans un fichier", "op": "draft"}],
        "rules": [], "slots": [], "strategy": "hot", "trigger": null, "unknowns": []
    });
    let out = round(J02, Some(&earlier), &[MODEL]);
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert_eq!(keys(&out), ["const.output_path"], "{out:#?}");
    assert_eq!(
        planned_writes(&out),
        [("un fichier".to_owned(), "dans un fichier".to_owned())]
    );
    let out = round(J02, Some(&earlier), &[MODEL, PATH]);
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(write_tasks(&document(&out)), ["write_output"]);
}

#[tokio::test]
async fn a_cold_plan_that_drops_the_write_keeps_the_readers_write_and_its_question() {
    const INTENT: &str = "Résume ./notes.md dans un fichier.";
    // The seat reads and drafts, and proposes no write at all.
    let proposal = json!({
        "steps": [
            {"op": "read", "detail": "./notes.md", "evidence": "./notes.md"},
            {"op": "draft", "detail": "un résumé de ./notes.md", "evidence": "Résume ./notes.md dans un fichier"}
        ],
        "effects": [], "obligations": [], "constraints": [], "unknowns": []
    });
    let provider = Provider::new(&proposal);
    let request = CompileRequest::create(INTENT)
        .with_authoring_policy(policy())
        .with_hot_policy(HotPolicy::Off);
    let first = compile_with_provider(&request, &provider).await.unwrap();
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        first.provenance.strategy,
        Some(Strategy::Cold),
        "{first:#?}"
    );
    assert!(
        mandatory(&first, "const.output_path"),
        "the reader's floor rides the merge: {first:#?}"
    );
    assert!(first.candidate.is_none(), "{first:#?}");
    assert_eq!(
        planned_writes(&first),
        [("un fichier".to_owned(), "dans un fichier".to_owned())]
    );
    // The answer round replays the seat's plan with zero calls and writes the answered file.
    let done = round(INTENT, Some(&recorded(&first)), &[MODEL, PATH]);
    assert_eq!(done.status, CompileStatus::Ready, "{done:#?}");
    let doc = document(&done);
    assert_eq!(
        doc["permits"]["fs"],
        json!({"read": ["./notes.md"], "write": ["./out/resume.md"]}),
        "{doc:#}"
    );
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
}

/// A requested file is realized, questioned or refused, never dropped into READY.
/// Typographic spellings preserve the same destination.
#[test]
fn a_typographic_spelling_of_the_unnamed_file_is_still_asked() {
    for intent in [
        "Résume mes notes dans\u{a0}un fichier.",
        "Résume mes notes dans un fichier\u{2026}",
        "Summarize my notes into a .md file.",
    ] {
        let first = round(intent, None, &[]);
        assert!(
            mandatory(&first, "const.output_path"),
            "{intent}: {first:#?}"
        );
        assert_eq!(planned_writes(&first).len(), 1, "{intent}: {first:#?}");
        let plan = recorded(&first);
        let model = round(intent, Some(&plan), &[MODEL]);
        assert_eq!(keys(&model), ["const.output_path"], "{intent}: {model:#?}");
        let done = round(intent, Some(&plan), &[MODEL, PATH]);
        assert_eq!(done.status, CompileStatus::Ready, "{intent}: {done:#?}");
        assert_eq!(write_tasks(&document(&done)), ["write_output"], "{intent}");
    }
}

#[test]
fn an_unnamed_file_beside_a_named_one_or_stated_twice_is_never_dropped() {
    for intent in [
        "Résume ./notes.md dans ./out/resume.md et extrais les dates dans un fichier.",
        "Résume mes notes dans un fichier. Extrais les dates dans un fichier.",
        "Summarize my notes into a file. Extract the dates into a file.",
    ] {
        let first = round(intent, None, &[]);
        assert_eq!(planned_writes(&first).len(), 2, "{intent}: {first:#?}");
        let plan = recorded(&first);
        // One answered path is not two files: READY needs every file's path.
        for answers in [&[MODEL][..], &[MODEL, PATH][..]] {
            let out = round(intent, Some(&plan), answers);
            assert_ne!(out.status, CompileStatus::Ready, "{intent}: {out:#?}");
        }
    }
}

#[test]
fn a_stated_approval_holds_the_unnamed_write() {
    for intent in [
        "Summarize my notes into a file. Never send anything. Only after my approval.",
        "Résume mes notes dans un fichier après mon approbation.",
        "Summarize my notes into a file, but ask me before writing it.",
    ] {
        let first = round(intent, None, &[]);
        assert_eq!(
            keys(&first),
            ["model", "const.output_path"],
            "{intent}: {first:#?}"
        );
        let done = round(intent, Some(&recorded(&first)), &[MODEL, PATH]);
        assert_eq!(done.status, CompileStatus::Ready, "{intent}: {done:#?}");
        let doc = document(&done);
        assert_eq!(write_tasks(&doc), ["write_output"], "{intent}: {doc:#}");
        assert_eq!(
            doc["tasks"]["write_output"]["when"], "${{ with.approved == true }}",
            "{intent}: {doc:#}"
        );
    }
}
