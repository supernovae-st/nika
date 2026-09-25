// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Session owns whether its monetary observation admits a bounded decision call.
//! The selected host adapter and its journal live beside the shared `TypeSafe` transport.
pub(crate) use nika_cli_host::compile::typesafe::session::SessionSeat;
pub use nika_cli_host::compile::typesafe::session::{
    DECISION_ENV, DECISION_SCHEMA, DecisionSetup, MAX_DECISION_CALLS,
};
use nika_providers::{AdmissionState, InferenceAdmission};

/// The money law for one seated compile: the seat may be charged only on the session's no-budget
/// observation (an operator-selected, explicitly unbudgeted use), never against a number.
pub(crate) fn admit(admission: Option<&InferenceAdmission>) -> Result<(), String> {
    let Some(account) = admission else {
        return Err("the selected intelligence is not a priced API route this session observes (local, subscription or unpriced); an external decision service is not consulted".to_owned());
    };
    let receipt = account
        .snapshot()
        .map_err(|e| format!("the Session account is unreadable ({e}); no decision call"))?;
    if receipt.state != AdmissionState::Open {
        return Err(format!(
            "the Session account is {:?}; no further paid call, the decision seat included",
            receipt.state
        ));
    }
    if !receipt.unbudgeted {
        return Err("this Session holds a numeric allowance or an unknown-cost scope; the decision seat has no catalog tariff, so its unknown cost is never charged against it".to_owned());
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests;
