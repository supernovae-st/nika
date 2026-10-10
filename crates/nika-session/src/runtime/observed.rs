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
use super::{SessionRuntime, TurnOutcome, aside, is_quit, shown_trace, under};

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
        let root = self.snapshot.root.clone();
        // The trace's own frames, when the door left one this session can
        // read: the views below say what they prove, the line stays the fact.
        let facts = trace.and_then(|t| crate::run_view::RunFacts::read(&under(&root, t)));
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
        if exit == 4
            && let (Some(trace), Some(workflow)) = (trace, self.last_workflow.clone())
            && let Some(gate) = PendingGate::from_trace(&workflow, trace)
        {
            let gated = aside::gated_tasks(&root.join(&workflow), &gate.task);
            let view = facts.as_ref().map_or_else(
                || gate.question(),
                |f| f.gate(&workflow, &gate.message, &gate.mode, &gated),
            );
            let id = GateId::new(&gate.trace, &gate.task);
            self.pending_gate = Some(gate);
            return TurnOutcome::GateAsk {
                id,
                question: format!("{line}\n{view}"),
            };
        }
        match (exit, facts, self.last_workflow.clone()) {
            (0 | 1, Some(f), Some(workflow)) => {
                TurnOutcome::Facts(format!("{}\n  {line}", f.result(&root, &workflow)))
            }
            _ => TurnOutcome::Facts(line),
        }
    }

    /// `/proof` — what the last observed run's trace proves, through the
    /// ONE verify door; before any run, where a proof will come from.
    pub(super) fn proof_unrecorded(&self) -> TurnOutcome {
        let Some(trace) = &self.last_trace else {
            return TurnOutcome::Facts(
                "No run observed in this session yet · « run it » runs the accepted workflow once · `/proof` then reads the trace it leaves (`nika trace ls` lists earlier ones)".to_owned(),
            );
        };
        match crate::run_view::RunFacts::read(&under(&self.snapshot.root, trace)) {
            Some(facts) => TurnOutcome::Facts(facts.proof(&self.snapshot.root)),
            None => TurnOutcome::Facts(format!(
                "the trace `{shown}` cannot be read now · `nika trace verify {shown}` judges it from the shell",
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
            let text = aside::explain_gate(&gate, &self.snapshot.root);
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
                crate::turn::TurnAct::Modify | crate::turn::TurnAct::Mixed => {
                    "the gate takes a yes or a no — a change belongs to the workflow: answer `no`, then say the change".to_owned()
                }
                _ => format!(
                    "{}\n  the gate still waits · `yes` or `no` answers it",
                    aside::explain_gate(&gate, &self.snapshot.root)
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

    fn observation_line(&mut self, exit: u8, trace: Option<&Path>, with_produced: bool) -> String {
        // The run door's exit codes have one reading, shared with the work snapshot.
        let meaning = crate::work::RunEnd::of(exit).meaning();
        let line = match trace {
            Some(t) => format!(
                "run observed · exit {exit} · {meaning} · trace `{}`",
                shown_trace(&self.snapshot.root, t)
            ),
            None => format!("run observed · exit {exit} · {meaning}"),
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
