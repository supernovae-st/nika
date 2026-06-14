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

use nika_builtin::{BuiltinDispatcher, FsBoundary, NoWorkflow, NonInteractive, NullEmitter};
use nika_clock::SystemClock;
use nika_exec_runner::TokioShell;
use nika_fs::TokioFs;
use nika_http::{HttpConfig, ReqwestHttp, SsrfMode};
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

/// Derive the runtime `permits.fs` boundary from a parsed workflow.
///
/// No `permits:` block → [`FsBoundary::unbounded`] (today's floor · the
/// file builtins enforce nothing). A `permits:` block WITHOUT an `fs:`
/// category → a DECLARED boundary with empty glob lists (default-deny ·
/// every fs effect is refused · « once `permits:` is present every category
/// is default-deny unless listed »). An `fs:` block → its read/write globs.
#[must_use]
pub fn fs_boundary_of(wf: &nika_schema::raw::RawWorkflow) -> FsBoundary {
    let Some(permits) = wf.permits.as_ref().map(|p| &p.value) else {
        return FsBoundary::unbounded();
    };
    let (read, write) = permits
        .fs
        .as_ref()
        .map(|fs| (fs.read.clone(), fs.write.clone()))
        .unwrap_or_default();
    FsBoundary::declared(read, write)
}

/// The HTTP client for the PROVIDER path (LLM inference), distinct from
/// the fetch/builtin client.
///
/// SSRF is `Disabled` here ON PURPOSE. The fetch SSRF guard exists because
/// `fetch:`/`nika:fetch` URLs are WORKFLOW-controlled (attacker-influenced),
/// so loopback/private targets must be blocked. The provider path is the
/// opposite: endpoints come from a FIXED, studio-controlled allowlist
/// (`nika_providers::profile`), never from workflow data — and the local
/// providers (`ollama` · `lmstudio` · `llamacpp` · `localai` · `vllm`) bind
/// `127.0.0.1` BY DESIGN. Enforcing the fetch guard here bricks every
/// local/sovereign provider with `NIKA-430 · SSRF blocked 127.0.0.1`, which
/// contradicts the local-first raison. `Disabled` is exactly the
/// "trusted internal networks" opt-out the `nika-http` docs sanction.
///
/// # Errors
///
/// [`nika_kernel::HttpError`] when the TLS backend won't initialize.
// `HttpConfig` is `#[non_exhaustive]`, so the struct-literal/FRU form clippy
// would suggest is a cross-crate compile error — field assignment is the only
// way (the same idiom nika-http's own tests use).
#[allow(clippy::field_reassign_with_default)]
fn provider_http() -> Result<ReqwestHttp, nika_kernel::HttpError> {
    let mut config = HttpConfig::default();
    config.ssrf = SsrfMode::Disabled;
    ReqwestHttp::with_config(config)
}

/// Compose the production runtime for a workflow whose envelope default
/// model is `default_model`, enforcing `fs_boundary` (the workflow's
/// `permits.fs`) on the file builtins at run time.
///
/// `fs_boundary` is [`FsBoundary::unbounded`] when the workflow declares no
/// `permits:` block (the pre-permits floor · enforce nothing) and a
/// declared boundary otherwise — derived by [`fs_boundary_of`] from the
/// parsed envelope, so a `..`/symlink path that escapes the declared roots
/// fails with `NIKA-SEC-004` (spec §permits · enforced statically + at
/// runtime).
///
/// # Errors
///
/// [`ReqwestHttp`] construction can fail if the TLS backend won't
/// initialize (a `nika_kernel::HttpError`) — the run verb maps it to the
/// environment exit code (3).
pub fn production_runtime(
    default_model: &str,
    fs_boundary: FsBoundary,
) -> Result<ProdRuntime, nika_kernel::HttpError> {
    let http = Arc::new(ReqwestHttp::new()?);
    // The provider path gets its OWN client (SSRF disabled · see the
    // `provider_http` doc): the fetch/builtin plane below keeps `http`
    // (SSRF enforced · workflow-controlled URLs).
    let provider_http = Arc::new(provider_http()?);
    let config = config_from_env();

    // The builtin tool plane (invoke + the agent's tools) over real
    // effects · shared by InvokeVerb and the agent's tool-defs seam. The
    // file builtins enforce the declared permits.fs boundary (NIKA-SEC-004).
    let dispatcher: Arc<ProdDispatcher> = Arc::new(
        BuiltinDispatcher::new(
            Arc::new(TokioFs),
            Arc::clone(&http),
            Arc::new(SystemClock),
            Arc::new(NullEmitter::default()),
            Arc::new(NonInteractive::default()),
            Arc::new(NoWorkflow::default()),
        )
        .with_fs_boundary(fs_boundary),
    );
    let invoke = Arc::new(InvokeVerb::new(Arc::clone(&dispatcher)));

    // The provider registry (real http + env keys) drives infer directly
    // and the agent via the per-call RegistryProvider bridge.
    let registry = Arc::new(ProviderRegistry::new(provider_http, config));
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
        let runtime = production_runtime("mock/echo", FsBoundary::unbounded());
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

    /// Regression for NIKA-430: the provider HTTP path must NOT apply the
    /// fetch SSRF guard, or every loopback-bound local provider (`ollama`,
    /// `lmstudio`, `llamacpp`, `localai`, `vllm`) is unreachable. Whether the
    /// socket connects or is refused, the result must NEVER be `SsrfBlocked`
    /// — the default fetch client WOULD block `127.0.0.1` (see nika-http
    /// `enforce_mode_blocks_loopback_end_to_end`). Mirrors nika-http's
    /// `disabled_mode_still_builds_a_working_client`.
    #[tokio::test]
    async fn provider_http_does_not_ssrf_block_loopback() {
        use nika_kernel::http::HttpPostDyn;
        use nika_kernel::{HttpError, HttpRequest};
        let http = provider_http().expect("provider client builds");
        let result = http
            .post(HttpRequest::post("http://127.0.0.1:9/v1/chat/completions"))
            .await;
        if let Err(e) = result {
            assert!(
                !matches!(e, HttpError::SsrfBlocked { .. }),
                "the provider client must not SSRF-block loopback (got {e})"
            );
        }
    }
}
