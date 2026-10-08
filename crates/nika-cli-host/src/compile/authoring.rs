// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Opt-in host wiring only. Semantics and emission remain in the shared compiler.
//! `--authoring-model` seats one generative provider (COLD) or the operator's own agent
//! harness through ACP (`<harness>/<model>`); `--decision-model` seats one bounded-choice
//! capability (WARM): `typesafe/<jev>` through System One, any other `provider/name` through
//! a closed JSON-schema enum.
use super::{authoring_http_with_deadline, config};
use nika_kernel::ai::provider::ProviderInferDyn;
use nika_kernel::http::HttpPostDyn;
use nika_onboard::compile::authority::{Authority, Envelope, Seat, Wire, usage_complete};
use nika_onboard::compile::{
    Cognition, CompileError, CompileOutcome, CompileRequest, NoProvider,
    compile_with_cognition_composed,
    decide::{DecisionSeat, ProviderChoice},
    rehearse::Rehearse,
};
use nika_onboard::knowledge::ComponentCatalog;
use std::{sync::Arc, time::Duration};

/// Whether `--authoring-model` names one of the engine's harness seats (`claude-code/default`).
fn names_a_harness(args: &super::CompileArgs) -> bool {
    args.authoring_model.as_deref().is_some_and(|m| {
        m.split_once('/')
            .is_some_and(|(id, _)| nika_types::access::HarnessRuntime::lookup(id).is_some())
    })
}

/// The per-call bounds every call keeps (the operator's, else the provider-owned route
/// capacity) and the request with its authoring policy when a seat is named: the strategy and
/// reasoning effort the configuration resolved (the flags over the environment), with the
/// operator's samples and repairs.
fn with_policy(
    request: &CompileRequest,
    args: &super::CompileArgs,
    config: &config::AuthoringConfig,
    providers: nika_providers::ProvidersConfig,
) -> Result<(CompileRequest, (u32, Duration)), String> {
    let model = args
        .authoring_model
        .as_deref()
        .or(args.decision_model.as_deref())
        .unwrap_or("");
    let route = nika_providers::authoring::policy::completion_bounds(
        model,
        names_a_harness(args),
        providers,
    );
    let bounds = (
        args.authoring_max_tokens.unwrap_or(route.max_tokens),
        args.authoring_timeout
            .map_or(route.timeout, Duration::from_secs),
    );
    config::check_call_bounds(bounds.0, bounds.1)?;
    let Some(model) = args.authoring_model.as_deref() else {
        return Ok((request.clone(), bounds));
    };
    let policy = config
        .policy(model, bounds.0, bounds.1)?
        .with_initial_max_tokens(route.initial_tokens.min(bounds.0))
        .with_samples(args.authoring_samples.unwrap_or(1));
    // A typed repair limit runs as typed; none typed keeps the policy's own default: no count.
    let policy = match args.authoring_repairs {
        Some(repairs) => policy.with_repairs(repairs),
        None => policy,
    };
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
/// Every door that seats a decision model (the CLI, Serve) states it in these words.
#[must_use]
pub fn decision_seat_note(model: &str) -> &'static str {
    if model.starts_with("typesafe/") {
        "outside this authority: its own single-attempt client"
    } else {
        "outside this authority: its own client, protocol retries included"
    }
}

/// Compile under the seats the flags name. The caps are every door's (the operator's, else the
/// selected route's technical capacity and deadline), and never move with the effort; the
/// decision call asks the same effort under the declared authoring cap (R4 B16). With a
/// rehearsal `host` (the observed room over the working directory), each final candidate is
/// tried on a scratch copy of its stated inputs, as the Session tries it: a failed or missing
/// trial is evidence the judge reads and the repairs start from, never a READY. With a
/// `catalog` (the release the pack came from, its holdout kept out), the pack is qualified over
/// the whole catalogue.
pub(super) fn compile(
    request: &CompileRequest,
    args: &super::CompileArgs,
    (config, authority): (&config::AuthoringConfig, &Authority),
    capture_flags: &super::CaptureFlags,
    (host, catalog): (Option<&dyn Rehearse>, Option<&dyn ComponentCatalog>),
) -> Result<CompileOutcome, String> {
    let providers = nika_runtime::compose::config_from_env();
    let (request, (max_tokens, timeout)) = with_policy(request, args, config, providers.clone())?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    runtime.block_on(async {
        let registry = provider_registry(providers.clone())?;
        let invocations = authority.envelope();
        let requests = authority.envelope();
        let harness = harness_seat(args)?.map(|seat| Seat::new(seat, invocations.clone()));
        let (provider, backend) = match args.authoring_model.as_deref() {
            Some(model) if harness.is_none() => {
                let registry = authoring_registry(requests.clone(), providers.clone(), timeout)?;
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
                let choice = ProviderChoice::new(provider, model, timeout, max_tokens);
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
        let rooms = (host, catalog);
        let work = seated(&request, (harness.as_ref(), provider.as_ref()), seat, rooms);
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
        let mut account = authority.record(&invocations, wire);
        // An explicit source recovery draws on this same allowance (`max_calls`): the theoretical
        // worst case is stated with and without its requests (null when none is finite), the
        // allowance never raised.
        if config.source_recovery > 0 {
            let extra = nika_onboard::compile::authority::recovery_requests(config.source_recovery);
            let configured = &mut account["configured"];
            let bound = configured["worst_case"]
                .as_u64()
                .map(|w| w + u64::from(extra));
            configured["worst_case_with_recovery"] = bound.into();
            configured["recovery_requests"] = extra.into();
            configured["source_recovery"] = config.source_recovery.into();
        }
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
    (harness, provider): (Option<&H>, Option<&P>),
    seat: Option<&dyn DecisionSeat>,
    (host, catalog): (Option<&dyn Rehearse>, Option<&dyn ComponentCatalog>),
) -> Result<CompileOutcome, CompileError> {
    match (harness, provider) {
        (Some(harness), _) => {
            Box::pin(compile_with_cognition_composed(
                request,
                Cognition {
                    provider: Some(harness),
                    seat,
                },
                host,
                catalog,
            ))
            .await
        }
        (None, Some(provider)) => {
            Box::pin(compile_with_cognition_composed(
                request,
                Cognition {
                    provider: Some(provider),
                    seat,
                },
                host,
                catalog,
            ))
            .await
        }
        (None, None) => {
            Box::pin(compile_with_cognition_composed::<NoProvider>(
                request,
                Cognition {
                    provider: None,
                    seat,
                },
                host,
                catalog,
            ))
            .await
        }
    }
}

/// The registry of the authoring seat: the shared authoring transport at the call's own
/// `deadline`, under the authority's wire counter (the decision seat keeps its own authority and
/// client).
fn authoring_registry(
    requests: Arc<Envelope>,
    providers: nika_providers::ProvidersConfig,
    deadline: Duration,
) -> Result<nika_providers::ProviderRegistry<Wire<impl HttpPostDyn>>, String> {
    let http = authoring_http_with_deadline(deadline).map_err(|e| e.to_string())?;
    Ok(nika_providers::ProviderRegistry::new(
        Arc::new(Wire::new(http, requests)),
        providers,
    ))
}

/// The provider registry over the PROVIDER client, not the fetch client: the same fixed
/// allowlist of provider endpoints the runtime talks to, with its transport ceiling above the
/// per-request deadline (the policy's timeout) and no SSRF floor (a local seat binds
/// 127.0.0.1). The default client cut every authoring call at its 30s idle-read guard whatever
/// `--authoring-timeout` asked, and refused a loopback seat outright. Reached only AFTER the
/// explicit opt-in; it does not probe a keychain, select a provider, or resolve business
/// credentials.
fn provider_registry(
    providers: nika_providers::ProvidersConfig,
) -> Result<nika_providers::ProviderRegistry<nika_http::ReqwestHttp>, String> {
    let http = nika_runtime::compose::provider_http().map_err(|e| e.to_string())?;
    Ok(nika_providers::ProviderRegistry::new(
        Arc::new(http),
        providers,
    ))
}

/// A `<harness>/<model>` seat is the operator's own agent through ACP (the addendum's
/// authoring backend); every other `provider/model` is a provider of the registry. The
/// receipt says which answered.
#[cfg(feature = "access-harness")]
fn harness_seat(
    args: &super::CompileArgs,
) -> Result<Option<nika_harness::compile_seat::HarnessSeat>, String> {
    Ok(match args.authoring_model.as_deref() {
        Some(model) if nika_harness::compile_seat::HarnessSeat::names_a_harness(model) => {
            Some(nika_harness::compile_seat::HarnessSeat::meet(model)?)
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
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use clap::Parser as _;
    use nika_providers::ProvidersConfig;

    #[derive(clap::Parser)]
    struct Door {
        #[command(flatten)]
        args: super::super::CompileArgs,
    }

    fn request(argv: &[&str], providers: ProvidersConfig) -> Result<CompileRequest, String> {
        let door = Door::try_parse_from(
            ["compile", "create a report"]
                .into_iter()
                .chain(argv.iter().copied()),
        )
        .unwrap();
        let settings = config::AuthoringSettings::none();
        let config = config::resolve(&settings, &settings).unwrap();
        super::with_policy(
            &CompileRequest::create("create a report"),
            &door.args,
            &config,
            providers,
        )
        .map(|(request, _)| request)
    }

    #[test]
    fn defaults_follow_the_exact_route_and_do_not_bound_repairs() {
        let args = ["--authoring-model", "deepseek/deepseek-v4-pro"];
        let direct = request(&args, ProvidersConfig::new())
            .unwrap()
            .authoring
            .unwrap();
        assert_eq!(
            (direct.max_tokens, direct.initial_max_tokens),
            (393_216, Some(131_072))
        );
        assert_eq!(direct.timeout, Duration::from_secs(600));
        assert_eq!(direct.repairs, None);
        let gateway = request(
            &args,
            ProvidersConfig::new()
                .with_base_url("deepseek", "https://gateway.invalid/v1/chat/completions"),
        )
        .unwrap()
        .authoring
        .unwrap();
        assert_eq!(
            (gateway.max_tokens, gateway.initial_max_tokens),
            (32_768, Some(16_384))
        );
        let scaleway = request(
            &["--authoring-model", "openai/gpt-oss-120b"],
            ProvidersConfig::new()
                .with_base_url("openai", "https://api.scaleway.ai/v1/chat/completions"),
        )
        .unwrap()
        .authoring
        .unwrap();
        assert_eq!(scaleway.max_tokens, 32_768);
    }

    #[test]
    fn explicit_large_output_and_repairs_are_kept_not_clamped() {
        let args = [
            "--authoring-model",
            "deepseek/deepseek-v4-pro",
            "--authoring-max-tokens",
            "65536",
            "--authoring-repairs",
            "9",
            "--authoring-timeout",
            "42",
        ];
        let policy = request(&args, ProvidersConfig::new())
            .unwrap()
            .authoring
            .unwrap();
        assert_eq!(
            (policy.max_tokens, policy.initial_max_tokens),
            (65_536, Some(65_536))
        );
        assert_eq!(policy.timeout, Duration::from_secs(42));
        assert_eq!(policy.repairs, Some(9));
        assert!(
            request(
                &[
                    "--authoring-model",
                    "mock/echo",
                    "--authoring-max-tokens",
                    "0"
                ],
                ProvidersConfig::new()
            )
            .is_err()
        );
    }

    #[test]
    fn clap_accepts_the_full_repair_type_and_harness_defaults_are_route_owned() {
        let request = request(
            &[
                "--authoring-model",
                "codex/default",
                "--authoring-repairs",
                "4294967295",
            ],
            ProvidersConfig::new(),
        )
        .unwrap();
        let policy = request.authoring.unwrap();
        assert_eq!(policy.repairs, Some(u32::MAX));
        assert_eq!(policy.timeout, Duration::from_secs(600));
    }

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
