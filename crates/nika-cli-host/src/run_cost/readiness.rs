// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Shared host route observation; source checks never acquire spending authority.
use nika_providers::{ExecutionAccessPlan, ProvidersConfig};
use nika_schema::raw::RawWorkflow;
// The Run's unknown-cost route law, owned beside its model admission.
pub(super) use nika_service_execution::run_cost::unknown_routes;

pub(crate) fn readiness(wf: &RawWorkflow, plan: &ExecutionAccessPlan) -> Option<String> {
    readiness_with_config(wf, plan, &nika_runtime::compose::config_from_env())
}

fn readiness_with_config(
    wf: &RawWorkflow,
    plan: &ExecutionAccessPlan,
    config: &ProvidersConfig,
) -> Option<String> {
    let routes = match unknown_routes(plan, config) {
        Ok(routes) => routes,
        Err(why) => return Some(format!("Run monetary admission is unresolved: {why}")),
    };
    if routes.is_empty() {
        // Check mirrors the Run's refusal of what a declared-free observation
        // cannot admit: never run ready, never a known zero.
        return nika_service_execution::run_cost::declared_free_shape(wf, plan, config, None)
            .err()
            .map(|refusal| format!("Run cannot observe its declared-free route: {refusal}"));
    }
    Some(
        match nika_service_execution::run_cost::request_bound(wf, plan, routes.len()) {
            Ok(bound) => format!(
                "USD cost is unknown: Run requires a fresh finite-call choice for at most {bound} requests, including schema re-asks; Check has not admitted spend or effects"
            ),
            Err(why) => format!("USD cost is unknown; Run cannot obtain a bounded choice: {why}"),
        },
    )
}

#[cfg(test)]
mod tests;
