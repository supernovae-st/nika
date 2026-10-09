// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Durable boundaries around the existing session operations: the
//! conversation history under the home (the transcript), the project's
//! structured record (#1464 · `.nika/session-state.json`) and the consent
//! journal (#1465 · `.nika/consents.ndjson`).

use super::draft::{self, Restored};
use super::history::{
    AuthorityState, EffectState, History, HistoryMode, Operation, RunState, Saved,
};
use super::inference::{
    DISPATCH_PREFIX, GATE_MONEY_PREFIX, OBSERVED_PREFIX, RECONFIRM, gate_money_marker,
    is_money_marker,
};
use super::round::KeptRound;
use crate::run_view::KeptRun;
use nika_providers::authoring::preparation::PreparationCosts;
use nika_trace::lineage::{Standing, lineage_of};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use super::{ConversationChoice, IntentDraft, Refusal, RefusalClass, SessionRuntime, TurnOutcome};
use crate::change::{Applied, ApplyAttempt, PendingGate, ProjectChangeSet};
use crate::consent::{CONSENTS_FILE, ConsentDecision, ConsentRecord};
use crate::intelligence::{IntelligenceKind, ResolvedSessionIntelligence, now_rfc3339};
use crate::outcome::ProposalId;
use crate::state::{Pending, STATE_FILE, SessionState};

/// Preparation evidence before one turn; never an execution or monetary authority snapshot.
#[derive(Default)]
pub(super) struct PreparationBefore {
    goal: Option<String>,
    reading: Option<nika_onboard::compile::CompileOutcome>,
    proposal: Option<crate::ProposalId>,
    question: Option<crate::authoring::AuthoringRound>,
}

impl SessionRuntime {
    /// A kept choice its history could not read stays this conversation's: unavailable with its
    /// reason until chosen again, kept unchanged; never the operator's default in its place.
    fn unreadable_choice(&mut self, raw: serde_json::Value, error: &str) {
        let why = format!(
            "this conversation's kept intelligence choice is unreadable ({error}) · `/intelligence` chooses again"
        );
        let resolved = ResolvedSessionIntelligence::new(
            IntelligenceKind::None,
            None,
            crate::intelligence::DataLocus::None,
            false,
            Some(why),
        );
        if let Some(factory) = &self.factory {
            self.reasoner = factory(&resolved);
        }
        self.intelligence = resolved;
        self.refresh_seat();
        self.chosen = true;
        self.conversation = Some(ConversationChoice::Unreadable(raw));
    }

    /// Enable private, project-bound conversation history below `home/.nika`.
    ///
    /// Rebuilds the conversation projection and returns a notice on recovery.
    /// Pending proposals, gates and old consent identities never regain
    /// authority. The current project and intelligence are observed afresh.
    /// Call immediately after `open` or `open_with`, before the first turn.
    ///
    /// # Errors
    /// Refuses concurrent ownership, corrupt history or failed I/O.
    /// A failed enable also blocks this instance; ignoring the error cannot
    /// silently downgrade a requested durable session to an ephemeral one.
    pub fn enable_history(&mut self, home: &Path) -> Result<Option<String>, Refusal> {
        if !matches!(self.history, HistoryMode::Ephemeral)
            || !self.recent.is_empty()
            || self.pending.is_some()
            || self.intent.goal.is_some()
        {
            self.history =
                HistoryMode::Blocked("history activation must precede the first turn".to_owned());
            return Err(Refusal::new(
                RefusalClass::WrongState,
                "enable history before the first turn",
            ));
        }
        self.history = HistoryMode::Blocked("conversation history did not open".to_owned());
        let history = History::open(home, &self.snapshot.root).map_err(history_refusal)?;
        // The round kept beside its projected labels: the one durable copy, read, never live.
        self.restored_round = history
            .state
            .round
            .clone()
            .map(|raw| KeptRound::new(raw, history.state.unresolved.clone()));
        let expired = self.restore_intent(IntentDraft {
            goal: history.state.goal.clone(),
            decisions: history.state.decisions.clone(),
            unresolved: history.state.unresolved.clone(),
        });
        self.programs.clone_from(&history.state.programs);
        self.last_workflow =
            nika_onboard::compile::program_records::last_saved(self.programs.as_ref())
                .map(PathBuf::from);
        // Its program record says a consent saved it.
        self.consented.clone_from(&self.last_workflow);
        self.recent.clone_from(&history.state.recent);
        self.kept_run.clone_from(&history.state.last_run);
        // This conversation's own explicit choice resumes, servable or not (its fix said): never
        // the operator's default instead, unless the opener named another, which replaces it.
        let opener = self.conversation.is_some();
        if let (false, Some(raw)) = (opener, history.state.selection.clone()) {
            match serde_json::from_value(raw.clone()) {
                Ok(pref) => self.adopt(pref),
                Err(error) => self.unreadable_choice(raw, &error.to_string()),
            }
        }
        self.restored_draft = history.state.pending.clone().map(Restored::from_raw);
        self.money.reconfirm |= history.restored && history.monetary_seen;
        if self.money.reconfirm {
            // Legacy history cannot distinguish spent/unknown Session exposure
            // from gate-only money. Preserve that uncertainty in both stores.
            self.retain_money_guard();
        }
        let notice = history.restored.then(|| {
            let mut text = "conversation restored · previous proposals and gates require fresh validation".to_owned();
            text.push_str(&expired);
            if history.uncertain {
                text.push_str("\nhistorical unresolved operation: an earlier result or charge remains uncertain; later success does not reconcile it · inspect its effects and receipts before retrying it · nothing was replayed");
            }
            if let Some(restored) = &self.restored_draft {
                text.push('\n');
                text.push_str(&draft::restored_line(restored, self.money.reconfirm));
                // While a kept round can be continued, `/restore` continues it first.
                if self.restored_draft_id().is_some() && !self.round_is_continuable() {
                    text.push_str(super::restore::RESTORE_HINT);
                }
            }
            if let Some(line) = self.round_line() {
                text.push('\n');
                text.push_str(&line);
            }
            text
        });
        self.history = HistoryMode::Active(Box::new(history));
        // The opener's selection replaced the kept one: recorded now, so it is what resumes next;
        // a history that cannot record it opens nothing.
        if opener
            && let TurnOutcome::Refusal(refused) =
                self.recorded(Operation::Choice, "(the opener's selection)", |s| {
                    TurnOutcome::Facts(s.intelligence_line())
                })
        {
            return Err(refused);
        }
        Ok(notice)
    }

    /// Process one turn, recording its boundaries when history is enabled.
    pub fn turn(&mut self, input: &str) -> TurnOutcome {
        self.last_answer = None;
        // Closing remains possible even after storage failure.
        if matches!(input.trim(), "/quit" | "/exit") {
            self.set_cost_host_evidence(nika_runtime::cost_choice::CostHostEvidence::default());
            self.pending = None;
            self.pending_gate = None;
            return TurnOutcome::Quit;
        }
        // `/restore` records itself as the continuation or re-proposal act; what already waits
        // refuses it.
        if input.trim() == "/restore" {
            return self.restore_kept();
        }
        let operation = if self.local_run_line(input) {
            Operation::Run
        } else {
            Operation::Turn
        };
        self.recorded(operation, input, |s| {
            if s.waiting_cost_choice() {
                s.cost_answer(input)
            } else {
                s.turn_unrecorded(input)
            }
        })
    }

    /// Answer the current intelligence choice through the same durable boundary.
    pub fn choose(&mut self, answer: &str) -> TurnOutcome {
        self.last_answer = None;
        self.recorded(Operation::Choice, answer, |s| s.choose_unrecorded(answer))
    }

    /// Record intent before applying a currently pending, freshly witnessed
    /// proposal. A consent that decided (applied · discarded · landed
    /// partially) is a decision of the durable intent and keeps the
    /// project's structured record (#1464); a held question, a stale
    /// revision, a run's cost review still waiting or a blocked history
    /// decides nothing and writes nothing.
    pub fn consent(&mut self, answer: &str) -> TurnOutcome {
        self.last_answer = None;
        // Closing a review expires authority through the same door as closing a turn.
        // It must not overwrite the journal that keeps the unaccepted draft.
        if super::is_quit(answer) {
            return self.turn(answer);
        }
        if let Some(refused) = self.review_first(answer) {
            return refused;
        }
        if self.waiting_cost_choice() {
            return self.turn(answer);
        }
        self.unknown_cost.in_consent = true;
        let staged = self.pending_proposal();
        let outcome = self.recorded(Operation::Consent, answer, |s| {
            let outcome = s.consent_unrecorded(answer);
            if let (Some(id), TurnOutcome::Facts(text)) = (&staged, &outcome)
                && text.starts_with("discarded")
            {
                s.intent.decisions.push(format!("discarded proposal {id}"));
            }
            outcome
        });
        self.unknown_cost.in_consent = false;
        let decided = staged.is_some()
            && self.pending.is_none()
            && self.revising.is_none()
            && !matches!(
                &outcome,
                TurnOutcome::Refusal(Refusal {
                    class: RefusalClass::StaleRevision | RefusalClass::NotAllowed,
                    ..
                })
            );
        if decided {
            self.keep_state(outcome)
        } else {
            outcome
        }
    }

    /// Record the human's gate answer before returning a resume request;
    /// an answer that resumes or holds a monetary amendment keeps the project's
    /// structured record (#1464), even when conversation history is unavailable.
    /// While a run's cost review waits, it answers nothing.
    pub fn answer_gate(&mut self, line: &str) -> TurnOutcome {
        self.last_answer = None;
        if let Some(refused) = self.review_first(line) {
            return refused;
        }
        let waiting = self.waiting_gate();
        let outcome = self.recorded(Operation::Gate, line, |s| {
            let outcome = s.answer_gate_unrecorded(line);
            if let (Some(gate), TurnOutcome::ResumeRequested { answer, .. }) = (&waiting, &outcome)
            {
                // Expire only this gate's hold, not an earlier inference marker.
                let marker = gate_money_marker(gate);
                s.intent.decisions.retain(|d| d != &marker);
                s.restore_money_guards();
                s.intent
                    .decisions
                    .push(format!("answered the gate {gate}: {answer}"));
            } else if let Some(gate) = &waiting
                && s.money.gate.is_some()
            {
                let marker = gate_money_marker(gate);
                if !s.intent.decisions.contains(&marker) {
                    s.intent.decisions.push(marker);
                }
            }
            outcome
        });
        if matches!(outcome, TurnOutcome::ResumeRequested { .. }) || self.money.gate.is_some() {
            self.keep_state(outcome)
        } else {
            outcome
        }
    }

    /// Record the host's observation; this never discovers or reruns an
    /// execution. Every observed run keeps the project's structured record
    /// (#1464): the gate it paused on, when it did, is what waits.
    pub fn observe_run(&mut self, exit: u8, trace: Option<&Path>) -> TurnOutcome {
        self.observe_run_leg(exit, trace, KeptRun::new())
    }

    /// [`Self::observe_run`] with the run's observed identity (`leg`), kept in HOME
    /// history as the last run: evidence for a later open, never authority to run.
    pub fn observe_run_leg(&mut self, exit: u8, trace: Option<&Path>, leg: KeptRun) -> TurnOutcome {
        self.kept_run = Some(
            leg.ended(self.last_workflow.as_deref(), exit, trace)
                .to_value(),
        );
        let outcome = self.recorded(Operation::Observation, "(run observation)", |s| {
            s.observe_run_unrecorded(exit, trace)
        });
        self.keep_state(outcome)
    }

    fn restore_checkpoint(&mut self, checkpoint: Option<&serde_json::Value>, notice: &mut String) {
        if let (Some(raw), HistoryMode::Active(history)) = (checkpoint, &self.history)
            && self.money.account.is_none()
            && !history.uncertain
            && history.state.inference_checkpoint.as_ref() == Some(raw)
            && !self
                .intent
                .decisions
                .iter()
                .any(|d| d.starts_with(DISPATCH_PREFIX))
            && let Ok(project) = self.snapshot.root.canonicalize()
        {
            if let Ok(report) = nika_providers::admission::CompletedCostReport::read(
                &self.unknown_cost.observations,
            ) && report.matches_checkpoint(raw, project.as_os_str().as_encoded_bytes())
            {
                self.unknown_cost.completed_restored = true;
                notice.push_str(if self.money.preparation.is_some() {
                    "\nprior costs retained, including unknown charges; preparation can continue; Run is separate"
                } else { "\ncompleted unknown-cost observations retained; a fresh one-time cost review is required; no account or consent restored" });
                return;
            }
            match nika_providers::InferenceAdmission::from_checkpoint(
                raw,
                project.as_os_str().as_encoded_bytes(),
            ) {
                Ok((account, observed)) if self.unknown_cost.observations.contains(&observed) => {
                    self.unknown_cost.observations.retain(|o| o != &observed);
                    self.money.account = Some(account);
                    notice.push_str(if self.money.preparation.is_some() {
                        "\nprior expenses and reservations retained; preparation can continue; Run is separate"
                    } else { "\nnumeric inference ledger restored closed; confirm a new TOTAL Session ceiling in its own sentence, e.g. Budget: 10 USD. (total, not additional); prior expenses and reservations remain" });
                }
                Ok(_) => notice.push_str("\nledger refused: project cost observation differs"),
                Err(error) => {
                    let _ = write!(notice, "\nledger refused: {error}");
                }
            }
        }
    }

    /// The project's structured record, read at open (#1464) — after
    /// [`Self::enable_history`] when the door keeps one: the record wins
    /// over the transcript's ordinary goal/decisions/questions (the transcript
    /// keeps the dialogue); monetary restrictions are conserved from both.
    /// A proposal
    /// never survives a close (ADR-133 · nothing is written before its
    /// consent); a gate pending at close is the engine's own paused trace
    /// and waits again when that trace still carries the pause. A record
    /// that cannot be read is named and left in place, never rewritten.
    pub fn restore_state(&mut self) -> Option<String> {
        let state = match SessionState::load(&self.snapshot.root) {
            Ok(Some(state)) => state,
            Ok(None) => return None,
            Err(error) => {
                self.money.reconfirm = true;
                return Some(format!(
                    "session record unreadable (.nika/{STATE_FILE}: {error}) · left in place · inference exposure is unknown; paid continuation is blocked"
                ));
            }
        };
        self.unknown_cost.completed_restored = false;
        self.unknown_cost.observations = state.inference_observations;
        // A no-budget observation had no allowance to reconfirm: it stays
        // exposure, never a restriction. Any other observation restricts.
        if self
            .unknown_cost
            .observations
            .iter()
            .any(|o| o["unbudgeted"] != true)
        {
            self.money.reconfirm = true;
        }
        let checkpoint = state.inference_checkpoint;
        self.money.reconfirm |= checkpoint.is_some();
        let expired = self.restore_intent(IntentDraft {
            goal: state.goal,
            decisions: state.decisions,
            unresolved: state.unresolved,
        });
        let mut notice = format!(
            "session record restored (.nika/{STATE_FILE} · written {})",
            state.updated_at
        );
        notice.push_str(&expired);
        self.restore_checkpoint(checkpoint.as_ref(), &mut notice);
        for line in self
            .intent
            .decisions
            .iter()
            .filter(|d| d.starts_with(DISPATCH_PREFIX) || d.starts_with(OBSERVED_PREFIX))
        {
            let _ = write!(notice, "\n  ⚠ {line} · nothing was replayed");
        }
        if let Some(Pending::Gate {
            workflow,
            trace,
            task,
            ..
        }) = state.pending
        {
            // The pause is offered only where the journals beside it show no continuation that
            // settled it, runs it or cannot be judged (C7b §3.4).
            match PendingGate::from_trace(&workflow, &trace)
                .map(|gate| self.gate_standing(gate, false))
            {
                Some(Ok(gate)) => {
                    let seen = if gate.trace == trace {
                        "no continuation of this run in .nika/traces"
                    } else {
                        "the run was continued outside this session and paused again here"
                    };
                    let _ = write!(
                        notice,
                        "\n{}\n  ({seen} · the engine's approval still judges your answer)",
                        gate.question()
                    );
                    self.last_workflow = Some(workflow);
                    self.pending_gate = Some(gate);
                }
                Some(Err(why)) => {
                    let _ = write!(notice, "\n  {why}");
                }
                None => {
                    let _ = write!(
                        notice,
                        "\n  the run paused at `{task}` no longer waits: its trace `{}` carries no pause",
                        super::shown_trace(&self.snapshot.root, &trace)
                    );
                }
            }
        }
        self.restore_money_guards();
        Some(notice)
    }

    // Conversation and structured state can have different last-write times.
    // Replace ordinary prose, but never erase independently recorded constraints.
    fn restore_intent(&mut self, mut restored: IntentDraft) -> String {
        // Labels remain historical evidence, but no authoring/input round was
        // restored to accept an answer. Name that expiry before dropping them
        // from the current projection; opening neither calls nor writes. The
        // labels a readable kept round owns are its own line's, never expired.
        let kept = self.kept_round_labels();
        restored.unresolved.retain(|label| !kept.contains(label));
        let expired = if restored.unresolved.is_empty() {
            String::new()
        } else {
            format!(
                "\nprevious unanswered questions expired: {} · state the request again to continue",
                restored.unresolved.join(" · ")
            )
        };
        restored.unresolved.clear();
        for marker in self.intent.decisions.iter().filter(|d| is_money_marker(d)) {
            if !restored.decisions.contains(marker) {
                restored.decisions.push(marker.clone());
            }
        }
        self.intent = restored;
        self.money.reconfirm |= self
            .intent
            .decisions
            .iter()
            .any(|d| d == RECONFIRM || d.starts_with(DISPATCH_PREFIX));
        expired
    }

    /// Where the paused run of `gate` stands now, from the journals beside it (C7b §3.4): the
    /// gate this session may offer — its own, or the one a continuation paused at again — or why
    /// it offers none. One read of one directory: never an authorization, never atomic, never
    /// exactly-once; the engine's approval still judges any answer. `observed` is a pause this
    /// session saw itself: when the only doubt is that pause's own journal naming no run (not an
    /// engine journal) or no trace store existing at all, no continuation can be followed and
    /// it stands, as it did.
    fn gate_standing(&self, gate: PendingGate, observed: bool) -> Result<PendingGate, String> {
        let dir = self.snapshot.root.join(nika_dap::store::TRACE_DIR);
        let paused = dir.join(gate.trace.file_name().unwrap_or_default());
        let task = gate.task.clone();
        match lineage_of(&dir, &paused).standing(&paused, observed) {
            Standing::Stands => Ok(gate),
            Standing::PausedAgain(trace) => PendingGate::from_trace(&gate.workflow, &trace)
                .ok_or_else(|| {
                    format!(
                        "the run paused at `{task}` was continued outside this session and paused again, but that pause cannot be read (trace `{}`) · nothing is offered",
                        trace.file_name().unwrap_or_default().to_string_lossy()
                    )
                }),
            Standing::Settled { state, trace } => Err(format!(
                "the run paused at `{task}` was continued outside this session and ended {} (trace `{trace}`) · nothing waits · nothing was replayed",
                state.as_str()
            )),
            Standing::Running(liveness) => Err(format!(
                "the run paused at `{task}` was continued outside this session and has not settled ({}) · nothing waits here · nothing was replayed",
                liveness.map_or("liveness unknown", |l| l.as_str())
            )),
            Standing::Undecided(reasons) => Err(format!(
                "the run paused at `{task}` cannot be judged from .nika/traces ({}) · nothing is offered · `nika trace ls`, then a deliberate `nika run … --resume`, which the engine's approval guards",
                reasons.join(" · ")
            )),
            _ => Err(format!(
                "the run paused at `{task}` has a lineage this engine cannot read · nothing is offered"
            )),
        }
    }

    /// Before any resume: the gate still stands as it was offered, or the answer is not sent —
    /// a continuation the journals show (settled, running, paused again, several) or journals
    /// that can no longer be read withhold it.
    pub(super) fn stale_gate(&mut self, gate: &PendingGate) -> Option<TurnOutcome> {
        match self.gate_standing(gate.clone(), true) {
            Ok(now) if now.trace == gate.trace => None,
            Ok(head) => {
                let question = head.question();
                self.pending_gate = Some(head);
                Some(TurnOutcome::Facts(format!(
                    "the run was continued outside this session and waits again at another pause · your answer was not sent\n{question}"
                )))
            }
            Err(why) => Some(TurnOutcome::Facts(format!(
                "{why} · your answer was not sent"
            ))),
        }
    }

    fn restore_money_guards(&mut self) {
        let waiting = self.waiting_gate().as_ref().map(gate_money_marker);
        let mut matched = false;
        let mut lost = false;
        for marker in self
            .intent
            .decisions
            .iter()
            .filter(|d| d.starts_with(GATE_MONEY_PREFIX))
        {
            if waiting.as_ref() == Some(marker) {
                matched = true;
            } else {
                lost = true;
            }
        }
        if matched {
            self.restore_gate_money();
        }
        if lost {
            // Missing/different gate authority cannot release a monetary hold.
            // No scope can now prove completion, so only conservative refusal is safe.
            self.money.reconfirm = true;
            self.retain_money_guard();
        }
    }

    /// The durable evidence of a consent that landed every change (#1465),
    /// and the decision it is: the empty string, or the warning line the
    /// report must carry when the journal refused.
    pub(super) fn evidence_applied(
        &mut self,
        set: &ProjectChangeSet,
        id: &ProposalId,
        applied: &Applied,
    ) -> String {
        self.witness_consent(set, id, ConsentDecision::Applied, &applied.written)
    }

    /// The durable evidence of a consent whose later write was refused
    /// after earlier ones landed (#1465): exactly the write loop's record.
    pub(super) fn evidence_partial(
        &mut self,
        set: &ProjectChangeSet,
        id: &ProposalId,
        attempt: &ApplyAttempt,
    ) -> String {
        self.witness_consent(set, id, ConsentDecision::Partial, &attempt.written)
    }

    fn witness_consent(
        &mut self,
        set: &ProjectChangeSet,
        id: &ProposalId,
        decision: ConsentDecision,
        written: &[PathBuf],
    ) -> String {
        let paths: Vec<String> = written
            .iter()
            .map(|p| format!("`{}`", p.display()))
            .collect();
        self.intent.decisions.push(match decision {
            ConsentDecision::Applied => {
                format!("applied proposal {id} · wrote {}", paths.join(" · "))
            }
            ConsentDecision::Partial => format!(
                "proposal {id} landed partially · wrote {} · left undecided",
                paths.join(" · ")
            ),
            // The decision is non-exhaustive across the member boundary (ADR-144); this
            // session records only the two above, and names nothing it did not decide.
            _ => format!("proposal {id} · wrote {}", paths.join(" · ")),
        });
        let record = ConsentRecord::of(set, id, decision, written, now_rfc3339());
        match record.append(&self.snapshot.root) {
            Ok(()) => String::new(),
            Err(error) => format!(
                "\n  ⚠ the consent's evidence was not written (.nika/{CONSENTS_FILE}): {error}"
            ),
        }
    }

    /// The structured record as this session would write it now — the
    /// same redaction the transcript gets.
    fn projected_state(&self) -> SessionState {
        let redact = |s: &String| crate::broker::redact(s).0;
        let mut state = SessionState::new(now_rfc3339());
        state.inference_observations = self.cost_observations();
        state.inference_checkpoint = self.account_checkpoint();
        state.goal = self.intent.goal.as_ref().map(redact);
        state.decisions = self.saved_decisions();
        state.unresolved = self.intent.unresolved.iter().map(redact).collect();
        state.pending = self.pending_gate.as_ref().map(|gate| Pending::Gate {
            workflow: gate.workflow.clone(),
            trace: gate.trace.clone(),
            task: gate.task.clone(),
            mode: gate.mode.clone(),
        });
        state
    }

    /// Keep the structured record after an operation that decided or
    /// observed something. A refused write rides the outcome's own text:
    /// the effect happened, and the human must know the record did not.
    pub(super) fn save_cost_state(&self) -> Result<(), String> {
        self.projected_state()
            .save(&self.snapshot.root)
            .map_err(|e| e.to_string())
    }

    /// The record at a paid-dispatch boundary, written BEFORE any request can
    /// enter transport: this projection plus the in-flight line. The line lives
    /// only in the record, so reading it back means the writer left before the
    /// settlement's own write (S98 F10: a record never reads « Open · 0 calls »
    /// while a request may be in flight).
    pub(super) fn save_dispatch_boundary(&self) -> Result<(), String> {
        self.save_boundary(self.dispatch_marker())
    }

    /// The same write for any in-flight line, including a no-budget one.
    pub(super) fn save_boundary(&self, marker: Option<String>) -> Result<(), String> {
        let mut state = self.projected_state();
        if let Some(marker) = marker {
            state.decisions.push(crate::broker::redact(&marker).0);
        }
        state.save(&self.snapshot.root).map_err(|e| e.to_string())
    }

    fn keep_state(&self, outcome: TurnOutcome) -> TurnOutcome {
        match self.projected_state().save(&self.snapshot.root) {
            Ok(()) => outcome,
            Err(error) => with_note(
                outcome,
                &format!("the session record was not kept (.nika/{STATE_FILE}): {error}"),
            ),
        }
    }

    pub(super) fn preparation_snapshot(&self) -> PreparationBefore {
        PreparationBefore {
            goal: self.intent.goal.clone(),
            reading: self.last_outcome.clone(),
            proposal: self.pending.as_ref().map(|set| self.proposal_id(set)),
            question: self.authoring.clone(),
        }
    }

    /// Withdraw only the new preparation before host presentation; retain earlier waiting work.
    /// Gates, Run, monetary observations and the durable human intent are unchanged.
    pub fn withdraw_cancelled_preparation(&mut self) -> Option<String> {
        if !self
            .money
            .preparation
            .as_ref()
            .is_some_and(PreparationCosts::was_stopped)
        {
            return None;
        }
        let proposal = self.pending.as_ref().map(|set| self.proposal_id(set));
        let changed_proposal = proposal.is_some() && proposal != self.preparation_before.proposal;
        let changed_question =
            self.authoring.is_some() && self.authoring != self.preparation_before.question;
        if !changed_proposal && !changed_question {
            return None;
        }
        if changed_proposal {
            self.pending = None;
        }
        if changed_question {
            self.authoring = None;
            self.intent.unresolved.clear();
        }
        self.last_outcome
            .clone_from(&self.preparation_before.reading);
        let text = crate::authoring::AuthoringError::Cancelled.to_string();
        let outcome = self.recorded(Operation::Turn, "(preparation stopped)", |_| {
            TurnOutcome::Cancelled(text)
        });
        match outcome {
            TurnOutcome::Cancelled(text) => Some(text),
            _ => Some("preparation stopped; the conversation record could not be updated".into()),
        }
    }

    fn accept_preparation(
        &mut self,
        operation: Operation,
        previous: PreparationBefore,
        outcome: TurnOutcome,
    ) -> TurnOutcome {
        if matches!(
            operation,
            Operation::Run | Operation::Gate | Operation::Observation
        ) || matches!(
            outcome,
            TurnOutcome::RunRequested { .. } | TurnOutcome::ResumeRequested { .. }
        ) || (!matches!(outcome, TurnOutcome::Cancelled(_)) && !PreparationCosts::interrupted())
        {
            return outcome;
        }
        if self.pending.as_ref().map(|set| self.proposal_id(set)) != previous.proposal {
            self.pending = None;
        }
        if previous.goal.is_some() {
            self.intent.goal = previous.goal;
        }
        self.last_outcome = previous.reading;
        if self.authoring != previous.question {
            self.authoring = None;
            self.intent.unresolved.clear();
        }
        self.keep_revising(TurnOutcome::Cancelled(
            crate::authoring::AuthoringError::Cancelled.to_string(),
        ))
    }

    pub(super) fn recorded(
        &mut self,
        operation: Operation,
        input: &str,
        perform: impl FnOnce(&mut Self) -> TurnOutcome,
    ) -> TurnOutcome {
        let _cost_scope = self.money.preparation.as_ref().map(PreparationCosts::enter);
        let _activity_scope = self.progress.enter();
        let previous = self.preparation_snapshot();
        let mut history = match std::mem::replace(
            &mut self.history,
            HistoryMode::Blocked("the previous operation did not complete".to_owned()),
        ) {
            HistoryMode::Ephemeral => {
                self.history = HistoryMode::Ephemeral;
                let outcome = perform(self);
                return self.accept_preparation(operation, previous, outcome);
            }
            HistoryMode::Blocked(message) => {
                self.history = HistoryMode::Blocked(message.clone());
                return TurnOutcome::Refusal(history_refusal(message));
            }
            HistoryMode::Active(history) => history,
        };
        if let Err(error) = history.begin(operation, input) {
            return self.history_failed(error);
        }
        let charged_before = self.uncertain_charges();
        let outcome = perform(self);
        let outcome = self.accept_preparation(operation, previous, outcome);
        // A new proposal replaces a draft kept from an earlier session.
        if self.pending.is_some() {
            self.restored_draft = None;
        }
        self.drop_replaced_round();
        // A choice that resumed a waiting line is judged by that line's
        // own outcome: the record sees what the human's request became.
        let judged = match &outcome {
            TurnOutcome::Resumed { outcome, .. } => outcome.as_ref(),
            other => other,
        };
        let effect = if matches!(operation, Operation::Consent | Operation::Gate)
            && let TurnOutcome::Refusal(Refusal {
                class: RefusalClass::Io,
                text,
            }) = judged
        {
            let text = text.clone();
            self.remember("(effect)", &text);
            EffectState::Unknown
        } else if self.uncertain_charges() > charged_before {
            // This operation left a possibly billed request without usable
            // settlement: the history says so, as the record's observation does.
            EffectState::Unknown
        } else {
            EffectState::NoUncertaintyReported
        };
        let run = match (judged, operation) {
            (TurnOutcome::RunRequested { .. } | TurnOutcome::ResumeRequested { .. }, _) => {
                RunState::AwaitingObservation
            }
            (_, Operation::Observation) => RunState::Idle,
            _ => history.run,
        };
        let authority = if self.pending.is_some() {
            AuthorityState::Proposal
        } else if self.pending_gate.is_some() {
            AuthorityState::Gate
        } else {
            AuthorityState::None
        };
        if self.money.account.is_some()
            && let Err(error) = self.save_cost_state()
        {
            return self.history_failed(error);
        }
        let evidence = outcome_kind(&outcome).to_owned();
        if let Err(error) =
            history.complete(self.saved_conversation(), run, authority, evidence, effect)
        {
            return self.history_failed(error);
        }
        self.history = HistoryMode::Active(history);
        outcome
    }

    fn history_failed(&mut self, error: impl std::fmt::Display) -> TurnOutcome {
        let reason = format!(
            "{error}; the last operation may be incomplete — inspect the history and effects before retrying"
        );
        self.history = HistoryMode::Blocked(reason.clone());
        TurnOutcome::Refusal(history_refusal(reason))
    }

    fn saved_decisions(&self) -> Vec<String> {
        let mut decisions: Vec<_> = self
            .intent
            .decisions
            .iter()
            .map(|s| crate::broker::redact(s).0)
            .collect();
        // An unreadable record supplies no conversational facts, but its unknown
        // exposure must survive the next legitimate history/state write.
        if self.money.reconfirm && !decisions.iter().any(|d| d == RECONFIRM) {
            decisions.push(RECONFIRM.into());
        }
        decisions
    }

    fn account_checkpoint(&self) -> Option<serde_json::Value> {
        match self.snapshot.root.canonicalize() {
            Ok(root) => nika_providers::admission::accounting_checkpoint(
                self.money.account.as_ref(),
                &self.cost_observations(),
                root.as_os_str().as_encoded_bytes(),
            ),
            Err(e) => self
                .money
                .account
                .as_ref()
                .map(|_| serde_json::Value::String(e.to_string())),
        }
    }

    fn saved_conversation(&self) -> Saved {
        let redact = |s: &String| crate::broker::redact(s).0;
        Saved {
            goal: self.intent.goal.as_ref().map(redact),
            decisions: self.saved_decisions(),
            unresolved: self.intent.unresolved.iter().map(redact).collect(),
            recent: self
                .recent
                .iter()
                .map(|(a, b)| (redact(a), redact(b)))
                .collect(),
            pending: self
                .pending
                .as_ref()
                .or(self.revising.as_ref().map(|(set, _)| set))
                .and_then(|set| draft::capture(&self.proposal_id(set), set))
                .or_else(|| self.restored_draft.as_ref().map(|r| r.raw().clone())),
            round: self.round_to_keep(),
            programs: self.programs.clone(),
            inference_checkpoint: self.account_checkpoint(),
            last_run: self.kept_run.clone(),
            selection: (self.conversation.as_ref()).and_then(super::ConversationChoice::value),
        }
    }

    /// The last observed run, read back (unreadable: its reason, its bytes kept).
    #[must_use]
    pub fn kept_run(&self) -> Option<Result<KeptRun, String>> {
        self.kept_run.as_ref().map(KeptRun::from_value)
    }

    /// The recent turns kept (restored at open): what was said, never replayed.
    #[must_use]
    pub fn kept_turns(&self) -> &[(String, String)] {
        &self.recent
    }
}

// Persist no Debug representation of the payload. Debug escapes newlines
// before the line-oriented redactor can see them, and can expose secrets
// already removed from Saved. Dialogue lives in that redacted projection;
// this diagnostic is only a category, never an executable request.
fn outcome_kind(outcome: &TurnOutcome) -> &'static str {
    match outcome {
        TurnOutcome::Reply(_) => "reply",
        TurnOutcome::Facts(_) => "facts",
        TurnOutcome::Cancelled(_) => "cancelled",
        TurnOutcome::Help(_) => "help",
        TurnOutcome::Quit => "quit",
        TurnOutcome::Refusal(_) => "refusal",
        TurnOutcome::Ask(_) => "ask",
        TurnOutcome::Held { .. } => "held",
        TurnOutcome::Question { .. } => "question",
        TurnOutcome::Proposal { .. } => "proposal",
        TurnOutcome::RunRequested { .. } => "run_requested",
        TurnOutcome::GateAsk { .. } => "gate_ask",
        TurnOutcome::ResumeRequested { .. } => "resume_requested",
        TurnOutcome::Aside(_) => "aside",
        TurnOutcome::RunReviewed { .. } => "run_reviewed",
        TurnOutcome::Resumed { outcome, .. } => outcome_kind(outcome),
    }
}

/// The outcome with a warning line appended to whatever text it shows; an
/// outcome without text (a quit · a resume the door runs) carries none —
/// the observation that follows a resume keeps the record again.
fn with_note(outcome: TurnOutcome, note: &str) -> TurnOutcome {
    let line = format!("\n  ⚠ {note}");
    match outcome {
        TurnOutcome::Reply(text) => TurnOutcome::Reply(text + &line),
        TurnOutcome::Facts(text) => TurnOutcome::Facts(text + &line),
        TurnOutcome::Cancelled(text) => TurnOutcome::Cancelled(text + &line),
        TurnOutcome::Help(text) => TurnOutcome::Help(text + &line),
        TurnOutcome::Ask(text) => TurnOutcome::Ask(text + &line),
        TurnOutcome::Aside(text) => TurnOutcome::Aside(text + &line),
        TurnOutcome::Refusal(why) => {
            TurnOutcome::Refusal(Refusal::new(why.class, why.text + &line))
        }
        TurnOutcome::Held { id, preview } => TurnOutcome::Held {
            id,
            preview: preview + &line,
        },
        TurnOutcome::Proposal { id, preview } => TurnOutcome::Proposal {
            id,
            preview: preview + &line,
        },
        TurnOutcome::Question { key, question } => TurnOutcome::Question {
            key,
            question: question + &line,
        },
        TurnOutcome::RunRequested { report, run } => TurnOutcome::RunRequested {
            report: report + &line,
            run,
        },
        TurnOutcome::GateAsk { id, question } => TurnOutcome::GateAsk {
            id,
            question: question + &line,
        },
        TurnOutcome::Resumed { notice, outcome } => TurnOutcome::Resumed {
            notice,
            outcome: Box::new(with_note(*outcome, note)),
        },
        other @ (TurnOutcome::Quit
        | TurnOutcome::ResumeRequested { .. }
        | TurnOutcome::RunReviewed { .. }) => other,
    }
}

fn history_refusal(error: impl std::fmt::Display) -> Refusal {
    Refusal::new(
        RefusalClass::Io,
        format!(
            "conversation history unavailable: {error} · session stopped; close and reconcile before reopening"
        ),
    )
}
