// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a yes that landed its set reports (moved whole out of `runtime.rs` at its file bound,
//! C9): the decision, the on-disk check of every workflow written, the project observed again,
//! and a run requested only when that check is clean.

use std::fmt::Write as _;

use super::{SessionRuntime, TurnOutcome};
use crate::change::{Applied, ProjectChangeSet, WorkflowAudit, check_on_disk};
use crate::outcome::{ProposalId, Refusal};
use crate::snapshot::ProjectSnapshot;
use nika_session_change::save_run::SaveRun;

impl SessionRuntime {
    /// After a yes lands the set: mark decided, check every workflow,
    /// re-observe, remember, and request a run only when that check is
    /// clean. Empty-write and mid-set Io stay on `consent` so a refusal
    /// never becomes `already_consumed`.
    pub(super) fn report_landed(
        &mut self,
        set: &ProjectChangeSet,
        applied: &Applied,
        id: &ProposalId,
        basis: Option<&str>,
        run: Option<SaveRun>,
    ) -> TurnOutcome {
        self.save_proposal_money(set, id);
        let evidence = self.evidence_applied(set, id, applied);
        // A rehearsed copy's proof moves to the workflow it was saved as (`rehearsed.rs`).
        let landed_workflow = set.workflows().into_iter().next();
        let rehearsed = self.land_rehearsal(id, landed_workflow.as_deref());
        self.decided = Some(id.clone());
        let written: Vec<String> = applied
            .written
            .iter()
            .map(|p| format!("`{}`", p.display()))
            .collect();
        let mut report = format!("applied · wrote {}{evidence}", written.join(" · "));
        // What the yes found of the sources the bytes rely on (F4 · `fresh.rs`).
        if let Some(basis) = basis {
            let _ = write!(report, "\n  {basis}");
        }
        let audits = checked(set, &mut report);
        let all_clean = audits.iter().all(|a| a.clean);
        // Where each landed workflow reaches, from the same on-disk check the report states.
        let world_of = |workflow: &std::path::Path| {
            audits
                .iter()
                .find(|a| a.path == workflow)
                .map(|a| a.world.clone())
        };
        self.snapshot = ProjectSnapshot::observe(&self.snapshot.cwd);
        self.remember("(consent)", &report);
        // The workflow just accepted is the one « run it » names next —
        // an explicit line, never this consent — and the schedule its
        // request asked for is what « activate » declares.
        if let Some(first) = landed_workflow.clone() {
            // Run evidence belongs to the previous saved bytes. A new Save is not a Run,
            // including when it replaces the workflow at the same path.
            self.last_run = None;
            let path = first
                .strip_prefix(&self.snapshot.root)
                .unwrap_or(&first)
                .display()
                .to_string();
            if let Some(change) = set.changes.iter().find(|c| c.path() == first) {
                nika_onboard::compile::program_records::saved(
                    &mut self.programs,
                    &path,
                    change.content(),
                    &id.to_string(),
                    &|text| crate::broker::redact(text).0,
                );
            }
            self.saved_reach = world_of(&first).map(|world| (first.clone(), world));
            self.consented = Some(first.clone());
            self.last_workflow = Some(first);
            self.last_check_clean = Some(all_clean);
            self.last_trigger = self.pending_trigger.take();
        }
        let project_only = landed_workflow.is_none()
            && set
                .changes
                .iter()
                .any(|c| c.path() == std::path::Path::new("nika.yaml"));
        match run {
            Some(run) if all_clean => self.run_saved(&report, run),
            Some(_) => {
                report.push_str(
                    "\n  the run was not started: findings stop it — repair them, then ask to run",
                );
                TurnOutcome::Facts(report)
            }
            None if project_only => {
                report.push_str(
                    "\nDeclared in `nika.yaml` · not active: a firer must run on this machine\n  `nika serve` fires it while it runs · `nika arm --emit launchd --write` installs the OS unit · `nika arm` lists what is declared and proves what fired",
                );
                TurnOutcome::Facts(report)
            }
            None if all_clean => {
                report.push_str(Self::landed_words(rehearsed));
                if let Some(t) = &self.last_trigger
                    && t.status == nika_onboard::compile::TriggerStatus::RequiresBinding
                {
                    let _ = write!(
                        report,
                        "\n  say « activate » to declare « {} » in `nika.yaml` (Nika asks the time zone, the missed policy and the ceiling first) · saving activated nothing",
                        t.source_hint.as_deref().unwrap_or("the schedule")
                    );
                }
                TurnOutcome::Facts(report)
            }
            None => TurnOutcome::Facts(report),
        }
    }

    /// `save & run`'s run, once its save checked clean: its typed target and values through the
    /// one run admission (`admit_run`); not started, the save stands and says why.
    fn run_saved(&mut self, report: &str, run: SaveRun) -> TurnOutcome {
        let SaveRun {
            workflow,
            vars,
            access_pin,
            max_cost_usd,
            ..
        } = run;
        match self.admit_run("save & run", workflow, max_cost_usd, access_pin, vars) {
            TurnOutcome::RunRequested {
                report: run_report,
                run,
            } => {
                let report = format!("{report}\n  run once · {run_report}");
                TurnOutcome::RunRequested { report, run }
            }
            TurnOutcome::Question { key, question } => {
                let question = format!("{report}\n{question}");
                TurnOutcome::Question { key, question }
            }
            TurnOutcome::Refusal(Refusal { text, .. }) | TurnOutcome::Facts(text) => {
                TurnOutcome::Facts(format!("{report}\n  the run was not started: {text}"))
            }
            _ => TurnOutcome::Facts(format!("{report}\n  the run was not started")),
        }
    }
}

/// Check every landed workflow on disk, append each verdict to the report, and return the
/// audits so the reach of the saved and requested bytes comes from this same check.
fn checked(set: &ProjectChangeSet, report: &mut String) -> Vec<WorkflowAudit> {
    let mut audits = Vec::new();
    for wf in set.workflows() {
        let audit = check_on_disk(&set.root, &wf);
        let _ = write!(
            report,
            "\n  check · `{}` · {}",
            wf.display(),
            if audit.clean {
                "clean ✔"
            } else {
                "findings ✖"
            }
        );
        for f in &audit.findings {
            let _ = write!(report, "\n    · {f}");
        }
        if let Some(line) = crate::change::compact_hints(&audit.hints, &wf.display().to_string()) {
            let _ = write!(report, "\n    · {line}");
        }
        audits.push(audit);
    }
    audits
}
