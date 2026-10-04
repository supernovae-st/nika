// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One compile's rehearsal journal. Only reports produced in this call can discharge its
//! final barrier. Replayed provenance is data, never evidence that the present world ran.

use nika_compile_fidelity::behavior::{Cause, RunEnd, Usage};
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

/// The same decision beside its structured, source-bound report.
#[derive(Clone, Debug)]
pub(super) struct Verdict {
    pub result: Result,
    pub record: Value,
}

/// A last accepted report from this invocation, never a persisted cache.
struct Checked {
    candidate: String,
    inputs: Vec<String>,
    targets: Vec<String>,
    verdict: Verdict,
}

/// The journal and consumption of one invocation. The native loop owns its repair limit;
/// this bridge adds no retry and the final barrier never repeats its identical last report.
pub(super) struct Rehearsals<'a> {
    host: Option<&'a dyn Rehearse>,
    last: Option<Checked>,
    records: Vec<Value>,
    usage: Usage,
}

impl<'a> Rehearsals<'a> {
    pub(super) fn new(host: Option<&'a dyn Rehearse>) -> Self {
        Self {
            host,
            last: None,
            records: Vec::new(),
            usage: Usage::default(),
        }
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
                record: Value::Null,
            };
        }
        let (inputs, targets) = match effective_paths(request, out) {
            Ok(paths) => paths,
            Err(why) => {
                return Verdict {
                    result: Result::Stop(why),
                    record: Value::Null,
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
                record: Value::Null,
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
        self.usage = self.usage.plus(&run.usage);
        self.records.push(entry.clone());
        let verdict = Verdict {
            result,
            record: entry,
        };
        self.last = Some(Checked {
            candidate: candidate.to_owned(),
            inputs,
            targets,
            verdict: verdict.clone(),
        });
        verdict
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
                    record: Value::Null,
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
        }
    }
    Ok((inputs, targets))
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
