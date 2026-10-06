// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The authoring turns: a free-text intent reaches the ONE compiler; its
//! typed question becomes the next line's meaning; a Ready candidate
//! becomes the proposal the consent line answers; an explicit `run …`
//! line runs an accepted workflow through the door. Each pending state
//! gives the next line exactly one typed meaning — a `yes` never crosses
//! from an authoring answer to a consent to a gate.

use nika_onboard::routing::run_options::{self, inline_vars, input_question, run_line_is_plain};
pub(super) use nika_onboard::routing::run_options::{is_run_verb, run_prefix};

use std::fmt::Write as _;
use std::path::PathBuf;

use nika_onboard::compile::program_records;
use nika_onboard::compile::round::{change_money, compiled};
use nika_onboard::compile::{CompileOutcome, CompileQuestion, CompileRequest, revise_intent};
// The compiler's reasons in a human's words and its questions' own grammar live beside the
// reading they refine (C10).
pub(crate) use nika_onboard::compile::reading::human_reasons;
pub(super) use nika_onboard::compile::reading::{asks_for_syntax, clause_of};
use nika_onboard::compile::reading::{clauses_understood, incomplete_words, syntax_prompt};
use nika_onboard::compile::seat::{priced_words, unpriced_cloud, unpriced_warning};

use super::{SessionRuntime, TurnOutcome, ceiling_in, named_files};
use crate::activity::{Activity, Phase};
use crate::authoring::{
    AuthoringContext, AuthoringError, AuthoringRound, AuthoringSeat, Reading, compile_in,
    is_cancel, is_greeting, reasons,
};
use crate::change::{
    ChangeError, ProjectChange, ProjectChangeSet, RunRequest, Witness, check_with_access,
};
use crate::outcome::{ProposalId, Refusal, RefusalClass};
use crate::review;
use crate::turn::{RouteRecord, RoutingMethod, SessionPhase, TurnAct, TurnDecision};

/// The seat of the readings the session settles without a model: zero calls, the project observed.
pub(super) const DETERMINISTIC: AuthoringSeat = AuthoringSeat::Deterministic { why: None };

impl SessionRuntime {
    /// The authoring question the next line answers, when one is open.
    #[must_use]
    pub fn pending_question(&self) -> Option<&CompileQuestion> {
        self.authoring.as_ref().and_then(AuthoringRound::current)
    }

    /// The seat authoring reasons with (the banner names it).
    #[must_use]
    pub fn authoring_seat(&self) -> &AuthoringSeat {
        &self.seat
    }

    /// Re-derive the seat from the reasoner in place (open · `/intelligence`).
    pub(super) fn refresh_seat(&mut self) {
        self.seat = AuthoringSeat::from_reasoner(self.reasoner.as_ref(), &self.intelligence);
    }

    /// The authoring context a provider seat authors under: the strategy and
    /// the knowledge snapshot pinned for this session (`/status` names it).
    #[must_use]
    pub fn authoring_context(&self) -> &AuthoringContext {
        &self.authoring_context
    }

    /// A host's explicit authoring context, in place of the one the session
    /// read when it opened. The seat is unchanged: the context never selects
    /// a model, and a deterministic seat never reads it.
    pub fn set_authoring_context(&mut self, context: AuthoringContext) {
        self.authoring_context = context;
    }

    /// A request that is not a round (a revision, a request read again with
    /// its change) through the seat under the session's context, its pack
    /// composed for `intent`, bracketed like every other dispatch.
    pub(super) fn compile_request(
        &mut self,
        request: &CompileRequest,
        intent: &str,
    ) -> Result<CompileOutcome, AuthoringError> {
        self.rehearsals.clear_native();
        let context = self.project_context();
        if self.money_blocks_cognition() {
            return compile_in(&DETERMINISTIC, &context, request, intent);
        }
        let seat = self.seat.clone();
        if !seat.has_model() {
            return self.seated(&seat, |account| {
                crate::authoring::compile_in_rehearsed(
                    &seat, &context, request, intent, account, None,
                )
            });
        }
        self.rehearse_dispatch(intent, |this, host| {
            this.seated(&seat, |account| {
                crate::authoring::compile_in_rehearsed(
                    &seat,
                    &context,
                    request,
                    intent,
                    account,
                    Some(host),
                )
            })
        })
    }

    /// The session's authoring context rooted at its own project, never the process's working
    /// directory: every round observes the files it names there, never through a link outside it
    /// (R4 S1). A seated round roots the session's context with it; a deterministic one reads it.
    pub(super) fn project_context(&self) -> AuthoringContext {
        let root = self.snapshot.root.clone();
        self.authoring_context.clone().with_project_root(root)
    }

    /// A free-text line as work to build: the deterministic ladder first
    /// (zero calls · the compiler's own order), then the seat when the
    /// reader read work it cannot settle alone. `None` when nothing in
    /// the line reads as work — the conversation owns that line.
    pub(super) fn author_unrecorded(&mut self, intent: &str) -> Option<TurnOutcome> {
        // A lone greeting is the conversation's before any door reads it:
        // the compiler's exact-skeleton door would take `hello` literally.
        if is_greeting(intent) {
            return None;
        }
        let mut round = AuthoringRound::new(intent);
        round.money.clone_from(&self.money.admitted);
        let context = self.project_context();
        let out = match compile_in(&DETERMINISTIC, &context, &round.request(), intent) {
            Ok(out) => out,
            Err(e) => return Some(self.machinery(&e)),
        };
        // What the compiler's ledger recorded of the reading — a count,
        // never a claim that it was verified.
        if let Some(n) = clauses_understood(&out) {
            self.activity(&Activity::done(
                Phase::Understanding,
                format!("recorded {n} requirement{}", if n == 1 { "" } else { "s" }),
            ));
        }
        let reading = super::route::as_written(Reading::of(out), &round, &context, intent);
        // Only work owns the automation goal. Keep this round before any
        // seat/admission failure; an earlier conversation is not its request.
        // The request itself is never a goal beside itself: resumed after the
        // intelligence choice or typed again after a reopening, it IS the goal.
        let earlier = (self.intent.goal.clone())
            .filter(|goal| goal.trim() != round.effective_intent().trim());
        if !matches!(reading, Reading::NotWork(_)) {
            self.intent.goal = Some(round.effective_intent());
        }
        match reading {
            // Not work the reader knows: open language. A question-shaped
            // line is the conversation's at once (a fast path, never a veto:
            // nothing waits, so nothing can be modified); otherwise the act
            // is a bounded decision — new work goes to the seat, a change
            // revises the saved workflow, the rest is the conversation's
            // (the intelligence sees the line either way).
            Reading::NotWork(_) => {
                if super::route::question_outside_money(intent, &round.money) {
                    return None;
                }
                let seat_reads = self.seat.has_model();
                match self.classify(SessionPhase::Idle, intent).act {
                    // Work the deterministic reader did not recognise, routed
                    // as new work: the seat reads it, files named or not;
                    // without a seat the first screen is asked in context,
                    // as for an unsettled reading (never a conversational
                    // paraphrase of a plan that nothing will build).
                    TurnAct::NewWork => {
                        self.intent.goal = Some(round.effective_intent());
                        if seat_reads {
                            Some(self.compile_under_seat(round))
                        } else if !self.chosen || !self.intelligence.ready {
                            Some(self.ask_for_intelligence(intent, super::Need::Authoring))
                        } else {
                            None
                        }
                    }
                    TurnAct::Modify | TurnAct::Mixed => self.revise_current(intent),
                    _ => None,
                }
            }
            Reading::Unsettled(out) => Some(match &self.seat {
                // The ceiling refuses the seat (R4 A6): what the reader could not settle is
                // still said — a directive that also names a field is the human's to restate.
                AuthoringSeat::Provider { .. } | AuthoringSeat::Harness { .. }
                    if self.money_blocks_cognition() =>
                {
                    self.refused_unsettled(out)
                }
                AuthoringSeat::Provider { .. } | AuthoringSeat::Harness { .. } => self
                    .beside_goal(intent, earlier)
                    .unwrap_or_else(|| self.compile_under_seat(round)),
                // No usable intelligence is chosen: ask here, in context,
                // and resume this request under the resulting choice.
                AuthoringSeat::Unavailable { .. } | AuthoringSeat::Deterministic { .. }
                    if !self.chosen || !self.intelligence.ready =>
                {
                    self.ask_for_intelligence(intent, super::Need::Authoring)
                }
                AuthoringSeat::Unavailable { why } => {
                    self.machinery(&AuthoringError::Seat(why.clone()))
                }
                AuthoringSeat::Deterministic { why } => {
                    let text = incomplete_words(&out, why.as_deref());
                    self.last_outcome = Some(out);
                    TurnOutcome::Facts(text)
                }
            }),
            reading => Some(self.settle(round, reading)),
        }
    }

    /// Work the reader could not settle, under a ceiling that refuses the seat (R4 A6): the
    /// refusal says the compiler's reasons, never the bare ceiling; `why` keeps the outcome.
    fn refused_unsettled(&mut self, out: CompileOutcome) -> TurnOutcome {
        let text = incomplete_words(&out, Some(&self.cognition_blocked()));
        self.last_outcome = Some(out);
        TurnOutcome::Refusal(Refusal::new(RefusalClass::NotAllowed, text))
    }

    /// A saved file or unfinished goal gives a change its context. The classifier sees that
    /// earlier request, never this line substituted for it; `NEW_WORK` inherits none of it.
    fn beside_goal(&mut self, intent: &str, earlier: Option<String>) -> Option<TurnOutcome> {
        if self.last_workflow.is_none() && earlier.is_none() {
            return None;
        }
        let current = std::mem::replace(&mut self.intent.goal, earlier);
        let before = self.inference_receipt().ok().flatten();
        let decision = self.classify(SessionPhase::Idle, intent);
        // Only the shared account refusing the label before any send keeps today's truthful
        // admission card: the authoring reservation it would make is refused the same way.
        let refused =
            decision.method == RoutingMethod::Failed && self.label_refused(before.as_ref());
        if decision.act == TurnAct::NewWork || refused {
            self.intent.goal = current;
            return None;
        }
        match decision.act {
            TurnAct::Modify | TurnAct::Mixed => self.revise_current(intent),
            _ => Some(TurnOutcome::Facts(Self::unknown_route_text(
                SessionPhase::Idle,
                decision.method,
            ))),
        }
    }

    /// No source exists after an interrupted creation: restate its request, never invent an
    /// EDIT base, replay an interrupted plan or restore permission to save or run.
    fn revise_current(&mut self, change: &str) -> Option<TurnOutcome> {
        if let Some(saved) = self.last_workflow.clone() {
            return Some(self.revise_saved(&saved, change));
        }
        let original = self.intent.goal.as_ref()?;
        let intent = format!(
            "Original request:\n{original}\nCorrection (takes precedence over the original where it changes it; keep the other requirements):\n{change}"
        );
        Some(self.restate_request(intent, change))
    }

    /// The same Compile, under the seat the human permitted, for work the
    /// deterministic policy could not settle. How long it takes and how
    /// hard it thinks is the compiler's; the human only learns that Nika
    /// is working (a truthful line, no invented detail).
    fn compile_under_seat(&mut self, round: AuthoringRound) -> TurnOutcome {
        if self.money_blocks_cognition() {
            return self.cognition_money_refusal();
        }
        self.authoring_context = self.project_context();
        self.activity(&Activity::now(Phase::Authoring, self.authoring_note()));
        match self.compile_round(&round, &self.seat.clone()) {
            Ok(out) if nika_onboard::compile::round::awaiting_judge(&out) => {
                self.keep_unjudged(round, out)
            }
            Ok(out) => match Reading::of(out) {
                // The seat could not settle it: Nika keeps working — once
                // more with the provider's stronger model (the product law:
                // quality first) — before it says, in its own words, what
                // it could not express. Never « rephrase with details ».
                Reading::Unsettled(out) | Reading::NotWork(out) => {
                    if let Some(stronger) = self.stronger_seat() {
                        self.activity(&Activity::now(
                            Phase::Repairing,
                            "still working · a stronger model reads it",
                        ));
                        return match self.compile_round(&round, &stronger) {
                            Ok(again) if nika_onboard::compile::round::awaiting_judge(&again) => {
                                self.keep_unjudged(round, again)
                            }
                            Ok(again) => match Reading::of(again) {
                                Reading::Unsettled(again) | Reading::NotWork(again) => {
                                    self.cannot_express(again)
                                }
                                reading => self.settle(round, reading),
                            },
                            Err(e) => self.machinery(&e),
                        };
                    }
                    self.cannot_express(out)
                }
                reading => self.settle(round, reading),
            },
            Err(e) => self.machinery(&e),
        }
    }

    /// A stronger provider seat is available only for an unnamed, unmetered
    /// default. A human-named model and a bounded account retain their identity.
    pub(super) fn stronger_seat(&self) -> Option<AuthoringSeat> {
        if self.money.account.is_some() || self.intelligence.model.is_some() {
            return None;
        }
        let AuthoringSeat::Provider { model } = &self.seat else {
            return None;
        };
        let overridden = crate::authoring::openai_base_overridden();
        crate::authoring::stronger_model_under(model, overridden).map(|m| AuthoringSeat::Provider {
            model: m.to_owned(),
        })
    }

    /// What Nika could not express, in its own words: what stopped it (the
    /// compiler's reasons), what helps — never a request for syntax, never
    /// « rephrase with implementation details ».
    fn cannot_express(&mut self, out: CompileOutcome) -> TurnOutcome {
        let text =
            held_words(&out, self.seat.has_model()).unwrap_or_else(|| cannot_express_text(&out));
        self.last_outcome = Some(out);
        TurnOutcome::Facts(text)
    }

    /// What the reader could not settle, or a native finish held for its round's judge.
    fn unsettled_words(&self, out: &CompileOutcome) -> String {
        held_words(out, self.seat.has_model()).unwrap_or_else(|| incomplete_words(out, None))
    }

    /// A change, a mixed line or new work said at a question: the request
    /// is read again with the human's own words (the round is dropped, the
    /// plan read a different request). Never a paraphrase.
    pub(super) fn restate_round(&mut self, round: &AuthoringRound, line: &str) -> TurnOutcome {
        self.restate_request(format!("{}. {}", round.intent, line.trim()), line)
    }

    /// Compile the host's exact reconstructed request; all money spans bind to these bytes.
    fn restate_request(&mut self, intent: String, line: &str) -> TurnOutcome {
        // Its directives, the request's own and the added ones, are spans of this very string (C11).
        let money = match self.built_money(&intent, line) {
            Ok(money) => money,
            Err(refusal) => return refusal,
        };
        self.remember(line, "(the request read again with these words)");
        let mut again = AuthoringRound::new(intent);
        again.money = money;
        self.intent.goal = Some(again.effective_intent());
        match self.compile_request(&again.request(), &again.intent) {
            Ok(out) => {
                let reading = Reading::of(out);
                self.settle(again, reading)
            }
            Err(e) => self.machinery(&e),
        }
    }

    /// Propose the revised bytes and the Meaning delta while retaining the
    /// original request and the human's change as the source of truth.
    pub(super) fn propose_revision(
        &mut self,
        round: &AuthoringRound,
        out: &CompileOutcome,
    ) -> TurnOutcome {
        let delta = self
            .last_outcome
            .as_ref()
            .and_then(|base| base.provenance.decision.as_ref()?.get("ledger").cloned())
            .zip(
                out.provenance
                    .decision
                    .as_ref()
                    .and_then(|d| d.get("ledger").cloned()),
            )
            .and_then(|(before, after)| crate::meaning::delta(&before, &after));
        match self.propose(round, out) {
            TurnOutcome::Proposal { id, preview } => {
                // The Meaning and details doors must describe the bytes now
                // awaiting consent. Keep the earlier reading if proposal fails;
                // a rehearsed selection is already the one `propose` installed.
                if !self.rehearsed_pending(&id) {
                    self.last_outcome = Some(out.clone());
                }
                let mut text = preview;
                if let Some(delta) = delta {
                    text.push('\n');
                    text.push_str(&delta);
                }
                TurnOutcome::Proposal { id, preview: text }
            }
            other => other,
        }
    }

    /// A change said while nothing waits and a workflow was accepted: the
    /// saved workflow is the base, the human's words the change; the
    /// revision is a new proposal beside it, through the compiler's edit door.
    /// An unsuccessful edit never becomes a fresh request without its base.
    pub(super) fn revise_saved(&mut self, saved: &std::path::Path, change: &str) -> TurnOutcome {
        let root = self.snapshot.root.clone();
        let Ok(base) = std::fs::read_to_string(root.join(saved)) else {
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::WrongState,
                format!(
                    "`{}` cannot be read any more — describe the work again",
                    saved.display()
                ),
            ));
        };
        let path = saved.strip_prefix(&root).unwrap_or(saved).to_path_buf();
        let plan = program_records::plan(
            self.programs.as_ref(),
            program_records::Place::Saved(&path.to_string_lossy()),
            &base,
        );
        let original = plan
            .as_ref()
            .and_then(program_records::original)
            .map(str::to_owned)
            .or_else(|| self.intent.goal.clone());
        let goal = original
            .clone()
            .unwrap_or_else(|| format!("the workflow `{}`", saved.display()));
        let goal = format!("{goal} — {}", change.trim());
        self.activity(&Activity::now(
            Phase::Authoring,
            "revising with your change",
        ));
        // The revision reads the change beside the request the saved workflow
        // answered (the whole meaning, never the change alone), and its
        // knowledge is composed for that same request from the pinned snapshot.
        let mut round = AuthoringRound::new(goal.clone());
        // The proposal updates this file over exactly the bytes read here (a later selection
        // never retargets it), and refuses if they move before it is proposed.
        let witness = Witness::of(base.as_bytes());
        // Before any cognition: the file's own ceiling, or this revision's stated one.
        if let Err(refused) = self.bind_revision_money(&path, change.trim(), &witness) {
            return refused;
        }
        round.target = Some((path, witness));
        round.continuation = plan;
        round.edit = Some((base, change.trim().to_owned(), original));
        // The line's own admitted directives, as the law reads the change the EDIT holds (B15).
        round.money = change_money(change.trim(), !self.money.admitted.is_empty());
        let request = round.request();
        let revised = revise_intent(&request).unwrap_or_else(|| goal.clone());
        let out = match self.compile_request(&request, &revised) {
            Ok(out) => out,
            Err(e) => return self.machinery(&e),
        };
        match Reading::of(out) {
            Reading::Ready(out) => {
                self.remember(change, "(revised the saved workflow)");
                self.propose(&round, &out)
            }
            // What the revision asks waits in its round: the same EDIT, answered.
            reading if round.asks(&reading) => self.settle(round, reading),
            reading => {
                let why = human_reasons(reasons(reading.outcome())).join(" · ");
                let way = revision_way(reading.outcome());
                self.last_outcome = Some(reading.outcome().clone());
                TurnOutcome::Facts(format!(
                    "I could not revise `{}` with « {} »{} — the saved workflow is unchanged · {way}",
                    saved.display(),
                    change.trim(),
                    if why.is_empty() {
                        String::new()
                    } else {
                        format!(" ({why})")
                    }
                ))
            }
        }
    }

    /// A change said at the consent prompt: the compiler revises the
    /// pending candidate through its edit door (base bytes + the change in
    /// words), the meaning is read again and the new proposal replaces the
    /// old one. When the revision cannot settle, the old proposal still waits.
    pub(super) fn revise_pending(
        &mut self,
        set: crate::change::ProjectChangeSet,
        change: &str,
    ) -> TurnOutcome {
        self.revise_pending_with(set, change, |this, request, intent| {
            this.compile_request(request, intent)
        })
    }

    /// The same revision boundary with its compiler call explicit: the production caller uses
    /// `compile_request`, and boundary tests can supply a result without a provider or Runtime.
    pub(super) fn revise_pending_with(
        &mut self,
        set: crate::change::ProjectChangeSet,
        change: &str,
        compile: impl FnOnce(&mut Self, &CompileRequest, &str) -> Result<CompileOutcome, AuthoringError>,
    ) -> TurnOutcome {
        // The closed Copy proposal keeps its contract. Native revisions need a fresh proof.
        if let Some(held) = self.rehearsed_change(&set) {
            return held;
        }
        let base = set.changes.iter().find_map(|c| match c {
            crate::change::ProjectChange::CreateWorkflow { content, .. }
            | crate::change::ProjectChange::UpdateWorkflow { content, .. } => Some(content.clone()),
            _ => None,
        });
        let Some(base) = base else {
            let id = self.proposal_id(&set);
            self.pending = Some(set);
            return TurnOutcome::Held {
                id,
                preview: "the proposal carries no workflow to revise\n(the proposal still waits · `yes` applies it · `no` discards it)".to_owned(),
            };
        };
        if let Err(refused) = self.suspend_native_rehearsal(&set) {
            return refused;
        }
        let previous = self.last_outcome.clone();
        let goal = format!("{} — {}", set.goal, change.trim());
        self.activity(&Activity::now(
            Phase::Authoring,
            "revising with your change",
        ));
        // The compiler sees the exact base, original request and raw change.
        // Its bounded edit/repair loop owns the revision; a model paraphrase
        // must never replace these inputs through a fresh Create request.
        let mut round = AuthoringRound::new(goal.clone());
        round.continuation = program_records::plan(
            self.programs.as_ref(),
            program_records::Place::Proposal(&self.proposal_id(&set).to_string()),
            &base,
        );
        round.edit = Some((base, change.trim().to_owned(), Some(set.goal.clone())));
        // A proposal that updates a saved file keeps that file and its witness; a fresh
        // creation keeps its own fresh destination.
        round.target = updated_target(&set);
        let request = round.request();
        let revised = revise_intent(&request).unwrap_or_else(|| goal.clone());
        let out = match compile(self, &request, &revised) {
            Ok(out) => out,
            Err(e) => {
                if let Err(refused) = self.restore_native_rehearsal(&set) {
                    return refused;
                }
                self.pending = Some(set);
                return self.machinery(&e);
            }
        };
        self.settle_pending_revision(set, previous, round, change, out)
    }

    fn settle_pending_revision(
        &mut self,
        set: crate::change::ProjectChangeSet,
        previous: Option<CompileOutcome>,
        round: AuthoringRound,
        change: &str,
        out: CompileOutcome,
    ) -> TurnOutcome {
        if nika_onboard::compile::round::awaiting_judge(&out) {
            self.revising = Some((set, previous));
            return self.keep_unjudged(round, out);
        }
        match Reading::of(out) {
            Reading::Ready(out) => {
                self.remember(change, "(revised the proposal)");
                let outcome = self.propose_revision(&round, &out);
                if matches!(outcome, TurnOutcome::Proposal { .. }) {
                    self.finish_native_revision();
                } else {
                    self.pending = None;
                    if let Err(refused) = self.restore_native_rehearsal(&set) {
                        return refused;
                    }
                    self.pending = Some(set);
                    self.last_outcome = previous;
                }
                outcome
            }
            // What the revision asks (a value, a clause's disposition) waits in its round; the
            // proposal it revises waits aside, never consentable meanwhile (`keep_revising`).
            reading if round.asks(&reading) => {
                self.revising = Some((set, previous));
                let asked = self.settle(round, reading);
                self.keep_revising(asked)
            }
            reading => {
                if let Err(refused) = self.restore_native_rehearsal(&set) {
                    return refused;
                }
                let id = self.proposal_id(&set);
                let why = human_reasons(reasons(reading.outcome())).join(" · ");
                let way = revision_way(reading.outcome());
                self.pending = Some(set);
                TurnOutcome::Held {
                    id,
                    preview: format!(
                        "I could not revise the proposal with « {} »{}\n(the proposal still waits · `yes` applies it · `no` discards it · {way})",
                        change.trim(),
                        if why.is_empty() {
                            String::new()
                        } else {
                            format!(" — {why}")
                        }
                    ),
                }
            }
        }
    }

    /// After a line at a revision's question: it still waits, the revised proposal replaced the
    /// one it revises, or that one waits again exactly as it was — an answer never applies it.
    pub(super) fn keep_revising(&mut self, outcome: TurnOutcome) -> TurnOutcome {
        let kept = self.revising.take_if(|_| self.authoring.is_none());
        let Some((set, reading)) = kept else {
            return outcome;
        };
        if self.pending.is_some() {
            self.finish_native_revision();
            return outcome;
        }
        if let Err(refused) = self.restore_native_rehearsal(&set) {
            return refused;
        }
        let id = ProposalId::of(&self.draft_preview(&set));
        self.last_outcome = reading;
        self.intent.unresolved.clear();
        self.bind_proposal_money(&id);
        match outcome {
            TurnOutcome::Facts(text) => self.hold_pending(set, id, &text),
            other => {
                self.pending = Some(set);
                other
            }
        }
    }

    /// The compiler's machinery failed under a seat, or the compiler
    /// itself: a seat failure is a recovery (the goal is kept, the ways on
    /// are named); a compiler failure is a refusal that names it; an
    /// authoring configuration that cannot be honored is refused before
    /// anything is sent — never authored without the knowledge it names.
    pub(super) fn machinery(&mut self, error: &AuthoringError) -> TurnOutcome {
        match error {
            AuthoringError::Cancelled => TurnOutcome::Cancelled(error.to_string()),
            AuthoringError::Seat(_) => self.recovery(
                Some(RefusalClass::IntelligenceRefused),
                "I couldn't use the authoring seat for this part",
                &error.to_string(),
            ),
            AuthoringError::Context(_) => TurnOutcome::Refusal(Refusal::new(
                RefusalClass::AuthoringRefused,
                format!(
                    "{error} · nothing was sent to the authoring model, nothing was written · fix or unset the knowledge (NIKA_KNOWLEDGE · NIKA_AUTHORING_STRATEGY) and open the session again"
                ),
            )),
            AuthoringError::Compiler(_) | AuthoringError::Runtime(_) => {
                TurnOutcome::Refusal(Refusal::new(
                    RefusalClass::AuthoringRefused,
                    format!("{error} · nothing was written and nothing was substituted"),
                ))
            }
        }
    }

    /// What a reading becomes for the human: a proposal, a question, an
    /// honest incomplete, a refusal.
    pub(super) fn settle(&mut self, round: AuthoringRound, reading: Reading) -> TurnOutcome {
        if nika_onboard::compile::round::awaiting_judge(reading.outcome()) {
            return self.keep_unjudged(round, reading.outcome().clone());
        }
        self.settle_reading(round, reading)
    }

    fn settle_reading(&mut self, mut round: AuthoringRound, reading: Reading) -> TurnOutcome {
        // The compiler's reading is what `/meaning` shows, clause by clause.
        self.last_outcome = Some(reading.outcome().clone());
        // A revision left unsettled may still ask its clauses' dispositions (`absorb`).
        let reading = match reading {
            Reading::Unsettled(out) if round.edit.is_some() => Reading::Questions(out),
            reading => reading,
        };
        match reading {
            // The revised proposal replaces the one it revises: the delta reads from that one.
            Reading::Ready(out) if self.revising.is_some() => {
                self.last_outcome = self.revising.as_ref().and_then(|(_, base)| base.clone());
                self.propose_revision(&round, &out)
            }
            Reading::Ready(out) => self.propose(&round, &out),
            Reading::Questions(out) => {
                round.absorb(&out);
                let Some(question) = round.current() else {
                    return TurnOutcome::Facts(self.unsettled_words(&out));
                };
                let key = question.key.clone();
                // A rule the compiler can only ask as code is never asked
                // as code: once, the clause is asked in words (the human's
                // words replace it in the request); a second time, or a
                // clause the request does not carry verbatim, is an honest
                // incomplete that names the way on — as for a revision, whose
                // EDIT is never restated as a fresh request.
                if asks_for_syntax(question) {
                    let clause = clause_of(&question.label);
                    let carried = clause.as_deref().is_some_and(|c| round.intent.contains(c));
                    if round.restatements > 0 || !carried || round.edit.is_some() {
                        self.intent.unresolved.clear();
                        let text = syntax_incomplete(clause.as_deref());
                        self.remember(&round.intent, &text);
                        return TurnOutcome::Facts(text);
                    }
                    let clause = clause.as_deref().unwrap_or_default();
                    let text = syntax_prompt(clause);
                    self.intent.unresolved = vec![text.clone()];
                    self.remember(&round.intent, &text);
                    self.questions.ask();
                    self.authoring = Some(round);
                    return TurnOutcome::Question {
                        key,
                        question: text,
                    };
                }
                let mut text = question_text(question, &round.reasons);
                if key == "model"
                    && let Some(offer) = seat_offer(&self.seat)
                {
                    let _ = write!(text, "\n  {offer}");
                }
                if let Some(notice) = self.as_typed_notice(question) {
                    let _ = write!(text, "\n  {notice}");
                }
                self.intent.unresolved = vec![question.label.clone()];
                self.remember(&round.intent, &text);
                self.questions.ask();
                self.authoring = Some(round);
                TurnOutcome::Question {
                    key,
                    question: text,
                }
            }
            Reading::Unsettled(out) | Reading::NotWork(out) => {
                TurnOutcome::Facts(self.unsettled_words(&out))
            }
            // A turn the session could not finish: the recovery card (what
            // is kept · what did not happen · the ways on), never a bare
            // « failed ». The round is not kept: the human says it again.
            Reading::BudgetExhausted(out) => self.recovery(
                None,
                nika_onboard::compile::reading::authoring_budget_headline(
                    out.provenance.authoring.as_ref(),
                ),
                &reasons(&out).join(" · "),
            ),
            Reading::ProviderFailed(out) => self.recovery(
                Some(RefusalClass::IntelligenceRefused),
                "I couldn't use the authoring model for this part",
                &reasons(&out).join(" · "),
            ),
            Reading::Refused(out) => TurnOutcome::Refusal(Refusal::new(
                RefusalClass::AuthoringRefused,
                format!(
                    "the compiler refused this request — {}",
                    reasons(&out).join(" · ")
                ),
            )),
            // A reading this session does not know yet is refused, never proposed.
            other => TurnOutcome::Refusal(Refusal::new(
                RefusalClass::AuthoringRefused,
                format!(
                    "the compiler refused this request — {}",
                    reasons(other.outcome()).join(" · ")
                ),
            )),
        }
    }

    /// The Ready candidate of `round` as the proposal the consent line answers:
    /// exact bytes, a fresh destination, the same check facade.
    fn propose(&mut self, round: &AuthoringRound, out: &CompileOutcome) -> TurnOutcome {
        // A closed copy is proposed only as the candidate its rehearsals selected (`rehearsed.rs`).
        let qualified = match self.rehearse_copy(round) {
            Ok(qualified) => qualified,
            Err(refused) => {
                self.rehearsals.clear_native();
                return refused;
            }
        };
        let out = qualified.as_ref().map_or(out, |q| &q.outcome);
        let root = &self.snapshot.root;
        let proposed = match &round.target {
            Some((path, base)) => review::propose_over(root, &round.intent, path, base, out),
            None => review::propose(root, &round.intent, out),
        };
        match proposed {
            Ok(set) => {
                let bytes = self.draft_preview(&set);
                let id = ProposalId::of(&bytes);
                // What the candidate records of its sources is bound before any yes (F4).
                self.bind_basis(&id, &set, compiled(round.request(), out), out);
                let mut preview = self.draft_review(&set, out, &bytes);
                if let Err(refused) =
                    self.bind_rehearsal(&id, qualified.as_ref(), round, out, &mut preview)
                {
                    return refused;
                }
                if nika_providers::authoring::preparation::PreparationCosts::stopped() {
                    return self.machinery(&AuthoringError::Cancelled);
                }
                self.authoring = None;
                self.intent.unresolved.clear();
                self.remember(&round.intent, &format!("(proposed {id})"));
                // The schedule the request asked for rides beside the set:
                // « activate » declares it once the program is saved.
                self.pending_trigger.clone_from(&out.requested_trigger);
                self.bind_proposal_money(&id);
                self.pending = Some(set);
                TurnOutcome::Proposal { id, preview }
            }
            // The saved file moved since the revision read it: nothing proposed, nothing written.
            Err(ChangeError::Stale(path)) if round.target.is_some() => {
                TurnOutcome::Refusal(Refusal::new(
                    RefusalClass::StaleRevision,
                    format!(
                        "`{path}` changed since this revision read it — nothing was proposed or written · say the change again to revise the file as it is now"
                    ),
                ))
            }
            Err(e) => TurnOutcome::Refusal(Refusal::from_change(&e)),
        }
    }

    /// The human's line as the answer to the open question: typed to its
    /// shape, bound to its key, and the same plan replayed. Its protocol
    /// (`why` · a cancel word · a command) was answered before any review
    /// (`question_protocol`); an empty line is not an answer.
    pub(super) fn answer_question_unrecorded(&mut self, line: &str) -> TurnOutcome {
        let Some(round) = self.authoring.take() else {
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::WrongState,
                "no authoring question waits",
            ));
        };
        // `drop` is the disposition a clause's question names (`gap.N`): its answer there.
        let disposes = line.trim().eq_ignore_ascii_case("drop")
            && round.current().is_some_and(|q| q.key.starts_with("gap."));
        // An empty line takes the offered default and nothing else: the
        // `model` question's default is the seat the human already chose.
        let seat_default = match (round.current().map(|q| q.key.as_str()), &self.seat) {
            (Some("model"), AuthoringSeat::Provider { model }) => Some(model.clone()),
            _ => None,
        };
        let line = if line.trim().is_empty() {
            let Some(default) = seat_default else {
                self.authoring = Some(round);
                return TurnOutcome::Refusal(Refusal::new(
                    RefusalClass::EmptyAnswer,
                    "the question needs an answer — nothing answers for you (`cancel` drops it)",
                ));
            };
            default
        } else {
            line.to_owned()
        };
        let line = line.as_str();
        let routed = if disposes {
            Ok(round)
        } else {
            self.route_at_question(round, line)
        };
        let round = match routed {
            Ok(round) => round,
            Err(outcome) => return outcome,
        };
        // The clause asked in words: the human's words take its place in
        // the request, which the compiler reads again — a fresh round (the
        // plan read a different request), one restatement counted.
        if round.current().is_some_and(asks_for_syntax) {
            let clause = round
                .current()
                .and_then(|q| clause_of(&q.label))
                .unwrap_or_default();
            let mut restated = round.restate_clause(&clause, line.trim());
            restated.money = match self.built_money(&restated.intent, line) {
                Ok(money) => money,
                Err(refusal) => return refusal,
            };
            let asked = self.question_id_of(&round);
            self.questions.close(asked);
            self.remember(line, &format!("(restated « {clause} » in words)"));
            return self.compile_again(restated);
        }
        // A cloud model the catalog does not price is refused HERE, in
        // words, with the priced models of its provider — not at run time,
        // where a run under a spending ceiling refuses it (NIKA-1709). The
        // seat itself is never refused: the human chose it on the first
        // screen, told where its bytes go (a gateway row, a legacy name the
        // vendor still serves); a ceiling-bound run says its own word.
        if round.current().is_some_and(|q| q.key == "model")
            && !is_own_seat(&self.seat, line)
            && let Some(text) = unpriced_model_text(line)
        {
            self.authoring = Some(round);
            return TurnOutcome::Question {
                key: "model".to_owned(),
                question: text,
            };
        }
        // A value said in words binds through its typed reading (`answer.rs`).
        self.bind_answer(round, line)
    }

    /// The round compiled again after an answer or a restatement: a
    /// provider seat climbs the same ladder as the request itself (the
    /// stronger model before « cannot express » — the product law: quality
    /// first); the deterministic seat settles what it reads.
    pub(super) fn compile_again(&mut self, round: AuthoringRound) -> TurnOutcome {
        if self.money_blocks_cognition() {
            let (context, contextual) = (self.project_context(), round.effective_intent());
            return match compile_in(&DETERMINISTIC, &context, &round.request(), &contextual) {
                Ok(out) => self.settle(round, Reading::of(out)),
                Err(e) => self.machinery(&e),
            };
        }
        // A revision's answer replays its EDIT on this seat: no stronger model reads it again.
        if self.seat.has_model() && round.edit.is_none() {
            return self.compile_under_seat(round);
        }
        self.authoring_context = self.project_context();
        match self.compile_round(&round, &self.seat.clone()) {
            Ok(out) => {
                let reading = Reading::of(out);
                self.settle(round, reading)
            }
            Err(e) => self.machinery(&e),
        }
    }

    /// Open language at a question, routed: a question about the question
    /// explains it (`Err`, the question still waits), a change or new work
    /// reads the request again with the words (`Err`), a run is refused
    /// (`Err`); an ANSWER binds (`Ok`) — the route's, or the question's
    /// declared protocol when nothing could judge the line — and UNKNOWN
    /// binds nothing (`Err`, the question still waits).
    fn route_at_question(
        &mut self,
        round: AuthoringRound,
        line: &str,
    ) -> Result<AuthoringRound, TurnOutcome> {
        // A line that is the question's answer by its shape alone cannot change the request,
        // ask about the question, run or cancel: it binds, and no route reads it.
        if super::answer::answers_alone(&round, line) {
            let answer = TurnDecision::new(TurnAct::Answer, RoutingMethod::Protocol);
            let record = RouteRecord::new(SessionPhase::QuestionPending, line, &answer);
            self.routes.push(record);
            return Ok(round);
        }
        // Open language at a question: its act is a bounded decision — an
        // answer binds, a question about the question explains it (the
        // question still waits), a change reads the request again with the
        // human's words; UNKNOWN binds nothing.
        let decision = self.classify(SessionPhase::QuestionPending, line);
        let act = match (decision.act, decision.method) {
            // A `?` is a hint, never a veto: it decides only when nothing could
            // judge the line — an unread question is then asked, not bound.
            (TurnAct::Unknown, RoutingMethod::Fallback) if line.trim_end().ends_with('?') => {
                TurnAct::Discuss
            }
            // Nothing could judge the line: it answers only by the question's
            // declared protocol, and the route records that it did.
            (TurnAct::Unknown, RoutingMethod::Fallback)
                if self.answers_by_protocol(&round, line) =>
            {
                let answer = TurnDecision::new(TurnAct::Answer, RoutingMethod::Protocol);
                let record = RouteRecord::new(SessionPhase::QuestionPending, line, &answer);
                self.routes.push(record);
                TurnAct::Answer
            }
            (act, _) => act,
        };
        match act {
            TurnAct::Cancel => {
                let asked = self.question_id_of(&round);
                self.questions.close(asked);
                self.intent.unresolved.clear();
                self.remember(line, "(authoring discarded)");
                return Err(TurnOutcome::Facts(
                    "authoring discarded · nothing was written · describe the work again when ready"
                        .to_owned(),
                ));
            }
            TurnAct::Discuss => {
                let text = round.current().map_or_else(
                    || "no authoring question waits".to_owned(),
                    |q| super::aside::explain_question(q, &round),
                );
                self.authoring = Some(round);
                return Err(TurnOutcome::Aside(text));
            }
            // A revision's question is answered in words, never read again as a fresh request.
            TurnAct::Modify | TurnAct::Mixed | TurnAct::NewWork if round.edit.is_some() => {}
            TurnAct::Modify | TurnAct::Mixed | TurnAct::NewWork => {
                return Err(self.restate_round(&round, line));
            }
            TurnAct::RequestRun => {
                self.authoring = Some(round);
                return Err(TurnOutcome::Aside(
                    "answer the question or `cancel` first — a run comes once the workflow exists"
                        .to_owned(),
                ));
            }
            // ANSWER — the route's, or the declared protocol's — binds through
            // the typed reading of the question asked.
            TurnAct::Answer => {}
            // UNKNOWN — a line the route could not read, a route that failed,
            // a line no protocol answers — binds nothing: the question waits,
            // unchanged, and says how to go on.
            TurnAct::Unknown => {
                // Nothing reads replies: a value question's sentence waits for the value alone.
                let why = if decision.method == RoutingMethod::Fallback && !self.reads_answers() {
                    "that line is not a value on its own and no intelligence reads replies — nothing was bound · put a longer value in quotes".to_owned()
                } else {
                    Self::unknown_route_text(SessionPhase::QuestionPending, decision.method)
                };
                return Err(self.answer_waits(round, &why));
            }
        }
        Ok(round)
    }

    /// Whether this turn reaches only Run validation, without Session cognition.
    pub(super) fn local_run_line(&self, input: &str) -> bool {
        !self.waiting_cost_choice()
            && self.authoring.is_none()
            && self.run_inputs.is_none()
            && self.activation.is_none()
            && run_prefix(input).is_some_and(|_| {
                ceiling_in(input).is_err()
                    || run_options::parse(input)
                        .is_none_or(|(line, _)| run_line_is_plain(&line.to_lowercase()))
            })
    }

    /// An explicit run line — `run it` · `run brief.nika with a ceiling of
    /// 0.05` · `test it now` — runs a workflow the human named or the one
    /// last accepted, only when its check on disk is clean. `None` when
    /// the line is not a run line.
    pub(super) fn run_turn(&mut self, input: &str) -> Option<TurnOutcome> {
        run_prefix(input)?;
        let Some((line, access_pin)) = run_options::parse(input) else {
            return Some(self.refuse_run_money(input, "invalid or duplicate Run option — use --access <pin> and --max-cost-usd <amount> once each"));
        };
        let lower = line.to_lowercase();
        // A label from the conversational router cannot turn an invalid
        // amount (or an unqualified time/count) into permission to run.
        let ceiling = match ceiling_in(input) {
            Ok(ceiling) => ceiling,
            Err(reason) => return Some(self.refuse_run_money(input, reason)),
        };
        if self.pending_gate.is_some() || self.money.gate.is_some() {
            return Some(self.refuse_run_money(
                input,
                "a paused gate waits; answer it before requesting another Run",
            ));
        }
        if self.money.reconfirm && ceiling.is_none() {
            return Some(self.refuse_run_money(input, &self.restored_refusal()));
        }
        // The run grammar is closed: the verb, the workflow named or « it »,
        // a ceiling. A line that carries more (« run it, but only on
        // Fridays ») is not a run: its act is a bounded decision, and a
        // change comes before any run.
        if !run_line_is_plain(&lower) {
            return match self.classify(SessionPhase::Idle, input).act {
                TurnAct::RequestRun => Some(self.run_plain(&line, ceiling, access_pin)),
                TurnAct::Modify | TurnAct::Mixed => Some(TurnOutcome::Refusal(Refusal::new(
                    RefusalClass::WrongState,
                    "a run with a change in it — say the change first (in a sentence), review the new workflow, then « run it »",
                ))),
                _ => None,
            };
        }
        Some(self.run_plain(&line, ceiling, access_pin))
    }

    /// The closed run line: the verb, the file or the last accepted
    /// workflow, the ceiling.
    fn run_plain(
        &mut self,
        input: &str,
        ceiling: Option<f64>,
        access_pin: Option<String>,
    ) -> TurnOutcome {
        let root = self.snapshot.root.clone();
        let named = named_files(input)
            .into_iter()
            .map(PathBuf::from)
            .find(|p| root.join(p).is_file());
        let Some(workflow) = named.or_else(|| self.last_workflow.clone()) else {
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::WrongState,
                "nothing to run — name a workflow file (« run brief.nika »), or describe the work and Nika builds one first",
            ));
        };
        // A rehearsed copy runs only over the bytes and the world it was rehearsed on.
        if let Some(withdrawn) = self.rehearsed_at_run(&workflow) {
            return withdrawn;
        }
        let audit = check_with_access(&root, &workflow, access_pin.as_deref());
        if !audit.clean {
            let mut text = format!(
                "check · `{}` · findings ✖ — the run was not started",
                workflow.display()
            );
            for f in &audit.findings {
                text.push_str("\n  · ");
                text.push_str(f);
            }
            return TurnOutcome::Facts(text);
        }
        let max_cost_usd = match self.run_money(input, &workflow, ceiling) {
            Ok(amount) => amount,
            Err(refusal) => return refusal,
        };
        self.last_workflow = Some(workflow.clone());
        // The workflow's own declared inputs: a required one with no
        // default is asked, in the product, before the run is requested —
        // the engine would refuse the launch (NIKA-1708) otherwise.
        let given = inline_vars(input);
        let needed: Vec<String> = required_inputs_of(&root, &workflow)
            .into_iter()
            .filter(|name| !given.iter().any(|v| v.starts_with(&format!("{name}="))))
            .collect();
        let inputs = RunInputs {
            workflow: workflow.clone(),
            max_cost_usd,
            access_pin,
            needed,
            given,
        };
        self.remember(input, "(run requested)");
        self.request_or_ask(inputs)
    }

    /// The run request when every declared input is bound; the next
    /// input's question otherwise (the next line answers it).
    fn request_or_ask(&mut self, mut inputs: RunInputs) -> TurnOutcome {
        if let Some(name) = inputs.needed.first().cloned() {
            let question = input_question(&inputs.workflow, &name, inputs.needed.len());
            self.intent.unresolved =
                vec![format!("input `{name}` of `{}`", inputs.workflow.display())];
            self.run_inputs = Some(inputs);
            return TurnOutcome::Question {
                key: format!("input.{name}"),
                question,
            };
        }
        inputs.needed.clear();
        let mut report = if inputs.given.is_empty() {
            format!("check · `{}` · clean ✔", inputs.workflow.display())
        } else {
            format!(
                "check · `{}` · clean ✔ · inputs {}",
                inputs.workflow.display(),
                inputs.given.join(" · ")
            )
        };
        if let Some(pin) = &inputs.access_pin {
            let _ = write!(report, " · access {pin} (explicit)");
        }
        TurnOutcome::RunRequested {
            report,
            run: RunRequest {
                workflow: inputs.workflow,
                vars: inputs.given,
                max_cost_usd: inputs.max_cost_usd,
                access_pin: inputs.access_pin,
            },
        }
    }

    /// The declared input the next line binds, when a run waits on one.
    #[must_use]
    pub fn pending_input(&self) -> Option<&str> {
        self.run_inputs
            .as_ref()
            .and_then(|r| r.needed.first())
            .map(String::as_str)
    }

    /// The human's line as the value of the input the run waits on; a
    /// cancel word drops the run request; an empty line is not a value.
    pub(super) fn answer_input_unrecorded(&mut self, line: &str) -> TurnOutcome {
        let Some(mut inputs) = self.run_inputs.take() else {
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::WrongState,
                "no run waits on an input",
            ));
        };
        // Its « why? » is the turn's (`read_only_turn`): it never reaches here.
        if is_cancel(line) {
            self.intent.unresolved.clear();
            self.remember(line, "(run request discarded)");
            return TurnOutcome::Facts(
                "run discarded · nothing ran · say « run it » again when the inputs are ready"
                    .to_owned(),
            );
        }
        // A command-shaped line is never an input's value.
        if let Some(text) = super::protocol::unserved_command(line) {
            self.run_inputs = Some(inputs);
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::WrongState,
                format!(
                    "{text}\n  the input still waits · reply on the next line · `cancel` drops the run"
                ),
            ));
        }
        let value = line.trim();
        if value.is_empty() {
            self.run_inputs = Some(inputs);
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::EmptyAnswer,
                "the input needs a value — nothing answers for you (`cancel` drops the run)",
            ));
        }
        // A value in quotes is its content: the escape for a word the protocol would take.
        let value = serde_json::from_str::<String>(value).unwrap_or_else(|_| value.to_owned());
        let name = inputs.needed.remove(0);
        inputs.given.push(format!("{name}={value}"));
        self.intent.unresolved.clear();
        self.remember(line, &format!("(input {name} bound)"));
        self.request_or_ask(inputs)
    }
}

/// A run request waiting for the values of the workflow's own declared
/// inputs (required, no default): the next lines bind them, in order.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct RunInputs {
    workflow: PathBuf,
    max_cost_usd: f64,
    access_pin: Option<String>,
    needed: Vec<String>,
    given: Vec<String>,
}

impl RunInputs {
    /// The input the next line binds, when one is still needed.
    pub(super) fn first_needed(&self) -> Option<&str> {
        self.needed.first().map(String::as_str)
    }

    /// The workflow the run waits to start.
    pub(super) fn workflow(&self) -> &std::path::Path {
        &self.workflow
    }

    /// How many inputs still wait, this one included.
    pub(super) fn remaining(&self) -> usize {
        self.needed.len()
    }
}

/// The declared inputs the run must bind — from the engine's parser over
/// the bytes on disk, the same list `nika check` warns about.
fn required_inputs_of(root: &std::path::Path, workflow: &std::path::Path) -> Vec<String> {
    let Ok(source) = std::fs::read_to_string(root.join(workflow)) else {
        return Vec::new();
    };
    let Ok(wf) = nika_schema::parse(
        &source,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    ) else {
        return Vec::new();
    };
    nika_cli_host::display::check_render::required_inputs(&wf)
        .into_iter()
        .map(str::to_owned)
        .collect()
}

pub(super) use nika_onboard::compile::reading::{question_text, revision_way, syntax_incomplete};

use nika_onboard::compile::reading::held_words;

pub(super) fn cannot_express_text(out: &CompileOutcome) -> String {
    let authoring_failed = matches!(
        out.provenance.cognition,
        nika_onboard::compile::AuthoringCognition::ExplicitProvider
    );
    nika_cli_host::display::front_door::recovery::cannot_express(
        authoring_failed,
        &human_reasons(reasons(out)),
    )
}

/// Is `line` the seat the human already chose (`<provider>/<model>`,
/// spacing aside)? The seat is never refused at the model question.
pub(super) fn is_own_seat(seat: &AuthoringSeat, line: &str) -> bool {
    matches!(seat, AuthoringSeat::Provider { model } if model.trim() == line.trim())
}

/// The words when a run model names a CLOUD model the catalog does not
/// price: a run under a spending ceiling would refuse it (NIKA-1709), so
/// the question says so now and names the priced models of that provider.
/// `None` when the model is priced, when the provider is a local engine
/// (unpriced by nature, never refused), or when the line is not
/// `provider/model` (the compiler judges it).
pub(super) fn unpriced_model_text(answer: &str) -> Option<String> {
    let (row, model, priced) = unpriced_cloud(answer)?;
    let way = if priced.is_empty() {
        "name a priced <provider>/<model>, or `cancel`"
    } else {
        "name one of them (the question still waits)"
    };
    Some(format!(
        "{} (NIKA-1709 · unpriced cloud spend cannot be bounded).\n  {} — {way}",
        unpriced_warning(&format!("{row}/{model}")),
        priced_words(&row, &priced)
    ))
}

/// The seat's offer under the model question: Enter takes it, or another
/// `<provider>/<model>`. A seat the catalog does not price says so HERE,
/// before Enter, with the priced models of its provider — the seat is
/// never refused (the human chose it on the first screen), and a run under
/// a spending ceiling would refuse it later (NIKA-1709) at the run door,
/// where the human used to hear it for the first time. `None` without a
/// seat.
pub(super) fn seat_offer(seat: &AuthoringSeat) -> Option<String> {
    let AuthoringSeat::Provider { model } = seat else {
        return None;
    };
    let mut offer = format!("Enter takes your seat `{model}` · or name another <provider>/<model>");
    if let Some((row, _, priced)) = unpriced_cloud(model) {
        let _ = write!(
            offer,
            "\n  {} (NIKA-1709) · {}",
            unpriced_warning(model),
            priced_words(&row, &priced)
        );
    }
    Some(offer)
}

impl SessionRuntime {
    /// The authoring note: the model when the seat names one.
    fn authoring_note(&self) -> String {
        match &self.seat {
            AuthoringSeat::Provider { model } => format!("authoring · {model}"),
            AuthoringSeat::Harness { .. } | AuthoringSeat::Unavailable { .. } => self.seat.line(),
            AuthoringSeat::Deterministic { .. } => "authoring".to_owned(),
        }
    }
}

/// The saved file a proposal updates and the witness it was proposed over; `None` for a
/// proposal that creates its file (a fresh creation keeps its own fresh destination).
pub(super) fn updated_target(set: &ProjectChangeSet) -> Option<(PathBuf, Witness)> {
    set.changes.iter().find_map(|change| match change {
        ProjectChange::UpdateWorkflow { path, before, .. } => Some((path.clone(), before.clone())),
        _ => None,
    })
}
