// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The copy door's protocol over a double of the existing rehearsal port.
//!
//! **The double.** It reads the files of each world the door hands it: the user's project, or a
//! fixture root the door made. It answers what a room would observe there, as each test scripts
//! it, and records every call with the most calls active at once.
//!
//! **What is proven.** No real run is claimed: the room's own suites prove the room. These prove
//! the door's order, budget, binding, coverage and selection. The expected usages come from the
//! receipt table below, never from the door.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nika_compile::CompileRequest;
use nika_compile::surface::sha256;
use nika_compile_cognition::rehearse::{
    Attempt, Bounds, CopyReceipt, Digest, EffectCounts, FinalReceipt, FinalState, Held,
    LedgerFacts, Observation, Rehearsal, RehearsalFuture, RehearsalReport, Rehearse, RoomEvidence,
    Spent,
};
use nika_compile_fidelity::behavior::{Run, RunEnd};

use super::witness::relative;
use super::{Allowance, CopyLowering, Limits, Qualification, Seen, Usage, qualify, read, rounds};
use crate::compile::room::ObservedRoom;

const INTENT: &str = "Copy ./in/source.txt as is to ./out/copied.txt";
const SOURCE: &str = "./in/source.txt";
const TARGET: &str = "./out/copied.txt";
/// The user's own text, distinct from every discriminating world's.
const USER: &str = "the user notes, line one\n";
/// What each call reports as its attempt's elapsed time, by call order.
const ELAPSED: [u64; 6] = [7, 11, 13, 17, 19, 23];
/// Limits no test reaches.
const WIDE: Limits = Limits::new(100, 100, 1 << 30, 1 << 30);

/// One call the double received: the candidate's sha256, the source text it read, its inputs and
/// targets.
#[derive(Clone, Debug)]
struct Call {
    candidate: String,
    source: String,
    inputs: Vec<String>,
    targets: Vec<String>,
}

/// What the double answers.
#[derive(Default)]
struct Script {
    /// The lowering whose run turns CRLF into LF: a candidate wrong on the CRLF world.
    rewrites: Option<CopyLowering>,
    /// On the call of this order, the report names these bytes instead of the candidate's.
    other: Option<(usize, String)>,
}

/// The double's record, shared by every room it builds.
#[derive(Default)]
struct Shared {
    script: Script,
    calls: Mutex<Vec<Call>>,
    active: AtomicUsize,
    most: AtomicUsize,
}

/// A room over one world's root, answering as its script says.
struct Double {
    root: PathBuf,
    shared: Arc<Shared>,
}

impl Rehearse for Double {
    fn rehearse<'a>(&'a self, candidate: &'a str, inputs: &'a [String]) -> RehearsalFuture<'a> {
        self.rehearse_reading(candidate, inputs, &[])
    }

    fn bound(&self) -> Duration {
        ObservedRoom::BOUND
    }

    fn rehearse_reading<'a>(
        &'a self,
        candidate: &'a str,
        inputs: &'a [String],
        targets: &'a [String],
    ) -> RehearsalFuture<'a> {
        Box::pin(async move {
            let now = self.shared.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.shared.most.fetch_max(now, Ordering::SeqCst);
            let report = self.answer(candidate, inputs, targets);
            self.shared.active.fetch_sub(1, Ordering::SeqCst);
            report
        })
    }
}

impl Double {
    /// What a room would observe of `candidate` over this world, as the script says.
    fn answer(&self, candidate: &str, inputs: &[String], targets: &[String]) -> RehearsalReport {
        let read = |path: &str| {
            std::fs::read(self.root.join(relative(path).expect("a room path")))
                .expect("the door made every input")
        };
        let source = String::from_utf8(read(SOURCE)).expect("text");
        let order = {
            let mut calls = self.shared.calls.lock().expect("calls");
            calls.push(Call {
                candidate: sha256(candidate),
                source: source.clone(),
                inputs: inputs.to_vec(),
                targets: targets.to_vec(),
            });
            calls.len() - 1
        };
        let lowering = if candidate.contains("binary: true") {
            CopyLowering::Bytes
        } else {
            CopyLowering::Text
        };
        let result = if self.shared.script.rewrites == Some(lowering) {
            source.replace("\r\n", "\n")
        } else {
            source
        };
        let mut observation = Observation::none();
        for input in inputs {
            let bytes = read(input);
            let digest = Digest::of(&bytes);
            let held = Held::of(&bytes, ObservedRoom::PREVIEW_BOUND);
            observation.copies.push(CopyReceipt::new(
                input.clone(),
                digest.clone(),
                Some(digest),
                held,
            ));
        }
        for target in targets {
            let state = FinalState::File {
                digest: Digest::of(result.as_bytes()),
                held: Held::of(result.as_bytes(), ObservedRoom::PREVIEW_BOUND),
            };
            observation
                .finals
                .push(FinalReceipt::new(target.clone(), state));
        }
        observation.ledger = LedgerFacts::clean(vec!["out/copied.txt".to_owned()]);
        let copied = observation
            .copies
            .iter()
            .map(|copy| copy.source.bytes)
            .sum();
        observation.spent = Spent::new(copied, result.len() as u64);
        observation.bounds = Bounds::new(
            10_000,
            ObservedRoom::COPY_BOUND,
            ObservedRoom::PREVIEW_BOUND,
        );
        let named = match &self.shared.script.other {
            Some((at, other)) if *at == order => other.clone(),
            _ => sha256(candidate),
        };
        let elapsed_ms = ELAPSED.get(order).copied().unwrap_or(1);
        RehearsalReport::new(
            Rehearsal::Passed {
                outputs: Vec::new(),
            },
            Attempt::Completed { elapsed_ms },
            EffectCounts::none(),
            named.clone(),
        )
        .with_room(RoomEvidence::new(true, true))
        .with_admitted_digest(format!("admitted-{named}"))
        .with_observation(observation)
    }
}

/// One scenario: the user's project, a scratch parent for the fixture roots, and the double.
struct Scene {
    project: tempfile::TempDir,
    scratch: tempfile::TempDir,
    shared: Arc<Shared>,
}

impl Scene {
    fn new(script: Script) -> Self {
        let project = tempfile::tempdir().expect("project");
        std::fs::create_dir_all(project.path().join("in")).expect("in");
        std::fs::write(project.path().join("in/source.txt"), USER).expect("source");
        Self {
            project,
            scratch: tempfile::tempdir().expect("scratch"),
            shared: Arc::new(Shared {
                script,
                ..Shared::default()
            }),
        }
    }

    /// The door over this scene, under `allowance`.
    fn qualify(&self, allowance: Allowance) -> Qualification {
        let shared = Arc::clone(&self.shared);
        let host = move |root: &Path| -> Box<dyn Rehearse> {
            Box::new(Double {
                root: root.to_path_buf(),
                shared: Arc::clone(&shared),
            })
        };
        qualify(
            &CompileRequest::create(INTENT),
            INTENT,
            self.project.path(),
            &host,
            self.scratch.path(),
            allowance,
        )
    }

    fn calls(&self) -> Vec<Call> {
        self.shared.calls.lock().expect("calls").clone()
    }

    /// The fixture roots are gone, and the user's world is as it was.
    fn left_as_it_was(&self) {
        let left: Vec<_> = std::fs::read_dir(self.scratch.path())
            .expect("scratch")
            .collect();
        assert!(left.is_empty(), "every fixture root removed: {left:?}");
        let source = std::fs::read_to_string(self.project.path().join("in/source.txt"));
        assert_eq!(source.expect("source"), USER);
        assert!(
            !self.project.path().join("out").exists(),
            "no target written"
        );
    }
}

/// The two candidates' sha256, the text copy's first.
fn shas() -> (String, String) {
    let Some(Ok((_, built))) = read(&CompileRequest::create(INTENT), INTENT) else {
        panic!("harness: the closed copy must give its two candidates");
    };
    let [text, bytes] = built.as_slice() else {
        panic!("harness: two candidates");
    };
    assert_eq!(
        (text.lowering, bytes.lowering),
        (CopyLowering::Text, CopyLowering::Bytes)
    );
    (text.sha256.clone(), bytes.sha256.clone())
}

/// The usage one call reports, from the receipt table: `copied` bytes in, `read_back` bytes back
/// and the call's elapsed time.
fn usage(order: usize, copied: usize, read_back: usize) -> Usage {
    Usage::new(1, 1, copied as u64, read_back as u64, ELAPSED[order])
}

/// The receipts of the three worlds of one candidate, by call order: the user's world (its
/// source, the target absent), the CRLF world (its source and the six bytes of `alpha\n`), the
/// empty world (the five bytes of `stale`). `crlf` is the length of the CRLF world's source and
/// `crlf_back` what the candidate wrote there.
fn round(first: usize, crlf: usize, crlf_back: usize) -> [Usage; 3] {
    let user = USER.len();
    [
        usage(first, user, user),
        usage(first + 1, crlf + 6, crlf_back),
        usage(first + 2, 5, 0),
    ]
}

/// The length of the CRLF world's source, as the double read it.
fn crlf_len(calls: &[Call]) -> usize {
    calls
        .iter()
        .find(|call| call.source.contains("\r\n"))
        .map(|call| call.source.len())
        .expect("a CRLF world was rehearsed")
}

fn total(before: Usage, usages: &[Usage]) -> Usage {
    usages.iter().fold(before, |sum, one| sum.plus(one))
}

/// The text copy rewrites the CRLF world's line ends; the byte copy keeps them. The byte
/// copy is selected, its own outcome and the user's own world give the preview, and both
/// candidates ran one call at a time, the text copy first. When the text copy holds everywhere,
/// the byte copy is never rehearsed.
#[test]
fn a_text_candidate_that_rewrites_crlf_yields_to_the_byte_candidate() {
    let (text, bytes) = shas();
    let scene = Scene::new(Script {
        rewrites: Some(CopyLowering::Text),
        ..Script::default()
    });
    let Qualification::Qualified(q) = scene.qualify(Allowance::new(WIDE, WIDE, Usage::default()))
    else {
        panic!("the byte copy qualifies");
    };
    assert_eq!(q.lowering, CopyLowering::Bytes);
    let selected = q.outcome.candidate.as_deref().expect("a candidate");
    assert_eq!(sha256(selected), bytes);
    assert_eq!(q.witness.candidate_sha256(), bytes);
    let calls = scene.calls();
    let order: Vec<&str> = calls.iter().map(|call| call.candidate.as_str()).collect();
    let (a, b) = (text.as_str(), bytes.as_str());
    assert_eq!(order, [a, a, a, b, b, b]);
    assert_eq!(
        scene.shared.most.load(Ordering::SeqCst),
        1,
        "one call at a time"
    );
    // The user's world first: its source alone (no target there), then the two fixture worlds,
    // each with the target the door wrote; the target is read back every time.
    assert_eq!(calls[0].source, USER);
    assert_eq!(calls[0].inputs, [SOURCE]);
    assert!(calls[1].source.contains("\r\n") && calls[2].source.is_empty());
    for call in &calls[1..3] {
        assert_eq!(call.inputs, [SOURCE, TARGET]);
    }
    assert!(calls.iter().all(|call| call.targets == [TARGET]));
    let p = &q.preview;
    assert_eq!((p.target.as_str(), p.source.as_str()), (TARGET, SOURCE));
    assert!(p.published && p.replaced.is_none());
    assert_eq!(
        (p.bytes, p.sha256.as_str()),
        (USER.len() as u64, sha256(USER).as_str())
    );
    assert_eq!(p.excerpt, r"the user notes, line one\n");
    assert_eq!(
        p.worlds,
        ["observed", "crlf-unicode-template", "empty-over-stale"]
    );
    assert_eq!(
        q.witness.world(),
        [
            (SOURCE.to_owned(), Seen::File(Digest::of(USER.as_bytes()))),
            (TARGET.to_owned(), Seen::Absent),
        ]
    );
    assert!(q.verdicts[0].contains("the wrong text on crlf-unicode-template"));
    scene.left_as_it_was();

    let scene = Scene::new(Script::default());
    let Qualification::Qualified(q) = scene.qualify(Allowance::new(WIDE, WIDE, Usage::default()))
    else {
        panic!("the text copy qualifies");
    };
    assert_eq!(q.lowering, CopyLowering::Text);
    let called: Vec<String> = scene
        .calls()
        .into_iter()
        .map(|call| call.candidate)
        .collect();
    assert_eq!(
        called,
        [text.clone(), text.clone(), text],
        "no byte copy rehearsed"
    );
    scene.left_as_it_was();
}

/// A candidate is never certified on less than its whole list, nor on a report that names
/// other bytes; and the coverage guard refuses a missing, repeated or extra world.
#[test]
fn a_fixture_missing_renamed_or_bound_to_other_bytes_never_certifies() {
    let (_, bytes) = shas();
    // The last world never runs (a round of two worlds), after two passing ones.
    let scene = Scene::new(Script::default());
    let two = Limits::new(2, 100, 1 << 30, 1 << 30);
    let Qualification::Refused { why, .. } =
        scene.qualify(Allowance::new(two, WIDE, Usage::default()))
    else {
        panic!("a prefix never qualifies");
    };
    assert!(why.contains("the text copy: a world did not run"), "{why}");
    assert_eq!(
        scene.calls().len(),
        4,
        "two worlds per candidate, none more"
    );
    scene.left_as_it_was();

    // The text copy's CRLF report names the byte copy's sha256: an invalid harness, never a pass.
    let scene = Scene::new(Script {
        other: Some((1, bytes)),
        ..Script::default()
    });
    let Qualification::Qualified(q) = scene.qualify(Allowance::new(WIDE, WIDE, Usage::default()))
    else {
        panic!("the byte copy qualifies on its own reports");
    };
    assert_eq!(q.lowering, CopyLowering::Bytes);
    assert!(
        q.verdicts[0].contains("an observation was invalid"),
        "{:?}",
        q.verdicts
    );
    scene.left_as_it_was();

    // The coverage guard, over runs named as a host could name them.
    let names = ["observed", "crlf-unicode-template", "empty-over-stale"];
    let runs = |fixtures: &[&str]| -> Vec<Run> {
        fixtures
            .iter()
            .map(|name| Run::new(*name, RunEnd::Completed, Usage::default()))
            .collect()
    };
    assert!(rounds::covered(&runs(&names), &names));
    for wrong in [
        vec!["observed", "observed", "empty-over-stale"],
        vec!["observed", "crlf-unicode-template"],
        vec![
            "observed",
            "crlf-unicode-template",
            "empty-over-stale",
            "observed",
        ],
        vec!["crlf-unicode-template", "observed", "empty-over-stale"],
    ] {
        assert!(!rounds::covered(&runs(&wrong), &names), "{wrong:?}");
    }
    assert!(!rounds::covered(
        &runs(&["observed", "observed"]),
        &["observed", "observed"]
    ));
}

/// The turn is what it already spent plus every run made, the failed candidate's included,
/// charged once. A spent turn calls no host, and a green prefix is never selected.
#[test]
fn the_turn_usage_is_the_runs_made_and_a_spent_budget_calls_no_host() {
    let before = Usage::new(2, 2, 100, 50, 40);
    let rewrites = || Script {
        rewrites: Some(CopyLowering::Text),
        ..Script::default()
    };
    // Room enough: the text copy fails on the CRLF world, the byte copy is selected.
    let scene = Scene::new(rewrites());
    let Qualification::Qualified(q) = scene.qualify(Allowance::new(WIDE, WIDE, before)) else {
        panic!("the byte copy qualifies");
    };
    let crlf = crlf_len(&scene.calls());
    let text = round(0, crlf, crlf - 1);
    let byte = round(3, crlf, crlf);
    assert_eq!(q.turn, total(before, &[text, byte].concat()));
    scene.left_as_it_was();

    // The turn's worlds run out exactly after the text copy: the byte copy is never called.
    let scene = Scene::new(rewrites());
    let turn = Limits::new(before.fixtures + 3, 100, 1 << 30, 1 << 30);
    let Qualification::Refused { why, turn: spent } =
        scene.qualify(Allowance::new(WIDE, turn, before))
    else {
        panic!("a spent turn refuses");
    };
    assert!(
        why.contains("the rehearsal budget is spent (worlds)"),
        "{why}"
    );
    assert_eq!(
        scene.calls().len(),
        3,
        "no host call once the turn is spent"
    );
    let crlf = crlf_len(&scene.calls());
    assert_eq!(spent, total(before, &round(0, crlf, crlf - 1)));
    scene.left_as_it_was();

    // The turn runs out after two green worlds of the byte copy: that prefix is never selected.
    let scene = Scene::new(rewrites());
    let turn = Limits::new(before.fixtures + 5, 100, 1 << 30, 1 << 30);
    let Qualification::Refused { why, turn: spent } =
        scene.qualify(Allowance::new(WIDE, turn, before))
    else {
        panic!("a prefix never qualifies");
    };
    assert!(why.contains("the byte copy: a world did not run"), "{why}");
    assert_eq!(scene.calls().len(), 5);
    let crlf = crlf_len(&scene.calls());
    let byte = round(3, crlf, crlf);
    let made = [round(0, crlf, crlf - 1).to_vec(), byte[..2].to_vec()].concat();
    assert_eq!(spent, total(before, &made));
    scene.left_as_it_was();
}
