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

use super::{SessionRuntime, authoring::DETERMINISTIC};
use crate::authoring::{AuthoringContext, AuthoringRound, Reading, compile_in};

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
        } else if self.pending_question().is_some()
            || self.run_inputs.is_some()
            || self.activation.is_some()
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
        let blocked = self.money_blocks_cognition();
        let reads = self.reads_answers();
        let mut door = self.classifier.take();
        let routed = door.is_none();
        let asks = !blocked && (!routed || reads);
        let asked = self.authoring_context.reasoning_asked();
        // The chosen intelligence routes through the same factory the
        // conversation's reasoner came from (a fresh one: the route never
        // consumes the conversation's own turn), so its route is observable.
        // A door's classifier names no route: without money it keeps its path.
        let fresh = (self.factory.as_ref())
            .filter(|_| asks && routed && asked.is_ok())
            .map(|factory| factory(&self.intelligence));
        let model = (fresh.as_ref())
            .filter(|reasoner| reasoner.supports_admission())
            .and_then(|reasoner| reasoner.authoring_model());
        let mut fresh = fresh.map(crate::turn::ReasonerClassifier::new);
        let mut classifier = match door.as_deref_mut() {
            Some(door) => Some(door),
            None => fresh.as_mut().map(|fresh| fresh as &mut dyn TurnClassifier),
        };
        // Every label asks the session's named level through the classifier that sends it, a
        // door's included (B19 F2). A word the session cannot ask, or a level that classifier
        // cannot carry, refuses the label before any record or byte (R4 B16).
        let effort = match (asked, classifier.as_mut()) {
            (Err(why), _) => Err(format!("nothing was sent · {why}")),
            // The classifier's typed refusal, said in its own words (display only).
            (Ok(level), Some(classifier)) => classifier
                .carry_effort(level)
                .map_err(|why| why.to_string()),
            (Ok(_), None) => Ok(()),
        };
        // A paid label request may leave only after the record says it might.
        let entered = if !asks || classifier.is_none() || effort.is_err() {
            Ok((None, false))
        } else {
            self.enter_dispatch(model.as_deref())
        };
        let account = (entered.as_ref().ok()).and_then(|(account, _)| account.clone());
        let decision = if !asks {
            TurnDecision::new(TurnAct::Unknown, RoutingMethod::Fallback)
        } else if let Err(error) = &entered {
            TurnDecision::failed(&format!(
                "the paid-dispatch boundary was not recorded ({error}); nothing was sent"
            ))
        } else {
            if routed {
                self.activity(&crate::activity::Activity::now(
                    crate::activity::Phase::Understanding,
                    "reading your line",
                ));
            }
            match (effort, classifier, &account) {
                (Err(why), ..) => TurnDecision::failed(&why),
                (Ok(()), Some(c), Some(a)) => c.classify_with_admission(&context, raw, a),
                (Ok(()), Some(c), None) => c.classify(&context, raw),
                (Ok(()), None, _) => TurnDecision::new(TurnAct::Unknown, RoutingMethod::Fallback),
            }
        };
        self.classifier = door;
        self.leave_paid_dispatch(entered.is_ok_and(|(_, written)| written));
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
                "routing could not obtain a usable label — `/details` shows the recorded failure; check it before trying again"
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
        let blocked = self.money_blocks_cognition();
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
            // prompt ask (what it reads and writes), then the protocol forms —
            // or, when money blocks cognition, the reason nothing read it.
            _ => {
                let why = if blocked {
                    format!(
                        "that line is not a consent, and nothing read it — {}",
                        self.cognition_blocked()
                    )
                } else {
                    Self::unknown_route_text(SessionPhase::ProposalPending, decision.method)
                };
                let text = format!("{}\n{why}", set.effects_fact());
                self.hold_pending(set, id, &text)
            }
        }
    }

    /// The proposal held with a line beside it.
    pub(super) fn hold_pending(
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
        let findings = self.known.audit_over(
            &reply.text,
            self.census
                .as_ref()
                .map_or(&[], |census| census.provider_context.as_slice()),
        );
        let shown = crate::guard::KnownWorld::correct(&reply.text, &findings);
        self.remember(raw, &shown);
        Some(shown)
    }
}

/// The question's last words outside admitted directives, after the compiler validated their
/// exact spans. This is classification only: no money is parsed, admitted or rewritten here.
pub(super) fn question_outside_money(intent: &str, money: &[std::ops::Range<usize>]) -> bool {
    if money.is_empty() {
        return intent.trim_end().ends_with('?');
    }
    intent
        .char_indices()
        .rev()
        .find(|(at, c)| {
            !money.iter().any(|span| span.contains(at))
                && !c.is_whitespace()
                && !matches!(*c, '.' | ',' | ';' | '!')
        })
        .is_some_and(|(_, c)| c == '?')
}

/// A line the reader does not settle once its admitted directives are blanked is routed as
/// written (R4 A6): conversation stays conversation, unread work stays work.
pub(super) fn as_written(
    reading: Reading,
    round: &AuthoringRound,
    context: &AuthoringContext,
    intent: &str,
) -> Reading {
    match reading {
        // Only a reading already known not to be work can take the question fast path.
        Reading::NotWork(out) if question_outside_money(intent, &round.money) => {
            Reading::NotWork(out)
        }
        Reading::NotWork(out) | Reading::Unsettled(out) if !round.money.is_empty() => {
            let mut written = round.clone();
            written.money.clear();
            match compile_in(&DETERMINISTIC, context, &written.request(), intent).map(Reading::of) {
                Ok(Reading::NotWork(_)) => Reading::NotWork(out),
                _ => Reading::Unsettled(out),
            }
        }
        reading => reading,
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod endpoint_guard_tests {
    use crate::intelligence::{IntelligenceCensus, IntelligenceKind, UserIntelligencePreference};
    use crate::reasoner::ScriptedReasoner;
    use crate::runtime::{SessionRuntime, TurnOutcome};
    use nika_providers::probe::{ExecutionLocus, ProviderProbe, ProviderReadiness};

    #[test]
    fn conversation_and_proposal_answers_keep_the_collected_endpoint_context() {
        const ANSWER: &str = "Use `openai/deepseek-v4-flash-0731`.";
        for (endpoint, refused) in [
            ("https://api.openai.com/v1/chat/completions", true),
            ("https://api.scaleway.ai/v1/chat/completions", false),
        ] {
            let root = tempfile::tempdir().expect("isolated project");
            let mut census = IntelligenceCensus::empty();
            census.api_keys.push("openai".to_owned());
            census.provider_context.push(ProviderProbe::new(
                "openai",
                true,
                true,
                "NIKA_OPENAI_API_KEY",
                true,
                ProviderReadiness::new(
                    true,
                    true,
                    None,
                    None,
                    false,
                    ExecutionLocus::classify(
                        Some(endpoint),
                        "https://api.openai.com/v1/chat/completions",
                    ),
                    nika_types::access::AccessClass::Api,
                ),
                endpoint,
            ));
            let pref = UserIntelligencePreference::new(
                IntelligenceKind::Api {
                    provider: "openai".to_owned(),
                },
                Some("openai/gpt-4o-mini".to_owned()),
            );
            let mut session = SessionRuntime::open_with(
                root.path(),
                census,
                &pref,
                None,
                Box::new(|_| Box::new(ScriptedReasoner::new(vec![ANSWER.into(), ANSWER.into()]))),
            );
            session.set_authoring_context(crate::authoring::AuthoringContext::from_settings(
                &nika_cli_host::compile::config::AuthoringSettings::none(),
                &nika_cli_host::compile::config::AuthoringSettings::none(),
            ));
            // Use the interactive preparation policy, as the TUI does. This
            // scripted reasoner has no real model or metered account.
            session.enable_continuous_preparation();
            // Exercise the conversation owner directly; arbitrary open text first goes
            // through the compiler router, which is outside this guard regression.
            let TurnOutcome::Reply(conversation) =
                session.converse_unrecorded("hello there, how are you today?")
            else {
                panic!("the conversational reply remains a reply");
            };
            let discussion = session
                .reason_about("a proposal under review", "explain it")
                .expect("the proposal discussion answered");
            for shown in [conversation, discussion] {
                assert_eq!(
                    shown.contains("does not resolve in this binary"),
                    refused,
                    "{shown}"
                );
                if !refused {
                    assert_eq!(shown, ANSWER);
                }
            }
        }
    }
}
