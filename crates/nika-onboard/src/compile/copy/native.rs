// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One synchronous compile dispatch owns this account. A host call is admitted before it
//! starts, drained before its usage is charged, and never grants Save or Run. Its live preview
//! is taken once by the proposal boundary; serialized compiler records cannot recreate it.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use super::{Admission, Allowance, Budget, RunEnd, Usage, Witness, WorldBefore};
use nika_compile_cognition::rehearse::{
    Attempt, EffectCounts, Observation, Refusal, Rehearsal, RehearsalFuture, RehearsalReport,
    Rehearse, judged_run,
};
use nika_event::source_id::sha256_hex;

/// Evidence produced in this dispatch only, with no public constructor or writable fields.
/// A completed result contains a checked world witness; a source-only refusal contains none.
#[non_exhaustive]
pub struct Preview {
    candidate_sha256: String,
    witness: Option<Witness>,
    lines: String,
}

impl Preview {
    /// The exact candidate this live decision names.
    #[must_use]
    pub fn candidate_sha256(&self) -> &str {
        &self.candidate_sha256
    }

    /// Consume the live evidence and its words. The caller owns proposal identity and consent.
    #[must_use]
    pub fn into_parts(self) -> (Option<Witness>, String) {
        (self.witness, self.lines)
    }
}

/// The drained dispatch's consumption, last live evidence and any fail-closed account refusal.
#[non_exhaustive]
pub struct Settled {
    turn: Usage,
    preview: Option<Preview>,
    blocked: Option<String>,
}

impl Settled {
    /// Consume the account. Charge its usage even if the caller's compilation failed.
    /// A blocked account discards its preview instead of exposing evidence beside a refusal.
    #[must_use = "charge the returned usage and handle refusals before using a preview"]
    pub fn into_parts(self) -> (Usage, Result<Option<Preview>, String>) {
        let preview = match self.blocked {
            Some(why) => Err(why),
            None => Ok(self.preview),
        };
        (self.turn, preview)
    }
}

struct State {
    turn: Usage,
    in_flight: bool,
    preview: Option<Preview>,
    blocked: Option<String>,
}

/// One compile dispatch's serialized rehearsal account over an explicitly supplied host.
/// It owns no proposal, consent, source basis, or persistent proof.
#[non_exhaustive]
pub struct Scoped {
    root: PathBuf,
    host: Box<dyn Rehearse>,
    allowance: Allowance,
    state: Mutex<State>,
}

impl Scoped {
    /// Start with the caller's existing turn consumption and round/turn limits.
    #[must_use]
    pub fn new(root: PathBuf, host: Box<dyn Rehearse>, allowance: Allowance) -> Self {
        Self {
            root,
            host,
            allowance,
            state: Mutex::new(State {
                turn: allowance.before,
                in_flight: false,
                preview: None,
                blocked: None,
            }),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        match self.state.lock() {
            Ok(state) => state,
            Err(poisoned) => {
                let mut state = poisoned.into_inner();
                state.blocked = Some("the rehearsal account was interrupted".to_owned());
                state
            }
        }
    }

    fn begin(&self) -> Result<Budget, String> {
        let mut state = self.state();
        state.preview = None;
        if let Some(reason) = &state.blocked {
            return Err(reason.clone());
        }
        if state.in_flight {
            let why = "another rehearsal in this dispatch has not drained".to_owned();
            state.blocked = Some(why.clone());
            return Err(why);
        }
        // Each native candidate has one observed world. The turn is carried, never reset.
        let account = Budget::new(self.allowance.round, self.allowance.turn, state.turn);
        if account.admission() != Admission::Open {
            let why = "the shared turn rehearsal budget admits no further world".to_owned();
            state.blocked = Some(why.clone());
            return Err(why);
        }
        state.in_flight = true;
        Ok(account)
    }

    /// Consume this dispatch after compilation; an in-flight or poisoned account is blocked.
    #[must_use]
    pub fn finish(self) -> Settled {
        let mut state = match self.state.into_inner() {
            Ok(state) => state,
            Err(poisoned) => {
                let mut state = poisoned.into_inner();
                state.blocked = Some("the rehearsal account was interrupted".to_owned());
                state
            }
        };
        if state.in_flight {
            state.blocked = Some("the rehearsal did not finish draining".to_owned());
        }
        Settled {
            turn: state.turn,
            preview: state.preview,
            blocked: state.blocked,
        }
    }

    async fn observe(
        &self,
        candidate: &str,
        inputs: &[String],
        targets: &[String],
    ) -> RehearsalReport {
        let mut account = match self.begin() {
            Ok(account) => account,
            Err(why) => return refused(candidate, why, Refusal::NotBuilt),
        };
        let before = match WorldBefore::capture(&self.root, candidate, inputs, targets).await {
            Ok(before) => before,
            Err(why) => {
                let mut state = self.state();
                state.in_flight = false;
                state.blocked = Some(format!("the original world could not be observed: {why}"));
                return refused(candidate, why, Refusal::CopyIn);
            }
        };
        // No second timeout abandons this future: the room owns its stop and drainage.
        let report = self.host.rehearse_reading(candidate, inputs, targets).await;
        let declared = match &report.outcome {
            Rehearsal::Passed { outputs } => outputs.iter().map(|o| o.path.clone()).collect(),
            Rehearsal::Missing { outputs } => outputs.clone(),
            _ => Vec::new(),
        };
        let run = judged_run("observed", &report, inputs, targets, &declared);
        let charged = account.charge(&run.usage);
        {
            let mut state = self.state();
            state.turn = account.turn();
            if charged != Admission::Open {
                state.blocked =
                    Some("the observed rehearsal exceeded its shared budget".to_owned());
            }
        }
        let identity = report.candidate_sha256 == sha256_hex(candidate.as_bytes());
        let preview = if charged != Admission::Open || !identity {
            None
        } else if matches!(
            (&report.attempt, &report.outcome, &run.end),
            (
                Attempt::NeverAttempted,
                Rehearsal::NotRun { .. },
                RunEnd::NotRun { .. }
            )
        ) {
            match &report.outcome {
                Rehearsal::NotRun { reason } => Some(Preview {
                    candidate_sha256: report.candidate_sha256.clone(),
                    witness: None,
                    lines: format!(
                        "Rehearsal not run · {reason} · source-only preview · no output observed · `yes` saves only; running is separate\n"
                    ),
                }),
                _ => None,
            }
        } else if matches!(
            (&report.outcome, &run.end),
            (Rehearsal::Passed { .. }, RunEnd::Completed)
        ) {
            match before.witness(candidate, &report).await {
                Ok(witness) => Some(Preview {
                    candidate_sha256: witness.candidate_sha256().to_owned(),
                    witness: Some(witness),
                    lines: passed_lines(run.read_back.iter().map(|read| {
                        (
                            read.path.as_str(),
                            read.text.as_str(),
                            read.written,
                            read.truncated,
                        )
                    })),
                }),
                Err(why) => {
                    self.state().blocked = Some(format!(
                        "the rehearsal cannot bind the original world: {why}"
                    ));
                    None
                }
            }
        } else {
            None
        };
        let mut state = self.state();
        state.in_flight = false;
        state.preview = preview;
        report
    }
}

impl Rehearse for Scoped {
    fn bound(&self) -> Duration {
        self.host.bound()
    }

    fn rehearse<'a>(&'a self, candidate: &'a str, inputs: &'a [String]) -> RehearsalFuture<'a> {
        self.rehearse_reading(candidate, inputs, &[])
    }

    fn rehearse_reading<'a>(
        &'a self,
        candidate: &'a str,
        inputs: &'a [String],
        targets: &'a [String],
    ) -> RehearsalFuture<'a> {
        Box::pin(self.observe(candidate, inputs, targets))
    }
}

fn refused(candidate: &str, reason: String, refusal: Refusal) -> RehearsalReport {
    RehearsalReport::new(
        Rehearsal::NotRun { reason },
        Attempt::NeverAttempted,
        EffectCounts::none(),
        sha256_hex(candidate.as_bytes()),
    )
    .with_observation(Observation::refused(refusal))
}

// Readback was validated by judged_run, and the before/after witness covered every written path.
fn passed_lines<'a>(reads: impl Iterator<Item = (&'a str, &'a str, bool, bool)>) -> String {
    let mut lines = "Rehearsed on a copy of your files · nothing ran on the originals · observed result of this candidate · `yes` saves these exact bytes; running is a separate line (« run it »)\n".to_owned();
    let mut observed = false;
    for (path, text, written, truncated) in reads {
        observed = true;
        let publication = if written {
            "written by the run"
        } else {
            "not written by the run"
        };
        let coverage = if truncated {
            "prefix only"
        } else {
            "whole text observed"
        };
        let excerpt: String = text.chars().take(240).collect();
        let _ = writeln!(
            lines,
            "  read back · `{path}` · {publication} · {coverage} · excerpt {excerpt:?}"
        );
    }
    if !observed {
        lines.push_str("  no text output observed\n");
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_preview() -> Preview {
        Preview {
            candidate_sha256: "synthetic-candidate".into(),
            witness: Some(Witness::new("synthetic-candidate".into(), Vec::new())),
            lines: "synthetic evidence".into(),
        }
    }

    #[test]
    fn a_blocked_account_cannot_release_a_live_preview_but_keeps_its_usage() {
        // An overlapping begin can block while the first observation later completes.
        // Build that settled state directly: no host, concurrency, or workflow executes.
        let spent = Usage::new(1, 1, 10, 20, 30);
        let (charged, result) = Settled {
            turn: spent,
            preview: Some(synthetic_preview()),
            blocked: Some("another rehearsal has not drained".into()),
        }
        .into_parts();
        assert_eq!(charged, spent);
        assert!(matches!(result, Err(why) if why == "another rehearsal has not drained"));
    }

    #[test]
    fn an_open_account_releases_its_evidence_and_usage_unchanged() {
        let spent = Usage::new(1, 1, 10, 20, 30);
        let (charged, result) = Settled {
            turn: spent,
            preview: Some(synthetic_preview()),
            blocked: None,
        }
        .into_parts();
        assert_eq!(charged, spent);
        let preview = result.expect("an open account").expect("the live evidence");
        assert_eq!(preview.candidate_sha256(), "synthetic-candidate");
        let (witness, lines) = preview.into_parts();
        assert_eq!(witness.unwrap().candidate_sha256(), "synthetic-candidate");
        assert_eq!(lines, "synthetic evidence");
    }
}
