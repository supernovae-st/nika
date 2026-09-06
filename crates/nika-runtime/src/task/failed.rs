// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The settled attempt-loop failure channel — split from `task.rs` at the
//! 1500-LOC wall (the `task/{declassify,fan_out,finally}` precedent).

use crate::record::TaskErrorRecord;

impl crate::task::RunResult {
    /// An `on_error: recover` repair — the ONE constructor both recover
    /// paths share: author-supplied value, the WHOLE original error
    /// riding as `recovered_from` (spec 13 §payload), the failed
    /// attempts' spend kept (recovery never refunds it), no child, no
    /// model (nothing inferred the fallback value). Lives here with the
    /// rest of the failure channel — `task.rs` sits at the LOC wall.
    pub(crate) fn recovered(
        value: serde_json::Value,
        original: TaskErrorRecord,
        cost_usd: Option<f64>,
        cost_unpriced: Option<nika_types::cost::UnpricedReason>,
    ) -> Self {
        Self::Success {
            access: None,
            value,
            tokens: None,
            recovered_from: Some(original),
            warning: None,
            child: None,
            cost_usd,
            cost_unpriced,
            model: None,
        }
    }
}

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
    /// The admitted lane the failed dispatch rode (wave 2b).
    pub access: Option<Box<nika_types::access::AccessPlan>>,
    /// Q01 · the usage split of the billed-then-failed attempts.
    pub usage: Option<Box<crate::usage::UsageSplit>>,
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
            access: None,
            usage: None,
        }
    }

    /// Attach the lane the failed dispatch rode (wave 2b).
    pub(crate) fn with_access(
        mut self,
        access: Option<Box<nika_types::access::AccessPlan>>,
    ) -> Self {
        self.access = access;
        self
    }

    /// Attach the split of what the failed attempts burned (Q01).
    pub(crate) fn with_usage(mut self, usage: Option<Box<crate::usage::UsageSplit>>) -> Self {
        self.usage = usage;
        self
    }
}
