// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The judgment of a contract over the fixtures of one round. What each fixture can show is
//! established first: a contradictory observation makes the whole fixture an invalid harness,
//! and a source missing, malformed or outside the stated domain, or a result with no read-back,
//! makes that obligation one, whatever the run did. The end of the run is then classified once
//! against the end the contract predicts on the records it consumed — a completed run, a
//! failure an established cause or the contract explains as a defect, or a failure the
//! evidence does not settle — and every obligation is read through that class. An established
//! cause (an engine failure, a file the request never names, a well-formed source the run could
//! not parse) is a defect even where a stated rule predicts a stop. A stop is never certified:
//! a structured stop consistent with the one a stated rule predicts names its operation, not
//! the data and policy that governed it, so another occurrence of that operation on the same
//! records would report the same fact; it stays unattested. A defect fails every obligation (an
//! absence never proves a run that failed), and a failure the evidence does not settle never
//! hides a wrong value or a forbidden write already observed. Only a completed run can pass.
//! Rows compare as a multiset, block by block where the request orders them; a cut through
//! tied rows admits any of them unless ties keep file order; an output whose name no stated
//! fact gives is matched by its value under a key no other output reserves, and only a proven
//! free name passes.

use std::collections::{BTreeMap, BTreeSet};

use super::accounting::{Admission, Budget};
use super::evaluate::{Expected, Halt, Order, Stop, Undefined, any_passes, evaluate, may_stop};
use super::formats::{Coverage, Format, Refusal, json_document, records};
use super::numbers::{Law, law};
use super::pipeline::{Naming, OnEmpty, Pipeline};
use super::values::{Datum, Row, form, jq_equal, row_form, shown, shown_row};
use super::{
    Cause, Condition, Contract, Failure, Finding, Form, Identity, Judged, Obligation, Operation,
    Outcome, Presence, ReadBack, Report, Requirement, Run, RunEnd, StopFact, StopReason, Target,
    same_path,
};

/// What a fixture shows about one obligation: its outcome and the words for it.
type Shown = (Outcome, String);

const OVER_BUDGET: &str =
    "the fixture ran past the round or turn budget: its evidence does not count";

/// Judge `contract` on the round's `runs`, charging each run's usage to `budget` first: a run
/// past a round or turn limit is charged, but its evidence does not count.
#[must_use]
pub fn judge(contract: &Contract, runs: &[Run], budget: &mut Budget) -> Report {
    let analyses: Vec<Option<Analysis>> = runs
        .iter()
        .map(|run| (budget.charge(&run.usage) == Admission::Open).then(|| analyse(contract, run)))
        .collect();
    let judged = contract
        .obligations
        .iter()
        .enumerate()
        .map(|(at, obligation)| {
            let findings = runs
                .iter()
                .zip(&analyses)
                .map(|(run, analysis)| {
                    analysis.as_ref().map_or_else(
                        || {
                            let fixture = run.fixture.clone();
                            Finding::new(fixture, Outcome::NotRun, OVER_BUDGET, Vec::new())
                        },
                        |analysis| finding(at, obligation, run, analysis),
                    )
                })
                .collect();
            Judged::new(
                obligation.id.clone(),
                requested(obligation),
                assumptions(obligation),
                findings,
            )
        })
        .collect();
    Report::new(judged, budget.round(), budget.turn(), budget.admission())
}

/// Why a reading gives no expected value.
enum Settle {
    /// A stated rule stops the run on these records.
    Stop(Stop),
    /// The case stays open: a sample, a cut, an operation not evaluated, a reading not stated.
    Open(String),
    /// The fixture is invalid for this obligation: a source missing, malformed or outside the
    /// stated domain.
    Invalid(String),
}

fn settle(undefined: Undefined) -> Settle {
    match undefined {
        Undefined::Stops(stop) => Settle::Stop(stop),
        Undefined::Unverified(why) => Settle::Open(why),
        Undefined::OutOfDomain(why) => {
            Settle::Invalid(format!("the fixture is outside the stated domain: {why}"))
        }
    }
}

/// One obligation's prediction on the records a fixture consumed.
#[derive(Default)]
struct Reading {
    /// The truth of its condition, for a conditional presence.
    condition: Option<Result<bool, Settle>>,
    /// The expected result of its relation, for a computed requirement.
    relation: Option<Result<Expected, Settle>>,
    /// The stops its condition and relation predict, each with the source it reads.
    stops: Vec<(String, Stop)>,
    /// The cut through tied rows its relation admits as a stop: the source and the field.
    tie: Option<(String, String)>,
    /// The sources it reads whose open reading could stop the run.
    open: Vec<String>,
    /// The identities of the sources it read.
    evidence: Vec<Identity>,
    /// Why the fixture is no valid observation of this obligation, when it is none.
    invalid: Option<String>,
}

impl Reading {
    /// What an unsettled reading of `source` says about stops and validity.
    fn note(&mut self, source: &str, settled: Option<&Settle>, stoppable: bool) {
        match settled {
            Some(Settle::Stop(stop)) => self.stops.push((source.to_owned(), stop.clone())),
            Some(Settle::Open(_)) if stoppable => self.open.push(source.to_owned()),
            Some(Settle::Invalid(why)) if self.invalid.is_none() => {
                self.invalid = Some(why.clone());
            }
            Some(_) | None => {}
        }
    }

    /// Whether `fact` is consistent with a stop this reading predicts.
    fn predicts(&self, fact: &StopFact) -> bool {
        self.stops
            .iter()
            .any(|(source, stop)| consistent(fact, source, stop))
            || self.tie.as_ref().is_some_and(|(source, field)| {
                matches!(fact.reason, StopReason::TiedCut)
                    && fact.operation == Operation::Rank
                    && fact.field == *field
                    && same_path(&fact.source, source)
            })
    }

    /// The stop its condition predicts.
    fn condition_stop(&self) -> Option<&Stop> {
        match self.condition.as_ref()? {
            Err(Settle::Stop(stop)) => Some(stop),
            Ok(_) | Err(_) => None,
        }
    }

    /// The stop its relation predicts.
    fn relation_stop(&self) -> Option<&Stop> {
        match self.relation.as_ref()? {
            Err(Settle::Stop(stop)) => Some(stop),
            Ok(_) | Err(_) => None,
        }
    }
}

/// How a fixture ended, against the end the contract predicts.
enum Class {
    /// The observation is invalid: no obligation can be judged on it.
    Invalid(String),
    /// Nothing ran that can show anything.
    NotRun(String),
    /// A failure no stated stop explains: a defect of the workflow.
    Foreign(String),
    /// A failure the evidence does not settle as a defect: an unattested stop, or a failure
    /// where a stated rule may stop the run or a result is invalid.
    Undetermined(String),
    /// The run completed.
    Completed,
}

/// What one fixture shows before any obligation is read.
struct Analysis {
    class: Class,
    readings: Vec<Reading>,
}

fn analyse(contract: &Contract, run: &Run) -> Analysis {
    let early = |class| Analysis {
        class,
        readings: Vec::new(),
    };
    if let Some(why) = contradiction(run) {
        return early(Class::Invalid(why));
    }
    let failure = match &run.end {
        RunEnd::NotRun { reason } => {
            return early(Class::NotRun(format!("not rehearsed: {reason}")));
        }
        RunEnd::InvalidHarness { reason } => {
            let why = format!("the host could not build a valid observation: {reason}");
            return early(Class::Invalid(why));
        }
        RunEnd::Completed => None,
        RunEnd::Failed(failure) => Some(failure),
    };
    let readings: Vec<Reading> = contract
        .obligations
        .iter()
        .map(|obligation| read(obligation, run))
        .collect();
    let class = failure.map_or(Class::Completed, |failure| {
        classify(contract, run, failure, &readings)
    });
    Analysis { class, readings }
}

/// Two different observations of one path in one collection: the evidence contradicts itself.
/// What the run consumed and what the host read back after it are never compared.
fn contradiction(run: &Run) -> Option<String> {
    for (at, read) in run.consumed.iter().enumerate() {
        let differs = run.consumed.iter().skip(at + 1).any(|other| {
            same_path(&read.path, &other.path)
                && (read.text != other.text || read.coverage != other.coverage)
        });
        if differs {
            return Some(format!(
                "two observations of what the run consumed from {} differ",
                read.path
            ));
        }
    }
    for (at, read) in run.read_back.iter().enumerate() {
        let differs = run.read_back.iter().skip(at + 1).any(|other| {
            same_path(&read.path, &other.path)
                && (read.text != other.text
                    || read.written != other.written
                    || read.truncated != other.truncated)
        });
        if differs {
            return Some(format!(
                "two observations of {} read back after the run differ",
                read.path
            ));
        }
    }
    None
}

fn read(obligation: &Obligation, run: &Run) -> Reading {
    let mut reading = Reading::default();
    if let Presence::When(condition) | Presence::OnlyWhen(condition) = &obligation.presence {
        let truth = condition_of(condition, run, &mut reading.evidence);
        let stoppable = !condition.policies.is_empty();
        reading.note(&condition.source, truth.as_ref().err(), stoppable);
        reading.condition = Some(truth);
    }
    if let Requirement::Computed {
        source,
        source_format,
        pipeline,
        ..
    } = &obligation.requirement
    {
        let relation = source_rows(source, *source_format, run, &mut reading.evidence)
            .and_then(|rows| evaluate(pipeline, rows).map_err(settle));
        reading.note(source, relation.as_ref().err(), may_stop(pipeline));
        if let Ok(expected) = &relation
            && let Some(field) = &expected.tie_stop
        {
            reading.tie = Some((source.clone(), field.clone()));
        }
        reading.relation = Some(relation);
    }
    if let Some(target) = &obligation.target
        && reading.invalid.is_none()
        && !run
            .read_back
            .iter()
            .any(|read| same_path(&read.path, &target.path))
    {
        reading.invalid = Some(format!(
            "the host observed no final state of {}: a missing receipt is no observation of a \
             write or of its absence",
            target.path
        ));
    }
    reading
}

fn identity(path: &str, text: &str, coverage: Coverage, count: Option<usize>) -> Identity {
    let mut identity = Identity::new(path, text.as_bytes(), coverage);
    identity.records = count;
    identity
}

/// Whether every number of the typed values of `rows` survives the runtime's number type.
fn carried(rows: &[Row]) -> bool {
    fn survives(datum: &Datum) -> bool {
        match datum {
            Datum::Number(number) => number.survives_runtime(),
            Datum::List(items) => items.iter().all(survives),
            Datum::Record(fields) => fields.values().all(survives),
            Datum::Null | Datum::Bool(_) | Datum::Text(_) => true,
        }
    }
    rows.iter()
        .flat_map(Row::values)
        .all(|cell| cell.loose || survives(&cell.datum))
}

/// The records the run consumed from `source`, when the evidence is exactly that.
fn source_rows(
    source: &str,
    format: Format,
    run: &Run,
    evidence: &mut Vec<Identity>,
) -> Result<Vec<Row>, Settle> {
    let Some(consumed) = run
        .consumed
        .iter()
        .find(|read| same_path(&read.path, source))
    else {
        return Err(Settle::Invalid(format!(
            "the host supplied no evidence of what the run consumed from {source}"
        )));
    };
    let read = records(format, &consumed.text);
    let count = read.as_ref().ok().map(Vec::len);
    evidence.push(identity(
        &consumed.path,
        &consumed.text,
        consumed.coverage,
        count,
    ));
    if consumed.coverage != Coverage::Complete {
        return Err(Settle::Open(format!(
            "the evidence of {source} is {}: the judgment needs exactly the records the run \
             consumed",
            consumed.coverage.word()
        )));
    }
    let rows = read.map_err(|refusal| match refusal {
        Refusal::Malformed(why) => Settle::Invalid(format!(
            "{source} is not well-formed {}: {why}",
            format.word()
        )),
        Refusal::NotRecords(why) => Settle::Open(format!(
            "{source}: {why}; which records the request means is not stated"
        )),
        Refusal::Beyond => Settle::Open(format!(
            "{source} holds a number beyond the supported precision"
        )),
    })?;
    if !carried(&rows) {
        return Err(Settle::Open(format!(
            "{source} holds a number the runtime's number type does not carry exactly: exact \
             equality is not a verdict on it"
        )));
    }
    Ok(rows)
}

fn condition_of(
    condition: &Condition,
    run: &Run,
    evidence: &mut Vec<Identity>,
) -> Result<bool, Settle> {
    let rows = source_rows(&condition.source, condition.format, run, evidence)?;
    any_passes(&condition.filter, &rows, &condition.policies)
        .map(|any| any == condition.when_any)
        .map_err(settle)
}

/// Whether the request names `path` as a source.
fn named_source(contract: &Contract, path: &str) -> bool {
    let named = |source: &str| same_path(source, path);
    contract.sources.iter().any(|source| named(source))
        || contract.obligations.iter().any(|obligation| {
            let condition = matches!(&obligation.presence,
                Presence::When(condition) | Presence::OnlyWhen(condition)
                    if named(&condition.source));
            let computed = matches!(&obligation.requirement,
                Requirement::Computed { source, .. } if named(source));
            condition || computed
        })
}

/// How the host recorded the copy of `path` into the room, when it recorded one.
fn copied(run: &Run, path: &str) -> Option<Coverage> {
    run.consumed
        .iter()
        .find(|read| same_path(&read.path, path))
        .map(|read| read.coverage)
}

/// What the canonical reading says of a source a run could not parse: `Some(true)` when it
/// refuses it too, `Some(false)` when it reads its records, `None` when the evidence does not
/// say (no complete evidence of it, or a document that holds no records).
fn malformed(run: &Run, path: &str) -> Option<bool> {
    let format = Format::of_path(path)?;
    let consumed = run
        .consumed
        .iter()
        .find(|read| same_path(&read.path, path))?;
    if consumed.coverage != Coverage::Complete {
        return None;
    }
    match records(format, &consumed.text) {
        Ok(_) => Some(false),
        Err(Refusal::Malformed(_)) => Some(true),
        Err(Refusal::NotRecords(_) | Refusal::Beyond) => None,
    }
}

/// Whether the value a stop fact reports (its JSON text; `None` for null or a missing field)
/// is `value`.
fn names(value: &Datum, reported: Option<&str>) -> bool {
    reported.map_or(*value == Datum::Null, |text| {
        json_document(text).is_ok_and(|datum| jq_equal(&datum, value))
    })
}

/// Whether `fact` is consistent with `stop`, predicted on the records of `source`: the same
/// source, operation and field, and the same reason, naming one of the values it may name.
/// Consistency is no attestation: the fact names no data or policy independent of the workflow.
fn consistent(fact: &StopFact, source: &str, stop: &Stop) -> bool {
    let reason = match (&fact.reason, &stop.halt) {
        (StopReason::NotANumber { value }, Halt::NotANumber(values)) => values
            .iter()
            .any(|candidate| names(candidate, value.as_deref())),
        (StopReason::NoNumber, Halt::NoNumber) => true,
        _ => false,
    };
    reason
        && fact.operation == stop.operation
        && fact.field == stop.field
        && same_path(&fact.source, source)
}

fn operation_word(operation: Operation) -> &'static str {
    match operation {
        Operation::Condition => "the condition",
        Operation::Test => "the filter",
        Operation::Aggregate => "the aggregate",
        Operation::Rank => "the ranking",
        Operation::Column => "the number column",
    }
}

fn fact_words(fact: &StopFact) -> String {
    let at = format!(
        "{} on {} of {}",
        operation_word(fact.operation),
        fact.field,
        fact.source
    );
    match &fact.reason {
        StopReason::NotANumber { value } => format!(
            "{at}: {} is no number",
            value.as_deref().unwrap_or("null or a missing value")
        ),
        StopReason::NoNumber => format!("{at}: no value is a number"),
        StopReason::TiedCut => format!("{at}: a cut through tied rows"),
    }
}

fn classify(contract: &Contract, run: &Run, failure: &Failure, readings: &[Reading]) -> Class {
    let head = format!("the run failed in {} with {}", failure.task, failure.code);
    let named = |path: &str| named_source(contract, path);
    // An established cause decides first: it never waits on a stop the contract predicts.
    let established = match &failure.cause {
        Cause::TimeBound => {
            return Class::NotRun(format!("{head}: the host's time bound stopped it"));
        }
        // An open that finds no file is a fact of the run, not of the fixture: a whole recorded
        // copy shows the run lost the source; no record proves no initial absence (only the
        // host's own invalid-harness end does), so it settles nothing.
        Cause::MissingFile { path } if named(path) => match copied(run, path) {
            None => {
                return Class::Undetermined(format!(
                    "{head}: {path} was missing when the run opened it, and no record of its \
                     copy says whether the fixture held it"
                ));
            }
            Some(Coverage::Complete) => Some(format!(
                "{path}, a source the request names and the host copied whole, went missing \
                 during the run"
            )),
            Some(_) => {
                return Class::Undetermined(format!(
                    "{head}: {path} went missing during the run, and the evidence does not attest \
                     it was copied whole"
                ));
            }
        },
        Cause::Unparsable { path } if named(path) => match malformed(run, path) {
            Some(true) => {
                let why = format!("{head}: {path}, a source the request names, is malformed");
                return Class::Invalid(why);
            }
            Some(false) => Some(format!(
                "it could not parse {path}, which the canonical reading reads"
            )),
            None => {
                return Class::Undetermined(format!(
                    "{head}: it could not parse {path}, and the evidence does not say whether \
                     that source is well-formed"
                ));
            }
        },
        Cause::Engine => Some("the engine failed on a valid fixture".to_owned()),
        Cause::MissingFile { path } => {
            Some(format!("it opened {path}, which the request never names"))
        }
        Cause::Unparsable { path } => Some(format!(
            "it could not parse {path}, which the request never names"
        )),
        Cause::Stop(_) | Cause::Unclassified => None,
    };
    if let Some(explained) = established {
        return Class::Foreign(format!("{head}: {explained}"));
    }
    unsettled(failure, readings, &head)
}

/// A stop or a failure no structured fact explains: never attested, and a defect only where
/// the contract rules out every stop it could be and every result is valid.
fn unsettled(failure: &Failure, readings: &[Reading], head: &str) -> Class {
    let invalid = readings.iter().find_map(|reading| reading.invalid.as_ref());
    let invalid_words = |why: &String| {
        format!("the fixture is invalid for a result ({why}), and the failure may come from it")
    };
    if let Cause::Stop(fact) = &failure.cause {
        let words = fact_words(fact);
        let predicted = readings
            .iter()
            .any(|reading| reading.invalid.is_none() && reading.predicts(fact));
        let open = readings.iter().any(|reading| {
            reading
                .open
                .iter()
                .any(|source| same_path(source, &fact.source))
        });
        let why = if predicted {
            "it is consistent with the stop a stated rule predicts, but it names no data or \
             policy independent of the workflow: another occurrence of that operation on these \
             records would report the same fact"
                .to_owned()
        } else if open {
            "a relation the judgment could not evaluate may stop on these records".to_owned()
        } else if let Some(why) = invalid {
            invalid_words(why)
        } else {
            return Class::Foreign(format!(
                "{head}: a stop ({words}) no stated rule predicts on these records"
            ));
        };
        return Class::Undetermined(format!("{head}: an unattested stop ({words}): {why}"));
    }
    let stoppable = readings.iter().any(|reading| {
        !reading.stops.is_empty() || reading.tie.is_some() || !reading.open.is_empty()
    });
    if stoppable {
        return Class::Undetermined(format!(
            "{head}: a stated rule may stop the run here, and no structured fact ties this \
             failure to it"
        ));
    }
    if let Some(why) = invalid {
        return Class::Undetermined(format!("{head}: {}", invalid_words(why)));
    }
    Class::Foreign(format!(
        "{head}: no structured fact says why, and no stated rule stops the run here"
    ))
}

fn finding(at: usize, obligation: &Obligation, run: &Run, analysis: &Analysis) -> Finding {
    let reading = analysis.readings.get(at);
    let mut evidence = reading.map_or_else(Vec::new, |reading| reading.evidence.clone());
    let valid = !matches!(analysis.class, Class::Invalid(_));
    let observed = obligation
        .target
        .as_ref()
        .filter(|_| valid)
        .and_then(|target| {
            run.read_back
                .iter()
                .find(|read| same_path(&read.path, &target.path))
        });
    if let Some(read) = observed {
        let coverage = if read.truncated {
            Coverage::Truncated
        } else {
            Coverage::Complete
        };
        evidence.push(identity(&read.path, &read.text, coverage, None));
    }
    let written = observed.filter(|read| read.written);
    let (outcome, words) = outcome_of(obligation, reading, &analysis.class, written);
    Finding::new(run.fixture.clone(), outcome, words, evidence)
}

/// What the run left at the obligation's file, for the words of a failed run.
fn absence(obligation: &Obligation, written: Option<&ReadBack>) -> String {
    let Some(target) = &obligation.target else {
        return String::new();
    };
    if written.is_some() {
        format!("; {} was written", target.path)
    } else {
        format!(
            "; {} was not written, which proves nothing about a run that failed",
            target.path
        )
    }
}

/// One obligation on one fixture: an invalid observation first, then the class of the run.
fn outcome_of(
    obligation: &Obligation,
    reading: Option<&Reading>,
    class: &Class,
    written: Option<&ReadBack>,
) -> Shown {
    let invalid = match class {
        Class::Invalid(why) => Some(why),
        _ => reading.and_then(|reading| reading.invalid.as_ref()),
    };
    if let Some(why) = invalid {
        return (Outcome::InvalidHarness, why.clone());
    }
    match class {
        Class::Invalid(why) => (Outcome::InvalidHarness, why.clone()),
        Class::NotRun(why) => (Outcome::NotRun, why.clone()),
        Class::Foreign(why) => (
            Outcome::Failed,
            format!("{why}{}", absence(obligation, written)),
        ),
        Class::Undetermined(why) => ended(obligation, reading, written, why),
        Class::Completed => completed(obligation, reading, written),
    }
}

/// The truth of an obligation's condition on these records.
#[derive(Clone, Copy)]
enum Truth {
    Holds,
    Fails,
    Open,
}

fn truth(reading: Option<&Reading>) -> Truth {
    match reading.and_then(|reading| reading.condition.as_ref()) {
        Some(Ok(true)) => Truth::Holds,
        Some(Ok(false)) => Truth::Fails,
        Some(Err(_)) | None => Truth::Open,
    }
}

/// What the request says about the write, on these records.
enum Need {
    /// Written, and holding the requested value.
    Write,
    /// Never written, for the reason in words.
    Absent(&'static str),
    /// Written only after a human approves: a rehearsal never takes that decision.
    Approval,
    /// Not required: a written result must still hold the requested value.
    Optional,
    /// No fact says: a written result must hold the requested value, nothing certifies.
    Open(&'static str),
}

fn need(presence: &Presence, truth: Truth) -> Need {
    match (presence, truth) {
        (Presence::Required, _) | (Presence::When(_) | Presence::OnlyWhen(_), Truth::Holds) => {
            Need::Write
        }
        (Presence::Forbidden, _) => Need::Absent("which the request forbids"),
        (Presence::OnlyWhen(_), Truth::Fails) => Need::Absent("though its condition does not hold"),
        (Presence::When(_), Truth::Fails) => Need::Optional,
        (Presence::Approval, _) => Need::Approval,
        (Presence::Unproven, _) => {
            Need::Open("whether a condition governs this write is not a fact the plan states")
        }
        (Presence::Undecided, _) => Need::Open("the request leaves this write undecided"),
        (Presence::When(_) | Presence::OnlyWhen(_), Truth::Open) => {
            Need::Open("the condition of this write is not verified")
        }
    }
}

/// An obligation on a run that failed for a reason the evidence does not settle: what was
/// written is judged (a stop never goes back in time, nor hides a defect already observed),
/// nothing written is certified, and an absence proves nothing.
fn ended(
    obligation: &Obligation,
    reading: Option<&Reading>,
    written: Option<&ReadBack>,
    why: &str,
) -> Shown {
    let Some(target) = &obligation.target else {
        return (Outcome::Incomplete, why.to_owned());
    };
    let path = &target.path;
    let Some(output) = written else {
        return (
            Outcome::Incomplete,
            format!("{path} was not written: {why}"),
        );
    };
    if let Some(stop) = reading.and_then(Reading::condition_stop) {
        let words = format!(
            "{path} was written though the stated rule of its condition stops the run on these \
             records ({})",
            stop.describe()
        );
        return (Outcome::Failed, words);
    }
    match need(&obligation.presence, truth(reading)) {
        Need::Absent(words) => (Outcome::Failed, format!("{path} was written {words}")),
        Need::Approval => (
            Outcome::Failed,
            format!("{path} was written without the human approval the request requires"),
        ),
        Need::Write | Need::Optional | Need::Open(_) => {
            match content(obligation, reading, target, output) {
                (outcome @ (Outcome::Failed | Outcome::InvalidHarness), observed) => {
                    (outcome, observed)
                }
                (_, observed) => (Outcome::Incomplete, format!("{observed}; {why}")),
            }
        }
    }
}

/// An obligation on a run that completed.
fn completed(
    obligation: &Obligation,
    reading: Option<&Reading>,
    written: Option<&ReadBack>,
) -> Shown {
    if let Some(stop) = reading.and_then(Reading::condition_stop) {
        let why = format!(
            "the stated rule of the condition stops the run on these records ({}), but the run \
             completed",
            stop.describe()
        );
        return (Outcome::Failed, why);
    }
    let Some(target) = &obligation.target else {
        return (
            Outcome::Incomplete,
            "the request names no file this component reads for this result".to_owned(),
        );
    };
    let path = &target.path;
    let judged = |output: &ReadBack| content(obligation, reading, target, output);
    match (need(&obligation.presence, truth(reading)), written) {
        (Need::Absent(words), Some(_)) => (Outcome::Failed, format!("{path} was written {words}")),
        (Need::Absent(_), None) => (
            Outcome::Passed,
            format!("{path} was not written, as the request requires"),
        ),
        (Need::Approval, Some(_)) => (
            Outcome::Failed,
            format!("{path} was written without the human approval the request requires"),
        ),
        (Need::Approval, None) => (
            Outcome::NotRun,
            "the write waits for a human decision a rehearsal never takes".to_owned(),
        ),
        (Need::Write | Need::Optional, Some(output)) => judged(output),
        (Need::Write, None) => {
            let why = reading.and_then(Reading::relation_stop).map_or_else(
                || format!("{path} was not written"),
                |stop| {
                    format!(
                        "the stated rule stops the run on these records ({}), but the run \
                         completed",
                        stop.describe()
                    )
                },
            );
            (Outcome::Failed, why)
        }
        (Need::Optional, None) => (
            Outcome::Passed,
            "not written, and not required: its condition does not hold".to_owned(),
        ),
        (Need::Open(why), Some(output)) => match judged(output) {
            (Outcome::Passed, observed) => (Outcome::Incomplete, format!("{observed}; {why}")),
            other => other,
        },
        (Need::Open(why), None) => (
            Outcome::Incomplete,
            format!("{path} was not written; {why}"),
        ),
    }
}

/// What a written file holds, against what the obligation requires of it. The relation comes
/// from the rule, never from the policy of the write, so a wrong value fails whatever the
/// presence.
fn content(
    obligation: &Obligation,
    reading: Option<&Reading>,
    target: &Target,
    output: &ReadBack,
) -> Shown {
    let path = &target.path;
    let Requirement::Computed {
        pipeline,
        form: shape,
        ..
    } = &obligation.requirement
    else {
        return match &obligation.requirement {
            Requirement::Unsupported(why) => (
                Outcome::Incomplete,
                format!("{path} was written; its content is not verified: {why}"),
            ),
            _ => (Outcome::Passed, format!("{path} was written")),
        };
    };
    match reading.and_then(|reading| reading.relation.as_ref()) {
        Some(Ok(expected)) => written_result(expected, pipeline, target, *shape, output),
        Some(Err(Settle::Stop(stop))) => (
            Outcome::Failed,
            format!(
                "{path} was written, but the stated rule stops the run on these records ({}): \
                 no written value can be the requested one",
                stop.describe()
            ),
        ),
        Some(Err(Settle::Open(why))) => (
            Outcome::Incomplete,
            format!("the requested result is not verified: {why}"),
        ),
        Some(Err(Settle::Invalid(why))) => (Outcome::InvalidHarness, why.clone()),
        None => (
            Outcome::Incomplete,
            "the requested result was not read".to_owned(),
        ),
    }
}

fn written_result(
    expected: &Expected,
    pipeline: &Pipeline,
    target: &Target,
    shape: Form,
    output: &ReadBack,
) -> Shown {
    let path = &target.path;
    if expected.totals != matches!(shape, Form::Totals | Form::Alone) {
        return (
            Outcome::Incomplete,
            "the requested form and the relation disagree on whether it writes totals".to_owned(),
        );
    }
    if output.truncated {
        return (
            Outcome::Incomplete,
            format!("{path} was cut at the host's byte bound"),
        );
    }
    match shape {
        Form::Rows => match records(target.format, &output.text) {
            Ok(rows) => compare_rows(expected, pipeline.keep_order, &rows),
            Err(refusal) => unreadable(target, refusal),
        },
        Form::Totals => match json_document(&output.text) {
            Ok(document) => compare_totals(expected, &document),
            Err(refusal) => unreadable(target, refusal),
        },
        Form::Alone => alone_written(expected, target, &output.text),
    }
}

fn unreadable(target: &Target, refusal: Refusal) -> Shown {
    let path = &target.path;
    match refusal {
        Refusal::Malformed(why) => (
            Outcome::Failed,
            format!("{path} is not well-formed {}: {why}", target.format.word()),
        ),
        Refusal::NotRecords(why) => (
            Outcome::Failed,
            format!("{path} does not hold the requested rows: {why}"),
        ),
        Refusal::Beyond => (
            Outcome::Incomplete,
            format!("{path} holds a number beyond the supported precision"),
        ),
    }
}

/// Remove one of each of `wanted` from `pool`; the first one missing, if any.
fn take_from<'a>(pool: &mut BTreeMap<&'a str, usize>, wanted: &[&'a str]) -> Option<usize> {
    for (at, item) in wanted.iter().enumerate() {
        let Some(count) = pool.get_mut(item).filter(|count| **count > 0) else {
            return Some(at);
        };
        *count -= 1;
    }
    None
}

fn counts<'a>(forms: impl Iterator<Item = &'a str>) -> BTreeMap<&'a str, usize> {
    let mut seen = BTreeMap::new();
    for item in forms {
        *seen.entry(item).or_insert(0) += 1;
    }
    seen
}

/// The comparison forms of the values a row holds under keys outside `reserved`.
fn free_values(row: &Row, reserved: &BTreeSet<String>, loose: bool) -> Vec<String> {
    row.iter()
        .filter(|(key, _)| !reserved.contains(key.as_str()))
        .filter_map(|(_, cell)| form(&cell.datum, cell.loose || loose))
        .collect()
}

/// The comparison form of a row: its reserved columns by name, then the multiset of the
/// values it holds under any other key, so that no key serves two outputs and no other key is
/// ignored.
fn keyed_form(
    row: &Row,
    reserved: &BTreeSet<String>,
    loose: &BTreeSet<String>,
    free_loose: bool,
) -> String {
    let named: Row = row
        .iter()
        .filter(|(key, _)| reserved.contains(key.as_str()))
        .map(|(key, cell)| (key.clone(), cell.clone()))
        .collect();
    let mut values = free_values(row, reserved, free_loose);
    values.sort();
    let mut out = row_form(&named, loose);
    for value in values {
        out.push('\u{0}');
        out.push_str(&value);
    }
    out
}

/// A matched result is certified only where no output name stays open: none is unstated, or
/// the one that is was proven free.
fn certify_names(shown: Shown, unstated: &[(&String, Naming)]) -> Shown {
    let (outcome, observed) = shown;
    if outcome != Outcome::Passed || matches!(unstated, [] | [(_, Naming::Free)]) {
        return (outcome, observed);
    }
    let names: Vec<&str> = unstated.iter().map(|(name, _)| name.as_str()).collect();
    let why = if names.len() > 1 {
        format!(
            "no stated name tells which key holds which of {}: the association is not established",
            names.join(", ")
        )
    } else {
        format!(
            "no fact says whether the request states the name {}: the value is right, the name \
             is not certified",
            names.join(", ")
        )
    };
    (Outcome::Incomplete, format!("{observed}; {why}"))
}

/// The written rows against the expected blocks.
fn compare_rows(expected: &Expected, keep_order: bool, written: &[Row]) -> Shown {
    let size = expected.size();
    if written.len() != size {
        return (
            Outcome::Failed,
            format!(
                "{} rows were written, the request asks for {size}",
                written.len()
            ),
        );
    }
    let expected_rows = || expected.blocks.iter().flat_map(|block| &block.rows);
    let columns: BTreeSet<&String> = expected_rows().flat_map(Row::keys).collect();
    let unstated: Vec<(&String, Naming)> = expected
        .unstated
        .iter()
        .filter(|(name, _)| columns.contains(name))
        .map(|(name, naming)| (name, *naming))
        .collect();
    let reserved: BTreeSet<String> = columns
        .into_iter()
        .filter(|column| !expected.unstated.contains_key(column.as_str()))
        .cloned()
        .collect();
    // A reserved column either side reads loosely (a CSV source or a CSV output) compares
    // loosely; so do the other values once a written one is loose.
    let mut loose = BTreeSet::new();
    let mut free_loose = false;
    for row in expected_rows().chain(written) {
        for (column, _) in row.iter().filter(|(_, cell)| cell.loose) {
            if reserved.contains(column.as_str()) {
                loose.insert(column.clone());
            } else {
                free_loose = true;
            }
        }
    }
    for (at, row) in written.iter().enumerate() {
        let held = free_values(row, &reserved, free_loose).len();
        if held != unstated.len() {
            return (
                Outcome::Failed,
                format!(
                    "written row {} holds {held} values under keys the request does not name, \
                     where {} are expected",
                    at + 1,
                    unstated.len()
                ),
            );
        }
    }
    let form_of = |row: &Row| keyed_form(row, &reserved, &loose, free_loose);
    let written_forms: Vec<String> = written.iter().map(form_of).collect();
    let block_forms: Vec<Vec<String>> = expected
        .blocks
        .iter()
        .map(|block| block.rows.iter().map(form_of).collect())
        .collect();
    let matched = match expected.order {
        Order::Sorted { .. } => ordered_rows(expected, &block_forms, written, &written_forms),
        Order::File if keep_order => sequence(expected, &block_forms, written, &written_forms),
        Order::File | Order::Free => multiset(expected, &block_forms, written, &written_forms),
    };
    certify_names(matched, &unstated)
}

/// The rows in the source order the request keeps.
fn sequence(
    expected: &Expected,
    block_forms: &[Vec<String>],
    written: &[Row],
    written_forms: &[String],
) -> Shown {
    let wanted = expected.blocks.iter().flat_map(|block| &block.rows);
    let wanted_forms = block_forms.iter().flatten();
    for (at, ((row, want), got)) in wanted.zip(wanted_forms).zip(written_forms).enumerate() {
        if want != got {
            let found = written.get(at).map_or_else(String::new, shown_row);
            return (
                Outcome::Failed,
                format!(
                    "row {} is {found}, the request keeps {} there",
                    at + 1,
                    shown_row(row)
                ),
            );
        }
    }
    (
        Outcome::Passed,
        format!(
            "{} rows, the requested rows in the source order",
            written.len()
        ),
    )
}

/// The rows as a multiset: every whole block's rows with their multiplicity, the rest drawn
/// from the blocks a cut goes through.
fn multiset(
    expected: &Expected,
    block_forms: &[Vec<String>],
    written: &[Row],
    written_forms: &[String],
) -> Shown {
    let mut remaining = counts(written_forms.iter().map(String::as_str));
    for (block, forms) in expected.blocks.iter().zip(block_forms) {
        if block.partial() {
            continue;
        }
        let wanted: Vec<&str> = forms.iter().map(String::as_str).collect();
        if let Some(missing) = take_from(&mut remaining, &wanted) {
            let row = block.rows.get(missing).map_or_else(String::new, shown_row);
            return (
                Outcome::Failed,
                format!("the requested row {row} is missing"),
            );
        }
    }
    let mut optional = counts(
        expected
            .blocks
            .iter()
            .zip(block_forms)
            .filter(|(block, _)| block.partial())
            .flat_map(|(_, forms)| forms.iter().map(String::as_str)),
    );
    for (row, key) in written.iter().zip(written_forms) {
        let Some(count) = remaining.get_mut(key.as_str()).filter(|count| **count > 0) else {
            continue;
        };
        let Some(left) = optional.get_mut(key.as_str()).filter(|left| **left > 0) else {
            return (
                Outcome::Failed,
                format!(
                    "the written row {} is not one the request asks for",
                    shown_row(row)
                ),
            );
        };
        *left -= 1;
        *count -= 1;
    }
    (
        Outcome::Passed,
        format!(
            "{} rows, the requested rows with their multiplicity",
            written.len()
        ),
    )
}

/// The rows block by block in the requested order.
fn ordered_rows(
    expected: &Expected,
    block_forms: &[Vec<String>],
    written: &[Row],
    written_forms: &[String],
) -> Shown {
    let stable = expected.order == Order::Sorted { stable: true };
    let mut at = 0;
    for (block, wanted) in expected.blocks.iter().zip(block_forms) {
        let end = at + block.take;
        let segment: Vec<&str> = written_forms
            .get(at..end)
            .unwrap_or_default()
            .iter()
            .map(String::as_str)
            .collect();
        let fits = if stable {
            wanted
                .iter()
                .map(String::as_str)
                .eq(segment.iter().copied())
        } else {
            let mut pool = counts(wanted.iter().map(String::as_str));
            take_from(&mut pool, &segment).is_none()
        };
        if !fits {
            let row = written.get(at).map_or_else(String::new, shown_row);
            return (
                Outcome::Failed,
                format!(
                    "rows {} to {end} are not the requested rows at that place of the order \
                     (from {row})",
                    at + 1
                ),
            );
        }
        at = end;
    }
    (
        Outcome::Passed,
        format!("{at} rows, each block of equal keys at its place in the requested order"),
    )
}

/// The one object of totals: every stated total under its name, and the totals whose name is
/// not stated as the values of the other keys, one key each.
fn compare_totals(expected: &Expected, document: &Datum) -> Shown {
    let Some(wanted) = expected.blocks.first().and_then(|block| block.rows.first()) else {
        return (Outcome::Incomplete, "no totals are expected".to_owned());
    };
    let Datum::Record(fields) = document else {
        return (
            Outcome::Failed,
            format!(
                "the output is {}, not the one object of totals",
                shown(document)
            ),
        );
    };
    let unstated: Vec<(&String, Naming)> = wanted
        .keys()
        .filter_map(|name| expected.unstated.get(name).map(|naming| (name, *naming)))
        .collect();
    let mut wanted_free = Vec::new();
    for (name, cell) in wanted {
        if expected.unstated.contains_key(name) {
            wanted_free.push(form(&cell.datum, false));
            continue;
        }
        let Some(value) = fields.get(name) else {
            return (Outcome::Failed, format!("{name} is missing"));
        };
        if form(value, false) != form(&cell.datum, false) {
            return (
                Outcome::Failed,
                format!(
                    "{name} is {}, the request gives {}",
                    shown(value),
                    shown(&cell.datum)
                ),
            );
        }
    }
    let stated = |key: &str| wanted.contains_key(key) && !expected.unstated.contains_key(key);
    let mut written_free: Vec<Option<String>> = fields
        .iter()
        .filter(|(key, _)| !stated(key.as_str()))
        .map(|(_, value)| form(value, false))
        .collect();
    if written_free.len() != wanted_free.len() {
        let keys: Vec<&String> = fields.keys().collect();
        return (
            Outcome::Failed,
            format!(
                "the object holds {keys:?}, the request asks for {} totals",
                wanted.len()
            ),
        );
    }
    written_free.sort();
    wanted_free.sort();
    if written_free != wanted_free {
        return (
            Outcome::Failed,
            "the totals whose name the request does not state hold other values".to_owned(),
        );
    }
    let matched = (
        Outcome::Passed,
        format!("the {} totals hold the requested values", wanted.len()),
    );
    certify_names(matched, &unstated)
}

fn alone_written(expected: &Expected, target: &Target, text: &str) -> Shown {
    let Some(cell) = expected
        .blocks
        .first()
        .and_then(|block| block.rows.first())
        .and_then(|row| row.values().next())
    else {
        return (Outcome::Incomplete, "no value is expected".to_owned());
    };
    let written = if target.format == Format::Text {
        match law(text.trim_matches(|c: char| c.is_ascii_whitespace())) {
            Law::Number(number) => Datum::Number(number),
            Law::NotANumber => Datum::Text(text.to_owned()),
            Law::Beyond => return unreadable(target, Refusal::Beyond),
        }
    } else {
        match json_document(text) {
            Ok(document) => document,
            Err(refusal) => return unreadable(target, refusal),
        }
    };
    let path = &target.path;
    if form(&written, false) == form(&cell.datum, false) {
        (
            Outcome::Passed,
            format!("{path} holds {} alone", shown(&written)),
        )
    } else {
        (
            Outcome::Failed,
            format!(
                "{path} holds {}, the request asks for {} alone",
                shown(&written),
                shown(&cell.datum)
            ),
        )
    }
}

/// The requested result, in words, for the preview.
fn requested(obligation: &Obligation) -> String {
    let place = obligation
        .target
        .as_ref()
        .map_or_else(|| "a result with no file".to_owned(), |t| t.path.clone());
    let what = match &obligation.requirement {
        Requirement::PresenceOnly => "whether it is written".to_owned(),
        Requirement::Computed {
            source,
            pipeline,
            form: written_as,
            ..
        } => {
            let shape = match written_as {
                Form::Rows => "rows",
                Form::Totals => "totals",
                Form::Alone => "one value alone",
            };
            format!("{shape} from {source}: {}", pipeline.describe())
        }
        Requirement::Unsupported(why) => format!("not verified by this component ({why})"),
    };
    let when = match &obligation.presence {
        Presence::Required => "written",
        Presence::Unproven => "asked for, under a condition the plan cannot state or none",
        Presence::Forbidden => "never written",
        Presence::Approval => "written after a human approves",
        Presence::Undecided => "undecided",
        Presence::When(_) => "written when its condition holds",
        Presence::OnlyWhen(_) => "written only when its condition holds",
    };
    format!(
        "{place}, {when}: {what} (the request: {:?})",
        obligation.evidence
    )
}

/// The interpretations every judgment of an obligation applies.
fn presence_assumptions(obligation: &Obligation, out: &mut Vec<String>) {
    out.push(
        "a stop is never certified until a provenance independent of the workflow binds it to \
         the occurrence a stated rule stops; a failure with an established cause, or where no \
         stated rule stops the run, is a defect; a host bound proves nothing, and a \
         contradictory or missing observation is no verdict"
            .to_owned(),
    );
    out.push(
        "a result not written is the host's observation of it, never a missing receipt; a stop \
         never hides a wrong value or a forbidden write already observed"
            .to_owned(),
    );
    match &obligation.presence {
        Presence::When(_) | Presence::OnlyWhen(_) => out.push(
            "the file is required only where its condition holds on the records the run consumed"
                .to_owned(),
        ),
        Presence::Unproven => out.push(
            "the plan cannot state whether a condition governs this write: a written result \
             must hold the requested value, but neither the write nor its absence certifies"
                .to_owned(),
        ),
        Presence::Approval => {
            out.push("a rehearsal never takes the human decision the write waits for".to_owned());
        }
        Presence::Required | Presence::Forbidden | Presence::Undecided => {}
    }
}

/// The interpretations the relation of a computed obligation applies.
fn relation_assumptions(pipeline: &Pipeline, out: &mut Vec<String>) {
    if pipeline.policies.is_empty() {
        out.push(
            "a field read as a number holds a number: a fixture where it does not is outside \
             the request's domain, not a defect"
                .to_owned(),
        );
    } else {
        out.push(
            "a stated number policy decides each value that is no number: fail stops the run, \
             skip leaves the record out"
                .to_owned(),
        );
    }
    let aggregates = || {
        pipeline
            .steps
            .iter()
            .flat_map(|step| &step.stages.aggregates)
    };
    if aggregates().any(|aggregate| aggregate.on_empty == OnEmpty::Unstated) {
        out.push(
            "an average, minimum or maximum of no value has no assumed value: never zero"
                .to_owned(),
        );
    }
    if aggregates().any(|aggregate| aggregate.naming == Naming::Unknown) {
        out.push(
            "an output name no fact traces to the request is unknown: a right value under some \
             key is not certified, a wrong one fails"
                .to_owned(),
        );
    }
    if aggregates().any(|aggregate| aggregate.naming == Naming::Free) {
        out.push(
            "an output name the request leaves free is matched by its value under a key no other \
             output reserves; two such outputs are not associated"
                .to_owned(),
        );
    }
    let last = pipeline.steps.last().map(|step| &step.stages);
    match last.and_then(|stages| stages.sort.as_ref().map(|sort| (sort, stages.limit))) {
        None if pipeline.keep_order => {
            out.push("the rows keep the order of the source".to_owned());
        }
        None => out.push("no order is requested: the rows compare as a multiset".to_owned()),
        Some((sort, limit)) if !sort.stable_ties => {
            out.push("rows with equal sort keys may come in any order".to_owned());
            if limit.is_some() {
                out.push(
                    "a cut through tied rows may keep any of them; under a stated number policy \
                     a stop there is as valid"
                        .to_owned(),
                );
            }
        }
        Some(_) => out.push("rows with equal sort keys keep their file order".to_owned()),
    }
}

/// The interpretations a judgment of this obligation applies, for the preview.
fn assumptions(obligation: &Obligation) -> Vec<String> {
    let mut out = Vec::new();
    presence_assumptions(obligation, &mut out);
    let Requirement::Computed {
        source_format,
        pipeline,
        form: shape,
        ..
    } = &obligation.requirement
    else {
        return out;
    };
    out.push(
        "the judgment reads exactly the records the run consumed; a sample or a cut proves \
         nothing"
            .to_owned(),
    );
    out.push(
        "numbers compare by exact value (70 and 70.0 agree), never through a binary float; a \
         value rounds only where the request rounds, half away from zero"
            .to_owned(),
    );
    out.push("a missing field and null read the same".to_owned());
    let csv_target = obligation
        .target
        .as_ref()
        .is_some_and(|target| target.format == Format::Csv);
    if *source_format == Format::Csv || csv_target {
        out.push(
            "a CSV cell is text, read as nika:convert reads it; it agrees with a JSON value of \
             the same reading, a number by value"
                .to_owned(),
        );
    }
    relation_assumptions(pipeline, &mut out);
    let stated_form = match shape {
        Form::Totals => Some("the totals are one object, one key per total"),
        Form::Alone => {
            Some("the value is written alone: a bare JSON value, or the whole text of the file")
        }
        Form::Rows => None,
    };
    out.extend(stated_form.map(str::to_owned));
    out
}
