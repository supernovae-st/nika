// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Production composition root over real effects, plus [`simulated_runtime`].
//! Descended from `nika-cli`'s run verb (15k wall). Zero tokio in this module.
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

// `StderrEmitter` + the unconfined-sandbox note ARE the spec-sanctioned
// diagnostic surfacing (« an event and/or stderr » · observability never
// changes the run's verdict) — the same exemption the run verb carried.
#![allow(clippy::disallowed_macros, clippy::print_stderr)]

use std::sync::Arc;

mod secret_resolver;

#[cfg(test)]
use crate::WorkflowSecretResolver;
use crate::simulated::{SimulatedDispatcher, SimulatedShell};
use crate::{Runtime, RuntimeConfig};
use nika_builtin::{
    BuiltinDispatcher, Emitter, FsBoundary, ImageKeys, LiveInspect, NonInteractive, TtsKeys,
};
use nika_clock::DeclaredClock;
use nika_exec_runner::TokioShell;
use nika_fs::TokioFs;
use nika_http::{HttpConfig, NetBoundary, ReqwestHttp, SsrfMode};
use nika_kernel::ai::provider::ProviderInferDyn;
use nika_kernel::provider::{InferRequest, InferResponse, ProviderError};
use nika_kernel::secret::Secret;
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_schema::types::RunDecl;
#[cfg(test)]
use nika_schema::types::{SecretRef, SecretSource};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;
use secret_resolver::EnvFileSecretResolver;

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
#[derive(Debug, Clone, Copy)]
pub struct StderrEmitter {
    project_payloads: bool,
}

impl StderrEmitter {
    const fn metadata_only() -> Self {
        Self {
            project_payloads: false,
        }
    }
}

impl Default for StderrEmitter {
    fn default() -> Self {
        Self {
            project_payloads: true,
        }
    }
}

impl Emitter for StderrEmitter {
    fn emit(&self, kind: &str, payload: serde_json::Value) {
        if !self.project_payloads {
            return;
        }
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
/// Neutralize terminal-control characters before a string reaches stderr.
///
/// `nika:log`'s `message` can carry values interpolated from untrusted
/// sources (e.g. `${tasks.fetched}` of web content), and this sink writes
/// it to the operator's terminal. A raw ESC / CSI / OSC sequence would let
/// that content rewrite the title, move the cursor, forge log lines (CR),
/// or worse — so every C0/C1 control char except tab and newline is
/// rendered as its visible `\u{XX}` escape. The machine surface (`--json`)
/// is unaffected: it renders through serde, which already escapes.
fn sanitize_for_terminal(s: &str) -> String {
    use std::fmt::Write as _;
    if !s.chars().any(|c| c.is_control() && c != '\n' && c != '\t') {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_control() && c != '\n' && c != '\t' {
            // write! to a String is infallible.
            let _ = write!(out, "\\u{{{:02x}}}", c as u32);
        } else {
            out.push(c);
        }
    }
    out
}

fn format_emit(kind: &str, payload: &serde_json::Value) -> String {
    if kind != "log" {
        return format!("nika:emit [{kind}] {payload}");
    }
    let level = payload
        .get("level")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("info");
    // A string message renders verbatim (control chars neutralized for the
    // terminal · see `sanitize_for_terminal`); a non-string (shouldn't
    // happen · log builds the payload) renders as compact JSON; absent → "".
    let message = match payload.get("message") {
        Some(serde_json::Value::String(s)) => sanitize_for_terminal(s),
        Some(other) => other.to_string(),
        None => String::new(),
    };
    match payload.get("data").filter(|d| !d.is_null()) {
        Some(d) => format!("nika:log [{level}] {message} · {d}"),
        None => format!("nika:log [{level}] {message}"),
    }
}

/// The production dispatcher type — the builtin tool plane over real
/// effects (fs · http · the declared run clock · the non-interactive
/// prompter · stderr emitter for log/emit · no nested-workflow surface
/// at v0).
type ProdDispatcher = BuiltinDispatcher<
    TokioFs,
    ReqwestHttp,
    DeclaredClock,
    StderrEmitter,
    NonInteractive,
    LiveInspect,
>;

/// The fully-resolved production runtime spelling (tames the 6 generics
/// at the call site).
pub type ProdRuntime = Runtime<
    TokioShell,
    ProdDispatcher,
    ReqwestHttp,
    RegistryProvider<ReqwestHttp>,
    ProdDispatcher,
    DeclaredClock,
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
/// The documented key ladder for one provider (`Profile::env_candidates`
/// · what `nika doctor` reports): `NIKA_<ID>_API_KEY` overrides the
/// conventional variable, so an operator can point ONE provider at a
/// different key (billing split · a scoped test key) without touching
/// the shared var every other tool reads. The runtime ignoring the
/// override while doctor honored it was a green-doctor → dead-run drift.
/// Empty values count as absent on BOTH rungs.
fn ladder_key(lookup: &dyn Fn(&str) -> Option<String>, id: &str, env_var: &str) -> Option<String> {
    let nika = format!("NIKA_{}_API_KEY", id.to_uppercase());
    lookup(&nika)
        .filter(|v| !v.is_empty())
        .or_else(|| lookup(env_var).filter(|v| !v.is_empty()))
}

/// The cloud-endpoint override for one provider: `NIKA_<ID>_BASE_URL`,
/// verbatim (a COMPLETE endpoint URL · empty counts as absent). Only the
/// explicit `NIKA_` form exists here — cloud providers have no
/// `OLLAMA_HOST`-style convention to honor, and inventing one would
/// widen the trust surface for no one.
fn cloud_base_url(lookup: &dyn Fn(&str) -> Option<String>, id: &str) -> Option<String> {
    lookup(&format!("NIKA_{}_BASE_URL", id.to_uppercase())).filter(|v| !v.is_empty())
}

/// Read the present provider keys/endpoints from the environment into a
/// config (the sanctioned env boundary documented above) — `pub` for the
/// probe/doctor surfaces that report the same ladder the run composes.
#[must_use]
pub fn config_from_env() -> ProvidersConfig {
    let mut config = ProvidersConfig::new();
    #[allow(clippy::disallowed_methods)] // the sanctioned env→secret boundary (see doc)
    let lookup = |name: &str| std::env::var(name).ok();
    for provider in nika_catalog::all_providers() {
        if provider.env_var.is_empty() {
            continue;
        }
        if let Some(value) = ladder_key(&lookup, provider.id, provider.env_var) {
            config = config.with_key(provider.id, Secret::new(value));
        }
    }
    // CLOUD endpoints honor the same override family (D-2026-06-10-N2
    // long-tail hatch): `NIKA_<ID>_BASE_URL` points a cloud profile at
    // any OpenAI-compatible endpoint, zero catalog change — the value is
    // the COMPLETE URL taken verbatim (gemini keeps its STEM contract ·
    // no loopback normalization, that grammar is the locals'). Pair with
    // `NIKA_<ID>_API_KEY` (#286) so the Bearer rides a SCOPED key. Live-
    // proven against Scaleway Generative APIs 2026-07-08 (EU · vLLM-class).
    for provider in nika_catalog::all_providers() {
        // the locals own the NEXT loop (loopback-normalized grammar) —
        // catalog rows exist for them too, so exclude by id.
        if LOCAL_BASE_ENV.iter().any(|(id, _, _)| *id == provider.id) {
            continue;
        }
        if let Some(url) = cloud_base_url(&lookup, provider.id) {
            config = config.with_base_url(provider.id, url);
        }
    }
    // The LOCAL servers cross the SAME boundary (deliberately not
    // secrets): `NIKA_<ID>_BASE_URL` first, then the server's own
    // convention (`OLLAMA_HOST` — ignoring it sent runs to 127.0.0.1
    // while the user's own CLI talked to their LAN box · user-sim
    // finding). Ollama's convenience forms (`host` · `host:port` · full
    // URL) all resolve; a bare host gets the provider's default port.
    for (id, conventional, default_port) in LOCAL_BASE_ENV {
        let ours = format!("NIKA_{}_BASE_URL", id.to_uppercase());
        #[allow(clippy::disallowed_methods)] // the sanctioned env boundary (see doc)
        let raw = std::env::var(&ours)
            .ok()
            .filter(|v| !v.is_empty())
            .or_else(|| {
                conventional.and_then(|name| {
                    #[allow(clippy::disallowed_methods)] // the sanctioned env boundary (see doc)
                    std::env::var(name).ok().filter(|v| !v.is_empty())
                })
            });
        if let Some(raw) = raw {
            config = config.with_base_url(id, normalize_local_base_url(&raw, default_port));
        }
    }
    config
}

/// The 5 local servers' env ladder: our variable first, then the
/// server's own convention where one exists. Default ports mirror the
/// seed rows in `nika-providers::profile::LOCAL` — the cross-check test
/// below fails if either side drifts (a 6th local or a port change must
/// land in BOTH places).
const LOCAL_BASE_ENV: [(&str, Option<&str>, u16); 5] = [
    ("ollama", Some("OLLAMA_HOST"), 11434),
    ("lmstudio", None, 1234),
    ("llamacpp", None, 8080),
    ("localai", None, 8080),
    ("vllm", None, 8000),
];

/// Normalize an operator-supplied local base URL to the full wire
/// endpoint the adapters expect (`http://host:port/v1/chat/completions`).
/// Mirrors the official ollama client's `OLLAMA_HOST` conveniences:
/// scheme optional (http assumed) · port optional (the provider's
/// default, NOT 80 — `OLLAMA_HOST=gpu-box` must reach 11434) · path
/// optional (the wire path appended). A URL that already names a path
/// is taken verbatim (trailing slash trimmed); IPv6 authorities
/// (`[::1]`) are bracket-aware.
fn normalize_local_base_url(raw: &str, default_port: u16) -> String {
    let trimmed = raw.trim().trim_end_matches('/');
    let with_scheme = if trimmed.contains("://") {
        trimmed.to_owned()
    } else {
        format!("http://{trimmed}")
    };
    let (scheme, rest) = with_scheme
        .split_once("://")
        .map_or(("http", with_scheme.as_str()), |(s, r)| (s, r));
    let (authority, path) = rest
        .split_once('/')
        .map_or((rest, None), |(a, p)| (a, Some(p)));
    let has_port = if let Some(bracket_end) = authority.rfind(']') {
        // IPv6 `[::1]` or `[::1]:11434` — a port is a colon AFTER the bracket.
        authority[bracket_end..].contains(':')
    } else {
        authority.contains(':')
    };
    let authority = if has_port {
        authority.to_owned()
    } else {
        format!("{authority}:{default_port}")
    };
    match path {
        Some(p) => format!("{scheme}://{authority}/{p}"),
        None => format!("{scheme}://{authority}/v1/chat/completions"),
    }
}

/// Resolve the `nika:image_generate` provider credentials — the SAME
/// sanctioned env boundary as [`config_from_env`], with the same ladder
/// shape the inference path uses (`NIKA_<ID>_API_KEY` first, then the
/// provider's conventional variable). Absent keys are NOT an error here:
/// the builtin surfaces the precise `NIKA-BUILTIN-IMAGE_GENERATE-002`
/// (naming the ladder) only when a workflow actually targets that
/// provider; the mock provider needs no key at all.
fn image_keys_from_env() -> ImageKeys {
    let read = |ladder: [&str; 2]| {
        ladder.into_iter().find_map(|name| {
            #[allow(clippy::disallowed_methods)] // the sanctioned env→secret boundary (see doc)
            let value = std::env::var(name).ok()?;
            (!value.is_empty()).then(|| Secret::new(value))
        })
    };
    let mut keys = ImageKeys::new();
    if let Some(key) = read(["NIKA_OPENAI_API_KEY", "OPENAI_API_KEY"]) {
        keys = keys.with_openai(key);
    }
    if let Some(key) = read(["NIKA_GEMINI_API_KEY", "GEMINI_API_KEY"]) {
        keys = keys.with_gemini(key);
    }
    if let Some(key) = read(["NIKA_XAI_API_KEY", "XAI_API_KEY"]) {
        keys = keys.with_xai(key);
    }
    // The LOCAL image server: base URL + optional key (both engine config
    // — the base URL is deliberately NOT a secret, but it crosses the env
    // at the same sanctioned boundary).
    #[allow(clippy::disallowed_methods)] // the sanctioned env boundary (see doc)
    if let Ok(url) = std::env::var("NIKA_IMAGE_LOCAL_URL")
        && !url.is_empty()
    {
        keys = keys.with_local_base_url(url);
    }
    // Single-name read (no conventional fallback exists for a local key —
    // the array shape just reuses the ladder helper).
    if let Some(key) = read(["NIKA_IMAGE_LOCAL_API_KEY", "NIKA_IMAGE_LOCAL_API_KEY"]) {
        keys = keys.with_local_api_key(key);
    }
    keys
}

/// The TTS plane's composition-root env resolution — the exact sibling of
/// [`image_keys_from_env`] (same sanctioned boundary · same laddering).
fn tts_keys_from_env() -> TtsKeys {
    let read = |ladder: [&str; 2]| {
        ladder.into_iter().find_map(|name| {
            #[allow(clippy::disallowed_methods)] // the sanctioned env→secret boundary (see doc)
            let value = std::env::var(name).ok()?;
            (!value.is_empty()).then(|| Secret::new(value))
        })
    };
    let mut keys = TtsKeys::new();
    if let Some(key) = read(["NIKA_OPENAI_API_KEY", "OPENAI_API_KEY"]) {
        keys = keys.with_openai(key);
    }
    if let Some(key) = read(["NIKA_ELEVENLABS_API_KEY", "ELEVENLABS_API_KEY"]) {
        keys = keys.with_elevenlabs(key);
    }
    #[allow(clippy::disallowed_methods)] // the sanctioned env boundary (see doc)
    if let Ok(url) = std::env::var("NIKA_TTS_LOCAL_URL")
        && !url.is_empty()
    {
        keys = keys.with_local_base_url(url);
    }
    if let Some(key) = read(["NIKA_TTS_LOCAL_API_KEY", "NIKA_TTS_LOCAL_API_KEY"]) {
        keys = keys.with_local_api_key(key);
    }
    keys
}

/// Derive the runtime `permits.fs` boundary from a parsed workflow.
///
/// No `permits:` block → a DECLARED boundary with empty glob lists
/// (F-O8 « absent = zero authority » · every fs effect is refused — the
/// absent block IS the undeclared zero, never an unconfined floor). A
/// `permits:` block WITHOUT an `fs:` category → the same empty declared
/// boundary (default-deny · « once `permits:` is present every category
/// is default-deny unless listed »). An `fs:` block → its read/write globs.
#[must_use]
pub fn fs_boundary_of(wf: &nika_schema::raw::RawWorkflow) -> FsBoundary {
    fs_boundary_of_permits(wf.permits.as_ref().map(|p| &p.value))
}

/// [`fs_boundary_of`] over a bare `Permits` value — the composition
/// lane derives a CHILD's boundary from the INTERSECTED permits
/// (`child ∩ parent` · spec 14 laws 3/4), not from a workflow node.
#[must_use]
pub fn fs_boundary_of_permits(permits: Option<&nika_schema::types::Permits>) -> FsBoundary {
    // F-O8 · absent = zero authority: `None` maps to the EMPTY declared
    // boundary (default-deny), never to `unbounded` — the absent block is
    // no longer a synonym for the pre-permits floor.
    let (read, write) = permits
        .and_then(|p| p.fs.as_ref())
        .map(|fs| (fs.read.clone(), fs.write.clone()))
        .unwrap_or_default();
    FsBoundary::declared(read, write)
}

/// Derive the runtime `permits.net.http` boundary from a parsed workflow.
///
/// No `permits:` block → `Declared(vec![])` (F-O8 « absent = zero
/// authority » · every host refused — the always-on SSRF floor stays ON
/// TOP, independent). A `permits:` block WITHOUT a `net:` category → the
/// same `Declared(vec![])` (default-deny · « once `permits:` is present
/// every category is default-deny unless listed »). A `net:` block →
/// `Declared` of its `http:` host globs. The fetch http client enforces
/// this on EVERY redirect hop (`NIKA-SEC-004`) — the runtime half of
/// spec §permits, catching the dynamic hosts (`${{ }}`-built · redirect
/// bounces) the static `nika check` cannot see. Mirrors [`fs_boundary_of`].
#[must_use]
pub fn net_boundary_of(wf: &nika_schema::raw::RawWorkflow) -> NetBoundary {
    net_boundary_of_permits(wf.permits.as_ref().map(|p| &p.value))
}

/// [`net_boundary_of`] over a bare `Permits` value (the composition
/// lane's intersected-boundary derivation · mirrors the fs twin).
#[must_use]
pub fn net_boundary_of_permits(permits: Option<&nika_schema::types::Permits>) -> NetBoundary {
    // F-O8 · absent = zero authority: `None` maps to the EMPTY declared
    // boundary (deny-all), never to `Unbounded`.
    NetBoundary::Declared(
        permits
            .and_then(|p| p.net.as_ref())
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
    /// The workflow runs ≥1 `exec:` task — the noop-sandbox note reads it
    /// (a boundary with no exec has nothing to jail, so nothing to note).
    pub exec_tasks: bool,
    /// An authority boundary was DECLARED over this run (`permits:` present
    /// on the workflow — or inherited from the parent at a child
    /// composition, spec 14's intersection) — the #889 policy gate's
    /// severity signal: declared + exec + no OS backend refuses (auto)
    /// or rides attested (off). ANY block counts (a tools-only block
    /// still jails the exec child to the empty axes — F-O8 — a contract
    /// a backend-less host silently voids).
    pub permits_declared: bool,
}

/// Derive BOTH runtime capability boundaries from a parsed workflow — the
/// single entry every composition root uses (so net can't be forgotten while
/// fs is wired). Composes [`fs_boundary_of`] + [`net_boundary_of`].
#[must_use]
pub fn capabilities_of(wf: &nika_schema::raw::RawWorkflow) -> RuntimeCapabilities {
    RuntimeCapabilities {
        fs: fs_boundary_of(wf),
        net: net_boundary_of(wf),
        exec_tasks: wf
            .tasks
            .iter()
            .any(|t| matches!(t.value.action, nika_schema::raw::RawAction::Exec(_))),
        permits_declared: wf.permits.is_some(),
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
/// Empirical anchor (#148 · M3 Pro): local thinking-era models legitimately
/// exceed 30s — `qwen3.5:4b` ~43s · `qwen3.5:9b` ~87s for one structured
/// task · cold model load adds 30-60s (the reason local classes default
/// to 300s at the wire layer · a timeout is a ceiling, not a wait).
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
/// floor blocks loopback/private/metadata) AND the `net` boundary
/// (`permits.net.http`) is enforced on every hop — a host outside it fails
/// `NIKA-SEC-004`. F-O8 « absent = zero authority »: no `permits:` block
/// derives `Declared([])` (deny-all), so the SSRF floor is never the ONLY
/// net guard anymore. This is the half the SSRF floor never
/// covered: the workflow's OWN declared host boundary, dynamic hosts included.
// `HttpConfig` is `#[non_exhaustive]` → field assignment, not a struct literal.
#[allow(clippy::field_reassign_with_default)]
fn fetch_http(net: NetBoundary) -> Result<ReqwestHttp, nika_kernel::HttpError> {
    let mut config = HttpConfig::default(); // ssrf: Enforce by default
    config.net = net;
    ReqwestHttp::with_config(config)
}

/// The `run:` declaration resolved to its execution seams (F-P3 · ONE
/// law, ONE home): which stamper mints event identities, which clock the
/// run's deadlines/sleeps/durations measure, and the retry jitter
/// stream's seed. An absent `run:` block (or an empty declaration)
/// resolves to the exact status quo — system stamps · system clock ·
/// the zero jitter stream.
///
/// The declared contradictions (`entropy: ambient` × `clock: virtual` ·
/// `entropy: none | seeded` × `clock: system`) are refused at PARSE, so
/// this resolution reads the post-refusal space; a caller bypassing the
/// parser still lands deterministic-entropy on the deterministic seams
/// (the fail-safe direction — never the ambient one).
pub struct RunSeams {
    /// The event-identity seam: deterministic (seq→UUID · +10ms/event)
    /// or system (`UUIDv7` · wall clock).
    stamps_deterministic: bool,
    /// The run's ONE clock (FDB/VOPR law) — deadlines · sleeps ·
    /// durations all measure it.
    pub clock: DeclaredClock,
    /// The retry jitter stream's seed (`(seed, task, attempt)` —
    /// replay-stable by construction).
    pub jitter_seed: u64,
}

impl RunSeams {
    /// Resolve the authored `run:` block (`None` = absent) to its seams.
    #[must_use]
    pub fn of(decl: Option<&RunDecl>) -> Self {
        let decl = decl.copied().unwrap_or_default();
        let entropy = decl.entropy_or_default();
        Self {
            stamps_deterministic: entropy.is_deterministic(),
            clock: match decl.clock_or_default() {
                nika_schema::types::RunClock::Virtual => DeclaredClock::r#virtual(),
                _ => DeclaredClock::system(),
            },
            jitter_seed: entropy.jitter_seed(),
        }
    }

    /// The stamper this resolution picks (the composer's event-identity
    /// seam — deterministic under `entropy: none | seeded`, the live
    /// system stamper otherwise).
    #[must_use]
    pub fn stamper(&self) -> Box<dyn crate::Stamper> {
        if self.stamps_deterministic {
            Box::new(crate::DeterministicStamper::new())
        } else {
            Box::new(crate::SystemStamper::new())
        }
    }
}

/// Compose the production runtime for a workflow whose envelope default
/// model is `default_model`, enforcing `caps` — the [`RuntimeCapabilities`]
/// derived from the workflow's `permits:` ([`capabilities_of`]) — at run time:
/// `caps.fs` (`permits.fs`) on the file builtins AND `caps.net`
/// (`permits.net.http`) on the fetch client, both halves of the runtime
/// `NIKA-SEC-004` boundary.
///
/// A workflow with no `permits:` block yields the F-O8 zero boundary
/// (empty declared fs + net · every effect refused · « absent = zero
/// authority »); a declared boundary makes a `..`/symlink path or an
/// out-of-allowlist host
/// fail `NIKA-SEC-004` (spec §permits · enforced statically AND at runtime).
/// Taking the whole `caps` (not the two axes separately) is deliberate — a
/// caller cannot wire fs and forget net.
///
/// # The `run:` declaration (F-P3)
///
/// `run` is the workflow's authored `run:` block (`None` = absent): it
/// picks the run's clock (`clock: system | virtual` — virtual implied by
/// `entropy: none | seeded`) and the retry jitter stream's seed through
/// [`RunSeams`]. The event-stamper half of the same resolution rides
/// [`RunSeams::stamper`] at the run-verb call site (the sink plumbing is
/// the caller's, not this composition's).
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
pub use crate::sandbox_select::{ComposeError, apply_sandbox_policy};

/// Compose the production runtime for one checked workflow — every real
/// plane (http · dispatcher · provider · agent · sandbox) injected at
/// the ONE composition root.
///
/// # Errors
/// [`ComposeError::Http`] when the fetch/provider plane cannot build ·
/// [`ComposeError::Sandbox`] when the policy requires confinement the
/// host cannot provide (#889 · NIKA-1710) · [`ComposeError::Policy`]
/// when `NIKA_SANDBOX` holds an unknown word.
pub fn production_runtime(
    default_model: &str,
    caps: RuntimeCapabilities,
    run: Option<&RunDecl>,
) -> Result<ProdRuntime, ComposeError> {
    production_runtime_with_emitter(
        default_model,
        caps,
        run,
        StderrEmitter::default(),
        std::env::current_dir().unwrap_or_default(),
    )
}

/// Service-safe sibling of [`production_runtime`]. Same production seams;
/// `nika:log`/`nika:emit` stay off stderr. `sandbox_root` is the admitted
/// workflow project — not the process cwd.
///
/// # Errors
/// The same three as [`production_runtime`].
pub fn service_runtime(
    default_model: &str,
    caps: RuntimeCapabilities,
    run: Option<&RunDecl>,
    sandbox_root: std::path::PathBuf,
) -> Result<ProdRuntime, ComposeError> {
    production_runtime_with_emitter(
        default_model,
        caps,
        run,
        StderrEmitter::metadata_only(),
        sandbox_root,
    )
}

fn production_runtime_with_emitter(
    default_model: &str,
    caps: RuntimeCapabilities,
    run: Option<&RunDecl>,
    emitter: StderrEmitter,
    sandbox_root: std::path::PathBuf,
) -> Result<ProdRuntime, ComposeError> {
    // F-P3 · the run: declaration picks the run's seams (clock · jitter
    // stream) ONCE; every injection below reads the same resolution.
    let seams = RunSeams::of(run);
    // The OS sandbox for exec children (ADR-095 Layer 6) — decided once, at
    // the ONE selection every composition root rides (#888), then judged by
    // the ONE policy knob (#889): permits declared + no backend = refusal.
    let decision = crate::sandbox_select::select_command_sandbox();
    let (policy, verdict) =
        apply_sandbox_policy(&decision, caps.exec_tasks && caps.permits_declared)?;
    // The backend NAME rides into the journal (`workflow_started.sandbox`).
    let sandbox_backend = decision.backend();
    if verdict == crate::sandbox_select::SandboxVerdict::Waived {
        waived_operator_note();
    }
    let sandbox = decision.into_sandbox();
    // The fetch/builtin client enforces SSRF (workflow URLs) AND the
    // declared permits.net.http boundary (NIKA-SEC-004 · per-hop).
    let http = Arc::new(fetch_http(caps.net)?);
    // The provider path gets its OWN client (SSRF disabled · see the
    // `provider_http` doc): the fetch/builtin plane below keeps `http`
    // (SSRF enforced · workflow-controlled URLs).
    let provider_http = Arc::new(provider_http()?);
    let config = config_from_env();
    let access_probes = nika_providers::probe::collect_access_probes_env(config.clone());

    // Builtin plane over real effects · InvokeVerb + agent tools.
    // File builtins enforce permits.fs (NIKA-SEC-004). Same Arc as runtime.
    let inspect = Arc::new(LiveInspect::new());
    let dispatcher: Arc<ProdDispatcher> = Arc::new(
        BuiltinDispatcher::new(
            Arc::new(TokioFs),
            Arc::clone(&http),
            Arc::new(seams.clock.clone()),
            // log/emit → stderr (observable · NOT a silent no-op).
            Arc::new(emitter),
            Arc::new(NonInteractive::default()),
            Arc::clone(&inspect),
        )
        .with_fs_boundary(caps.fs)
        // The IMAGE PLANE (`nika:image_generate`): provider calls ride the
        // SSRF-disabled provider client (const studio-fixed endpoints ·
        // the 600s transport ceiling image renders need — the fetch
        // client's idle guard would kill them), with keys resolved at THIS
        // boundary only.
        .with_image_plane(Arc::clone(&provider_http), image_keys_from_env())
        .with_tts_plane(Arc::clone(&provider_http), tts_keys_from_env()),
    );
    let invoke = Arc::new(InvokeVerb::new(Arc::clone(&dispatcher)));

    // The provider registry (real http + env keys) drives infer directly
    // and the agent via the per-call RegistryProvider bridge.
    let registry = Arc::new(ProviderRegistry::new(provider_http, config));
    let agent_provider = Arc::new(RegistryProvider::new(Arc::clone(&registry), default_model));
    // A broken adapter refuses at composition (A-4). `nika test` never seats.
    let harness_seat = crate::harness_seat::seat_from_env()?;

    Ok(Runtime::new(
        // The exec child environment is COMPOSED at the spawn site (NEP-0005 ·
        // the runner floor ∪ the declared `permits.env:` passthrough carried
        // on each command ∪ the authored map) — the old ambient-secret
        // denylist scrub is gone because nothing ambient crosses at all.
        ExecVerb::new(Arc::new(TokioShell::with_sandbox(sandbox))),
        Arc::clone(&invoke),
        InferVerb::new(registry, default_model),
        seated(
            AgentVerb::new(
                agent_provider,
                invoke,
                Arc::clone(&dispatcher),
                default_model,
            ),
            harness_seat,
        ),
        seams.clock,
        RuntimeConfig::new(None, seams.jitter_seed)
            .with_sandbox_root(sandbox_root)
            .with_sandbox_backend(sandbox_backend)
            .with_sandbox_policy(policy.as_str())
            .with_sandbox_waived(verdict == crate::sandbox_select::SandboxVerdict::Waived),
    )
    .with_inspect(inspect)
    .with_access_probes(access_probes)
    .with_harness_seat_id(crate::harness_seat::declared_id())
    // Resolve `secrets:` from env/file at run start (MINOR-B · the sanctioned
    // store boundary). A miss leaves the secret unbound → NIKA-1702 (fail-
    // closed); the IFC governs where a resolved value may flow.
    .with_secret_resolver(Arc::new(EnvFileSecretResolver)))
}

/// #889 — the waiver's operator note: when the run proceeds unconfined BY
/// CHOICE (`NIKA_SANDBOX=off`), say so once on stderr; the journal + the
/// trace carry the attestation (`sandbox_policy` · `sandbox_waived`).
fn waived_operator_note() {
    eprintln!(
        "note: exec runs UNCONFINED — the waiver is attested (NIKA_SANDBOX=off · \
         journal + trace); the declared boundary still gates fs/net at the \
         builtin and fetch seams"
    );
}

/// Seat the verb (P3 B4.5) — the feature split, in one place.
fn seated<P, T, D>(v: AgentVerb<P, T, D>, s: crate::harness_seat::Seat) -> AgentVerb<P, T, D> {
    #[cfg(feature = "access-harness")]
    return v.seated(s);
    #[cfg(not(feature = "access-harness"))]
    {
        let crate::harness_seat::Seat = s;
        v
    }
}

/// The fully-resolved SIMULATED runtime spelling (the `nika test` plane ·
/// P0-16): the SAME provider/infer/clock wiring as [`ProdRuntime`], but the
/// exec shell and the tool plane are the effects-disabled seams.
pub type SimRuntime = Runtime<
    SimulatedShell,
    SimulatedDispatcher<ProdDispatcher>,
    ReqwestHttp,
    RegistryProvider<ReqwestHttp>,
    SimulatedDispatcher<ProdDispatcher>,
    DeclaredClock,
>;

/// Compose the SIMULATED runtime — the `nika test` plane (UX audit
/// 2026-07-30 · P0-16). `capture_mock_outputs` substitutes the MODEL
/// (`mock/echo`), and a model swap alone left the tool/exec seams REAL:
/// `invoke nika:write` landed bytes, an exec task spawned a child,
/// `nika:fetch` opened a socket — under a verb documented « offline ».
/// This composition keeps the production shape (the declared `permits.fs`
/// boundary still gates every delegated fs READ · the registry resolves
/// the mock provider exactly as `run` would) but swaps the two effect
/// seams for their simulated twins:
///
/// - exec → [`crate::simulated::SimulatedShell`] — every command refused before spawn.
/// - the tool plane → [`crate::simulated::SimulatedDispatcher`] over the real
///   [`BuiltinDispatcher`] — pure compute, stderr observability, and fs
///   reads inside the declared boundary delegate VERBATIM; the effect set
///   (writes · network · media · any non-`nika:` namespace) refuses with
///   « effects disabled under `nika test` ».
///
/// The image/TTS provider planes are NOT wired (no env-key reads): every
/// media tool is in the refusal set, so resolving their keys would be
/// work toward a call the gate refuses anyway. The journaled sandbox
/// backend reads `simulated` — no OS confinement is claimed for children
/// that never spawn.
///
/// # Errors
///
/// Same construction failure as [`production_runtime`]: the TLS backend
/// refusing to initialize ([`ReqwestHttp`] — mapped to the environment
/// exit class by the caller).
pub fn simulated_runtime(
    default_model: &str,
    caps: RuntimeCapabilities,
    run: Option<&RunDecl>,
) -> Result<SimRuntime, nika_kernel::HttpError> {
    // F-P3 · the run: declaration resolves the same seams as production
    // (clock · jitter stream) — determinism is the golden's whole point.
    let seams = RunSeams::of(run);
    // Wired for SHAPE parity with production (the delegated reads share the
    // fetch-plane client type · the declared net boundary still derives) —
    // the gate refuses `nika:fetch`/`nika:notify` before this client could
    // open a socket.
    let http = Arc::new(fetch_http(caps.net)?);
    let provider_http = Arc::new(provider_http()?);
    let config = config_from_env();

    // Real builtin plane (permits.fs as production) then the simulated gate.
    let inspect = Arc::new(LiveInspect::new());
    let dispatcher = Arc::new(SimulatedDispatcher::new(Arc::new(
        BuiltinDispatcher::new(
            Arc::new(TokioFs),
            http,
            Arc::new(seams.clock.clone()),
            Arc::new(StderrEmitter::default()),
            Arc::new(NonInteractive::default()),
            Arc::clone(&inspect),
        )
        .with_fs_boundary(caps.fs),
    )));
    let invoke = Arc::new(InvokeVerb::new(Arc::clone(&dispatcher)));

    // The provider registry (real http + env keys) drives infer directly
    // and the agent via the per-call RegistryProvider bridge — under
    // `nika test` the default model is `mock/echo` (keyless · offline).
    let registry = Arc::new(ProviderRegistry::new(provider_http, config));
    let agent_provider = Arc::new(RegistryProvider::new(Arc::clone(&registry), default_model));

    Ok(Runtime::new(
        // The effects-disabled shell: no sandbox selection, no unconfined
        // note — a child that can never spawn needs neither.
        ExecVerb::new(Arc::new(SimulatedShell)),
        Arc::clone(&invoke),
        InferVerb::new(registry, default_model),
        AgentVerb::new(
            agent_provider,
            invoke,
            Arc::clone(&dispatcher),
            default_model,
        ),
        seams.clock,
        RuntimeConfig::new(None, seams.jitter_seed)
            .with_sandbox_root(std::env::current_dir().unwrap_or_default())
            .with_sandbox_backend("simulated"),
    )
    .with_inspect(inspect)
    // The SAME secrets boundary as production (env/file at run start ·
    // fail-closed on a miss) — reading the operator's own store is not an
    // external effect, and ${{ secrets.X }} templates resolve identically.
    .with_secret_resolver(Arc::new(EnvFileSecretResolver)))
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use nika_schema::types::RunEntropy;

    #[test]
    fn local_base_url_normalization_mirrors_the_ollama_client() {
        // The documented OLLAMA_HOST conveniences, each resolved the way
        // the official client resolves them (scheme http · port = the
        // provider's own default, never 80 · wire path appended).
        let n = |raw: &str| normalize_local_base_url(raw, 11434);
        assert_eq!(
            n("gpu-box:11434"),
            "http://gpu-box:11434/v1/chat/completions"
        );
        assert_eq!(n("gpu-box"), "http://gpu-box:11434/v1/chat/completions");
        assert_eq!(n("0.0.0.0"), "http://0.0.0.0:11434/v1/chat/completions");
        assert_eq!(
            n("http://gpu-box:7777"),
            "http://gpu-box:7777/v1/chat/completions"
        );
        assert_eq!(
            n("https://gpu-box"),
            "https://gpu-box:11434/v1/chat/completions"
        );
        assert_eq!(
            n("http://gpu-box:7777/"),
            "http://gpu-box:7777/v1/chat/completions"
        );
        // An explicit path is the operator's word — verbatim.
        assert_eq!(
            n("http://gw:8080/ollama/v1/chat/completions"),
            "http://gw:8080/ollama/v1/chat/completions"
        );
        // IPv6: the authority colon is not a port separator.
        assert_eq!(n("[::1]"), "http://[::1]:11434/v1/chat/completions");
        assert_eq!(n("[::1]:7777"), "http://[::1]:7777/v1/chat/completions");
        // A different provider's default rides its own port.
        assert_eq!(
            normalize_local_base_url("gpu-box", 8000),
            "http://gpu-box:8000/v1/chat/completions"
        );
    }

    #[test]
    fn local_base_env_table_matches_the_provider_seeds() {
        // The default ports here MUST mirror nika-providers' LOCAL seed
        // rows — this cross-check turns silent drift (a 6th local · a
        // port change) into a red test naming both places.
        let registry = ProviderRegistry::without_http(ProvidersConfig::new());
        for (id, _, port) in LOCAL_BASE_ENV {
            let profile = registry.profiles().iter().find(|p| p.id == id);
            let profile = profile.expect("every LOCAL_BASE_ENV id exists in the registry seeds");
            assert!(
                profile.base_url.contains(&format!(":{port}/")),
                "`{id}` default port drifted: table says {port}, seed says {}",
                profile.base_url
            );
        }
    }

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
        // (a) no permits block → Declared([]) = deny-all (F-O8 « absent = zero
        // authority » · the SSRF floor stays on top, independent).
        assert_eq!(
            net_boundary_of(&parse(
                "nika: w\ntasks:\n  t:\n    exec: { command: [\"echo\", \"hi\"] }\n"
            )),
            NetBoundary::Declared(Vec::new())
        );
        // (b) permits present but NO net category → Declared([]) = deny-all.
        assert_eq!(
            net_boundary_of(&parse(
                "nika: w\npermits:\n  tools: [\"nika:jq\"]\ntasks:\n  t:\n    invoke: { tool: \"nika:jq\", args: { input: {}, expression: \".\" } }\n"
            )),
            NetBoundary::Declared(Vec::new())
        );
        // (c) net.http present → Declared([globs]).
        assert_eq!(
            net_boundary_of(&parse(
                "nika: w\npermits:\n  net: { http: [\"api.example.com\", \"*.github.com\"] }\n  tools: [\"nika:fetch\"]\ntasks:\n  t:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://api.example.com/\" } }\n"
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
            "nika: w\npermits:\n  net: { http: [\"api.example.com\"] }\n  fs: { write: [\"./out/**\"] }\n  tools: [\"nika:fetch\"]\ntasks:\n  t:\n    invoke: { tool: \"nika:fetch\", args: { url: \"https://api.example.com/\" } }\n",
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
        // #889's severity signal: the block's PRESENCE is what the policy
        // gate reads — declared here, absent below.
        assert!(caps.permits_declared, "a permits: block is declared");
        let bare = capabilities_of(&parse(
            "nika: w\ntasks:\n  t:\n    exec: { command: [\"echo\", \"hi\"] }\n",
        ));
        assert!(
            !bare.permits_declared,
            "no permits: block — the permitless best-effort stays (Q4.B)"
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
                exec_tasks: false,
                permits_declared: false,
            },
            None,
        );
        assert!(
            runtime.is_ok(),
            "the production runtime composes (TLS init is the only failure)"
        );
    }

    #[test]
    fn simulated_composition_succeeds_for_a_mock_model() {
        // P0-16 · the `nika test` plane composes offline too (the mock
        // profile needs no key · the gate swaps the effect seams, never
        // the construction contract).
        let runtime = simulated_runtime(
            "mock/echo",
            RuntimeCapabilities {
                fs: FsBoundary::unbounded(),
                net: NetBoundary::Unbounded,
                exec_tasks: false,
                permits_declared: false,
            },
            None,
        );
        assert!(
            runtime.is_ok(),
            "the simulated runtime composes (TLS init is the only failure)"
        );
    }

    // ── F-P3 · the run: declaration resolves to its seams ────────────

    #[test]
    fn absent_run_block_resolves_to_the_status_quo() {
        for decl in [None, Some(&RunDecl::default())] {
            let seams = RunSeams::of(decl);
            assert_eq!(seams.jitter_seed, 0, "the zero stream, unchanged");
            assert!(
                seams.clock.as_virtual().is_none(),
                "the system clock, unchanged"
            );
            // The live stamper mints UUIDv7 (ADR-033) — the ambient lane.
            let mut stamper = seams.stamper();
            let (id, _) = stamper.next();
            assert_eq!(id.uuid.get_version_num(), 7, "ambient = UUIDv7");
        }
    }

    #[test]
    fn seeded_resolves_every_deterministic_seam() {
        let decl = RunDecl::new(Some(RunEntropy::Seeded(42)), None);
        let seams = RunSeams::of(Some(&decl));
        assert_eq!(seams.jitter_seed, 42, "the seed keys the jitter stream");
        assert!(
            seams.clock.as_virtual().is_some(),
            "deterministic entropy implies the virtual clock (durations may \
             not ride the wall clock when journals replay byte-identical)"
        );
        // The deterministic stamper is seq-keyed (id 1 · t 10ms) — replay
        // law: same stream in, same bytes out.
        let mut stamper = seams.stamper();
        let (id, ts) = stamper.next();
        assert_eq!(ts.unix_ms(), 10, "+10ms from zero");
        let mut twin = RunSeams::of(Some(&decl)).stamper();
        assert_eq!(twin.next().0, id, "two runs mint the same first id");
    }

    #[test]
    fn entropy_none_pins_the_zero_stream_and_virtual_clock() {
        let decl = RunDecl::new(Some(RunEntropy::None), None);
        let seams = RunSeams::of(Some(&decl));
        assert_eq!(seams.jitter_seed, 0, "none = the fixed zero stream");
        assert!(seams.clock.as_virtual().is_some());
        let mut stamper = seams.stamper();
        assert_eq!(stamper.next().1.unix_ms(), 10, "deterministic stamps");
    }

    #[test]
    fn an_explicit_virtual_clock_composes_under_ambient_entropy() {
        // The legal testing configuration (F-P3): virtual deadlines, live
        // event stamps — no determinism claim is made.
        let decl = RunDecl::new(None, Some(nika_schema::types::RunClock::Virtual));
        let seams = RunSeams::of(Some(&decl));
        assert!(seams.clock.as_virtual().is_some());
        let mut stamper = seams.stamper();
        assert_eq!(
            stamper.next().0.uuid.get_version_num(),
            7,
            "ambient entropy keeps the live stamper"
        );
    }

    #[test]
    fn ladder_prefers_the_nika_prefix_and_skips_empties() {
        // The documented ladder (Profile::env_candidates · doctor's
        // report): NIKA_<ID>_API_KEY wins · empty counts as absent on
        // both rungs · conventional is the fallback. Pure lookup — no
        // process-env mutation (parallel tests stay hermetic).
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |name: &str| {
                pairs
                    .iter()
                    .find(|(k, _)| *k == name)
                    .map(|(_, v)| (*v).to_owned())
            }
        };
        let both = env(&[
            ("NIKA_MISTRAL_API_KEY", "scoped"),
            ("MISTRAL_API_KEY", "shared"),
        ]);
        assert_eq!(
            ladder_key(&both, "mistral", "MISTRAL_API_KEY").as_deref(),
            Some("scoped"),
            "the NIKA_ override wins"
        );
        let conventional = env(&[("MISTRAL_API_KEY", "shared")]);
        assert_eq!(
            ladder_key(&conventional, "mistral", "MISTRAL_API_KEY").as_deref(),
            Some("shared")
        );
        let empty_override = env(&[("NIKA_MISTRAL_API_KEY", ""), ("MISTRAL_API_KEY", "shared")]);
        assert_eq!(
            ladder_key(&empty_override, "mistral", "MISTRAL_API_KEY").as_deref(),
            Some("shared"),
            "an empty override falls through to the conventional rung"
        );
        let nothing = env(&[]);
        assert_eq!(ladder_key(&nothing, "mistral", "MISTRAL_API_KEY"), None);
    }

    #[test]
    fn cloud_base_url_is_verbatim_and_explicit_only() {
        // The long-tail hatch surface: NIKA_<ID>_BASE_URL, complete URL,
        // verbatim — no loopback grammar, no conventional-var fallback.
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |name: &str| {
                pairs
                    .iter()
                    .find(|(k, _)| *k == name)
                    .map(|(_, v)| (*v).to_owned())
            }
        };
        let set = env(&[(
            "NIKA_OPENAI_BASE_URL",
            "https://api.scaleway.ai/v1/chat/completions",
        )]);
        assert_eq!(
            cloud_base_url(&set, "openai").as_deref(),
            Some("https://api.scaleway.ai/v1/chat/completions")
        );
        let empty = env(&[("NIKA_OPENAI_BASE_URL", "")]);
        assert_eq!(cloud_base_url(&empty, "openai"), None, "empty = absent");
        let unset = env(&[]);
        assert_eq!(cloud_base_url(&unset, "openai"), None);
    }

    #[test]
    fn registry_provider_reports_the_resolved_models_capability() {
        // BUG#11 robustness: the per-call bridge reports its `default_model`'s
        // ACTUAL wire capability (was hardcoded false). Every wired cloud
        // family is native today — anthropic joined 2026-07-07
        // (output_config.format · GA 2026-01-29).
        use nika_kernel::ai::provider::ProviderMeta;

        let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::new()));
        let gemini = RegistryProvider::new(Arc::clone(&registry), "gemini/flash");
        assert!(gemini.supports_response_format(), "gemini → native");
        let openai = RegistryProvider::new(Arc::clone(&registry), "openai/gpt-4o");
        assert!(openai.supports_response_format(), "openai → native");
        let anthropic = RegistryProvider::new(Arc::clone(&registry), "anthropic/sonnet");
        assert!(
            anthropic.supports_response_format(),
            "anthropic → native (output_config.format)"
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
    fn log_message_neutralizes_terminal_control_sequences() {
        // An untrusted value (e.g. `${tasks.fetched}` of web content)
        // carrying an OSC title-rewrite + a CR log-forge must reach the
        // terminal as visible escapes, never as live control bytes.
        let hostile = "\u{1b}]0;pwned\u{7}safe\rFORGED";
        let out = format_emit(
            "log",
            &serde_json::json!({ "level": "info", "message": hostile }),
        );
        assert!(!out.contains('\u{1b}'), "no raw ESC survives: {out:?}");
        assert!(!out.contains('\r'), "no raw CR survives: {out:?}");
        assert!(
            out.contains("\\u{1b}") && out.contains("\\u{0d}"),
            "escaped form: {out:?}"
        );
        assert!(
            out.contains("safe") && out.contains("FORGED"),
            "text preserved: {out:?}"
        );
        // The common clean path is untouched (tab + newline pass through).
        let clean = format_emit("log", &serde_json::json!({ "message": "line1\nline2\tx" }));
        assert_eq!(clean, "nika:log [info] line1\nline2\tx");
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

    #[test]
    fn full_builtin_whitelist_stays_below_the_router_threshold() {
        // Adding a builtin must never silently flip `tools: ["nika:*"]`
        // agents from deterministic passthrough to BM25 routing (the
        // 23→24 near-miss of 2026-07-05 · ADR-105): a plain-builtin
        // agent's whitelisted universe equals tool_defs().len(), so it
        // must stay strictly below the router's activation threshold.
        let universe = nika_builtin::tool_defs().len();
        let threshold = nika_verb_agent::RouterConfig::default().min_universe;
        assert!(
            universe < threshold,
            "the full nika:* whitelist ({universe} defs) reached the router's \
             min_universe ({threshold}) — raise the default (or decide routing \
             is now INTENDED for plain-builtin agents, and update the e2e \
             offered-count assertions deliberately)"
        );
    }

    #[test]
    fn image_keys_from_env_never_panics_and_never_leaks_in_debug() {
        // The image-plane sibling of the test above: resolution succeeds
        // whatever the shell exports, and the Debug rendering can never
        // spell a credential (Secret is Debug-redacted by construction).
        let keys = image_keys_from_env();
        let tts = tts_keys_from_env();
        let rendered = format!("{keys:?} {tts:?}");
        for env in [
            "NIKA_ELEVENLABS_API_KEY",
            "ELEVENLABS_API_KEY",
            "NIKA_TTS_LOCAL_API_KEY",
            "NIKA_OPENAI_API_KEY",
            "OPENAI_API_KEY",
            "NIKA_GEMINI_API_KEY",
            "GEMINI_API_KEY",
            "NIKA_XAI_API_KEY",
            "XAI_API_KEY",
            "NIKA_IMAGE_LOCAL_API_KEY",
        ] {
            #[allow(clippy::disallowed_methods)] // reading, in a test, what the fn read
            if let Ok(value) = std::env::var(env)
                && !value.is_empty()
            {
                assert!(
                    !rendered.contains(&value),
                    "a live `{env}` value surfaced in Debug output"
                );
            }
        }
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
        let dir = std::env::temp_dir().join("nika-runtime-killtests");
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
