// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The authoring turns: a free-text intent reaches the ONE compiler; its
//! typed question becomes the next line's meaning; a Ready candidate
//! becomes the proposal the consent line answers; an explicit `run …`
//! line runs an accepted workflow through the door. Each pending state
//! gives the next line exactly one typed meaning — a `yes` never crosses
//! from an authoring answer to a consent to a gate.

use std::fmt::Write as _;
use std::path::PathBuf;

use nika_onboard::compile::{CompileOutcome, CompileQuestion, CompileRequest, revise_intent};

use super::{SessionRuntime, TurnOutcome, ceiling_in, named_files};
use crate::activity::{Activity, Phase};
use crate::authoring::{
    AuthoringContext, AuthoringError, AuthoringRound, AuthoringSeat, Reading,
    compile_deterministic, compile_in, is_cancel, is_greeting, is_why, reasons,
};
use crate::change::{RunRequest, check_on_disk};
use crate::outcome::{ProposalId, Refusal, RefusalClass};
use crate::review;
use crate::turn::{RoutingMethod, SessionPhase, TurnAct};

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
        &self,
        request: &CompileRequest,
        intent: &str,
    ) -> Result<CompileOutcome, AuthoringError> {
        if self.money_blocks_cognition() {
            return compile_deterministic(request);
        }
        // The session's own project root, never the process's working directory.
        let context = self
            .authoring_context
            .clone()
            .with_project_root(self.snapshot.root.clone());
        self.seated(&self.seat, |account| match account {
            Some(a) => crate::authoring::compile_in_with_admission(
                &self.seat, &context, request, intent, a,
            ),
            None => compile_in(&self.seat, &context, request, intent),
        })
    }

    /// Every seated round observes the files its request names under the session's own
    /// project root (never the process's working directory, never through a link outside it).
    fn observe_project(&mut self) {
        let root = self.snapshot.root.clone();
        self.authoring_context.set_project_root(&root);
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
        let round = AuthoringRound::new(intent);
        let out = match compile_deterministic(&round.request()) {
            Ok(out) => out,
            Err(e) => return Some(self.machinery(&e)),
        };
        // What the reading understood, from the compiler's own ledger —
        // never a count invented from the prose.
        if let Some(n) = clauses_understood(&out) {
            self.activity(&Activity::done(
                Phase::Understanding,
                format!(
                    "understood {n} requirement{}",
                    if n == 1 { "" } else { "s" }
                ),
            ));
        }
        let reading = Reading::of(out);
        // Only work owns the automation goal. Keep this round before any
        // seat/admission failure; an earlier conversation is not its request.
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
                if intent.trim().ends_with('?') {
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
                    TurnAct::Modify | TurnAct::Mixed => {
                        let saved = self.last_workflow.clone()?;
                        Some(self.revise_saved(&saved, intent))
                    }
                    _ => None,
                }
            }
            Reading::Unsettled(out) => Some(match &self.seat {
                AuthoringSeat::Provider { .. } | AuthoringSeat::Harness { .. } => {
                    self.compile_under_seat(round)
                }
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
                    let text = honest_incomplete(&out, why.as_deref());
                    self.last_outcome = Some(out);
                    TurnOutcome::Facts(text)
                }
            }),
            reading => Some(self.settle(round, reading)),
        }
    }

    /// The same Compile, under the seat the human permitted, for work the
    /// deterministic policy could not settle. How long it takes and how
    /// hard it thinks is the compiler's; the human only learns that Nika
    /// is working (a truthful line, no invented detail).
    fn compile_under_seat(&mut self, round: AuthoringRound) -> TurnOutcome {
        if self.money_blocks_cognition() {
            return self.cognition_money_refusal();
        }
        self.observe_project();
        self.activity(&Activity::now(Phase::Authoring, self.authoring_note()));
        match self.compile_round(&round, &self.seat) {
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
        let text = cannot_express_text(&out);
        self.last_outcome = Some(out);
        TurnOutcome::Facts(text)
    }

    /// A change, a mixed line or new work said at a question: the request
    /// is read again with the human's own words (the round is dropped, the
    /// plan read a different request). Never a paraphrase.
    fn restate_round(&mut self, round: &AuthoringRound, line: &str) -> TurnOutcome {
        let intent = format!("{}. {}", round.intent, line.trim());
        self.remember(line, "(the request read again with these words)");
        let again = AuthoringRound::new(intent);
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
    fn propose_revision(&mut self, goal: &str, out: &CompileOutcome) -> TurnOutcome {
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
        match self.propose(goal, out) {
            TurnOutcome::Proposal { id, preview } => {
                // The Meaning and details doors must describe the bytes now
                // awaiting consent. Keep the earlier reading if proposal fails.
                self.last_outcome = Some(out.clone());
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
        let original = self.intent.goal.clone();
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
        let mut request = CompileRequest::edit(base, change.trim());
        if let Some(original) = original {
            request = request.with_original_intent(original);
        }
        let revised = revise_intent(&request).unwrap_or_else(|| goal.clone());
        let out = match self.compile_request(&request, &revised) {
            Ok(out) => out,
            Err(e) => return self.machinery(&e),
        };
        match Reading::of(out) {
            Reading::Ready(out) => {
                self.remember(change, "(revised the saved workflow)");
                self.propose(&goal, &out)
            }
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
        let goal = format!("{} — {}", set.goal, change.trim());
        self.activity(&Activity::now(
            Phase::Authoring,
            "revising with your change",
        ));
        // The compiler sees the exact base, original request and raw change.
        // Its bounded edit/repair loop owns the revision; a model paraphrase
        // must never replace these inputs through a fresh Create request.
        let request =
            CompileRequest::edit(base, change.trim()).with_original_intent(set.goal.clone());
        let revised = revise_intent(&request).unwrap_or_else(|| goal.clone());
        let out = match self.compile_request(&request, &revised) {
            Ok(out) => out,
            Err(e) => {
                self.pending = Some(set);
                return self.machinery(&e);
            }
        };
        match Reading::of(out) {
            Reading::Ready(out) => {
                self.remember(change, "(revised the proposal)");
                self.propose_revision(&goal, &out)
            }
            reading => {
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

    /// The compiler's machinery failed under a seat, or the compiler
    /// itself: a seat failure is a recovery (the goal is kept, the ways on
    /// are named); a compiler failure is a refusal that names it; an
    /// authoring configuration that cannot be honored is refused before
    /// anything is sent — never authored without the knowledge it names.
    fn machinery(&mut self, error: &AuthoringError) -> TurnOutcome {
        match error {
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
    fn settle(&mut self, mut round: AuthoringRound, reading: Reading) -> TurnOutcome {
        // The compiler's reading is what `/meaning` shows, clause by clause.
        self.last_outcome = Some(reading.outcome().clone());
        match reading {
            Reading::Ready(out) => self.propose(&round.intent, &out),
            Reading::Questions(out) => {
                round.absorb(&out);
                let Some(question) = round.current() else {
                    return TurnOutcome::Facts(honest_incomplete(&out, None));
                };
                let key = question.key.clone();
                // A rule the compiler can only ask as code is never asked
                // as code: once, the clause is asked in words (the human's
                // words replace it in the request); a second time, or a
                // clause the request does not carry verbatim, is an honest
                // incomplete that names the way on.
                if asks_for_syntax(question) {
                    let clause = clause_of(&question.label);
                    let carried = clause
                        .as_deref()
                        .is_some_and(|c| round.intent.contains(c));
                    if round.restatements > 0 || !carried {
                        self.intent.unresolved.clear();
                        let text = syntax_incomplete(clause.as_deref());
                        self.remember(&round.intent, &text);
                        return TurnOutcome::Facts(text);
                    }
                    let text = syntax_question_text(clause.as_deref().unwrap_or_default());
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
                TurnOutcome::Facts(honest_incomplete(&out, None))
            }
            // A turn the session could not finish: the recovery card (what
            // is kept · what did not happen · the ways on), never a bare
            // « failed ». The round is not kept: the human says it again.
            Reading::BudgetExhausted(_) => self.recovery(
                None,
                "I couldn't finish a workflow I trust within the authoring budget",
                "the budget ran out before a candidate I could stand behind; narrowing the request helps",
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
        }
    }

    /// The Ready candidate as the proposal the consent line answers:
    /// exact bytes, a fresh destination, the same check facade.
    fn propose(&mut self, goal: &str, out: &CompileOutcome) -> TurnOutcome {
        match review::propose(&self.snapshot.root, goal, out) {
            Ok(set) => {
                let bytes = self.draft_preview(&set);
                let id = ProposalId::of(&bytes);
                let preview = self.draft_review(&set, out, &bytes);
                self.authoring = None;
                self.intent.unresolved.clear();
                self.remember(goal, &format!("(proposed {id})"));
                // The schedule the request asked for rides beside the set:
                // « activate » declares it once the program is saved.
                self.pending_trigger.clone_from(&out.requested_trigger);
                self.bind_proposal_money(&id);
                self.pending = Some(set);
                TurnOutcome::Proposal { id, preview }
            }
            Err(e) => TurnOutcome::Refusal(Refusal::from_change(&e)),
        }
    }

    /// The human's line as the answer to the open question: typed to its
    /// shape, bound to its key, and the same plan replayed. A cancel word
    /// drops the round; an empty line is not an answer.
    pub(super) fn answer_question_unrecorded(&mut self, line: &str) -> TurnOutcome {
        let Some(round) = self.authoring.take() else {
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::WrongState,
                "no authoring question waits",
            ));
        };
        // « why? » beside the question: what the value is for, from the
        // compiler's own words; the question keeps waiting.
        if is_why(line) {
            let text = round.current().map_or_else(
                || "no authoring question waits".to_owned(),
                |q| super::aside::explain_question(q, &round),
            );
            self.authoring = Some(round);
            return TurnOutcome::Aside(text);
        }
        if is_cancel(line) {
            let asked = self.question_id_of(&round);
            self.questions.close(asked);
            self.intent.unresolved.clear();
            self.remember(line, "(authoring discarded)");
            return TurnOutcome::Facts(
                "authoring discarded · nothing was written · describe the work again when ready"
                    .to_owned(),
            );
        }
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
        let round = match self.route_at_question(round, line) {
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
            let intent = round.intent.replacen(&clause, line.trim(), 1);
            let mut restated = AuthoringRound::new(intent);
            restated.restatements = round.restatements.saturating_add(1);
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
            return match compile_deterministic(&round.request()) {
                Ok(out) => self.settle(round, Reading::of(out)),
                Err(e) => self.machinery(&e),
            };
        }
        if self.seat.has_model() {
            return self.compile_under_seat(round);
        }
        self.observe_project();
        match self.compile_round(&round, &self.seat) {
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
    /// (`Err`); an answer, or a line nothing could read, binds (`Ok`).
    fn route_at_question(
        &mut self,
        round: AuthoringRound,
        line: &str,
    ) -> Result<AuthoringRound, TurnOutcome> {
        // Open language at a question: its act is a bounded decision — an
        // answer binds, a question about the question explains it (the
        // question still waits), a change reads the request again with the
        // human's words; without any intelligence a line is the answer.
        let decision = self.classify(SessionPhase::QuestionPending, line);
        // A `?` is a hint, never a veto: it decides only when nothing could
        // judge the line — an unread question is then asked, not bound.
        let act = if decision.act == TurnAct::Unknown
            && decision.method == RoutingMethod::Fallback
            && line.trim_end().ends_with('?')
        {
            TurnAct::Discuss
        } else {
            decision.act
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
            // ANSWER, or a line the route could not read: the answer to the
            // question asked (a short line at a question is the answer).
            TurnAct::Answer | TurnAct::Unknown => {}
        }
        Ok(round)
    }

    /// An explicit run line — `run it` · `run brief.nika with a ceiling of
    /// 0.05` · `test it now` — runs a workflow the human named or the one
    /// last accepted, only when its check on disk is clean. `None` when
    /// the line is not a run line.
    pub(super) fn run_turn(&mut self, input: &str) -> Option<TurnOutcome> {
        let lower = input.trim().to_lowercase();
        let first = lower
            .split(|c: char| c.is_whitespace() || c == ',' || c == ':')
            .next()?;
        if !is_run_verb(first) {
            return None;
        }
        // A label from the conversational router cannot turn an invalid
        // amount (or an unqualified time/count) into permission to run.
        let ceiling = match ceiling_in(input) {
            Ok(ceiling) => ceiling,
            Err(reason) => {
                return Some(self.refuse_money(input, reason));
            }
        };
        // The run grammar is closed: the verb, the workflow named or « it »,
        // a ceiling. A line that carries more (« run it, but only on
        // Fridays ») is not a run: its act is a bounded decision, and a
        // change comes before any run.
        if !run_line_is_plain(&lower) {
            let workflow = named_files(input)
                .into_iter()
                .map(PathBuf::from)
                .find(|p| self.snapshot.root.join(p).is_file())
                .or_else(|| self.last_workflow.clone());
            if let Some(workflow) = workflow
                && let Err(refusal) = self.run_money(input, &workflow, ceiling)
            {
                return Some(refusal);
            }
            return match self.classify(SessionPhase::Idle, input).act {
                TurnAct::RequestRun => Some(self.run_plain(input, ceiling)),
                TurnAct::Modify | TurnAct::Mixed => Some(TurnOutcome::Refusal(Refusal::new(
                    RefusalClass::WrongState,
                    "a run with a change in it — say the change first (in a sentence), review the new workflow, then « run it »",
                ))),
                _ => None,
            };
        }
        Some(self.run_plain(input, ceiling))
    }

    /// The closed run line: the verb, the file or the last accepted
    /// workflow, the ceiling.
    fn run_plain(&mut self, input: &str, ceiling: Option<f64>) -> TurnOutcome {
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
        let audit = check_on_disk(&root, &workflow);
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
        let report = if inputs.given.is_empty() {
            format!("check · `{}` · clean ✔", inputs.workflow.display())
        } else {
            format!(
                "check · `{}` · clean ✔ · inputs {}",
                inputs.workflow.display(),
                inputs.given.join(" · ")
            )
        };
        TurnOutcome::RunRequested {
            report,
            run: RunRequest {
                workflow: inputs.workflow,
                vars: inputs.given,
                max_cost_usd: inputs.max_cost_usd,
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
        // « why? » beside the input: what it is and who declares it; the
        // input keeps waiting.
        if is_why(line) {
            let text = match inputs.first_needed() {
                Some(name) => {
                    super::aside::explain_input(&inputs.workflow, name, inputs.remaining())
                }
                None => "no input waits".to_owned(),
            };
            self.run_inputs = Some(inputs);
            return TurnOutcome::Aside(text);
        }
        if is_cancel(line) {
            self.intent.unresolved.clear();
            self.remember(line, "(run request discarded)");
            return TurnOutcome::Facts(
                "run discarded · nothing ran · say « run it » again when the inputs are ready"
                    .to_owned(),
            );
        }
        let value = line.trim();
        if value.is_empty() {
            self.run_inputs = Some(inputs);
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::EmptyAnswer,
                "the input needs a value — nothing answers for you (`cancel` drops the run)",
            ));
        }
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

/// `name=value` pairs the human wrote on the run line itself.
fn inline_vars(input: &str) -> Vec<String> {
    input
        .split_whitespace()
        .filter(|token| {
            token.split_once('=').is_some_and(|(k, v)| {
                !k.is_empty()
                    && !v.is_empty()
                    && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            })
        })
        .map(|token| token.trim_matches(|c| c == ',' || c == ';').to_owned())
        .collect()
}

/// The question for one declared input, in the product's words.
fn input_question(workflow: &std::path::Path, name: &str, remaining: usize) -> String {
    let more = if remaining > 1 {
        format!(" ({} more after this one)", remaining - 1)
    } else {
        String::new()
    };
    format!(
        "`{}` declares an input it needs before it runs: `{name}`{more}\n  reply on the next line with its value (`input.{name}`) · `cancel` drops the run",
        workflow.display()
    )
}

/// The question as the human reads it: the compiler's label, why it
/// cannot invent the value, what it could not settle, how to abandon.
pub(super) fn question_text(question: &CompileQuestion, reasons: &[String]) -> String {
    let mut text = question.label.clone();
    if !question.why.is_empty() {
        text.push_str("\n  (");
        text.push_str(&question.why);
        text.push(')');
    }
    if !reasons.is_empty() {
        text.push_str("\n  what I could not settle:");
        for reason in reasons {
            text.push_str("\n    · ");
            text.push_str(reason);
        }
    }
    // The raw key stays out of the human's line: « why? » names it, with
    // what the value is for; the prompt that follows (`reply ›`) says whose
    // turn it is.
    text.push_str("\n  reply on the next line · `cancel` drops this · `why?` explains");
    text
}

/// The card when nothing could be built, in the reading's own truth: a
/// seat's draft the compiler's fidelity check refused is an AUTHORING
/// failure (another attempt may hold every part), never a language gap;
/// the deterministic reader's unsupported clause is a gap in what Nika
/// can express. Neither is the human's ambiguity (mandate: a compiler gap
/// is never presented as user ambiguity, nor an authoring failure as a gap).
/// The way on after a revision that could not settle: an authoring failure (a seat tried and
/// failed on Nika's side) keeps the base and the change — the same words try again; a reading
/// the compiler could not settle asks for the change in other words. Never « describe the whole
/// automation again »: the base and the original request are kept.
pub(super) fn revision_way(out: &CompileOutcome) -> &'static str {
    if matches!(
        out.provenance.cognition,
        nika_onboard::compile::AuthoringCognition::ExplicitProvider
    ) {
        "an authoring step failed on Nika's side: your change is kept — send it again unchanged for another attempt, or `/intelligence` for another model"
    } else {
        "say the change another way"
    }
}

pub(super) fn cannot_express_text(out: &CompileOutcome) -> String {
    let authoring_failed = matches!(
        out.provenance.cognition,
        nika_onboard::compile::AuthoringCognition::ExplicitProvider
    );
    let mut text = if authoring_failed {
        "Nika could not finish building this automation — an authoring step failed on Nika's side (below), not because of how you asked; nothing was written.".to_owned()
    } else {
        "Nika cannot express this automation yet — nothing was written.".to_owned()
    };
    let stopped = human_reasons(reasons(out));
    if !stopped.is_empty() {
        text.push_str("\n  what stopped it:");
        for reason in stopped {
            text.push_str("\n    · ");
            text.push_str(&reason);
        }
    }
    // An internal failure never asks the human to rewrite or split what they asked: the
    // request stays the goal, and the same words make another attempt.
    text.push_str(if authoring_failed {
        "\n  your request is kept as the goal: send it again unchanged for another attempt, or `/intelligence` for another model · `/meaning` shows what was understood"
    } else {
        "\n  what helps: say the outcome in one sentence (what to read · what to produce · where it goes), or split the work in two requests · `/meaning` shows what was understood"
    });
    text
}

/// The words when a run model names a CLOUD model the catalog does not
/// price: a run under a spending ceiling would refuse it (NIKA-1709), so
/// the question says so now and names the priced models of that provider.
/// `None` when the model is priced, when the provider is a local engine
/// (unpriced by nature, never refused), or when the line is not
/// `provider/model` (the compiler judges it).
/// Is `line` the seat the human already chose (`<provider>/<model>`,
/// spacing aside)? The seat is never refused at the model question.
pub(super) fn is_own_seat(seat: &AuthoringSeat, line: &str) -> bool {
    matches!(seat, AuthoringSeat::Provider { model } if model.trim() == line.trim())
}

pub(super) fn unpriced_model_text(answer: &str) -> Option<String> {
    let (row, model, priced) = unpriced_cloud(answer)?;
    let mut text = format!(
        "`{row}/{model}` is not priced in Nika's catalog: a run under a spending ceiling would refuse it (NIKA-1709 · unpriced cloud spend cannot be bounded)."
    );
    if priced.is_empty() {
        let _ = write!(
            text,
            "\n  no priced model is known for `{row}` yet — name a priced <provider>/<model>, or `cancel`"
        );
    } else {
        let _ = write!(
            text,
            "\n  priced for `{row}`: {} — name one of them (the question still waits)",
            priced.join(" · ")
        );
    }
    Some(text)
}

/// A CLOUD model the catalog does not price, as (the provider's row id,
/// the model, the priced models of that provider). `None` when the model
/// is priced, when the provider is a local engine (unpriced by nature,
/// never refused), or when the line is not `provider/model`.
fn unpriced_cloud(answer: &str) -> Option<(String, String, Vec<String>)> {
    let (provider, model) = answer.trim().split_once('/')?;
    let row = nika_catalog::all_providers()
        .iter()
        .find(|p| p.id == provider || p.aliases.contains(&provider))?;
    if !row.requires_key || nika_catalog::find_pricing_scoped(row.id, model).is_some() {
        return None;
    }
    let priced = row
        .models
        .iter()
        .filter(|m| nika_catalog::find_pricing_scoped(row.id, m.model).is_some())
        .map(|m| format!("{}/{}", row.id, m.model))
        .collect();
    Some((row.id.to_owned(), model.to_owned(), priced))
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
            "\n  `{model}` is not priced in Nika's catalog: a run under a spending ceiling would refuse it (NIKA-1709)"
        );
        if priced.is_empty() {
            let _ = write!(offer, " · no priced model is known for `{row}` yet");
        } else {
            let _ = write!(offer, " · priced for `{row}`: {}", priced.join(" · "));
        }
    }
    Some(offer)
}

/// How many clauses the compiler's ledger holds for this reading —
/// « understood N requirements » — `None` when the outcome carries no ledger.
fn clauses_understood(out: &CompileOutcome) -> Option<usize> {
    let ledger = out
        .provenance
        .decision
        .as_ref()?
        .get("ledger")?
        .as_array()?;
    (!ledger.is_empty()).then_some(ledger.len())
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

/// A question that asks the human for code (a jq or CEL expression, a
/// `const.*_expression` value): a product defect when it reaches them.
pub(super) fn asks_for_syntax(question: &CompileQuestion) -> bool {
    let label = question.label.to_ascii_lowercase();
    question.key.ends_with("_expression")
        || label.contains("jq expression")
        || label.contains(" jq ")
        || label.contains("cel expression")
}

/// The clause the compiler quotes in its question (between backticks),
/// as the request carries it.
pub(super) fn clause_of(label: &str) -> Option<String> {
    let start = label.find('`')? + 1;
    let end = start + label[start..].find('`')?;
    let clause = label[start..end].trim();
    (!clause.is_empty()).then(|| clause.to_owned())
}

/// The clause asked in words — never a syntax — with what to say and
/// what Nika does with it.
fn syntax_question_text(clause: &str) -> String {
    format!(
        "One thing I need from you, in words: how to do « {clause} ». Say it as you would to a colleague — what to keep, what to compute, over which column (e.g. « the total of the amount column » · « the rows whose status is paid »); your words take the place of « {clause} » in your request and Nika reads it again. No code is needed.\n  reply on the next line · `cancel` drops this · `why?` explains"
    )
}

/// The honest incomplete when the rule stays code after the human's words
/// (or the clause is not in the request as quoted): the way on, no syntax.
fn syntax_incomplete(clause: Option<&str>) -> String {
    let what = clause.map_or("this step".to_owned(), |c| format!("« {c} »"));
    format!(
        "I read this as work but cannot build {what} from your words yet: it would need a rule I can only write as code, and I never ask you for code.\n  · say the step differently — what to keep, what to compute, over which column, and where to write it\n  · or `cancel` and describe the work again\n  nothing was written"
    )
}

/// An incomplete the human can act on: what the reader could not settle,
/// and the next safe step — never a substitute workflow.
fn honest_incomplete(out: &CompileOutcome, why: Option<&str>) -> String {
    let mut text = "I read this as work but cannot build it yet:".to_owned();
    let reasons = human_reasons(reasons(out));
    if reasons.is_empty() {
        text.push_str("\n  · the request names no operation I can read");
    }
    for reason in reasons {
        text.push_str("\n  · ");
        text.push_str(&reason);
    }
    text.push_str("\n  ");
    text.push_str(why.unwrap_or(
        "say what to read, what to produce and where to write it, e.g. « read ./docs, draft a digest and write it to ./digest.md »",
    ));
    text
}

/// The compiler's reasons a human can act on: its machine sentences (the
/// plan's own vocabulary, an unmapped part with nothing after the colon)
/// dropped, duplicates folded, the rest verbatim.
pub(crate) fn human_reasons(reasons: Vec<String>) -> Vec<String> {
    let mut kept: Vec<String> = Vec::new();
    for reason in reasons {
        let r = reason.trim();
        let machine = r.contains("semantic plan") || r.ends_with(": .") || r.ends_with(':');
        if machine || r.is_empty() {
            continue;
        }
        let said = human_reason(r);
        if kept.contains(&said) {
            continue;
        }
        kept.push(said);
    }
    kept
}

/// One compiler reason in the human's words — the compiler's fidelity
/// grammar is a closed set (« Candidate N is not feasible: … », « dropped
/// the recognized operation `x` (evidence) », « the path `p` is no longer
/// carried … », « the literal `v` is not in the request »); any other line
/// is kept as the compiler said it.
fn human_reason(raw: &str) -> String {
    let r = raw.trim().trim_end_matches('.');
    // A cut answer is the seat's output limit, an internal cause: its command-line advice
    // (`--authoring-max-tokens`) is no gesture a Session has, and the request is not at fault.
    if r.contains("--authoring-max-tokens") {
        let tokens: String = r
            .chars()
            .skip_while(|c| !c.is_ascii_digit())
            .take_while(char::is_ascii_digit)
            .collect();
        let limit = if tokens.is_empty() {
            "its output limit".to_owned()
        } else {
            format!("its {tokens}-token output limit")
        };
        return format!(
            "the model's answer was cut at {limit} before it was complete — an internal limit of this attempt, not a problem with your request"
        );
    }
    let r = match r.find("is not feasible: ") {
        Some(at) if r.starts_with("Candidate ") => &r[at + "is not feasible: ".len()..],
        _ => r,
    };
    let quoted = |s: &str| -> Option<(String, String)> {
        let start = s.find('`')?;
        let end = s[start + 1..].find('`')? + start + 1;
        Some((s[start + 1..end].to_owned(), s[end + 1..].to_owned()))
    };
    if let Some(rest) = r.strip_prefix("dropped the recognized operation ")
        && let Some((op, tail)) = quoted(rest)
    {
        let evidence = tail
            .trim()
            .trim_start_matches('(')
            .trim_end_matches(')')
            .trim_end_matches(',')
            .trim();
        return if evidence.is_empty() {
            format!("the draft lost the « {op} » step")
        } else {
            format!("the draft lost « {evidence} » (the {op} step)")
        };
    }
    if let Some(rest) = r.strip_prefix("the path ")
        && let Some((path, tail)) = quoted(rest)
        && tail.contains("no longer carried")
    {
        return format!("the draft dropped « {path} »: nothing reads or writes it any more");
    }
    if let Some(rest) = r.strip_prefix("the literal ")
        && let Some((value, tail)) = quoted(rest)
        && tail.contains("not in the request")
    {
        return format!("the draft invented a value (« {value} ») your request never gave");
    }
    r.to_owned()
}

/// The first word of an explicit run line (EN/FR). The French imperative
/// with its object pronoun — « lance-le », « exécute-la », « relance-le » —
/// is the same verb: it reaches the same run gate (check, money, the fresh
/// Run decision), never a conversation and never a run by itself.
pub(super) fn is_run_verb(first: &str) -> bool {
    let first = first.trim_end_matches(['.', '!']);
    let verb = match first.rsplit_once('-') {
        Some((verb, "le" | "la" | "les" | "moi")) => verb,
        _ => first,
    };
    matches!(
        verb,
        "run" | "execute" | "test" | "lance" | "exécute" | "teste" | "relance" | "run:"
    )
}

/// Whether a run line is the closed grammar and nothing more: the verb,
/// a workflow name, « it », a ceiling phrase, a few fillers. Anything
/// else in the line is a meaning of its own (a change, a condition).
fn run_line_is_plain(lower: &str) -> bool {
    const FILLERS: &[&str] = &[
        "it", "again", "the", "workflow", "once", "now", "this", "that", "le", "la", "ça",
        "encore", "please", "stp", "svp", "with", "a", "ceiling", "of", "cap", "max", "cost",
        "usd", "dollar", "dollars", "budget", "plafond", "de", "un", "une", "avec", "at", "à", "$",
    ];
    lower
        .split(|c: char| c.is_whitespace() || c == ',' || c == ':')
        .skip(1)
        .map(|w| {
            w.trim_matches(|c: char| matches!(c, '.' | ';' | '!' | '(' | ')' | '"' | '\'' | '`'))
        })
        .filter(|w| !w.is_empty())
        .all(|w| {
            FILLERS.contains(&w)
                || std::path::Path::new(w)
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("nika"))
                || w.starts_with("./")
                || w.starts_with("--max-cost-usd")
                || w.contains('=')
                || super::money_parse::parse(w).is_ok_and(|money| money.money_only)
                || w.trim_start_matches('$').parse::<f64>().is_ok()
        })
}
