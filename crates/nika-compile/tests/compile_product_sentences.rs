// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Ordinary product sentences the deterministic door must read as a human does: a brief
//! combined from per-file drafts, a list of fields extracted from each file into one
//! record per file, a translation admitted without source anchors. Each sentence is HOT
//! with the one `model` question a language step needs; the near-misses stay unsettled.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileOutcome, CompileRequest, CompileStatus, Strategy, compile};
use serde_json::Value;

fn hot(intent: &str) -> CompileOutcome {
    let out = compile(&CompileRequest::create(intent)).unwrap();
    assert_eq!(out.provenance.strategy, Some(Strategy::Hot), "{out:#?}");
    out
}

fn keys(out: &CompileOutcome) -> Vec<&str> {
    out.questions.iter().map(|q| q.key.as_str()).collect()
}

fn ready_with_model(intent: &str) -> Value {
    let out = compile(&CompileRequest::create(intent).answer("model", r#""mock/echo""#)).unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(out.provenance.strategy, Some(Strategy::Hot), "{out:#?}");
    assert!(
        out.check_preview.as_ref().unwrap().report.is_clean(),
        "{out:#?}"
    );
    serde_yaml_bw::from_str(out.candidate.as_deref().unwrap()).unwrap()
}

fn expression(doc: &Value, task: &str) -> String {
    let jq = doc["tasks"][task]["invoke"]["args"]["expression"].as_str();
    assert!(jq.is_some(), "no jq on `{task}`: {doc:#}");
    jq.unwrap_or_default().to_owned()
}

#[test]
fn a_brief_combined_from_a_draft_of_each_file_refers_back_to_the_draft() {
    let intent = "Read every file in ./rfc/*.md, draft the 3 most important changes of each one, and write the combined brief to ./brief.md";
    let out = hot(intent);
    assert_eq!(keys(&out), ["model"], "{out:#?}");
    let doc = ready_with_model(intent);
    // One draft over the folded corpus, its body written; "the combined brief" is that
    // draft, not a second one.
    assert!(
        doc["tasks"]["draft"]["infer"]["prompt"]
            .as_str()
            .unwrap_or_default()
            .starts_with("Draft the following: the 3 most important changes of each one."),
        "{doc:#}"
    );
    assert!(doc["tasks"].get("draft_2").is_none(), "{doc:#}");
    assert!(doc["tasks"].get("documents").is_some(), "{doc:#}");
    assert_eq!(
        doc["tasks"]["write_output"]["with"]["content"],
        "${{ tasks.draft.output.body }}"
    );
    assert_eq!(doc["const"]["source_glob"], "./rfc/*.md");
    assert!(doc["inputs"]["item"].is_null(), "phantom item: {doc:#}");
    // Near-misses: a plain or uncounted draft object is fine too; an unknown verb is not.
    for fine in [
        "Read every file in ./rfc/*.md, draft something, and write the combined brief to ./brief.md",
        "Read every file in ./rfc/*.md, draft the changes, and write the combined brief to ./brief.md",
    ] {
        let out = hot(fine);
        assert_eq!(keys(&out), ["model"], "{fine}: {out:#?}");
    }
    let out = compile(&CompileRequest::create(
        "Read every file in ./rfc/*.md, do something clever, and write the combined brief to ./brief.md",
    ))
    .unwrap();
    assert_ne!(out.provenance.strategy, Some(Strategy::Hot), "{out:#?}");
    assert_eq!(keys(&out), ["intent.clarification"], "{out:#?}");
    // With nothing produced before it, "the combined brief" is new content the write
    // demands: a draft of it, still admitted with its model question.
    let out = hot("Read every file in ./rfc/*.md and write the combined brief to ./brief.md");
    assert_eq!(keys(&out), ["model"], "{out:#?}");
}

#[test]
fn a_list_of_fields_of_each_file_is_one_record_per_file() {
    let intent = "Read ./invoices/*.txt, extract the supplier, the date and the amount of each one, and write them as JSON to ./invoices.json";
    let out = hot(intent);
    assert_eq!(keys(&out), ["model"], "{out:#?}");
    let doc = ready_with_model(intent);
    let tasks = doc["tasks"].as_object().unwrap();
    // The fan-out is zipped into items; the extract runs once per item and never sees a
    // folded corpus; the fold makes one object per item; the write carries the fold.
    assert!(tasks.contains_key("source_items"), "{doc:#}");
    assert!(!tasks.contains_key("documents"), "{doc:#}");
    assert_eq!(
        doc["tasks"]["extract"]["for_each"]["items"],
        "${{ with.items }}"
    );
    assert_eq!(
        doc["tasks"]["extract"]["with"]["items"],
        "${{ tasks.source_items.output }}"
    );
    let prompt = doc["tasks"]["extract"]["infer"]["prompt"].as_str().unwrap();
    assert!(
        prompt.starts_with(
            "Extract the following from the supplied item: the supplier, the date and the amount."
        ),
        "{prompt}"
    );
    assert!(prompt.contains("${{ item.text }}"), "{prompt}");
    assert_eq!(
        expression(&doc, "extract_fold"),
        ". as $r | [$r.extracts[] | .fields | map({key: .name, value: .value}) | from_entries]"
    );
    assert!(
        expression(&doc, "extract_anchors")
            .contains("($r.extracts | length) == ($r.items | length)"),
        "{doc:#}"
    );
    assert_eq!(
        doc["tasks"]["write_output"]["with"]["content"],
        "${{ tasks.extract_fold.output }}"
    );
    assert_eq!(
        doc["tasks"]["write_output"]["after"],
        serde_json::json!({"extract_admit": "success"})
    );
    assert_eq!(doc["outputs"]["fields"], "${{ tasks.extract_fold.output }}");
    assert!(doc["inputs"]["item"].is_null(), "phantom item: {doc:#}");
    // Without the per-item scope, one extract reads the folded corpus, as before.
    let doc = ready_with_model(
        "Read ./invoices/*.txt, extract the supplier, the date and the amount, and write them as JSON to ./invoices.json",
    );
    assert!(doc["tasks"].get("documents").is_some(), "{doc:#}");
    assert!(doc["tasks"].get("extract_fold").is_none(), "{doc:#}");
    assert!(doc["tasks"]["extract"].get("for_each").is_none(), "{doc:#}");
}

#[test]
fn a_translation_is_admitted_without_source_anchors() {
    for intent in [
        "Translate ./notes/brief.md into English and write the translation to ./out/brief-en.md",
        "Traduis ./notes/brief.md en anglais et écris la traduction dans ./out/brief-en.md",
    ] {
        let out = hot(intent);
        assert_eq!(keys(&out), ["model"], "{intent}: {out:#?}");
        let doc = ready_with_model(intent);
        assert_eq!(
            expression(&doc, "draft_anchors"),
            ". as $root | ($root.body | length) > 0",
            "{intent}: {doc:#}"
        );
        let prompt = doc["tasks"]["draft"]["infer"]["prompt"].as_str().unwrap();
        assert!(prompt.starts_with("Translate the following:"), "{prompt}");
        assert!(
            doc["tasks"]["draft_admit"]["invoke"]["args"]["message"]
                .as_str()
                .unwrap()
                .contains("no anchor is required"),
            "{doc:#}"
        );
        assert_eq!(
            doc["tasks"]["write_output"]["with"]["content"],
            "${{ tasks.draft.output.body }}"
        );
    }
    // A summary keeps the anchor law: every claim anchored in the corpus.
    let doc = ready_with_model(
        "Read ./notes/brief.md, summarize it in three bullets, and write the summary to ./out/summary.md",
    );
    assert!(
        expression(&doc, "draft_anchors").contains("facts_used[]"),
        "{doc:#}"
    );
}

#[test]
fn a_seated_plan_with_only_a_draft_over_nothing_is_never_a_candidate() {
    // The plan a seat proposed for "build me a digest of the docs": one draft, no source,
    // no effect, no trigger. Replayed as a recorded plan, it reaches the assembler alone.
    let record = serde_json::json!({
        "operations": [{"op": "draft", "detail": "a digest of the docs", "evidence": "build me a digest of the docs", "categories": []}],
        "effects": [], "obligations": [], "bindings": [], "constraints": [], "unknowns": [],
        "trigger": null, "rules": []
    });
    let out = compile(
        &CompileRequest::create("build me a digest of the docs")
            .with_plan(record.clone())
            .answer("model", r#""mock/echo""#),
    )
    .unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(keys(&out), ["intent.clarification"], "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.message.contains("names no material to work on")),
        "{out:#?}"
    );
    // The same draft over material an invocation supplies, or per incoming request, is
    // fed: a candidate whose item is real.
    let mut supplied = record.clone();
    supplied["operations"][0]["evidence"] = serde_json::json!("summarize the supplied text");
    let out = compile(
        &CompileRequest::create("summarize the supplied text")
            .with_plan(supplied)
            .answer("model", r#""mock/echo""#),
    )
    .unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let mut triggered = record;
    triggered["trigger"] = serde_json::json!("for each request");
    triggered["operations"][0]["evidence"] = serde_json::json!("draft a digest of the docs");
    let out = compile(
        &CompileRequest::create("For each request, draft a digest of the docs")
            .with_plan(triggered)
            .answer("model", r#""mock/echo""#),
    )
    .unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
}

#[test]
fn a_draft_prompt_asks_verbatim_anchors_and_one_bullet_per_line() {
    let doc = ready_with_model(
        "Lis ./notes/brief.md, rédige un résumé en 3 puces et écris ce résumé dans ./out/resume.md",
    );
    let prompt = doc["tasks"]["draft"]["infer"]["prompt"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert!(
        prompt.contains("verbatim copy of one contiguous span"),
        "{prompt}"
    );
    assert!(prompt.contains("never a paraphrase"), "{prompt}");
    assert!(
        prompt.contains("Put each bullet or point on its own line"),
        "{prompt}"
    );
    let doc = ready_with_model(
        "Read ./notes/brief.md, summarize it in one paragraph, and write the summary to ./out/summary.md",
    );
    let prompt = doc["tasks"]["draft"]["infer"]["prompt"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert!(!prompt.contains("on its own line"), "{prompt}");
}

#[test]
fn a_classification_of_each_record_routes_the_records_to_the_files_named_after_its_categories() {
    let intent = "Read ./tickets.json, classify each ticket as bug or feature, and write the bugs to ./bugs.json and the features to ./features.json";
    let out = hot(intent);
    assert_eq!(keys(&out), ["model"], "{out:#?}");
    let doc = ready_with_model(intent);
    let tasks = doc["tasks"].as_object().unwrap();
    // The source is parsed once; the classify runs per record; each write carries the
    // records routed to the category its clause names.
    assert_eq!(expression(&doc, "parse_source"), "fromjson");
    assert_eq!(
        doc["tasks"]["classify"]["for_each"]["items"],
        "${{ with.records }}"
    );
    assert_eq!(
        doc["tasks"]["classify"]["with"]["records"],
        "${{ tasks.parse_source.output }}"
    );
    assert_eq!(
        doc["tasks"]["classify"]["infer"]["schema"]["properties"]["category"]["enum"],
        serde_json::json!(["bug", "feature"])
    );
    assert!(
        doc["tasks"]["classify"]["infer"]["prompt"]
            .as_str()
            .unwrap_or_default()
            .contains("Record: ${{ item }}"),
        "{doc:#}"
    );
    assert_eq!(doc["const"]["output_path"], "./bugs.json");
    assert_eq!(doc["const"]["features_path"], "./features.json");
    assert_eq!(
        doc["tasks"]["route_bugs"]["invoke"]["args"]["input"]["category"],
        "bug"
    );
    assert_eq!(
        doc["tasks"]["route_features"]["invoke"]["args"]["input"]["category"],
        "feature"
    );
    assert!(
        expression(&doc, "route_bugs").contains("$r.categories[$i].category == $r.category"),
        "{doc:#}"
    );
    assert_eq!(
        doc["tasks"]["write_output"]["with"]["content"],
        "${{ tasks.route_bugs.output }}"
    );
    assert_eq!(
        doc["tasks"]["write_features"]["with"]["content"],
        "${{ tasks.route_features.output }}"
    );
    assert!(!tasks.contains_key("draft"), "{doc:#}");
    assert!(doc["inputs"]["item"].is_null(), "phantom item: {doc:#}");
    assert_eq!(
        doc["permits"]["fs"]["write"],
        serde_json::json!(["./bugs.json", "./features.json"])
    );
    // One write of "the results" carries every record with its category, never the bare
    // category word.
    let doc = ready_with_model(
        "Read ./tickets.json, classify each ticket as bug or feature, and write the results to ./out.json",
    );
    assert_eq!(
        doc["tasks"]["write_output"]["with"]["content"],
        "${{ tasks.out_classified.output }}"
    );
    assert!(
        expression(&doc, "out_classified").contains("+ {category: $r.categories[$i].category}"),
        "{doc:#}"
    );
    // Without "each", the classification is one judgement over the document, as before.
    let doc = ready_with_model(
        "Read ./tickets.json, classify the tickets as bug or feature, and write the category to ./category.txt",
    );
    assert!(
        doc["tasks"]["classify"].get("for_each").is_none(),
        "{doc:#}"
    );
    assert_eq!(
        doc["tasks"]["write_output"]["with"]["content"],
        "${{ tasks.classify.output.category }}"
    );
}
