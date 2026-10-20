// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Monetary admission and revision custody. The saved binding is to the
//! proposal AND the exact workflow bytes; it never grants execution.

use std::path::PathBuf;

mod saved;

use super::money_parse::{self, ParsedMoney};
use super::{DEFAULT_CEILING_USD, SessionRuntime, TurnOutcome};
use crate::change::{ProjectChangeSet, Witness};
use crate::consent::ConsentRecord;
use crate::money::{CapKnowledge, InferenceEnforcement, MonetaryDecision, MonetarySource};
use crate::outcome::{ProposalId, Refusal, RefusalClass};

pub(super) struct MoneyState {
    pub preparation: Option<nika_providers::authoring::preparation::PreparationCosts>,
    // Persistent Session inference constraint, independent of the next
    // proposal/Run default; a refusal/zero survives even without an account.
    pub inference_guard: Option<MonetaryDecision>,
    pub account: Option<nika_providers::InferenceAdmission>,
    pub admission_note: Option<String>,
    // A paused Run amendment holds cognition only until that gate is answered.
    // It never replaces the Session draft, guard or shared admission account.
    pub gate: Option<MonetaryDecision>,
    pub reconfirm: bool,
    // The live round was admitted while `reconfirm` held: its answers keep that admission.
    pub stated_under_reconfirm: bool,
    pub current: Option<MonetaryDecision>,
    pub draft: Option<MonetaryDecision>,
    pub pending: Option<MonetaryDecision>,
    // The monetary directives of the current work request this gate admitted: the
    // compiler reads them as this ceiling, never as business clauses. A line no work admission
    // reads (a local run line) clears them: no admitted directive rides with it.
    pub admitted: Vec<std::ops::Range<usize>>,
    // Priced calls made without any Session budget ride this account: an
    // observation with no allowance, never admission, a cap or a review.
    pub observed: nika_providers::InferenceAdmission,
    saved: Vec<SavedMoney>,
    // The ceiling a line set for the saved workflow's next runs (« mets le plafond à 0,001 $ »).
    pub next_runs: Option<(PathBuf, MonetaryDecision)>,
}

impl Default for MoneyState {
    fn default() -> Self {
        Self {
            preparation: None,
            inference_guard: None,
            account: None,
            admission_note: None,
            gate: None,
            reconfirm: false,
            stated_under_reconfirm: false,
            current: None,
            draft: None,
            pending: None,
            admitted: Vec::new(),
            observed: nika_providers::InferenceAdmission::unbudgeted(),
            saved: Vec::new(),
            next_runs: None,
        }
    }
}

struct SavedMoney {
    path: PathBuf,
    resolved: Option<PathBuf>,
    witness: Witness,
    decision: MonetaryDecision,
}

/// The refusal when the latest Save of a file bound no ceiling this Session can prove.
const UNPROVEN: &str = "the saved spending constraint cannot be proved in this session — provide an explicit Run ceiling or prepare and review the workflow again; no default was substituted";

/// Why a Run's money was not admitted. Either way the refusal is recorded as the Run's
/// observation ([`SessionRuntime::refuse_run_money`]); no default is substituted.
#[derive(Debug)]
pub(super) enum Unadmitted {
    /// No ceiling can be proved for this Run (the latest Save bound none this Session holds, or
    /// a restored exposure has no default to reconfirm it), and nothing else refuses it: a
    /// ceiling the person states admits it through the explicit path of
    /// [`SessionRuntime::run_money`].
    Unproven(TurnOutcome),
    /// Any other refusal (a changed file, another proposal, an unreadable receipt, several
    /// bindings, an invalid amount): it stands.
    Refused(TurnOutcome),
}

impl Unadmitted {
    /// The refusal as the turn's outcome.
    pub(super) fn outcome(self) -> TurnOutcome {
        match self {
            Self::Unproven(outcome) | Self::Refused(outcome) => outcome,
        }
    }
}

impl SessionRuntime {
    pub(super) fn proposal_preview(&self, set: &ProjectChangeSet) -> String {
        preview_with_money(set, self.money.pending.as_ref())
    }

    pub(super) fn proposal_id(&self, set: &ProjectChangeSet) -> ProposalId {
        ProposalId::of(&self.proposal_preview(set))
    }

    pub(super) fn draft_review(
        &self,
        set: &ProjectChangeSet,
        out: &nika_onboard::compile::CompileOutcome,
        bytes: &str,
    ) -> String {
        let review = crate::review::render(set, out, bytes);
        match &self.money.draft {
            Some(decision) => format!("{review}{}\n", scoped_money_line(decision)),
            None => review,
        }
    }

    pub(super) fn draft_preview(&self, set: &ProjectChangeSet) -> String {
        preview_with_money(set, self.money.draft.as_ref())
    }

    pub(super) fn consent_money_route(
        &mut self,
        set: ProjectChangeSet,
        id: &ProposalId,
        answer: &str,
    ) -> TurnOutcome {
        // Keep custody of the exact reviewed proposal while a cost question waits.
        let review = self.unreviewed_unknown_route();
        if review {
            self.pending = Some(set.clone());
        }
        // Blocked cognition reads nothing, and a line that states no money has nothing
        // to admit: a restored exposure's refusal would expire the proposal for a line
        // nobody read. The route holds it with its observed effects.
        let law = super::authoring::run_prefix(answer).is_none();
        let stated = self.read_money(answer, law);
        let unread = !review
            && self.money_blocks_cognition()
            && stated.as_ref().is_ok_and(|(p, _)| p.amount.is_none());
        if !unread && let Err(refusal) = self.admit_money(answer, true, false) {
            return refusal;
        }
        self.pending = None;
        // A closed monetary-only amendment changes Session's own ceiling,
        // never workflow bytes. It has its own preview and fresh consent id.
        if stated.is_ok_and(|(p, _)| p.money_only) {
            let mut preview = self.draft_preview(&set);
            let new = ProposalId::of(&preview);
            // A rehearsal's proof follows only its exact identity and bytes (`rehearsed.rs`).
            if let Err(withdrawn) = self.rebind_rehearsal(id, &new, &set, &mut preview) {
                return withdrawn;
            }
            self.bind_proposal_money(&new);
            self.pending = Some(set);
            return TurnOutcome::Proposal { id: new, preview };
        }
        let changed = self
            .money
            .draft
            .as_ref()
            .map(|d| (&d.input, d.effective_usd, d.inference))
            != self
                .money
                .pending
                .as_ref()
                .map(|d| (&d.input, d.effective_usd, d.inference));
        let outcome = self.route_at_consent(set, id.clone(), answer);
        if changed && self.pending_proposal().as_ref() == Some(id) {
            return self.refuse_money(answer, "the monetary amendment did not produce a new reviewed proposal — the old consent expired; prepare the revised request");
        }
        outcome
    }

    /// Actual monetary admission for the latest work/Run request or amendment.
    /// A refused request replaces the observation; it cannot expose stale
    /// accepted money. Unknown caps and billing remain explicitly unknown.
    #[must_use]
    pub fn monetary_decision(&self) -> Option<&MonetaryDecision> {
        self.money.current.as_ref()
    }

    pub(super) fn money_line(&self) -> String {
        self.money.current.as_ref().map_or_else(
            || "money: no request admitted · independent caps and billed cost unknown".to_owned(),
            scoped_money_line,
        ) + "\n"
            + &self.inference_line()
    }

    fn money_decision(&self, input: &str, parsed: &ParsedMoney) -> MonetaryDecision {
        let project = self.snapshot.ceiling;
        let explicit = parsed.amount;
        let source = match (explicit, project) {
            (Some(_), Some(_)) => MonetarySource::Override,
            (Some(_), None) => MonetarySource::Explicit,
            (None, Some(_)) => MonetarySource::ProjectDefault,
            (None, None) => MonetarySource::SessionDefault,
        };
        let amount = explicit.or(project).unwrap_or(DEFAULT_CEILING_USD);
        MonetaryDecision {
            original_intent: input.to_owned(),
            input: input.to_owned(),
            effective_usd: Some(amount),
            source,
            explicit_amount: parsed.literal.clone(),
            project_default_usd: project,
            project_file: self.snapshot.project_file.clone(),
            policy_cap: CapKnowledge::Unknown {
                reason: "Session has no independent policy-cap observation; project ceiling is a default".to_owned(),
            },
            machine_cap: CapKnowledge::Unknown {
                reason: "Session has no independent machine-cap observation".to_owned(),
            },
            inference: if explicit.is_some() || amount == 0.0 {
                InferenceEnforcement::CallsBlocked
            } else {
                InferenceEnforcement::NotMetered
            },
            admission: None,
            observed_cost_usd: None,
            proposal: None,
            refusal: None,
        }
    }

    fn rejected_money(&self, input: &str, reason: &str) -> MonetaryDecision {
        let mut decision = self.money_decision(input, &ParsedMoney::default());
        decision.effective_usd = None;
        decision.source = MonetarySource::Rejected;
        decision.refusal = Some(reason.to_owned());
        decision.inference = InferenceEnforcement::CallsBlocked;
        decision
    }

    pub(super) fn refuse_money(&mut self, input: &str, reason: &str) -> TurnOutcome {
        self.retain_money_guard();
        self.hold_subscription();
        if let Some(a) = &self.money.account {
            let _ = a.close("Session monetary request refused");
        }
        let mut decision = self.rejected_money(input, reason);
        if let Some(previous) = self
            .money
            .pending
            .as_ref()
            .or_else(|| self.authoring.as_ref().and(self.money.draft.as_ref()))
        {
            decision
                .original_intent
                .clone_from(&previous.original_intent);
        }
        // Session rejection expires authority, not the cognition restriction.
        self.money.inference_guard = Some(decision.clone());
        self.money.draft = Some(decision.clone());
        self.money.current = Some(decision);
        self.expire_money_authority();
        TurnOutcome::Refusal(Refusal::new(RefusalClass::NotAllowed, reason))
    }

    fn expire_money_authority(&mut self) {
        self.rehearsals.expire_pending();
        self.money.pending = None;
        self.pending = None;
        self.authoring = None;
        self.revising = None;
        self.run_inputs = None;
        self.interrupted = None;
        self.intent.unresolved.clear();
        self.last_outcome = None;
    }

    // Shared validation precedes either scope's continuation fast path. Work, consent and answer
    // lines state money only in their directives; Run and gate lines, the whole line.
    fn read_money(
        &self,
        input: &str,
        law: bool,
    ) -> Result<(ParsedMoney, Vec<std::ops::Range<usize>>), String> {
        let (parsed, spans) = if law {
            let found = money_parse::directives(input).map_err(str::to_owned)?;
            (
                found.money,
                found.found.into_iter().map(|d| d.span).collect(),
            )
        } else {
            (
                money_parse::parse(input).map_err(str::to_owned)?,
                Vec::new(),
            )
        };
        if let Some(error) = &self.snapshot.project_error {
            return Err(format!(
                "project money/default is unavailable: {error} — correct nika.yaml before preparing work"
            ));
        }
        if parsed
            .amount
            .or(self.snapshot.ceiling)
            .is_some_and(|v| !v.is_finite() || v < 0.0)
        {
            return Err(money_parse::INVALID.into());
        }
        if parsed.replaced_default.is_some() && parsed.replaced_default != self.snapshot.ceiling {
            return Err("the stated default to replace does not match the observed project default — confirm one finite, nonnegative amount".into());
        }
        Ok((parsed, spans))
    }

    /// Whether `line` states money and nothing else, as the consent door reads a monetary-only
    /// amendment (« avec un plafond de 2 $ »): a malformed amount states nothing here.
    pub(super) fn states_money_only(&self, line: &str) -> bool {
        let law = super::authoring::run_prefix(line).is_none();
        (self.read_money(line, law)).is_ok_and(|(parsed, _)| parsed.money_only)
    }

    pub(super) fn admit_gate_money(&mut self, input: &str) -> Result<(), TurnOutcome> {
        let mut decision = match self.read_money(input, false) {
            Ok((parsed, _)) if parsed.amount.is_none() => return Ok(()),
            Ok((parsed, _)) => self.money_decision(input, &parsed),
            Err(reason) => {
                let decision = self.rejected_money(input, &reason);
                self.record_gate_money(decision);
                self.expire_money_authority();
                return Err(TurnOutcome::Refusal(Refusal::new(
                    RefusalClass::NotAllowed,
                    reason,
                )));
            }
        };
        decision.inference = InferenceEnforcement::CallsBlocked;
        self.record_gate_money(decision);
        Ok(())
    }

    fn record_gate_money(&mut self, mut decision: MonetaryDecision) {
        if let Some(previous) = &self.money.gate {
            decision
                .original_intent
                .clone_from(&previous.original_intent);
        }
        self.money.gate = Some(decision.clone());
        self.money.current = Some(decision);
    }

    pub(super) fn restore_gate_money(&mut self) {
        if self.money.gate.is_none() {
            // Only a hold was proved. Neither an amount nor an account is restored.
            let decision = self.rejected_money(
                "restored paused gate",
                "restored gate monetary amendment held; amount and billed cost unknown",
            );
            self.record_gate_money(decision);
        }
    }

    pub(super) fn finish_gate_money(&mut self) {
        // Only the protocol answer ends this scope. No account is restored,
        // replaced, reopened or repriced: the prior Session state stayed live.
        if self.money.gate.take().is_some() {
            self.money.current.clone_from(&self.money.draft);
        }
    }

    /// Called before any compiler/classifier/reasoner. A continuation with no monetary clause
    /// keeps the admitted round's money, not a new default, and the no-budget observation its
    /// work began with; a kept round's zero-call `replay` needs no ceiling under the restriction.
    pub(super) fn admit_money(
        &mut self,
        input: &str,
        continuation: bool,
        replay: bool,
    ) -> Result<(), TurnOutcome> {
        if !continuation {
            self.rotate_observation();
        }
        if self.money.preparation.is_none() {
            self.maybe_review_unknown(input)?;
        }
        // A fresh turn is not an escape from a still-pending gate amendment.
        if self.money.gate.is_some() {
            return self.admit_gate_money(input);
        }
        let law = super::authoring::run_prefix(input).is_none();
        let parsed = match self.read_money(input, law) {
            Ok((parsed, spans)) => {
                if !continuation {
                    self.money.admitted = spans;
                }
                parsed
            }
            Err(reason) => {
                self.money.admitted.clear();
                return Err(self.refuse_money(input, &reason));
            }
        };
        if self.money.preparation.is_none() && self.subscription() && parsed.amount.is_some() {
            self.hold_subscription();
        }
        // The Session restriction cannot replace an independent gate observation.
        // Gate parsing above still holds cognition, but does not consume this flag.
        // A round admitted under it (a stated ceiling or a zero-call replay) answers under it.
        if self.money.preparation.is_none()
            && self.money.reconfirm
            && !self.subscription_open()
            && !self.unknown_cost.active
            && parsed.amount.is_none()
            && !(replay || (continuation && self.money.stated_under_reconfirm))
        {
            let why = self.restored_refusal();
            return Err(self.refuse_money(input, &why));
        }
        if continuation && parsed.amount.is_none() {
            return Ok(());
        }
        if !continuation {
            self.money.stated_under_reconfirm = self.money.reconfirm;
        }
        let mut decision = self.money_decision(input, &parsed);
        if continuation && let Some(previous) = &self.money.draft {
            decision
                .original_intent
                .clone_from(&previous.original_intent);
        }
        if self.money.preparation.is_some() {
            decision.inference = InferenceEnforcement::NotMetered;
        } else if self.unknown_cost.active {
            decision.inference = InferenceEnforcement::ExplicitUnknown;
        } else if parsed.amount.is_some() {
            self.retain_money_guard();
            self.configure_admission(&mut decision);
            self.money.inference_guard = Some(decision.clone());
        } else if self.money.inference_guard.is_none() && self.money.account.is_none() {
            self.configure_admission(&mut decision);
            if decision.inference == InferenceEnforcement::CallsBlocked {
                self.retain_money_guard();
                self.money.inference_guard = Some(decision.clone());
            }
        }
        // A fresh request owns its proposal/Run default, but only an explicit
        // Session amendment can change the persistent inference constraint.
        let mut decision = if self.money.preparation.is_some() {
            decision
        } else {
            self.inference_observation(decision)
        };
        if !continuation && parsed.amount.is_none() {
            self.keep_copied_ceiling(input, &mut decision);
        }
        self.money.current = Some(decision.clone());
        self.money.draft = Some(decision);
        Ok(())
    }

    /// A request that copies a saved workflow (« identique à rapport.nika », « une copie de
    /// rapport.nika ») and states no ceiling keeps, as its proposal and Run ceiling, the one the
    /// person set for that workflow's next runs in this conversation, else the one they saved
    /// with it, as a revision of it does; other new work keeps its own default. The Session
    /// inference constraint is left as it was, and a project default stands.
    fn keep_copied_ceiling(&self, input: &str, decision: &mut MonetaryDecision) {
        if decision.source != MonetarySource::SessionDefault {
            return;
        }
        let Some(base) = self.copied_workflow(input) else {
            return;
        };
        // The latest ceiling the person said for the base comes first (live gauntlet wave 7 r09,
        // 2026-10-19: « Le plafond est de 0,10 $ par run stp », then « identique à
        // rapport-ventes-2026 »: the copy ran at $0.25).
        let root = &self.snapshot.root;
        let set = (self.money.next_runs.as_ref())
            .filter(|(set_for, _)| root.join(set_for) == root.join(&base))
            .map(|(_, decision)| decision.clone());
        let Some(kept) = set.or_else(|| self.saved_ceiling(&base)) else {
            return;
        };
        decision.effective_usd = kept.effective_usd;
        decision.source = kept.source;
        decision.explicit_amount.clone_from(&kept.explicit_amount);
    }

    /// The saved workflow a request asks a copy of: a workflow of the project it names, with a
    /// word that makes the new one its copy (« identique à », « copie de », « same as »).
    fn copied_workflow(&self, input: &str) -> Option<String> {
        const COPY: &[&str] = &[
            "identique",
            "identiques",
            "copie",
            "copier",
            "duplique",
            "dupliquer",
            "pareil",
            "même",
            "same",
            "copy",
            "duplicate",
            "identical",
            "clone",
        ];
        let lowered = input.replace('\u{2019}', "'").to_lowercase();
        let words: Vec<&str> = (lowered
            .split(|c: char| c.is_whitespace() || ",;:!?()«»\"`".contains(c)))
        .map(|word| word.trim_end_matches('.'))
        .filter(|word| !word.is_empty())
        .collect();
        if !words
            .iter()
            .any(|word| COPY.contains(&word.rsplit('\'').next().unwrap_or(word)))
        {
            return None;
        }
        let named = |path: &str| {
            let path = path.to_lowercase();
            let stem = path.trim_end_matches(".nika").to_owned();
            words.iter().any(|word| {
                let word = word
                    .rsplit('\'')
                    .next()
                    .unwrap_or(word)
                    .trim_start_matches("./");
                word == path || word == stem
            })
        };
        let mut hits = (self.snapshot.workflows.iter()).filter(|w| named(&w.path));
        let first = hits.next()?;
        hits.next().is_none().then(|| first.path.clone())
    }

    /// The amount a line sets as the ceiling and nothing else (« Mets le plafond à 0,001 $ »,
    /// « plafond 0,05 $ par run », « set the ceiling to $0.05 »): its words are a verb that sets
    /// it, the ceiling's name, link words and the amount. `None` for any other line.
    pub(super) fn sets_the_ceiling(&self, line: &str) -> Option<f64> {
        let amount = (self.ceiling_line_money(line))
            .and_then(|parsed| parsed.amount)
            .filter(|amount| amount.is_finite() && *amount >= 0.0)?;
        let lowered = line.replace('\u{2019}', "'").to_lowercase();
        let only = (lowered.split(|c: char| c.is_whitespace() || "!?;:,«»".contains(c)))
            .map(|word| word.trim_end_matches('.'))
            .filter(|word| !word.is_empty())
            .all(|word| {
                CEILING_LINE_WORDS.contains(&word)
                    || word.starts_with(|c: char| c.is_ascii_digit())
                    || word
                        .strip_prefix('$')
                        .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
            });
        only.then_some(amount)
    }

    /// The money of a line that holds nothing but a ceiling, its courtesy aside: read by its
    /// directive, else whole (« Le plafond est de 0,10 $ par run » names no directive the law
    /// reads, and holds no other word).
    fn ceiling_line_money(&self, line: &str) -> Option<ParsedMoney> {
        // The copula is a link (« Le plafond est de 0,10 $ »), as a directive's own words read it.
        let words = (without_courtesy(line).split_whitespace())
            .filter(|word| !matches!(word.to_lowercase().as_str(), "est" | "sera" | "is"))
            .collect::<Vec<_>>()
            .join(" ");
        let directive = (self.read_money(&words, true).ok())
            .map(|(parsed, _)| parsed)
            .filter(|parsed| parsed.amount.is_some());
        directive.or_else(|| {
            (self.read_money(&words, false).ok())
                .map(|(parsed, _)| parsed)
                .filter(|parsed| parsed.amount.is_some())
        })
    }

    /// A ceiling set alone while nothing waits is the saved workflow's next runs' (live gauntlet
    /// r09, 2026-10-17): its outcome, or `None` when something waits or the line is not one.
    pub(super) fn ceiling_alone(&mut self, input: &str) -> Option<TurnOutcome> {
        let idle = self.pending.is_none()
            && self.authoring.is_none()
            && self.run_inputs.is_none()
            && self.activation.is_none()
            && self.install_offer.is_none()
            && self.pending_gate.is_none();
        let workflow = self.last_workflow.clone().filter(|_| idle)?;
        self.sets_the_ceiling(input)?;
        Some(self.set_run_ceiling(input, workflow))
    }

    /// A ceiling set alone with nothing waiting, for the saved workflow's next runs in this
    /// conversation: said, and kept as the person's ceiling for the work that follows.
    pub(super) fn set_run_ceiling(&mut self, line: &str, workflow: PathBuf) -> TurnOutcome {
        let Some(parsed) = self.ceiling_line_money(line) else {
            return self.refuse_money(line, money_parse::INVALID);
        };
        let decision = self.money_decision(line, &parsed);
        let Some(amount) = decision.effective_usd else {
            return self.refuse_money(line, money_parse::INVALID);
        };
        let shown = workflow.display().to_string();
        self.money.next_runs = Some((workflow, decision));
        let stated = nika_display::vocab::stated_usd(amount);
        TurnOutcome::Facts(match self.language {
            super::Lang::Fr => format!(
                "plafond de `{shown}` : {} $ par run · les prochains runs de `{shown}` dans cette conversation partent avec ce plafond",
                stated.replace('.', ",")
            ),
            _ => format!(
                "ceiling of `{shown}`: ${stated} per run · the next runs of `{shown}` in this conversation start under it"
            ),
        })
    }

    /// The monetary directives of request bytes Session built from the human's words (a
    /// replacement, a restatement), as spans of exactly those bytes, never offsets carried from
    /// another text. A malformed or conflicting amount refuses under the money law, and so
    /// does a built request stating another ceiling than the account holds.
    pub(super) fn built_money(
        &mut self,
        text: &str,
        line: &str,
    ) -> Result<Vec<std::ops::Range<usize>>, TurnOutcome> {
        let found = money_parse::directives(text).map_err(|why| self.refuse_money(line, why))?;
        let held = self.money.draft.as_ref().and_then(|d| d.effective_usd);
        if (found.money.amount)
            .is_some_and(|stated| held.is_none_or(|h| h.to_bits() != stated.to_bits()))
        {
            let why = "the request these words build states another ceiling than the one admitted — say the budget as its own sentence";
            return Err(self.refuse_money(line, why));
        }
        Ok(found.found.into_iter().map(|d| d.span).collect())
    }

    /// Called at every cognition seam. Deterministic reading stays available;
    /// the selected intelligence is never substituted by a monetary decision.
    pub(super) fn money_blocks_cognition(&self) -> bool {
        if self.money.preparation.is_some() {
            return false;
        }
        if self.subscription_open() {
            return self.money.gate.is_some()
                || self.snapshot.ceiling == Some(0.0)
                || self
                    .intent
                    .decisions
                    .iter()
                    .any(|d| d.starts_with(super::inference::GATE_MONEY_PREFIX));
        }
        if self.unreviewed_unknown_route() {
            return true;
        }
        if self.money.gate.is_some() || (self.money.reconfirm && !self.unknown_cost.active) {
            return true;
        }
        if let Some(a) = &self.money.account
            && a.snapshot().map_or(true, |r| {
                r.state != nika_providers::AdmissionState::Open
                    || (r.unknown_cost.is_none() && r.limit.nano_usd == 0)
            })
        {
            return true;
        }
        self.money
            .inference_guard
            .as_ref()
            .is_some_and(|d| d.inference == InferenceEnforcement::CallsBlocked)
    }

    pub(super) fn cognition_money_refusal(&self) -> TurnOutcome {
        TurnOutcome::Refusal(Refusal::new(
            RefusalClass::NotAllowed,
            self.cognition_blocked(),
        ))
    }

    /// Why no cognition reads a line now, in the account's own words.
    pub(super) fn cognition_blocked(&self) -> String {
        let way = if self.money.reconfirm && self.money.account.is_none() {
            format!(" · {}", super::inference::RESTORED_WAY)
        } else {
            String::new()
        };
        format!(
            "no further cognition admitted on {}: {} · deterministic work remains available; billed cost is unknown{way}",
            self.reasoner.name(),
            self.inference_line()
        )
    }

    pub(super) fn bind_proposal_money(&mut self, id: &ProposalId) {
        let receipt = self.money.account.as_ref().and_then(|a| a.snapshot().ok());
        self.money.pending = self.money.draft.clone().map(|mut d| {
            d.proposal = Some(id.clone());
            d.admission = receipt;
            d
        });
        if self.money.pending.is_some() {
            self.money.current.clone_from(&self.money.pending);
        }
    }

    pub(super) fn save_proposal_money(&mut self, set: &ProjectChangeSet, id: &ProposalId) {
        let Some(decision) = self
            .money
            .pending
            .take()
            .filter(|d| d.proposal.as_ref() == Some(id))
        else {
            return;
        };
        for change in set.changes.iter().filter(|c| c.is_workflow()) {
            let path = change.path();
            self.money.saved.retain(|saved| saved.path != path);
            self.money.saved.push(SavedMoney {
                resolved: self.snapshot.root.join(&path).canonicalize().ok(),
                path,
                witness: Witness::of(change.content().as_bytes()),
                decision: decision.clone(),
            });
        }
        self.money.current = Some(decision);
    }

    /// A saved workflow's revision stating no amount keeps that workflow's ceiling: the one live
    /// binding to the resolved file AND the exact base the revision compiled, including the
    /// durable constraint after reopening. Changed or missing evidence refuses; a file never
    /// linked to a Save keeps the change's own admitted law. No account is replaced or
    /// refilled and no Run authority is restored.
    pub(super) fn bind_revision_money(
        &mut self,
        saved: &std::path::Path,
        change: &str,
        base: &Witness,
    ) -> Result<(), TurnOutcome> {
        if self
            .read_money(change, true)
            .is_ok_and(|(parsed, _)| parsed.amount.is_some())
        {
            return Ok(());
        }
        let resolved = self.snapshot.root.join(saved).canonicalize();
        let kept = match resolved.map(|resolved| self.saved_binding(saved, &resolved, Some(base))) {
            Ok(Ok(None)) => return Ok(()),
            Ok(Ok(Some(kept))) => kept,
            Ok(Err(why)) => {
                let (head, _) = why.split_once(" — ").unwrap_or((why, ""));
                let why = format!(
                    "{head} — this revision cannot inherit its ceiling · nothing was sent · or state this revision's ceiling"
                );
                return Err(self.refuse_money(change, &why));
            }
            Err(_) => {
                let why =
                    "the revised workflow's identity could not be resolved — nothing was sent";
                return Err(self.refuse_money(change, why));
            }
        };
        let mut decision = self.inference_observation(kept);
        decision.proposal = None;
        self.money.current = Some(decision.clone());
        self.money.draft = Some(decision);
        Ok(())
    }

    /// The ceiling the person stated for the work: an admitted amount of theirs, never a default.
    pub(super) fn stated_ceiling(&self) -> Option<f64> {
        let draft = self.money.draft.as_ref()?;
        let theirs = matches!(
            draft.source,
            MonetarySource::Explicit | MonetarySource::Override
        );
        (theirs && draft.refusal.is_none())
            .then_some(draft.effective_usd)
            .flatten()
    }

    /// The ceiling the person stated, saved with `workflow`'s current bytes and proven as a Run
    /// proves it: the latest Save's receipt, or this Session's live binding of those bytes.
    /// None for a saved default, for unproven evidence and for a file no Save bound.
    pub(super) fn saved_ceiling(&self, workflow: &str) -> Option<MonetaryDecision> {
        let target = self.snapshot.root.join(workflow);
        let resolved = target.canonicalize().ok()?;
        let now = std::fs::read(&target).ok().map(|bytes| Witness::of(&bytes));
        let saved = std::path::Path::new(workflow);
        let kept = (self.saved_binding(saved, &resolved, now.as_ref()))
            .ok()
            .flatten()?;
        let theirs = matches!(
            kept.source,
            MonetarySource::Explicit | MonetarySource::Override
        );
        (theirs && kept.refusal.is_none()).then_some(kept)
    }

    /// A conversation's revision of a saved workflow, with no ceiling of its own, keeps the one
    /// the person stated and saved with it, as a revision through the request door does
    /// ([`Self::bind_revision_money`]): its Save keeps it and a rerun reuses it. QA gauntlet q09:
    /// the revision bound the $0.25 default, and « Relance-le » ran under it.
    pub(super) fn keep_saved_ceiling(&mut self, workflow: &str) {
        if self.stated_ceiling().is_some() {
            return;
        }
        let Some(kept) = self.saved_ceiling(workflow) else {
            return;
        };
        let mut decision = self.inference_observation(kept);
        decision.proposal = None;
        self.money.current = Some(decision.clone());
        self.money.draft = Some(decision);
    }

    /// Whether the shared account refused a label since `before`, before any send: a new refusal
    /// and no new priced or unknown attempt. An unreadable snapshot proves nothing (fail closed).
    pub(super) fn label_refused(&self, before: Option<&nika_providers::InferenceReceipt>) -> bool {
        let (Some(before), Ok(Some(after))) = (before, self.inference_receipt()) else {
            return false;
        };
        after.refusal.is_some()
            && after.refusal != before.refusal
            && after.attempts.len() == before.attempts.len()
            && after.unknown_attempts.len() == before.unknown_attempts.len()
    }

    /// Run refusals change the Run observation, never the shared Session account.
    pub(super) fn refuse_run_money(&mut self, input: &str, reason: &str) -> TurnOutcome {
        self.money.current = Some(self.inference_observation(self.rejected_money(input, reason)));
        let (unbound, french) = super::run_budget::UNBOUND;
        let said = match self.language {
            super::Lang::Fr if reason == unbound => french,
            _ => reason,
        };
        TurnOutcome::Refusal(Refusal::new(RefusalClass::NotAllowed, said))
    }

    pub(super) fn run_money(
        &mut self,
        input: &str,
        workflow: &std::path::Path,
        explicit: Option<f64>,
    ) -> Result<f64, Unadmitted> {
        // A stated ceiling a free sentence names by its directive stands when the whole line
        // also mentions another amount (QA gauntlet q03: « Peut-être que le plafond de 0,25 $ est
        // trop bas. Lance translate-notes.nika avec un plafond de 2 $. »): the line is then read
        // by its directives, as the request door reads it.
        // The ceiling a line set for this workflow's next runs stands for a run line stating none.
        let explicit = explicit.or_else(|| self.next_runs_ceiling(workflow));
        let read = (self.read_money(input, false))
            .or_else(|why| explicit.map_or(Err(why), |_| self.read_money(input, true)));
        let (mut parsed, _) =
            read.map_err(|reason| Unadmitted::Refused(self.refuse_run_money(input, &reason)))?;
        if explicit.is_some() {
            // The ceiling stated: the line's own, or a typed request's (`save & run`).
            parsed.amount = explicit;
            self.money.current =
                Some(self.inference_observation(self.money_decision(input, &parsed)));
        } else {
            self.restore_run_money(input, workflow)?;
        }
        self.money
            .current
            .as_ref()
            .and_then(|d| d.effective_usd)
            .ok_or_else(|| Unadmitted::Refused(self.refuse_run_money(input, money_parse::INVALID)))
    }

    /// The ceiling a line set for `workflow`'s next runs, if any.
    pub(super) fn next_runs_ceiling(&self, workflow: &std::path::Path) -> Option<f64> {
        let (set_for, decision) = self.money.next_runs.as_ref()?;
        let root = &self.snapshot.root;
        (root.join(set_for) == root.join(workflow))
            .then_some(decision.effective_usd)
            .flatten()
    }

    fn restore_run_money(
        &mut self,
        input: &str,
        workflow: &std::path::Path,
    ) -> Result<(), Unadmitted> {
        let target = self.snapshot.root.join(workflow);
        let Ok(resolved) = target.canonicalize() else {
            let why = "the workflow identity could not be resolved — review it before running";
            return Err(Unadmitted::Refused(self.refuse_run_money(input, why)));
        };
        let now = std::fs::read(&target).ok().map(|bytes| Witness::of(&bytes));
        match self.saved_binding(workflow, &resolved, now.as_ref()) {
            Ok(Some(decision)) => {
                self.money.current = Some(self.inference_observation(decision));
                return Ok(());
            }
            Err(UNPROVEN) => {
                return Err(Unadmitted::Unproven(self.refuse_run_money(input, UNPROVEN)));
            }
            Err(why) => return Err(Unadmitted::Refused(self.refuse_run_money(input, why))),
            Ok(None) => {}
        }
        // A restored exposure has no default to reconfirm it: only a stated ceiling or the exact
        // proof of a saved one runs, and neither settles that earlier exposure.
        if self.money.reconfirm {
            let why = self.restored_refusal();
            return Err(Unadmitted::Unproven(self.refuse_run_money(input, &why)));
        }
        // No saved identity or journal claim binds this file. Its own Run
        // default does not amend the ongoing Session inference allowance.
        self.money.current =
            Some(self.inference_observation(self.money_decision(input, &ParsedMoney::default())));
        Ok(())
    }

    /// The latest Save's exact execution constraint, shared by Run and revision.
    /// Durable evidence is checked before RAM so another Session's newer Save cannot
    /// leave an older ceiling effective. A live binding can cover its own Save whose
    /// monetary receipt failed to write, never a different or incomplete Save.
    fn saved_binding(
        &self,
        workflow: &std::path::Path,
        resolved: &PathBuf,
        witness: Option<&Witness>,
    ) -> Result<Option<MonetaryDecision>, &'static str> {
        let records = match ConsentRecord::read_all(&self.snapshot.root) {
            Ok(records) if records.iter().all(|r| r.version == ConsentRecord::VERSION) => records,
            _ => {
                return Err(
                    "saved monetary evidence is unreadable — provide an explicit ceiling or prepare and review the workflow again",
                );
            }
        };
        let latest = records.iter().rev().find(|record| {
            record
                .written
                .iter()
                .any(|path| self.names(path, workflow, resolved))
        });
        let Some(record) = latest else {
            return self.live_binding(workflow, resolved, witness);
        };
        let restored = saved::restore(&self.snapshot.root, record, resolved, witness)?;
        if let Some(mut decision) = restored {
            // The receipt proves the ceiling and grants no proposal. The proposal is provenance
            // this Session observed: its live binding of that same Save, never an id read back
            // from the receipt.
            decision.proposal = (self.live_binding(workflow, resolved, witness).ok())
                .flatten()
                .and_then(|live| live.proposal)
                .filter(|id| id.as_str() == record.proposal);
            return Ok(Some(decision));
        }
        if let Some(decision) = self.live_binding(workflow, resolved, witness)?
            && decision
                .proposal
                .as_ref()
                .is_some_and(|p| p.as_str() == record.proposal)
            && record.witnesses.iter().any(|w| {
                self.names(&w.path, workflow, resolved)
                    && witness.is_some_and(|now| now.0 == w.after)
            })
        {
            return Ok(Some(decision));
        }
        Err(UNPROVEN)
    }

    /// Whether `path` names `workflow`: the same text, or a file resolving to `resolved`.
    fn names(
        &self,
        path: &std::path::Path,
        workflow: &std::path::Path,
        resolved: &PathBuf,
    ) -> bool {
        path == workflow
            || (self.snapshot.root.join(path).canonicalize()).is_ok_and(|p| p == *resolved)
    }

    /// The one live monetary binding of `workflow` (Run and revision share this law): `Ok(None)`
    /// when none names it; refused when several do, or when the one that does was made for
    /// another resolved identity or for other bytes than `witness` (none: unreadable).
    fn live_binding(
        &self,
        workflow: &std::path::Path,
        resolved: &PathBuf,
        witness: Option<&Witness>,
    ) -> Result<Option<MonetaryDecision>, &'static str> {
        let mut saved = (self.money.saved.iter()).filter(|saved| {
            self.names(&saved.path, workflow, resolved) || saved.resolved.as_ref() == Some(resolved)
        });
        let Some(kept) = saved.next() else {
            return Ok(None);
        };
        if saved.next().is_some() {
            return Err(
                "multiple saved monetary decisions resolve to this workflow — prepare and review one unambiguous revision",
            );
        }
        if kept.resolved.as_ref() != Some(resolved) || witness != Some(&kept.witness) {
            return Err(
                "the prepared workflow changed since its monetary decision — prepare and review the revision before running",
            );
        }
        Ok(Some(kept.decision.clone()))
    }

    fn inference_observation(&self, mut decision: MonetaryDecision) -> MonetaryDecision {
        decision.inference = if self.money_blocks_cognition() {
            InferenceEnforcement::CallsBlocked
        } else if self.unknown_cost.active {
            InferenceEnforcement::ExplicitUnknown
        } else if self.money.account.is_some() && !self.subscription() {
            InferenceEnforcement::CatalogAdmission
        } else {
            InferenceEnforcement::NotMetered
        };
        decision.admission = self.money.account.as_ref().and_then(|a| a.snapshot().ok());
        decision
    }
}

/// The words a line that sets the ceiling alone may hold beside its amount: a verb that sets
/// it, the ceiling's name, link words, the currency and courtesy.
const CEILING_LINE_WORDS: &[&str] = &[
    "mets",
    "met",
    "mettez",
    "remets",
    "remettez",
    "fixe",
    "fixez",
    "règle",
    "réglez",
    "passe",
    "passez",
    "change",
    "changez",
    "baisse",
    "baissez",
    "monte",
    "montez",
    "augmente",
    "augmentez",
    "descends",
    "descendez",
    "définis",
    "définissez",
    "set",
    "make",
    "lower",
    "raise",
    "put",
    "reset",
    "le",
    "la",
    "les",
    "un",
    "une",
    "mon",
    "ma",
    "ton",
    "ta",
    "son",
    "sa",
    "notre",
    "votre",
    "ce",
    "cet",
    "cette",
    "the",
    "a",
    "my",
    "its",
    "this",
    "plafond",
    "budget",
    "ceiling",
    "cap",
    "limite",
    "limit",
    "maximum",
    "max",
    "à",
    "de",
    "du",
    "to",
    "at",
    "of",
    "for",
    "pour",
    "par",
    "per",
    "run",
    "runs",
    "exécution",
    "chaque",
    "each",
    "every",
    "prochains",
    "prochain",
    "next",
    "maintenant",
    "now",
    "désormais",
    "dorénavant",
    "stp",
    "svp",
    "please",
    "merci",
    "alors",
    "ok",
    "d'accord",
    "plutôt",
    "instead",
    "est",
    "is",
    "sera",
    "$",
    "usd",
    "dollar",
    "dollars",
];

/// A line without the courtesy around it (« stp », « s'il te plaît », « please »), which the
/// money law reads past no directive.
fn without_courtesy(line: &str) -> String {
    const COURTESY: &[&str] = &[
        "stp", "svp", "please", "merci", "s'il", "te", "vous", "plaît",
    ];
    let words: Vec<&str> = line.split_whitespace().collect();
    let courtesy = |word: &str| {
        let bare = (word.trim_matches(|c: char| ",.!?;:".contains(c)))
            .replace('\u{2019}', "'")
            .to_lowercase();
        COURTESY.contains(&bare.as_str())
    };
    let start = words.iter().take_while(|w| courtesy(w)).count();
    let end = words.len() - words.iter().rev().take_while(|w| courtesy(w)).count();
    words
        .get(start..end.max(start))
        .unwrap_or_default()
        .join(" ")
}

fn preview_with_money(set: &ProjectChangeSet, decision: Option<&MonetaryDecision>) -> String {
    match decision {
        // The words the decision binds: money input only when they stated an amount, the
        // request otherwise (a default ceiling read no money from them).
        Some(decision) => format!(
            "{}\n{}\n{}: «{}»\n",
            set.preview(),
            scoped_money_line(decision),
            if decision.explicit_amount.is_some() {
                "monetary input"
            } else {
                "request"
            },
            decision.input
        ),
        None => set.preview(),
    }
}

// The work's execution ceiling and Session inference allowance can differ.
// Render only stable admission limits here: live spending must not change a
// reviewed proposal's consent identity between preview and an answer.
fn scoped_money_line(decision: &MonetaryDecision) -> String {
    if decision.refusal.is_some() {
        return decision.line();
    }
    let inference = match decision.inference {
        InferenceEnforcement::ExplicitUnknown => "explicit unknown cost · finite one-time inference · no USD guarantee · separate Run review required".into(),
        InferenceEnforcement::CatalogAdmission => decision.admission.as_ref().map_or_else(
            || "catalog allowance tracked separately".to_owned(),
            |receipt| format!("catalog allowance {}", receipt.limit),
        ),
        InferenceEnforcement::CallsBlocked => {
            "blocked by the Session monetary constraint".to_owned()
        }
        InferenceEnforcement::NotMetered => {
            "no Session budget; no aggregate inference allowance; qualified priced calls observed at catalog estimates, never capped".to_owned()
        }
    };
    format!(
        "money: ${} USD · {} · proposal/Run ceiling · {} · policy cap unknown · machine cap unknown · execution requires a separate Run and downstream admission\nSession inference: {inference}; separate from the proposal/Run ceiling; catalog estimates are not invoices or a hard billing cap; billed cost unknown",
        nika_display::vocab::stated_usd(decision.effective_usd.unwrap_or_default()),
        source_words(decision.source),
        project_default_words(decision.project_default_usd),
    )
}

/// Where the ceiling came from, in the words a person reads: the enum's
/// name is a type, not a sentence.
fn source_words(source: MonetarySource) -> &'static str {
    match source {
        MonetarySource::SessionDefault => "session default",
        MonetarySource::ProjectDefault => "project default",
        MonetarySource::Explicit => "the amount you gave",
        MonetarySource::Override => "the amount you gave, over the project default",
        MonetarySource::Rejected => "refused",
    }
}

/// The project's own default ceiling, or its absence said as such (never
/// an `Option` printed as-is).
fn project_default_words(usd: Option<f64>) -> String {
    usd.map_or_else(
        || "no project default".to_owned(),
        |usd| {
            format!(
                "project default ${} USD",
                nika_display::vocab::stated_usd(usd)
            )
        },
    )
}

#[cfg(test)]
mod tests {
    use super::{project_default_words, source_words};
    use crate::money::MonetarySource;

    /// The consent's money line names where its ceiling came from in words,
    /// never a type name or an `Option` printed as-is.
    #[test]
    fn the_money_line_speaks_words_not_types() {
        assert_eq!(
            source_words(MonetarySource::SessionDefault),
            "session default"
        );
        assert_eq!(
            source_words(MonetarySource::ProjectDefault),
            "project default"
        );
        assert_eq!(
            source_words(MonetarySource::Override),
            "the amount you gave, over the project default"
        );
        assert_eq!(project_default_words(None), "no project default");
        assert_eq!(project_default_words(Some(1.0)), "project default $1 USD");
        assert_eq!(
            project_default_words(Some(0.05)),
            "project default $0.05 USD"
        );
        // An amount as it was set: cents where it has a fraction, never rounded to zero.
        assert_eq!(
            project_default_words(Some(0.1)),
            "project default $0.10 USD"
        );
        assert_eq!(
            project_default_words(Some(0.001)),
            "project default $0.001 USD"
        );
    }
}
