// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The run the door started for the human, as this session observes it (moved whole out of
//! `runtime.rs` at its file bound): its exit and trace remembered as a fact, `/proof` read
//! from that trace, and a pause's gate answered by the human alone. Observing never re-runs
//! or re-authorizes a run, and nothing answers a gate for the human.

use std::path::Path;

use crate::change::PendingGate;
use crate::outcome::{GateId, Refusal, RefusalClass};

use super::decision::{is_gate_token, local_command_of};
use super::{Lang, SessionRuntime, TurnOutcome, aside, is_quit, shown_trace, under};

impl SessionRuntime {
    /// The refusal for an answer with no gate: the last one was answered,
    /// or no run ever paused.
    fn no_gate_waiting(&self) -> Refusal {
        match &self.answered {
            Some(id) => Refusal::new(
                RefusalClass::AlreadyConsumed,
                format!("the gate {id} was answered once — no run is waiting for an answer"),
            ),
            None => Refusal::new(RefusalClass::WrongState, "no run is waiting for an answer"),
        }
    }

    /// What the door observed of the run it started for the human: the
    /// exit code's meaning and the trace, remembered as a fact of this
    /// session — never re-run, never re-authorized (attaching is
    /// observation). A pause (exit 4) whose trace carries the gate
    /// becomes the question asked to the human.
    pub(super) fn observe_run_unrecorded(&mut self, exit: u8, trace: Option<&Path>) -> TurnOutcome {
        // A run observed closes the optional offer about an earlier result, whatever it did.
        self.feedback.close();
        let root = self.snapshot.root.clone();
        let told = std::mem::take(&mut self.run_told);
        // The trace's own frames, when the door left one this session can
        // read: the views below say what they prove, the line stays the fact.
        let facts = trace.and_then(|t| crate::run_view::RunFacts::read(&under(&root, t)));
        self.last_run_refused =
            (facts.as_ref()).is_some_and(crate::run_view::RunFacts::refused_at_gate);
        // This session's own settled run moves its rehearsed proof past what it completed
        // writing; anything else, a refused advance included, keeps the rehearsed world.
        if let (0, Some(f), Some(workflow)) = (exit, &facts, self.last_workflow.clone())
            && f.terminal() == Some("succeeded")
            && let Some(sha) = f.workflow_sha256()
        {
            let _ = self.advance_rehearsal(&workflow, sha, &f.completed_writes());
        }
        let line = self.observation_line(exit, trace, facts.is_none());
        self.last_trace = trace.map(Path::to_path_buf);
        // What these bytes cost when they last ran, for the next run's ceiling.
        if let (Some(f), Some(workflow)) = (&facts, self.last_workflow.clone())
            && let (Some(sha), Some(spent)) = (f.workflow_sha256(), f.spent_usd())
        {
            self.last_spend = Some((workflow, sha.to_owned(), spent));
        }
        let fr = self.language == Lang::Fr;
        if exit == 4
            && let (Some(trace), Some(workflow)) = (trace, self.last_workflow.clone())
            && let Some(gate) = PendingGate::from_trace(&workflow, trace)
        {
            let gated = aside::gated_tasks(&root.join(&workflow), &gate.task);
            let view = facts.as_ref().map_or_else(
                || gate.question_in(fr),
                |f| f.gate_in(&workflow, &gate.message, &gate.mode, &gated, fr),
            );
            let id = GateId::new(&gate.trace, &gate.task);
            let values = gate.values.clone().map(Box::new);
            let withheld = gate.withheld.clone().map(Box::new);
            self.pending_gate = Some(gate);
            return TurnOutcome::GateAsk {
                id,
                question: format!("{line}\n{view}"),
                values,
                withheld,
            };
        }
        match (exit, facts, self.last_workflow.clone()) {
            (0 | 1, Some(f), Some(workflow)) => {
                // The conversation's author hears of its candidate's run: a failure is repaired.
                // A success of its current candidate that wrote its artifact earns the optional
                // offer about that result.
                let offered = (exit == 0).then(|| self.feedback_after(&f)).flatten();
                let said = format!("{}\n  {line}", f.result_in(&root, &workflow, fr));
                match (self.after_run(exit, said), offered) {
                    (TurnOutcome::Facts(said), Some(offer)) => {
                        TurnOutcome::Facts(format!("{said}\n\n{offer}"))
                    }
                    (outcome, _) => outcome,
                }
            }
            // Any other end (a refusal before or around the execution, an unreadable trace) is
            // still the conversation's to know with the person's next line.
            (_, facts, workflow) => {
                let said = match (facts, workflow) {
                    (Some(f), Some(workflow)) => {
                        format!("{}\n  {line}", f.result_in(&root, &workflow, fr))
                    }
                    // Refused before any frame: what the stream told the person is the reason.
                    _ if !told.is_empty() => format!("{line}\n  {}", told.join("\n  ")),
                    _ => line.clone(),
                };
                self.note_run(&said);
                TurnOutcome::Facts(line)
            }
        }
    }

    /// `/proof` — what the last observed run's trace proves, through the
    /// ONE verify door; before any run, where a proof will come from.
    pub(super) fn proof_unrecorded(&self) -> TurnOutcome {
        let Some(trace) = &self.last_trace else {
            return TurnOutcome::Facts(
                super::restore::named(self.language.pick(
                    "No run observed in this session yet · « run it » runs the accepted workflow once · `/proof` then reads the trace it leaves (`nika trace ls` lists earlier ones)",
                    "Aucun run observé dans cette session · « lance-le » lance une fois le workflow accepté · `/proof` lit ensuite la trace qu'il laisse (`nika trace ls` liste les précédentes)",
                )),
            );
        };
        match crate::run_view::RunFacts::read(&under(&self.snapshot.root, trace)) {
            Some(facts) => TurnOutcome::Facts(facts.proof(&self.snapshot.root)),
            None => TurnOutcome::Facts(format!(
                "the trace `{shown}` cannot be read now · `{} trace verify {shown}` judges it from the shell",
                super::schedule::program(),
                shown = shown_trace(&self.snapshot.root, trace)
            )),
        }
    }

    /// The human's answer to a pending gate: the resume the door runs.
    /// Nothing answers for the human; an empty line is not an answer.
    pub(super) fn answer_gate_unrecorded(&mut self, line: &str) -> TurnOutcome {
        let Some(gate) = self.pending_gate.take() else {
            return TurnOutcome::Refusal(self.no_gate_waiting());
        };
        // Leaving is always one line away: the gate keeps waiting in its
        // paused trace (and in the record), nothing answers for the human.
        if is_quit(line) {
            self.pending_gate = Some(gate);
            return TurnOutcome::Quit;
        }
        // « why? » beside the gate: what the answer lets happen, from the
        // workflow's own bytes; the gate keeps waiting.
        if crate::authoring::is_why(line) {
            let text = aside::explain_gate(&gate, &self.snapshot.root, self.language);
            self.pending_gate = Some(gate);
            return TurnOutcome::Aside(text);
        }
        // A local command beside the gate answers from the session's own
        // facts, the gate kept: a slash line is never the gate's answer.
        if let Some(command) = local_command_of(line) {
            self.pending_gate = Some(gate);
            return self.answer_locally(command);
        }
        if let Some(outcome) = self.beside(line, "the gate still waits · nothing answers for you")
        {
            self.pending_gate = Some(gate);
            return outcome;
        }
        if line.trim().is_empty() {
            self.pending_gate = Some(gate);
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::EmptyAnswer,
                "the gate needs an answer — nothing answers for you",
            ));
        }
        // A confirm gate takes its protocol tokens and nothing else: any
        // other line is open language — a question about the gate explains
        // it, a change belongs to the workflow (« no », then the change);
        // neither answers the gate. Authority never comes from a reading.
        if gate.mode == "confirm" && !is_gate_token(line) {
            if let Err(refusal) = self.admit_gate_money(line) {
                self.pending_gate = Some(gate);
                return refusal;
            }
            if self.money_blocks_cognition() {
                self.pending_gate = Some(gate);
                return self.cognition_money_refusal();
            }
            let decision = self.classify(crate::turn::SessionPhase::GatePending, line);
            let text = match decision.act {
                crate::turn::TurnAct::Modify | crate::turn::TurnAct::Mixed => self
                    .language
                    .pick(
                        "the gate takes a yes or a no — a change belongs to the workflow: answer `no`, then say the change",
                        "la validation attend un oui ou un non — un changement appartient au workflow : répondez « non », puis dites le changement",
                    )
                    .to_owned(),
                _ => format!(
                    "{}\n  {}",
                    aside::explain_gate(&gate, &self.snapshot.root, self.language),
                    self.language.pick(
                        "the gate still waits · `yes` or `no` answers it",
                        "la validation attend toujours · « oui » ou « non » lui répond",
                    )
                ),
            };
            self.pending_gate = Some(gate);
            return TurnOutcome::Aside(text);
        }
        if let Some(stale) = self.stale_gate(&gate) {
            return stale;
        }
        self.answered = Some(GateId::new(&gate.trace, &gate.task));
        let answer = gate.answer_arg(line);
        self.finish_gate_money();
        self.remember("(gate)", &format!("{} answered: {answer}", gate.task));
        TurnOutcome::ResumeRequested {
            workflow: gate.workflow,
            trace: gate.trace,
            answer,
        }
    }

    /// The last run of `workflow` when it failed, as a revision of that workflow reads it: the
    /// exit and each failed task with the error its trace recorded. « corrige ça » after a
    /// failed run names that failure; without it the author guessed (live gauntlet, 2026-10-15:
    /// a fetch refused after a redirect « fixed » by an unrelated `max_tokens` edit, « No
    /// diagnostic was supplied »). A run that succeeded or paused at a gate is no failure.
    pub(super) fn failed_run_of(&self, workflow: &Path) -> Option<serde_json::Value> {
        let (exit, _) = self.last_run.as_ref()?;
        if matches!(exit, 0 | 4) {
            return None;
        }
        let root = &self.snapshot.root;
        let relative = |path: &Path| path.strip_prefix(root).unwrap_or(path).to_path_buf();
        if relative(self.last_workflow.as_deref()?) != relative(workflow) {
            return None;
        }
        let raw = std::fs::read_to_string(under(root, self.last_trace.as_deref()?)).ok()?;
        let failed: Vec<serde_json::Value> = (raw.lines())
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .filter(|event| event["kind"] == "task_failed")
            .map(|event| {
                let field = |key: &str| {
                    (event["fields"].as_array().into_iter().flatten())
                        .find(|f| f["key"] == key)
                        .map(|f| f["value"].clone())
                };
                serde_json::json!({
                    "task": field("task"),
                    "error": field("detail").or_else(|| field("error")),
                })
            })
            .collect();
        Some(serde_json::json!({
            "workflow": relative(workflow),
            "exit": exit,
            "failed_tasks": failed,
        }))
    }

    fn observation_line(&mut self, exit: u8, trace: Option<&Path>, with_produced: bool) -> String {
        // The run door's exit codes have one reading, shared with the work snapshot; a run whose
        // gate the person refused ended where their answer stopped it, never as a success.
        let meaning = if exit == 0 && self.last_run_refused {
            self.language
                .pick("stopped at your no", "arrêté à votre « non »")
        } else if self.language == crate::Lang::Fr {
            meaning_fr(crate::work::RunEnd::of(exit))
        } else {
            crate::work::RunEnd::of(exit).meaning()
        };
        // Said in the person's language (live gauntlet wave 6 r12 B1, 2026-10-18: « run observed
        // · exit 0 · succeeded · trace » on a French run card).
        let observed = (self.language).pick("run observed · exit", "run observé · sortie");
        let line = match trace {
            Some(t) => format!(
                "{observed} {exit} · {meaning} · trace `{}`",
                shown_trace(&self.snapshot.root, t)
            ),
            None => format!("{observed} {exit} · {meaning}"),
        };
        // The trace's git hygiene, once per run, and what a green run left behind
        // (`run_view::{hygiene_note, produced}`).
        let line = match crate::run_view::hygiene_note(self.snapshot.git_root.as_deref()) {
            Some(note) => format!("{line}\n  {note}"),
            None => line,
        };
        let produced = (self.last_workflow.as_ref())
            .and_then(|w| crate::run_view::produced(&self.snapshot.root, w));
        let line = match (exit, produced) {
            (0, Some(produced)) if with_produced => format!("{line}\n  {produced}"),
            _ => line,
        };
        self.last_run = Some((exit, line.clone()));
        self.remember("(run)", &line);
        line
    }
}

/// How a run ended ([`crate::work::RunEnd::meaning`]), in French.
fn meaning_fr(end: crate::work::RunEnd) -> &'static str {
    use crate::work::RunEnd;
    match end {
        RunEnd::Succeeded => "réussi",
        RunEnd::Failed => "le workflow a échoué",
        RunEnd::RefusedFindings => "refusé avant de tourner (constats)",
        RunEnd::RefusedEnvironment => "refusé par l'environnement",
        RunEnd::Paused => {
            "en pause pour la réponse d'une personne : le run reprend une fois sa question répondue"
        }
        RunEnd::Interrupted => "interrompu avant la fin (Ctrl+C) : la trace montre ce qui a tourné",
        _ => "terminé avec un code inconnu",
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::SessionRuntime;
    use crate::intelligence::{DataLocus, IntelligenceKind, ResolvedSessionIntelligence};
    use crate::reasoner::NoReasoner;

    /// The trace of the live gauntlet's failed veille run, its two frames that matter.
    const TRACE: &str = concat!(
        r#"{"kind":"task_completed","fields":[{"key":"task","value":"fetch_hn"}]}"#,
        "\n",
        r#"{"kind":"task_failed","fields":[{"key":"task","value":"fetch_anthropic"},{"key":"note","value":"invoke · nika:fetch"},{"key":"detail","value":"NIKA-SEC-004 · `platform.claude.com` resolves outside the declared net.http boundary · reached by a redirect from `docs.claude.com`"}]}"#,
        "\n",
        r#"{"kind":"workflow_failed","fields":[{"key":"status","value":"failed"}]}"#,
        "\n",
    );

    fn session(root: &Path, exit: u8) -> SessionRuntime {
        let none = ResolvedSessionIntelligence::new(
            IntelligenceKind::None,
            None,
            DataLocus::None,
            false,
            None,
        );
        let traces = root.join(".nika/traces");
        std::fs::create_dir_all(&traces).expect("traces");
        std::fs::write(traces.join("run.ndjson"), TRACE).expect("trace");
        let mut s = SessionRuntime::open(root, none, Box::new(NoReasoner));
        s.last_workflow = Some(PathBuf::from("veille-quotidienne.nika"));
        s.last_trace = Some(PathBuf::from(".nika/traces/run.ndjson"));
        s.last_run = Some((exit, format!("run observed · exit {exit}")));
        s
    }

    /// A run whose gate the person refused is said stopped at their no, in the observation and in
    /// `/status`, never « succeeded » (supernovae-1c, after the TUI's dd13f65f8).
    #[test]
    fn a_refused_gate_is_never_a_success() {
        let root = tempfile::tempdir().expect("root");
        let mut s = session(root.path(), 0);
        let refused = concat!(
            r#"{"kind":"workflow_started","fields":[{"key":"workflow","value":"veille"}]}"#,
            "\n",
            r#"{"kind":"approval_decided","fields":[{"key":"task","value":"publish"},{"key":"decision","value":"deny"},{"key":"source","value":"resume"}]}"#,
            "\n",
            r#"{"kind":"workflow_completed","fields":[{"key":"status","value":"succeeded"}]}"#,
            "\n",
        );
        std::fs::write(root.path().join(".nika/traces/refused.ndjson"), refused).expect("trace");
        let said = format!(
            "{:?}",
            s.observe_run(0, Some(Path::new(".nika/traces/refused.ndjson")))
        );
        assert!(
            said.contains("run observed · exit 0 · stopped at your no"),
            "{said}"
        );
        assert!(!said.contains("· succeeded ·"), "{said}");
        assert!(
            s.status_line().contains("stopped at your no"),
            "{}",
            s.status_line()
        );
    }

    /// « corrige ça » after a failed run revises that run's workflow with its failure in hand:
    /// each failed task and the error its trace recorded, nothing else of the run.
    #[test]
    fn a_failed_run_is_what_its_workflow_revision_reads() {
        let root = tempfile::tempdir().expect("root");
        let s = session(root.path(), 1);
        let failed = s
            .failed_run_of(Path::new("veille-quotidienne.nika"))
            .expect("the failed run of this workflow");
        assert_eq!(failed["exit"], 1);
        assert_eq!(failed["workflow"], "veille-quotidienne.nika");
        let tasks = failed["failed_tasks"].as_array().expect("tasks");
        assert_eq!(tasks.len(), 1, "{failed}");
        assert_eq!(tasks[0]["task"], "fetch_anthropic");
        assert!(
            tasks[0]["error"]
                .as_str()
                .is_some_and(|e| e.contains("reached by a redirect from `docs.claude.com`")),
            "{failed}"
        );
        // The same workflow under its absolute path is the same workflow.
        assert!(
            s.failed_run_of(&root.path().join("veille-quotidienne.nika"))
                .is_some()
        );
        // Another workflow, a success and a paused gate carry no failure.
        assert!(s.failed_run_of(Path::new("other.nika")).is_none());
        for exit in [0, 4] {
            let quiet = session(root.path(), exit);
            assert!(
                quiet
                    .failed_run_of(Path::new("veille-quotidienne.nika"))
                    .is_none(),
                "exit {exit}"
            );
        }
    }
}
