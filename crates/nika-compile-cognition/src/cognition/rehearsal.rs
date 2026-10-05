// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One compile's rehearsal journal. Only reports produced in this call can discharge its
//! final barrier. Replayed provenance is data, never evidence that the present world ran.

use nika_compile_fidelity::behavior::{
    Budget, Cause, Contract, Limits, Report, Run, RunEnd, Usage, judge,
};
use serde_json::{Value, json};

use crate::fidelity::Diagnostic;
use crate::rehearse::{Attempt, Rehearsal, RehearsalReport, Rehearse, judged_run};
use crate::{CompileOutcome, CompileRequest, CompileStatus, DiagnosticKind};

mod record;
#[cfg(test)]
mod tests;

/// What the author may do after a report has been checked.
#[derive(Clone, Debug)]
pub(super) enum Result {
    Proceed,
    Repair(Diagnostic),
    Stop(String),
}

/// The decision on a checked report (its structured, source-bound report is journaled in
/// `records`).
#[derive(Clone, Debug)]
pub(super) struct Verdict {
    pub result: Result,
}

/// A last accepted report from this invocation, never a persisted cache.
struct Checked {
    candidate: String,
    inputs: Vec<String>,
    targets: Vec<String>,
    verdict: Verdict,
    /// The judged run of that report, what this call had spent before it, and the room's bounds.
    run: Run,
    spent_before: Usage,
    room_bytes: u64,
}

/// The journal and consumption of one invocation. The native loop owns its repair limit;
/// this bridge adds no retry and the final barrier never repeats its identical last report.
pub(super) struct Rehearsals<'a> {
    host: Option<&'a dyn Rehearse>,
    last: Option<Checked>,
    records: Vec<Value>,
    usage: Usage,
    /// The compile this journal serves: the caller's own basis and request, which bind a
    /// semantic record before its paths are read, and the request the final barrier reads.
    serves: Option<Serves>,
}

/// The compile a journal serves (slice C): what binds a semantic record, what the barrier reads.
pub(super) struct Serves {
    pub(super) caller: Value,
    pub(super) raw: CompileRequest,
    pub(super) reading: CompileRequest,
}

impl<'a> Rehearsals<'a> {
    pub(super) fn new(host: Option<&'a dyn Rehearse>) -> Self {
        Self {
            host,
            last: None,
            records: Vec::new(),
            usage: Usage::default(),
            serves: None,
        }
    }

    /// The same journal, serving this compile.
    pub(super) fn serving(mut self, serves: Serves) -> Self {
        self.serves = Some(serves);
        self
    }

    /// The compile this journal serves, when its entry named one.
    pub(super) fn serves(&self) -> Option<&Serves> {
        self.serves.as_ref()
    }

    pub(super) fn offered(&self) -> bool {
        self.host.is_some()
    }

    /// The request and its admitted answers supply paths; candidate reads cannot add inputs.
    pub(super) async fn inspect(
        &mut self,
        request: &CompileRequest,
        out: &CompileOutcome,
    ) -> Verdict {
        if !self.offered() {
            return Verdict {
                result: Result::Proceed,
            };
        }
        let (inputs, targets) = match effective_paths(request, out) {
            Ok(paths) => paths,
            Err(why) => {
                return Verdict {
                    result: Result::Stop(why),
                };
            }
        };
        self.inspect_paths(
            out.candidate.as_deref().unwrap_or_default(),
            inputs,
            targets,
        )
        .await
    }

    async fn inspect_paths(
        &mut self,
        candidate: &str,
        inputs: Vec<String>,
        targets: Vec<String>,
    ) -> Verdict {
        let Some(host) = self.host else {
            return Verdict {
                result: Result::Proceed,
            };
        };
        let bound = host.bound();
        let report = host.rehearse_reading(candidate, &inputs, &targets).await;
        // Await the host's drain. Dropping this future at a second timeout could leave its
        // owned worker running; the host owns the run's timeout and cleanup.
        let declared = declared(&report);
        let run = judged_run("observed", &report, &inputs, &targets, &declared);
        let result = classify(candidate, &report, &run.end);
        let entry = record::report(&report, bound, &run, &result);
        let spent_before = self.usage;
        self.usage = self.usage.plus(&run.usage);
        self.records.push(entry);
        let verdict = Verdict { result };
        self.last = Some(Checked {
            candidate: candidate.to_owned(),
            inputs,
            targets,
            verdict: verdict.clone(),
            run,
            spent_before,
            room_bytes: report.observation.bounds.room_bytes,
        });
        verdict
    }

    /// Whether this call may rehearse once more under `attempts` rehearsals in all: checked
    /// before the host is asked, never after.
    pub(super) fn admits(&self, attempts: u32) -> bool {
        self.usage.fixtures < attempts
    }

    /// The behavioural judgment of this call's last run against `contract` (the request's own,
    /// never one the candidate states). The round admits that one run within the host's own
    /// bounds (its time bound, twice its room); the turn admits `attempts` such runs (`None`:
    /// the runs spent before it and this one). The run was charged once when it ran: the turn
    /// resumes from what was spent before it.
    pub(super) fn judged(&self, contract: &Contract, attempts: Option<u32>) -> Option<Report> {
        let last = self.last.as_ref()?;
        let host = self.host?;
        let attempts = attempts.unwrap_or_else(|| last.spent_before.fixtures.saturating_add(1));
        let time = u64::try_from(host.bound().as_millis()).unwrap_or(u64::MAX);
        let bytes = last.room_bytes.saturating_mul(2);
        let round = Limits::new(1, 1, bytes, time);
        let turn = Limits::new(
            attempts,
            attempts,
            bytes.saturating_mul(u64::from(attempts)),
            time.saturating_mul(u64::from(attempts)),
        );
        let mut budget = Budget::new(round, turn, last.spent_before);
        Some(judge(
            contract,
            std::slice::from_ref(&last.run),
            &mut budget,
        ))
    }

    /// Every returned Ready candidate, including COLD and replay, faces the same barrier on
    /// its final bytes. It may reuse only this call's last report over the same request paths.
    pub(super) async fn finish(&mut self, request: &CompileRequest, out: &mut CompileOutcome) {
        if !self.offered() {
            return;
        }
        if out.status == CompileStatus::Ready && !ready(out) {
            withdraw(
                out,
                "A ready candidate has no clean Check to rehearse.".to_owned(),
            );
        }
        if ready(out) {
            let paths = effective_paths(request, out);
            let verdict = match paths {
                Ok((inputs, targets)) => {
                    let candidate = out.candidate.as_deref().unwrap_or_default();
                    let prior = self.last.as_ref().filter(|last| {
                        last.candidate == candidate
                            && last.inputs == inputs
                            && last.targets == targets
                    });
                    if let Some(last) = prior {
                        last.verdict.clone()
                    } else {
                        self.inspect_paths(candidate, inputs, targets).await
                    }
                }
                Err(why) => Verdict {
                    result: Result::Stop(why),
                },
            };
            match verdict.result {
                Result::Proceed => {}
                Result::Repair(diagnostic) => withdraw(out, diagnostic.message),
                Result::Stop(reason) => withdraw(out, reason),
            }
        }
        let mut decision = out.provenance.decision.take().unwrap_or_else(|| json!({}));
        decision["rehearsal"] = json!({
            "version": 1,
            "scope": "this compile invocation",
            "reports": self.records,
            "usage": record::usage(&self.usage),
        });
        out.provenance.decision = Some(decision);
    }
}

/// A candidate being present is insufficient: finish also stores candidates that Check refused.
pub(super) fn ready(out: &CompileOutcome) -> bool {
    out.status == CompileStatus::Ready
        && out.candidate.is_some()
        && out
            .check_preview
            .as_ref()
            .is_some_and(|preview| preview.report.is_clean())
}

/// The core owns answer application. Reconstruct its result for this candidate instead of
/// reading a saved inventory or copying whatever paths the candidate happens to require.
fn effective_paths(
    request: &CompileRequest,
    out: &CompileOutcome,
) -> std::result::Result<(Vec<String>, Vec<String>), String> {
    let intent = intent_of(request);
    let mut inputs = nika_compile::stated_sources(&intent);
    let mut targets = nika_compile::stated_destinations(&intent);
    if matches!(&request.input, crate::types::Input::Create(_)) {
        let native = out.provenance.plan.as_ref().filter(|record| {
            record["strategy"] == "native" || record.get("semantic_record").is_some()
        });
        if let Some(record) = native {
            let paths = nika_compile::surface::native_answered_paths(
                record,
                request,
                out.candidate.as_deref().unwrap_or_default(),
            )
            .ok_or_else(|| {
                "The current native answers do not bind these candidate paths.".to_owned()
            })?;
            extend_paths(&mut inputs, paths.reads());
            extend_paths(&mut targets, paths.writes());
        } else if out.provenance.strategy == Some(crate::Strategy::Native) {
            return Err("The native candidate has no current answer record.".to_owned());
        } else if let Some(record) = (out.provenance.plan.as_ref())
            .filter(|record| record["strategy"] == "cold" && !request.answers.is_empty())
        {
            let candidate = out.candidate.as_deref().unwrap_or_default();
            let (reads, writes) =
                cold_answered_paths(record, request, candidate).ok_or_else(|| {
                    "The current answers do not rebuild these candidate bytes and their paths."
                        .to_owned()
                })?;
            extend_paths(&mut inputs, &reads);
            extend_paths(&mut targets, &writes);
        }
    }
    Ok((inputs, targets))
}

/// The answers a COLD plan's candidate uses as file paths, by role (reads, writes). The
/// deterministic compiler must rebuild exactly these clean-checked bytes from the plan and the
/// answers (the barrier holds the final outcome Ready; a clause this round's judge settled
/// leaves the rebuild pending, never other bytes). An answer is a path only where the inferred
/// permits move with it (the same rebuild with that answer replaced reads or writes elsewhere),
/// in each role it moves. A saved path list, a constant merely present and a content answer that
/// equals a path grant nothing; an empty, glob, absolute or escaping path refuses.
fn cold_answered_paths(
    record: &Value,
    request: &CompileRequest,
    candidate: &str,
) -> Option<(Vec<String>, Vec<String>)> {
    let rebuild = |request: CompileRequest| {
        let out = nika_compile::compile(&request.with_plan(record.clone())).ok()?;
        let clean = (out.check_preview.as_ref()).is_some_and(|p| p.report.is_clean());
        let source = out
            .candidate
            .filter(|_| out.status != CompileStatus::Refused && clean)?;
        let fs = nika_check::infer_permits(&nika_compile::parse(&source).ok()?)
            .permits
            .fs;
        Some((source, fs.map(|fs| (fs.read, fs.write)).unwrap_or_default()))
    };
    let (source, (reads, writes)) = rebuild(request.clone())?;
    if source != candidate {
        return None;
    }
    let (mut read, mut write) = (Vec::new(), Vec::new());
    for (key, answer) in &request.answers {
        if !key.starts_with("const.") || serde_json::from_str::<String>(answer).is_err() {
            continue;
        }
        // An answer the probe cannot replace (a rebuild that no longer settles) binds no path.
        let probe = json!("./nika-rehearsal-answer-probe.txt").to_string();
        let Some((_, (moved_reads, moved_writes))) = rebuild(request.clone().answer(key, probe))
        else {
            continue;
        };
        for (paths, moved, side) in [
            (&reads, moved_reads, &mut read),
            (&writes, moved_writes, &mut write),
        ] {
            let bound: Vec<String> = (paths.iter())
                .filter(|path| !moved.contains(path))
                .cloned()
                .collect();
            // Bound as a path the survey cannot name (an absolute one): refused, never dropped.
            if bound.is_empty() && moved.iter().any(|path| !paths.contains(path)) {
                return None;
            }
            side.extend(bound);
        }
    }
    let unsafe_path = |path: &String| {
        path.is_empty()
            || path.contains(['*', '?', '['])
            || std::path::Path::new(path).components().any(|c| {
                !matches!(
                    c,
                    std::path::Component::Normal(_) | std::path::Component::CurDir
                )
            })
    };
    (!read.iter().chain(&write).any(unsafe_path)).then_some((read, write))
}

fn extend_paths(paths: &mut Vec<String>, added: &[String]) {
    for path in added {
        if !paths.contains(path) {
            paths.push(path.clone());
        }
    }
}

fn intent_of(request: &CompileRequest) -> String {
    match &request.input {
        crate::types::Input::Create(intent) => request
            .answers
            .get("intent.clarification")
            .and_then(|answer| serde_json::from_str::<String>(answer).ok())
            .unwrap_or_else(|| intent.clone()),
        crate::types::Input::Edit { .. } => crate::revise_intent(request).unwrap_or_default(),
        _ => String::new(),
    }
}

/// The host owns output discovery. Its reported output inventory must have corresponding
/// final observations; the request's independent targets are checked alongside that inventory.
fn declared(report: &RehearsalReport) -> Vec<String> {
    match &report.outcome {
        Rehearsal::Passed { outputs } => outputs.iter().map(|output| output.path.clone()).collect(),
        Rehearsal::Missing { outputs } => outputs.clone(),
        _ => Vec::new(),
    }
}

fn classify(candidate: &str, report: &RehearsalReport, end: &RunEnd) -> Result {
    let invalid = if report.candidate_sha256 != super::knowledge::sha256(candidate) {
        Some("the rehearsal report names another candidate".to_owned())
    } else if !matches!(report.attempt, Attempt::NeverAttempted)
        && report.admitted_digest.is_empty()
    {
        Some("the rehearsal began without an admitted digest".to_owned())
    } else if let RunEnd::InvalidHarness { reason } = end {
        Some(reason.clone())
    } else {
        None
    };
    if let Some(reason) = invalid {
        return Result::Stop(format!("Invalid rehearsal evidence: {reason}"));
    }
    if let RunEnd::Failed(failure) = end
        && matches!(failure.cause, Cause::Engine)
    {
        return Result::Stop("The rehearsal engine failed; the candidate is not ready and no author repair is requested.".to_owned());
    }
    if let RunEnd::Failed(failure) = end
        && matches!(failure.cause, Cause::TimeBound)
    {
        return Result::Repair(Diagnostic {
            kind: "rehearsal_time_bound",
            message: json!({"kind": "run_failure", "cause": "time_bound"}).to_string(),
        });
    }
    // Missing is Completed for the behavioural adapter, not a successful native round.
    // Only a coherent refusal before any attempt remains source-only NotRun.
    match &report.outcome {
        Rehearsal::Missing { outputs } => Result::Repair(Diagnostic {
            kind: "rehearsal_missing",
            message: json!({"kind": "missing_outputs", "outputs": outputs}).to_string(),
        }),
        Rehearsal::Failed {
            code,
            task,
            message,
        } => Result::Repair(Diagnostic {
            kind: "rehearsal_failed",
            message: json!({"kind": "run_failure", "task": task, "code": code, "message": message})
                .to_string(),
        }),
        Rehearsal::NotRun { .. }
            if matches!(report.attempt, Attempt::NeverAttempted)
                && matches!(end, RunEnd::NotRun { .. }) =>
        {
            Result::Proceed
        }
        Rehearsal::NotRun { .. } => Result::Stop(
            "The rehearsal did not establish a coherent refusal before an attempt.".to_owned(),
        ),
        Rehearsal::Passed { .. } if matches!(end, RunEnd::Completed) => Result::Proceed,
        Rehearsal::Passed { .. } => Result::Stop(
            "The rehearsal claimed success without a completed, effect-free observation."
                .to_owned(),
        ),
    }
}

fn withdraw(out: &mut CompileOutcome, reason: String) {
    out.status = CompileStatus::Incomplete;
    out.candidate = None;
    out.check_preview = None;
    out.requested_boundary = None;
    out.provenance.plan = None;
    crate::finding(out, DiagnosticKind::Unknown, "rehearsal", reason);
}
