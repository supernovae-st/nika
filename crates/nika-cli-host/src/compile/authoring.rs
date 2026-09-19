// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Opt-in host wiring only. Semantics and emission remain in the shared compiler.
use nika_onboard::compile::{
    AuthoringPolicy, CompileOutcome, CompileRequest, compile_with_provider,
};
use std::{sync::Arc, time::Duration};

pub(super) fn compile(
    request: &CompileRequest,
    args: &super::CompileArgs,
) -> Result<CompileOutcome, String> {
    let model = args
        .authoring_model
        .as_deref()
        .ok_or("Explicit authoring model is required")?;
    let max_tokens = args.authoring_max_tokens.unwrap_or(2048);
    let timeout = args.authoring_timeout.unwrap_or(30);
    if !(1..=8192).contains(&max_tokens) || !(1..=120).contains(&timeout) {
        return Err(
            "Authoring limits must be 1..8192 output tokens and 1..120 seconds.".to_owned(),
        );
    }
    let request = request.clone().with_authoring_policy(AuthoringPolicy::new(
        model,
        max_tokens,
        Duration::from_secs(timeout),
    ));
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
        let provider = registry.resolve(model).map_err(|e| e.to_string())?;
        compile_with_provider(&request, &provider)
            .await
            .map_err(|e| e.to_string())
    })
}
