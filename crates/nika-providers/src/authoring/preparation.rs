// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Interactive preparation observes physical requests; it grants no execution or credit.
//! The existing dispatch journal owns send/return/cancellation evidence. Durable projections
//! use the same route privacy law as Run. This scope never changes a numeric account.
use crate::dispatch_journal::DispatchJournal;
use serde_json::{Value, json};
use std::{cell::RefCell, future::Future, marker::PhantomData, rc::Rc};

const SCHEMA: &str = "nika/preparation-cost-observation@1";
thread_local! {
    static ACTIVE: RefCell<Option<PreparationCosts>> = const { RefCell::new(None) };
}
/// Live, observational costs for one interactive Session. Not an admission handle.
#[derive(Clone, Debug)]
pub struct PreparationCosts {
    id: String,
    journal: DispatchJournal,
    cancel: Option<nika_types::cancel::CancelCtx>,
    started: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl Default for PreparationCosts {
    fn default() -> Self {
        Self {
            id: nika_types::id::CorrelationId::generate().to_string(),
            journal: DispatchJournal::default(),
            cancel: None,
            started: std::sync::Arc::default(),
        }
    }
}
impl PreparationCosts {
    /// Start a new operator-controlled preparation turn. Earlier costs are unchanged.
    #[must_use]
    pub fn begin_turn(&mut self) -> nika_types::cancel::CancelCtx {
        let cancel = nika_types::cancel::CancelCtx::new();
        self.cancel = Some(cancel.clone());
        self.started = std::sync::Arc::default();
        cancel
    }
    /// The token of this preparation turn, including after its worker returns.
    #[must_use]
    pub fn was_stopped(&self) -> bool {
        self.cancel
            .as_ref()
            .is_some_and(nika_types::cancel::CancelCtx::is_cancelled)
    }
    /// Whether Stop was requested for the current scoped preparation, never for a Run.
    #[must_use]
    pub fn stopped() -> bool {
        ACTIVE.with(|slot| {
            slot.borrow()
                .as_ref()
                .and_then(|c| c.cancel.as_ref())
                .is_some_and(nika_types::cancel::CancelCtx::is_cancelled)
        })
    }
    /// Stop affected a polled preparation future, not a local Save or execution action.
    #[must_use]
    pub fn interrupted() -> bool {
        Self::stopped()
            && ACTIVE.with(|slot| {
                slot.borrow()
                    .as_ref()
                    .is_some_and(|c| c.started.load(std::sync::atomic::Ordering::Acquire))
            })
    }
    /// Run preparation until it completes or the operator stops it. None means Stop.
    /// Dropping the future retains sent requests in the existing dispatch journal; a remote
    /// provider may still bill them. This neither cancels a Run nor guarantees a remote refund.
    pub async fn while_active<F: Future>(future: F) -> Option<F::Output> {
        let cancel = ACTIVE.with(|slot| slot.borrow().as_ref().and_then(|c| c.cancel.clone()));
        let Some(cancel) = cancel else {
            return Some(Self::capture(future).await);
        };
        if cancel.is_cancelled() {
            return None;
        }
        ACTIVE.with(|slot| {
            if let Some(costs) = slot.borrow().as_ref() {
                costs
                    .started
                    .store(true, std::sync::atomic::Ordering::Release);
            }
        });
        let result = tokio::select! {
            biased;
            () = async {
                while !cancel.is_cancelled() {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
            } => None,
            result = Self::capture(future) => Some(result),
        };
        result.filter(|_| !cancel.is_cancelled())
    }

    /// Scope the synchronous Session driver. Provider futures capture this journal before IO.
    /// Nested scopes restore their predecessor; the guard cannot move to another thread.
    #[must_use]
    pub fn enter(&self) -> PreparationScope {
        PreparationScope(
            ACTIVE.with(|slot| slot.replace(Some(self.clone()))),
            PhantomData,
        )
    }
    /// Whether the current driver is preparing continuously, rather than enforcing a Run cap.
    #[must_use]
    pub fn active() -> bool {
        ACTIVE.with(|slot| slot.borrow().is_some())
    }
    /// Observe a provider future, including returned failures and dropped requests.
    pub async fn capture<F: Future>(future: F) -> F::Output {
        let active = ACTIVE.with(|slot| slot.borrow().clone());
        match active {
            Some(costs) => costs.journal.capture(future).await,
            None => future.await,
        }
    }
    /// Nonempty, privacy-projected evidence; missing prices remain unknown, never free.
    #[must_use]
    pub fn observation(&self) -> Option<Value> {
        let calls = self.journal.snapshot();
        if calls.is_empty() {
            return None;
        }
        let known = calls
            .iter()
            .filter_map(nika_types::cost::InferenceCall::known_estimate)
            .try_fold(0_i128, |sum, cost| sum.checked_add(cost.nano_usd));
        Some(json!({"schema":SCHEMA, "scope":self.id, "unbudgeted":true,
            "known_subtotal_nano_usd":known.map(|n| n.to_string()),
            "unknown_calls":calls.iter().filter(|c| c.known_estimate().is_none()).count(),
            "calls":crate::route_identity::durable_calls(&calls),
            "billing":"unknown", "state":if self.journal.unfinished() { "Uncertain" } else { "Closed" }, "authority":"observation_only"}))
    }
    /// Each physical request whose charge remains unknown, including every cancelled request.
    /// This is a count, not a flag: another interrupted request creates another uncertainty.
    #[must_use]
    pub fn uncertain_requests(&self) -> usize {
        self.journal
            .snapshot()
            .iter()
            .filter(|call| call.known_estimate().is_none())
            .count()
    }
    /// Historical observations that still carry uncertainty, without inferring any restriction.
    #[must_use]
    pub fn uncertain_observations(observations: &[Value]) -> usize {
        observations
            .iter()
            .filter(|o| o["state"] == "Uncertain" || !o.is_object())
            .count()
    }
    /// Live accounts, historical and companion observations, and unresolved API responses.
    /// A returned preparation response with no price remains in `unknown_calls` and
    /// `uncertain_requests`, but does not imply an unfinished operation. Every unreadable
    /// account remains uncertain; no count grants authority or restricts design.
    #[must_use]
    pub fn uncertain_exposure(
        account: Option<&crate::InferenceAdmission>,
        observed: &crate::InferenceAdmission,
        kept: &[Value],
        decisions: &[Value],
        preparation: Option<&Self>,
    ) -> usize {
        let live = |account: &crate::InferenceAdmission| {
            usize::from(
                account
                    .snapshot()
                    .map_or(true, |r| r.state == crate::AdmissionState::Uncertain),
            )
        };
        account.map_or(0, live)
            + live(observed)
            + Self::uncertain_observations(kept)
            + decisions
                .iter()
                .filter(|o| o["state"] == "Uncertain")
                .count()
            + preparation.map_or(0, |costs| costs.journal.unresolved_responses())
    }
    /// Historical and live scopes are summarized once each, without recreating any account.
    #[must_use]
    pub fn summary(observations: &[Value]) -> String {
        let scopes: Vec<_> = observations
            .iter()
            .filter(|o| o["schema"] == SCHEMA)
            .collect();
        let sent: usize = scopes
            .iter()
            .filter_map(|o| o["calls"].as_array())
            .map(Vec::len)
            .sum();
        let unknown: u64 = scopes
            .iter()
            .filter_map(|o| o["unknown_calls"].as_u64())
            .sum();
        let known = scopes
            .iter()
            .try_fold(0_i128, |sum, o| {
                sum.checked_add(
                    o["known_subtotal_nano_usd"]
                        .as_str()?
                        .parse::<i128>()
                        .ok()?,
                )
            })
            .map_or_else(
                || "unknown".into(),
                |n| nika_types::cost::Cost::new(n).to_string(),
            );
        let mut lines = vec![format!(
            "Preparation: {sent} API requests observed · known estimate {known} · {unknown} unpriced requests · subscription and decision-service invoices unknown · Run has its own budget"
        )];
        lines.extend(observations.iter().filter_map(retained_account));
        if let Some(decisions) =
            crate::admission::decision_summary(observations, "nika/session-decision-seat@2")
        {
            lines.push(decisions);
        }
        let unread = observations.iter().filter(|o| !o.is_object()).count();
        if unread > 0 {
            lines.push(format!(
                "{unread} cost observation(s) unreadable; unknown exposure retained"
            ));
        }
        lines.join("\n")
    }
}
// Numeric accounts stay visible as historical observations, never as today's design gate.
// Read the canonical serializer's evidence, without reconstructing or amending an account.
fn retained_account(observation: &Value) -> Option<String> {
    if !matches!(
        observation["schema"].as_str(),
        Some("nika/inference-cost-observation@1" | "nika/inference-cost-observation@2")
    ) {
        return None;
    }
    if !crate::admission::observation_readable(observation)
        || crate::admission::observation_consistent(observation).is_err()
    {
        return Some("Historical accounting unreadable; exposure unknown and retained".into());
    }
    let amount = |value: &Value| {
        value
            .as_str()
            .and_then(|n| n.parse::<i128>().ok())
            .map_or_else(
                || "unknown".into(),
                |n| nika_types::cost::Cost::new(n).to_string(),
            )
    };
    let held = observation["attempts"]
        .as_array()
        .and_then(|attempts| {
            attempts
                .iter()
                .filter(|a| a["sent"] == true && a["estimated_nano_usd"].is_null())
                .try_fold(0_i128, |sum, a| {
                    sum.checked_add(a["reserved_nano_usd"].as_str()?.parse::<i128>().ok()?)
                })
        })
        .map_or_else(
            || "unknown".into(),
            |n| nika_types::cost::Cost::new(n).to_string(),
        );
    let limit = if observation["limit_nano_usd"].is_null() {
        "none".into()
    } else {
        amount(&observation["limit_nano_usd"])
    };
    Some(format!(
        "Historical accounting (no preparation authority): known estimate {} · retained reservation {held} (not a charge) · {} unknown call(s) · old allowance {limit} · {} · invoice unknown",
        amount(&observation["known_subtotal_nano_usd"]),
        observation["unknown_calls"],
        observation["state"].as_str().unwrap_or("unknown")
    ))
}

/// Restores the observation scope on normal return or unwind; never cancels a request.
pub struct PreparationScope(Option<PreparationCosts>, PhantomData<Rc<()>>);
impl Drop for PreparationScope {
    fn drop(&mut self) {
        ACTIVE.with(|slot| {
            slot.replace(self.0.take());
        });
    }
}

#[cfg(test)]
mod tests;
