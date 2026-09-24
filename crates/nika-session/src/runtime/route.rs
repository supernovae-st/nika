// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The route of one open line: the typed state and the raw line go to the
//! bounded classifier — a door's (a decision seat, a scripted one), else
//! the session's own intelligence answering one label, else the
//! conservative fallback (UNKNOWN). The decision is recorded; it never
//! grants a consent, never rewrites the line.

use crate::turn::{
    RouteRecord, RoutingMethod, SessionPhase, TurnAct, TurnClassifier, TurnContext, TurnDecision,
};

use super::SessionRuntime;

impl SessionRuntime {
    /// Inject the bounded classifier a door holds (a decision seat, the
    /// tests' scripted one). Without one the session's intelligence routes.
    pub fn with_classifier(&mut self, classifier: Box<dyn TurnClassifier>) {
        self.classifier = Some(classifier);
    }

    /// The routes decided so far (`/details` · the proof).
    #[must_use]
    pub fn routes(&self) -> &[RouteRecord] {
        &self.routes
    }

    /// The phase the next line arrives in.
    #[must_use]
    pub fn phase(&self) -> SessionPhase {
        if self.pending_gate.is_some() {
            SessionPhase::GatePending
        } else if self.pending.is_some() {
            SessionPhase::ProposalPending
        } else if self.authoring.is_some() || self.run_inputs.is_some() || self.activation.is_some()
        {
            SessionPhase::QuestionPending
        } else {
            SessionPhase::Idle
        }
    }

    /// The context the classifier sees: the phase, the current automation
    /// in one line (the request it came from), the last thing Nika asked.
    fn turn_context(&self, phase: SessionPhase) -> TurnContext {
        let automation = self
            .pending
            .as_ref()
            .map(|set| set.goal.clone())
            .or_else(|| self.intent.goal.clone());
        let last_prompt = match phase {
            SessionPhase::QuestionPending => self
                .pending_question()
                .map(|q| q.label.clone())
                .or_else(|| self.pending_input().map(|n| format!("the value of `{n}`"))),
            SessionPhase::GatePending => self.pending_gate.as_ref().map(|g| g.message.clone()),
            SessionPhase::ProposalPending => Some("the proposal, waiting for yes or no".to_owned()),
            SessionPhase::Idle => None,
        };
        TurnContext {
            phase,
            automation,
            last_prompt,
        }
    }

    /// Route one open line in `phase`: the door's classifier, else the
    /// session's intelligence (one bounded label, never the guard's
    /// business), else UNKNOWN. Recorded.
    pub(super) fn classify(&mut self, phase: SessionPhase, raw: &str) -> TurnDecision {
        let context = self.turn_context(phase);
        let decision = if self.money_blocks_cognition() {
            TurnDecision::new(TurnAct::Unknown, RoutingMethod::Fallback)
        } else if let Some(classifier) = self.classifier.as_mut() {
            match &self.money.account {
                Some(a) => classifier.classify_with_admission(&context, raw, a),
                None => classifier.classify(&context, raw),
            }
        } else if self.intelligence.ready && self.chosen {
            // The chosen intelligence routes, through the same factory the
            // conversation's reasoner came from (a fresh one: the route
            // never consumes the conversation's own turn).
            self.activity(&crate::activity::Activity::now(
                crate::activity::Phase::Understanding,
                "reading your line",
            ));
            match self.factory.as_ref() {
                Some(factory) => {
                    let mut classifier =
                        crate::turn::ReasonerClassifier::new(factory(&self.intelligence));
                    match &self.money.account {
                        Some(a) => classifier.classify_with_admission(&context, raw, a),
                        None => classifier.classify(&context, raw),
                    }
                }
                None => TurnDecision::new(TurnAct::Unknown, RoutingMethod::Fallback),
            }
        } else {
            TurnDecision::new(TurnAct::Unknown, RoutingMethod::Fallback)
        };
        self.routes.push(RouteRecord::new(phase, raw, &decision));
        decision
    }

    /// What an UNKNOWN route says when no intelligence could judge the
    /// line: the automation is kept, the words are not lost, the protocol
    /// forms are named.
    pub(super) fn unknown_route_text(phase: SessionPhase, method: RoutingMethod) -> String {
        // What the line is NOT is known by construction (no protocol token
        // matched); what it means is what no one could read.
        let (not, forms) = match phase {
            SessionPhase::ProposalPending => (
                "that line is not a consent",
                "`yes` applies the proposal · `no` discards it · say the change you want in one sentence · ask about it",
            ),
            SessionPhase::QuestionPending => (
                "that line is not an answer I can bind",
                "answer the question · `why` explains it · `cancel` drops it",
            ),
            SessionPhase::GatePending => (
                "that line is not a gate answer",
                "`yes` or `no` answers the gate · `why` explains it",
            ),
            SessionPhase::Idle => (
                "that line is not work I can read",
                "describe the work to build · ask a question · `/help`",
            ),
        };
        let reason = match method {
            RoutingMethod::Fallback => {
                "no intelligence is available to read what it means (`/intelligence` chooses one)"
            }
            RoutingMethod::Failed => {
                "the intelligence could not read it (its answer failed or came back blank) — say it again, or in other words"
            }
            _ => "I could not tell what it means",
        };
        format!("{not}, and {reason} — nothing changed · {forms}")
    }
}

impl SessionRuntime {
    /// An open line at the consent prompt, routed: a change (or a mixed
    /// line — a yes in it is never a consent) revises the proposal with
    /// the human's own words; a question is answered with the proposal
    /// held; a run, new work, an answer or an unknown line hold the
    /// proposal and name the protocol forms. Nothing here applies.
    pub(super) fn route_at_consent(
        &mut self,
        set: crate::change::ProjectChangeSet,
        id: crate::outcome::ProposalId,
        raw: &str,
    ) -> super::TurnOutcome {
        // An engine fact answers first, deterministically and for zero
        // tokens (« what workflows are here? »): a closed set of engine
        // questions, not a reading of language.
        if let Some(fact) = crate::facts::answer(raw, &self.snapshot, &self.snapshot.root) {
            return self.hold_pending(set, id, &fact);
        }
        let decision = self.classify(SessionPhase::ProposalPending, raw);
        match decision.act {
            TurnAct::Cancel => {
                self.decided = Some(id);
                super::TurnOutcome::Facts(
                    "discarded · nothing was written · ask again for the change when ready".to_owned(),
                )
            }
            TurnAct::Modify | TurnAct::Mixed if self.activation_proposal.as_ref() == Some(&id) => {
                self.activation_proposal = None;
                self.decided = Some(id);
                super::TurnOutcome::Facts(
                    "discarded the old schedule declaration · nothing was written · restate the work with the revised cadence, save it, then activate again for a fresh review".to_owned(),
                )
            }
            TurnAct::Modify | TurnAct::Mixed => self.revise_pending(set, raw),
            TurnAct::Discuss => self.discuss_pending(set, id, raw),
            TurnAct::RequestRun => self.hold_pending(
                set,
                id,
                "consent is never a run — `yes` applies the proposal first, then « run it »",
            ),
            TurnAct::NewWork => self.hold_pending(
                set,
                id,
                "a new automation while this one waits — `yes` or `no` first, then describe it",
            ),
            TurnAct::Answer => self.hold_pending(
                set,
                id,
                "no question is open — `yes` applies the proposal, `no` discards it, or say the change you want",
            ),
            // UNKNOWN: the set's own effects answer what most lines at this
            // prompt ask (what it reads and writes), then the protocol forms.
            _ => {
                let text = format!(
                    "{}\n{}",
                    set.effects_fact(),
                    Self::unknown_route_text(SessionPhase::ProposalPending, decision.method)
                );
                self.hold_pending(set, id, &text)
            }
        }
    }

    /// The proposal held with a line beside it.
    fn hold_pending(
        &mut self,
        set: crate::change::ProjectChangeSet,
        id: crate::outcome::ProposalId,
        text: &str,
    ) -> super::TurnOutcome {
        self.pending = Some(set);
        super::TurnOutcome::Held {
            id,
            preview: format!(
                "{text}\n(the proposal still waits · `yes` applies it · `no` discards it)"
            ),
        }
    }

    /// A question about the proposal: the engine's facts first (zero
    /// tokens), then the intelligence with the proposal's exact bytes
    /// beside the question, then the set's own effects line.
    fn discuss_pending(
        &mut self,
        set: crate::change::ProjectChangeSet,
        id: crate::outcome::ProposalId,
        raw: &str,
    ) -> super::TurnOutcome {
        let text = crate::facts::answer(raw, &self.snapshot, &self.snapshot.root)
            .or_else(|| self.reason_about(&set.preview(), raw))
            .unwrap_or_else(|| set.effects_fact());
        self.hold_pending(set, id, &text)
    }

    /// The conversational intelligence over the broker's bundle with a
    /// document (the proposal's bytes) beside the human's line — words
    /// only, read by the guard; `None` when no intelligence answers.
    fn reason_about(&mut self, document: &str, raw: &str) -> Option<String> {
        if self.money_blocks_cognition() {
            return None;
        }
        if !(self.intelligence.ready && self.chosen && self.reasoner.name() != "none") {
            return None;
        }
        let bundle = self.broker.bundle(
            &self.snapshot,
            self.intent.goal.as_deref(),
            &[],
            &self.intelligence.locus.line(),
        );
        let turn = format!(
            "{raw}\n\n(The proposal under review, exact bytes — answer about it, change nothing:)\n```yaml\n{document}\n```"
        );
        let prompt = crate::broker::ContextBroker::prompt(&bundle, &self.recent, &turn);
        let reply = self.reason_with_money(&prompt, false).ok()?;
        let findings = self.known.audit(&reply.text);
        let shown = crate::guard::KnownWorld::correct(&reply.text, &findings);
        self.remember(raw, &shown);
        Some(shown)
    }
}
