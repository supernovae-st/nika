// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The failed verb's usage split. The model-spend computation it sits beside
//! descended to its evidence owner, `nika_providers::spend`, at the
//! runtime's 15k wall (2026-09-28); the dispatch seam reads it from there.

pub(super) use nika_providers::spend::{price_failed_spend, spend_for_calls, spend_for_model};

/// the split of what a FAILED verb had already burned — the same
/// numbers `price_failed_spend` turns into dollars, so `task_failed`
/// explains its own `cost_usd`.
pub(super) fn failed_usage_split(
    spend: Option<&nika_types::cost::SpendOnFailure>,
) -> Option<Box<crate::usage::UsageSplit>> {
    let spend = spend?;
    crate::usage::UsageSplit::of(&spend.usage)
        .with_calls(&spend.inference_calls)
        .carried()
}

#[cfg(test)]
mod billing_tests;
