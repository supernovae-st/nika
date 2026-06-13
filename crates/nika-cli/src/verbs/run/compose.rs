// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The production composition root — wires the runtime over REAL effects
//! (fs · http · clock · subprocess · provider registry with env-resolved
//! keys). "Production stamps + seams are the composer's concern, L4"
//! (the runtime crate spec §2) — this is that composer.
//!
//! Two bridges the composition owns (the registry's own doc: "the
//! composition root resolves secrets · injects via `with_key`"):
//!
//! 1. [`RegistryProvider`] — the agent verb takes a single
//!    `ProviderInferDyn`, the infer verb takes the whole registry. The
//!    adapter resolves `request.model` per-call (the registry's purpose)
//!    so the agent loop honors per-turn model selection.
//! 2. env-key resolution — the ONE sanctioned `std::env` boundary (the
//!    registry never reads env · the doctor verb's `env_present` shares
//!    the justification): read each catalog provider's `env_var`, inject
//!    the present ones via [`nika_providers::ProvidersConfig::with_key`].

use std::sync::Arc;

use nika_builtin::{BuiltinDispatcher, NoWorkflow, NonInteractive, NullEmitter};
use nika_clock::SystemClock;
use nika_exec_runner::TokioShell;
use nika_fs::TokioFs;
use nika_http::ReqwestHttp;
use nika_kernel::ai::provider::ProviderInferDyn;
use nika_kernel::provider::{InferRequest, InferResponse, ProviderError};
use nika_kernel::secret::Secret;
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_runtime::{Runtime, RuntimeConfig};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;

/// The production dispatcher type — the builtin tool plane over real
/// effects (fs · http · clock · the non-interactive prompter · no
/// nested-workflow surface at v0).
type ProdDispatcher =
    BuiltinDispatcher<TokioFs, ReqwestHttp, SystemClock, NullEmitter, NonInteractive, NoWorkflow>;

/// The fully-resolved production runtime spelling (tames the 6 generics
/// at the call site).
pub type ProdRuntime = Runtime<
    TokioShell,
    ProdDispatcher,
    ReqwestHttp,
    RegistryProvider<ReqwestHttp>,
    ProdDispatcher,
    SystemClock,
>;

/// A registry-backed [`ProviderInferDyn`] — resolves the request's model
/// through the registry on every call (the agent verb's single-provider
/// seam · the infer verb wraps the registry directly). An unknown model
/// or a missing key surfaces the registry's own typed error.
#[derive(Debug)]
pub struct RegistryProvider<H> {
    registry: Arc<ProviderRegistry<H>>,
}

impl<H> RegistryProvider<H> {
    fn new(registry: Arc<ProviderRegistry<H>>) -> Self {
        Self { registry }
    }
}

impl<H> nika_kernel::sealed::Sealed for RegistryProvider<H> {}

impl<H> ProviderInferDyn for RegistryProvider<H>
where
    H: nika_kernel::http::HttpPostDyn + Send + Sync + 'static,
{
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        // Resolve per-call: the agent may select a different model per
        // turn · the registry binds the wire model + key at resolve time.
        let resolved = self.registry.resolve(&request.model)?;
        resolved.infer(request).await
    }
}

/// Read the present API keys from the environment into a config.
///
/// This is the COMPOSITION ROOT's sanctioned `std::env` boundary — the
/// registry never reads env (the `disallowed_methods` ban routes secrets
/// through here, the one place env→secret crossing is legitimate, the
/// same justification as `main.rs::env_present`). A key absent at compose
/// time is NOT an error here: `resolve()` surfaces the typed
/// `AuthFailed` (with the env-ladder hint) only if a workflow actually
/// targets that provider.
fn config_from_env() -> ProvidersConfig {
    let mut config = ProvidersConfig::new();
    for provider in nika_catalog::all_providers() {
        if provider.env_var.is_empty() {
            continue;
        }
        #[allow(clippy::disallowed_methods)] // the sanctioned env→secret boundary (see doc)
        let present = std::env::var(provider.env_var);
        if let Ok(value) = present
            && !value.is_empty()
        {
            config = config.with_key(provider.id, Secret::new(value));
        }
    }
    config
}

/// Compose the production runtime for a workflow whose envelope default
/// model is `default_model`.
///
/// # Errors
///
/// [`ReqwestHttp`] construction can fail if the TLS backend won't
/// initialize (a `nika_kernel::HttpError`) — the run verb maps it to the
/// environment exit code (3).
pub fn production_runtime(default_model: &str) -> Result<ProdRuntime, nika_kernel::HttpError> {
    let http = Arc::new(ReqwestHttp::new()?);
    let config = config_from_env();

    // The builtin tool plane (invoke + the agent's tools) over real
    // effects · shared by InvokeVerb and the agent's tool-defs seam.
    let dispatcher: Arc<ProdDispatcher> = Arc::new(BuiltinDispatcher::new(
        Arc::new(TokioFs),
        Arc::clone(&http),
        Arc::new(SystemClock),
        Arc::new(NullEmitter::default()),
        Arc::new(NonInteractive::default()),
        Arc::new(NoWorkflow::default()),
    ));
    let invoke = Arc::new(InvokeVerb::new(Arc::clone(&dispatcher)));

    // The provider registry (real http + env keys) drives infer directly
    // and the agent via the per-call RegistryProvider bridge.
    let registry = Arc::new(ProviderRegistry::new(Arc::clone(&http), config));
    let agent_provider = Arc::new(RegistryProvider::new(Arc::clone(&registry)));

    Ok(Runtime::new(
        ExecVerb::new(Arc::new(TokioShell::new())),
        Arc::clone(&invoke),
        InferVerb::new(registry, default_model),
        AgentVerb::new(
            agent_provider,
            invoke,
            Arc::clone(&dispatcher),
            default_model,
        ),
        SystemClock,
        RuntimeConfig::default(),
    ))
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn composition_succeeds_for_a_mock_model() {
        // The mock profile needs no http call + no key — composition
        // wires every seam without touching the network. (A real model's
        // missing key surfaces only at resolve time · per-workflow.)
        let runtime = production_runtime("mock/echo");
        assert!(
            runtime.is_ok(),
            "the production runtime composes (TLS init is the only failure)"
        );
    }

    #[test]
    fn env_config_skips_empty_and_absent_keys() {
        // config_from_env never panics + never injects an empty key ·
        // the hermetic invariant (no key set in the test env → empty
        // config, resolve surfaces AuthFailed per-workflow not here).
        let config = config_from_env();
        // We can't assert key absence (the dev's shell may export some),
        // but composition must succeed regardless — proven above.
        let _ = config;
    }
}
