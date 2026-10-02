// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! What a rehearsal reports about outputs is evidence. An output the candidate writes
//! unconditionally but the run never published is Missing, even when a copied file of that name
//! exists. A conditional write whose condition was false is a legitimate absence, reported as not
//! written, never Missing. Only a successful publish by the run is a write. A failed run names
//! its failing task and that task's code. Every read is bounded: a cut preview says so, carries
//! the whole content's size and digest, and ends on a character boundary; an input past the copy
//! bound is never copied cut, the candidate is not run. Every report names the exact candidate
//! bytes it rehearsed.
#![allow(clippy::unwrap_used, clippy::expect_used)]
mod room_support;
use nika_compile::surface::sha256;
use nika_onboard::compile::rehearse::Rehearsal;
use room_support::{
    SALES, World, bound_to, completed, computed_target, conditional_copy, copy_file, gated_source,
    never_attempted, not_run_because, published, rehearsed, sales_input,
};

const PREVIEW_BOUND: usize = 64 * 1024;

#[test]
fn the_fixtures_are_well_formed_before_any_verdict() {
    // The harness guard: a failure here makes the tests using that fixture harness-invalid.
    room_support::assert_catalog_is_well_formed(&World::new(&[("data/sales.csv", SALES)]));
}

#[tokio::test]
async fn a_run_failure_names_its_task_and_code() {
    const TEST: &str = concat!(module_path!(), "::a_run_failure_names_its_task_and_code");
    let world = World::new(&[("data/sales.csv", SALES)]);
    let report = rehearsed(
        TEST,
        "only",
        &world.room(),
        &computed_target(&world, "../escape.json"),
        &sales_input(&world),
        &[],
    )
    .await;
    assert!(
        matches!(&report.outcome, Rehearsal::Failed { task, code, .. }
            if task == "write_output" && code.starts_with("NIKA-")),
        "{report:?}"
    );
    assert!(
        completed(&report),
        "a failed run began and ended: {report:?}"
    );
    assert_eq!(
        report
            .observation
            .failure
            .as_ref()
            .map(|failure| failure.task.as_str()),
        Some("write_output"),
        "{report:?}"
    );
}

#[tokio::test]
async fn a_required_output_the_run_skipped_is_missing() {
    const TEST: &str = concat!(
        module_path!(),
        "::a_required_output_the_run_skipped_is_missing"
    );
    let world = World::new(&[("data/sales.csv", SALES)]);
    let report = rehearsed(
        TEST,
        "only",
        &world.room(),
        &gated_source(&world),
        &sales_input(&world),
        &[],
    )
    .await;
    assert_eq!(
        report.outcome,
        Rehearsal::Missing {
            outputs: vec![world.path("out/copy.csv")]
        },
        "{report:?}"
    );
}

#[tokio::test]
async fn a_required_output_skipped_while_a_copy_of_it_exists_is_still_missing() {
    const TEST: &str = concat!(
        module_path!(),
        "::a_required_output_skipped_while_a_copy_of_it_exists_is_still_missing"
    );
    let world = World::new(&[("data/sales.csv", SALES), ("out/copy.csv", "stale\n")]);
    let inputs = vec![world.path("data/sales.csv"), world.path("out/copy.csv")];
    let report = rehearsed(
        TEST,
        "only",
        &world.room(),
        &gated_source(&world),
        &inputs,
        &[],
    )
    .await;
    assert_eq!(
        report.outcome,
        Rehearsal::Missing {
            outputs: vec![world.path("out/copy.csv")]
        },
        "a copied file is never the run's write: {report:?}"
    );
    assert!(published(&report).is_empty(), "{report:?}");
}

#[tokio::test]
async fn a_real_overwrite_of_a_copied_path_is_written() {
    const TEST: &str = concat!(
        module_path!(),
        "::a_real_overwrite_of_a_copied_path_is_written"
    );
    let world = World::new(&[("data/sales.csv", SALES), ("out/copy.txt", "stale\n")]);
    let before = world.files();
    let inputs = vec![world.path("data/sales.csv"), world.path("out/copy.txt")];
    let source = copy_file(&world, "data/sales.csv", "out/copy.txt");
    let report = rehearsed(TEST, "only", &world.room(), &source, &inputs, &[]).await;
    assert!(
        matches!(&report.outcome, Rehearsal::Passed { outputs }
            if outputs[0].written && outputs[0].text == SALES),
        "{report:?}"
    );
    assert_eq!(world.files(), before, "the stale original is untouched");
}

#[tokio::test]
async fn a_conditional_write_whose_condition_is_false_is_a_legitimate_absence() {
    const TEST: &str = concat!(
        module_path!(),
        "::a_conditional_write_whose_condition_is_false_is_a_legitimate_absence"
    );
    let world = World::new(&[("data/sales.csv", SALES)]);
    let report = rehearsed(
        TEST,
        "only",
        &world.room(),
        &conditional_copy(&world),
        &sales_input(&world),
        &[],
    )
    .await;
    let copy = world.path("out/copy.csv");
    assert!(
        matches!(&report.outcome, Rehearsal::Passed { outputs }
            if outputs.iter().any(|o| o.path == copy && !o.written)),
        "not Missing: the condition allowed the absence: {report:?}"
    );
}

#[tokio::test]
async fn a_file_copied_into_the_room_is_never_evidence_of_a_write() {
    const TEST: &str = concat!(
        module_path!(),
        "::a_file_copied_into_the_room_is_never_evidence_of_a_write"
    );
    let world = World::new(&[("data/sales.csv", SALES), ("out/copy.csv", "stale\n")]);
    let inputs = vec![world.path("data/sales.csv"), world.path("out/copy.csv")];
    let report = rehearsed(
        TEST,
        "only",
        &world.room(),
        &conditional_copy(&world),
        &inputs,
        &[],
    )
    .await;
    let copy = world.path("out/copy.csv");
    assert!(
        matches!(&report.outcome, Rehearsal::Passed { outputs }
            if outputs.iter().any(|o| o.path == copy && !o.written)),
        "the stale copy exists in the room, but the run never wrote it: {report:?}"
    );
}

#[tokio::test]
async fn a_cut_preview_carries_the_whole_size_and_digest_and_is_never_whole() {
    const TEST: &str = concat!(
        module_path!(),
        "::a_cut_preview_carries_the_whole_size_and_digest_and_is_never_whole"
    );
    let big = "0123456789\n".repeat(9_000);
    let world = World::new(&[("data/big.txt", big.as_str())]);
    let source = copy_file(&world, "data/big.txt", "out/big.txt");
    let report = rehearsed(
        TEST,
        "only",
        &world.room(),
        &source,
        &[world.path("data/big.txt")],
        &[],
    )
    .await;
    let Rehearsal::Passed { outputs } = &report.outcome else {
        assert!(
            matches!(report.outcome, Rehearsal::Passed { .. }),
            "{report:?}"
        );
        return;
    };
    let out = &outputs[0];
    assert!(out.truncated && out.text.len() <= PREVIEW_BOUND, "{out:?}");
    assert_eq!(out.full_bytes, big.len() as u64);
    assert_eq!(out.full_sha256, sha256(&big));
    assert!(big.starts_with(&out.text) && out.text.len() < big.len());
}

#[tokio::test]
async fn a_preview_cut_inside_a_multibyte_character_ends_on_a_boundary() {
    const TEST: &str = concat!(
        module_path!(),
        "::a_preview_cut_inside_a_multibyte_character_ends_on_a_boundary"
    );
    // Two-byte characters with an odd lead byte offset, past the preview bound.
    let big = format!("x{}", "é".repeat(40_000));
    let world = World::new(&[("data/wide.txt", big.as_str())]);
    let source = copy_file(&world, "data/wide.txt", "out/wide.txt");
    let report = rehearsed(
        TEST,
        "only",
        &world.room(),
        &source,
        &[world.path("data/wide.txt")],
        &[],
    )
    .await;
    assert!(
        matches!(&report.outcome, Rehearsal::Passed { outputs }
            if outputs[0].truncated
                && outputs[0].text.len() <= PREVIEW_BOUND
                && big.starts_with(&outputs[0].text)),
        "a character-boundary prefix of the content: {report:?}"
    );
}

#[tokio::test]
async fn an_input_past_the_copy_bound_is_never_run() {
    const TEST: &str = concat!(
        module_path!(),
        "::an_input_past_the_copy_bound_is_never_run"
    );
    let big = "client,amount\n".to_owned() + &"a,1\n".repeat(300_000);
    let world = World::new(&[("data/sales.csv", big.as_str())]);
    let before = world.files();
    let source = copy_file(&world, "data/sales.csv", "out/copy.csv");
    let report = rehearsed(
        TEST,
        "only",
        &world.room(),
        &source,
        &sales_input(&world),
        &[],
    )
    .await;
    assert!(not_run_because(&report, "copy bound"), "{report:?}");
    assert!(never_attempted(&report), "{report:?}");
    assert!(report.room.prepared && report.room.cleaned, "{report:?}");
    assert_eq!(world.files(), before);
}

#[tokio::test]
async fn every_report_names_the_exact_candidate_it_rehearsed() {
    const TEST: &str = concat!(
        module_path!(),
        "::every_report_names_the_exact_candidate_it_rehearsed"
    );
    let world = World::new(&[("data/sales.csv", SALES)]);
    let first = copy_file(&world, "data/sales.csv", "out/copy.csv");
    let second = first.replace("nika: copy", "nika: copy-again");
    let a = rehearsed(
        TEST,
        "first",
        &world.room(),
        &first,
        &sales_input(&world),
        &[],
    )
    .await;
    let b = rehearsed(
        TEST,
        "second",
        &world.room(),
        &second,
        &sales_input(&world),
        &[],
    )
    .await;
    assert!(bound_to(&a, &first) && bound_to(&b, &second), "{a:?} {b:?}");
    assert_ne!(a.candidate_sha256, b.candidate_sha256);
    assert_eq!(a.candidate_sha256, sha256(&first));
    assert_ne!(a.admitted_digest, b.admitted_digest, "{a:?} {b:?}");
}
