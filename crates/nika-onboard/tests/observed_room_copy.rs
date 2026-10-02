// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The first positive: « Copy ./<prefix>/in/source.txt as is to ./<prefix>/out/copied.txt ».
//! Its two programs are made once each by the real compiler, before any run: the text lowering
//! (the candidate of `compile()`) and the byte lowering (the same plan's read as an opaque
//! envelope the write decodes). Both must be Ready with a clean Check and distinct digests, and
//! the request's own contract must require the text copy, or the test is `HARNESS_INVALID`.
//!
//! Each program's exact bytes are then rehearsed in three worlds that share one prefix, each its
//! own original project:
//!
//! | World | Source                                 | Target before |
//! |-------|----------------------------------------|---------------|
//! | A     | `alpha\n`                              | absent        |
//! | B     | `beta\r\ncafé ${{ const.not_code }}\n` | `alpha\n`     |
//! | C     | empty                                  | `stale`       |
//!
//! A world's expected result is its own source, never a reference workflow's output. Every real
//! report crosses the existing adapter, then the real behavioural judgment of the request's
//! contract: each program is certified over its three worlds, and a reader that publishes
//! nothing over a target already holding the right bytes is defective. A decoy under the process
//! working directory holds the same relative paths with other bytes, one world at a time; the
//! world and the decoy end byte for byte as they began.
#![allow(clippy::unwrap_used, clippy::expect_used)]
mod room_support;
use nika_compile::surface::sha256;
use nika_compile_fidelity::behavior::{Contract, Verdict};
use nika_onboard::compile::rehearse::{Rehearsal, RehearsalReport, judged_run, targets_of};
use room_support::{
    ALPHA, BETA, DECOY_SOURCE, DECOY_TARGET, Lowered, World, admitted_digest_of, bound_to,
    completed, copy_contract, final_text, lowered_copy, no_op, published, rehearsed, requires_copy,
    unique_prefix, verdict,
};

#[test]
fn the_fixtures_are_well_formed_before_any_verdict() {
    // The harness guard: a failure here is HARNESS_INVALID for the tests using that fixture.
    room_support::assert_catalog_is_well_formed(&World::new(&[("in/source.txt", ALPHA)]));
}

/// The copy's two programs over `prefix`, the request's contract and the one result path it
/// names, each positive before any run, or the test is `HARNESS_INVALID`.
fn positive(prefix: &str) -> (Lowered, Contract, Vec<String>) {
    let lowered = lowered_copy(prefix).expect("HARNESS_INVALID: the copy's two programs");
    let contract = copy_contract(prefix);
    assert!(
        requires_copy(&contract, prefix),
        "HARNESS_INVALID: the request's contract requires no text copy: {contract:?}"
    );
    let targets: Vec<String> = targets_of(&contract).collect();
    assert_eq!(
        targets,
        [format!("./{prefix}/out/copied.txt")],
        "HARNESS_INVALID: the request's contract names one result"
    );
    assert_ne!(
        sha256(&lowered.text),
        sha256(&lowered.bytes),
        "HARNESS_INVALID: the two lowerings are one program"
    );
    (lowered, contract, targets)
}

#[test]
fn the_two_lowerings_and_the_request_contract_are_positive_before_any_run() {
    // No run: the compiler and the request's contract alone.
    let prefix = unique_prefix();
    let (lowered, _, _) = positive(&prefix);
    let (again, _, _) = positive(&prefix);
    assert_eq!(
        lowered.text, again.text,
        "the text lowering is deterministic"
    );
    assert_eq!(
        lowered.bytes, again.bytes,
        "the byte lowering is deterministic"
    );
}

/// One world of the copy: its label, its source and the target it held before, if any.
struct CopyWorld {
    name: &'static str,
    source: &'static str,
    before: Option<&'static str>,
}

const WORLDS: [CopyWorld; 3] = [
    CopyWorld {
        name: "A",
        source: ALPHA,
        before: None,
    },
    CopyWorld {
        name: "B",
        source: BETA,
        before: Some(ALPHA),
    },
    CopyWorld {
        name: "C",
        source: "",
        before: Some("stale"),
    },
];

/// Rehearse `candidate` as `subrun` of `test` in its own original project over `prefix`, with a
/// decoy owned for that world alone; the world and the decoy must end as they began. The report,
/// and the inputs the host was given.
async fn in_world(
    test: &str,
    subrun: &str,
    prefix: &str,
    world_case: &CopyWorld,
    candidate: &str,
    targets: &[String],
) -> (RehearsalReport, Vec<String>) {
    let world = World::with_prefix(prefix, &[("in/source.txt", world_case.source)]);
    let mut inputs = vec![world.path("in/source.txt")];
    if let Some(before) = world_case.before {
        world.put("out/copied.txt", before);
        inputs.push(world.path("out/copied.txt"));
    }
    let decoy = world.decoy(&[
        ("in/source.txt", DECOY_SOURCE),
        ("out/copied.txt", DECOY_TARGET),
    ]);
    let (files, decoy_files) = (world.files(), decoy.files());
    let report = rehearsed(test, subrun, &world.room(), candidate, &inputs, targets).await;
    assert_eq!(world.files(), files, "{subrun}: the world is as it was");
    assert_eq!(
        decoy.files(),
        decoy_files,
        "{subrun}: the decoy is as it was"
    );
    (report, inputs)
}

#[tokio::test]
async fn the_compiled_copy_publishes_exactly_its_source_in_every_world() {
    const TEST: &str = concat!(
        module_path!(),
        "::the_compiled_copy_publishes_exactly_its_source_in_every_world"
    );
    let prefix = unique_prefix();
    let (lowered, contract, targets) = positive(&prefix);
    let at = format!("{prefix}/out/copied.txt");
    for (lowering, candidate) in [("text", &lowered.text), ("bytes", &lowered.bytes)] {
        let mut runs = Vec::new();
        for world_case in &WORLDS {
            let subrun = format!("{lowering}:{}", world_case.name);
            let (report, inputs) =
                in_world(TEST, &subrun, &prefix, world_case, candidate, &targets).await;
            assert!(
                matches!(&report.outcome, Rehearsal::Passed { outputs }
                    if outputs.len() == 1
                        && outputs[0].path == targets[0]
                        && outputs[0].written
                        && !outputs[0].truncated
                        && outputs[0].text == world_case.source
                        && outputs[0].full_sha256 == sha256(world_case.source)),
                "{subrun}: {report:?}"
            );
            assert_eq!(
                final_text(&report, &targets[0]),
                Some(world_case.source),
                "{subrun}"
            );
            assert_eq!(
                published(&report),
                std::slice::from_ref(&at),
                "{subrun}: the run's own publish"
            );
            let copy = &report.observation.copies[0];
            assert_eq!(copy.room.as_ref(), Some(&copy.source), "{subrun}: {copy:?}");
            assert_eq!(copy.source.sha256, sha256(world_case.source), "{subrun}");
            assert!(
                completed(&report) && report.effects.is_none(),
                "{subrun}: {report:?}"
            );
            assert!(
                report.room.prepared && report.room.cleaned,
                "{subrun}: {report:?}"
            );
            assert!(
                bound_to(&report, candidate),
                "{subrun}: one program in every world: {report:?}"
            );
            assert_eq!(
                report.admitted_digest,
                admitted_digest_of(candidate),
                "{subrun}"
            );
            runs.push(judged_run(&subrun, &report, &inputs, &targets, &targets));
        }
        assert_eq!(
            verdict(&contract, &runs),
            Verdict::Certified,
            "{lowering}: {runs:?}"
        );
    }
}

#[tokio::test]
async fn a_target_that_already_held_the_source_is_no_publication() {
    const TEST: &str = concat!(
        module_path!(),
        "::a_target_that_already_held_the_source_is_no_publication"
    );
    // The target already holds the source's bytes. The copy publishes them again; a candidate
    // that only reads ends with the same bytes in the room and published nothing.
    let prefix = unique_prefix();
    let (lowered, contract, targets) = positive(&prefix);
    let world = World::with_prefix(
        &prefix,
        &[("in/source.txt", ALPHA), ("out/copied.txt", ALPHA)],
    );
    let inputs = vec![world.path("in/source.txt"), world.path("out/copied.txt")];
    let idle_candidate = no_op(&world);
    let copied = rehearsed(
        TEST,
        "copy",
        &world.room(),
        &lowered.text,
        &inputs,
        &targets,
    )
    .await;
    let idle = rehearsed(
        TEST,
        "idle",
        &world.room(),
        &idle_candidate,
        &inputs,
        &targets,
    )
    .await;
    assert_eq!(final_text(&copied, &targets[0]), Some(ALPHA), "{copied:?}");
    assert_eq!(final_text(&idle, &targets[0]), Some(ALPHA), "{idle:?}");
    let at = format!("{prefix}/out/copied.txt");
    assert_eq!(published(&copied), std::slice::from_ref(&at), "{copied:?}");
    assert!(
        published(&idle).is_empty(),
        "a file already there is no write: {idle:?}"
    );
    assert!(
        matches!(&idle.outcome, Rehearsal::Passed { outputs } if outputs.is_empty()),
        "the reader declares no output: {idle:?}"
    );
    let copy_run = judged_run("copy", &copied, &inputs, &targets, &targets);
    let idle_run = judged_run("idle", &idle, &inputs, &targets, &[]);
    assert_eq!(verdict(&contract, &[copy_run]), Verdict::Certified);
    assert_eq!(
        verdict(&contract, &[idle_run]),
        Verdict::Defective,
        "the right bytes the run did not publish never pass"
    );
}

#[tokio::test]
async fn an_empty_copy_is_a_published_empty_file_never_an_absence() {
    const TEST: &str = concat!(
        module_path!(),
        "::an_empty_copy_is_a_published_empty_file_never_an_absence"
    );
    let prefix = unique_prefix();
    let (lowered, contract, targets) = positive(&prefix);
    let world = World::with_prefix(&prefix, &[("in/source.txt", "")]);
    let inputs = vec![world.path("in/source.txt")];
    let report = rehearsed(
        TEST,
        "empty",
        &world.room(),
        &lowered.text,
        &inputs,
        &targets,
    )
    .await;
    assert_eq!(final_text(&report, &targets[0]), Some(""), "{report:?}");
    assert!(
        matches!(&report.outcome, Rehearsal::Passed { outputs }
            if outputs[0].written && outputs[0].full_bytes == 0),
        "{report:?}"
    );
    let run = judged_run("empty", &report, &inputs, &targets, &targets);
    assert_eq!(verdict(&contract, &[run]), Verdict::Certified);
}
