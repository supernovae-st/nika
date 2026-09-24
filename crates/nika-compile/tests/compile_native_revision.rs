// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Conversational revisions preserve the full base. One literal path substitution is
//! proven against the complete document, with unchanged secondary paths allowed in the
//! change. Additions, invented destinations and changes to other obligations do not
//! inherit that proof. The model need not announce a gap for an exact substitution.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use nika_compile::{
    AuthoringPolicy, CompileOutcome, CompileRequest, CompileStatus, DiagnosticKind, NativeMode,
    compile,
};
use nika_compile_cognition::compile_with_provider;
use serde_json::{Value, json};
use std::sync::atomic::Ordering;
use std::time::Duration;

mod common;
use common::{Rotating, keys};

const ORIGINAL: &str = "Copie entree.txt dans a.txt.";
const CHANGE: &str = "Finalement, utilise b.txt.";
/// The accepted base of the campaign's DIALOG-06, verbatim.
const BASE: &str = r#"nika: copie-entree-vers-a
inputs: {}
const: {}
secrets: {}
permits:
  tools: ["nika:read", "nika:write"]
  fs:
    read: ["entree.txt"]
    write: ["a.txt"]
run: {}
tasks:
  read_source:
    invoke:
      tool: "nika:read"
      args:
        path: "entree.txt"
  write_dest:
    with:
      content: "${{ tasks.read_source.output }}"
    invoke:
      tool: "nika:write"
      args:
        path: "a.txt"
        content: "${{ with.content }}"
        overwrite: true
        create_dirs: false
outputs: {}
"#;

fn policy() -> AuthoringPolicy {
    AuthoringPolicy::new("mock/authoring", 4096, Duration::from_secs(2))
        .with_native(NativeMode::Only)
        .with_repairs(2)
}

fn answer(candidate: &str, gaps: &[&str]) -> String {
    json!({"candidate": candidate, "questions": [], "gaps": gaps, "notes": "revised"}).to_string()
}

fn revise(change: &str) -> CompileRequest {
    CompileRequest::edit(BASE, change)
        .with_original_intent(ORIGINAL)
        .with_authoring_policy(policy())
}

fn rounds(out: &CompileOutcome) -> Vec<Value> {
    out.provenance.decision.as_ref().unwrap()["native"]["rounds"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

fn pending_gap(out: &CompileOutcome) -> bool {
    keys(out).contains(&"gap.1")
        && out
            .diagnostics
            .iter()
            .any(|d| d.kind == DiagnosticKind::Missed && d.target == "gap")
}

/// The base without its write: the old path dropped, nothing written in its place.
fn without_the_write() -> String {
    let cut = BASE.find("  write_dest:").unwrap();
    BASE[..cut]
        .replace(
            "tools: [\"nika:read\", \"nika:write\"]",
            "tools: [\"nika:read\"]",
        )
        .replace("    write: [\"a.txt\"]\n", "")
        + "outputs:\n  text: ${{ tasks.read_source.output }}\n"
}

#[tokio::test]
async fn the_campaign_revision_replaces_the_destination_and_supersedes_the_old_path() {
    let revised = BASE.replace("a.txt", "b.txt");
    // A faithful first answer is enough: no artificial gap-writing repair.
    let provider = Rotating::new(vec![answer(&revised, &[])]);
    let out = compile_with_provider(&revise(CHANGE), &provider)
        .await
        .unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1, "{out:#?}");
    let candidate = out.candidate.as_deref().unwrap();
    assert!(
        candidate.contains("b.txt") && !candidate.contains("a.txt"),
        "{candidate}"
    );
    let rounds = rounds(&out);
    assert_eq!(rounds[0]["diagnostics"], json!([]), "{rounds:#?}");
    let record = out.provenance.plan.clone().unwrap();
    assert_eq!(record["gaps"], json!([]), "{record:#}");
    assert_eq!(record["superseded"][0]["path"], "a.txt");
    assert_eq!(record["superseded"][0]["by"], "b.txt");
    assert!(!keys(&out).contains(&"gap.1"), "{out:#?}");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.kind == DiagnosticKind::Applied
                && d.target == "revision"
                && d.message.contains("`a.txt` is replaced by `b.txt`")),
        "{out:#?}"
    );
    // The answer round replays the record with zero calls: the same READY candidate.
    let replayed = compile(
        &CompileRequest::edit(BASE, CHANGE)
            .with_original_intent(ORIGINAL)
            .with_plan(record),
    )
    .unwrap();
    assert_eq!(replayed.status, CompileStatus::Ready, "{replayed:#?}");
    assert_eq!(replayed.candidate, out.candidate);
}

#[tokio::test]
async fn an_english_replacement_of_a_rooted_path_is_proven_the_same_way() {
    let base = "nika: copy-source\npermits:\n  tools: [\"nika:read\", \"nika:write\"]\n  fs:\n    read: [\"./in/source.txt\"]\n    write: [\"./out/first.txt\"]\ntasks:\n  read_source:\n    invoke:\n      tool: \"nika:read\"\n      args:\n        path: \"./in/source.txt\"\n  write_copy:\n    with:\n      content: \"${{ tasks.read_source.output }}\"\n    invoke:\n      tool: \"nika:write\"\n      args:\n        path: \"./out/first.txt\"\n        content: \"${{ with.content }}\"\n        overwrite: true\n        create_dirs: true\n";
    let revised = base.replace("./out/first.txt", "./out/second.txt");
    let gap = "`./out/first.txt` is replaced by `./out/second.txt`.";
    let provider = Rotating::new(vec![answer(&revised, &[gap])]);
    let request = CompileRequest::edit(base, "Use ./out/second.txt instead.")
        .with_original_intent("Copy ./in/source.txt to ./out/first.txt")
        .with_authoring_policy(policy());
    let out = compile_with_provider(&request, &provider).await.unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let record = out.provenance.plan.clone().unwrap();
    assert_eq!(
        record["superseded"][0]["path"], "./out/first.txt",
        "{record:#}"
    );
    assert_eq!(
        record["superseded"][0]["by"], "./out/second.txt",
        "{record:#}"
    );
}

#[tokio::test]
async fn short_of_proof_the_old_path_is_a_gap_the_human_disposes_of_before_ready() {
    let revised = BASE.replace("a.txt", "b.txt");
    for (change, candidate, why) in [
        // An addition the seat misread as a replacement.
        (
            "Écris aussi dans b.txt.",
            revised.clone(),
            "an addition is never a replacement",
        ),
        // The right path, and a change nobody asked for beside it.
        (
            CHANGE,
            revised.replace("overwrite: true", "overwrite: false"),
            "the base changed beyond the path",
        ),
        // A change unrelated to any path: the omission plus the gap prove nothing.
        (
            "Finalement, utilise un ton formel.",
            without_the_write(),
            "the change names no path in its place",
        ),
    ] {
        let gap = "`a.txt` is not written any more.";
        let provider = Rotating::new(vec![answer(&candidate, &[gap])]);
        let out = compile_with_provider(&revise(change), &provider)
            .await
            .unwrap();
        assert_ne!(out.status, CompileStatus::Ready, "{why}: {out:#?}");
        assert!(pending_gap(&out), "{why}: {out:#?}");
        let record = out.provenance.plan.clone().unwrap();
        assert_eq!(record["gaps"], json!([gap]), "{why}: {record:#}");
        assert!(record.get("superseded").is_none(), "{why}: {record:#}");
        // The human disposes of it; only then is the revision READY.
        let disposed = compile(
            &CompileRequest::edit(BASE, change)
                .with_original_intent(ORIGINAL)
                .with_plan(record)
                .answer("gap.1", r#""drop""#),
        )
        .unwrap();
        assert_eq!(
            disposed.status,
            CompileStatus::Ready,
            "{why}: {disposed:#?}"
        );
    }
}

#[tokio::test]
async fn unproven_silent_omissions_and_creation_gaps_do_not_waive_paths() {
    let revised = BASE.replace("a.txt", "b.txt");
    // A silent omission plus another change is not a proven substitution.
    let silent = Rotating::new(vec![answer(
        &revised.replace("overwrite: true", "overwrite: false"),
        &[],
    )]);
    let out = compile_with_provider(&revise(CHANGE), &silent)
        .await
        .unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    // A creation opens every stated path; a gap waives none.
    let created = Rotating::new(vec![answer(
        &without_the_write(),
        &["`a.txt` cannot be written."],
    )]);
    let out = compile_with_provider(
        &CompileRequest::create(ORIGINAL).with_authoring_policy(policy()),
        &created,
    )
    .await
    .unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(
        rounds(&out)
            .iter()
            .all(|r| r["diagnostics"].to_string().contains("UNREALIZED PATH")),
        "{out:#?}"
    );
}

#[tokio::test]
async fn an_explicit_old_and_new_path_can_prove_the_same_substitution() {
    let revised = BASE.replace("a.txt", "b.txt");
    let provider = Rotating::new(vec![answer(&revised, &[])]);
    let out = compile_with_provider(
        &revise("Finalement, n'écris plus dans a.txt, utilise b.txt."),
        &provider,
    )
    .await
    .unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
}

const FILTER_TOTAL: &str = include_str!("fixtures/compile/revision-filter-total.nika");
const FILTER_INTENT: &str = "Lis commandes.csv, écris uniquement les commandes confirmées dans commandes-confirmees.csv et leur montant total sous forme de nombre dans total.txt.";
const FILTER_CHANGE: &str = "Écris finalement les commandes dans commandes-finales.csv ; conserve le filtre et le total dans total.txt.";

#[tokio::test]
async fn a_destination_replacement_preserves_the_recalled_total_and_full_computation() {
    let revised = FILTER_TOTAL.replace("commandes-confirmees.csv", "commandes-finales.csv");
    let provider = Rotating::new(vec![answer(&revised, &[])]);
    let request = CompileRequest::edit(FILTER_TOTAL, FILTER_CHANGE)
        .with_original_intent(FILTER_INTENT)
        .with_authoring_policy(policy());
    let out = compile_with_provider(&request, &provider).await.unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    let emitted =
        nika_compile::surface::literal_projection(out.candidate.as_deref().unwrap()).unwrap();
    let expected = nika_compile::surface::literal_projection(&revised).unwrap();
    assert_eq!(
        emitted, expected,
        "every task, calculation and other destination survives"
    );
    assert_eq!(
        out.provenance.plan.as_ref().unwrap()["superseded"][0]["by"],
        "commandes-finales.csv"
    );
}

#[tokio::test]
async fn a_replacement_cannot_use_its_path_proof_to_change_a_calculation_or_add_a_destination() {
    let revised = FILTER_TOTAL.replace("commandes-confirmees.csv", "commandes-finales.csv");
    let changed_calculation = revised.replace("| add // 0", "| length");
    assert_ne!(changed_calculation, revised);
    for candidate in [
        changed_calculation,
        revised.replace("commandes-finales.csv", "invented.csv"),
    ] {
        let provider = Rotating::new(vec![answer(
            &candidate,
            &["commandes-confirmees.csv is superseded."],
        )]);
        let request = CompileRequest::edit(FILTER_TOTAL, FILTER_CHANGE)
            .with_original_intent(FILTER_INTENT)
            .with_authoring_policy(policy());
        let out = compile_with_provider(&request, &provider).await.unwrap();
        assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
        assert!(
            out.provenance
                .plan
                .as_ref()
                .is_none_or(|p| p.get("superseded").is_none())
        );
    }
}
