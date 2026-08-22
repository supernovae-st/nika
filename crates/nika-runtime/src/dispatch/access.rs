// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Dynamic-model refusal at the runtime dispatch seam.

use super::{Dispatched, FailedDispatch};
use crate::record::TaskErrorRecord;

pub(crate) use nika_providers::AccessObserver;
pub use nika_providers::AccessReceipt;

impl Dispatched {
    /// Backstop for a dynamically rendered model whose access meet has no
    /// survivor. Static models were already judged at admission; this value
    /// did not exist until rendering. No provider/harness effect has fired.
    pub(super) fn access_refused(note: &str, refusal: &nika_providers::AccessRefusal) -> Self {
        let witnesses = refusal
            .rejected
            .iter()
            .map(nika_types::access::AccessRejection::witness_line)
            .collect::<Vec<_>>();
        let suffix = if witnesses.is_empty() {
            String::new()
        } else {
            format!(" · {}", witnesses.join(" · "))
        };
        Self {
            note: note.to_owned(),
            result: Err(FailedDispatch {
                record: TaskErrorRecord {
                    code: nika_error::codes::NIKA_1800.to_string(),
                    message: format!(
                        "no access path survives admission for `{}`{suffix}",
                        refusal.model
                    ),
                    transient: false,
                },
                cost_usd: None,
                cost_source: None,
                cost_unpriced: None,
                access_receipt: Some(AccessReceipt::refused(refusal)),
                evidence: None,
            }),
        }
    }
}
