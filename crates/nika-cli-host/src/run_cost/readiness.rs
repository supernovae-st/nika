// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Shared host route observation; source checks never acquire spending authority.
use nika_providers::admission::CostRoute;
use nika_providers::{ExecutionAccessPlan, ProvidersConfig};
use nika_schema::raw::RawWorkflow;

pub(super) fn unknown_routes(
    plan: &ExecutionAccessPlan,
    config: &ProvidersConfig,
) -> Result<Vec<(String, CostRoute)>, String> {
    let mut unknown = Vec::new();
    for (model, lane) in plan.admitted() {
        if lane.plan.chosen == nika_types::access::AccessClass::Api {
            match CostRoute::observe(model, config.clone()) {
                Ok(route) if route.needs_unknown_choice() => {
                    unknown.push((model.to_owned(), route));
                }
                Ok(_) => {}
                Err(_)
                    if nika_providers::admission::native_catalog_price_known(
                        model,
                        config.clone(),
                    ) => {}
                Err(why) => return Err(why),
            }
        }
    }
    Ok(unknown)
}

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
        return None;
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
