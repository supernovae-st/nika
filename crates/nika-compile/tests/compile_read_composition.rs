// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a request that reads several files writes. With no step reading them as language
//! material, the written bytes are the sources' own, in the stated order, with nothing
//! added: the program that produces them reads the texts alone, never the paths, so it can
//! invent no heading. A language step still receives the sources labelled by path, a join of
//! structured files still parses them apart, a single file is still copied as read, and a
//! requested heading is never dropped silently.
#![allow(clippy::expect_used, clippy::panic)]

use nika_compile::{CompileOutcome, CompileRequest, CompileStatus, compile};
use serde_json::Value;

fn compiled(intent: &str, answers: &[(&str, &str)]) -> CompileOutcome {
    let mut request = CompileRequest::create(intent);
    for (key, literal) in answers {
        request = request.answer(*key, *literal);
    }
    compile(&request).expect("compile")
}

fn workflow(out: &CompileOutcome) -> Value {
    serde_yaml_bw::from_str(out.candidate.as_deref().expect("candidate")).expect("yaml")
}

/// The task a `${{ tasks.<name>.output }}` reference names, if any.
fn producer<'a>(doc: &'a Value, reference: &Value) -> Option<(&'a str, &'a Value)> {
    let name = reference
        .as_str()?
        .strip_prefix("${{ tasks.")?
        .strip_suffix(".output }}")?;
    doc["tasks"]
        .as_object()?
        .get_key_value(name)
        .map(|(k, v)| (k.as_str(), v))
}

/// The task whose output the write puts on disk.
fn written(doc: &Value) -> (&str, &Value) {
    producer(doc, &doc["tasks"]["write_output"]["with"]["content"]).expect("the write's producer")
}

#[test]
fn several_files_written_together_are_their_bytes_in_order_and_nothing_else() {
    for (intent, sources) in [
        (
            "Read ./first.txt and ./second.txt and write them one after the other to ./out/both.txt.",
            ["./first.txt", "./second.txt"],
        ),
        (
            "Read ./left.md and ./right.md and write them to ./out/joined.md.",
            ["./left.md", "./right.md"],
        ),
    ] {
        let out = compiled(intent, &[]);
        assert_eq!(out.status, CompileStatus::Ready, "{intent}: {out:#?}");
        let doc = workflow(&out);
        assert_eq!(
            doc["const"]["source_paths"],
            serde_json::json!(sources),
            "{intent}: the stated order"
        );
        let (name, task) = written(&doc);
        // The written bytes are a function of the read texts alone: no path reaches the
        // program, so no heading or label can be added to what the request asked for.
        let input = &task["invoke"]["args"]["input"];
        assert!(input.get("texts").is_some(), "{intent}: {name} {task:#}");
        assert!(input.get("paths").is_none(), "{intent}: {name} {task:#}");
        let program = task["invoke"]["args"]["expression"]
            .as_str()
            .expect("a jq program");
        assert!(
            !program.contains("##") && !program.contains("paths"),
            "{intent}: {program}"
        );
    }
}

#[test]
fn a_single_file_is_written_as_read() {
    let out = compiled("Read ./a.txt and write it to ./out/copy.txt.", &[]);
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let doc = workflow(&out);
    let (name, task) = written(&doc);
    assert_eq!(name, "read_source", "{doc:#}");
    assert_eq!(task["invoke"]["tool"], "nika:read");
}

#[test]
fn a_language_step_still_receives_the_sources_labelled_by_path() {
    let intent = "Read ./first.txt and ./second.txt and summarize them in three bullets into ./out/summary.md.";
    let out = compiled(intent, &[("model", r#""mock/echo""#)]);
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let doc = workflow(&out);
    let fold = &doc["tasks"]["documents"];
    assert_eq!(fold["invoke"]["tool"], "nika:jq", "{doc:#}");
    assert!(
        fold["invoke"]["args"]["input"].get("paths").is_some(),
        "the model is told which file each text came from: {doc:#}"
    );
    let rendered = serde_json::to_string(&doc["tasks"]).expect("tasks");
    assert!(
        rendered.contains("tasks.documents.output"),
        "the labelled document feeds the language step: {doc:#}"
    );
}

#[test]
fn a_join_of_structured_files_still_parses_them_apart() {
    let intent = "Read ./a.csv and ./b.csv, merge them on the id column, and write the result to ./out/merged.csv.";
    let out = compiled(intent, &[]);
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let doc = workflow(&out);
    assert!(doc["tasks"].get("documents").is_none(), "{doc:#}");
}

/// A heading per file on a plain copy has no realizing contract (the per-item heading contract
/// distributes a draft): the request stays a question naming the clause, never a copy that
/// drops the heading. This states the refusal, not a realized heading.
#[test]
fn a_requested_heading_stays_a_named_question() {
    let intent = "Read ./a.txt and ./b.txt and write them to ./out/ab.md with one heading per file named after the file.";
    let out = compiled(intent, &[]);
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    assert!(
        out.questions
            .iter()
            .any(|q| q.key == "intent.clarification"),
        "{out:#?}"
    );
    assert!(
        out.diagnostics.iter().any(|d| d
            .message
            .contains("with one heading per file named after the file")),
        "{out:#?}"
    );
}
