// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The resident's unknown-cost gate: an unattended Serve run has no Run cost
//! review, so an admitted API route whose price needs a fresh one-time choice
//! (for example an OpenAI-compatible override on plain HTTP, which `nika run`
//! refuses before dispatch) refuses before the worker starts. Plans come from
//! the public constructors and the provider configuration is explicit: no
//! environment is read or written and nothing is dispatched.

use std::collections::BTreeMap;

use nika_providers::{ExecutionAccessPlan, LaneVerdict, ProvidersConfig, ResolvedLane};
use nika_types::access::{AccessClass, AccessPlan, BillingClass};

use super::unknown_cost_refusal;

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

#[test]
fn an_http_api_override_refuses_before_dispatch() {
    // An OpenAI-compatible override on plain HTTP (a loopback endpoint).
    let config =
        ProvidersConfig::new().with_base_url("openai", "http://127.0.0.1:9/v1/chat/completions");
    let plan = plan_with(
        "openai/gpt-4.1-mini",
        "openai",
        AccessClass::Api,
        BillingClass::ApiMetered,
    );
    let refusal = unknown_cost_refusal(&plan, &config);
    assert!(
        refusal.is_some(),
        "an unreviewed HTTP override must not reach the provider"
    );
    let message = refusal.unwrap_or_default();
    assert!(
        message.contains("price unknown for `openai/gpt-4.1-mini`"),
        "{message}"
    );
    assert!(message.contains("cost review"), "{message}");
}

#[test]
fn a_default_openai_route_without_admission_refuses() {
    // Default OpenAI endpoints need a fresh review in 0.121.0; Serve has none.
    let plan = plan_with(
        "openai/gpt-4.1-mini",
        "openai",
        AccessClass::Api,
        BillingClass::ApiMetered,
    );
    assert!(
        unknown_cost_refusal(&plan, &ProvidersConfig::new()).is_some(),
        "an unadmitted default API route must refuse on the unattended resident"
    );
}

#[test]
fn deepseek_direct_stays_admitted() {
    let plan = plan_with(
        "deepseek/deepseek-flash",
        "deepseek",
        AccessClass::Api,
        BillingClass::ApiMetered,
    );
    assert_eq!(unknown_cost_refusal(&plan, &ProvidersConfig::new()), None);
}

#[test]
fn a_local_lane_is_never_observed_as_an_api_price() {
    let config = ProvidersConfig::new().with_base_url("ollama", "http://127.0.0.1:11434");
    let plan = plan_with(
        "ollama/llama3.2",
        "ollama",
        AccessClass::Local,
        BillingClass::Local,
    );
    assert_eq!(unknown_cost_refusal(&plan, &config), None);
}

#[test]
fn a_plan_without_model_lanes_passes() {
    assert_eq!(
        unknown_cost_refusal(&ExecutionAccessPlan::default(), &ProvidersConfig::new()),
        None
    );
}

#[test]
fn a_catalog_priced_native_default_stays_admitted() {
    // A native (non OpenAI-compatible) wire at its default endpoint with a
    // catalog price keeps its existing paid path.
    let plan = plan_with(
        "anthropic/claude-sonnet-4-20250514",
        "anthropic",
        AccessClass::Api,
        BillingClass::ApiMetered,
    );
    assert_eq!(unknown_cost_refusal(&plan, &ProvidersConfig::new()), None);
}

#[test]
fn an_overridden_native_gateway_refuses() {
    // An override never borrows the native catalog price.
    let config =
        ProvidersConfig::new().with_base_url("anthropic", "https://gateway.invalid/v1/messages");
    let plan = plan_with(
        "anthropic/claude-sonnet-4-20250514",
        "anthropic",
        AccessClass::Api,
        BillingClass::ApiMetered,
    );
    assert!(
        unknown_cost_refusal(&plan, &config).is_some(),
        "an overridden native gateway must refuse on the unattended resident"
    );
}
