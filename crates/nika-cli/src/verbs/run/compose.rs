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

use nika_builtin::{BuiltinDispatcher, Emitter, FsBoundary, NoWorkflow, NonInteractive};
use nika_clock::SystemClock;
use nika_exec_runner::TokioShell;
use nika_fs::TokioFs;
use nika_http::{HttpConfig, NetBoundary, ReqwestHttp, SsrfMode};
use nika_kernel::ai::provider::ProviderInferDyn;
use nika_kernel::provider::{InferRequest, InferResponse, ProviderError};
use nika_kernel::secret::Secret;
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_runtime::{Runtime, RuntimeConfig, SecretResolveError, WorkflowSecretResolver};
use nika_schema::types::{SecretRef, SecretSource};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;

/// Surfaces `nika:log` / `nika:emit` onto STDERR (the operator's
/// diagnostic channel · NEVER stdout, which carries the `--json` event
/// stream verbatim).
///
/// WHY stderr and not the `EventLog`: the `Emitter` seam is `&self`
/// (shared · the dispatcher is composed ONCE and shared across every
/// concurrent task), while the runtime's `EventSink` is a `&mut`
/// threaded through the single-threaded settle pass — so a builtin
/// `emit` cannot reach the live stamped stream without a collecting
/// bridge + the reserved `EventKind::Extension` variant (FCI-009 · not
/// yet implemented). Until that lands, stderr is the honest, complete
/// surfacing the spec sanctions (« an event and/or stderr » for log ·
/// best-effort for emit) — NOT a silent no-op (the prior `NullEmitter`
/// dropped both on the floor). See the `production_runtime` doc for the
/// precise remaining wiring.
///
/// Best-effort by contract (spec §log/§emit): a failed stderr write is
/// swallowed — observability never changes the run's verdict.
///
/// `pub` only because it appears in the public [`ProdRuntime`] type
/// spelling (the emitter generic) — not a re-export surface.
#[derive(Debug, Clone, Copy, Default)]
pub struct StderrEmitter;

impl Emitter for StderrEmitter {
    fn emit(&self, kind: &str, payload: serde_json::Value) {
        // Best-effort (spec §log/§emit): `eprintln!` swallows any write
        // error · observability never changes the run's verdict.
        eprintln!("{}", format_emit(kind, &payload));
    }
}

/// Render one `Emitter` event as the stderr line (pure · the side-effect-
/// free core `StderrEmitter` prints). `nika:log` arrives as kind "log"
/// with `{ level, message, data }`; `nika:emit` as kind == the custom
/// `event_type` with its payload. Each is prefixed so an operator greps
/// them apart from the run fold.
fn format_emit(kind: &str, payload: &serde_json::Value) -> String {
    if kind != "log" {
        return format!("nika:emit [{kind}] {payload}");
    }
    let level = payload
        .get("level")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("info");
    // A string message renders verbatim; a non-string (shouldn't happen ·
    // log builds the payload) renders as compact JSON; absent → empty.
    let message = match payload.get("message") {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
        None => String::new(),
    };
    match payload.get("data").filter(|d| !d.is_null()) {
        Some(d) => format!("nika:log [{level}] {message} · {d}"),
        None => format!("nika:log [{level}] {message}"),
    }
}

/// The production dispatcher type — the builtin tool plane over real
/// effects (fs · http · clock · the non-interactive prompter · stderr
/// emitter for log/emit · no nested-workflow surface at v0).
type ProdDispatcher =
    BuiltinDispatcher<TokioFs, ReqwestHttp, SystemClock, StderrEmitter, NonInteractive, NoWorkflow>;

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
///
/// Carries the workflow's `default_model` so the model-LESS
/// `ProviderMeta::supports_response_format` can answer for the model
/// the agent actually runs (an agent run locks ONE model · BUG#11).
#[derive(Debug)]
pub struct RegistryProvider<H> {
    registry: Arc<ProviderRegistry<H>>,
    default_model: String,
}

impl<H> RegistryProvider<H> {
    fn new(registry: Arc<ProviderRegistry<H>>, default_model: impl Into<String>) -> Self {
        Self {
            registry,
            default_model: default_model.into(),
        }
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

impl<H> nika_kernel::ai::provider::ProviderMeta for RegistryProvider<H>
where
    H: Send + Sync,
{
    // The trait signature ties the return to `&self`; this bridge's name is
    // a fixed literal (the model — hence the provider — is per-request, so
    // there is no per-instance name to borrow). The literal is correct.
    #[allow(clippy::unnecessary_literal_bound)]
    fn name(&self) -> &str {
        "registry"
    }

    /// Reports the RESOLVED provider's actual capability (BUG#11
    /// robustness). The agent run locks ONE model — `input.model` or this
    /// `default_model` — so the workflow's `default_model` is the model
    /// the schema re-ask will use in the common path. Delegating to the
    /// registry's keyless wire query means gemini/openai-family agents get
    /// NATIVE structured output (robust · not model-flaky under a tight
    /// budget) while anthropic correctly stays on the instruction fallback
    /// (its wire rejects `response_format`). An empty/unknown default
    /// answers `false` — the safe instruction fallback (a `model:`-less
    /// exec-only workflow never reaches an infer/agent turn anyway).
    fn supports_response_format(&self) -> bool {
        self.registry.supports_response_format(&self.default_model)
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

/// The production workflow-`secrets:` resolver — the `env` + `file` stores
/// (MINOR-B). This is the COMPOSITION ROOT's sanctioned secret-store
/// boundary (the same justification as [`config_from_env`] · the runtime L3
/// never reads env/files itself).
///
/// - `source: env` → reads the OS env var named by the secret's `key`.
/// - `source: file` → reads the file at the secret's `key` path
///   (trailing newline trimmed · the common `cat secret > file` shape).
/// - `source: vault` → NOT yet wired · returns a typed miss so the
///   reference fails closed (NIKA-1702) rather than silently reading null.
///   The checker WARNs about this ahead of run (see `nika check`).
///
/// A resolved value is NEVER logged here (the resolver returns it straight
/// to the runtime's in-memory `secrets` namespace · the IFC governs where it
/// then flows · it is never emitted to the event stream).
///
/// Not `pub` — it is injected as `Arc<dyn WorkflowSecretResolver>` (it never
/// appears in a public type spelling, unlike [`ProdRuntime`]'s generics).
#[derive(Debug, Clone, Copy, Default)]
struct EnvFileSecretResolver;

impl WorkflowSecretResolver for EnvFileSecretResolver {
    fn resolve(&self, name: &str, reference: &SecretRef) -> Result<String, SecretResolveError> {
        let miss = |reason: &str| SecretResolveError {
            name: name.to_owned(),
            reason: reason.to_owned(),
        };
        match reference.source {
            SecretSource::Env => {
                // The sanctioned env→secret boundary (the registry never reads
                // env · same `disallowed_methods` carve-out as config_from_env).
                #[allow(clippy::disallowed_methods)]
                let value = std::env::var(&reference.key)
                    .map_err(|_| miss(&format!("env var `{}` is not set", reference.key)))?;
                if value.is_empty() {
                    return Err(miss(&format!("env var `{}` is empty", reference.key)));
                }
                Ok(value)
            }
            SecretSource::File => {
                let raw = std::fs::read_to_string(&reference.key)
                    .map_err(|e| miss(&format!("file `{}` unreadable: {e}", reference.key)))?;
                let value = raw.trim_end_matches(['\n', '\r']).to_owned();
                if value.is_empty() {
                    return Err(miss(&format!("file `{}` is empty", reference.key)));
                }
                Ok(value)
            }
            // vault is not yet runtime-resolvable (the checker WARNs · the
            // reference then fails closed at NIKA-1702 · never a leak).
            _ => Err(miss("`vault` secrets are not yet runtime-resolvable")),
        }
    }
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

/// Derive the runtime `permits.net.http` boundary from a parsed workflow.
///
/// No `permits:` block → [`NetBoundary::Unbounded`] (today's floor · the fetch
/// SSRF guard is the only net boundary). A `permits:` block WITHOUT a `net:`
/// category → `Declared(vec![])` (default-deny · every host refused · « once
/// `permits:` is present every category is default-deny unless listed »). A
/// `net:` block → `Declared` of its `http:` host globs. The fetch http client
/// enforces this on EVERY redirect hop (`NIKA-SEC-004`) — the runtime half of
/// spec §permits, catching the dynamic hosts (`${{ }}`-built · redirect
/// bounces) the static `nika check` cannot see. Mirrors [`fs_boundary_of`].
#[must_use]
pub fn net_boundary_of(wf: &nika_schema::raw::RawWorkflow) -> NetBoundary {
    let Some(permits) = wf.permits.as_ref().map(|p| &p.value) else {
        return NetBoundary::Unbounded;
    };
    NetBoundary::Declared(
        permits
            .net
            .as_ref()
            .map(|net| net.http.clone())
            .unwrap_or_default(),
    )
}

/// The runtime capability boundary derived from a workflow's `permits:` —
/// BOTH axes (fs + net) in ONE value so a composition root cannot wire one
/// and silently forget the other. Every binary that runs a workflow derives
/// it via [`capabilities_of`] and hands it to [`production_runtime`]; adding a
/// future axis here propagates to all roots at once (structure over discipline
/// · the secure path is the only path).
pub struct RuntimeCapabilities {
    /// `permits.fs` → the file builtins' boundary (`NIKA-SEC-004`).
    pub fs: FsBoundary,
    /// `permits.net.http` → the fetch client's boundary (`NIKA-SEC-004` · per-hop).
    pub net: NetBoundary,
}

/// Derive BOTH runtime capability boundaries from a parsed workflow — the
/// single entry every composition root uses (so net can't be forgotten while
/// fs is wired). Composes [`fs_boundary_of`] + [`net_boundary_of`].
#[must_use]
pub fn capabilities_of(wf: &nika_schema::raw::RawWorkflow) -> RuntimeCapabilities {
    RuntimeCapabilities {
        fs: fs_boundary_of(wf),
        net: net_boundary_of(wf),
    }
}

/// The provider client's transport ceiling — `HttpConfig::timeout` on the
/// PROVIDER client, which reqwest applies as the client-level idle-read
/// guard (armed even while AWAITING RESPONSE HEADERS) and as the fallback
/// total deadline for a request without an explicit one.
///
/// Every buffered provider call sets its OWN total deadline (the task
/// `timeout:` · else the wire layer's per-provider default — 30s cloud ·
/// 300s local), so this ceiling never governs a well-formed call; it only
/// reaps sockets that stopped delivering. It MUST comfortably exceed the
/// longest silent local-model wait (a non-streaming completion delivers
/// ZERO bytes while the model computes — the default 30s guard killed every
/// `timeout: 7m` ollama task at 30s · F1). A task `timeout:` beyond this
/// ceiling is capped by it on a fully-silent connection.
const PROVIDER_TRANSPORT_CEILING: std::time::Duration = std::time::Duration::from_secs(600);

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
/// The config `timeout` is raised to [`PROVIDER_TRANSPORT_CEILING`]: the
/// per-REQUEST deadline is owned by the wire layer (task `timeout:` else
/// the per-provider default), and the 30s client default would undercut
/// any longer budget via the idle-read guard (see the ceiling's doc).
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
    config.timeout = PROVIDER_TRANSPORT_CEILING;
    ReqwestHttp::with_config(config)
}

/// The HTTP client for the FETCH/builtin plane (`nika:fetch` · `nika:notify`).
///
/// SSRF stays ENFORCED (these URLs are workflow-controlled · the engine
/// floor blocks loopback/private/metadata) AND, when the workflow declares
/// a `permits:` block, the `net` boundary (`permits.net.http`) is enforced
/// on every hop — a host outside it fails `NIKA-SEC-004`.
/// [`NetBoundary::Unbounded`] = no declared boundary (the SSRF floor is the
/// only net guard · today's behavior). This is the half the SSRF floor never
/// covered: the workflow's OWN declared host boundary, dynamic hosts included.
// `HttpConfig` is `#[non_exhaustive]` → field assignment, not a struct literal.
#[allow(clippy::field_reassign_with_default)]
fn fetch_http(net: NetBoundary) -> Result<ReqwestHttp, nika_kernel::HttpError> {
    let mut config = HttpConfig::default(); // ssrf: Enforce by default
    config.net = net;
    ReqwestHttp::with_config(config)
}

/// Compose the production runtime for a workflow whose envelope default
/// model is `default_model`, enforcing `caps` — the [`RuntimeCapabilities`]
/// derived from the workflow's `permits:` ([`capabilities_of`]) — at run time:
/// `caps.fs` (`permits.fs`) on the file builtins AND `caps.net`
/// (`permits.net.http`) on the fetch client, both halves of the runtime
/// `NIKA-SEC-004` boundary.
///
/// A workflow with no `permits:` block yields the pre-permits floor
/// ([`FsBoundary::unbounded`] + [`NetBoundary::Unbounded`] · enforce nothing);
/// a declared boundary makes a `..`/symlink path or an out-of-allowlist host
/// fail `NIKA-SEC-004` (spec §permits · enforced statically AND at runtime).
/// Taking the whole `caps` (not the two axes separately) is deliberate — a
/// caller cannot wire fs and forget net.
///
/// # `nika:log` / `nika:emit` — observability wiring (the remaining gap)
///
/// Today both surface on STDERR via `StderrEmitter` (observable · not the
/// prior silent no-op). Surfacing them on the `run --json` EVENT STREAM
/// (so a tailing agent sees `nika:emit`'s custom event inline with the
/// stamped frames) is deeper, and needs THREE pieces that don't exist yet:
///
/// 1. **`EventKind::Extension { ns, name, payload }`** — the FCI-009
///    reserved variant (`docs/architecture/forward-compat-invariants.md`
///    Decision 1) is NOT yet implemented in `nika-event`. Adding it is
///    additive (the enum is `#[non_exhaustive]`) but touches `as_str`
///    (Extension has no `&'static str` name → return a `"extension"`
///    discriminator + carry the real name in the event `fields`), `class`,
///    and the `nika-cli` `display::{render,state}` folds (exhaustive
///    matches today).
/// 2. **A collecting `Emitter` bridge** — `Arc<Mutex<Vec<(String, Value)>>>`
///    injected here in place of `StderrEmitter`; the run verb holds the
///    same `Arc`. (The `Emitter` seam is `&self`/shared because the
///    dispatcher is composed ONCE for the whole run; the `EventSink` is a
///    `&mut` threaded through the single-threaded settle pass — the two
///    cannot meet without this bridge OR a per-task buffer like
///    `agent_events`.)
/// 3. **A drain at settle** — fold the buffered `(name, payload)` into
///    `sink.emit(Extension …)` either per-task (true ordering · the
///    `BufferingObserver`/`agent_events` precedent) or once post-run
///    (simpler · spec says emit/log delivery is best-effort, so strict
///    interleaving is not required).
///
/// Until that lands, stderr is the honest surfacing the spec sanctions
/// (« an event and/or stderr » for `log` · best-effort for `emit`).
///
/// # Errors
///
/// [`ReqwestHttp`] construction can fail if the TLS backend won't
/// initialize (a `nika_kernel::HttpError`) — the run verb maps it to the
/// environment exit code (3).
pub fn production_runtime(
    default_model: &str,
    caps: RuntimeCapabilities,
) -> Result<ProdRuntime, nika_kernel::HttpError> {
    // The fetch/builtin client enforces SSRF (workflow URLs) AND the
    // declared permits.net.http boundary (NIKA-SEC-004 · per-hop).
    let http = Arc::new(fetch_http(caps.net)?);
    // The provider path gets its OWN client (SSRF disabled · see the
    // `provider_http` doc): the fetch/builtin plane below keeps `http`
    // (SSRF enforced · workflow-controlled URLs).
    let provider_http = Arc::new(provider_http()?);
    let config = config_from_env();
    // The provider API-key env-var names the engine reads for its OWN inference
    // calls — scrubbed from every exec child's ambient environment so an
    // untrusted command cannot exfiltrate them (ADR-095 Layer 3). A workflow
    // that needs a key in its child still sets it explicitly in `env:`.
    let provider_secret_env: Vec<String> = nika_catalog::all_providers()
        .iter()
        .filter(|p| !p.env_var.is_empty())
        .map(|p| p.env_var.to_string())
        .collect();

    // The builtin tool plane (invoke + the agent's tools) over real
    // effects · shared by InvokeVerb and the agent's tool-defs seam. The
    // file builtins enforce the declared permits.fs boundary (NIKA-SEC-004).
    let dispatcher: Arc<ProdDispatcher> = Arc::new(
        BuiltinDispatcher::new(
            Arc::new(TokioFs),
            Arc::clone(&http),
            Arc::new(SystemClock),
            // log/emit → stderr (observable · NOT a silent no-op). The
            // `run --json` event-stream integration is the deeper wiring
            // documented on this fn.
            Arc::new(StderrEmitter),
            Arc::new(NonInteractive::default()),
            Arc::new(NoWorkflow::default()),
        )
        .with_fs_boundary(caps.fs),
    );
    let invoke = Arc::new(InvokeVerb::new(Arc::clone(&dispatcher)));

    // The provider registry (real http + env keys) drives infer directly
    // and the agent via the per-call RegistryProvider bridge.
    let registry = Arc::new(ProviderRegistry::new(provider_http, config));
    let agent_provider = Arc::new(RegistryProvider::new(Arc::clone(&registry), default_model));

    Ok(Runtime::new(
        ExecVerb::new(Arc::new(
            TokioShell::new().with_ambient_secret_env(provider_secret_env),
        )),
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
    )
    // Resolve `secrets:` from env/file at run start (MINOR-B · the sanctioned
    // store boundary). A miss leaves the secret unbound → NIKA-1702 (fail-
    // closed); the IFC governs where a resolved value may flow.
    .with_secret_resolver(Arc::new(EnvFileSecretResolver)))
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    fn parse(yaml: &str) -> nika_schema::raw::RawWorkflow {
        nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses")
    }

    #[test]
    fn net_boundary_of_three_cases() {
        // The check↔runtime derivation contract (spec §permits default-deny),
        // the net companion to fs_boundary_of — asserted at the compose layer.
        // (a) no permits block → Unbounded (the SSRF floor is the only guard).
        assert_eq!(
            net_boundary_of(&parse(
                "nika: v1\nworkflow: w\ntasks:\n  - id: t\n    exec: { command: \"echo hi\" }\n"
            )),
            NetBoundary::Unbounded
        );
        // (b) permits present but NO net category → Declared([]) = deny-all.
        assert_eq!(
            net_boundary_of(&parse(
                "nika: v1\nworkflow: w\npermits:\n  tools: [\"nika:jq\"]\ntasks:\n  - id: t\n    invoke: { tool: \"nika:jq\", args: { input: {}, expression: \".\" } }\n"
            )),
            NetBoundary::Declared(Vec::new())
        );
        // (c) net.http present → Declared([globs]).
        assert_eq!(
            net_boundary_of(&parse(
                "nika: v1\nworkflow: w\npermits:\n  net: { http: [\"api.example.com\", \"*.github.com\"] }\n  tools: [\"nika:fetch\"]\ntasks:\n  - id: t\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://api.example.com/\" } }\n"
            )),
            NetBoundary::Declared(vec![
                "api.example.com".to_owned(),
                "*.github.com".to_owned()
            ])
        );
    }

    #[test]
    fn capabilities_of_bundles_both_axes() {
        // The single derivation a composition root uses — both axes from one
        // workflow (so net can't be forgotten while fs is wired).
        let caps = capabilities_of(&parse(
            "nika: v1\nworkflow: w\npermits:\n  net: { http: [\"api.example.com\"] }\n  fs: { write: [\"./out/**\"] }\n  tools: [\"nika:fetch\"]\ntasks:\n  - id: t\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://api.example.com/\" } }\n",
        ));
        assert_eq!(
            caps.net,
            NetBoundary::Declared(vec!["api.example.com".to_owned()])
        );
        // fs is the declared boundary (not unbounded) — both axes derived.
        assert_ne!(
            format!("{:?}", caps.fs),
            format!("{:?}", FsBoundary::unbounded()),
            "fs is a declared boundary when permits.fs is present"
        );
    }

    #[test]
    fn composition_succeeds_for_a_mock_model() {
        // The mock profile needs no http call + no key — composition
        // wires every seam without touching the network. (A real model's
        // missing key surfaces only at resolve time · per-workflow.)
        let runtime = production_runtime(
            "mock/echo",
            RuntimeCapabilities {
                fs: FsBoundary::unbounded(),
                net: NetBoundary::Unbounded,
            },
        );
        assert!(
            runtime.is_ok(),
            "the production runtime composes (TLS init is the only failure)"
        );
    }

    #[test]
    fn registry_provider_reports_the_resolved_models_capability() {
        // BUG#11 robustness: the per-call bridge reports its `default_model`'s
        // ACTUAL wire capability (was hardcoded false). gemini/openai-family
        // → native structured output (robust under a tight budget) · anthropic
        // → false (the instruction fallback its wire requires).
        use nika_kernel::ai::provider::ProviderMeta;

        let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::new()));
        let gemini = RegistryProvider::new(Arc::clone(&registry), "gemini/flash");
        assert!(gemini.supports_response_format(), "gemini → native");
        let openai = RegistryProvider::new(Arc::clone(&registry), "openai/gpt-4o");
        assert!(openai.supports_response_format(), "openai → native");
        let anthropic = RegistryProvider::new(Arc::clone(&registry), "anthropic/sonnet");
        assert!(
            !anthropic.supports_response_format(),
            "anthropic → instruction fallback (wire rejects response_format)"
        );
        // An exec-only workflow has no envelope model → safe fallback.
        let none = RegistryProvider::new(registry, "");
        assert!(!none.supports_response_format(), "no model → safe false");
    }

    #[test]
    fn stderr_emitter_renders_log_and_emit_distinctly() {
        // F4: log/emit are OBSERVABLE (not the prior silent NullEmitter).
        // The pure formatter is the contract the StderrEmitter prints.

        // nika:log → level + message (+ data when present).
        let log = format_emit(
            "log",
            &serde_json::json!({ "level": "warn", "message": "disk low", "data": null }),
        );
        assert_eq!(log, "nika:log [warn] disk low");
        let log_data = format_emit(
            "log",
            &serde_json::json!({ "level": "info", "message": "hi", "data": { "n": 1 } }),
        );
        assert_eq!(log_data, r#"nika:log [info] hi · {"n":1}"#);
        // a missing level defaults to info (matches the builtin's clamp).
        let log_default = format_emit("log", &serde_json::json!({ "message": "x" }));
        assert_eq!(log_default, "nika:log [info] x");

        // nika:emit → the custom event_type as kind + its payload verbatim.
        let emit = format_emit("deploy.started", &serde_json::json!({ "version": "1.2.3" }));
        assert_eq!(emit, r#"nika:emit [deploy.started] {"version":"1.2.3"}"#);
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

    /// The production secret resolver reads BOTH stores it claims (spec
    /// §secrets) — `env` from the OS environment, `file` from disk (newline
    /// trimmed). Tested against real sources (PATH · a temp file) so no racy
    /// `set_var` is needed; a deleted `Env`/`File` arm or an `Ok("…")` constant
    /// returns the wrong value and fails the equality.
    #[test]
    fn secret_resolver_reads_env_and_file_stores() {
        let resolver = EnvFileSecretResolver;
        // env: PATH is always present + non-empty → resolves to its value.
        // (the same sanctioned env→secret boundary the resolver itself carries)
        #[allow(clippy::disallowed_methods)]
        let path_value = std::env::var("PATH").expect("PATH set");
        let got = resolver
            .resolve("p", &SecretRef::new(SecretSource::Env, "PATH"))
            .expect("env secret resolves");
        assert_eq!(got, path_value);
        // file: a temp file whose trailing newline is trimmed.
        let dir = std::env::temp_dir().join("nika-cli-killtests");
        std::fs::create_dir_all(&dir).expect("tmp dir");
        let secret_file = dir.join("secret.txt");
        std::fs::write(&secret_file, "s3cr3t\n").expect("write secret");
        let got = resolver
            .resolve(
                "f",
                &SecretRef::new(SecretSource::File, secret_file.to_str().expect("utf8")),
            )
            .expect("file secret resolves");
        assert_eq!(got, "s3cr3t");
    }
}
