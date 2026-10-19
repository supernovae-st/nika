// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! What a yes that landed its set reports (moved whole out of `runtime.rs` at its file bound): the
//! decision, the on-disk check of every workflow written, the project observed again,
//! and a run requested only when that check is clean.

use std::fmt::Write as _;

use super::{Lang, SessionRuntime, TurnOutcome};
use crate::change::{Applied, ProjectChangeSet, WorkflowAudit, check_on_disk};
use crate::outcome::{ProposalId, Refusal};
use crate::snapshot::ProjectSnapshot;
use nika_session_change::save_run::SaveRun;

impl SessionRuntime {
    /// A yes's writes: the files they landed, or the refusal it ends on.
    pub(super) fn apply_pending(
        &mut self,
        set: &ProjectChangeSet,
        id: &ProposalId,
    ) -> Result<Applied, TurnOutcome> {
        set.apply_attempt().map_err(|attempt| {
            // No write returned success; the failing target may have changed. The proposal
            // is neither pending nor decided, so retry by identity reads `wrong_state`, never
            // a false claim of a completed effect.
            if attempt.written.is_empty() {
                return TurnOutcome::Refusal(Refusal::from_change(&attempt.error));
            }
            // A later write failed after this call itself landed files. The account is the
            // write loop's record, not a tree scan; the proposal stays undecided.
            let evidence = self.evidence_partial(set, id, &attempt);
            let text = format!("{}{evidence}", attempt.refusal_text(set));
            self.snapshot = ProjectSnapshot::observe(&self.snapshot.cwd);
            let class = Refusal::from_change(&attempt.error).class;
            TurnOutcome::Refusal(Refusal::new(class, text))
        })
    }

    /// The workflow just accepted is the one « run it » names next — an explicit line, never
    /// this consent — and the schedule its request asked for is what « activate » declares.
    fn keep_landed(
        &mut self,
        set: &ProjectChangeSet,
        id: &ProposalId,
        first: std::path::PathBuf,
        reach: Option<crate::world::World>,
        all_clean: bool,
    ) {
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
                &crate::broker::redact_for_record,
            );
            if let Some(driver) = &self.agent {
                driver.saved(&path, change.content());
            }
        }
        self.saved_reach = reach.map(|world| (first.clone(), world));
        // A revision of the saved workflow that carries no schedule keeps the one asked for
        // it, its zone included (live gauntlet r02 B10: after a repair, the zone was asked
        // again).
        let revised = self.last_workflow.as_ref() == Some(&first);
        let kept = revised.then(|| self.last_trigger.take()).flatten();
        self.consented = Some(first.clone());
        self.last_workflow = Some(first);
        self.last_check_clean = Some(all_clean);
        self.last_trigger = self.pending_trigger.take().or(kept);
    }

    /// A no to the proposal: decided, discarded, nothing written.
    pub(super) fn discard_pending(&mut self, id: ProposalId) -> TurnOutcome {
        self.decided = Some(id);
        self.agent_discarded();
        TurnOutcome::Facts(
            self.language
                .pick(
                    "discarded · nothing was written · ask again for the change when ready",
                    "écarté · rien n'a été écrit · redemandez le changement quand vous voulez",
                )
                .to_owned(),
        )
    }

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
        // The save's one wording, in the person's language (live gauntlet wave 7, 2026-10-19:
        // « applied · wrote » in every French conversation).
        let applied_words = self.language.pick("applied · wrote", "appliqué · écrit");
        let mut report = format!("{applied_words} {}{evidence}", written.join(" · "));
        // What the yes found of the sources the bytes rely on (`fresh.rs`).
        if let Some(basis) = basis {
            let _ = write!(report, "\n  {basis}");
        }
        let audits = checked(set, &mut report, self.language);
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
        if let Some(first) = landed_workflow.clone() {
            let reach = world_of(&first);
            self.keep_landed(set, id, first, reach, all_clean);
        }
        // The saved workflow as a terminal names it: relative to the project's root.
        let saved_file = (landed_workflow.as_deref()).map(|first| {
            (first.strip_prefix(&self.snapshot.root))
                .unwrap_or(first)
                .display()
                .to_string()
        });
        let project_only = landed_workflow.is_none()
            && set
                .changes
                .iter()
                .any(|c| c.path() == std::path::Path::new("nika.yaml"));
        let outcome = match run {
            Some(run) if all_clean => self.run_saved(&report, run),
            Some(_) => {
                report.push_str(self.language.pick(
                    "\n  the run was not started: findings stop it — repair them, then ask to run",
                    "\n  le run n'a pas été lancé : des constats l'arrêtent — corrigez-les, puis demandez de le lancer",
                ));
                TurnOutcome::Facts(report)
            }
            None if project_only => match self.unscheduled.take() {
                Some(workflow) => self.landed_removal(&workflow, report),
                None => self.landed_declaration(report),
            },
            None if all_clean => {
                report.push_str(Self::landed_words(rehearsed, self.language));
                if let Some(file) = &saved_file {
                    report.push_str(&self.terminal_run(file));
                }
                if let Some(t) = &self.last_trigger
                    && t.status == nika_onboard::compile::TriggerStatus::RequiresBinding
                {
                    let when = t.source_hint.as_deref().unwrap_or("the schedule");
                    let said = self.language.pick(
                        "\n  say « activate » to declare « {when} » in `nika.yaml` (Nika asks the time zone, the missed policy and the ceiling first) · saving activated nothing",
                        "\n  dites « active-le » pour déclarer « {when} » dans `nika.yaml` (Nika demande d'abord le fuseau, ce qu'il faut faire d'un run manqué et le plafond) · l'enregistrement n'a rien activé",
                    );
                    report.push_str(&said.replace("{when}", when));
                }
                TurnOutcome::Facts(report)
            }
            None => TurnOutcome::Facts(report),
        };
        if let Some(file) = &saved_file {
            self.note_saved(file, all_clean);
        }
        outcome
    }

    /// The author learns the save from the Session, after any run it was told (live gauntlet
    /// r02, 2026-10-17: right after « applied · wrote » it said « il n'est ni sauvegardé »).
    fn note_saved(&mut self, file: &str, clean: bool) {
        let schedule = (self.last_trigger.as_ref())
            .and_then(|t| t.source_hint.clone())
            .map_or_else(String::new, |when| {
                format!(
                    "; the schedule the person asked for (« {when} ») is declared only when they say « active-le », and nothing fires before"
                )
            });
        let checked = if clean { "clean" } else { "with findings" };
        self.note_for_author(&format!(
            "Nika (not the person) saved `{file}` on the person's yes: the file on disk now holds the proposal they consented to, checked {checked}. Saving activates nothing{schedule}. When the person asks whether it is saved, say it is."
        ));
    }

    /// A save that landed `nika.yaml`'s declaration alone: said declared and not active, the
    /// author told so, and the trigger offered when this host installs one (D1).
    fn landed_declaration(&mut self, mut report: String) -> TurnOutcome {
        let nika = super::schedule::program();
        let said = self.language.pick(
            "\nDeclared in `nika.yaml` · not active: a firer must run on this machine\n  `{nika} serve` fires it while it runs · `{nika} arm --emit launchd --write` installs the OS unit · `{nika} arm` lists what is declared and proves what fired",
            "\nDéclaré dans `nika.yaml` · pas actif : un déclencheur doit tourner sur cette machine\n  `{nika} serve` le déclenche tant qu'il tourne · `{nika} arm --emit launchd --write` installe l'unité système · `{nika} arm` liste ce qui est déclaré et prouve ce qui s'est déclenché",
        );
        report.push_str(&said.replace("{nika}", &nika));
        let declared = super::schedule::declared_schedules(&self.snapshot.root);
        self.note_for_author(&format!(
            "Nika (not the person) saved the schedule declaration they consented to: `nika.yaml` now declares {declared}. Declared is not active: nothing fires it until a firer runs on this machine. Nika now asks the person whether to install that firer on this Mac (a launchd unit, on their yes; « désactive-le » removes it). When the person asks whether it runs by itself, say what Nika's last line says: installed and active, or declared and not active."
        ));
        self.offer_install(report)
    }

    /// How to run the saved `file` from a terminal, by the program Nika runs as
    /// (`schedule::program`, so the command reaches this same binary) and with the ceiling the
    /// person saved with it; with none of theirs the flag is left out, never a default (NIK-171).
    fn terminal_run(&self, file: &str) -> String {
        let nika = super::schedule::program();
        let ceiling = (self.saved_ceiling(file))
            .and_then(|kept| kept.effective_usd)
            .map_or(String::new(), |usd| format!(" --max-cost-usd {usd}"));
        let file = shell_word(file);
        let from = self
            .language
            .pick("from a terminal:", "depuis un terminal :");
        format!("\n  {from} `{nika} run {file}{ceiling}`")
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
                let lead = self
                    .language
                    .pick("the run was not started:", "le run n'a pas été lancé :");
                TurnOutcome::Facts(format!("{report}\n  {lead} {text}"))
            }
            _ => TurnOutcome::Facts(format!(
                "{report}\n  {}",
                self.language
                    .pick("the run was not started", "le run n'a pas été lancé")
            )),
        }
    }
}

/// Why a consent saves nothing: the check of the bytes it would write fails, as the preview
/// showed it. The proposal keeps waiting for a change, or `no`.
pub(super) fn failing_check(set: &ProjectChangeSet, lang: Lang) -> Option<TurnOutcome> {
    let failing = set.failing_audits();
    let first = failing.first()?;
    let file = first.path.display();
    let mut text = match lang {
        Lang::Fr => format!(
            "la vérification de ces octets échoue (`{file}`) : rien n'a été enregistré ni lancé, et aucun fichier n'a été remplacé"
        ),
        _ => format!(
            "the check of these bytes fails (`{file}`): nothing was saved or run, and no file was replaced"
        ),
    };
    for finding in failing.iter().flat_map(|audit| &audit.findings) {
        let _ = write!(text, "\n  · {finding}");
    }
    text.push_str(lang.pick(
        "\n(the proposal still waits · say what to change · `no` discards it)",
        "\n(la proposition attend toujours · dites ce qu'il faut changer · « non » l'écarte)",
    ));
    Some(TurnOutcome::Refusal(Refusal::new(
        crate::outcome::RefusalClass::NotAllowed,
        text,
    )))
}

/// Check every landed workflow on disk, append each verdict to the report, and return the
/// audits so the reach of the saved and requested bytes comes from this same check.
fn checked(set: &ProjectChangeSet, report: &mut String, lang: Lang) -> Vec<WorkflowAudit> {
    let mut audits = Vec::new();
    for wf in set.workflows() {
        let audit = check_on_disk(&set.root, &wf);
        let verdict = if audit.clean {
            lang.pick("clean ✔", "propre ✔")
        } else {
            lang.pick("findings ✖", "constats ✖")
        };
        let _ = write!(
            report,
            "\n  {} · `{}` · {verdict}",
            lang.pick("check", "vérification"),
            wf.display()
        );
        for f in &audit.findings {
            let _ = write!(report, "\n    · {f}");
        }
        let shown = wf.display().to_string();
        if let Some(line) = crate::change::compact_hints_in(&audit.hints, &shown, lang == Lang::Fr)
        {
            let _ = write!(report, "\n    · {line}");
        }
        audits.push(audit);
    }
    audits
}

/// `path` as one shell word: as it is when every character is safe there, else single-quoted
/// (a quote inside closed, escaped and reopened).
fn shell_word(path: &str) -> String {
    let safe = |c: char| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/');
    if !path.is_empty() && path.chars().all(safe) {
        path.to_owned()
    } else {
        format!("'{}'", path.replace('\'', "'\\''"))
    }
}
