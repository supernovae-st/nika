// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The lineage view (C7c): the frozen C7b oracle LN1–LN17 and the C7c
//! uncertainty cases U1–U12, on minimized real journals of the C7b public
//! runs (S1 · X1 · T1 on the C6 binary: every frame kept, only the fields
//! the view does not read dropped) and on synthetic controls.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use nika_dap::store::{DoubtWhy, SkipWhy, Survey, TraceMeta, TraceState, scan, survey};

use crate::lineage::{Head, Lineage, MAX_LINKS, Undecided, fold, lineage_of};

/// A fresh per-test trace directory under the cargo tmp root.
fn store(name: &str) -> PathBuf {
    let base = std::env::temp_dir().join("nika-trace-lineage");
    let dir = base.join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("store dir");
    dir
}

/// Stage one journal and backdate its mtime by `age`.
fn stage_aged(dir: &Path, name: &str, body: &str, age: Duration) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, body).expect("journal staged");
    let mtime = SystemTime::now().checked_sub(age).expect("age fits");
    let file = std::fs::File::options()
        .write(true)
        .open(&path)
        .expect("reopen");
    file.set_times(std::fs::FileTimes::new().set_modified(mtime))
        .expect("mtime set");
    path
}

/// Stage journals, the first one oldest; returns their paths in order.
fn stage(dir: &Path, journals: &[(&str, &str)]) -> Vec<PathBuf> {
    let count = u64::try_from(journals.len()).expect("few journals");
    journals
        .iter()
        .zip(0_u64..)
        .map(|((name, body), i)| stage_aged(dir, name, body, Duration::from_secs((count - i) * 10)))
        .collect()
}

/// The run identity a journal's opening frame names (the continuation link).
fn run_of(dir: &Path, name: &str) -> String {
    let meta = scan(dir).into_iter().find(|t| t.name == name).expect(name);
    meta.run_id.expect("identified")
}

/// The reasons of an indeterminate verdict (empty for any other verdict).
fn reasons(lineage: &Lineage) -> &[Undecided] {
    match lineage {
        Lineage::Indeterminate(reasons) => reasons,
        _ => &[],
    }
}

/// A verdict's shape, free of names and orders: what LN17 compares.
fn shape(lineage: &Lineage) -> String {
    match lineage {
        Lineage::NoneObserved => "none".to_owned(),
        Lineage::Chain { links, head } => {
            let head = match head {
                Head::Settled(state) => format!("settled {}", state.as_str()),
                Head::Paused { task, .. } => format!("paused {task:?}"),
                Head::Running(liveness) => format!("running {liveness:?}"),
            };
            format!("chain {} {head}", links.len())
        }
        Lineage::Indeterminate(reasons) => {
            let mut words: Vec<String> = reasons
                .iter()
                .map(|reason| match reason {
                    Undecided::Fork { run_id, successors } => {
                        let mut states: Vec<&str> =
                            successors.iter().map(|(_, s)| s.as_str()).collect();
                        states.sort_unstable();
                        format!("fork {run_id} {states:?}")
                    }
                    other => format!("{other:?}"),
                })
                .collect();
            words.sort();
            format!("indeterminate {words:?}")
        }
    }
}

/// One synthetic journal's facts for the pure fold (no file).
fn meta(name: &str, run: Option<&str>, from: Option<&str>, state: TraceState) -> TraceMeta {
    TraceMeta::new(
        PathBuf::from(name),
        name.to_owned(),
        "w".to_owned(),
        state,
        None,
        1,
        SystemTime::UNIX_EPOCH,
    )
    .with_resumed_from(from.map(str::to_owned))
    .with_identity(run.map(str::to_owned), Some("p".repeat(64)))
}

fn survey_of(traces: Vec<TraceMeta>) -> Survey {
    let mut survey = Survey::new();
    survey.traces = traces;
    survey
}

/// A journal's lines without the ones `drop` names (a torn or trimmed copy).
fn lines_without(body: &str, drop: impl Fn(&str) -> bool) -> String {
    let mut kept = String::new();
    for line in body.lines().filter(|l| !drop(l)) {
        kept.push_str(line);
        kept.push('\n');
    }
    kept
}

// ── LN · the frozen C7b oracle ─────────────────────────────────────────

/// LN1 · the paused journal alone: nothing observed, never authorized.
#[test]
fn ln1_the_paused_journal_alone_observes_nothing() {
    let dir = store("ln1");
    let paths = stage(&dir, &[S1_PAUSED]);
    assert_eq!(lineage_of(&dir, &paths[0]), Lineage::NoneObserved);
}

/// LN2 · a directory the reader may not list decides nothing.
#[cfg(unix)]
#[test]
fn ln2_an_unlistable_directory_decides_nothing() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = store("ln2");
    let paths = stage(&dir, &[S1_PAUSED]);
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o000)).expect("chmod");
    let listed = std::fs::read_dir(&dir).is_ok();
    let lineage = lineage_of(&dir, &paths[0]);
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).expect("chmod back");
    if listed {
        return; // a privileged runner lists anything: nothing to prove here
    }
    let reasons = reasons(&lineage);
    assert!(
        reasons.contains(&Undecided::PausedUnidentified),
        "{lineage:?}"
    );
    assert!(reasons.iter().any(|r| matches!(
        r,
        Undecided::Unsurveyed { dir_errors, .. } if dir_errors == &[ErrorKind::PermissionDenied]
    )));
}

/// LN3 · an entry the reader refuses may be a continuation.
#[test]
fn ln3_a_refused_entry_decides_nothing() {
    let dir = store("ln3");
    let paths = stage(&dir, &[S1_PAUSED, ("garbage.ndjson", "{not json\n")]);
    let lineage = lineage_of(&dir, &paths[0]);
    assert!(
        reasons(&lineage).iter().any(|r| matches!(
            r,
            Undecided::Unsurveyed { skipped, .. }
                if matches!(skipped.as_slice(), [s] if matches!(s.why, SkipWhy::NoOpening(_)))
        )),
        "{lineage:?}"
    );
}

/// LN4 · a directory named like a journal, and a name that is not UTF-8.
#[test]
fn ln4_a_non_journal_entry_decides_nothing() {
    let dir = store("ln4");
    let paths = stage(&dir, &[S1_PAUSED]);
    std::fs::create_dir(dir.join("folder.ndjson")).expect("a directory");
    let lineage = lineage_of(&dir, &paths[0]);
    assert!(reasons(&lineage).iter().any(|r| matches!(
        r,
        Undecided::Unsurveyed { skipped, .. } if skipped.iter().any(|s| s.why == SkipWhy::NotAFile)
    )));
    let mut surveyed = survey(&dir);
    surveyed.skipped.clear();
    surveyed.skipped.push(nika_dap::store::Skipped::new(
        dir.join("name.ndjson"),
        SkipWhy::NameNotUtf8,
    ));
    let lineage = fold(&surveyed, &paths[0]);
    assert!(
        matches!(reasons(&lineage), [Undecided::Unsurveyed { .. }]),
        "{lineage:?}"
    );
}

/// LN5 · S1: the outside resume completed, the stale answer was refused as
/// a replay — two continuations, never ranked, whatever their mtimes.
#[test]
fn ln5_s1_leaves_a_fork_of_completed_and_refused() {
    let dir = store("ln5");
    let paths = stage(&dir, &[S1_PAUSED, S1_COMPLETED, S1_REFUSED]);
    let run = run_of(&dir, S1_PAUSED.0);
    let lineage = lineage_of(&dir, &paths[0]);
    let reasons = reasons(&lineage);
    assert!(
        matches!(reasons, [Undecided::Fork { .. }]),
        "a fork, and only a fork: {lineage:?}"
    );
    let Some(Undecided::Fork { run_id, successors }) = reasons.first() else {
        return;
    };
    assert_eq!(run_id, &run);
    let mut states: Vec<TraceState> = successors.iter().map(|(_, s)| *s).collect();
    states.sort_by_key(|s| s.as_str());
    assert_eq!(states, vec![TraceState::Failed, TraceState::Succeeded]);
}

/// LN6 · X1 (two completed continuations) and T1 (a completed one and a
/// re-paused one): forks, each sibling with its state.
#[test]
fn ln6_x1_and_t1_leave_forks() {
    let dir = store("ln6-x1");
    let paths = stage(&dir, &[X1_PAUSED, X1_COMPLETED_1, X1_COMPLETED_2]);
    let lineage = lineage_of(&dir, &paths[0]);
    assert!(
        matches!(
            reasons(&lineage),
            [Undecided::Fork { successors, .. }]
                if successors.iter().all(|(_, s)| *s == TraceState::Succeeded) && successors.len() == 2
        ),
        "{lineage:?}"
    );
    let dir = store("ln6-t1");
    let paths = stage(&dir, &[T1_PAUSED, T1_COMPLETED, T1_REPAUSED]);
    let lineage = lineage_of(&dir, &paths[0]);
    let reasons = reasons(&lineage);
    assert!(
        matches!(reasons, [Undecided::Fork { .. }]),
        "a fork: {lineage:?}"
    );
    let Some(Undecided::Fork { successors, .. }) = reasons.first() else {
        return;
    };
    let mut states: Vec<&str> = successors.iter().map(|(_, s)| s.as_str()).collect();
    states.sort_unstable();
    assert_eq!(states, vec!["paused", "succeeded"]);
}

/// LN7 · a continuation carrying the paused run's own identity: a cycle
/// (and a duplicate identity) — the walk ends.
#[test]
fn ln7_a_crafted_cycle_ends_the_walk() {
    let survey = survey_of(vec![
        meta("paused", Some("a"), None, TraceState::Paused),
        meta("crafted", Some("a"), Some("a"), TraceState::Succeeded),
    ]);
    let lineage = fold(&survey, Path::new("paused"));
    let reasons = reasons(&lineage);
    assert!(
        reasons.contains(&Undecided::Cycle {
            run_id: "a".to_owned()
        }),
        "{lineage:?}"
    );
    assert!(
        reasons
            .iter()
            .any(|r| matches!(r, Undecided::Duplicate { run_id, .. } if run_id == "a"))
    );
}

/// LN8 · a byte copy of the paused journal under another name.
#[test]
fn ln8_a_byte_copy_is_a_duplicate_identity() {
    let dir = store("ln8");
    let paths = stage(&dir, &[S1_PAUSED, ("copy.ndjson", S1_PAUSED.1)]);
    let lineage = lineage_of(&dir, &paths[0]);
    assert!(
        matches!(
            reasons(&lineage),
            [Undecided::Duplicate { paths, .. }] if paths.len() == 2
        ),
        "{lineage:?}"
    );
}

/// LN9 · LN10 · a continuation recording another workflow or project.
#[test]
fn ln9_ln10_a_disagreeing_continuation() {
    let mut other = meta("next", Some("b"), Some("a"), TraceState::Succeeded);
    other.workflow = "other".to_owned();
    other.project = Some("q".repeat(64));
    let survey = survey_of(vec![
        meta("paused", Some("a"), None, TraceState::Paused),
        other,
    ]);
    let lineage = fold(&survey, Path::new("paused"));
    let fields: Vec<&str> = reasons(&lineage)
        .iter()
        .filter_map(|r| match r {
            Undecided::Disagreement { field, .. } => Some(*field),
            _ => None,
        })
        .collect();
    assert_eq!(fields, vec!["workflow", "project"], "{lineage:?}");
}

/// LN11 · a continuation whose opening frame names no execution.
#[test]
fn ln11_a_continuation_without_identity_disagrees() {
    let survey = survey_of(vec![
        meta("paused", Some("a"), None, TraceState::Paused),
        meta("next", None, Some("a"), TraceState::Succeeded),
    ]);
    let lineage = fold(&survey, Path::new("paused"));
    assert_eq!(
        reasons(&lineage),
        &[Undecided::Disagreement {
            path: PathBuf::from("next"),
            field: "run_id"
        }]
    );
}

/// LN12 · S1's paused journal and its completed continuation: a settled chain.
#[test]
fn ln12_s1_chain_settles() {
    let dir = store("ln12");
    let paths = stage(&dir, &[S1_PAUSED, S1_COMPLETED]);
    assert_eq!(
        lineage_of(&dir, &paths[0]),
        Lineage::Chain {
            links: vec![paths[1].clone()],
            head: Head::Settled(TraceState::Succeeded)
        }
    );
}

/// LN13 · T1's paused journal and its re-paused continuation: the head's
/// own gate, never the stale one.
#[test]
fn ln13_t1_chain_pauses_again() {
    let dir = store("ln13");
    let paths = stage(&dir, &[T1_PAUSED, T1_REPAUSED]);
    assert_eq!(
        lineage_of(&dir, &paths[0]),
        Lineage::Chain {
            links: vec![paths[1].clone()],
            head: Head::Paused {
                trace: paths[1].clone(),
                task: Some("ask".to_owned())
            }
        }
    );
}

/// LN14 · a continuation with no terminal: unknown (no lease), dead (a
/// lease nobody holds), alive (a lease held) — ADR-129's words.
#[cfg(unix)]
#[test]
fn ln14_a_running_continuation_says_its_liveness() {
    use nika_dap::liveness::Liveness;
    let dir = store("ln14");
    let running = lines_without(S1_COMPLETED.1, |l| {
        !l.contains("\"kind\":\"workflow_started\"") && !l.contains("\"kind\":\"task_scheduled\"")
    });
    let paths = stage(&dir, &[S1_PAUSED, (S1_COMPLETED.0, &running)]);
    let head = |dir: &Path| match lineage_of(dir, &paths[0]) {
        Lineage::Chain { head, .. } => Some(head),
        _ => None,
    };
    assert_eq!(head(&dir), Some(Head::Running(Some(Liveness::Unknown))));
    let lease = nika_dap::liveness::lease_path(&paths[1]);
    let host = nika_dap::liveness::host_name();
    std::fs::write(&lease, format!("{{\"pid\":1,\"host\":\"{host}\"}}\n")).expect("a lease");
    assert_eq!(
        head(&dir),
        Some(Head::Running(Some(Liveness::Dead { pid: 1 })))
    );
    let _held = nika_dap::liveness::hold(&paths[1]).expect("held");
    assert_eq!(
        head(&dir),
        Some(Head::Running(Some(Liveness::Alive {
            pid: std::process::id()
        })))
    );
}

/// LN15 · a chain of 65 continuations ends `TooLong` after exactly 64
/// links; a chain of 64 is a chain. No input makes the fold loop.
#[test]
fn ln15_the_walk_terminates_at_64_links() {
    let chain = |len: usize| {
        let mut traces = vec![meta("j0", Some("r0"), None, TraceState::Paused)];
        for i in 1..=len {
            let (name, run, from) = (format!("j{i}"), format!("r{i}"), format!("r{}", i - 1));
            traces.push(meta(&name, Some(&run), Some(&from), TraceState::Succeeded));
        }
        survey_of(traces)
    };
    let lineage = fold(&chain(MAX_LINKS), Path::new("j0"));
    assert!(matches!(&lineage, Lineage::Chain { links, .. } if links.len() == MAX_LINKS));
    let lineage = fold(&chain(MAX_LINKS + 1), Path::new("j0"));
    assert_eq!(reasons(&lineage), &[Undecided::TooLong]);
}

/// LN16 · `scan` is the survey's fail-open projection, fact for fact, on
/// every fixture directory (real journals, a torn one, a refused one).
#[test]
fn ln16_scan_is_the_projection_of_the_survey() {
    let dir = store("ln16");
    let torn = format!("{}{{\"id\":{{\"uuid\":\"torn", S1_REFUSED.1);
    stage(
        &dir,
        &[
            S1_PAUSED,
            S1_COMPLETED,
            (S1_REFUSED.0, &torn),
            T1_REPAUSED,
            ("garbage.ndjson", "{not json\n"),
        ],
    );
    let facts = |t: &TraceMeta| {
        let name = (t.path.clone(), t.name.clone(), t.workflow.clone(), t.state);
        let rest = (t.paused_task.clone(), t.bytes, t.modified, t.liveness);
        (
            name,
            rest,
            t.resumed_from.clone(),
            t.run_id.clone(),
            t.project.clone(),
        )
    };
    let mut surveyed = survey(&dir).traces;
    surveyed.sort_by(|a, b| b.modified.cmp(&a.modified).then(a.name.cmp(&b.name)));
    let scanned: Vec<_> = scan(&dir).iter().map(facts).collect();
    assert_eq!(scanned, surveyed.iter().map(facts).collect::<Vec<_>>());
    assert_eq!(
        scanned.len(),
        4,
        "the torn journal stays; the refused entry does not"
    );
}

/// LN17 · renaming journals and permuting mtimes never changes a verdict.
#[test]
fn ln17_names_and_mtimes_never_rank() {
    let verdicts = |order: &[usize], names: &[&str]| {
        let journals = [S1_PAUSED, S1_COMPLETED, S1_REFUSED];
        let dir = store(&format!("ln17-{}", names.join("")));
        let staged: Vec<(&str, &str)> = order.iter().map(|&i| (names[i], journals[i].1)).collect();
        let paths = stage(&dir, &staged);
        let paused = paths[order.iter().position(|&i| i == 0).expect("paused staged")].clone();
        let fork = shape(&lineage_of(&dir, &paused));
        std::fs::remove_file(&paths[order.iter().position(|&i| i == 2).expect("refused")])
            .expect("drop the refused leg");
        (fork, shape(&lineage_of(&dir, &paused)))
    };
    let base = verdicts(&[0, 1, 2], &["a.ndjson", "b.ndjson", "c.ndjson"]);
    assert!(base.0.starts_with("indeterminate") && base.1 == "chain 1 settled succeeded");
    for (order, names) in [
        ([2, 1, 0], ["c.ndjson", "b.ndjson", "a.ndjson"]),
        ([1, 2, 0], ["z.ndjson", "a.ndjson", "m.ndjson"]),
        ([0, 2, 1], ["m.ndjson", "z.ndjson", "a.ndjson"]),
    ] {
        assert_eq!(
            verdicts(&order, &names),
            base,
            "order {order:?} names {names:?}"
        );
    }
}

// ── U · the C7c uncertainty cases ─────────────────────────────────────

/// U1 · the paused journal torn after its pause frame: never `NoneObserved`.
#[test]
fn u1_a_torn_paused_journal_is_doubtful() {
    let dir = store("u1");
    let torn = format!("{}{{\"id\":{{\"uuid\":\"torn", S1_PAUSED.1);
    let paths = stage(&dir, &[(S1_PAUSED.0, &torn)]);
    let lineage = lineage_of(&dir, &paths[0]);
    assert!(
        matches!(
            reasons(&lineage),
            [Undecided::Doubtful { path, why: DoubtWhy::TornSuffix(_) }] if path == &paths[0]
        ),
        "{lineage:?}"
    );
}

/// U2 · a continuation torn mid-run (its terminal lost): never settled.
#[test]
fn u2_a_torn_continuation_never_settles() {
    let dir = store("u2");
    let cut = lines_without(S1_COMPLETED.1, |l| {
        l.contains("\"kind\":\"workflow_completed\"")
    });
    let torn = format!("{cut}{{\"id\":{{\"uuid\":\"torn");
    let paths = stage(&dir, &[S1_PAUSED, (S1_COMPLETED.0, &torn)]);
    let lineage = lineage_of(&dir, &paths[0]);
    assert!(
        matches!(
            reasons(&lineage),
            [Undecided::Doubtful { path, why: DoubtWhy::TornSuffix(_) }] if path == &paths[1]
        ),
        "{lineage:?}"
    );
}

/// U3 · an unrelated torn journal (its link names no run of this lineage)
/// does not block; the survey still says its doubt.
#[test]
fn u3_an_unrelated_torn_journal_does_not_block() {
    let dir = store("u3");
    let torn = format!("{}{{\"id\":{{\"uuid\":\"torn", X1_PAUSED.1);
    let paths = stage(&dir, &[S1_PAUSED, (X1_PAUSED.0, &torn)]);
    assert_eq!(lineage_of(&dir, &paths[0]), Lineage::NoneObserved);
    assert!(matches!(
        survey(&dir).doubts.as_slice(),
        [d] if d.path == paths[1] && matches!(d.why, DoubtWhy::TornSuffix(_))
    ));
}

/// U4 · a journal with no opening frame: its link is unknown, it blocks.
#[test]
fn u4_a_journal_without_opening_frame_blocks() {
    let dir = store("u4");
    let headless = lines_without(X1_COMPLETED_1.1, |l| {
        l.contains("\"kind\":\"workflow_started\"")
    });
    let paths = stage(&dir, &[S1_PAUSED, (X1_COMPLETED_1.0, &headless)]);
    let lineage = lineage_of(&dir, &paths[0]);
    assert!(
        matches!(
            reasons(&lineage),
            [Undecided::Doubtful {
                why: DoubtWhy::NoOpeningFrame,
                ..
            }]
        ),
        "{lineage:?}"
    );
}

/// U5 · one journal carrying two runs' frames: its identity is ambiguous.
#[test]
fn u5_conflicting_identities_block() {
    let dir = store("u5");
    let both = format!("{}{}", X1_PAUSED.1, T1_PAUSED.1);
    let paths = stage(&dir, &[S1_PAUSED, ("both.ndjson", &both)]);
    let lineage = lineage_of(&dir, &paths[0]);
    assert!(
        matches!(
            reasons(&lineage),
            [Undecided::Doubtful {
                why: DoubtWhy::ConflictingIdentity,
                ..
            }]
        ),
        "{lineage:?}"
    );
}

/// U7 · a listing error decides nothing, whatever was listed around it.
#[test]
fn u7_a_listing_error_decides_nothing() {
    let dir = store("u7");
    let paths = stage(&dir, &[S1_PAUSED, S1_COMPLETED]);
    let mut surveyed = survey(&dir);
    surveyed.dir_errors.push(ErrorKind::Interrupted);
    let lineage = fold(&surveyed, &paths[0]);
    assert!(
        matches!(
            reasons(&lineage),
            [Undecided::Unsurveyed { dir_errors, .. }] if dir_errors == &[ErrorKind::Interrupted]
        ),
        "{lineage:?}"
    );
}

/// U8 · U9 · a valid settlement envelope changes nothing; a torn one is a tear.
#[test]
fn u8_u9_the_settlement_envelope() {
    let envelope =
        "{\"kind\":\"run_settled\",\"status\":\"paused\",\"outputs\":{},\"chain\":\"c\"}\n";
    let dir = store("u8");
    let settled = format!("{}{envelope}", S1_PAUSED.1);
    let paths = stage(&dir, &[(S1_PAUSED.0, &settled), S1_COMPLETED]);
    assert!(matches!(
        lineage_of(&dir, &paths[0]),
        Lineage::Chain {
            head: Head::Settled(TraceState::Succeeded),
            ..
        }
    ));
    let dir = store("u9");
    let torn = format!("{}{{\"kind\":\"run_settled\",\"status\":\"pau", S1_PAUSED.1);
    let paths = stage(&dir, &[(S1_PAUSED.0, &torn)]);
    assert!(matches!(
        reasons(&lineage_of(&dir, &paths[0])),
        [Undecided::Doubtful {
            why: DoubtWhy::TornSuffix(_),
            ..
        }]
    ));
}

/// U10 · an empty entry and a garbage entry: both said, both block.
#[test]
fn u10_empty_and_garbage_entries_block() {
    let dir = store("u10");
    let paths = stage(
        &dir,
        &[
            S1_PAUSED,
            ("empty.ndjson", ""),
            ("garbage.ndjson", "{not json\n"),
        ],
    );
    let lineage = lineage_of(&dir, &paths[0]);
    assert!(
        matches!(
            reasons(&lineage),
            [Undecided::Unsurveyed { skipped, dir_errors }]
                if dir_errors.is_empty()
                    && skipped.len() == 2
                    && skipped.iter().all(|s| matches!(s.why, SkipWhy::NoOpening(_)))
        ),
        "{lineage:?}"
    );
}

/// U11 · a missing trace directory: unsurveyed, and no paused journal.
#[test]
fn u11_a_missing_directory() {
    let dir = store("u11").join("absent");
    let lineage = lineage_of(&dir, &dir.join(S1_PAUSED.0));
    assert_eq!(
        reasons(&lineage),
        &[
            Undecided::Unsurveyed {
                dir_errors: vec![ErrorKind::NotFound],
                skipped: Vec::new()
            },
            Undecided::PausedUnidentified
        ]
    );
}

/// U12 · the "paused" journal records a completion after its pause (the
/// store's own last-terminal case): not a gate anyone can answer.
#[test]
fn u12_a_journal_no_longer_paused() {
    let dir = store("u12");
    let paused = S1_PAUSED.1.lines().last().expect("the pause frame");
    let completed = paused.replace(
        "\"kind\":\"workflow_paused\"",
        "\"kind\":\"workflow_completed\"",
    );
    let body = format!("{}{completed}\n", S1_PAUSED.1);
    let paths = stage(&dir, &[(S1_PAUSED.0, &body)]);
    assert_eq!(
        reasons(&lineage_of(&dir, &paths[0])),
        &[Undecided::NotPaused {
            state: TraceState::Succeeded
        }]
    );
}

/// The paused journal must name its run: otherwise nothing is decidable.
#[test]
fn a_paused_journal_without_identity_decides_nothing() {
    let survey = survey_of(vec![meta("paused", None, None, TraceState::Paused)]);
    assert_eq!(
        reasons(&fold(&survey, Path::new("paused"))),
        &[Undecided::PausedUnidentified]
    );
    assert_eq!(
        reasons(&fold(&survey, Path::new("elsewhere"))),
        &[Undecided::PausedUnidentified]
    );
}

// ── the real journals (C7b public runs · minimized, every frame kept) ──

/// C7b `S1` on the C6 binary: `2026-09-28T12-59-54Z-41fb.ndjson` (source sha256 `be34a1fbddba669b…`).
const S1_COMPLETED: (&str, &str) = (
    "2026-09-28T12-59-54Z-41fb.ndjson",
    r#"{"id":{"uuid":"01a0e819-b6cd-70f2-8edf-c29fbd00bc4d"},"timestamp":1790600394445000000,"kind":"workflow_started","execution":{"uuid":"01a0e819-b6c9-7781-bb7b-48dc617a41fb"},"run":null,"correlation":null,"chain":"7466341540fd02fca9ec21937862176b7821a52495b86d81bb5f30d16c8462dc","fields":[{"key":"workflow","value":"gate-keyed"},{"key":"project_root_fingerprint","value":"68bc0fa6f93982fd69bcd7dc3b4074d55f54a57579461599d47765293bdbf7cd"},{"key":"resumed_from","value":"01a0e819b689730eab2140ea767e53b6"}]}
{"id":{"uuid":"01a0e819-b6ce-7024-9501-1244e9a1a0c9"},"timestamp":1790600394446000000,"kind":"task_scheduled","execution":{"uuid":"01a0e819-b6c9-7781-bb7b-48dc617a41fb"},"run":null,"correlation":null,"chain":"15f0b09884bd0ed88e270574cdb8a2849b6e7489240f380ab5cb5961b9c8b99a","fields":[{"key":"task","value":"before"}]}
{"id":{"uuid":"01a0e819-b6ce-7024-9501-1245ad4dca3b"},"timestamp":1790600394446000000,"kind":"task_scheduled","execution":{"uuid":"01a0e819-b6c9-7781-bb7b-48dc617a41fb"},"run":null,"correlation":null,"chain":"8cf9b0b6fb9391c89505660bf274d117814dcba569924f63effb2c31f4b5635a","fields":[{"key":"task","value":"ask"}]}
{"id":{"uuid":"01a0e819-b6ce-7024-9501-1246be5519c8"},"timestamp":1790600394446000000,"kind":"task_scheduled","execution":{"uuid":"01a0e819-b6c9-7781-bb7b-48dc617a41fb"},"run":null,"correlation":null,"chain":"110eb07f681bcf221e8b2e715ec06abaa9be7de90a302788bf543f3e09993b1c","fields":[{"key":"task","value":"after_gate"}]}
{"id":{"uuid":"01a0e819-b6cf-7402-8b41-27cd6cd68b8b"},"timestamp":1790600394447000000,"kind":"task_cache_hit","execution":{"uuid":"01a0e819-b6c9-7781-bb7b-48dc617a41fb"},"run":null,"correlation":null,"chain":"6f5921942037ec71413c487fe4329300eff8be9378271384e5cb5ad7aba617f1","fields":[{"key":"task","value":"before"}]}
{"id":{"uuid":"01a0e819-b6d4-7387-b93e-b6a69b64de1c"},"timestamp":1790600394452000000,"kind":"task_started","execution":{"uuid":"01a0e819-b6c9-7781-bb7b-48dc617a41fb"},"run":null,"correlation":null,"chain":"e89a1afe6e231c5a1088e2980fdee1b8199a8727bf5d6cce87d197b92d3064de","fields":[{"key":"task","value":"ask"}]}
{"id":{"uuid":"01a0e819-b6d4-7387-b93e-b6a72ebcc7e3"},"timestamp":1790600394452000000,"kind":"permit_checked","execution":{"uuid":"01a0e819-b6c9-7781-bb7b-48dc617a41fb"},"run":null,"correlation":null,"chain":"1b960fe27fb23bf5aa960a78c3ba50d88b6db82f3a107763c765e3f83c39ae9d","fields":[{"key":"task","value":"ask"},{"key":"decision","value":"allow"},{"key":"why","value":"pure-internal exemption (NEP-0003 law 1 · any block form)"}]}
{"id":{"uuid":"01a0e819-b6d4-7387-b93e-b6a8e3e1f7fb"},"timestamp":1790600394452000000,"kind":"approval_decided","execution":{"uuid":"01a0e819-b6c9-7781-bb7b-48dc617a41fb"},"run":null,"correlation":null,"chain":"4a82dbd7bdd3c58acc7a07c42d5d1ba8138226982fed8b6fdb959c9f9ccfdcb8","fields":[{"key":"task","value":"ask"},{"key":"mode","value":"confirm"},{"key":"decision","value":"allow"}]}
{"id":{"uuid":"01a0e819-b6d5-74be-bcf2-9d35e96a9078"},"timestamp":1790600394453000000,"kind":"task_completed","execution":{"uuid":"01a0e819-b6c9-7781-bb7b-48dc617a41fb"},"run":null,"correlation":null,"chain":"f78e9613890f4b346fb04e5344f0ef6e41bba07314c96938d16f7910dfcc7d33","fields":[{"key":"task","value":"ask"}]}
{"id":{"uuid":"01a0e819-b6d8-70a7-9830-d5240220fffc"},"timestamp":1790600394456000000,"kind":"task_started","execution":{"uuid":"01a0e819-b6c9-7781-bb7b-48dc617a41fb"},"run":null,"correlation":null,"chain":"3bda8e91c45e1f2bb4ed5e6c5a502b61a11ca003888b31de905121748f4c4eb4","fields":[{"key":"task","value":"after_gate"}]}
{"id":{"uuid":"01a0e819-b6d8-70a7-9830-d5251116396a"},"timestamp":1790600394456000000,"kind":"permit_checked","execution":{"uuid":"01a0e819-b6c9-7781-bb7b-48dc617a41fb"},"run":null,"correlation":null,"chain":"c3d5e3e65a00bb2c2604cb17cc2842d13c5a3c0306632798a69bea75eb1330fd","fields":[{"key":"task","value":"after_gate"},{"key":"decision","value":"allow"},{"key":"why","value":"permits.tools covers the id"}]}
{"id":{"uuid":"01a0e819-b6d8-70a7-9830-d5264e94113f"},"timestamp":1790600394456000000,"kind":"task_completed","execution":{"uuid":"01a0e819-b6c9-7781-bb7b-48dc617a41fb"},"run":null,"correlation":null,"chain":"4ff0d008815cd7e67b118a71478c8f7d29b3eb9c670115a16da0b7bae5c0aa83","fields":[{"key":"task","value":"after_gate"}]}
{"id":{"uuid":"01a0e819-b6d8-70a7-9830-d52752200e4a"},"timestamp":1790600394456000000,"kind":"workflow_completed","execution":{"uuid":"01a0e819-b6c9-7781-bb7b-48dc617a41fb"},"run":null,"correlation":null,"chain":"f672fab9369fcf120b70c1796026b33ee3ee19363eb18656b808c8033715f2e8","fields":[{"key":"workflow","value":"gate-keyed"},{"key":"status","value":"succeeded"},{"key":"cause","value":"normal"}]}
"#,
);

/// C7b `S1` on the C6 binary: `2026-09-28T12-59-54Z-53b6.ndjson` (source sha256 `6c0952bcb645ed57…`).
const S1_PAUSED: (&str, &str) = (
    "2026-09-28T12-59-54Z-53b6.ndjson",
    r#"{"id":{"uuid":"01a0e819-b68b-7649-85b2-39277be74e66"},"timestamp":1790600394379000000,"kind":"workflow_started","execution":{"uuid":"01a0e819-b689-730e-ab21-40ea767e53b6"},"run":null,"correlation":null,"chain":"7466341540fd02fca9ec21937862176b7821a52495b86d81bb5f30d16c8462dc","fields":[{"key":"workflow","value":"gate-keyed"},{"key":"project_root_fingerprint","value":"68bc0fa6f93982fd69bcd7dc3b4074d55f54a57579461599d47765293bdbf7cd"}]}
{"id":{"uuid":"01a0e819-b68c-726d-a8e3-3ef859c76d0f"},"timestamp":1790600394380000000,"kind":"task_scheduled","execution":{"uuid":"01a0e819-b689-730e-ab21-40ea767e53b6"},"run":null,"correlation":null,"chain":"527926e042b24c4415b65b50cca37f0f1f609ec9f52478191a9faf23491600c3","fields":[{"key":"task","value":"before"}]}
{"id":{"uuid":"01a0e819-b68d-735a-9777-3c6706958b21"},"timestamp":1790600394381000000,"kind":"task_scheduled","execution":{"uuid":"01a0e819-b689-730e-ab21-40ea767e53b6"},"run":null,"correlation":null,"chain":"eee513cc41db18434eb38cbf51b55d48946fbea527dc0f80777a31deaff40551","fields":[{"key":"task","value":"ask"}]}
{"id":{"uuid":"01a0e819-b68d-735a-9777-3c683f5bba50"},"timestamp":1790600394381000000,"kind":"task_scheduled","execution":{"uuid":"01a0e819-b689-730e-ab21-40ea767e53b6"},"run":null,"correlation":null,"chain":"0c76a73643ecc528974ba46ceb6025423d93139955cbd8807ee4549ed3be67f9","fields":[{"key":"task","value":"after_gate"}]}
{"id":{"uuid":"01a0e819-b690-75e2-8832-255d6a8aeee0"},"timestamp":1790600394384000000,"kind":"task_started","execution":{"uuid":"01a0e819-b689-730e-ab21-40ea767e53b6"},"run":null,"correlation":null,"chain":"1ee9c3dc4a185833b486d65cc324a5022fc69a96f4f39707ff60d4f19e4493a2","fields":[{"key":"task","value":"before"}]}
{"id":{"uuid":"01a0e819-b690-75e2-8832-255ea327f97a"},"timestamp":1790600394384000000,"kind":"permit_checked","execution":{"uuid":"01a0e819-b689-730e-ab21-40ea767e53b6"},"run":null,"correlation":null,"chain":"a7f5e04b1f6ddcd5ee13fa89aed3240f228b59b4ea4620fbb6c51bd442e8c5d8","fields":[{"key":"task","value":"before"},{"key":"decision","value":"allow"},{"key":"why","value":"permits.tools covers the id"}]}
{"id":{"uuid":"01a0e819-b690-75e2-8832-255f93243257"},"timestamp":1790600394384000000,"kind":"task_completed","execution":{"uuid":"01a0e819-b689-730e-ab21-40ea767e53b6"},"run":null,"correlation":null,"chain":"1e89b99a6737b7686166dd97a346879fb2d829c2ca73b550ba2a9b00967273b7","fields":[{"key":"task","value":"before"}]}
{"id":{"uuid":"01a0e819-b691-7011-a1fc-369f8aa8657f"},"timestamp":1790600394385000000,"kind":"workflow_paused","execution":{"uuid":"01a0e819-b689-730e-ab21-40ea767e53b6"},"run":null,"correlation":null,"chain":"33180c9c50ec797c947a4969df6319a91f404bad00f875bcfd7d44e02bdccba0","fields":[{"key":"workflow","value":"gate-keyed"},{"key":"task","value":"ask"},{"key":"mode","value":"confirm"},{"key":"message","value":"Ship it?"},{"key":"status","value":"paused"},{"key":"cause","value":"human_gate"}]}
"#,
);

/// C7b `S1` on the C6 binary: `2026-09-28T12-59-54Z-7ff7.ndjson` (source sha256 `97d82abffd250c73…`).
const S1_REFUSED: (&str, &str) = (
    "2026-09-28T12-59-54Z-7ff7.ndjson",
    r#"{"id":{"uuid":"01a0e819-b731-75c7-a60b-b2ca662fbc4f"},"timestamp":1790600394545000000,"kind":"workflow_started","execution":{"uuid":"01a0e819-b72d-704a-9ecb-5d0875707ff7"},"run":null,"correlation":null,"chain":"7466341540fd02fca9ec21937862176b7821a52495b86d81bb5f30d16c8462dc","fields":[{"key":"workflow","value":"gate-keyed"},{"key":"project_root_fingerprint","value":"68bc0fa6f93982fd69bcd7dc3b4074d55f54a57579461599d47765293bdbf7cd"},{"key":"resumed_from","value":"01a0e819b689730eab2140ea767e53b6"}]}
{"id":{"uuid":"01a0e819-b733-76b6-8f2b-50e056c66d01"},"timestamp":1790600394547000000,"kind":"task_scheduled","execution":{"uuid":"01a0e819-b72d-704a-9ecb-5d0875707ff7"},"run":null,"correlation":null,"chain":"6393caa49d2dcd88848d2902b86d9a130f962b97ac5a5ee69a663eb5f7519679","fields":[{"key":"task","value":"before"}]}
{"id":{"uuid":"01a0e819-b733-76b6-8f2b-50e1a48a3523"},"timestamp":1790600394547000000,"kind":"task_scheduled","execution":{"uuid":"01a0e819-b72d-704a-9ecb-5d0875707ff7"},"run":null,"correlation":null,"chain":"d9f1205be26f1d46a13d67049aa6e6b770c357a1999b040f95b464ae1e8ce924","fields":[{"key":"task","value":"ask"}]}
{"id":{"uuid":"01a0e819-b733-76b6-8f2b-50e269bf4c83"},"timestamp":1790600394547000000,"kind":"task_scheduled","execution":{"uuid":"01a0e819-b72d-704a-9ecb-5d0875707ff7"},"run":null,"correlation":null,"chain":"85b7a7eade7e2d9309cb1105a1bb1e23a3fd32617ed38e786f7c1bde9d70acc5","fields":[{"key":"task","value":"after_gate"}]}
{"id":{"uuid":"01a0e819-b733-76b6-8f2b-50e330695c78"},"timestamp":1790600394547000000,"kind":"task_cache_hit","execution":{"uuid":"01a0e819-b72d-704a-9ecb-5d0875707ff7"},"run":null,"correlation":null,"chain":"46d54688084ec83e977a2daf4fcedc48e0bc8c5184ebb3bcb3093acd2a4f7ec6","fields":[{"key":"task","value":"before"}]}
{"id":{"uuid":"01a0e819-b734-757b-9de7-0c2109b3f3a8"},"timestamp":1790600394548000000,"kind":"approval_decided","execution":{"uuid":"01a0e819-b72d-704a-9ecb-5d0875707ff7"},"run":null,"correlation":null,"chain":"501b36a2a076b6591c8eebebdf6a3af65fc5874e09de94ad6bc7513a25a3ccf4","fields":[{"key":"task","value":"ask"},{"key":"mode","value":"confirm"},{"key":"decision","value":"deny"},{"key":"why","value":"approval.replayed"}]}
{"id":{"uuid":"01a0e819-b734-757b-9de7-0c228dcb6754"},"timestamp":1790600394548000000,"kind":"task_failed","execution":{"uuid":"01a0e819-b72d-704a-9ecb-5d0875707ff7"},"run":null,"correlation":null,"chain":"20d251a24c5b5d7b058f302675cc79fbe2206979d31e671f335e11ff2d4a835e","fields":[{"key":"task","value":"ask"}]}
{"id":{"uuid":"01a0e819-b734-757b-9de7-0c2317b5aca6"},"timestamp":1790600394548000000,"kind":"task_cancelled","execution":{"uuid":"01a0e819-b72d-704a-9ecb-5d0875707ff7"},"run":null,"correlation":null,"chain":"b479c88fa17205021063523e60f59775b9ddd22fe82ba2994229d93d0a016e01","fields":[{"key":"task","value":"after_gate"}]}
{"id":{"uuid":"01a0e819-b734-757b-9de7-0c2488a5d9b9"},"timestamp":1790600394548000000,"kind":"workflow_failed","execution":{"uuid":"01a0e819-b72d-704a-9ecb-5d0875707ff7"},"run":null,"correlation":null,"chain":"dcb3fb2652ce38005539ba8abd4cd3893e26331f04fb8c76cd55d10afedb991d","fields":[{"key":"workflow","value":"gate-keyed"},{"key":"status","value":"failed"},{"key":"cause","value":"task_failed"}]}
"#,
);

/// C7b `X1` on the C6 binary: `2026-09-28T12-59-58Z-2144.ndjson` (source sha256 `c1989436f3a3766b…`).
const X1_COMPLETED_1: (&str, &str) = (
    "2026-09-28T12-59-58Z-2144.ndjson",
    r#"{"id":{"uuid":"01a0e819-c54e-75d8-a5a2-10a6a4b0feca"},"timestamp":1790600398158000000,"kind":"workflow_started","execution":{"uuid":"01a0e819-c54a-7393-bc48-b2237baf2144"},"run":null,"correlation":null,"chain":"7466341540fd02fca9ec21937862176b7821a52495b86d81bb5f30d16c8462dc","fields":[{"key":"workflow","value":"gate-keyed"},{"key":"project_root_fingerprint","value":"e2a74abf8a271206d2ca3873c6112b5f3837eebe17e559a0975a4ffd59abd7a8"},{"key":"resumed_from","value":"01a0e819c50a75709c242b13f16e2299"}]}
{"id":{"uuid":"01a0e819-c54f-73e6-9978-3b22e8b5b4eb"},"timestamp":1790600398159000000,"kind":"task_scheduled","execution":{"uuid":"01a0e819-c54a-7393-bc48-b2237baf2144"},"run":null,"correlation":null,"chain":"3438ba2b3f50061d5dff363a1c84e3d9f0a9579e8d7ddc5553823b4b3d33e3da","fields":[{"key":"task","value":"before"}]}
{"id":{"uuid":"01a0e819-c54f-73e6-9978-3b235bcc5081"},"timestamp":1790600398159000000,"kind":"task_scheduled","execution":{"uuid":"01a0e819-c54a-7393-bc48-b2237baf2144"},"run":null,"correlation":null,"chain":"fe75bfd6018c3c34725cc0c8629280ab3f5cab39ea0716d55a8e1c489d69115c","fields":[{"key":"task","value":"ask"}]}
{"id":{"uuid":"01a0e819-c54f-73e6-9978-3b24de874130"},"timestamp":1790600398159000000,"kind":"task_scheduled","execution":{"uuid":"01a0e819-c54a-7393-bc48-b2237baf2144"},"run":null,"correlation":null,"chain":"3cb34037513cf08c32a5a89fef01bb882a0ea9231cd2885b5da83c8b9e38dadd","fields":[{"key":"task","value":"after_gate"}]}
{"id":{"uuid":"01a0e819-c54f-73e6-9978-3b2563ae8b9e"},"timestamp":1790600398159000000,"kind":"task_cache_hit","execution":{"uuid":"01a0e819-c54a-7393-bc48-b2237baf2144"},"run":null,"correlation":null,"chain":"118f1600de61da81d3cb3b78fcc4e63f93b71986ecb297daefd0bbd6490debb9","fields":[{"key":"task","value":"before"}]}
{"id":{"uuid":"01a0e819-c554-7213-b987-56ca199ae410"},"timestamp":1790600398164000000,"kind":"task_started","execution":{"uuid":"01a0e819-c54a-7393-bc48-b2237baf2144"},"run":null,"correlation":null,"chain":"0e4f2ef46eff2d51a2cadc9924d639f95e41ef98467b6806d6bb6663bc24b748","fields":[{"key":"task","value":"ask"}]}
{"id":{"uuid":"01a0e819-c554-7213-b987-56cb776165ef"},"timestamp":1790600398164000000,"kind":"permit_checked","execution":{"uuid":"01a0e819-c54a-7393-bc48-b2237baf2144"},"run":null,"correlation":null,"chain":"222d634c55e9aadff7862c983bde5b75348d1c563cfc0a992fd82f162d43f6ca","fields":[{"key":"task","value":"ask"},{"key":"decision","value":"allow"},{"key":"why","value":"pure-internal exemption (NEP-0003 law 1 · any block form)"}]}
{"id":{"uuid":"01a0e819-c554-7213-b987-56cccef2f64d"},"timestamp":1790600398164000000,"kind":"approval_decided","execution":{"uuid":"01a0e819-c54a-7393-bc48-b2237baf2144"},"run":null,"correlation":null,"chain":"03b38050afe4b2bb4dea71eb72ba5d571bdea84bfd12884892f365932d3a53da","fields":[{"key":"task","value":"ask"},{"key":"mode","value":"confirm"},{"key":"decision","value":"allow"}]}
{"id":{"uuid":"01a0e819-c554-7213-b987-56cdf5b6980d"},"timestamp":1790600398164000000,"kind":"task_completed","execution":{"uuid":"01a0e819-c54a-7393-bc48-b2237baf2144"},"run":null,"correlation":null,"chain":"7831c633b1108602fb623e5cccafeda9f65cad89d56edca84c6f417f794271be","fields":[{"key":"task","value":"ask"}]}
{"id":{"uuid":"01a0e819-c556-74c7-b169-1214d3fe16d0"},"timestamp":1790600398166000000,"kind":"task_started","execution":{"uuid":"01a0e819-c54a-7393-bc48-b2237baf2144"},"run":null,"correlation":null,"chain":"8291606d4f2cbb2b79ca438577347c5ce9a495cdc24e7a6f178863c2c53a14eb","fields":[{"key":"task","value":"after_gate"}]}
{"id":{"uuid":"01a0e819-c556-74c7-b169-1215af9d6c90"},"timestamp":1790600398166000000,"kind":"permit_checked","execution":{"uuid":"01a0e819-c54a-7393-bc48-b2237baf2144"},"run":null,"correlation":null,"chain":"54e06d8782a9a76683bcd374c9f4bd0c84c9a16c57a68ec7f877886cc88e8b02","fields":[{"key":"task","value":"after_gate"},{"key":"decision","value":"allow"},{"key":"why","value":"permits.tools covers the id"}]}
{"id":{"uuid":"01a0e819-c556-74c7-b169-1216d370751d"},"timestamp":1790600398166000000,"kind":"task_completed","execution":{"uuid":"01a0e819-c54a-7393-bc48-b2237baf2144"},"run":null,"correlation":null,"chain":"1f14ca4eb9757692fe09649713fee6da0b798c4bd190a01f48b9ea61ca129dc8","fields":[{"key":"task","value":"after_gate"}]}
{"id":{"uuid":"01a0e819-c556-74c7-b169-1217ccb45fd9"},"timestamp":1790600398166000000,"kind":"workflow_completed","execution":{"uuid":"01a0e819-c54a-7393-bc48-b2237baf2144"},"run":null,"correlation":null,"chain":"296d28458d796012f12174326b8c4725cc7dee03395df896764352c076354ee6","fields":[{"key":"workflow","value":"gate-keyed"},{"key":"status","value":"succeeded"},{"key":"cause","value":"normal"}]}
"#,
);

/// C7b `X1` on the C6 binary: `2026-09-28T12-59-58Z-2299.ndjson` (source sha256 `b3105a37aa537da8…`).
const X1_PAUSED: (&str, &str) = (
    "2026-09-28T12-59-58Z-2299.ndjson",
    r#"{"id":{"uuid":"01a0e819-c50d-713f-b469-43f7121640e4"},"timestamp":1790600398093000000,"kind":"workflow_started","execution":{"uuid":"01a0e819-c50a-7570-9c24-2b13f16e2299"},"run":null,"correlation":null,"chain":"7466341540fd02fca9ec21937862176b7821a52495b86d81bb5f30d16c8462dc","fields":[{"key":"workflow","value":"gate-keyed"},{"key":"project_root_fingerprint","value":"e2a74abf8a271206d2ca3873c6112b5f3837eebe17e559a0975a4ffd59abd7a8"}]}
{"id":{"uuid":"01a0e819-c50e-7400-8e1c-2b8d1310ddd2"},"timestamp":1790600398094000000,"kind":"task_scheduled","execution":{"uuid":"01a0e819-c50a-7570-9c24-2b13f16e2299"},"run":null,"correlation":null,"chain":"989df54dfaf53d8736c072722f066ff7907a465eb201fba51e6b48f93e73e3d5","fields":[{"key":"task","value":"before"}]}
{"id":{"uuid":"01a0e819-c50e-7400-8e1c-2b8ec1e8adf8"},"timestamp":1790600398094000000,"kind":"task_scheduled","execution":{"uuid":"01a0e819-c50a-7570-9c24-2b13f16e2299"},"run":null,"correlation":null,"chain":"12d89baa4834f4bdc728e6ef4a8125879409c792272fdea9c7cb30a5cb3ec4b2","fields":[{"key":"task","value":"ask"}]}
{"id":{"uuid":"01a0e819-c50e-7400-8e1c-2b8f240e04ac"},"timestamp":1790600398094000000,"kind":"task_scheduled","execution":{"uuid":"01a0e819-c50a-7570-9c24-2b13f16e2299"},"run":null,"correlation":null,"chain":"b1aef9bf86d5d5c7edf2afc13fbad822dcb667a5759336f9b2c24677250dbf46","fields":[{"key":"task","value":"after_gate"}]}
{"id":{"uuid":"01a0e819-c514-724e-a047-f7769afb589e"},"timestamp":1790600398100000000,"kind":"task_started","execution":{"uuid":"01a0e819-c50a-7570-9c24-2b13f16e2299"},"run":null,"correlation":null,"chain":"7f32d05f13e911cc40545d76ca02395bbea875f83b071aa4109c5bc8af3f50a9","fields":[{"key":"task","value":"before"}]}
{"id":{"uuid":"01a0e819-c514-724e-a047-f7776d522695"},"timestamp":1790600398100000000,"kind":"permit_checked","execution":{"uuid":"01a0e819-c50a-7570-9c24-2b13f16e2299"},"run":null,"correlation":null,"chain":"605fe677a086dba2679509c8a938ba670b4a8f17a349334ffc0fb53e8d1bbf99","fields":[{"key":"task","value":"before"},{"key":"decision","value":"allow"},{"key":"why","value":"permits.tools covers the id"}]}
{"id":{"uuid":"01a0e819-c514-724e-a047-f7782fe42fec"},"timestamp":1790600398100000000,"kind":"task_completed","execution":{"uuid":"01a0e819-c50a-7570-9c24-2b13f16e2299"},"run":null,"correlation":null,"chain":"e1a43b8c19f755e261e1c44a7cc8f1da3a42f99f82051965ec70a8fb0516d10e","fields":[{"key":"task","value":"before"}]}
{"id":{"uuid":"01a0e819-c515-715c-a0f4-ac32c4533da8"},"timestamp":1790600398101000000,"kind":"workflow_paused","execution":{"uuid":"01a0e819-c50a-7570-9c24-2b13f16e2299"},"run":null,"correlation":null,"chain":"7cf5cde505089e74b464dbf671b8f25f72a36f6e4d0602470cef9cd097aed5bb","fields":[{"key":"workflow","value":"gate-keyed"},{"key":"task","value":"ask"},{"key":"mode","value":"confirm"},{"key":"message","value":"Ship it?"},{"key":"status","value":"paused"},{"key":"cause","value":"human_gate"}]}
"#,
);

/// C7b `X1` on the C6 binary: `2026-09-28T12-59-58Z-25f3.ndjson` (source sha256 `8d008521c8474f49…`).
const X1_COMPLETED_2: (&str, &str) = (
    "2026-09-28T12-59-58Z-25f3.ndjson",
    r#"{"id":{"uuid":"01a0e819-c5a5-7029-9d2f-b2eb7c76842c"},"timestamp":1790600398245000000,"kind":"workflow_started","execution":{"uuid":"01a0e819-c5a2-7415-94ef-c8e3b3e125f3"},"run":null,"correlation":null,"chain":"7466341540fd02fca9ec21937862176b7821a52495b86d81bb5f30d16c8462dc","fields":[{"key":"workflow","value":"gate-keyed"},{"key":"project_root_fingerprint","value":"e2a74abf8a271206d2ca3873c6112b5f3837eebe17e559a0975a4ffd59abd7a8"},{"key":"resumed_from","value":"01a0e819c50a75709c242b13f16e2299"}]}
{"id":{"uuid":"01a0e819-c5a6-726d-85cc-e56e36d33ec3"},"timestamp":1790600398246000000,"kind":"task_scheduled","execution":{"uuid":"01a0e819-c5a2-7415-94ef-c8e3b3e125f3"},"run":null,"correlation":null,"chain":"6677c1e56429fa4a23348b92c73d3697dcd1dc56451fcae376014c9a957c1760","fields":[{"key":"task","value":"before"}]}
{"id":{"uuid":"01a0e819-c5a6-726d-85cc-e56f4db0c2dd"},"timestamp":1790600398246000000,"kind":"task_scheduled","execution":{"uuid":"01a0e819-c5a2-7415-94ef-c8e3b3e125f3"},"run":null,"correlation":null,"chain":"200803b7c4819676fb111b4e2be949a1126911f44675bd7c822f9c0a7eed93d0","fields":[{"key":"task","value":"ask"}]}
{"id":{"uuid":"01a0e819-c5a6-726d-85cc-e570b4dbdec7"},"timestamp":1790600398246000000,"kind":"task_scheduled","execution":{"uuid":"01a0e819-c5a2-7415-94ef-c8e3b3e125f3"},"run":null,"correlation":null,"chain":"02bfe32698b70feddcbcee5f2a8bf4e31e637506b9bd3d43a727bb65c5382eea","fields":[{"key":"task","value":"after_gate"}]}
{"id":{"uuid":"01a0e819-c5a6-726d-85cc-e57158d394b9"},"timestamp":1790600398246000000,"kind":"task_cache_hit","execution":{"uuid":"01a0e819-c5a2-7415-94ef-c8e3b3e125f3"},"run":null,"correlation":null,"chain":"4711e23deb0e1f3a5e97b283cec11f9a2b9702e4fe58b6ecc3e8ce4e1cc5d9ec","fields":[{"key":"task","value":"before"}]}
{"id":{"uuid":"01a0e819-c5ab-73d5-84b0-ebafa449ccde"},"timestamp":1790600398251000000,"kind":"task_started","execution":{"uuid":"01a0e819-c5a2-7415-94ef-c8e3b3e125f3"},"run":null,"correlation":null,"chain":"799e68ae0df0224e0cdc5443c3f4aea3368e1890d09c3b5e33bb7bb4623182f3","fields":[{"key":"task","value":"ask"}]}
{"id":{"uuid":"01a0e819-c5ab-73d5-84b0-ebb06561cff7"},"timestamp":1790600398251000000,"kind":"permit_checked","execution":{"uuid":"01a0e819-c5a2-7415-94ef-c8e3b3e125f3"},"run":null,"correlation":null,"chain":"36daf001938a1e77427fd379ace7c84ff883be23cf568ef291de220e63ec6245","fields":[{"key":"task","value":"ask"},{"key":"decision","value":"allow"},{"key":"why","value":"pure-internal exemption (NEP-0003 law 1 · any block form)"}]}
{"id":{"uuid":"01a0e819-c5ab-73d5-84b0-ebb14367cbe5"},"timestamp":1790600398251000000,"kind":"approval_decided","execution":{"uuid":"01a0e819-c5a2-7415-94ef-c8e3b3e125f3"},"run":null,"correlation":null,"chain":"36c12c64f634845a57bab87c8f92a3e13e60cc9cc8131a481f4560276eed2f07","fields":[{"key":"task","value":"ask"},{"key":"mode","value":"confirm"},{"key":"decision","value":"allow"}]}
{"id":{"uuid":"01a0e819-c5ac-77dc-8c00-59835367ff1b"},"timestamp":1790600398252000000,"kind":"task_completed","execution":{"uuid":"01a0e819-c5a2-7415-94ef-c8e3b3e125f3"},"run":null,"correlation":null,"chain":"273f75405c249d6765982cc34f56126d6fce7a093bdd21a47d7dd86f1982097a","fields":[{"key":"task","value":"ask"}]}
{"id":{"uuid":"01a0e819-c5ae-71b8-9766-d52ea34f68dc"},"timestamp":1790600398254000000,"kind":"task_started","execution":{"uuid":"01a0e819-c5a2-7415-94ef-c8e3b3e125f3"},"run":null,"correlation":null,"chain":"300eb112f88204b6a93fd77eba0b90462a86c9054190284e240f3c953ff941a7","fields":[{"key":"task","value":"after_gate"}]}
{"id":{"uuid":"01a0e819-c5ae-71b8-9766-d52f4254db2a"},"timestamp":1790600398254000000,"kind":"permit_checked","execution":{"uuid":"01a0e819-c5a2-7415-94ef-c8e3b3e125f3"},"run":null,"correlation":null,"chain":"879eee5587bf9433d40dbdc5973d4c77c0a52b9eaab261d91f65bb6d69c35cbd","fields":[{"key":"task","value":"after_gate"},{"key":"decision","value":"allow"},{"key":"why","value":"permits.tools covers the id"}]}
{"id":{"uuid":"01a0e819-c5ae-71b8-9766-d5302f17be83"},"timestamp":1790600398254000000,"kind":"task_completed","execution":{"uuid":"01a0e819-c5a2-7415-94ef-c8e3b3e125f3"},"run":null,"correlation":null,"chain":"f6bc2076d97d53b700f98fdbe75d4da653e866fa8438af5aa99d1648eac12c1c","fields":[{"key":"task","value":"after_gate"}]}
{"id":{"uuid":"01a0e819-c5af-7549-87ac-e5c33e561825"},"timestamp":1790600398255000000,"kind":"workflow_completed","execution":{"uuid":"01a0e819-c5a2-7415-94ef-c8e3b3e125f3"},"run":null,"correlation":null,"chain":"bd9ed0a3ab406d8b63253beb3f80e5bca5e15f45d0b12827e4efae50a2ea481b","fields":[{"key":"workflow","value":"gate-keyed"},{"key":"status","value":"succeeded"},{"key":"cause","value":"normal"}]}
"#,
);

/// C7b `T1` on the C6 binary: `2026-09-28T12-59-36Z-8875.ndjson` (source sha256 `cd9147ecc009b287…`).
const T1_COMPLETED: (&str, &str) = (
    "2026-09-28T12-59-36Z-8875.ndjson",
    r#"{"id":{"uuid":"01a0e819-7204-7460-b311-583b344ca63f"},"timestamp":1790600376836000000,"kind":"workflow_started","execution":{"uuid":"01a0e819-7200-72ca-9a62-21cfd5fa8875"},"run":null,"correlation":null,"chain":"7466341540fd02fca9ec21937862176b7821a52495b86d81bb5f30d16c8462dc","fields":[{"key":"workflow","value":"gate-keyed"},{"key":"project_root_fingerprint","value":"4e3fa6930c69d22160a6848d9f54e2a9682204ce9b0bc02888ad067c84b656d3"},{"key":"resumed_from","value":"01a0e81971c47308ae2d839c28959a41"}]}
{"id":{"uuid":"01a0e819-7205-750c-86b7-5656c8f27b09"},"timestamp":1790600376837000000,"kind":"task_scheduled","execution":{"uuid":"01a0e819-7200-72ca-9a62-21cfd5fa8875"},"run":null,"correlation":null,"chain":"4895af5490b5a8ba7dc04112341684f6664b6fafc1f9bb634710982f718b0237","fields":[{"key":"task","value":"before"}]}
{"id":{"uuid":"01a0e819-7205-750c-86b7-56579b8f6e81"},"timestamp":1790600376837000000,"kind":"task_scheduled","execution":{"uuid":"01a0e819-7200-72ca-9a62-21cfd5fa8875"},"run":null,"correlation":null,"chain":"aab7634d050e53528419659dad72ad379d478a64440d05cba89dab1832974151","fields":[{"key":"task","value":"ask"}]}
{"id":{"uuid":"01a0e819-7205-750c-86b7-565865938ce4"},"timestamp":1790600376837000000,"kind":"task_scheduled","execution":{"uuid":"01a0e819-7200-72ca-9a62-21cfd5fa8875"},"run":null,"correlation":null,"chain":"c63c3a80b6503cb61af9a8b75b14949ba7e6602b5e4874c6c88e728a63ab9be3","fields":[{"key":"task","value":"after_gate"}]}
{"id":{"uuid":"01a0e819-7206-76c2-8767-2ad93ce03727"},"timestamp":1790600376838000000,"kind":"task_cache_hit","execution":{"uuid":"01a0e819-7200-72ca-9a62-21cfd5fa8875"},"run":null,"correlation":null,"chain":"be754246ef1c4e24805e94892c838da43e46fed93cc68db332289f17db901599","fields":[{"key":"task","value":"before"}]}
{"id":{"uuid":"01a0e819-720b-7259-bedc-2241595faf34"},"timestamp":1790600376843000000,"kind":"task_started","execution":{"uuid":"01a0e819-7200-72ca-9a62-21cfd5fa8875"},"run":null,"correlation":null,"chain":"3ad4356a02254b5d4ad5f91fa95dfa9f7b080859595017c9e7417750023643dd","fields":[{"key":"task","value":"ask"}]}
{"id":{"uuid":"01a0e819-720b-7259-bedc-224259e069ef"},"timestamp":1790600376843000000,"kind":"permit_checked","execution":{"uuid":"01a0e819-7200-72ca-9a62-21cfd5fa8875"},"run":null,"correlation":null,"chain":"1a5cda7a4b0cfd9e8fc67ef68ee1d8b86854832f85c276fc133e0a135470265a","fields":[{"key":"task","value":"ask"},{"key":"decision","value":"allow"},{"key":"why","value":"pure-internal exemption (NEP-0003 law 1 · any block form)"}]}
{"id":{"uuid":"01a0e819-720b-7259-bedc-2243d5558196"},"timestamp":1790600376843000000,"kind":"approval_decided","execution":{"uuid":"01a0e819-7200-72ca-9a62-21cfd5fa8875"},"run":null,"correlation":null,"chain":"c3c67f6b15a567cab64e95cd79c9a8d5dba19d24860487c4aa709c3b15834562","fields":[{"key":"task","value":"ask"},{"key":"mode","value":"confirm"},{"key":"decision","value":"allow"}]}
{"id":{"uuid":"01a0e819-720c-763c-8571-32be98efc3ba"},"timestamp":1790600376844000000,"kind":"task_completed","execution":{"uuid":"01a0e819-7200-72ca-9a62-21cfd5fa8875"},"run":null,"correlation":null,"chain":"2f87df8fce12797c4f017e6e4620e192f6bcb779294acbd739334099db877a65","fields":[{"key":"task","value":"ask"}]}
{"id":{"uuid":"01a0e819-720e-70f1-a240-9c269d8d0345"},"timestamp":1790600376846000000,"kind":"task_started","execution":{"uuid":"01a0e819-7200-72ca-9a62-21cfd5fa8875"},"run":null,"correlation":null,"chain":"dc9b574b3abacc9fb661ef333b677839f220cc37fb6c054faafa86aa106074ab","fields":[{"key":"task","value":"after_gate"}]}
{"id":{"uuid":"01a0e819-720e-70f1-a240-9c271020a5eb"},"timestamp":1790600376846000000,"kind":"permit_checked","execution":{"uuid":"01a0e819-7200-72ca-9a62-21cfd5fa8875"},"run":null,"correlation":null,"chain":"4a921f6e2b8d2d83b76bf244ff1f1ba02812ba1327e816414cf63240cc0bac0c","fields":[{"key":"task","value":"after_gate"},{"key":"decision","value":"allow"},{"key":"why","value":"permits.tools covers the id"}]}
{"id":{"uuid":"01a0e819-720e-70f1-a240-9c281b304769"},"timestamp":1790600376846000000,"kind":"task_completed","execution":{"uuid":"01a0e819-7200-72ca-9a62-21cfd5fa8875"},"run":null,"correlation":null,"chain":"bb65f888dd032c4bcf41ca772ee3448b966e026249d8a87a8fcb741955975e4b","fields":[{"key":"task","value":"after_gate"}]}
{"id":{"uuid":"01a0e819-720e-70f1-a240-9c29814c598f"},"timestamp":1790600376846000000,"kind":"workflow_completed","execution":{"uuid":"01a0e819-7200-72ca-9a62-21cfd5fa8875"},"run":null,"correlation":null,"chain":"49c5321a8da08db1c3206a71dd636c4c96dd9224de887a8b7c1c9f7bb478ae41","fields":[{"key":"workflow","value":"gate-keyed"},{"key":"status","value":"succeeded"},{"key":"cause","value":"normal"}]}
"#,
);

/// C7b `T1` on the C6 binary: `2026-09-28T12-59-36Z-9a41.ndjson` (source sha256 `ede398c7d96eb4c1…`).
const T1_PAUSED: (&str, &str) = (
    "2026-09-28T12-59-36Z-9a41.ndjson",
    r#"{"id":{"uuid":"01a0e819-71c9-7715-93d4-1573aa3681ab"},"timestamp":1790600376777000000,"kind":"workflow_started","execution":{"uuid":"01a0e819-71c4-7308-ae2d-839c28959a41"},"run":null,"correlation":null,"chain":"7466341540fd02fca9ec21937862176b7821a52495b86d81bb5f30d16c8462dc","fields":[{"key":"workflow","value":"gate-keyed"},{"key":"project_root_fingerprint","value":"4e3fa6930c69d22160a6848d9f54e2a9682204ce9b0bc02888ad067c84b656d3"}]}
{"id":{"uuid":"01a0e819-71ca-7735-9a13-fdc763c04d79"},"timestamp":1790600376778000000,"kind":"task_scheduled","execution":{"uuid":"01a0e819-71c4-7308-ae2d-839c28959a41"},"run":null,"correlation":null,"chain":"e09aae0d994d24d936ed18cdca50c1decab36fc3fb09cfb4e1a318fd7e91f28c","fields":[{"key":"task","value":"before"}]}
{"id":{"uuid":"01a0e819-71ca-7735-9a13-fdc884de3c0c"},"timestamp":1790600376778000000,"kind":"task_scheduled","execution":{"uuid":"01a0e819-71c4-7308-ae2d-839c28959a41"},"run":null,"correlation":null,"chain":"6dde84a55121d4570e233095bd7c9753e58d79ddb3d690318d8d869c98898969","fields":[{"key":"task","value":"ask"}]}
{"id":{"uuid":"01a0e819-71ca-7735-9a13-fdc9b18e9c3c"},"timestamp":1790600376778000000,"kind":"task_scheduled","execution":{"uuid":"01a0e819-71c4-7308-ae2d-839c28959a41"},"run":null,"correlation":null,"chain":"a03af3d59c3b0326bce200b443ce35d4a91cb43c0278a20b6dd5dd9ed18e28d9","fields":[{"key":"task","value":"after_gate"}]}
{"id":{"uuid":"01a0e819-71cd-7227-bcb3-ebc83f76de15"},"timestamp":1790600376781000000,"kind":"task_started","execution":{"uuid":"01a0e819-71c4-7308-ae2d-839c28959a41"},"run":null,"correlation":null,"chain":"f702107b5cc3687dd71afb0be317481dfbecf06bd9ef47ae77cbc29098a763a0","fields":[{"key":"task","value":"before"}]}
{"id":{"uuid":"01a0e819-71cd-7227-bcb3-ebc984c9ef4a"},"timestamp":1790600376781000000,"kind":"permit_checked","execution":{"uuid":"01a0e819-71c4-7308-ae2d-839c28959a41"},"run":null,"correlation":null,"chain":"7d881cb52f88094c03ebedf8318dd4e061412fc5d6a59cd921a457a7c5baf8a2","fields":[{"key":"task","value":"before"},{"key":"decision","value":"allow"},{"key":"why","value":"permits.tools covers the id"}]}
{"id":{"uuid":"01a0e819-71cd-7227-bcb3-ebcaa3ea49c7"},"timestamp":1790600376781000000,"kind":"task_completed","execution":{"uuid":"01a0e819-71c4-7308-ae2d-839c28959a41"},"run":null,"correlation":null,"chain":"db1db29798bd9ebef63043ad9ec51680ca48deaac51618700a1a6850c9dcbf8e","fields":[{"key":"task","value":"before"}]}
{"id":{"uuid":"01a0e819-71ce-7736-b844-f5081003d240"},"timestamp":1790600376782000000,"kind":"workflow_paused","execution":{"uuid":"01a0e819-71c4-7308-ae2d-839c28959a41"},"run":null,"correlation":null,"chain":"4d4f3e8f9c41210104fbce2cba848f83655fe6882e4060994e625391ff2772f2","fields":[{"key":"workflow","value":"gate-keyed"},{"key":"task","value":"ask"},{"key":"mode","value":"confirm"},{"key":"message","value":"Ship it?"},{"key":"status","value":"paused"},{"key":"cause","value":"human_gate"}]}
"#,
);

/// C7b `T1` on the C6 binary: `2026-09-28T13-14-52Z-07d5.ndjson` (source sha256 `8ae8bdc5ff9f59d4…`).
const T1_REPAUSED: (&str, &str) = (
    "2026-09-28T13-14-52Z-07d5.ndjson",
    r#"{"id":{"uuid":"01a0e827-68e9-7332-84b9-7ad28e96943b"},"timestamp":1790601292009000000,"kind":"workflow_started","execution":{"uuid":"01a0e827-68d9-76fe-b63b-5406d9c907d5"},"run":null,"correlation":null,"chain":"7466341540fd02fca9ec21937862176b7821a52495b86d81bb5f30d16c8462dc","fields":[{"key":"workflow","value":"gate-keyed"},{"key":"project_root_fingerprint","value":"4e3fa6930c69d22160a6848d9f54e2a9682204ce9b0bc02888ad067c84b656d3"},{"key":"resumed_from","value":"01a0e81971c47308ae2d839c28959a41"}]}
{"id":{"uuid":"01a0e827-68eb-715f-addc-2145fb4964ce"},"timestamp":1790601292012000000,"kind":"task_scheduled","execution":{"uuid":"01a0e827-68d9-76fe-b63b-5406d9c907d5"},"run":null,"correlation":null,"chain":"23d65f856bca1b4760f364359a3933ecf7ec5abde62d5a2d5d53fda779c01a3f","fields":[{"key":"task","value":"before"}]}
{"id":{"uuid":"01a0e827-68ec-77ad-b6dd-51aa07f035d2"},"timestamp":1790601292012000000,"kind":"task_scheduled","execution":{"uuid":"01a0e827-68d9-76fe-b63b-5406d9c907d5"},"run":null,"correlation":null,"chain":"79af4eadff34fc1b6316854a34e5ed2d0974abd6366dc228bc3f38c0dc0fcddc","fields":[{"key":"task","value":"ask"}]}
{"id":{"uuid":"01a0e827-68ec-77ad-b6dd-51abb0c10772"},"timestamp":1790601292012000000,"kind":"task_scheduled","execution":{"uuid":"01a0e827-68d9-76fe-b63b-5406d9c907d5"},"run":null,"correlation":null,"chain":"7fd3157ed02258df0ba15177bbd76efbb10b542cb621186eb24726cfd8e480df","fields":[{"key":"task","value":"after_gate"}]}
{"id":{"uuid":"01a0e827-68ef-7389-ab4f-59fc1a9335a9"},"timestamp":1790601292015000000,"kind":"task_cache_hit","execution":{"uuid":"01a0e827-68d9-76fe-b63b-5406d9c907d5"},"run":null,"correlation":null,"chain":"fde0c9ea46f94c8e911298e1f9e1fe83bb542041be18ae28d2062830e5cee0d1","fields":[{"key":"task","value":"before"}]}
{"id":{"uuid":"01a0e827-68f1-776c-99bd-5fda1959df06"},"timestamp":1790601292017000000,"kind":"workflow_paused","execution":{"uuid":"01a0e827-68d9-76fe-b63b-5406d9c907d5"},"run":null,"correlation":null,"chain":"a9d7b48d38cbd6e60d8406d99ab8c8cb71cd6e8565bf5647cd06a06a16f1bc6a","fields":[{"key":"workflow","value":"gate-keyed"},{"key":"task","value":"ask"},{"key":"mode","value":"confirm"},{"key":"message","value":"Ship it?"},{"key":"status","value":"paused"},{"key":"cause","value":"human_gate"}]}
"#,
);
