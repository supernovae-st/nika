// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The adapter from the host's record of one rehearsal to the behavioural judge's run. It first
//! establishes that the record shows what the judge needs: every input copied whole and verified
//! in the room, a final state of every path the contract or the candidate names, a room that
//! drained, stayed clean and was removed, and spending the receipts account for. Every observation
//! of one path, under the room's spelling, must be one observation: two copies or two final reads
//! of one file that differ in any byte the record holds contradict each other, whatever text the
//! judge would be shown. A record that cannot show it is an invalid harness, never a completed
//! run. The judge looks a file up under the contract's own spelling, and its `./` convention is
//! the only alias it knows: each receipt is shown under every spelling the contract or the caller
//! gives its file, the room's own only when none names it. The end then comes from typed
//! facts only (the runtime's message is never read), and the judge receives every receipt in the
//! host's order, repeats included: a write is what the room's ledger recorded, never a file that
//! was copied in or already there. Pure: it reads the report and nothing else, and it grants
//! nothing.

use nika_compile_fidelity::behavior::{
    Cause, Consumed, Contract, Coverage, Failure, ReadBack, Run, RunEnd, Usage,
};

use super::observed::{
    CopyReceipt, Digest, FinalReceipt, FinalState, Held, Observation, RecordedCause, Spent,
};
use super::{Attempt, EffectCounts, Rehearsal, RehearsalReport};

/// The behavioural judge's run of `fixture` from `report`. `inputs` are the paths of the
/// fixture's world the host was asked to copy, spelled as the contract names its sources,
/// `targets` the result paths of the request's contract ([`targets_of`]) and `declared` the
/// outputs the candidate declares: once a run began, the host owes a final state of each of
/// them. The judge is shown each copy under the spellings of `inputs` and each final state under
/// those of `targets`.
#[must_use]
pub fn judged_run(
    fixture: &str,
    report: &RehearsalReport,
    inputs: &[String],
    targets: &[String],
    declared: &[String],
) -> Run {
    let usage = usage_of(report);
    match end_of(report, inputs, targets, declared) {
        Ok(Ended::Observed(end)) => {
            let run = Run::new(fixture, end, usage);
            evidence(run, &report.observation, inputs, targets)
        }
        Ok(Ended::Bare(end)) => Run::new(fixture, end, usage),
        Err(reason) => Run::new(fixture, RunEnd::InvalidHarness { reason }, usage),
    }
}

/// The result paths `contract` names, each spelling once and in their order: what the host reads
/// back beside the candidate's declared outputs (each file once), and the names the judge looks
/// them up by. Collect them where a list is needed.
pub fn targets_of(contract: &Contract) -> impl Iterator<Item = String> + use<> {
    let mut paths: Vec<String> = Vec::new();
    let named = contract
        .obligations
        .iter()
        .filter_map(|obligation| obligation.target.as_ref());
    for target in named {
        if !paths.contains(&target.path) {
            paths.push(target.path.clone());
        }
    }
    paths.into_iter()
}

/// How the record ends a run for the judge.
enum Ended {
    /// An end the judge reads with what the run consumed and left.
    Observed(RunEnd),
    /// An end the judge reads alone: nothing ran that it could judge.
    Bare(RunEnd),
}

/// The run's end, or why the record is no valid observation.
fn end_of(
    report: &RehearsalReport,
    inputs: &[String],
    targets: &[String],
    declared: &[String],
) -> Result<Ended, String> {
    let began = !matches!(report.attempt, Attempt::NeverAttempted);
    if began || report.room.prepared {
        room(report, began)?;
    }
    let observation = &report.observation;
    if !began {
        return match report.outcome {
            Rehearsal::NotRun { .. } => Ok(Ended::Bare(RunEnd::NotRun {
                reason: observation.refusal.map_or_else(
                    || "the host stated no typed reason".to_owned(),
                    |refusal| refusal.word().to_owned(),
                ),
            })),
            _ => Err("no run began, yet the host reported the outcome of one".to_owned()),
        };
    }
    if !observation.bounds.stated() {
        return Err("the host stated no bounds for the run it began".to_owned());
    }
    copies(observation, inputs)?;
    finals(observation, &union(targets, declared)?)?;
    spent(observation)?;
    if let Err(reason) = coherent(observation) {
        // A contradiction is shown with its receipts, in their order, none merged.
        return Ok(Ended::Observed(RunEnd::InvalidHarness { reason }));
    }
    if !report.effects.is_none() {
        return Ok(Ended::Bare(RunEnd::NotRun {
            reason: denied(&report.effects),
        }));
    }
    ended(report).map(Ended::Observed)
}

/// The room's own facts: prepared for a run that began, removed, drained and clean.
fn room(report: &RehearsalReport, began: bool) -> Result<(), String> {
    let ledger = &report.observation.ledger;
    let checks = [
        (
            began && !report.room.prepared,
            "a run began in no prepared room",
        ),
        (
            !report.room.cleaned,
            "the removal of the room is not verified",
        ),
        (
            u64::from(report.room.late_refused) != ledger.late_refused,
            "the late refusals of the room and of its ledger disagree",
        ),
        (
            ledger.late_refused > 0,
            "an operation arrived after its phase was sealed",
        ),
        (
            ledger.panicked > 0,
            "an operation of the room panicked or never ran",
        ),
        (
            ledger.leftovers > 0,
            "a temporary name was left in the room",
        ),
        (
            !ledger.drained,
            "an operation was abandoned before its drain completed",
        ),
    ];
    match checks.iter().find(|(failed, _)| *failed) {
        Some((_, why)) => Err(String::from(*why)),
        None => Ok(()),
    }
}

/// Every input of the fixture's world copied, each copy verified in the room and its evidence
/// agreeing with it, within the room's bound; nothing else copied.
fn copies(observation: &Observation, inputs: &[String]) -> Result<(), String> {
    let wanted = inputs
        .iter()
        .map(|input| normal(input).ok_or_else(|| format!("{input} is not a room path")))
        .collect::<Result<Vec<String>, String>>()?;
    for (input, at) in inputs.iter().zip(&wanted) {
        if !observation
            .copies
            .iter()
            .any(|copy| normal(&copy.path).as_ref() == Some(at))
        {
            return Err(format!("no receipt of the copy of {input}"));
        }
    }
    let mut total = 0_u64;
    for copy in &observation.copies {
        let path = &copy.path;
        if !normal(path).is_some_and(|at| wanted.contains(&at)) {
            return Err(format!(
                "the room held {path}, which the fixture does not name"
            ));
        }
        let Some(room) = copy.room.as_ref().filter(|room| **room == copy.source) else {
            return Err(format!(
                "the copy of {path} in the room is unverified or differs from its source"
            ));
        };
        if !holds(&copy.held, room, observation.bounds.preview_bytes) {
            return Err(format!(
                "the evidence of {path} disagrees with its copy in the room"
            ));
        }
        total = total.saturating_add(room.bytes);
    }
    if total > observation.bounds.room_bytes {
        return Err("the copied world passes the room's byte bound".to_owned());
    }
    Ok(())
}

/// Whether `held` is the evidence of bytes of `digest`: the whole text hashing to it, or a
/// shorter preview; within the preview bound either way.
fn holds(held: &Held, digest: &Digest, bound: u64) -> bool {
    match held {
        Held::Whole(text) => byte_len(text) <= bound && Digest::of(text.as_bytes()) == *digest,
        Held::Preview(text) => byte_len(text) <= bound && byte_len(text) < digest.bytes,
        Held::NotText => true,
    }
}

fn byte_len(text: &str) -> u64 {
    u64::try_from(text.len()).unwrap_or(u64::MAX)
}

/// A final state of every path to read back, and every final state one the ledger and the
/// copies explain.
fn finals(observation: &Observation, union: &[String]) -> Result<(), String> {
    for at in union {
        if !observation
            .finals
            .iter()
            .any(|read| normal(&read.path).as_ref() == Some(at))
        {
            return Err(format!("no final state of {at}"));
        }
    }
    for read in &observation.finals {
        explained(observation, read)?;
    }
    Ok(())
}

/// Whether the ledger and the copies explain what the host found at one path: a file the run
/// published or the copy left unchanged, an absence that follows neither a publish nor a copy.
fn explained(observation: &Observation, read: &FinalReceipt) -> Result<(), String> {
    let path = &read.path;
    let at = normal(path).ok_or_else(|| format!("{path} is not a room path"))?;
    let published = observation
        .ledger
        .written
        .iter()
        .any(|written| normal(written).as_ref() == Some(&at));
    let copied = observation
        .copies
        .iter()
        .find(|copy| normal(&copy.path).as_ref() == Some(&at));
    let bound = observation.bounds.preview_bytes;
    match (&read.state, published, copied) {
        (FinalState::Unreadable, _, _) => Err(format!("{path} could not be read back")),
        (FinalState::File { digest, held }, _, _) if !holds(held, digest, bound) => Err(format!(
            "the evidence of {path} disagrees with what the room holds"
        )),
        (FinalState::Absent | FinalState::Directory, true, _) => Err(format!(
            "{path} was published, then removed, and the ledger keeps no removal"
        )),
        (FinalState::Absent | FinalState::Directory, false, Some(_)) => Err(format!(
            "the run removed the copy of {path}, and the ledger keeps no removal"
        )),
        (FinalState::File { .. }, false, None) => Err(format!(
            "{path} holds a file the run did not publish and the host did not copy"
        )),
        (FinalState::File { digest, .. }, false, Some(copy))
            if copy.room.as_ref() != Some(digest) =>
        {
            Err(format!("{path} changed without a publish"))
        }
        _ => Ok(()),
    }
}

/// Whether every observation of one path, under the room's spelling, is one observation: the
/// same source, room bytes and evidence for a copy; the same whole state, digest included, for a
/// final read. The judge is shown text only, so a difference only the digests hold is caught here.
fn coherent(observation: &Observation) -> Result<(), String> {
    for (at, copy) in observation.copies.iter().enumerate() {
        let differs = observation.copies.iter().skip(at + 1).any(|other| {
            normal(&other.path) == normal(&copy.path)
                && (other.source != copy.source
                    || other.room != copy.room
                    || other.held != copy.held)
        });
        if differs {
            return Err(format!("two copies of {} differ", copy.path));
        }
    }
    for (at, read) in observation.finals.iter().enumerate() {
        let differs =
            observation.finals.iter().skip(at + 1).any(|other| {
                normal(&other.path) == normal(&read.path) && other.state != read.state
            });
        if differs {
            return Err(format!("two final states of {} differ", read.path));
        }
    }
    Ok(())
}

/// Whether the bytes the host spent are the bytes its receipts show.
fn spent(observation: &Observation) -> Result<(), String> {
    let copied = observation
        .copies
        .iter()
        .filter_map(|copy| copy.room.as_ref())
        .fold(0_u64, |total, room| total.saturating_add(room.bytes));
    let read = observation
        .finals
        .iter()
        .fold(0_u64, |total, read| match &read.state {
            FinalState::File { digest, .. } => total.saturating_add(digest.bytes),
            FinalState::Absent | FinalState::Directory | FinalState::Unreadable => total,
        });
    if observation.spent == Spent::new(copied, read) {
        Ok(())
    } else {
        Err("the bytes the host spent disagree with its receipts".to_owned())
    }
}

/// The paths to read back once a run began: the contract's targets and the declared outputs,
/// once each, as the room spells them.
fn union(targets: &[String], declared: &[String]) -> Result<Vec<String>, String> {
    let mut paths: Vec<String> = Vec::new();
    for path in targets.iter().chain(declared) {
        let at = normal(path).ok_or_else(|| format!("{path} is not a room path"))?;
        if !paths.contains(&at) {
            paths.push(at);
        }
    }
    Ok(paths)
}

/// The room-relative spelling of `path` under the room's own law: `.` and empty components
/// dropped; an absolute path, a `..` or no name at all is no room path.
fn normal(path: &str) -> Option<String> {
    if path.starts_with('/') {
        return None;
    }
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => return None,
            name => parts.push(name),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// The end the record states, from typed facts only.
fn ended(report: &RehearsalReport) -> Result<RunEnd, String> {
    let record = report.observation.failure.as_ref();
    match (&report.attempt, &report.outcome, record) {
        (Attempt::Completed { .. }, Rehearsal::Passed { .. } | Rehearsal::Missing { .. }, None) => {
            Ok(RunEnd::Completed)
        }
        (Attempt::Completed { .. }, Rehearsal::Failed { code, task, .. }, None) => {
            Ok(RunEnd::Failed(Failure::new(
                task.clone(),
                code.clone(),
                Cause::Unclassified,
            )))
        }
        (Attempt::Completed { .. }, Rehearsal::Failed { code, task, .. }, Some(record))
            if record.task == *task && record.code == *code =>
        {
            let cause = cause_of(record.cause);
            Ok(RunEnd::Failed(Failure::new(
                task.clone(),
                code.clone(),
                cause,
            )))
        }
        (Attempt::Stopped { .. }, Rehearsal::NotRun { .. }, None) => {
            Ok(RunEnd::Failed(Failure::new("", "", Cause::TimeBound)))
        }
        _ => Err("the run's outcome and its records contradict how it ended".to_owned()),
    }
}

/// The judge's cause of a recorded failure: an engine failure, or a task failure no structured
/// fact ties to a stated stop.
fn cause_of(cause: RecordedCause) -> Cause {
    match cause {
        RecordedCause::Engine => Cause::Engine,
        RecordedCause::VerbError | RecordedCause::Timeout | RecordedCause::RetryExhausted => {
            Cause::Unclassified
        }
    }
}

/// The effects the denied seams saw attempted, counted.
fn denied(effects: &EffectCounts) -> String {
    let counted = [
        ("network", effects.network),
        ("provider", effects.provider),
        ("spawn", effects.spawn),
        ("prompt", effects.prompt),
        ("secret", effects.secret),
        ("child", effects.child),
    ];
    let attempted: Vec<String> = counted
        .iter()
        .filter(|(_, count)| *count > 0)
        .map(|(effect, count)| format!("{effect} {count}"))
        .collect();
    format!("a denied effect was attempted: {}", attempted.join(", "))
}

/// What the fixture spent: one fixture, the attempt made, the bytes copied in and read back, and
/// the attempt's elapsed time.
fn usage_of(report: &RehearsalReport) -> Usage {
    let (attempts, elapsed_ms) = match report.attempt {
        Attempt::NeverAttempted => (0, 0),
        Attempt::Completed { elapsed_ms } | Attempt::Stopped { elapsed_ms } => (1, elapsed_ms),
    };
    let spent = report.observation.spent;
    Usage::new(
        1,
        attempts,
        spent.copied_bytes,
        spent.read_back_bytes,
        elapsed_ms,
    )
}

/// The run with every receipt as evidence, in the host's order and with repeats: each copy under
/// every spelling `inputs` gives its file, each final state under every one `targets` gives.
fn evidence(run: Run, observation: &Observation, inputs: &[String], targets: &[String]) -> Run {
    let run = observation
        .copies
        .iter()
        .flat_map(|copy| {
            named(&copy.path, inputs)
                .into_iter()
                .map(move |path| consumed(copy, path))
        })
        .fold(run, Run::with_consumed);
    let written = &observation.ledger.written;
    observation
        .finals
        .iter()
        .flat_map(|read| {
            named(&read.path, targets)
                .into_iter()
                .map(move |path| read_back(read, path, written))
        })
        .fold(run, Run::with_read_back)
}

/// The names the judge is shown for the file at `path`: each distinct spelling of it in `names`,
/// in their order, or the room's own spelling when none names it.
fn named(path: &str, names: &[String]) -> Vec<String> {
    let at = normal(path);
    let mut shown: Vec<String> = Vec::new();
    for name in names {
        if at.is_some() && normal(name) == at && !shown.contains(name) {
            shown.push(name.clone());
        }
    }
    if shown.is_empty() {
        shown.push(at.unwrap_or_else(|| path.to_owned()));
    }
    shown
}

/// What the run consumed from one copy, shown at `path`: the text the evidence keeps, and how
/// much of it.
fn consumed(copy: &CopyReceipt, path: String) -> Consumed {
    let coverage = match copy.held {
        Held::Whole(_) => Coverage::Complete,
        Held::Preview(_) => Coverage::Truncated,
        Held::NotText => Coverage::Sampled,
    };
    Consumed::new(path, copy.held.text(), coverage)
}

/// The final state of one file, shown at `path`: a write only where the room's ledger recorded
/// the run's publish.
fn read_back(read: &FinalReceipt, path: String, written: &[String]) -> ReadBack {
    match &read.state {
        FinalState::File { held, .. } => {
            let at = normal(&read.path);
            ReadBack::new(path, held.text())
                .with_written(written.iter().any(|published| normal(published) == at))
                .with_truncated(!matches!(held, Held::Whole(_)))
        }
        FinalState::Absent | FinalState::Directory | FinalState::Unreadable => {
            ReadBack::unwritten(path)
        }
    }
}
