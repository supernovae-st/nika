// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The run's cost cap as an `agent:` loop's paid media tool calls meet it
//! (C-D3). The loop starts from what the cap left when its task dispatched
//! (a snapshot of the run's ledger) and charges, as it goes, everything it
//! spends that the ledger only debits once the task returns: its model turns
//! at their known estimate, its returned tool calls at their reported spend,
//! and the floors of the paid media calls still in flight. One turn's calls
//! run concurrently, so each one judged holds its floor until it returns: a
//! model batching image calls cannot spend the same remainder twice. A paid
//! call that billed without a figure stays charged at its floor, and from
//! then on a call billed only from the usage it reports is refused.
//!
//! The snapshot is per task: concurrent tasks of the run (fan-out items,
//! wave siblings) each judge their own loop against it, as text siblings in
//! flight do.

use std::sync::{Mutex, PoisonError};

use nika_types::cost::InferenceCall;

/// What the run's cost cap leaves this loop's paid media calls.
pub(crate) struct MediaAllowance {
    /// The run's USD left under its cap when the task dispatched · `None`:
    /// the run has no cap, and nothing is judged.
    cap: Option<f64>,
    charged: Mutex<Charged>,
}

#[derive(Default)]
struct Charged {
    /// The loop's model turns so far, at their known estimates.
    turns: f64,
    /// The loop's paid calls that returned: their reported spend, or the
    /// floor of one that billed without a figure.
    tools: f64,
    /// The floors held by the paid calls in flight.
    held: f64,
    /// A paid call billed without a figure.
    unread: bool,
}

/// One admitted paid media call's hold on the allowance: its floor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Held(f64);

impl MediaAllowance {
    /// The allowance of a loop whose task dispatched with `cap` left.
    pub(crate) fn new(cap: Option<f64>) -> Self {
        Self {
            cap,
            charged: Mutex::new(Charged::default()),
        }
    }

    fn charged(&self) -> std::sync::MutexGuard<'_, Charged> {
        self.charged.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Charge the loop's model turns so far (`calls`, every provider
    /// round-trip of the loop) before its next tools meet the cap: their
    /// known estimates, an unpriced one adding nothing it could claim.
    pub(crate) fn after_turns(&self, calls: &[InferenceCall]) -> &Self {
        let turns = calls
            .iter()
            .filter_map(InferenceCall::known_estimate)
            .map(nika_types::cost::Cost::to_usd_f64)
            .sum();
        self.charged().turns = turns;
        self
    }

    /// Judge tool call `name` with `args` before its request: `Ok(Some)`
    /// admits a paid media call, holding its floor until [`Self::settle`];
    /// `Ok(None)` is no cap or no paid media call; `Err` is the refusal fed
    /// back to the model (NIKA-1704), nothing sent, nothing billed.
    pub(crate) fn admit(
        &self,
        name: &str,
        args: &serde_json::Value,
    ) -> Result<Option<Held>, String> {
        let Some(cap) = self.cap else {
            return Ok(None);
        };
        let mut charged = self.charged();
        let left = (cap - charged.turns - charged.tools - charged.held).max(0.0);
        match nika_check::analyzer::media_cap_hold(name, args, left, charged.unread) {
            Ok(Some(floor)) => {
                charged.held += floor;
                Ok(Some(Held(floor)))
            }
            Ok(None) => Ok(None),
            Err(refusal) => Err(feedback(name, &refusal, left)),
        }
    }

    /// A tool call returned: `cost` its reported spend (charged whatever the
    /// tool), and an admitted paid media call (`held`) releases its floor,
    /// which stays charged when it billed without a figure (`unread`).
    pub(crate) fn settle(&self, held: Option<Held>, cost: Option<f64>, unread: bool) {
        let mut charged = self.charged();
        charged.tools += cost.unwrap_or(0.0);
        if let Some(Held(floor)) = held {
            charged.held = (charged.held - floor).max(0.0);
            if unread {
                charged.tools += floor;
                charged.unread = true;
            }
        }
    }
}

/// The refusal a capped loop feeds back, so the model can pick another
/// provider or model: an unpriced call names what the catalog prices.
fn feedback(name: &str, refusal: &nika_check::analyzer::MediaCapRefusal<'_>, left: f64) -> String {
    let refused = "NIKA-1704 · refused before the media request, nothing was sent";
    let why = refusal.reason(name, left);
    match refusal {
        nika_check::analyzer::MediaCapRefusal::Unpriced { .. } => format!(
            "{refused}: {why}. {}.",
            nika_check::analyzer::media_cap_remedy()
        ),
        _ => format!("{refused}: {why}."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const IMAGE: &str = "nika:image_generate";

    fn low() -> serde_json::Value {
        json!({ "provider": "openai", "size": "1024x1024", "quality": "low", "prompt": "a star" })
    }

    #[test]
    fn concurrent_calls_cannot_spend_the_same_remainder() {
        let allowance = MediaAllowance::new(Some(0.01));
        let first = allowance.admit(IMAGE, &low()).expect("fits").expect("held");
        let second = allowance
            .admit(IMAGE, &low())
            .expect_err("the first holds $0.006");
        assert!(second.contains("$0.004000 left"), "{second}");
        allowance.settle(Some(first), Some(0.00821), false);
        let third = allowance.admit(IMAGE, &low()).expect_err("$0.00179 left");
        assert!(third.contains("costs at least $0.006000"), "{third}");
    }

    #[test]
    fn a_call_billed_without_a_figure_stays_charged_and_blinds_usage_billing() {
        let allowance = MediaAllowance::new(Some(1.0));
        let held = allowance.admit(IMAGE, &low()).expect("fits").expect("held");
        allowance.settle(Some(held), None, true);
        let flare = json!({ "model": "gpt-image-2.5-flare", "prompt": "a star" });
        let refused = allowance
            .admit(IMAGE, &flare)
            .expect_err("no floor, unread");
        assert!(refused.contains("billed without a figure"), "{refused}");
        assert!(allowance.admit(IMAGE, &low()).expect("fits").is_some());
    }

    #[test]
    fn no_cap_and_unmetered_media_are_never_judged() {
        let free = MediaAllowance::new(None);
        assert_eq!(free.admit(IMAGE, &low()), Ok(None));
        let capped = MediaAllowance::new(Some(0.0));
        let mock = json!({ "provider": "mock", "prompt": "a star" });
        assert_eq!(capped.admit(IMAGE, &mock), Ok(None));
        assert_eq!(capped.admit("nika:read", &json!({ "path": "x" })), Ok(None));
    }
}
