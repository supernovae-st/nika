// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The one Session account, the no-budget observation and every bounded
//! consumer. Money is never consent; an observation is never an allowance.
use super::SessionRuntime;
use crate::authoring::{AuthoringError, AuthoringRound, AuthoringSeat};
use crate::money::{InferenceEnforcement, MonetaryDecision};
use crate::reasoner::{ReasonError, Reply};
use nika_providers::{InferenceAdmission, InferenceReceipt};
use nika_types::cost::Cost;

pub(super) const GATE_MONEY_PREFIX: &str = "paused gate monetary constraint: ";
pub(super) const RESTORED_EXPOSURE: &str = "restored inference exposure is unknown; no new catalog allowance can be inferred; billed cost unknown";
/// A paid dispatch that may be in flight. The line is written into the record
/// BEFORE any request of it can enter transport, and every write after its
/// settlement omits it: a record that still carries it was left while a
/// request may have been sent and billed. It is never authority.
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
        || decision.starts_with(GATE_MONEY_PREFIX)
        || decision.starts_with(DISPATCH_PREFIX)
        || decision.starts_with(OBSERVED_PREFIX)
}

pub(super) const RECONFIRM: &str =
    "monetary constraint recorded; restart requires explicit ceiling reconfirmation";

impl SessionRuntime {
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
        let account = match self.inference_receipt() {
            Ok(Some(r)) if r.unknown_cost.is_some() => format!(
                "explicit unknown cost · known USD subtotal {} · unknown calls {} · {:?} · invoice unknown · fresh review required for another invocation",
                r.estimated, r.unknown_calls, r.state
            ),
            Ok(Some(r)) => {
                let provenance = r.attempts.last().map_or_else(String::new, |a| {
                    format!(
                        " · {}/{} at {} · tariff {} ({})",
                        a.tariff.provider, a.model, a.endpoint, a.tariff.source, a.tariff.as_of
                    )
                });
                let refusal = r.refusal.map_or_else(String::new, |s| format!(" · {s}"));
                format!(
                    "catalog admission: allowance {} · estimated {} · reserved {} · charge-unknown {} · available {} · {:?} · billed cost unknown{provenance}{refusal}",
                    r.limit, r.estimated, r.active, r.held_unknown, r.available, r.state
                )
            }
            Ok(None) if self.money.reconfirm => format!(
                "{} · {} historical cost observation(s), without authority{}",
                RESTORED_EXPOSURE,
                self.unknown_cost.observations.len(),
                self.interrupted_note()
            ),
            Ok(None) => self
                .money
                .admission_note
                .clone()
                .or_else(|| {
                    self.money
                        .inference_guard
                        .as_ref()
                        .and_then(|d| d.refusal.as_ref())
                        .map(|reason| {
                            format!("earlier Session monetary admission refused: {reason}")
                        })
                })
                .unwrap_or_else(|| {
                    if self
                        .money
                        .inference_guard
                        .as_ref()
                        .is_some_and(|d| d.effective_usd == Some(0.0))
                    {
                        "catalog allowance is zero; no paid inference admitted; billed cost unknown"
                    } else {
                        "no qualified catalog admission account; billed cost unknown"
                    }
                    .into()
                }),
            Err(e) => format!("catalog admission unavailable: {e}; no paid call admitted"),
        };
        let observed = self
            .observed_line()
            .map_or_else(String::new, |line| format!(" · {line}"));
        let decision = decision_line(&self.cost_observations())
            .map_or_else(String::new, |line| format!(" · {line}"));
        let account = format!(
            "Session inference (separate from proposal/Run): {account}{observed}{decision}"
        );
        if self.money.gate.is_some() {
            format!(
                "confirm-gate monetary amendment held; no paid inference admitted; paused Run unchanged; answer yes or no separately\n{account}"
            )
        } else {
            account
        }
    }
    pub(super) fn configure_admission(&mut self, decision: &mut MonetaryDecision) {
        // A gate amendment changes neither the paused run nor its authority.
        if self.money.gate.is_some() {
            return;
        }
        let Some(amount) = decision.effective_usd else {
            return;
        };
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
        // A ceiling is not evidence of prior settled/held exposure. The durable
        // records carry a restriction, not a restorable admission ledger.
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
        let (provider, name) = model
            .split_once('/')
            .ok_or("model must be provider-qualified")?;
        let registry =
            nika_providers::ProviderRegistry::without_http(crate::reasoner::provider_config());
        let endpoint = registry
            .effective_base_url(provider)
            .ok_or("unknown provider endpoint")?;
        InferenceAdmission::qualify(provider, name, endpoint).map_err(|e| e.to_string())?;
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
        let model = if self.reasoner.supports_admission() {
            self.reasoner.authoring_model()
        } else {
            None
        };
        let (account, entered) = self
            .enter_dispatch(model.as_deref())
            .map_err(|e| ReasonError::Provider(format!("{UNRECORDED}: {e}")))?;
        let reply = match &account {
            Some(a) if label => self.reasoner.reason_label_with_admission(prompt, a),
            Some(a) => self.reasoner.reason_with_admission(prompt, a),
            None if label => self.reasoner.reason_label(prompt),
            None => self.reasoner.reason(prompt),
        };
        self.leave_paid_dispatch(entered);
        reply
    }
    pub(super) fn compile_round(
        &self,
        round: &AuthoringRound,
        seat: &AuthoringSeat,
    ) -> Result<nika_onboard::compile::CompileOutcome, AuthoringError> {
        self.seated(seat, |account| match account {
            Some(a) => round.compile_with_admission(seat, &self.authoring_context, a),
            None => round.compile(seat, &self.authoring_context),
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
        self.leave_paid_dispatch(entered);
        let out = out?;
        if let Some(a) = &account {
            let receipt = a
                .snapshot()
                .map_err(|e| AuthoringError::Seat(e.to_string()))?;
            if let Some(reason) = receipt.refusal {
                let scope = if receipt.unbudgeted {
                    "no-budget observation"
                } else {
                    "catalog admission"
                };
                return Err(AuthoringError::Seat(format!(
                    "{scope}: {reason}; billed cost unknown"
                )));
            }
        }
        Ok(out)
    }
    /// The in-flight line for the live account; `None` without an account.
    pub(super) fn dispatch_marker(&self) -> Option<String> {
        let account = self.money.account.as_ref()?;
        let (route, bound) = match account.snapshot() {
            Ok(receipt) => match &receipt.unknown_cost {
                Some(choice) => (
                    format!(
                        "{}/{} at {}",
                        choice.provider(),
                        choice.model(),
                        choice.endpoint()
                    ),
                    serde_json::to_value(choice)
                        .ok()
                        .and_then(|v| v["max_requests"].as_u64())
                        .map_or_else(
                            || "its bounded requests".to_owned(),
                            |n| format!("at most {n} request(s)"),
                        ),
                ),
                None => (
                    self.reasoner
                        .authoring_model()
                        .unwrap_or_else(|| "the selected route".to_owned()),
                    format!("catalog allowance {}", receipt.limit),
                ),
            },
            Err(e) => ("an unreadable account".to_owned(), e.to_string()),
        };
        Some(format!(
            "{DISPATCH_PREFIX}{route} · {bound} · recorded {} before transport; no settlement followed, so its request(s) may have been sent and billed · usage and cost unknown",
            crate::intelligence::now_rfc3339()
        ))
    }
    /// Before one dispatch on `model`: the account it rides, its in-flight line
    /// persisted first (`true`: a line was written). The Session allowance or
    /// explicit unknown-cost scope governs whatever it carries; the one-time
    /// invocation wrote its own line before cognition. Without either, a
    /// qualified priced route is observed on the no-budget account (no
    /// allowance, no cap) and any other route keeps its unobserved path.
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
        let live = |account: &InferenceAdmission| {
            usize::from(account.snapshot().map_or(true, |receipt| {
                receipt.state == nika_providers::AdmissionState::Uncertain
            }))
        };
        let kept = self
            .unknown_cost
            .observations
            .iter()
            .filter(|o| o["state"] == "Uncertain")
            .count();
        // A decision-seat request left without a response may have been billed as well.
        let decisions = self.authoring_context.decision().map_or(0, |setup| {
            setup
                .observations()
                .iter()
                .filter(|o| o["state"] == "Uncertain")
                .count()
        });
        self.money.account.as_ref().map_or(0, live) + live(&self.money.observed) + kept + decisions
    }
    /// New work starts on a clean no-budget account; a continuation (an
    /// answer, a revision at consent, a repair) stays on the one its work
    /// began with. An account frozen by a possibly billed request, or left
    /// with a refusal, is kept as history when it observed anything, and is
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
            self.unknown_cost.observations.push(receipt.observation());
        }
        self.money.observed = InferenceAdmission::unbudgeted();
    }
    /// Priced calls made without a Session budget: observed, never admitted.
    fn observed_line(&self) -> Option<String> {
        let observations = self.cost_observations();
        let observed: Vec<_> = observations
            .iter()
            .filter(|o| o["unbudgeted"] == true && !is_decision(o))
            .collect();
        let sent: usize = observed
            .iter()
            .filter_map(|o| o["attempts"].as_array())
            .map(|attempts| attempts.iter().filter(|a| a["sent"] == true).count())
            .sum();
        let interrupted = self
            .intent
            .decisions
            .iter()
            .filter(|d| d.starts_with(OBSERVED_PREFIX))
            .count();
        if sent == 0 && interrupted == 0 {
            return None;
        }
        let estimate = observed
            .iter()
            .try_fold(0i128, |total, o| {
                o["known_subtotal_nano_usd"]
                    .as_str()?
                    .parse::<i128>()
                    .ok()?
                    .checked_add(total)
            })
            .map_or_else(|| "unreadable".to_owned(), |n| Cost::new(n).to_string());
        let unsettled: u64 = observed
            .iter()
            .filter_map(|o| o["unknown_calls"].as_u64())
            .sum();
        let interrupted = match interrupted {
            0 => String::new(),
            n => format!(
                " · {n} no-budget dispatch(es) left without a recorded settlement may have been billed; usage and cost unknown"
            ),
        };
        Some(format!(
            "no-budget observation (outside any allowance or cap): {sent} priced call(s) sent · catalog estimate {estimate} of complete usage · {unsettled} without usable settlement · invoice unknown{interrupted}"
        ))
    }
    /// The restart refusal: what stays unknown, what was not done, the way on.
    pub(super) fn restored_refusal(&self) -> String {
        format!(
            "{RESTORED_EXPOSURE}{} · {RESTORED_WAY}",
            self.interrupted_note()
        )
    }
    fn interrupted_note(&self) -> String {
        match self
            .intent
            .decisions
            .iter()
            .filter(|d| d.starts_with(DISPATCH_PREFIX))
            .count()
        {
            0 => String::new(),
            n => format!(
                " · {n} paid dispatch(es) left without a recorded settlement may have been billed; usage and cost unknown"
            ),
        }
    }
}

/// Whether numeric catalog admission qualifies this exact route: the one kind
/// of no-budget call the bounded seam can observe without any authority.
fn priced_route(model: &str) -> bool {
    nika_runtime::cost_choice::CostRoute::observe(model, crate::reasoner::provider_config())
        .is_ok_and(|route| !route.needs_unknown_choice())
}

fn observed_marker(model: &str, decision: Option<&str>) -> String {
    let decision = decision.map_or_else(String::new, |seat| {
        format!(
            " · the operator-selected decision seat {seat} may also have been called (cost unknown)"
        )
    });
    format!(
        "{OBSERVED_PREFIX}{model}{decision} · no Session budget: observed, no allowance or cap · recorded {} before transport; no settlement followed, so its request(s) may have been sent and billed · usage and cost unknown",
        crate::intelligence::now_rfc3339()
    )
}

/// Whether one persisted observation is the operator-selected decision seat's (never part of
/// the no-budget priced subtotal: its cost is unknown).
fn is_decision(observation: &serde_json::Value) -> bool {
    observation["schema"] == crate::authoring::DECISION_SCHEMA
}

/// The decision seat's line: what it was asked, sent and refused; its cost unknown, never zero.
fn decision_line(observations: &[serde_json::Value]) -> Option<String> {
    let seats: Vec<&serde_json::Value> = observations.iter().filter(|o| is_decision(o)).collect();
    if seats.is_empty() {
        return None;
    }
    let attempts: Vec<&serde_json::Value> = seats
        .iter()
        .filter_map(|o| o["attempts"].as_array())
        .flatten()
        .collect();
    let count = |outcome: &str| attempts.iter().filter(|a| a["outcome"] == outcome).count();
    let sent = attempts.iter().filter(|a| a["sent"] == true).count();
    let unresolved = count("in_flight") + count("transport_error");
    let refused = count("refused") + count("capped");
    let mut names: Vec<&str> = seats.iter().filter_map(|o| o["seat"].as_str()).collect();
    names.dedup();
    let usage: u64 = attempts
        .iter()
        .filter_map(|a| a["usage"]["input_tokens"].as_u64())
        .chain(
            attempts
                .iter()
                .filter_map(|a| a["usage"]["output_tokens"].as_u64()),
        )
        .sum();
    Some(format!(
        "decision seat {} (operator-selected, outside any allowance or cap): {sent} call(s) sent · {} answered · {unresolved} without a response · {refused} need(s) refused before sending · {usage} token(s) reported · cost unknown (no catalog tariff), never zero; not in the no-budget subtotal · invoice unknown",
        names.join(", "),
        count("chosen") + count("none") + count("outside_options")
    ))
}

/// Convert the already validated binary number downward without another
/// monetary grammar or an upward-rounded floating multiplication.
fn allowance(amount: f64) -> Result<Cost, String> {
    if !amount.is_finite() || amount < 0.0 {
        return Err("invalid catalog allowance".into());
    }
    let bits = amount.to_bits();
    let raw_exp = i32::try_from((bits >> 52) & 0x7ff).map_err(|e| e.to_string())?;
    let fraction = bits & ((1u64 << 52) - 1);
    let mantissa = if raw_exp == 0 {
        fraction
    } else {
        fraction | (1u64 << 52)
    };
    let exponent = if raw_exp == 0 { -1074 } else { raw_exp - 1075 };
    let scaled = u128::from(mantissa) * 1_000_000_000;
    let nanos = if exponent < 0 {
        let shift = u32::try_from(-exponent).map_err(|e| e.to_string())?;
        scaled.checked_shr(shift).unwrap_or(0)
    } else {
        let shift = u32::try_from(exponent).map_err(|e| e.to_string())?;
        1u128
            .checked_shl(shift)
            .and_then(|n| scaled.checked_mul(n))
            .ok_or("catalog allowance overflow")?
    };
    i128::try_from(nanos)
        .map(Cost::new)
        .map_err(|_| "catalog allowance overflow".into())
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::{allowance, decision_line, observed_marker};
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
