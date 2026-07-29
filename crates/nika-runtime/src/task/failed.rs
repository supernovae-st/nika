// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The settled attempt-loop FAILURE channel — split from `task.rs` at
//! the 1500-LOC wall (the `task/{declassify,fan_out,finally}` precedent).

use crate::record::TaskErrorRecord;

/// A settled attempt-loop failure — the error + the spend the failed
/// attempts had already incurred (per-attempt debits happened live;
/// these fields feed the terminal frame).
pub(crate) struct FailedOutcome {
    pub record: TaskErrorRecord,
    pub cost_usd: Option<f64>,
    pub cost_unpriced: Option<nika_types::cost::UnpricedReason>,
    /// F-P6 · the commit gate's binding evidence (`Fired` — a verb error
    /// after a passed gate · `Refused` — the finding; never transient).
    pub evidence: Option<crate::dispatch::commit::CommitEvidence>,
}

impl FailedOutcome {
    pub(crate) fn new(
        record: TaskErrorRecord,
        cost_usd: Option<f64>,
        cost_unpriced: Option<nika_types::cost::UnpricedReason>,
        evidence: Option<crate::dispatch::commit::CommitEvidence>,
    ) -> Self {
        Self {
            record,
            cost_usd,
            cost_unpriced,
            evidence,
        }
    }
}
