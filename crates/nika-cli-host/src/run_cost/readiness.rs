// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Shared host route observation; source checks never acquire spending authority.
use nika_providers::ExecutionAccessPlan;
use nika_schema::raw::RawWorkflow;
// The Run's unknown-cost route law, owned beside its model admission.
pub(super) use nika_service_execution::run_cost::unknown_routes;

/// Check's mirror of the Run's monetary admission under this process's provider
/// configuration (the judgment is L3's `run_cost::readiness`).
pub(crate) fn readiness(wf: &RawWorkflow, plan: &ExecutionAccessPlan) -> Option<String> {
    let config = nika_runtime::compose::config_from_env();
    nika_service_execution::run_cost::readiness(wf, plan, &config)
}

#[cfg(test)]
mod tests;
