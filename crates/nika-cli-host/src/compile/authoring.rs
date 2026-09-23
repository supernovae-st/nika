// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Opt-in host wiring only. Semantics and emission remain in the shared compiler.
//! `--authoring-model` seats one generative provider (COLD) or the operator's own agent
//! harness through ACP (`<harness>/<model>`); `--decision-model` seats one bounded-choice
//! capability (WARM): `typesafe/<jev>` through System One, any other `provider/name` through
//! a closed JSON-schema enum.
use nika_onboard::compile::{
    AuthoringPolicy, Cognition, CompileOutcome, CompileRequest, NativeMode, NoProvider,
    compile_with_cognition,
    decide::{DecisionSeat, ProviderChoice},
};
use std::{sync::Arc, time::Duration};

/// Whether `--authoring-model` names one of the engine's harness seats (`claude-code/default`).
fn names_a_harness(args: &super::CompileArgs) -> bool {
    args.authoring_model.as_deref().is_some_and(|m| {
        m.split_once('/')
            .is_some_and(|(id, _)| nika_types::access::HarnessRuntime::lookup(id).is_some())
    })
}

/// The authoring caps: quality first — a reasoning seat spends its cap on its reasoning and a
/// strong seat writes a whole candidate — the defaults are the policy's ceilings, not a thrifty
/// guess (the product reality check of 2026-09-22 measured 7/7 gpt-5-mini probes truncated at
/// 2048 tokens and 7/7 cut at 30 s). A harness (the operator's own agent through ACP) thinks
/// and tools longer than one API call: its default deadline is 300 s; every seat may be bounded
/// up to 600 s and 32768 tokens.
fn caps(args: &super::CompileArgs) -> Result<(u32, u64), String> {
    let max_tokens = args.authoring_max_tokens.unwrap_or(8192);
    let timeout = args
        .authoring_timeout
        .unwrap_or(if names_a_harness(args) { 300 } else { 120 });
    if !(1..=32_768).contains(&max_tokens) || !(1..=600).contains(&timeout) {
        return Err(
            "Authoring limits must be 1..32768 output tokens and 1..600 seconds.".to_owned(),
        );
    }
    Ok((max_tokens, timeout))
}

/// The request with its authoring policy, when a seat is named.
fn with_policy(
    request: &CompileRequest,
    args: &super::CompileArgs,
    max_tokens: u32,
    timeout: u64,
) -> CompileRequest {
    match args.authoring_model.as_deref() {
        Some(model) => request.clone().with_authoring_policy(
            AuthoringPolicy::new(model, max_tokens, Duration::from_secs(timeout))
                .with_samples(args.authoring_samples.unwrap_or(1))
                .with_native(match args.authoring_strategy.as_deref() {
                    Some("only") => NativeMode::Only,
                    Some("sketch") => NativeMode::Sketch,
                    Some("off") => NativeMode::Off,
                    _ => NativeMode::Escalate,
                })
                .with_repairs(args.authoring_repairs.unwrap_or(3)),
        ),
        None => request.clone(),
    }
}

/// The receipt names its backend: the harness that answered, or the direct provider.
fn stamp_backend(
    outcome: &mut CompileOutcome,
    args: &super::CompileArgs,
    described: Option<serde_json::Value>,
) {
    if let Some(receipt) = outcome.provenance.authoring.as_mut() {
        receipt.backend = Some(described.unwrap_or_else(|| {
            serde_json::json!({
                "kind": "direct_api",
                "provider": args.authoring_model.as_deref().and_then(|m| m.split('/').next()),
                "cost_basis": "measured_by_tokens_at_catalog_price",
            })
        }));
    }
}

pub(super) fn compile(
    request: &CompileRequest,
    args: &super::CompileArgs,
) -> Result<CompileOutcome, String> {
    let (max_tokens, timeout) = caps(args)?;
    let request = with_policy(request, args, max_tokens, timeout);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    runtime.block_on(async {
        let registry = provider_registry()?;
        let harness = harness_seat(args)?;
        let provider = match args.authoring_model.as_deref() {
            Some(model) if harness.is_none() => {
                Some(registry.resolve(model).map_err(|e| e.to_string())?)
            }
            _ => None,
        };
        let decision_provider = match args.decision_model.as_deref() {
            Some(model) if !model.starts_with("typesafe/") => {
                Some(registry.resolve(model).map_err(|e| e.to_string())?)
            }
            _ => None,
        };
        let typesafe = typesafe_seat(args)?;
        let provider_choice = decision_provider
            .as_ref()
            .zip(args.decision_model.as_deref())
            .map(|(provider, model)| {
                ProviderChoice::new(provider, model, Duration::from_secs(timeout))
            });
        let seat: Option<&dyn DecisionSeat> = match (&typesafe, &provider_choice) {
            (Some(seat), _) => Some(seat),
            (None, Some(seat)) => Some(seat),
            (None, None) => None,
        };
        // The compile future carries a whole `CompileOutcome` (its boundary, its trigger
        // requirement, its preview): boxed so the host's stack frame stays small
        // (clippy::large_futures) whatever the outcome grows to.
        let outcome = match (harness.as_ref(), provider.as_ref()) {
            (Some(harness), _) => {
                Box::pin(compile_with_cognition(
                    &request,
                    Cognition {
                        provider: Some(harness),
                        seat,
                    },
                ))
                .await
            }
            (None, Some(provider)) => {
                Box::pin(compile_with_cognition(
                    &request,
                    Cognition {
                        provider: Some(provider),
                        seat,
                    },
                ))
                .await
            }
            (None, None) => {
                Box::pin(compile_with_cognition::<NoProvider>(
                    &request,
                    Cognition {
                        provider: None,
                        seat,
                    },
                ))
                .await
            }
        };
        let mut outcome = outcome.map_err(|e| e.to_string())?;
        #[cfg(feature = "access-harness")]
        let described = harness
            .as_ref()
            .map(super::harness_seat::HarnessSeat::descriptor);
        #[cfg(not(feature = "access-harness"))]
        let described: Option<serde_json::Value> = None;
        stamp_backend(&mut outcome, args, described);
        Ok(outcome)
    })
}

/// The provider registry over the PROVIDER client, not the fetch client: the same fixed
/// allowlist of provider endpoints the runtime talks to, with its transport ceiling above the
/// per-request deadline (the policy's timeout) and no SSRF floor (a local seat binds
/// 127.0.0.1). The default client cut every authoring call at its 30s idle-read guard whatever
/// `--authoring-timeout` asked, and refused a loopback seat outright. Reached only AFTER the
/// explicit opt-in; it does not probe a keychain, select a provider, or resolve business
/// credentials.
fn provider_registry() -> Result<nika_providers::ProviderRegistry<nika_http::ReqwestHttp>, String> {
    let http = nika_runtime::compose::provider_http().map_err(|e| e.to_string())?;
    Ok(nika_providers::ProviderRegistry::new(
        Arc::new(http),
        nika_runtime::compose::config_from_env(),
    ))
}

/// A `<harness>/<model>` seat is the operator's own agent through ACP (the addendum's
/// authoring backend); every other `provider/model` is a provider of the registry. The
/// receipt says which answered.
#[cfg(feature = "access-harness")]
fn harness_seat(
    args: &super::CompileArgs,
) -> Result<Option<super::harness_seat::HarnessSeat>, String> {
    Ok(match args.authoring_model.as_deref() {
        Some(model) if super::harness_seat::HarnessSeat::names_a_harness(model) => {
            Some(super::harness_seat::HarnessSeat::meet(model)?)
        }
        _ => None,
    })
}

/// Without the access-harness feature a harness cannot seat authoring: the refusal is named,
/// never a silent provider fallback.
#[cfg(not(feature = "access-harness"))]
fn harness_seat(args: &super::CompileArgs) -> Result<Option<NoProvider>, String> {
    if names_a_harness(args) {
        return Err(
            "this binary was built without the access-harness feature; a harness cannot seat authoring"
                .to_owned(),
        );
    }
    Ok(None)
}

/// The typesafe decision seat, when `--decision-model typesafe/<jev>` names it.
fn typesafe_seat(
    args: &super::CompileArgs,
) -> Result<Option<super::typesafe::TypesafeSeat>, String> {
    match args.decision_model.as_deref() {
        Some(model) if model.starts_with("typesafe/") => {
            #[allow(clippy::disallowed_methods)]
            // the sanctioned env→secret boundary for an explicitly named seat (compose.rs precedent)
            let key = std::env::var("TYPESAFE_API_KEY").map_err(|_| {
                "TYPESAFE_API_KEY is required for a typesafe decision seat".to_owned()
            })?;
            Ok(Some(super::typesafe::TypesafeSeat::new(
                key,
                model.trim_start_matches("typesafe/"),
            )?))
        }
        _ => Ok(None),
    }
}
