// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The rehearsals of one closed copy.
//!
//! - **Order.** The candidates are taken in order, each over the frozen worlds one at a time.
//! - **Budget.** Every host call is admitted by the budget before it starts, and a world the
//!   budget refuses is an explicit run that did not happen.
//! - **Binding.** Every report is bound to the candidate's exact bytes, and a discriminating
//!   world to the bytes this door wrote for it, before the judge reads it.
//! - **Judging.** A candidate is judged alone by the existing selection, over its whole list of
//!   worlds only, with the turn the previous candidate left.
//! - **Stop.** The first one certified, or a spent turn, ends the rehearsals.
//!
//! The scratch account that admits each call is never reported: the judge charges every run once,
//! and its turn is the one carried on.

use std::path::Path;

use nika_compile_cognition::rehearse::{
    Attempt, Digest, FinalState, Held, RehearsalReport, judged_run, targets_of,
};
use nika_compile_fidelity::behavior::{
    Admission, Axis, Budget, Candidate, Choice, Outcome, Ruling, Run, RunEnd, Usage, Verdict,
    select,
};

use super::witness::{Seen, Witness};
use super::worlds::{self, World};
use super::{Allowance, Built, Closed, EXCERPT, Host, Preview, lowering_word, same};

/// Why a world the budget refused never ran.
const SPENT: &str = "the rehearsal budget admitted no further world";

/// What the rehearsals of one closed copy work with.
#[derive(Clone, Copy)]
pub(super) struct Cx<'a> {
    pub(super) closed: &'a Closed,
    pub(super) built: &'a [Built],
    pub(super) root: &'a Path,
    pub(super) host: &'a Host<'a>,
    pub(super) scratch: &'a Path,
    pub(super) allowance: Allowance,
}

/// The candidate the rehearsals selected, and what it was selected on.
pub(super) struct Selected {
    pub(super) index: usize,
    pub(super) preview: Preview,
    pub(super) witness: Witness,
    pub(super) turn: Usage,
    pub(super) verdicts: Vec<String>,
}

/// Rehearse and judge the candidates in order, and select the first one certified; why none was,
/// with what the turn spent, otherwise.
pub(super) async fn rehearse(cx: Cx<'_>) -> Result<Selected, (String, Usage)> {
    let worlds = worlds::frozen(cx.closed, cx.root)
        .await
        .map_err(|why| (why, cx.allowance.before))?;
    let names: Vec<&str> = worlds.iter().map(|world| world.name).collect();
    let before = worlds
        .iter()
        .find(|world| world.observed())
        .and_then(|world| world.before.clone());
    let targets: Vec<String> = targets_of(&cx.closed.contract).collect();
    let mut turn = cx.allowance.before;
    let mut verdicts = Vec::with_capacity(cx.built.len());
    for (index, candidate) in cx.built.iter().enumerate() {
        let (runs, observed) = runs_of(&cx, candidate, &worlds, &targets, turn).await;
        if !covered(&runs, &names) {
            // Never judged on less than its whole list: the runs it made are still spent.
            turn = charged(&cx.allowance, turn, &runs);
            let word = lowering_word(candidate.lowering);
            verdicts.push(format!(
                "the {word} copy: its runs do not match the frozen worlds"
            ));
            continue;
        }
        let ruling = select(
            &cx.closed.contract,
            &[Candidate::new(candidate.sha256.clone(), runs)],
            cx.allowance.round,
            cx.allowance.turn,
            turn,
        );
        turn = ruling.turn;
        verdicts.push(verdict(candidate, &ruling));
        match ruling.choice {
            Choice::Selected(_) => {
                return selected(cx.closed, observed, before.as_ref(), &names)
                    .map(|(preview, witness)| Selected {
                        index,
                        preview,
                        witness,
                        turn,
                        verdicts,
                    })
                    .map_err(|why| (why, turn));
            }
            Choice::Spent(axis) => {
                let why = format!(
                    "the rehearsal budget is spent ({}) · {}",
                    axis_word(axis),
                    verdicts.join(" · ")
                );
                return Err((why, turn));
            }
            _ => {}
        }
    }
    Err((
        format!("no copy qualified · {}", verdicts.join(" · ")),
        turn,
    ))
}

/// One candidate's runs over the worlds, in order, each admitted by a scratch account of the
/// round before its host call, and the report of the user's own world.
async fn runs_of(
    cx: &Cx<'_>,
    candidate: &Built,
    worlds: &[World],
    targets: &[String],
    before: Usage,
) -> (Vec<Run>, Option<RehearsalReport>) {
    let mut account = Budget::new(cx.allowance.round, cx.allowance.turn, before);
    let mut runs = Vec::with_capacity(worlds.len());
    let mut observed = None;
    for world in worlds {
        if account.admission() != Admission::Open {
            let end = RunEnd::NotRun {
                reason: SPENT.to_owned(),
            };
            runs.push(Run::new(world.name, end, Usage::default()));
            continue;
        }
        let run = match one(cx, candidate, world, targets).await {
            Ok((run, report)) => {
                if world.observed() {
                    observed = Some(report);
                }
                run
            }
            Err(reason) => Run::new(
                world.name,
                RunEnd::InvalidHarness { reason },
                Usage::default(),
            ),
        };
        let _ = account.charge(&run.usage);
        runs.push(run);
    }
    (runs, observed)
}

/// One host call: the world's root made, the candidate rehearsed there, the root removed, and the
/// judge's run of the report, an invalid harness when the report is not bound to this candidate
/// and this world; why, when the world could not be made.
async fn one(
    cx: &Cx<'_>,
    candidate: &Built,
    world: &World,
    targets: &[String],
) -> Result<(Run, RehearsalReport), String> {
    let made = world.make(cx.scratch)?;
    let room = (cx.host)(made.root());
    let report = room
        .rehearse_reading(&candidate.bytes, &world.inputs, targets)
        .await;
    let removed = made.remove();
    let mut run = judged_run(world.name, &report, &world.inputs, targets, &[]);
    let refused = if removed {
        bound(&report, candidate, world).err()
    } else {
        Some(format!(
            "the fixture root of {} was not removed",
            world.name
        ))
    };
    if let Some(reason) = refused {
        run.end = RunEnd::InvalidHarness { reason };
    }
    Ok((run, report))
}

/// Whether `report` names exactly the candidate's bytes, an admitted world for a run that began,
/// and, for a discriminating world, a copy of exactly the bytes this door wrote for it.
fn bound(report: &RehearsalReport, candidate: &Built, world: &World) -> Result<(), String> {
    if report.candidate_sha256 != candidate.sha256 {
        return Err("the report names other bytes than the candidate's".to_owned());
    }
    if !matches!(report.attempt, Attempt::NeverAttempted) && report.admitted_digest.is_empty() {
        return Err("a run began with no admitted digest".to_owned());
    }
    for (path, bytes) in world.files() {
        let wrote = Digest::of(bytes);
        let other = report
            .observation
            .copies
            .iter()
            .any(|copy| same(&copy.path, path) && copy.source != wrote);
        if other {
            return Err(format!(
                "the room copied other bytes than the world's {path}"
            ));
        }
    }
    Ok(())
}

/// Whether the runs are exactly the frozen worlds, in order, each named once.
pub(super) fn covered(runs: &[Run], names: &[&str]) -> bool {
    let unique = names
        .iter()
        .enumerate()
        .all(|(at, name)| !names.iter().take(at).any(|earlier| earlier == name));
    unique
        && runs.len() == names.len()
        && runs
            .iter()
            .zip(names)
            .all(|(run, name)| run.fixture == *name)
}

/// The turn after `runs`, charged as the judge charges a round, for a candidate never judged.
fn charged(allowance: &Allowance, before: Usage, runs: &[Run]) -> Usage {
    let mut account = Budget::new(allowance.round, allowance.turn, before);
    for run in runs {
        let _ = account.charge(&run.usage);
    }
    account.turn()
}

/// What the user's own world showed of the selected candidate, and the witness of that world:
/// the source the room copied, the target as it was before the run (as the frozen list observed
/// it, and as the room copied it), and the result read back whole.
fn selected(
    closed: &Closed,
    observed: Option<RehearsalReport>,
    frozen: Option<&Seen>,
    names: &[&str],
) -> Result<(Preview, Witness), String> {
    let report = observed.ok_or("the user's world left no report")?;
    let copies = &report.observation.copies;
    let source = copies
        .iter()
        .find(|copy| same(&copy.path, &closed.source))
        .map(|copy| copy.source.clone())
        .ok_or("the user's source has no copy receipt")?;
    let before = copies
        .iter()
        .find(|copy| same(&copy.path, &closed.target))
        .map_or(Seen::Absent, |copy| Seen::File(copy.source.clone()));
    if frozen != Some(&before) {
        return Err("the target changed while the user's world was rehearsed".to_owned());
    }
    let read = report
        .observation
        .finals
        .iter()
        .find(|read| same(&read.path, &closed.target))
        .ok_or("the user's world has no final state of the target")?;
    let (digest, text) = match &read.state {
        FinalState::File {
            digest,
            held: Held::Whole(text),
        } => (digest.clone(), text.clone()),
        _ => return Err("the result is not kept whole within the preview bound".to_owned()),
    };
    let published = report
        .observation
        .ledger
        .written
        .iter()
        .any(|written| same(written, &closed.target));
    let replaced = match &before {
        Seen::File(digest) => Some(digest.bytes),
        Seen::Absent => None,
    };
    let preview = Preview {
        target: closed.target.clone(),
        published,
        bytes: digest.bytes,
        sha256: digest.sha256,
        excerpt: excerpt(&text),
        source: closed.source.clone(),
        source_bytes: source.bytes,
        source_sha256: source.sha256.clone(),
        replaced,
        worlds: names.iter().map(|name| (*name).to_owned()).collect(),
    };
    let world = vec![
        (closed.source.clone(), Seen::File(source)),
        (closed.target.clone(), before),
    ];
    Ok((
        preview,
        Witness::new(report.candidate_sha256.clone(), world),
    ))
}

/// One judged candidate's verdict, in words, the worlds it failed on named.
fn verdict(candidate: &Built, ruling: &Ruling) -> String {
    let word = lowering_word(candidate.lowering);
    let Some(report) = ruling.reports.last() else {
        return format!("the {word} copy: not judged, the turn's budget is spent");
    };
    let mut failed: Vec<&str> = Vec::new();
    for finding in report.judged.iter().flat_map(|judged| &judged.findings) {
        if finding.outcome == Outcome::Failed && !failed.contains(&finding.fixture.as_str()) {
            failed.push(&finding.fixture);
        }
    }
    match report.verdict() {
        Verdict::Certified => format!("the {word} copy: certified on every world"),
        Verdict::Defective => {
            format!("the {word} copy: the wrong text on {}", failed.join(", "))
        }
        Verdict::NotRun => format!("the {word} copy: a world did not run"),
        Verdict::Incomplete => format!("the {word} copy: a case stays open"),
        Verdict::InvalidHarness => format!("the {word} copy: an observation was invalid"),
        _ => format!("the {word} copy: not certified"),
    }
}

/// A budget axis, in words.
fn axis_word(axis: Axis) -> &'static str {
    match axis {
        Axis::Fixtures => "worlds",
        Axis::Attempts => "attempts",
        Axis::Bytes => "bytes",
        Axis::Time => "time",
        _ => "an axis",
    }
}

/// The start of `text`, at most [`EXCERPT`] characters, each escaped as Rust's debug form does.
fn excerpt(text: &str) -> String {
    let mut shown: String = text
        .chars()
        .take(EXCERPT)
        .flat_map(char::escape_debug)
        .collect();
    if text.chars().count() > EXCERPT {
        shown.push('…');
    }
    shown
}
