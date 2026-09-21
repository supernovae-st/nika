// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Ordinary product sentences the deterministic door must read as a human does: a brief
//! combined from per-file drafts, a list of fields extracted from each file into one
//! record per file, a translation admitted without source anchors. Each sentence is HOT
//! with the one `model` question a language step needs; the near-misses stay unsettled.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_onboard::compile::{CompileOutcome, CompileRequest, CompileStatus, Strategy, compile};
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
