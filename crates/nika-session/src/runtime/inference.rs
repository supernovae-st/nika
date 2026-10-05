// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The one Session account, the no-budget observation and every bounded
//! consumer. Money is never consent; an observation is never an allowance.
use super::SessionRuntime;
use crate::authoring::{AuthoringError, AuthoringRound, AuthoringSeat};
use crate::money::{InferenceEnforcement, MonetaryDecision};
use crate::reasoner::{ReasonError, Reply};
use nika_providers::admission::allowance;
use nika_providers::authoring::preparation::PreparationCosts;
use nika_providers::{InferenceAdmission, InferenceReceipt};
use nika_types::cost::Cost;

pub(super) const GATE_MONEY_PREFIX: &str = "paused gate monetary constraint: ";
pub(super) const RESTORED_EXPOSURE: &str = "restored inference exposure is unknown; no new catalog allowance can be inferred; billed cost unknown";
/// A paid dispatch that may be in flight. The line is written into the record BEFORE any request of
/// it can enter transport, and every write after its settlement omits it: a record that still
/// carries it was left while a request may have been sent and billed. It is never authority.
pub(super) const DISPATCH_PREFIX: &str = "paid inference may have been sent: ";
/// The same line for a priced dispatch made without any Session budget. It
/// names exposure only: restoring it demands no ceiling or reconfirmation.
pub(super) const OBSERVED_PREFIX: &str =
    "priced inference without a Session budget may have been sent: ";
/// What a restored Session offers instead of replaying or re-admitting.
pub(super) const RESTORED_WAY: &str = "nothing was replayed · a saved workflow still runs when restated with an explicit Run ceiling, e.g. « run <workflow>.nika budget 0.50 USD » (the Run keeps its own separate cost admission) · no ceiling covers an unknown earlier charge, so Session inference stays blocked here; deterministic work remains available";
const UNRECORDED: &str = "the paid-dispatch boundary was not recorded; nothing was sent";

pub(super) fn gate_money_marker(gate: &crate::GateId) -> String {
    // Hash each exact identity component separately, not GateId's human Display.
    // No lossy path conversion or ambiguous delimiter can release another hold.
    let trace = crate::change::Witness::of(gate.trace.as_os_str().as_encoded_bytes());
    let task = crate::change::Witness::of(gate.task.as_bytes());
    format!("{GATE_MONEY_PREFIX}{}:{}", trace.0, task.0)
}

pub(super) fn is_money_marker(decision: &str) -> bool {
    decision == RECONFIRM
        || decision == super::connection_money::SUBSCRIPTION_HOLD
        || decision.starts_with(GATE_MONEY_PREFIX)
        || decision.starts_with(DISPATCH_PREFIX)
        || decision.starts_with(OBSERVED_PREFIX)
}

pub(super) const RECONFIRM: &str =
    "monetary constraint recorded; restart requires explicit ceiling reconfirmation";

impl SessionRuntime {
    /// Interactive preparation observes costs without a monetary gate; Run keeps its own policy.
    /// Existing accounts and uncertain exposure remain unchanged and are never reused as credit.
    pub fn enable_continuous_preparation(&mut self) {
        self.money.preparation.get_or_insert_with(Default::default);
    }
    /// Get a fresh Stop token before moving this conversation to its preparation worker.
    /// The completed worker returns this Session and its journal; Run is outside this token.
    pub fn begin_preparation_turn(&mut self) -> nika_types::cancel::CancelCtx {
        self.preparation_before = self.preparation_snapshot();
        self.money
            .preparation
            .get_or_insert_with(Default::default)
            .begin_turn()
    }

    pub(super) fn retain_money_guard(&mut self) {
        if !self.intent.decisions.iter().any(|s| s == RECONFIRM) {
            self.intent.decisions.push(RECONFIRM.into());
        }
    }
    /// Actual aggregate admission observations; estimated cost is not a bill.
    /// # Errors
    /// An unavailable account reports failure, never a fabricated zero.
    pub fn inference_receipt(&self) -> Result<Option<InferenceReceipt>, ReasonError> {
        self.money
            .account
            .as_ref()
            .map(|a| {
                a.snapshot()
                    .map_err(|e| ReasonError::Provider(e.to_string()))
            })
            .transpose()
    }
    pub(super) fn inference_line(&self) -> String {
        if self.money.preparation.is_some() {
            return PreparationCosts::summary(&self.cost_observations());
        }
        let account = nika_providers::admission::account_status(
            self.inference_receipt().map_err(|error| error.to_string()),
            self.money.reconfirm,
            self.unknown_cost.observations.len(),
            &self.interrupted_note(),
            self.money.admission_note.as_deref(),
            self.money
                .inference_guard
                .as_ref()
                .and_then(|d| d.refusal.as_deref()),
            self.money
                .inference_guard
                .as_ref()
                .is_some_and(|d| d.effective_usd == Some(0.0)),
        );
        let details = nika_providers::admission::observation_details(
            &self.cost_observations(),
            &self.unknown_cost.observations,
            crate::authoring::DECISION_SCHEMA,
        );
        nika_providers::admission::inference_summary(
            &account,
            self.observed_line().as_deref(),
            &details,
            self.subscription(),
            self.money.gate.is_some(),
        )
    }
    pub(super) fn configure_admission(&mut self, decision: &mut MonetaryDecision) {
        // A gate amendment changes neither the paused run nor its authority.
        if self.money.gate.is_some() || self.subscription() {
            return;
        }
        let Some(amount) = decision.effective_usd else {
            return;
        };
        if self.money.reconfirm && decision.explicit_amount.is_none() {
            return;
        }
        if amount == 0.0 {
            if let Some(a) = &self.money.account {
                let _ = a.amend(Cost::zero());
            }
            return;
        }
        if decision.explicit_amount.is_none() && self.money.account.is_none() {
            return;
        }
        let result = self.configure_account(amount);
        match result {
            Ok(()) => {
                decision.inference = InferenceEnforcement::CatalogAdmission;
                self.money.admission_note = None;
                self.money.reconfirm = false;
                self.activity(&crate::activity::Activity::now(crate::activity::Phase::Understanding,
                    "catalog admission: token estimates at pinned prices; provider invoice unknown; Run is separate"));
            }
            Err(reason) => {
                decision.inference = InferenceEnforcement::CallsBlocked;
                self.money.admission_note = Some(reason);
            }
        }
    }
    fn configure_account(&mut self, amount: f64) -> Result<(), String> {
        // A fresh total amends only a complete ledger; old observations stay restrictions.
        if self.money.reconfirm && self.money.account.is_none() {
            return Err(RESTORED_EXPOSURE.into());
        }
        if !self.reasoner.supports_admission() {
            return Err(
                "selected intelligence has no catalog admission seam; cost remains unknown".into(),
            );
        }
        let model = self
            .reasoner
            .authoring_model()
            .ok_or("selected intelligence has no qualified authoring model")?;
        InferenceAdmission::qualify_selected(&model, crate::reasoner::provider_config())?;
        let limit = allowance(amount)?;
        if let Some(a) = &self.money.account {
            a.amend(limit).map_err(|e| e.to_string())?;
        } else {
            self.money.account = Some(InferenceAdmission::new(limit).map_err(|e| e.to_string())?);
            self.retain_money_guard();
        }
        Ok(())
    }
    pub(super) fn reason_with_money(
        &mut self,
        prompt: &str,
        label: bool,
    ) -> Result<Reply, ReasonError> {
        if self.money_blocks_cognition() {
            return Err(ReasonError::Provider(self.inference_line()));
        }
        // A named level the session cannot ask refuses the turn before any record or byte (R4 B16).
        let effort = (self.authoring_context.reasoning_asked())
            .map_err(|why| ReasonError::Provider(format!("{why} · nothing was sent")))?;
        if !self.reads_answers() {
            return Err(ReasonError::NoIntelligence);
        }
        let model = self
            .reasoner
            .supports_admission()
            .then(|| self.reasoner.authoring_model());
        let (account, entered) = self
            .enter_dispatch(model.flatten().as_deref())
            .map_err(|e| ReasonError::Provider(format!("{UNRECORDED}: {e}")))?;
        // The session's explicit effort rides every call it names one for (R4 B16).
        let reply = match (&account, effort) {
            (_, Some(effort)) => {
                (self.reasoner).reason_effort(prompt, label, account.as_ref(), effort)
            }
            (Some(a), None) if label => self.reasoner.reason_label_with_admission(prompt, a),
            (Some(a), None) => self.reasoner.reason_with_admission(prompt, a),
            (None, None) if label => self.reasoner.reason_label(prompt),
            (None, None) => self.reasoner.reason(prompt),
        };
        self.leave_paid_dispatch(entered);
        reply
    }
    pub(super) fn compile_round(
        &mut self,
        round: &AuthoringRound,
        seat: &AuthoringSeat,
    ) -> Result<nika_onboard::compile::CompileOutcome, AuthoringError> {
        self.rehearsals.clear_native();
        let context = self.authoring_context.clone();
        if !seat.has_model() {
            return self.seated(seat, |account| {
                round.compile_rehearsed(seat, &context, account, None)
            });
        }
        let intent = round.effective_intent();
        self.rehearse_dispatch(&intent, |this, host| {
            this.seated(seat, |account| {
                round.compile_rehearsed(seat, &context, account, Some(host))
            })
        })
    }
    /// One authoring dispatch on `seat`, bracketed like every other: its line
    /// before transport, the settled record after it, then the refusal of the
    /// account it rode. Only a provider seat names a route to observe.
    pub(super) fn seated<T>(
        &self,
        seat: &AuthoringSeat,
        compile: impl FnOnce(Option<&InferenceAdmission>) -> Result<T, AuthoringError>,
    ) -> Result<T, AuthoringError> {
        if PreparationCosts::stopped() {
            return Err(AuthoringError::Cancelled);
        }
        let model = match seat {
            AuthoringSeat::Provider { model } => Some(model.as_str()),
            _ => None,
        };
        // An operator-selected decision seat may be consulted inside this compile: the line
        // written before transport names it too, so an interruption never hides its call.
        let decision = self
            .authoring_context
            .decision()
            .filter(|setup| setup.refusal().is_none())
            .map(crate::authoring::DecisionSetup::model);
        let (account, entered) = self
            .enter_dispatch_naming(model, decision)
            .map_err(|e| AuthoringError::Seat(format!("{UNRECORDED}: {e}")))?;
        let out = compile(account.as_ref());
        let out = if PreparationCosts::stopped() {
            Err(AuthoringError::Cancelled)
        } else {
            out
        };
        self.leave_paid_dispatch(entered);
        let out = out?;
        if let Some(a) = &account {
            let receipt = a
                .snapshot()
                .map_err(|e| AuthoringError::Seat(e.to_string()))?;
            if let Some(reason) = receipt.dispatch_refusal() {
                return Err(AuthoringError::Seat(reason));
            }
        }
        Ok(out)
    }
    /// The in-flight line for the live account; `None` without an account.
    pub(super) fn dispatch_marker(&self) -> Option<String> {
        let account = self.money.account.as_ref()?;
        Some(format!(
            "{DISPATCH_PREFIX}{}",
            account.dispatch_note(
                self.reasoner.authoring_model().as_deref(),
                &crate::intelligence::now_rfc3339()
            )
        ))
    }
    /// Before one dispatch on `model`: the account it rides, its in-flight line persisted first
    /// (`true`: a line was written). The Session allowance or explicit unknown-cost scope governs
    /// whatever it carries; the one-time invocation wrote its own line before cognition. Without
    /// either, a qualified priced route is observed on the no-budget account (no allowance, no cap)
    /// and any other route keeps its unobserved path.
    pub(super) fn enter_dispatch(
        &self,
        model: Option<&str>,
    ) -> Result<(Option<InferenceAdmission>, bool), String> {
        self.enter_dispatch_naming(model, None)
    }
    /// [`Self::enter_dispatch`] whose no-budget line also names the operator-selected decision
    /// seat the dispatch may consult (its cost unknown, outside the observation's subtotal).
    pub(super) fn enter_dispatch_naming(
        &self,
        model: Option<&str>,
        decision: Option<&str>,
    ) -> Result<(Option<InferenceAdmission>, bool), String> {
        if self.money.preparation.is_some() {
            self.save_boundary(Some(observed_marker(
                model.unwrap_or("selected subscription"),
                decision,
            )))?;
            return Ok((None, true));
        }
        if self.subscription() && model.is_none() {
            return if self.money_blocks_cognition() {
                Err(self.cognition_blocked())
            } else {
                Ok((None, false))
            };
        }
        if let Some(account) = &self.money.account {
            if self.unknown_cost.active {
                return Ok((Some(account.clone()), false));
            }
            self.save_dispatch_boundary()?;
            return Ok((Some(account.clone()), true));
        }
        let Some(model) = model.filter(|m| priced_route(m)) else {
            return Ok((None, false));
        };
        // A frozen observation refuses before transport: it needs no line.
        let open = self
            .money
            .observed
            .snapshot()
            .map_or(true, |r| r.state == nika_providers::AdmissionState::Open);
        if open {
            self.save_boundary(Some(observed_marker(model, decision)))?;
        }
        Ok((Some(self.money.observed.clone()), open))
    }
    /// After it, the settled observation replaces the line. A failed write
    /// keeps the conservative line in place until the next one.
    pub(super) fn leave_paid_dispatch(&self, entered: bool) {
        if entered {
            let _ = self.save_cost_state();
        }
    }
    /// How many accounts may have been charged without usable settlement:
    /// the live ones and those kept as history. Moving an account into the
    /// history keeps the count; an unreadable account counts as uncertain.
    pub(super) fn uncertain_charges(&self) -> usize {
        PreparationCosts::uncertain_exposure(
            self.money.account.as_ref(),
            &self.money.observed,
            &self.unknown_cost.observations,
            &self
                .authoring_context
                .decision()
                .map_or_else(Vec::new, crate::authoring::DecisionSetup::observations),
            self.money.preparation.as_ref(),
        )
    }
    /// New work starts on a clean no-budget account; a continuation (an answer, a revision at
    /// consent, a repair) stays on the one its work began with. An account frozen by a possibly
    /// billed request, or left with a refusal, is kept as history when it observed anything, and is
    /// never reopened.
    pub(super) fn rotate_observation(&mut self) {
        let receipt = self.money.observed.snapshot();
        if receipt
            .as_ref()
            .is_ok_and(|r| r.state == nika_providers::AdmissionState::Open && r.refusal.is_none())
        {
            return;
        }
        if let Ok(receipt) = receipt
            && !receipt.attempts.is_empty()
        {
            let kept = receipt.durable_observation();
            self.unknown_cost.observations.push(kept);
        }
        self.money.observed = InferenceAdmission::unbudgeted();
    }
    /// Priced calls made without a Session budget: observed, never admitted.
    fn observed_line(&self) -> Option<String> {
        let interrupted = self
            .intent
            .decisions
            .iter()
            .filter(|d| d.starts_with(OBSERVED_PREFIX))
            .count();
        nika_providers::admission::unbudgeted_observation_summary(
            &self.cost_observations(),
            crate::authoring::DECISION_SCHEMA,
            interrupted,
        )
    }
    /// The restart refusal: what stays unknown, what was not done, the way on.
    pub(super) fn restored_refusal(&self) -> String {
        if self.subscription() && !self.subscription_open() {
            return super::connection_money::SUBSCRIPTION_HOLD.into();
        }
        if self.money.account.is_some() {
            return format!(
                "{} · confirm a fresh TOTAL Session ceiling in its own sentence, e.g. Budget: 10 USD. (total, not additional); settled expenses and reservations are retained",
                self.inference_line()
            );
        }
        format!(
            "{RESTORED_EXPOSURE}{} · {RESTORED_WAY}",
            self.interrupted_note()
        )
    }
    fn interrupted_note(&self) -> String {
        nika_providers::admission::interrupted_note(
            self.intent
                .decisions
                .iter()
                .filter(|d| d.starts_with(DISPATCH_PREFIX))
                .count(),
        )
    }
}

/// Whether numeric catalog admission qualifies this exact route: the one kind
/// of no-budget call the bounded seam can observe without any authority.
fn priced_route(model: &str) -> bool {
    nika_runtime::cost_choice::CostRoute::observe(model, crate::reasoner::provider_config())
        .is_ok_and(|route| !route.needs_unknown_choice())
}

fn observed_marker(model: &str, decision: Option<&str>) -> String {
    format!(
        "{OBSERVED_PREFIX}{}",
        nika_providers::admission::unbudgeted_dispatch_note(
            model,
            decision,
            &crate::intelligence::now_rfc3339()
        )
    )
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::{allowance, observed_marker};
    fn decision_line(observations: &[serde_json::Value]) -> Option<String> {
        nika_providers::admission::decision_summary(observations, crate::authoring::DECISION_SCHEMA)
    }
    use serde_json::json;

    #[test]
    fn the_line_before_transport_names_the_decision_seat_it_may_call() {
        let plain = observed_marker("deepseek/deepseek-flash", None);
        assert!(!plain.contains("decision seat"));
        let named = observed_marker("deepseek/deepseek-flash", Some("typesafe/jev-1.13.0"));
        assert!(named.starts_with(super::OBSERVED_PREFIX));
        assert!(named.contains("decision seat typesafe/jev-1.13.0 may also have been called"));
    }

    #[test]
    fn the_decision_line_counts_its_own_calls_and_never_prices_them() {
        let observations = [
            json!({"unbudgeted": true, "attempts": [{"sent": true}]}),
            json!({"schema": crate::authoring::DECISION_SCHEMA, "seat": "typesafe/jev-1.13.0",
                "unbudgeted": true, "attempts": [
                    {"sent": true, "outcome": "chosen", "usage": {"input_tokens": 40, "output_tokens": 2}},
                    {"sent": true, "outcome": "transport_error"},
                    {"sent": false, "outcome": "refused"}]}),
        ];
        let line = decision_line(&observations).expect("a decision seat was needed");
        assert!(line.contains("typesafe/jev-1.13.0"), "{line}");
        assert!(line.contains("2 call(s) sent"), "{line}");
        assert!(line.contains("1 answered"), "{line}");
        assert!(line.contains("1 without a response"), "{line}");
        assert!(line.contains("1 need(s) refused"), "{line}");
        assert!(line.contains("42 token(s) reported"), "{line}");
        assert!(line.contains("cost unknown"), "{line}");
        assert!(decision_line(&observations[..1]).is_none());
    }
    #[test]
    fn conversion_floors_nanos_and_refuses_nonfinite_or_overflow() {
        assert!(matches!(allowance(0.5), Ok(cost) if cost.nano_usd == 500_000_000));
        assert!(matches!(allowance(0.0), Ok(cost) if cost.nano_usd == 0));
        assert!(matches!(allowance(0.5e-9), Ok(cost) if cost.nano_usd == 0));
        assert!(matches!(allowance(f64::from_bits(1)), Ok(cost) if cost.nano_usd == 0));
        for n in [f64::INFINITY, f64::NAN, -1.0, f64::MAX] {
            assert!(allowance(n).is_err());
        }
    }
}
