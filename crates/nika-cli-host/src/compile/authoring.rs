// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Opt-in host wiring only. Semantics and emission remain in the shared compiler.
//! `--authoring-model` seats one generative provider (COLD); `--decision-model`
//! seats one bounded-choice capability (WARM): `typesafe/<jev>` through System
//! One, any other `provider/name` through a closed JSON-schema enum.
use nika_onboard::compile::{
    AuthoringPolicy, Cognition, CompileOutcome, CompileRequest, NoProvider, compile_with_cognition,
    decide::{DecisionSeat, ProviderChoice},
};
use std::{sync::Arc, time::Duration};

pub(super) fn compile(
    request: &CompileRequest,
    args: &super::CompileArgs,
) -> Result<CompileOutcome, String> {
    let max_tokens = args.authoring_max_tokens.unwrap_or(2048);
    let timeout = args.authoring_timeout.unwrap_or(30);
    if !(1..=8192).contains(&max_tokens) || !(1..=120).contains(&timeout) {
        return Err(
            "Authoring limits must be 1..8192 output tokens and 1..120 seconds.".to_owned(),
        );
    }
    let request = match args.authoring_model.as_deref() {
        Some(model) => request.clone().with_authoring_policy(AuthoringPolicy::new(
            model,
            max_tokens,
            Duration::from_secs(timeout),
        )),
        None => request.clone(),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    runtime.block_on(async {
        // Reuse the established environment/key/endpoint ladder only AFTER explicit opt-in.
        // This does not probe a keychain, select a provider, or resolve business credentials.
        let http = nika_http::ReqwestHttp::new().map_err(|e| e.to_string())?;
        let registry = nika_providers::ProviderRegistry::new(
            Arc::new(http),
            nika_runtime::compose::config_from_env(),
        );
        let provider = match args.authoring_model.as_deref() {
            Some(model) => Some(registry.resolve(model).map_err(|e| e.to_string())?),
            None => None,
        };
        let decision_provider = match args.decision_model.as_deref() {
            Some(model) if !model.starts_with("typesafe/") => {
                Some(registry.resolve(model).map_err(|e| e.to_string())?)
            }
            _ => None,
        };
        let typesafe = match args.decision_model.as_deref() {
            Some(model) if model.starts_with("typesafe/") => {
                #[allow(clippy::disallowed_methods)]
                // the sanctioned env→secret boundary for an explicitly named seat (compose.rs precedent)
                let key = std::env::var("TYPESAFE_API_KEY").map_err(|_| {
                    "TYPESAFE_API_KEY is required for a typesafe decision seat".to_owned()
                })?;
                Some(super::typesafe::TypesafeSeat::new(
                    key,
                    model.trim_start_matches("typesafe/"),
                )?)
            }
            _ => None,
        };
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
        match provider.as_ref() {
            Some(provider) => {
                compile_with_cognition(
                    &request,
                    Cognition {
                        provider: Some(provider),
                        seat,
                    },
                )
                .await
            }
            None => {
                compile_with_cognition::<NoProvider>(
                    &request,
                    Cognition {
                        provider: None,
                        seat,
                    },
                )
                .await
            }
        }
        .map_err(|e| e.to_string())
    })
}
