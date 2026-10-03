// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Synthetic filesystem observations and report doubles only. No workflow or host runs.

use super::*;
use nika_compile_cognition::rehearse::{
    Bounds, CopyReceipt, Digest, EffectCounts, FinalReceipt, FinalState, Held, LedgerFacts,
    Observation, RehearsedOutput, RoomEvidence, Spent,
};

const CANDIDATE: &str = "the exact synthetic candidate";
const SOURCE: &str = "./in.txt";
const TARGET: &str = "./out.txt";
const INPUT: &str = "the original input";
const OUTPUT: &str = "the observed result";

fn world(target: Option<&str>) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("in.txt"), INPUT).unwrap();
    if let Some(text) = target {
        std::fs::write(root.path().join("out.txt"), text).unwrap();
    }
    root
}

async fn before(root: &Path) -> WorldBefore {
    WorldBefore::capture(root, CANDIDATE, &[SOURCE.into()], &[TARGET.into()])
        .await
        .unwrap()
}

fn passed() -> RehearsalReport {
    let copied = Digest::of(INPUT.as_bytes());
    let output = Digest::of(OUTPUT.as_bytes());
    let mut observed = Observation::none();
    observed.bounds = Bounds::new(10_000, 1_048_576, 65_536);
    observed.copies = vec![CopyReceipt::new(
        "in.txt",
        copied.clone(),
        Some(copied.clone()),
        Held::Whole(INPUT.into()),
    )];
    observed.finals = vec![FinalReceipt::new(
        "out.txt",
        FinalState::File {
            digest: output.clone(),
            held: Held::Whole(OUTPUT.into()),
        },
    )];
    observed.ledger = LedgerFacts::clean(vec!["out.txt".into()]);
    observed.spent = Spent::new(copied.bytes, output.bytes);
    RehearsalReport::new(
        Rehearsal::Passed {
            outputs: vec![RehearsedOutput::new("out.txt", OUTPUT)],
        },
        Attempt::Completed { elapsed_ms: 3 },
        EffectCounts::none(),
        sha256(CANDIDATE),
    )
    .with_admitted_digest("synthetic-admission")
    .with_room(RoomEvidence::new(true, true))
    .with_observation(observed)
}

#[tokio::test]
async fn an_existing_or_absent_destination_is_bound_without_copying_it_as_input() {
    for target in [None, Some("previous destination")] {
        let root = world(target);
        let captured = before(root.path()).await;
        let report = passed();
        assert_eq!(report.observation.copies.len(), 1);
        let witness = captured.witness(CANDIDATE, &report).await.unwrap();
        assert_eq!(witness.candidate_sha256(), sha256(CANDIDATE));
        assert_eq!(witness.world().len(), 2);
        assert_eq!(
            witness.world()[0],
            (SOURCE.into(), Seen::File(Digest::of(INPUT.as_bytes())))
        );
        let expected = target.map_or(Seen::Absent, |text| Seen::File(Digest::of(text.as_bytes())));
        assert_eq!(witness.world()[1], (TARGET.into(), expected));
        assert!(witness.drift(root.path(), None).is_none());
        assert_eq!(
            std::fs::read_to_string(root.path().join("in.txt")).unwrap(),
            INPUT
        );
        assert_eq!(
            std::fs::read_to_string(root.path().join("out.txt"))
                .ok()
                .as_deref(),
            target
        );
    }
}

#[tokio::test]
async fn candidate_identity_and_admission_are_checked_before_binding() {
    let root = world(None);
    for wrong in ["candidate", "report", "admission"] {
        let captured = before(root.path()).await;
        let mut report = passed();
        let candidate = if wrong == "candidate" {
            "other candidate"
        } else {
            CANDIDATE
        };
        if wrong == "report" {
            report.candidate_sha256 = sha256("other candidate");
        }
        if wrong == "admission" {
            report.admitted_digest.clear();
        }
        assert!(
            captured.witness(candidate, &report).await.is_err(),
            "{wrong}"
        );
    }
}

#[tokio::test]
async fn source_drift_destination_drift_and_new_destination_refuse_the_witness() {
    for (target, changed) in [
        (None, "in.txt"),
        (Some("old"), "out.txt"),
        (None, "out.txt"),
    ] {
        let root = world(target);
        let captured = before(root.path()).await;
        std::fs::write(root.path().join(changed), "changed externally").unwrap();
        let error = captured.witness(CANDIDATE, &passed()).await.unwrap_err();
        assert!(error.contains(changed), "{error}");
    }
}

#[tokio::test]
async fn a_coherent_room_copy_of_another_world_cannot_bind_this_before_state() {
    let root = world(None);
    let captured = before(root.path()).await;
    let mut report = passed();
    let foreign = "different observed input";
    let digest = Digest::of(foreign.as_bytes());
    report.observation.copies[0] = CopyReceipt::new(
        "in.txt",
        digest.clone(),
        Some(digest.clone()),
        Held::Whole(foreign.into()),
    );
    report.observation.spent.copied_bytes = digest.bytes;
    let error = captured.witness(CANDIDATE, &report).await.unwrap_err();
    assert!(
        error.contains("copied input") && error.contains("before-state"),
        "{error}"
    );
}

#[tokio::test]
async fn an_incomplete_or_invalid_report_never_constructs_a_witness() {
    let root = world(None);
    for kind in [
        "never",
        "stopped",
        "missing",
        "dirty",
        "undrained",
        "denied",
        "missing-copy",
        "missing-final",
    ] {
        let captured = before(root.path()).await;
        let mut report = passed();
        match kind {
            "never" => {
                report.attempt = Attempt::NeverAttempted;
                report.outcome = Rehearsal::NotRun {
                    reason: "synthetic refusal".into(),
                };
            }
            "stopped" => {
                report.attempt = Attempt::Stopped { elapsed_ms: 10_000 };
                report.outcome = Rehearsal::NotRun {
                    reason: "synthetic bound".into(),
                };
            }
            "missing" => {
                report.outcome = Rehearsal::Missing {
                    outputs: vec![TARGET.into()],
                }
            }
            "dirty" => report.room.cleaned = false,
            "undrained" => report.observation.ledger.drained = false,
            "denied" => report.effects.network = 1,
            "missing-copy" => report.observation.copies.clear(),
            "missing-final" => report.observation.finals.clear(),
            _ => panic!("unknown invalid report fixture: {kind}"),
        }
        assert!(
            captured.witness(CANDIDATE, &report).await.is_err(),
            "{kind}"
        );
    }
}

#[tokio::test]
async fn declared_or_published_paths_without_a_before_state_refuse() {
    let root = world(None);
    for declared in [false, true] {
        let captured = before(root.path()).await;
        let mut report = passed();
        if declared {
            let Rehearsal::Passed { outputs } = &mut report.outcome else {
                panic!("the starting report fixture must pass")
            };
            outputs.push(RehearsedOutput::new("unobserved.txt", ""));
            report
                .observation
                .finals
                .push(FinalReceipt::new("unobserved.txt", FinalState::Absent));
        } else {
            report
                .observation
                .ledger
                .written
                .push("unobserved.txt".into());
        }
        let error = captured.witness(CANDIDATE, &report).await.unwrap_err();
        assert!(
            error.contains("unobserved.txt") && error.contains("before-state"),
            "{error}"
        );
    }
}

#[tokio::test]
async fn aliases_share_one_before_state_and_do_not_expand_inputs() {
    let root = world(None);
    let captured = WorldBefore::capture(
        root.path(),
        CANDIDATE,
        &[SOURCE.into(), "in.txt".into()],
        &[TARGET.into(), "out.txt".into()],
    )
    .await
    .unwrap();
    let witness = captured.witness(CANDIDATE, &passed()).await.unwrap();
    assert_eq!(witness.world().len(), 2);
}

#[tokio::test]
async fn paths_outside_the_root_and_unobservable_paths_refuse_capture() {
    let root = world(None);
    std::fs::create_dir(root.path().join("directory")).unwrap();
    for target in ["../outside", "/outside", ".", "directory"] {
        assert!(
            WorldBefore::capture(root.path(), CANDIDATE, &[SOURCE.into()], &[target.into()])
                .await
                .is_err(),
            "{target}"
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn a_symlink_before_or_after_capture_never_supplies_a_before_state() {
    let root = world(None);
    std::os::unix::fs::symlink("in.txt", root.path().join("out.txt")).unwrap();
    assert!(
        WorldBefore::capture(root.path(), CANDIDATE, &[SOURCE.into()], &[TARGET.into()])
            .await
            .is_err()
    );
    std::fs::remove_file(root.path().join("out.txt")).unwrap();
    let captured = before(root.path()).await;
    std::os::unix::fs::symlink("in.txt", root.path().join("out.txt")).unwrap();
    let error = captured.witness(CANDIDATE, &passed()).await.unwrap_err();
    assert!(error.contains("symlink"), "{error}");
}

#[tokio::test]
async fn the_checked_witness_still_refuses_a_changed_saved_candidate() {
    let root = world(None);
    let witness = before(root.path())
        .await
        .witness(CANDIDATE, &passed())
        .await
        .unwrap();
    let saved = Path::new("saved.nika");
    std::fs::write(root.path().join(saved), CANDIDATE).unwrap();
    assert!(witness.drift(root.path(), Some(saved)).is_none());
    std::fs::write(root.path().join(saved), "other candidate").unwrap();
    assert!(witness.drift(root.path(), Some(saved)).is_some());
}

#[tokio::test]
async fn protected_parent_is_not_captured_but_an_explicit_directory_read_is_refused() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("records")).unwrap();
    let input = "[{\"id\":1}]\n";
    let sentinel = "[{\"unrelated\":true}]\n";
    std::fs::write(root.path().join("records/input.json"), input).unwrap();
    std::fs::write(root.path().join("records/other.json"), sentinel).unwrap();
    let intent = "Lis ./records/input.json. Ne modifie rien dans ./records.";
    let inputs = nika_compile::stated_sources(intent);
    let targets = nika_compile::stated_destinations(intent);
    let captured = WorldBefore::capture(root.path(), CANDIDATE, &inputs, &targets)
        .await
        .expect("observe the declared file without its protected parent");
    assert_eq!(
        captured.world,
        vec![(
            "./records/input.json".into(),
            Seen::File(Digest::of(input.as_bytes()))
        )],
    );
    let directory = nika_compile::stated_sources("Lis ./records");
    assert_eq!(directory, ["./records"]);
    let error = WorldBefore::capture(root.path(), CANDIDATE, &directory, &[])
        .await
        .expect_err("a directory cannot supply the before-state of a file");
    assert!(error.contains("is not a regular file"), "{error}");
    assert_eq!(
        std::fs::read_to_string(root.path().join("records/input.json")).unwrap(),
        input
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join("records/other.json")).unwrap(),
        sentinel
    );
}
