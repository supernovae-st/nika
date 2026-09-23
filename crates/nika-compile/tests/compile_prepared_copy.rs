// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Public deterministic Compile must need no model for a literal prepared copy.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileRequest, CompileStatus, compile};
use serde_json::Value;

fn copy(intent: &str, source: &str, destination: &str) {
    let out = compile(&CompileRequest::create(intent)).unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{intent}: {out:#?}");
    assert!(out.questions.is_empty(), "{intent}: {out:#?}");
    let plan = out.provenance.plan.as_ref().unwrap();
    let operations = plan["operations"].as_array().unwrap();
    assert_eq!(operations.len(), 1, "{intent}: {plan:#?}");
    assert_eq!(operations[0]["op"], "read");
    assert_eq!(operations[0]["detail"], source);
    assert_eq!(plan["effects"][0]["target"], destination);
    let candidate = out.candidate.as_deref().unwrap();
    let doc: Value = serde_yaml_bw::from_str(candidate).unwrap();
    assert_eq!(doc["const"]["source_path"], source, "{candidate}");
    assert_eq!(doc["const"]["output_path"], destination, "{candidate}");
    assert!(!candidate.contains("infer:"), "{candidate}");
    let tasks = doc["tasks"].as_object().unwrap();
    assert_eq!(tasks.len(), 2, "{candidate}");
    assert_eq!(tasks["read_source"]["invoke"]["tool"], "nika:read");
    assert_eq!(tasks["write_output"]["invoke"]["tool"], "nika:write");
    assert!(
        tasks["write_output"]["with"]["content"]
            .as_str()
            .unwrap()
            .contains("tasks.read_source.output"),
        "{candidate}"
    );
}

#[test]
fn prepared_french_copy_is_ready_without_a_model() {
    for intent in [
        "Prépare la copie de entree.txt dans sortie.txt.",
        "Préparez une copie de entree.txt vers sortie.txt.",
        "Préparer la copie du fichier entree.txt dans le fichier sortie.txt.",
    ] {
        copy(intent, "entree.txt", "sortie.txt");
    }
}

#[test]
fn prepared_english_copy_is_ready_without_a_model() {
    for intent in [
        "Prepare a copy of ./A.txt to ./B.txt.",
        "Prepare the copy from ./A.txt into ./B.txt.",
        "Prepare a file copy of ./A.txt in ./B.txt.",
        "Prepare a copy of `./A.txt` as is to `./B.txt`.",
    ] {
        copy(intent, "./A.txt", "./B.txt");
    }
}

#[test]
fn language_work_and_missing_paths_do_not_compile_to_a_ready_copy() {
    for intent in [
        "Prepare marketing copy from ./a.txt to ./b.txt",
        "Prepare a summary of ./a.txt to ./b.txt",
        "Prepare a translated copy of ./a.txt to ./b.txt",
        "Prépare une copie réécrite de ./a.txt dans ./b.txt",
        "Prepare a copy of ./a.txt",
        "Prépare la copie dans ./b.txt",
        "Prepare a copy of the document to ./b.txt",
        "Prepare a copy of ./a.txt to ./out/",
        "Prepare a copy of ./a.txt to ./out/{name}.txt",
        "Prepare a copy of ./a.txt to ./b.txt and polish the tone",
        "Prepare a copy of ./a.txt to ./b.txt then frobnicate the result",
        "Copy ./a.txt to ./b.txt with spelling corrected",
    ] {
        let out = compile(&CompileRequest::create(intent)).unwrap();
        assert_ne!(out.status, CompileStatus::Ready, "{intent}: {out:#?}");
    }
}

#[test]
fn prepared_copy_approval_is_a_gate_or_an_explicit_gap() {
    let intent = "Prepare a copy of ./a.txt to ./b.txt only after my approval";
    let out = compile(&CompileRequest::create(intent)).unwrap();
    let plan = out.provenance.plan.as_ref().unwrap();
    assert_eq!(plan["effects"][0]["policy"], "human_first", "{out:#?}");
    assert!(
        !plan["operations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|op| op["op"] == "draft"),
        "{out:#?}"
    );
    if out.status == CompileStatus::Ready {
        assert!(
            out.candidate.as_deref().unwrap().contains("nika:prompt"),
            "{out:#?}"
        );
    }
}
