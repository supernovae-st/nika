// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The resident's cost gate for a job with no reviewed authority (manual,
//! snapshot or scheduled): the one host evaluator `nika run` uses (C6). An
//! admitted API route whose price needs a fresh one-time choice refuses before
//! the worker starts, with the review door named; exact priced, local and
//! model-free plans keep their composition; a declared-free route binds the
//! per-Run observer. Plans come from the public constructors; nothing is
//! dispatched and no environment is written. (The route-observation law, its
//! wrong-endpoint and override cases included, is tested with its owner:
//! `nika_service_execution::run_cost`.)

use std::collections::BTreeMap;
use std::path::Path;

use nika_providers::{ExecutionAccessPlan, LaneVerdict, ResolvedLane};
use nika_service_execution::ServiceExecutionDriver;
use nika_types::access::{AccessClass, AccessPlan, BillingClass};

use super::unreviewed_cost;

fn plan_with(
    model: &str,
    provider: &str,
    chosen: AccessClass,
    billing: BillingClass,
) -> ExecutionAccessPlan {
    let access = AccessPlan::new(
        model,
        provider,
        provider,
        chosen,
        billing,
        false,
        Vec::new(),
    );
    let mut lanes = BTreeMap::new();
    lanes.insert(
        model.to_owned(),
        LaneVerdict::Admitted(ResolvedLane::new(access, 1)),
    );
    ExecutionAccessPlan::new(lanes, None, None, None)
}

/// A driver over one admitted workflow that asks `model` for bounded text.
#[cfg(test)]
fn driver(root: &Path, model: &str, max_tokens: u32) -> ServiceExecutionDriver {
    let source = format!(
        "nika: gate\nmodel: {model}\npermits: {{}}\ntasks:\n  ask:\n    infer: {{ prompt: hi, max_tokens: {max_tokens} }}\n"
    );
    std::fs::write(root.join("gate.nika"), source).expect("workflow");
    let project = nika_fs::OwnedDir::open(root).expect("project");
    let service = nika_execution::ExecutionService::default();
    let admitted = service
        .admit(&project, Path::new("gate.nika"))
        .expect("a clean workflow admits");
    let session = service.begin(admitted);
    ServiceExecutionDriver::new(session.context(), root).expect("driver")
}

#[cfg(test)]
fn judge(
    model: &str,
    provider: &str,
    chosen: AccessClass,
    billing: BillingClass,
) -> Result<bool, String> {
    let root = tempfile::tempdir().expect("root");
    let plan = plan_with(model, provider, chosen, billing);
    // A reasoning seat needs room for its thinking before Check admits it.
    let max_tokens = if provider == "deepseek" { 512 } else { 16 };
    let cost = unreviewed_cost(
        root.path(),
        &driver(root.path(), model, max_tokens),
        &plan,
        &BTreeMap::new(),
    )?;
    // Whatever the verdict, the lease the evaluator may have taken is free again.
    let project = nika_fs::OwnedDir::open(root.path()).expect("project");
    let clear = nika_dap::cost_journal::clear(&project, "after").expect("journal");
    assert!(
        clear.is_ok(),
        "the evaluator never keeps the project's cost lease"
    );
    Ok(cost.is_some())
}

#[test]
fn a_default_openai_route_without_a_review_refuses_and_names_the_door() {
    let refused = judge(
        "openai/gpt-4.1-mini",
        "openai",
        AccessClass::Api,
        BillingClass::ApiMetered,
    )
    .expect_err("an unreviewed unknown-cost route must not reach the provider");
    assert!(refused.contains("price unknown"), "{refused}");
    assert!(refused.contains("--cost-review"), "{refused}");
    assert!(
        refused.contains("scheduled occurrences stay refused"),
        "{refused}"
    );
}

#[test]
fn deepseek_direct_stays_admitted_without_an_observer() {
    let observed = judge(
        "deepseek/deepseek-flash",
        "deepseek",
        AccessClass::Api,
        BillingClass::ApiMetered,
    );
    assert_eq!(observed, Ok(false));
}

#[test]
fn a_local_lane_is_never_observed_as_an_api_price() {
    let observed = judge(
        "ollama/llama3.2",
        "ollama",
        AccessClass::Local,
        BillingClass::Local,
    );
    assert_eq!(observed, Ok(false));
}

#[test]
fn a_catalog_priced_native_default_stays_admitted() {
    let model = "anthropic/claude-sonnet-4-20250514";
    let observed = judge(
        model,
        "anthropic",
        AccessClass::Api,
        BillingClass::ApiMetered,
    );
    assert_eq!(observed, Ok(false));
}

/// C4 parity on the resident: an exact declared-free route binds the same
/// per-Run observer `nika run` binds, never unknown-cost authority.
#[test]
fn a_declared_free_route_binds_the_observer() {
    let free = "openrouter/qwen/qwen3.8-27b:free";
    let observed = judge(
        free,
        "openrouter",
        AccessClass::Api,
        BillingClass::ApiMetered,
    );
    assert_eq!(observed, Ok(true));
}
