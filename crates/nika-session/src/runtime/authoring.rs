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

use nika_onboard::compile::{CompileOutcome, CompileQuestion, CompileRequest, CompileStatus};

use super::{DEFAULT_CEILING_USD, SessionRuntime, TurnOutcome, ceiling_in, named_files};
use crate::activity::{Activity, Phase};
use crate::authoring::{
    AuthoringError, AuthoringRound, AuthoringSeat, Reading, compile_deterministic, compile_through,
    is_cancel, is_greeting, is_why, reasons,
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
        match Reading::of(out) {
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
                let seat_reads = matches!(self.seat, AuthoringSeat::Provider { .. });
                match self.classify(SessionPhase::Idle, intent).act {
                    // Work the deterministic reader did not recognise, routed
                    // as new work: the seat reads it, files named or not;
                    // without a seat the first screen is asked in context,
                    // as for an unsettled reading (never a conversational
                    // paraphrase of a plan that nothing will build).
                    TurnAct::NewWork if seat_reads => Some(self.compile_under_seat(round)),
                    TurnAct::NewWork if !self.chosen || !self.intelligence.ready => {
                        Some(self.ask_for_intelligence(intent, super::Need::Authoring))
                    }
                    TurnAct::Modify | TurnAct::Mixed => {
                        let saved = self.last_workflow.clone()?;
                        Some(self.revise_saved(&saved, intent))
                    }
                    _ => None,
                }
            }
            Reading::Unsettled(out) => Some(match &self.seat {
                AuthoringSeat::Provider { .. } => self.compile_under_seat(round),
                // No intelligence chosen yet — or a kept choice this machine
                // cannot serve: the first screen is asked here, in context,
                // and the request resumes under the choice.
                AuthoringSeat::Deterministic { .. } if !self.chosen || !self.intelligence.ready => {
                    self.ask_for_intelligence(intent, super::Need::Authoring)
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
        self.activity(&Activity::now(Phase::Authoring, self.authoring_note()));
        match compile_through(&self.seat, &round.request()) {
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
                        return match compile_through(&stronger, &round.request()) {
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

    /// The provider's stronger model as an authoring seat, when the seat is
    /// a provider and a stronger model is known for it.
    fn stronger_seat(&self) -> Option<AuthoringSeat> {
        let AuthoringSeat::Provider { model } = &self.seat else {
            return None;
        };
        crate::authoring::stronger_model(model).map(|m| AuthoringSeat::Provider {
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
        match compile_through(&self.seat, &again.request()) {
            Ok(out) => {
                let reading = Reading::of(out);
                self.settle(again, reading)
            }
            Err(e) => self.machinery(&e),
        }
    }

    /// The revised proposal: the new candidate proposed, with the restated
    /// request beside it when one was read (« read as », a paraphrase the
    /// human can correct) and the Meaning delta — what the words changed,
    /// the base reading's ledger against the revised one (§21).
    fn propose_revision(
        &mut self,
        goal: &str,
        out: &CompileOutcome,
        read_as: Option<&str>,
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
        match self.propose(goal, out) {
            TurnOutcome::Proposal { id, preview } => {
                let mut text = String::new();
                if let Some(read_as) = read_as {
                    let _ = writeln!(
                        text,
                        "read as: « {read_as} » (your request with the change, as Nika read it — say it differently if that is not it)"
                    );
                }
                text.push_str(&preview);
                if let Some(delta) = delta {
                    text.push('\n');
                    text.push_str(&delta);
                }
                TurnOutcome::Proposal { id, preview: text }
            }
            other => other,
        }
    }

    /// The request as the human would now say it, with the change applied —
    /// one bounded call to the chosen intelligence; `None` without one, or
    /// when the answer is not one plain request (then the words are used as
    /// said). A paraphrase, shown beside the proposal, never applied unseen.
    fn restate_request(&mut self, goal: &str, change: &str) -> Option<String> {
        if !(self.intelligence.ready && self.chosen && self.reasoner.name() != "none") {
            return None;
        }
        let prompt = format!(
            "A human asked Nika, an automation tool, for this automation: «{goal}».\nNow the human says: «{change}».\nRewrite the request as the human would now state it in full, in one or two plain sentences and in the human's own language, keeping every part they did not change and applying the change exactly (a replaced destination, schedule or step replaces the old one; it is not added beside it). Answer with the rewritten request only: no quotes, no explanation."
        );
        let reply = self.reasoner.reason_label(&prompt).ok()?;
        let line = reply
            .text
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())?
            .trim_matches(|c: char| matches!(c, '"' | '«' | '»' | '\u{201c}' | '\u{201d}'))
            .trim()
            .to_owned();
        (!line.is_empty() && line.len() <= 600 && line != goal).then_some(line)
    }

    /// A change said while nothing waits and a workflow was accepted: the
    /// saved workflow is the base, the human's words the change; the
    /// revision is a new proposal beside it (the same edit door, then the
    /// request read again with the change under the seat).
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
        let goal = self
            .intent
            .goal
            .clone()
            .unwrap_or_else(|| format!("the workflow `{}`", saved.display()));
        let goal = format!("{goal} — {}", change.trim());
        self.activity(&Activity::now(
            Phase::Authoring,
            "revising with your change",
        ));
        let request = CompileRequest::edit(base, change.trim());
        let mut out = match compile_through(&self.seat, &request) {
            Ok(out) => out,
            Err(e) => return self.machinery(&e),
        };
        let settled = out.status == CompileStatus::Ready && out.candidate.is_some();
        if !settled && matches!(self.seat, AuthoringSeat::Provider { .. }) {
            self.activity(&Activity::now(
                Phase::Repairing,
                "reading your request again with the change",
            ));
            let again = CompileRequest::create(goal.clone());
            out = match compile_through(&self.seat, &again) {
                Ok(out) => out,
                Err(e) => return self.machinery(&e),
            };
        }
        match Reading::of(out) {
            Reading::Ready(out) => {
                self.remember(change, "(revised the saved workflow)");
                self.propose(&goal, &out)
            }
            reading => {
                let why = human_reasons(reasons(reading.outcome())).join(" · ");
                self.last_outcome = Some(reading.outcome().clone());
                TurnOutcome::Facts(format!(
                    "I could not revise `{}` with « {} »{} — the saved workflow is unchanged · say the change another way, or describe the whole automation again",
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
            let id = ProposalId::of(&set.preview());
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
        // The compiler's edit door first (today it settles a constant said
        // as « Set const.X to … »; a change in words is contract C6, asked of
        // the Compiler lane); then, under a seat, the request read again
        // WITH the change — the earlier proposal is replaced, never patched
        // by the session itself.
        let request = CompileRequest::edit(base, change.trim());
        let mut out = match compile_through(&self.seat, &request) {
            Ok(out) => out,
            Err(e) => {
                self.pending = Some(set);
                return self.machinery(&e);
            }
        };
        let settled = out.status == CompileStatus::Ready && out.candidate.is_some();
        // Until the compiler's revise door (contract C6) is shared truth, the
        // request is RESTATED with the change by the chosen intelligence —
        // the human's request as they would now say it, shown beside the new
        // proposal so a paraphrase can be corrected — and read again through
        // the same door (deterministic first, the seat when there is one).
        // Without an intelligence the words are composed as said
        // (« request. Change: … »), under a seat only.
        let mut read_as: Option<String> = None;
        if !settled {
            let restated = self.restate_request(&set.goal, change.trim());
            let seated = matches!(self.seat, AuthoringSeat::Provider { .. });
            if restated.is_some() || seated {
                self.activity(&Activity::now(
                    Phase::Repairing,
                    "reading your request again with the change",
                ));
                let text = restated
                    .clone()
                    .unwrap_or_else(|| format!("{}. Change: {}", set.goal, change.trim()));
                let again = CompileRequest::create(text);
                out = match compile_through(&self.seat, &again) {
                    Ok(out) => out,
                    Err(e) => {
                        self.pending = Some(set);
                        return self.machinery(&e);
                    }
                };
                read_as = restated;
            }
        }
        let goal = read_as.clone().unwrap_or(goal);
        match Reading::of(out) {
            Reading::Ready(out) => {
                self.remember(change, "(revised the proposal)");
                self.propose_revision(&goal, &out, read_as.as_deref())
            }
            reading => {
                let id = ProposalId::of(&set.preview());
                let why = human_reasons(reasons(reading.outcome())).join(" · ");
                self.pending = Some(set);
                TurnOutcome::Held {
                    id,
                    preview: format!(
                        "I could not revise the proposal with « {} »{}\n(the proposal still waits · `yes` applies it · `no` discards it · say the change another way)",
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
    /// are named); a compiler failure is a refusal that names it.
    fn machinery(&mut self, error: &AuthoringError) -> TurnOutcome {
        match error {
            AuthoringError::Seat(_) => self.recovery(
                Some(RefusalClass::IntelligenceRefused),
                "I couldn't use the authoring seat for this part",
                &error.to_string(),
            ),
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
                    self.authoring = Some(round);
                    return TurnOutcome::Question {
                        key,
                        question: text,
                    };
                }
                let mut text = question_text(question, &round.reasons);
                if key == "model"
                    && let AuthoringSeat::Provider { model } = &self.seat
                {
                    let _ = write!(
                        text,
                        "\n  Enter takes your seat `{model}` · or name another <provider>/<model>"
                    );
                }
                self.intent.unresolved = vec![question.label.clone()];
                self.remember(&round.intent, &text);
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
                let bytes = set.preview();
                let id = ProposalId::of(&bytes);
                let preview = review::render(&set, out, &bytes);
                self.authoring = None;
                self.intent.unresolved.clear();
                self.remember(goal, &format!("(proposed {id})"));
                // The schedule the request asked for rides beside the set:
                // « activate » declares it once the program is saved.
                self.pending_trigger.clone_from(&out.requested_trigger);
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
        let mut round = match self.route_at_question(round, line) {
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
            self.remember(line, &format!("(restated « {clause} » in words)"));
            return match compile_through(&self.seat, &restated.request()) {
                Ok(out) => {
                    let reading = Reading::of(out);
                    self.settle(restated, reading)
                }
                Err(e) => self.machinery(&e),
            };
        }
        // A cloud model the catalog does not price is refused HERE, in
        // words, with the priced models of its provider — not at run time,
        // where a run under a spending ceiling refuses it (NIKA-1709). The
        // typed line or the seat the empty line took: both are judged.
        if round.current().is_some_and(|q| q.key == "model")
            && let Some(text) = unpriced_model_text(line)
        {
            self.authoring = Some(round);
            return TurnOutcome::Question {
                key: "model".to_owned(),
                question: text,
            };
        }
        let Some(key) = round.answer_current(line) else {
            return TurnOutcome::Refusal(Refusal::new(
                RefusalClass::WrongState,
                "no authoring question waits",
            ));
        };
        self.remember(line, &format!("(answered {key})"));
        match compile_through(&self.seat, &round.request()) {
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
        if !matches!(
            first,
            "run" | "execute" | "test" | "lance" | "exécute" | "teste" | "run:"
        ) {
            return None;
        }
        // The run grammar is closed: the verb, the workflow named or « it »,
        // a ceiling. A line that carries more (« run it, but only on
        // Fridays ») is not a run: its act is a bounded decision, and a
        // change comes before any run.
        if !run_line_is_plain(&lower) {
            return match self.classify(SessionPhase::Idle, input).act {
                TurnAct::RequestRun => Some(self.run_plain(input)),
                TurnAct::Modify | TurnAct::Mixed => Some(TurnOutcome::Refusal(Refusal::new(
                    RefusalClass::WrongState,
                    "a run with a change in it — say the change first (in a sentence), review the new workflow, then « run it »",
                ))),
                _ => None,
            };
        }
        Some(self.run_plain(input))
    }

    /// The closed run line: the verb, the file or the last accepted
    /// workflow, the ceiling.
    fn run_plain(&mut self, input: &str) -> TurnOutcome {
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
        let max_cost_usd = ceiling_in(input)
            .or(self.snapshot.ceiling)
            .unwrap_or(DEFAULT_CEILING_USD);
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
fn question_text(question: &CompileQuestion, reasons: &[String]) -> String {
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
pub(super) fn cannot_express_text(out: &CompileOutcome) -> String {
    let authoring_failed = matches!(
        out.provenance.cognition,
        nika_onboard::compile::AuthoringCognition::ExplicitProvider
    );
    let mut text = if authoring_failed {
        "Nika could not build this automation faithfully yet — the model's draft lost part of your request and Nika refused it; nothing was written.".to_owned()
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
    text.push_str(if authoring_failed {
        "\n  what helps: say it again (another attempt, or `/intelligence` for another model, may hold every part), or split the work in two requests · `/meaning` shows what was understood"
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
pub(super) fn unpriced_model_text(answer: &str) -> Option<String> {
    let (provider, model) = answer.trim().split_once('/')?;
    let row = nika_catalog::all_providers()
        .iter()
        .find(|p| p.id == provider || p.aliases.contains(&provider))?;
    if !row.requires_key || nika_catalog::find_pricing_scoped(row.id, model).is_some() {
        return None;
    }
    let priced: Vec<String> = row
        .models
        .iter()
        .filter(|m| nika_catalog::find_pricing_scoped(row.id, m.model).is_some())
        .map(|m| {
            if m.id.contains('/') {
                m.id.to_owned()
            } else {
                format!("{}/{}", row.id, m.id)
            }
        })
        .collect();
    let mut text = format!(
        "`{}/{model}` is not priced in Nika's catalog: a run under a spending ceiling would refuse it (NIKA-1709 · unpriced cloud spend cannot be bounded).",
        row.id
    );
    if priced.is_empty() {
        let _ = write!(
            text,
            "\n  no priced model is known for `{}` yet — name a priced <provider>/<model>, or `cancel`",
            row.id
        );
    } else {
        let _ = write!(
            text,
            "\n  priced for `{}`: {} — name one of them (the question still waits)",
            row.id,
            priced.join(" · ")
        );
    }
    Some(text)
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
        "rephrase with what to read, what to produce and where to write it, e.g. « read ./docs, draft a digest and write it to ./digest.md »",
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
        if machine || r.is_empty() || kept.iter().any(|k| k == r) {
            continue;
        }
        kept.push(r.to_owned());
    }
    kept
}

/// Whether a run line is the closed grammar and nothing more: the verb,
/// a workflow name, « it », a ceiling phrase, a few fillers. Anything
/// else in the line is a meaning of its own (a change, a condition).
fn run_line_is_plain(lower: &str) -> bool {
    const FILLERS: &[&str] = &[
        "it", "again", "the", "workflow", "once", "now", "this", "that", "le", "la", "ça",
        "encore", "please", "stp", "svp", "with", "a", "ceiling", "of", "cap", "max", "cost",
        "usd", "budget", "plafond", "de", "un", "une", "avec", "at", "à", "$",
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
                || w.trim_start_matches('$').parse::<f64>().is_ok()
        })
}
