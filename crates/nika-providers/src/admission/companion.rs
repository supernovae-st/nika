// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Account compatibility for a separately observed, explicitly selected unpriced service.
use super::{AdmissionState, InferenceAdmission};

/// A selected decision service is separately observed in interactive preparation. A caller
/// that supplies an account keeps its explicit monetary restrictions; the companion never consumes it.
/// This checks account compatibility, not operator selection: the host must select the service.
/// # Errors
/// An unreadable, non-open or explicitly budgeted account cannot pay an unpriced companion.
pub fn admit_unpriced_companion(admission: Option<&InferenceAdmission>) -> Result<(), String> {
    let Some(account) = admission else {
        return Ok(());
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
