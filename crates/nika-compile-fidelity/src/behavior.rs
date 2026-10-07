// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The behavioural contract a request states, and its judgment over one round of rehearsals.
//!
//! A [`Contract`] ([`contract_of`]) comes only from requested and canonical facts: the reader's
//! typed plan of the request (its operations, effects and unknowns, a value written alone, the
//! typed fields of each rule), the paths the request names and the answers the human gave. It
//! never reads a candidate: no jq, no task, no intermediate format and no wrapper the candidate
//! chose. Each [`Obligation`] names where a result lands, whether the request wants it written,
//! what it must hold, and the request's words it comes from. A fact the plan cannot state stays
//! open: a write the plan cannot prove unconditional is [`Presence::Unproven`], an output name it
//! cannot trace to the request is [`Naming::Unknown`] (never assumed free), and requested work
//! this component cannot verify is an explicit [`Requirement::Unsupported`] obligation.
//! [`contract_of_request`] proves more only for a sentence of a small closed language (one read,
//! one equality filter or its count, one direct write; or one text file copied as is; identities
//! kept byte for byte) that the strict HOT door admits and that the whole request is
//! ([`Provenance`]): its write is [`Presence::Required`], its count's name free or stated where
//! the sentence labels it, and a copy holds exactly the text of its source
//! ([`Requirement::CopyText`]).
//!
//! [`judge`] establishes what each rehearsal can show before reading its end: a contradictory
//! observation, a source outside the stated domain or a missing receipt is an invalid harness,
//! whatever the run did. It then classifies the end once against the end the contract predicts
//! on the records the run consumed, and reads every obligation through that class. An
//! established cause (an engine failure, a file the request never names) is a defect even where
//! a stated rule predicts a stop. A stop is never certified: a structured terminal fact
//! ([`StopFact`]) consistent with the stop a stated rule predicts names its operation, not the
//! data and policy that governed it, so it stays unattested; an error code or message is never
//! a stop, and no stop hides a wrong value or a forbidden write already observed. Values are read by
//! the canonical readings (exact JSON numbers, the `nika:convert` CSV reading), bound to the
//! sha256 of the exact bytes read, and compared by value. No outcome is ordered above another:
//! each [`Judged`] obligation and the [`Report`] carry a [`Tally`], and their [`Verdict`] states
//! its dominance explicitly, an invalid harness first, while [`Report::failed`] keeps the
//! defects the valid fixtures showed.
//!
//! Pure: no file, process, clock or provider. The host runs each rehearsal in its room, stops
//! it at the budget, and hands over the bytes it consumed and read back ([`Run`]).
//!
//! [`select`] judges several candidates, however differently written, against one contract the
//! request alone states: each [`Candidate`] is its runs and the identity of its bytes, judged as
//! one round of the same turn's [`Budget`], and the first certified one is selected. A defect,
//! an open case, a fixture not run or the absence of a defect never selects; a spent turn says
//! so ([`Choice::Spent`]). It runs nothing and verifies no identity: its turn counts only the
//! candidates it judged, so a door rehearses and judges one candidate at a time, and a selected
//! index is a position the door keeps bound to the bytes rehearsed. [`targets`] names the paths
//! a host reads back, whatever a candidate declares.
//!
//! What the relation evaluates: filters (the reader's comparators, junctions, text spellings,
//! column comparisons and answered values), duplicates by key, groups with sum, count,
//! average, minimum and maximum, totals, a sort with or without the tie rule, the first N rows,
//! projection with number columns, renames, duplicates, and the steps a rule runs after its
//! first one. A join, rows per group with no named aggregate, outputs defined as arithmetic
//! over aggregates, a seat-written program, the lines of a text source, a step after totals or
//! after a cut through ties, and every value whose reading the request leaves open are
//! explicit unverified cases.

use std::collections::BTreeMap;

use nika_compile_reader::rules::NumberPolicy;

mod accounting;
mod composed;
mod evaluate;
mod formats;
mod numbers;
mod pipeline;
mod provenance;
mod requested;
mod selection;
mod values;
mod verdicts;

pub use accounting::{Admission, Axis, Budget, Limits, Usage};
pub use formats::{Coverage, Format};
pub use numbers::Decimal;
pub use pipeline::{
    Aggregate, Arith, Derived, Filter, Naming, OnEmpty, Operand, Pipeline, Sort, Stages, Step,
    Term, Test,
};
pub use provenance::{Production, Provenance, Written, contract_of_request, read_request};
pub use requested::{contract_of, pipeline_of};
pub use selection::{Candidate, Choice, Ruling, select, targets};
pub use verdicts::judge;

/// Two paths name the same file: a leading `./` names the same relative path.
pub(crate) fn same_path(left: &str, right: &str) -> bool {
    left.trim_start_matches("./") == right.trim_start_matches("./")
}

/// Where a requested result lands: a file the request names, read in the format its extension
/// names.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Target {
    pub path: String,
    pub format: Format,
}

impl Target {
    /// The file at `path`, read in `format`.
    #[must_use]
    pub fn new(path: impl Into<String>, format: Format) -> Self {
        Self {
            path: path.into(),
            format,
        }
    }
}

/// A condition on the records a run consumed from one source.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Condition {
    pub source: String,
    pub format: Format,
    pub filter: Filter,
    pub policies: BTreeMap<String, NumberPolicy>,
    /// The condition holds when some record passes the filter; `false`: when none does.
    pub when_any: bool,
}

impl Condition {
    /// Some (`when_any`) or no record of `source` passes `filter`.
    #[must_use]
    pub fn new(source: impl Into<String>, format: Format, filter: Filter, when_any: bool) -> Self {
        Self {
            source: source.into(),
            format,
            filter,
            policies: BTreeMap::new(),
            when_any,
        }
    }
}

/// Whether the request wants the result written.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Presence {
    /// Written, unconditionally, as a lower layer proves it.
    Required,
    /// The request asks for the write, and the plan cannot state whether a condition governs
    /// it: a written result must hold the requested value, but neither a write nor its absence
    /// certifies anything.
    Unproven,
    /// Never written.
    Forbidden,
    /// Written only after a human approves: a rehearsal never takes that decision.
    Approval,
    /// The request leaves the write undecided, states it and its prohibition at once, or
    /// carries policy words the plan does not type.
    Undecided,
    /// Required when the condition holds; when it does not, a written result must still hold
    /// the requested value.
    When(Condition),
    /// Required when the condition holds, and absent when it does not.
    OnlyWhen(Condition),
}

/// The form a result takes in its file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Form {
    /// Rows: a JSON array of objects, one object per line, or a CSV table.
    Rows,
    /// Totals over every row: one JSON object keyed by their names.
    Totals,
    /// One value alone, as the plan states it: a bare JSON value, or the value as the whole text
    /// of the file.
    Alone,
}

/// What a result must hold.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Requirement {
    /// The request states whether the file is written, and nothing of its content.
    PresenceOnly,
    /// The result of `pipeline` over the records the run consumed from `source`.
    Computed {
        source: String,
        source_format: Format,
        pipeline: Pipeline,
        form: Form,
    },
    /// Exactly the text the run consumed from `source`, byte for byte: a text file copied as is.
    CopyText { source: String },
    /// A requested result this component cannot verify, and why: never passed.
    Unsupported(String),
}

/// One requested result.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Obligation {
    /// A stable name, such as `write ./out/top.json`.
    pub id: String,
    /// `None` when the request names no file for the result.
    pub target: Option<Target>,
    pub presence: Presence,
    pub requirement: Requirement,
    /// The request's own words the obligation comes from.
    pub evidence: String,
}

impl Obligation {
    /// One requested result.
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        target: Option<Target>,
        presence: Presence,
        requirement: Requirement,
        evidence: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            target,
            presence,
            requirement,
            evidence: evidence.into(),
        }
    }
}

/// Every result a request states, independent of any candidate.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Contract {
    pub obligations: Vec<Obligation>,
    /// The source files the request names: a fixture without one of them is invalid.
    pub sources: Vec<String>,
}

impl Contract {
    /// The contract of these obligations, naming no source beyond theirs.
    #[must_use]
    pub fn new(obligations: Vec<Obligation>) -> Self {
        Self {
            obligations,
            sources: Vec::new(),
        }
    }

    /// The same contract, naming these source files.
    #[must_use]
    pub fn with_sources(mut self, sources: Vec<String>) -> Self {
        self.sources = sources;
        self
    }
}

/// The operation of the requested relation that read the value a stop names, as the contract
/// names it: never a task, an order of tasks or the structure of a workflow.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Operation {
    /// A test of the condition that governs a write.
    Condition,
    /// A test of a filter of the relation.
    Test,
    /// An aggregate: a sum, an average, a minimum or a maximum.
    Aggregate,
    /// The ranking of a sort, and the cut through it.
    Rank,
    /// A column the request writes as a number.
    Column,
}

/// What stopped the operation.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum StopReason {
    /// A value the number law does not read: its JSON text, `None` for null or a missing
    /// field.
    NotANumber { value: Option<String> },
    /// No value is a number where a stated policy needs one.
    NoNumber,
    /// A cut through distinct tied rows, bound by a stated policy, has no answer.
    TiedCut,
}

/// The structured terminal fact of a stop, in the contract's terms: the source whose records
/// the operation read, the operation, the field and the reason. The words or the code of an
/// error are never such a fact. It names no data or policy independent of the workflow, so a
/// fact consistent with a predicted stop stays unattested: another occurrence of the same
/// operation on the same records would report the same fact.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct StopFact {
    pub source: String,
    pub operation: Operation,
    pub field: String,
    pub reason: StopReason,
}

impl StopFact {
    /// The stop of `operation` on `field` of the records read from `source`, for `reason`.
    #[must_use]
    pub fn new(
        source: impl Into<String>,
        operation: Operation,
        field: impl Into<String>,
        reason: StopReason,
    ) -> Self {
        Self {
            source: source.into(),
            operation,
            field: field.into(),
            reason,
        }
    }
}

/// Why a run failed, as the host establishes it from the runtime's structured records.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Cause {
    /// A stop, as its structured fact records it.
    Stop(StopFact),
    /// The host's time bound or cancellation ended the run.
    TimeBound,
    /// The engine failed, and the host observed that failure on a valid fixture: a functional
    /// failure. A host failure that leaves no valid observation is
    /// [`RunEnd::InvalidHarness`].
    Engine,
    /// A file the run opened is absent from the room: a fact of the run, never by itself of the
    /// fixture. For a source the request names, a whole recorded copy means the run lost it (a
    /// failure of the run); no record or a partial one settles nothing. A fixture that lacked it
    /// is the host's to attest, with [`RunEnd::InvalidHarness`].
    MissingFile { path: String },
    /// A document the run read could not be parsed as it read it.
    Unparsable { path: String },
    /// Any other failure: no structured fact ties it to a stated stop.
    Unclassified,
}

/// One failed run.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Failure {
    /// The task that failed.
    pub task: String,
    /// The error code the runtime reported: evidence only, never a verdict.
    pub code: String,
    pub cause: Cause,
}

impl Failure {
    /// One failure of `task` with `code`, for `cause`.
    #[must_use]
    pub fn new(task: impl Into<String>, code: impl Into<String>, cause: Cause) -> Self {
        Self {
            task: task.into(),
            code: code.into(),
            cause,
        }
    }
}

/// How a run ended, as the host reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RunEnd {
    /// The run completed by itself.
    Completed,
    /// The run failed.
    Failed(Failure),
    /// The host did not complete a run it can vouch for: never attempted, stopped at its bound,
    /// cancelled, or an effect its seams denied.
    NotRun { reason: String },
    /// The host could not build a valid observation: the fixture or its evidence is invalid.
    InvalidHarness { reason: String },
}

/// What a run consumed from one source: the exact text the host copied into its room.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Consumed {
    pub path: String,
    pub text: String,
    pub coverage: Coverage,
}

impl Consumed {
    /// The text the run consumed at `path`, and how much of it the evidence holds.
    #[must_use]
    pub fn new(path: impl Into<String>, text: impl Into<String>, coverage: Coverage) -> Self {
        Self {
            path: path.into(),
            text: text.into(),
            coverage,
        }
    }
}

/// What the host read back at one path after a run: one final reading per file. A run that
/// started carries one for every path the contract names as a result, written or not: a path
/// with no read-back has no observation, and a missing receipt is never a non-write.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ReadBack {
    pub path: String,
    pub text: String,
    /// Whether the run itself wrote the path (a file copied into the room is no write).
    pub written: bool,
    /// Whether the host cut `text` at its byte bound.
    pub truncated: bool,
}

impl ReadBack {
    /// A file the run wrote, read back whole.
    #[must_use]
    pub fn new(path: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            text: text.into(),
            written: true,
            truncated: false,
        }
    }

    /// The host's observation that the run did not write `path`.
    #[must_use]
    pub fn unwritten(path: impl Into<String>) -> Self {
        Self::new(path, "").with_written(false)
    }

    /// The same read-back, marked as written by the run or not.
    #[must_use]
    pub fn with_written(mut self, written: bool) -> Self {
        self.written = written;
        self
    }

    /// The same read-back, marked as cut at the host's byte bound or not.
    #[must_use]
    pub fn with_truncated(mut self, truncated: bool) -> Self {
        self.truncated = truncated;
        self
    }
}

/// One rehearsal of the round: the observed world or one fixture made from it. Each path
/// appears once in `consumed` and once in `read_back`; two different observations of one path
/// make the fixture invalid.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Run {
    /// The fixture's name, such as `observed` or the name of a variant.
    pub fixture: String,
    pub end: RunEnd,
    pub consumed: Vec<Consumed>,
    pub read_back: Vec<ReadBack>,
    /// What this fixture spent of the budget.
    pub usage: Usage,
}

impl Run {
    /// One rehearsal that consumed and wrote nothing yet.
    #[must_use]
    pub fn new(fixture: impl Into<String>, end: RunEnd, usage: Usage) -> Self {
        Self {
            fixture: fixture.into(),
            end,
            consumed: Vec::new(),
            read_back: Vec::new(),
            usage,
        }
    }

    /// The same rehearsal, with what it consumed from one source.
    #[must_use]
    pub fn with_consumed(mut self, consumed: Consumed) -> Self {
        self.consumed.push(consumed);
        self
    }

    /// The same rehearsal, with one path the host read back.
    #[must_use]
    pub fn with_read_back(mut self, read_back: ReadBack) -> Self {
        self.read_back.push(read_back);
        self
    }
}

/// How one obligation ended on one fixture. No outcome is ordered above another: a tally keeps
/// them apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Outcome {
    /// The fixture showed the requested result.
    Passed,
    /// Unsupported or unverified: the request leaves the case open, the operation is not
    /// evaluated, the evidence is a sample or cut, or no structured fact attests a stop.
    Incomplete,
    /// Nothing ran that can show it.
    NotRun,
    /// The fixture or its observation is invalid: no verdict on the program.
    InvalidHarness,
    /// The fixture showed a result, or an end, the request does not allow.
    Failed,
}

/// How many findings ended in each outcome.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Tally {
    pub passed: usize,
    pub incomplete: usize,
    pub not_run: usize,
    pub invalid_harness: usize,
    pub failed: usize,
}

impl Tally {
    /// The tally of these outcomes.
    #[must_use]
    pub fn of(outcomes: impl IntoIterator<Item = Outcome>) -> Self {
        let mut tally = Self::default();
        for outcome in outcomes {
            match outcome {
                Outcome::Passed => tally.passed += 1,
                Outcome::Incomplete => tally.incomplete += 1,
                Outcome::NotRun => tally.not_run += 1,
                Outcome::InvalidHarness => tally.invalid_harness += 1,
                Outcome::Failed => tally.failed += 1,
            }
        }
        tally
    }

    /// Both tallies added.
    #[must_use]
    pub fn plus(&self, other: &Self) -> Self {
        Self {
            passed: self.passed + other.passed,
            incomplete: self.incomplete + other.incomplete,
            not_run: self.not_run + other.not_run,
            invalid_harness: self.invalid_harness + other.invalid_harness,
            failed: self.failed + other.failed,
        }
    }

    /// How many findings in all.
    #[must_use]
    pub fn total(&self) -> usize {
        self.passed + self.incomplete + self.not_run + self.invalid_harness + self.failed
    }

    /// The conclusion these findings support, by an explicit dominance: an invalid harness
    /// first, then a defect, then a fixture that did not run, then an open case; certified only
    /// when every one of at least one finding passed.
    #[must_use]
    pub fn verdict(&self) -> Verdict {
        if self.invalid_harness > 0 {
            Verdict::InvalidHarness
        } else if self.failed > 0 {
            Verdict::Defective
        } else if self.not_run > 0 || self.total() == 0 {
            Verdict::NotRun
        } else if self.incomplete > 0 {
            Verdict::Incomplete
        } else {
            Verdict::Certified
        }
    }
}

/// The conclusion a tally supports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Verdict {
    /// At least one finding, and every one passed.
    Certified,
    /// Nothing failed, but some case stays open.
    Incomplete,
    /// Nothing failed, but some fixture did not run, or none did.
    NotRun,
    /// A valid fixture showed a defect.
    Defective,
    /// Some fixture or observation is invalid: no qualified conclusion, whatever else holds.
    InvalidHarness,
}

/// The identity of bytes a finding read.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Identity {
    pub path: String,
    /// The lowercase hex sha256 of the bytes read.
    pub sha256: String,
    pub coverage: Coverage,
    /// How many records or rows the reading found, when it read any.
    pub records: Option<usize>,
}

impl Identity {
    /// The identity of `bytes` read at `path`.
    #[must_use]
    pub fn new(path: impl Into<String>, bytes: &[u8], coverage: Coverage) -> Self {
        Self {
            path: path.into(),
            sha256: formats::sha256_hex(bytes),
            coverage,
            records: None,
        }
    }
}

/// One obligation judged on one fixture: the observed proof.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Finding {
    pub fixture: String,
    pub outcome: Outcome,
    /// What the fixture showed, in words.
    pub observed: String,
    pub evidence: Vec<Identity>,
}

impl Finding {
    /// One finding.
    #[must_use]
    pub fn new(
        fixture: impl Into<String>,
        outcome: Outcome,
        observed: impl Into<String>,
        evidence: Vec<Identity>,
    ) -> Self {
        Self {
            fixture: fixture.into(),
            outcome,
            observed: observed.into(),
            evidence,
        }
    }
}

/// One obligation judged over the round.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Judged {
    pub obligation: String,
    /// The requested result, in words.
    pub requested: String,
    /// The interpretations the judgment applied.
    pub assumptions: Vec<String>,
    /// The observed proof, fixture by fixture.
    pub findings: Vec<Finding>,
    /// How the findings ended.
    pub tally: Tally,
}

impl Judged {
    /// One judged obligation; its tally counts `findings`.
    #[must_use]
    pub fn new(
        obligation: impl Into<String>,
        requested: impl Into<String>,
        assumptions: Vec<String>,
        findings: Vec<Finding>,
    ) -> Self {
        let tally = Tally::of(findings.iter().map(|finding| finding.outcome));
        Self {
            obligation: obligation.into(),
            requested: requested.into(),
            assumptions,
            findings,
            tally,
        }
    }

    /// The conclusion of this obligation over the round.
    #[must_use]
    pub fn verdict(&self) -> Verdict {
        self.tally.verdict()
    }
}

/// The judgment of one round.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Report {
    pub judged: Vec<Judged>,
    /// What the round spent.
    pub round: Usage,
    /// What the turn spent, this round included.
    pub turn: Usage,
    /// Whether the budget admits another fixture.
    pub admission: Admission,
}

impl Report {
    /// One round's judgment.
    #[must_use]
    pub fn new(judged: Vec<Judged>, round: Usage, turn: Usage, admission: Admission) -> Self {
        Self {
            judged,
            round,
            turn,
            admission,
        }
    }

    /// Every finding of the round, counted.
    #[must_use]
    pub fn tally(&self) -> Tally {
        self.judged
            .iter()
            .fold(Tally::default(), |sum, judged| sum.plus(&judged.tally))
    }

    /// The round's conclusion; `Incomplete` when no obligation exists (a round with nothing to
    /// judge proves nothing).
    #[must_use]
    pub fn verdict(&self) -> Verdict {
        if self.judged.is_empty() {
            Verdict::Incomplete
        } else {
            self.tally().verdict()
        }
    }

    /// Whether the round certifies the request: at least one obligation, every finding passed.
    #[must_use]
    pub fn certified(&self) -> bool {
        self.verdict() == Verdict::Certified
    }

    /// Whether some valid fixture showed a defect, even when another fixture is invalid.
    #[must_use]
    pub fn failed(&self) -> bool {
        self.tally().failed > 0
    }

    /// Whether some fixture or observation is invalid.
    #[must_use]
    pub fn harness_invalid(&self) -> bool {
        self.tally().invalid_harness > 0
    }

    /// Whether the round may feed a qualified score: no fixture or observation is invalid.
    #[must_use]
    pub fn scorable(&self) -> bool {
        !self.harness_invalid()
    }
}

#[cfg(test)]
mod numbers_tests;

#[cfg(test)]
mod formats_tests;

#[cfg(test)]
mod relation_tests;

#[cfg(test)]
mod judge_tests;

#[cfg(test)]
mod presence_tests;

#[cfg(test)]
mod stop_tests;

#[cfg(test)]
mod names_tests;

#[cfg(test)]
mod tally_tests;

#[cfg(test)]
mod derive_tests;

#[cfg(test)]
mod provenance_tests;

#[cfg(test)]
mod selection_tests;

#[cfg(test)]
mod copy_tests;

#[cfg(test)]
mod composed_tests;
