// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The one Session account and every bounded consumer. Money is never consent.
use super::SessionRuntime;
use crate::authoring::{AuthoringError, AuthoringRound, AuthoringSeat};
use crate::money::{InferenceEnforcement, MonetaryDecision};
use crate::reasoner::{ReasonError, Reply};
use nika_providers::{InferenceAdmission, InferenceReceipt};
use nika_types::cost::Cost;

pub(super) const GATE_MONEY_PREFIX: &str = "paused gate monetary constraint: ";
pub(super) const RESTORED_EXPOSURE: &str = "restored inference exposure is unknown; no new catalog allowance can be inferred; billed cost unknown";

pub(super) fn gate_money_marker(gate: &crate::GateId) -> String {
    // Hash each exact identity component separately, not GateId's human Display.
    // No lossy path conversion or ambiguous delimiter can release another hold.
    let trace = crate::change::Witness::of(gate.trace.as_os_str().as_encoded_bytes());
    let task = crate::change::Witness::of(gate.task.as_bytes());
    format!("{GATE_MONEY_PREFIX}{}:{}", trace.0, task.0)
}

pub(super) fn is_money_marker(decision: &str) -> bool {
    decision == RECONFIRM || decision.starts_with(GATE_MONEY_PREFIX)
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
                "{} · {} historical cost observation(s), without authority",
                RESTORED_EXPOSURE,
                self.unknown_cost.observations.len()
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
        let account = format!("Session inference (separate from proposal/Run): {account}");
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
        match &self.money.account {
            Some(a) if label => self.reasoner.reason_label_with_admission(prompt, a),
            Some(a) => self.reasoner.reason_with_admission(prompt, a),
            None if label => self.reasoner.reason_label(prompt),
            None => self.reasoner.reason(prompt),
        }
    }
    pub(super) fn compile_round(
        &self,
        round: &AuthoringRound,
        seat: &AuthoringSeat,
    ) -> Result<nika_onboard::compile::CompileOutcome, AuthoringError> {
        let out = match &self.money.account {
            Some(a) => round.compile_with_admission(seat, &self.authoring_context, a),
            None => round.compile(seat, &self.authoring_context),
        }?;
        self.check_inference_outcome()?;
        Ok(out)
    }
    pub(super) fn check_inference_outcome(&self) -> Result<(), AuthoringError> {
        if let Some(a) = &self.money.account {
            let receipt = a
                .snapshot()
                .map_err(|e| AuthoringError::Seat(e.to_string()))?;
            if let Some(reason) = receipt.refusal {
                return Err(AuthoringError::Seat(format!(
                    "catalog admission: {reason}; billed cost unknown"
                )));
            }
        }
        Ok(())
    }
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
mod tests {
    use super::allowance;
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
