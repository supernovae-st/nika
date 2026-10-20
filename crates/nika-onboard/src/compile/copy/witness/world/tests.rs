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

/// An output the request never named, declared or published, is observed in the original
/// project after the run: absent there, it joins the world as an absent destination; holding a
/// file there (one the room never held), it refuses.
#[tokio::test]
async fn an_unnamed_output_is_bound_as_its_absence_and_a_file_there_refuses() {
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
        let witness = captured.witness(CANDIDATE, &report.clone()).await.unwrap();
        let bound = ("unobserved.txt".to_owned(), Seen::Absent);
        assert_eq!(witness.world().last(), Some(&bound), "{declared}");
        std::fs::write(root.path().join("unobserved.txt"), "made by hand").unwrap();
        let appeared = witness.drift(root.path(), None).unwrap();
        assert_eq!(appeared, "`unobserved.txt` appeared", "{declared}");
        let captured = before(root.path()).await;
        let error = captured.witness(CANDIDATE, &report).await.unwrap_err();
        assert!(
            error.contains("`unobserved.txt` already holds a file the rehearsal never saw"),
            "{error}"
        );
        std::fs::remove_file(root.path().join("unobserved.txt")).unwrap();
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
    for target in ["../outside", "/outside", "."] {
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

/// A folder the request reads explicitly is observed as each file under it, as the room copies
/// it; the protected parent of a file it names is not captured, and nothing is written.
#[tokio::test]
async fn protected_parent_is_not_captured_and_an_explicit_folder_read_is_each_file_under_it() {
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
    let captured = WorldBefore::capture(root.path(), CANDIDATE, &directory, &[])
        .await
        .expect("a folder is observed as each file under it");
    assert_eq!(
        captured.world,
        vec![
            (
                "./records/input.json".into(),
                Seen::File(Digest::of(input.as_bytes()))
            ),
            (
                "./records/other.json".into(),
                Seen::File(Digest::of(sentinel.as_bytes()))
            ),
        ],
    );
    assert_eq!(captured.folders, directory);
    assert_eq!(
        std::fs::read_to_string(root.path().join("records/input.json")).unwrap(),
        input
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join("records/other.json")).unwrap(),
        sentinel
    );
}

/// A settled run's completed write advances only the bound destination: the source stays bound,
/// and a foreign write or a source edit after that observation is drift again.
#[tokio::test]
async fn a_completed_write_advances_only_its_destination() {
    let root = world(None);
    let witness = before(root.path())
        .await
        .witness(CANDIDATE, &passed())
        .await
        .unwrap();
    std::fs::write(root.path().join("out.txt"), OUTPUT).unwrap();
    assert!(
        witness
            .drift(root.path(), None)
            .unwrap()
            .contains("appeared")
    );
    let next = witness.advanced(root.path(), &[TARGET.into()]).unwrap();
    assert_eq!(
        next.world()[0],
        witness.world()[0],
        "the source stays bound"
    );
    let rebound = (TARGET.to_owned(), Seen::File(Digest::of(OUTPUT.as_bytes())));
    assert_eq!(next.world()[1], rebound);
    assert_eq!(next.candidate_sha256(), witness.candidate_sha256());
    assert!(next.drift(root.path(), None).is_none());
    std::fs::write(root.path().join("out.txt"), "foreign bytes").unwrap();
    assert!(next.drift(root.path(), None).unwrap().contains("changed"));
    std::fs::write(root.path().join("out.txt"), OUTPUT).unwrap();
    std::fs::write(root.path().join("in.txt"), "edited source").unwrap();
    assert!(next.drift(root.path(), None).unwrap().contains("in.txt"));
}

/// No write, a source, a path outside the project, an absent destination, a witness without
/// destinations, a target the request also reads: nothing advances.
#[tokio::test]
async fn only_a_present_bound_destination_advances() {
    let root = world(None);
    let witness = before(root.path())
        .await
        .witness(CANDIDATE, &passed())
        .await
        .unwrap();
    assert!(witness.advanced(root.path(), &[]).is_err());
    let absent = witness.advanced(root.path(), &[TARGET.into()]).unwrap_err();
    assert!(absent.contains("absent"), "{absent}");
    std::fs::write(root.path().join("out.txt"), OUTPUT).unwrap();
    let source = witness
        .advanced(root.path(), &[TARGET.into(), SOURCE.into()])
        .unwrap_err();
    assert!(source.contains("no destination"), "{source}");
    let outside = witness
        .advanced(root.path(), &[TARGET.into(), "../out.txt".into()])
        .unwrap_err();
    assert!(outside.contains("outside the project"), "{outside}");
    let synthetic = Witness::new(sha256(CANDIDATE), witness.world().to_vec());
    assert!(synthetic.advanced(root.path(), &[TARGET.into()]).is_err());
    // A source it only reads is no destination; a file it reads and writes is its own state.
    let read_only = synthetic.clone().writing(&[TARGET.into()]);
    assert!(read_only.advanced(root.path(), &[SOURCE.into()]).is_err());
}

/// A run that also writes a file the request never named (its own state, `etat.json`) advances
/// the destinations the rehearsal bound and binds nothing of that file: nothing witnessed it, so
/// nothing of it can drift, and the next run of the same bytes is no drift (live gauntlet wave 9
/// r11, 2026-10-20: the author's `etat_echecs.json` beside `statut.log` refused every advance, and
/// every rerun read « `statut.log` appeared »).
#[tokio::test]
async fn a_write_outside_the_witnessed_world_leaves_it_and_advances_the_rest() {
    let root = world(None);
    let witness = before(root.path())
        .await
        .witness(CANDIDATE, &passed())
        .await
        .unwrap();
    std::fs::write(root.path().join("out.txt"), OUTPUT).unwrap();
    std::fs::write(root.path().join("etat.json"), "[]").unwrap();
    let next = witness
        .advanced(root.path(), &[TARGET.into(), "./etat.json".into()])
        .unwrap();
    let rebound = (TARGET.to_owned(), Seen::File(Digest::of(OUTPUT.as_bytes())));
    assert_eq!(next.world(), [witness.world()[0].clone(), rebound]);
    assert!(
        next.drift(root.path(), None).is_none(),
        "its own writes are no drift"
    );
    std::fs::write(root.path().join("etat.json"), "[\"edited\"]").unwrap();
    assert!(
        next.drift(root.path(), None).is_none(),
        "a file no request named is never witnessed"
    );
    let only = witness
        .advanced(root.path(), &["./etat.json".into()])
        .unwrap();
    assert_eq!(
        only, witness,
        "nothing witnessed moved, so the witness stays"
    );
}

/// A file the request reads and writes is its own state: the run's own completed write advances
/// it, so the next run of the same bytes is no drift; an edit of that state by anyone else after
/// the run, or of a source it only reads, is still drift (live gauntlet wave 7 r11, 2026-10-19).
#[tokio::test]
async fn a_completed_write_advances_the_state_the_request_reads_too() {
    let root = world(None);
    let witness = before(root.path())
        .await
        .witness(CANDIDATE, &passed())
        .await
        .unwrap();
    let state = Witness::new(sha256(CANDIDATE), witness.world().to_vec()).writing(&[TARGET.into()]);
    std::fs::write(root.path().join("out.txt"), OUTPUT).unwrap();
    let next = state.advanced(root.path(), &[TARGET.into()]).unwrap();
    assert!(
        next.drift(root.path(), None).is_none(),
        "its own write is no drift"
    );
    std::fs::write(root.path().join("out.txt"), "edited by hand").unwrap();
    assert!(next.drift(root.path(), None).unwrap().contains("out.txt"));
    std::fs::write(root.path().join("out.txt"), OUTPUT).unwrap();
    std::fs::write(root.path().join("in.txt"), "edited source").unwrap();
    assert!(next.drift(root.path(), None).unwrap().contains("in.txt"));
}

/// The essays a teacher keeps in `copies/`, and where the marks go.
const ESSAYS: [(&str, &str); 2] = [
    ("copies/emma.txt", "Emma's essay"),
    ("copies/noah.txt", "Noah"),
];
const MARKS: &str = "./retours/notes.csv";

/// A project holding the essays, and nothing at the marks yet.
fn essays() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("copies")).unwrap();
    for (path, text) in ESSAYS {
        std::fs::write(root.path().join(path), text).unwrap();
    }
    root
}

/// A completed run over the room's copy of each essay, as the room spells them, that wrote the
/// marks.
fn marked() -> RehearsalReport {
    let mut report = passed();
    report.observation.copies = (ESSAYS.iter())
        .map(|(path, text)| {
            let digest = Digest::of(text.as_bytes());
            let held = Held::Whole((*text).into());
            CopyReceipt::new(format!("./{path}"), digest.clone(), Some(digest), held)
        })
        .collect();
    let output = Digest::of(OUTPUT.as_bytes());
    let held = Held::Whole(OUTPUT.into());
    let finals = FinalState::File {
        digest: output,
        held,
    };
    report.observation.finals = vec![FinalReceipt::new("retours/notes.csv", finals)];
    report.observation.ledger = LedgerFacts::clean(vec!["retours/notes.csv".into()]);
    let copied = (ESSAYS.iter()).map(|(_, text)| text.len()).sum::<usize>();
    let copied = u64::try_from(copied).unwrap();
    report.observation.spent = Spent::new(copied, Digest::of(OUTPUT.as_bytes()).bytes);
    report.outcome = Rehearsal::Passed {
        outputs: vec![RehearsedOutput::new("retours/notes.csv", OUTPUT)],
    };
    report
}

/// The witness of a rehearsal over the folder the request reads.
async fn witnessed(root: &Path, folder: &str) -> Witness {
    let captured = WorldBefore::capture(root, CANDIDATE, &[folder.into()], &[MARKS.into()])
        .await
        .expect("a folder the request reads is observed");
    captured.witness(CANDIDATE, &marked()).await.unwrap()
}

/// A folder the request reads binds each file under it, as the room copied them (live gauntlet
/// wave 10, 2026-10-20: « copies/ » was refused as no regular file before any rehearsal, so no
/// proposal over a folder was ever rehearsed). An edit of an essay, an essay gone and an essay
/// that appeared are each drift; the same folder again is none.
#[tokio::test]
async fn a_folder_the_request_reads_binds_each_file_under_it_and_drifts_on_any_change() {
    for folder in ["copies/", "./copies", "./copies/*.txt"] {
        let root = essays();
        let witness = witnessed(root.path(), folder).await;
        let essay = |text: &str| Seen::File(Digest::of(text.as_bytes()));
        assert_eq!(
            witness.world(),
            [
                ("./copies/emma.txt".to_owned(), essay(ESSAYS[0].1)),
                ("./copies/noah.txt".to_owned(), essay(ESSAYS[1].1)),
                (MARKS.to_owned(), Seen::Absent),
            ],
            "{folder}"
        );
        assert!(witness.drift(root.path(), None).is_none(), "{folder}");
        let lea = root.path().join("copies/lea.txt");
        std::fs::write(&lea, "Lea").unwrap();
        let appeared = witness.drift(root.path(), None).unwrap();
        assert_eq!(appeared, "`./copies/lea.txt` appeared", "{folder}");
        std::fs::remove_file(&lea).unwrap();
        std::fs::write(root.path().join("copies/emma.txt"), "rewritten").unwrap();
        let changed = witness.drift(root.path(), None).unwrap();
        assert!(
            changed.starts_with("`./copies/emma.txt` changed"),
            "{changed}"
        );
        std::fs::remove_file(root.path().join("copies/emma.txt")).unwrap();
        let gone = witness.drift(root.path(), None).unwrap();
        assert_eq!(gone, "`./copies/emma.txt` is gone", "{folder}");
    }
}

/// A pattern binds the files it matches only: another file of its folder is none of its world.
#[tokio::test]
async fn a_pattern_the_request_reads_binds_only_the_files_it_matches() {
    let root = essays();
    std::fs::write(root.path().join("copies/consignes.md"), "not an essay").unwrap();
    let witness = witnessed(root.path(), "./copies/*.txt").await;
    assert_eq!(witness.world().len(), 3, "{:?}", witness.world());
    std::fs::write(root.path().join("copies/plan.md"), "neither").unwrap();
    assert!(witness.drift(root.path(), None).is_none());
    std::fs::write(root.path().join("copies/zoe.txt"), "Zoe").unwrap();
    let appeared = witness.drift(root.path(), None).unwrap();
    assert_eq!(appeared, "`./copies/zoe.txt` appeared");
}

/// A file the run writes new in the folder it reads is its own from then on: no drift after the
/// run, its rewrite by the next run advances again, and a file anyone else adds is still drift.
/// An essay the run rewrote is a source the request only reads: it never advances.
#[tokio::test]
async fn a_new_file_the_run_writes_in_the_folder_it_reads_is_its_own() {
    let root = essays();
    let witness = witnessed(root.path(), "copies/").await;
    let summary = root.path().join("copies/bilan.txt");
    std::fs::write(&summary, "2 essays").unwrap();
    let next = witness
        .advanced(root.path(), &["./copies/bilan.txt".into()])
        .unwrap();
    assert!(
        next.drift(root.path(), None).is_none(),
        "its own file is no drift"
    );
    std::fs::write(&summary, "3 essays").unwrap();
    let again = next
        .advanced(root.path(), &["./copies/bilan.txt".into()])
        .unwrap();
    assert!(
        again.drift(root.path(), None).is_none(),
        "its rewrite advances"
    );
    std::fs::write(root.path().join("copies/lea.txt"), "Lea").unwrap();
    let foreign = again.drift(root.path(), None).unwrap();
    assert_eq!(foreign, "`./copies/lea.txt` appeared");
    let rewritten = witness
        .advanced(root.path(), &["./copies/emma.txt".into()])
        .unwrap_err();
    assert!(rewritten.contains("no destination"), "{rewritten}");
}

/// A folder past the files a room is given is refused before any room, never cut.
#[tokio::test]
async fn a_folder_past_the_files_a_room_is_given_refuses_capture() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("copies")).unwrap();
    let past = usize::try_from(crate::compile::room::ROOM_FILES).unwrap() + 1;
    for at in 0..past {
        std::fs::write(root.path().join(format!("copies/{at}.txt")), "e").unwrap();
    }
    let error = WorldBefore::capture(root.path(), CANDIDATE, &["copies/".into()], &[])
        .await
        .unwrap_err();
    assert!(error.contains("a cut world is never rehearsed"), "{error}");
}

/// A destination folder stands for each file it holds, each a destination; one that is absent
/// for nothing. The files a run writes there are bound as the absences they were, and the folder
/// the real run then makes is never drift (live gauntlet wave 10, 2026-10-20: a destination
/// folder would have been one path, refused as no regular file once a run had made it).
#[tokio::test]
async fn a_destination_folder_is_each_file_it_holds_and_its_new_files_bind_as_absences() {
    for held in [Some("ancien"), None] {
        let root = essays();
        if let Some(text) = held {
            std::fs::create_dir(root.path().join("retours")).unwrap();
            std::fs::write(root.path().join("retours/ancien.csv"), text).unwrap();
        }
        let captured = WorldBefore::capture(
            root.path(),
            CANDIDATE,
            &["copies/".into()],
            &["./retours/".into()],
        )
        .await
        .unwrap();
        // The room reads its destination folder back as the folder the run made there.
        let mut report = marked();
        let made = FinalReceipt::new("retours", FinalState::Directory);
        report.observation.finals.push(made);
        let witness = captured.witness(CANDIDATE, &report).await.unwrap();
        let paths: Vec<&str> = witness
            .world()
            .iter()
            .map(|(path, _)| path.as_str())
            .collect();
        let mut expected = vec!["./copies/emma.txt", "./copies/noah.txt"];
        expected.extend(held.map(|_| "./retours/ancien.csv"));
        expected.push("retours/notes.csv");
        assert_eq!(paths, expected, "{held:?}");
        std::fs::create_dir_all(root.path().join("retours")).unwrap();
        std::fs::write(root.path().join("retours/notes.csv"), OUTPUT).unwrap();
        let next = witness
            .advanced(root.path(), &["retours/notes.csv".into()])
            .unwrap();
        assert!(next.drift(root.path(), None).is_none(), "{held:?}");
        if held.is_some() {
            std::fs::write(root.path().join("retours/ancien.csv"), "edited").unwrap();
            let drift = next.drift(root.path(), None).unwrap();
            assert!(
                drift.starts_with("`./retours/ancien.csv` changed"),
                "{drift}"
            );
        }
    }
}
