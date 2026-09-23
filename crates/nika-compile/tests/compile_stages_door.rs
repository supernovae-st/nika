// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The stages of a computation at the deterministic door: a rename, a grouped total behind
//! a computation head, a filter followed by a sort in one sentence, a projection, a Spanish
//! filter with a plural write head. Each compiles READY with zero calls and the jq the
//! grammar states; what the grammar cannot read (a filter stated in a noun, a tone) still
//! asks or stays a constraint. Ported from season 2's wave 32 onto main's reader.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{CompileRequest, CompileStatus, Strategy, compile};
use serde_json::Value;

fn ready(intent: &str) -> Value {
    let out = compile(&CompileRequest::create(intent)).unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(out.provenance.strategy, Some(Strategy::Hot), "{out:#?}");
    assert!(
        out.check_preview.as_ref().unwrap().report.is_clean(),
        "{out:#?}"
    );
    serde_yaml_bw::from_str(out.candidate.as_deref().unwrap()).unwrap()
}

fn expression(doc: &Value) -> String {
    let jq = doc["tasks"]["compute"]["invoke"]["args"]["expression"].as_str();
    assert!(jq.is_some(), "no jq on `compute`: {doc:#}");
    jq.unwrap_or_default().to_owned()
}

fn writes(doc: &Value) -> Vec<String> {
    doc["permits"]["fs"]["write"]
        .as_array()
        .map(|paths| {
            paths
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn a_rename_is_a_computation_and_a_bare_path_after_write_is_its_destination() {
    for intent in [
        "Read ./sales.csv, rename the country column to region and write ./sales-region.csv",
        "Lis ./sales.csv, renomme la colonne country en region et écris ./sales-region.csv",
    ] {
        let doc = ready(intent);
        let jq = expression(&doc);
        assert!(
            jq.contains("with_entries(if .key == \"country\" then .key = \"region\" else . end)"),
            "{intent}: {jq}"
        );
        assert_eq!(writes(&doc), ["./sales-region.csv"], "{intent}: {doc:#}");
    }
}

#[test]
fn a_grouped_total_behind_a_computation_head_is_read_by_the_grammar() {
    let doc = ready(
        "Read ./sales.csv, compute the total of the amount column per client and write it to ./totals.json",
    );
    let jq = expression(&doc);
    assert!(jq.contains("group_by(.client)"), "{jq}");
    assert!(jq.contains("\"total\""), "{jq}");
    assert_eq!(writes(&doc), ["./totals.json"]);
}

#[test]
fn a_filter_then_a_sort_in_one_sentence_are_one_computation() {
    let doc = ready(
        "Read ./tickets.json, keep only the rows whose status is open, sort them by priority and write them to ./open-sorted.json",
    );
    let jq = expression(&doc);
    assert!(jq.contains("select(.status == \"open\")"), "{jq}");
    assert!(jq.contains("sort_by(.priority"), "{jq}");
    assert_eq!(writes(&doc), ["./open-sorted.json"]);
}

#[test]
fn a_projection_of_named_fields_is_read_whole() {
    let doc = ready(
        "Read ./people.json, keep only the name and email of each person and write them to ./contacts.json",
    );
    let jq = expression(&doc);
    assert!(jq.contains("\"name\": .name"), "{jq}");
    assert!(jq.contains("\"email\": .email"), "{jq}");
    assert_eq!(writes(&doc), ["./contacts.json"]);
}

#[test]
fn a_spanish_filter_with_a_plural_write_head_compiles_at_the_door() {
    let doc = ready(
        "Lee ./ventas.csv, conserva solo las filas cuyo importe supera 200 y escríbelas en ./grandes.csv",
    );
    let jq = expression(&doc);
    assert!(jq.contains("(.importe | tonumber) > 200"), "{jq}");
    assert_eq!(writes(&doc), ["./grandes.csv"]);
}

#[test]
fn what_the_grammar_cannot_read_still_asks_and_a_tone_stays_a_constraint() {
    // A filter stated in a noun (« the open tickets ») names no column: the door asks.
    let out = compile(&CompileRequest::create(
        "Read ./tickets.json, count the open tickets and write the count to ./count.json",
    ))
    .unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    // « keep the tone formal » is a constraint on the draft, never a computation.
    let out = compile(&CompileRequest::create(
        "Read ./notes.md, keep the tone formal and write a summary to ./out/summary.md",
    ))
    .unwrap();
    let keys: Vec<&str> = out.questions.iter().map(|q| q.key.as_str()).collect();
    assert_eq!(keys, ["model"], "{out:#?}");
    let plan = out.provenance.plan.as_ref().unwrap();
    let ops: Vec<&str> = plan["operations"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|s| s["op"].as_str())
        .collect();
    assert!(!ops.contains(&"compute"), "{plan:#}");
    assert!(
        plan["constraints"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c.as_str() == Some("keep the tone formal")),
        "{plan:#}"
    );
}
