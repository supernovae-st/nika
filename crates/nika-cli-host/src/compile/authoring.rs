// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Opt-in host wiring only. Semantics and emission remain in the shared compiler.
//! `--authoring-model` seats one generative provider (COLD) or the operator's own agent
//! harness through ACP (`<harness>/<model>`); `--decision-model` seats one bounded-choice
//! capability (WARM): `typesafe/<jev>` through System One, any other `provider/name` through
//! a closed JSON-schema enum.
use super::{authoring_http, config};
use nika_kernel::ai::provider::ProviderInferDyn;
use nika_kernel::http::HttpPostDyn;
use nika_onboard::compile::authority::{Authority, Envelope, Seat, Wire, usage_complete};
use nika_onboard::compile::{
    Cognition, CompileError, CompileOutcome, CompileRequest, NoProvider, compile_with_cognition,
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

/// The per-call bounds every call keeps (the shared producer's: the operator's caps, else the
/// defaults) and the request with its authoring policy when a seat is named: the strategy and
/// reasoning effort the configuration resolved (the flags over the environment), with the
/// operator's samples and repairs.
fn with_policy(
    request: &CompileRequest,
    args: &super::CompileArgs,
    config: &config::AuthoringConfig,
) -> Result<(CompileRequest, (u32, Duration)), String> {
    let timeout = args.authoring_timeout.map(Duration::from_secs);
    let bounds = config::call_bounds(args.authoring_max_tokens, timeout, names_a_harness(args))?;
    let Some(model) = args.authoring_model.as_deref() else {
        return Ok((request.clone(), bounds));
    };
    let policy = config
        .policy(model, bounds.0, bounds.1)?
        .with_samples(args.authoring_samples.unwrap_or(1))
        .with_repairs(args.authoring_repairs.unwrap_or(config::DEFAULT_REPAIRS));
    Ok((request.clone().with_authoring_policy(policy), bounds))
}

/// The receipt names its backend (the harness that answered, or the direct provider), the model
/// the operator requested beside the identities the responses reported (and how many reported
/// none), and the authority's account: what was sent and refused, apart from the core's own
/// journal of attempts.
fn stamp_backend(
    outcome: &mut CompileOutcome,
    args: &super::CompileArgs,
    described: Option<serde_json::Value>,
    ((observed, unreported), authority): ((Vec<String>, u32), serde_json::Value),
) {
    if let Some(receipt) = outcome.provenance.authoring.as_mut() {
        let mut backend = described.unwrap_or_else(|| {
            serde_json::json!({
                "kind": "direct_api",
                "provider": args.authoring_model.as_deref().and_then(|m| m.split('/').next()),
                "cost_basis": "unpriced; billing_unverified",
            })
        });
        backend["requested_model"] = serde_json::json!(args.authoring_model);
        backend["observed_models"] = serde_json::json!(observed);
        backend["unreported_models"] = serde_json::json!(unreported);
        backend["usage_complete"] = serde_json::json!(usage_complete(&receipt.context));
        backend["authority"] = authority;
        if let Some(model) = args.decision_model.as_deref() {
            backend["authority"]["decision_seat"] = serde_json::json!(decision_seat_note(model));
        }
        receipt.backend = Some(backend);
    }
}

/// What the receipt states about a seated decision model's own client: a `typesafe/<jev>` seat
/// sends each question once; a `provider/name` seat keeps its provider client's protocol retries.
fn decision_seat_note(model: &str) -> &'static str {
    if model.starts_with("typesafe/") {
        "outside this authority: its own single-attempt client"
    } else {
        "outside this authority: its own client, protocol retries included"
    }
}

/// Compile under the seats the flags name. The caps are every door's (the operator's, else the
/// defaults: a reasoning seat spends its cap on its reasoning, so the default is the policy's
/// ceiling, never a thrifty guess; a harness waits 300 s), and never move with the effort; the
/// decision call asks the same effort under the declared authoring cap (R4 B16).
pub(super) fn compile(
    request: &CompileRequest,
    args: &super::CompileArgs,
    config: &config::AuthoringConfig,
    authority: &Authority,
    capture_flags: &super::CaptureFlags,
) -> Result<CompileOutcome, String> {
    let (request, (max_tokens, timeout)) = with_policy(request, args, config)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    runtime.block_on(async {
        let registry = provider_registry()?;
        let invocations = authority.envelope();
        let requests = authority.envelope();
        let harness = harness_seat(args)?.map(|seat| Seat::new(seat, invocations.clone()));
        let (provider, backend) = match args.authoring_model.as_deref() {
            Some(model) if harness.is_none() => {
                let registry = authoring_registry(requests.clone())?;
                let backend = super::authoring_backend(&registry, model);
                let seat = registry.resolve(model).map_err(|e| e.to_string())?;
                (Some(Seat::new(seat, invocations.clone())), Some(backend))
            }
            _ => (None, None),
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
                let choice = ProviderChoice::new(provider, model, timeout);
                match config.reasoning {
                    Some(level) => choice.with_reasoning(level, max_tokens),
                    None => choice,
                }
            });
        let seat: Option<&dyn DecisionSeat> = match (&typesafe, &provider_choice) {
            (Some(seat), _) => Some(seat),
            (None, Some(seat)) => Some(seat),
            (None, None) => None,
        };
        // The resolved keys are withheld from private capture; a harness seat's context is not
        // admitted by the CLI opt-in, so its Text is observed but withheld.
        let keys = [
            provider.as_ref().and_then(|seat| seat.inner().key()),
            decision_provider.as_ref().and_then(|seat| seat.key()),
        ];
        let keys = keys.iter().flatten().map(|key| key.expose());
        let keys = keys.chain(typesafe.as_ref().map(super::typesafe::TypesafeSeat::key));
        let scope = (args.authoring_model.as_deref(), max_tokens, timeout);
        let work = seated(&request, harness.as_ref(), provider.as_ref(), seat);
        let outcome =
            super::capture::cli_observe(capture_flags, keys, harness.is_none(), scope, work).await;
        let mut outcome = outcome.map_err(|e| e.to_string())?;
        #[cfg(feature = "access-harness")]
        let described = harness.as_ref().map(|seat| seat.inner().descriptor());
        #[cfg(not(feature = "access-harness"))]
        let described: Option<serde_json::Value> = None;
        let reported = match (&harness, &provider) {
            (Some(seat), _) => (seat.observed(), seat.unreported()),
            (None, Some(seat)) => (seat.observed(), seat.unreported()),
            (None, None) => (Vec::new(), 0),
        };
        // A harness is counted in invocations; its own requests are not observable here.
        let wire = harness.is_none().then_some(requests.as_ref());
        let account = authority.record(&invocations, wire);
        let backend = described.or(backend);
        stamp_backend(&mut outcome, args, backend, (reported, account));
        Ok(outcome)
    })
}

/// One compile under its generative seat: the operator's harness when named, else the
/// provider, else none (the decision seat rides beside either). The compile future carries a
/// whole `CompileOutcome` (its boundary, its trigger requirement, its preview): boxed so the
/// host's stack frame stays small (`clippy::large_futures`) whatever the outcome grows to.
async fn seated<H: ProviderInferDyn, P: ProviderInferDyn>(
    request: &CompileRequest,
    harness: Option<&H>,
    provider: Option<&P>,
    seat: Option<&dyn DecisionSeat>,
) -> Result<CompileOutcome, CompileError> {
    match (harness, provider) {
        (Some(harness), _) => {
            Box::pin(compile_with_cognition(
                request,
                Cognition {
                    provider: Some(harness),
                    seat,
                },
            ))
            .await
        }
        (None, Some(provider)) => {
            Box::pin(compile_with_cognition(
                request,
                Cognition {
                    provider: Some(provider),
                    seat,
                },
            ))
            .await
        }
        (None, None) => {
            Box::pin(compile_with_cognition::<NoProvider>(
                request,
                Cognition {
                    provider: None,
                    seat,
                },
            ))
            .await
        }
    }
}

/// The registry of the authoring seat: the shared authoring transport, under the authority's
/// wire counter (the decision seat keeps its own authority and client).
fn authoring_registry(
    requests: Arc<Envelope>,
) -> Result<nika_providers::ProviderRegistry<Wire<impl HttpPostDyn>>, String> {
    let http = authoring_http().map_err(|e| e.to_string())?;
    Ok(nika_providers::ProviderRegistry::new(
        Arc::new(Wire::new(http, requests)),
        nika_runtime::compose::config_from_env(),
    ))
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
        // the one shared adapter reads the key only now that the flag named the seat
        Some(model) if model.starts_with("typesafe/") => Ok(Some(
            super::typesafe::TypesafeSeat::from_env(model.trim_start_matches("typesafe/"))?,
        )),
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::decision_seat_note;

    #[test]
    fn the_receipt_names_the_client_of_the_seated_decision_model() {
        assert_eq!(
            decision_seat_note("typesafe/jev-1.13.0"),
            "outside this authority: its own single-attempt client"
        );
        assert_eq!(
            decision_seat_note("vllm/loopback-seat"),
            "outside this authority: its own client, protocol retries included"
        );
    }
}
