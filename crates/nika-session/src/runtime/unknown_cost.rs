// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! One-time unknown-cost decisions, before any cognition. Observations are durable;
//! reviews and admission authority are deliberately not deserializable.
use super::{Refusal, RefusalClass, SessionRuntime, TurnOutcome};
use crate::authoring::AUTHORING_REPAIRS;
use nika_onboard::compile::authority::{recovery_requests, worst_case};
use nika_providers::admission::{CompletedCostReport, LegacyCostReport};
use nika_runtime::cost_choice::{CostHostEvidence, CostReview, CostRoute, monetary_default};
use std::io::Read as _;

#[derive(Default)]
pub(super) struct UnknownCostState {
    pub host: CostHostEvidence,
    pending: Option<PendingCost>,
    pub active: bool,
    pub in_consent: bool,
    pub observations: Vec<serde_json::Value>,
    pub completed_restored: bool,
}
struct PendingCost {
    review: CostReview,
    input: String,
    consent: bool,
}
impl SessionRuntime {
    /// Supply actual host evidence. Default library/service/scheduled scopes
    /// refuse unknown cost. Changing it invalidates any displayed review.
    pub fn set_cost_host_evidence(&mut self, evidence: CostHostEvidence) {
        if self.unknown_cost.pending.take().is_some() {
            self.pending = None;
            self.money.pending = None;
        }
        if let Some(a) = &self.money.account
            && self.unknown_cost.active
        {
            let _ = a.close("host monetary evidence changed");
        }
        self.unknown_cost.active = false;
        self.unknown_cost.host = evidence;
    }
    /// Whether the next line answers a one-time cost question, before any
    /// authoring, proposal consent or intelligence-choice routing.
    #[must_use]
    pub fn waiting_cost_choice(&self) -> bool {
        self.unknown_cost.pending.is_some()
    }

    /// Details of the pending choice (endpoint, witness, host evidence).
    #[must_use]
    pub fn cost_choice_details(&self) -> Option<String> {
        self.unknown_cost
            .pending
            .as_ref()
            .map(|p| p.review.details())
    }
    /// Historical observations plus the current account and, once it observed
    /// a call, the no-budget account — and, once it was needed, the operator-selected
    /// decision seat's journal (its own schema, cost unknown). No entry is authority.
    #[must_use]
    pub fn cost_observations(&self) -> Vec<serde_json::Value> {
        let mut observations = self.unknown_cost.observations.clone();
        if let Some(observed) = self
            .money
            .preparation
            .as_ref()
            .and_then(nika_providers::authoring::preparation::PreparationCosts::observation)
        {
            observations.push(observed);
        }
        if let Ok(Some(receipt)) = self.inference_receipt() {
            observations.push(receipt.durable_observation());
        }
        if let Ok(receipt) = self.money.observed.snapshot()
            && !receipt.attempts.is_empty()
        {
            observations.push(receipt.durable_observation());
        }
        if let Some(setup) = self.authoring_context.decision() {
            observations.extend(setup.observations());
        }
        observations
    }
    fn legacy_report(&self) -> Option<LegacyCostReport> {
        if !self.money.reconfirm || self.money.account.is_some() {
            return None;
        }
        LegacyCostReport::read(&self.unknown_cost.observations).ok()
    }
    pub(super) fn completed_report(&self) -> Option<CompletedCostReport> {
        if !self.unknown_cost.completed_restored
            || !self.money.reconfirm
            || self.money.account.is_some()
        {
            return None;
        }
        CompletedCostReport::read(&self.unknown_cost.observations).ok()
    }
    pub(super) fn unreviewed_unknown_route(&self) -> bool {
        self.money.preparation.is_none()
            && !self.unknown_cost.active
            && matches!(self.intelligence.kind, crate::IntelligenceKind::Api { .. })
            && self
                .selected_cost_route()
                .map_or(!self.legacy_native_price(), |r| {
                    r.needs_unknown_choice()
                        || self.legacy_report().is_some()
                        || self.completed_report().is_some()
                })
    }
    fn legacy_native_price(&self) -> bool {
        self.reasoner.authoring_model().is_some_and(|model| {
            nika_runtime::cost_choice::native_catalog_price_known(
                &model,
                crate::reasoner::provider_config(),
            )
        })
    }
    fn selected_cost_route(&self) -> Result<CostRoute, String> {
        if !self.reasoner.supports_admission() {
            return Err("this intelligence cannot enforce the requested unknown-cost HTTP bounds; subscription authorization is separate".into());
        }
        let model = self
            .reasoner
            .authoring_model()
            .ok_or("no selected provider model")?;
        CostRoute::observe(&model, crate::reasoner::provider_config())
    }
    fn cost_candidate(&self, input: &str) -> Result<String, String> {
        // Pin the exact input, pending proposal/round and root-owned source bytes.
        // Includes project defaults; excludes observations written after approval.
        let mut bytes = format!(
            "{input:?}\n{:?}\n{:?}\n{:?}\n{:?}",
            self.snapshot.root.as_os_str().as_encoded_bytes(),
            self.intent,
            self.pending_proposal(),
            self.authoring
        )
        .into_bytes();
        let completed = self.completed_report();
        if self.legacy_report().is_some() || completed.is_some() {
            let state = crate::SessionState::load(&self.snapshot.root)
                .map_err(|e| e.to_string())?
                .ok_or("legacy record disappeared")?;
            let checkpoint_ok = completed.map_or(state.inference_checkpoint.is_none(), |report| {
                self.snapshot.root.canonicalize().ok().is_some_and(|root| {
                    state.inference_checkpoint.as_ref().is_some_and(|raw| {
                        report.matches_checkpoint(raw, root.as_os_str().as_encoded_bytes())
                    })
                })
            });
            if !checkpoint_ok
                || state.pending.is_some()
                || state.inference_observations != self.cost_observations()
                || state
                    .decisions
                    .iter()
                    .chain(&self.intent.decisions)
                    .any(|d| d.starts_with(super::inference::DISPATCH_PREFIX))
            {
                return Err(
                    "legacy record diverged or an operation may still be active; no new invocation"
                        .into(),
                );
            }
            bytes.extend_from_slice(&serde_json::to_vec(&state).map_err(|e| e.to_string())?);
        }
        bytes.extend_from_slice(format!("{:?}", self.authoring_context).as_bytes());
        let snapshot = crate::ProjectSnapshot::observe(&self.snapshot.root);
        if let Some(e) = snapshot.project_error {
            return Err(e);
        }
        if snapshot.truncated || snapshot.walk_truncated {
            return Err(
                "cannot witness an incomplete project inventory for unknown-cost approval".into(),
            );
        }
        let root = nika_fs::OwnedDir::open(&self.snapshot.root).map_err(|e| e.to_string())?;
        let mut paths: Vec<std::path::PathBuf> = snapshot
            .workflows
            .iter()
            .map(|w| w.path.clone().into())
            .collect();
        paths.extend(
            super::named_files(input)
                .into_iter()
                .map(std::path::PathBuf::from),
        );
        paths.sort();
        paths.dedup();
        for path in paths {
            if path.is_absolute()
                || path
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                return Err("cannot witness a source outside this project".into());
            }
            bytes.extend_from_slice(path.as_os_str().as_encoded_bytes());
            match root.open_relative(&path) {
                Ok(file) => {
                    let mut content = Vec::new();
                    file.take(1_048_577)
                        .read_to_end(&mut content)
                        .map_err(|e| e.to_string())?;
                    if content.len() > 1_048_576 {
                        return Err("source too large for a bounded cost review".into());
                    }
                    bytes.extend_from_slice(crate::change::Witness::of(&content).0.as_bytes());
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    bytes.extend_from_slice(b"absent");
                }
                Err(e) => return Err(e.to_string()),
            }
        }
        // Discovery may find an ancestor project file, whose contents/default
        // must remain identical even though it is outside the writable root.
        bytes.extend_from_slice(
            format!("{:?}:{:?}", snapshot.project_file, snapshot.ceiling).as_bytes(),
        );
        Ok(crate::change::Witness::of(&bytes).0)
    }
    /// Stage before `admit_money` changes guards or any classifier/compiler runs.
    pub(super) fn maybe_review_unknown(&mut self, input: &str) -> Result<(), TurnOutcome> {
        let first = input
            .trim()
            .split(|c: char| c.is_whitespace() || c == ',' || c == ':')
            .next()
            .unwrap_or("")
            .to_lowercase();
        if super::authoring::is_run_verb(&first) {
            return Ok(());
        }
        if self.unknown_cost.active
            || !matches!(self.intelligence.kind, crate::IntelligenceKind::Api { .. })
        {
            return Ok(());
        }
        // Keep the native deterministic ladder available without cost consent.
        // This is the SAME Compiler reader, never a parallel language classifier.
        if !self.unknown_cost.in_consent
            && self.authoring.is_none()
            && !crate::authoring::is_greeting(input)
        {
            // The line's own monetary directives, read as its round reads them once admitted
            // (R4 A6); a malformed ceiling is the money gate's to refuse, precisely, next. An
            // explicit zero forbids every call on any route: no review is staged for it, and its
            // deterministic reading, question or refusal with the reader's reasons follows.
            let Ok(found) = super::money_parse::directives(input) else {
                return Ok(());
            };
            if found.money.amount == Some(0.0) {
                return Ok(());
            }
            let mut round = crate::authoring::AuthoringRound::new(input);
            round.money = found.found.into_iter().map(|d| d.span).collect();
            if let Ok(out) = crate::authoring::compile_deterministic(&round.request())
                && matches!(
                    crate::authoring::Reading::of(out),
                    crate::authoring::Reading::Ready(_)
                        | crate::authoring::Reading::Questions(_)
                        | crate::authoring::Reading::Refused(_)
                )
            {
                return Ok(());
            }
        }
        if (self.authoring.as_ref()).is_some_and(|r| {
            let local = input.trim_end().ends_with('?');
            r.current()
                .map_or(local, |q| super::answer::owes_no_review(q, input))
        }) {
            // A line bound with no reading, or a question, owes no review; any other line waits for
            // the chosen seat's one-time review; restored authority never answers it (F2).
            return Ok(());
        }
        let route = match self.selected_cost_route() {
            Ok(route) => route,
            Err(_) if self.legacy_native_price() => return Ok(()),
            Err(why) => return Err(cost_refusal(why)),
        };
        let legacy = self.legacy_report();
        if !route.needs_unknown_choice() && legacy.is_none() && self.completed_report().is_none() {
            return Ok(());
        }
        Err(self
            .stage_cost_review(input, route)
            .unwrap_or_else(|out| out))
    }
    fn stage_cost_review(
        &mut self,
        input: &str,
        route: CostRoute,
    ) -> Result<TurnOutcome, TurnOutcome> {
        let result = (|| {
            let legacy = self.legacy_report();
            if self.money.reconfirm && legacy.is_none() && self.completed_report().is_none() {
                return Err(self.restored_refusal());
            }
            if self.money.gate.is_some() {
                return Err("prior monetary exposure/gate must be reconciled; old approval cannot be replayed".into());
            }
            if let Some(account) = &self.money.account {
                let receipt = account.snapshot().map_err(|e| e.to_string())?;
                if receipt.state == nika_providers::AdmissionState::Uncertain
                    || receipt.held_unknown.nano_usd > 0
                {
                    return Err("a previous dispatch may have been billed; automatic retry and a replacement allowance are blocked".into());
                }
            }
            let parsed = super::money_parse::directives(input)?.money;
            if parsed.amount == Some(0.0)
                || self
                    .money
                    .inference_guard
                    .as_ref()
                    .is_some_and(|d| d.effective_usd == Some(0.0))
            {
                return Err(
                    "the explicit zero constraint forbids this call; no request was sent".into(),
                );
            }
            let current = crate::ProjectSnapshot::observe(&self.snapshot.root);
            if let Some(e) = &current.project_error {
                return Err(e.clone());
            }
            let candidate = self.cost_candidate(input)?;
            let invocation = nika_types::id::CorrelationId::generate().to_string();
            let reserved = recovery_requests(self.authoring_context.recovery);
            let strategy = self.authoring_context.strategy();
            let worst = worst_case(strategy, 1, AUTHORING_REPAIRS, false);
            let review = CostReview::new(
                candidate,
                invocation,
                route,
                self.unknown_cost.host.clone(),
                monetary_default(parsed.amount.or(Some(super::DEFAULT_CEILING_USD)))?,
                monetary_default(current.ceiling)?,
            )?
            .for_session()
            .with_recovery_requests(reserved, 1 + worst + reserved);
            let review = match legacy {
                Some(report) => review.after_legacy(report),
                None => review,
            };
            let review = match self.completed_report() {
                Some(report) => review.after_completed(report),
                None => review,
            };
            let question = review.question();
            self.unknown_cost.pending = Some(PendingCost {
                review,
                input: input.into(),
                consent: self.unknown_cost.in_consent,
            });
            Ok(TurnOutcome::Question {
                key: "unknown_cost".into(),
                question,
            })
        })();
        result.map_err(cost_refusal)
    }
    pub(super) fn cost_answer(&mut self, answer: &str) -> TurnOutcome {
        if matches!(answer.trim(), "/details" | "/why" | "/status") {
            return TurnOutcome::Facts(self.cost_choice_details().unwrap_or_default());
        }
        // The help card beside the review: local, the review keeps waiting.
        if answer.trim() == "/help" {
            return TurnOutcome::Help(super::HELP.to_owned());
        }
        let Some(pending) = self.unknown_cost.pending.take() else {
            return cost_refusal("no cost review waits".into());
        };
        // One grammar with the Run's cost decision (EN/FR): an unknown line is asked again with the
        // same review — it never approves, never spends and never silently cancels.
        match super::decision_answer(answer) {
            super::DecisionAnswer::Approve => {}
            super::DecisionAnswer::Decline => return self.declined_review(),
            super::DecisionAnswer::Details => {
                self.unknown_cost.pending = Some(pending);
                return TurnOutcome::Facts(self.cost_choice_details().unwrap_or_default());
            }
            super::DecisionAnswer::Unknown => {
                let question = format!(
                    "« {} » is not a yes or a no · nothing was sent · `yes`/`oui` continues once · `no`/`non` cancels · `details` shows the evidence\nThe request is unchanged: « {} ». To change it, cancel this review and describe the new request.\n{}",
                    answer.trim(),
                    pending.input,
                    pending.review.question()
                );
                self.unknown_cost.pending = Some(pending);
                return TurnOutcome::Question {
                    key: "unknown_cost".into(),
                    question,
                };
            }
        }
        let result = (|| {
            let candidate = self.cost_candidate(&pending.input)?;
            let route = self.selected_cost_route()?;
            pending.review.confirm(&candidate, &route)
        })();
        let account = match result {
            Ok(a) => a,
            Err(e) => return cost_refusal(e),
        };
        if let Some(old) = self.money.account.take() {
            if let Ok(kept) = old.snapshot().map(|r| r.durable_observation()) {
                self.unknown_cost.observations.push(kept);
            }
            let _ = old.close("superseded by a freshly reviewed invocation");
        }
        self.money.account = Some(account.clone());
        self.money.inference_guard = None;
        self.money.admission_note = None;
        self.unknown_cost.active = true;
        self.retain_money_guard();
        // A failed durable boundary must refuse BEFORE entering transport. It
        // already says a request may be in flight: the process can leave while
        // one is, and only the settlement's write below removes that line.
        if let Err(error) = self.save_dispatch_boundary() {
            self.unknown_cost.active = false;
            let _ = account.close("could not persist decision boundary");
            return cost_refusal(error);
        }
        let out = if pending.consent {
            self.consent_unrecorded(&pending.input)
        } else {
            self.turn_unrecorded(&pending.input)
        };
        self.unknown_cost.active = false;
        // Closing does not erase Uncertain or any possibly-billed attempt.
        let _ = account.close("one-time Session invocation ended; fresh review required");
        if self.money.reconfirm {
            match account.snapshot() {
                Ok(receipt) => self
                    .unknown_cost
                    .observations
                    .push(receipt.durable_observation()),
                Err(error) => return cost_refusal(error.to_string()),
            }
            self.money.account = None;
        }
        match self.save_cost_state() {
            Ok(()) => out,
            Err(error) => cost_refusal(format!(
                "request may have been billed; observation persistence failed: {error}"
            )),
        }
    }

    /// A declined review cancels its own call and nothing else: what waited before the line that
    /// asked it — a proposal, a question, the proposal a revision set aside — waits again,
    /// unchanged, its identity intact. Only its own answer (`no` · `cancel`) discards it.
    fn declined_review(&mut self) -> TurnOutcome {
        const CANCELLED: &str = "Unknown-cost request cancelled; nothing sent.";
        if let Some(set) = &self.pending {
            return TurnOutcome::Held {
                id: self.proposal_id(set),
                preview: format!(
                    "{CANCELLED} The proposal is unchanged.\n(the proposal still waits · `yes` applies it · `no` discards it)"
                ),
            };
        }
        if let Some(round) = &self.authoring
            && let Some(question) = round.current()
        {
            return TurnOutcome::Question {
                key: question.key.clone(),
                question: format!(
                    "{CANCELLED} The question is unchanged.\n{}",
                    super::authoring::question_text(question, &round.reasons)
                ),
            };
        }
        TurnOutcome::Facts(format!(
            "{CANCELLED} Describe the next request to review it afresh."
        ))
    }
}
fn cost_refusal(why: String) -> TurnOutcome {
    TurnOutcome::Refusal(Refusal::new(RefusalClass::NotAllowed, why))
}
