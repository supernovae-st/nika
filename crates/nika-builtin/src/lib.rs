// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-builtin` — the 24 canonical stdlib v0.1 builtins (L1.5).
//!
//! ONE dispatcher ([`BuiltinDispatcher`]) implements the three kernel tool
//! seams the verbs consume — [`ToolExecuteDyn`] + [`ToolBatchDyn`]
//! (`invoke` · `agent` dispatch) and [`ToolDefinitionProviderDyn`] (the
//! agent's model-facing tool catalog). Every builtin is a thin composition
//! over injected effect seams; the closed registry routes by name.
//!
//! Normative contracts live in `nika-spec stdlib/builtins-v0.1.md` — this
//! crate implements, never restates. Error planes (crate spec §3):
//!
//! 1. the tool RAN and failed → `ToolResult { is_error: true, content:
//!    "NIKA-BUILTIN-<NAME>-NNN · <message>" }` (the spec's 4-segment
//!    codes verbatim · what an agent loop feeds back to the model);
//! 2. the call could not be addressed (unknown name · malformed arg
//!    SHAPE the static `builtin_shape` ladder normally catches) →
//!    [`ToolExecError`] (`NotFound` / `ExecutionFailed`) — the spec
//!    taxonomy stays pure, dispatch-plane failures stay dispatch-plane.
//!
//! `nika:done` is REJECTED here (`NIKA-BUILTIN-DONE-001`): the sentinel is
//! agent-loop-owned — a loop intercepts it before any dispatcher runs, so
//! reaching this crate MEANS the call came from a standalone `invoke:`.

#![forbid(unsafe_code)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod core_tools;
pub mod data;
pub mod date;
pub mod defs;
pub mod file;
pub mod image;
pub mod inspect;
pub mod net;
pub mod permits;
pub mod tts;

use std::sync::Arc;

use nika_kernel::ai::tool_defs::ToolDefinitionProviderDyn;
use nika_kernel::io::clock::ClockDyn;
use nika_kernel::io::fs::{FsListDyn, FsReadDyn, FsWriteDyn};
use nika_kernel::io::http::{HttpGetDyn, HttpPostDyn};
use nika_kernel::runtime::tool_executor::{
    ToolBatchDyn, ToolCall, ToolErrorMeta, ToolExecError, ToolExecuteDyn, ToolResult,
};

pub use defs::{TOOLS_EXPORT_VERSION, tool_defs, tools_json};
pub use image::ImageKeys;
pub use permits::FsBoundary;
pub use tts::TtsKeys;

/// The closed builtin namespace prefix.
pub const NAMESPACE: &str = "nika:";

/// One builtin failure — the spec's 4-segment code + the human message.
/// Rendered into `ToolResult.content` as `"<code> · <message>"`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct BuiltinFailure {
    /// The spec code (`NIKA-BUILTIN-<NAME>-NNN` · taxonomy-owned by
    /// `spec/05-errors.md` — never invented here).
    pub code: &'static str,
    /// Actionable detail (lands after the code in the fed-back content).
    pub message: String,
    /// Retryability per the spec's per-builtin contract (e.g. `fetch` ·
    /// `transient: true` for 5xx/408/429). Typed here at the failure
    /// plane; the wire `ToolResult` has no metadata slot yet, so the
    /// flag projects when the kernel grows one (additive · both types
    /// are `#[non_exhaustive]`).
    pub transient: bool,
    /// Structured error details per the spec's error object (e.g.
    /// `fetch` · `details.status_code` carries the HTTP status —
    /// builtins-v0.1.md §fetch, normative). Machine-readable where the
    /// message is human-readable; same projection story as `transient`.
    pub details: Option<serde_json::Value>,
}

impl BuiltinFailure {
    /// A non-transient failure (the default for every builtin).
    #[must_use]
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            transient: false,
            details: None,
        }
    }

    /// Mark this failure transient (retryable per the spec contract).
    #[must_use]
    pub fn with_transient(mut self, transient: bool) -> Self {
        self.transient = transient;
        self
    }

    /// Attach structured details (the spec's machine-readable plane).
    #[must_use]
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}

/// A builtin's success value — any JSON value (stdlib §cross-builtin
/// invariants). The dispatcher serializes it into `ToolResult.content`.
pub type BuiltinValue = serde_json::Value;

/// Outcome of one builtin run.
pub type BuiltinOutcome = Result<BuiltinValue, BuiltinFailure>;

// ─── local seams (single-consumer traits live with their consumer) ──────

/// Answers `nika:prompt` — the human-in-the-loop seam. The L4 CLI
/// implements the TTY prompter; engines running headless use
/// [`NonInteractive`] (the normative CI contract).
pub trait Prompter: Send + Sync {
    /// Answer a `confirm` prompt (`None` = no human available).
    fn confirm(&self, message: &str) -> Option<bool>;
    /// Answer an `input` prompt (`None` = no human available).
    fn input(&self, message: &str) -> Option<String>;
    /// Answer a `choice` prompt with the CHOSEN VALUE (`None` = no human).
    fn choice(&self, message: &str, choices: &[String]) -> Option<String>;
}

/// The headless prompter: never answers — `nika:prompt` then uses
/// `default:` when present and fails `NIKA-BUILTIN-PROMPT-001` when
/// absent (stdlib §prompt · normative · never hang, never invent).
/// Construct via [`Default`].
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct NonInteractive;

impl Prompter for NonInteractive {
    fn confirm(&self, _message: &str) -> Option<bool> {
        None
    }
    fn input(&self, _message: &str) -> Option<String> {
        None
    }
    fn choice(&self, _message: &str, _choices: &[String]) -> Option<String> {
        None
    }
}

/// Routes `nika:log` / `nika:emit` content onto the workflow event
/// stream. The ENGINE owns event identity (ids · run · timestamps) —
/// this seam carries only (kind, payload), so the sealed kernel
/// `EventSink` stays an engine concern.
pub trait Emitter: Send + Sync {
    /// Deliver one event payload (best-effort — see per-builtin contract).
    fn emit(&self, kind: &str, payload: serde_json::Value);
}

/// Discards everything (tests · engines without a journal yet).
/// Construct via [`Default`].
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct NullEmitter;

impl Emitter for NullEmitter {
    fn emit(&self, _kind: &str, _payload: serde_json::Value) {}
}

/// Answers `nika:inspect`'s 4 views from live run state. The L3 engine
/// implements it per-run; [`NoWorkflow`] is the no-run-context stand-in.
///
/// # The wiring gap (F3 · why production injects [`NoWorkflow`] today)
///
/// A real impl needs the run's LIVE DAG (the check report's waves + the
/// task graph), the settling `records` map, and the running cost total —
/// all of which exist only INSIDE `nika_runtime::Runtime::run` (the
/// `records` map is built there; cost rides the per-task token counts).
/// The `BuiltinDispatcher` that serves `nika:inspect`, however, is
/// composed BEFORE the run and shared (`Arc`) across every concurrent
/// task — it holds no handle to that live state. Closing the gap means
/// either (a) the dispatcher holding an `Arc<Mutex<RunState>>` the
/// runtime updates as it executes (a new runtime-introspection surface),
/// or (b) the runtime owning the dispatcher and re-injecting a per-run
/// `WorkflowIntrospect` — both are a real subsystem, not a builtin patch.
/// Until then [`NoWorkflow`] returns an explicit `available: false` so the
/// builtin never serves zeros that look like a real empty run.
pub trait WorkflowIntrospect: Send + Sync {
    /// `view: cost` → `{ total_usd, by_task, by_provider }`.
    fn cost(&self) -> serde_json::Value;
    /// `view: records` → `{ tasks: [...] }`.
    fn records(&self) -> serde_json::Value;
    /// `view: dag_info` → `{ nodes, edges, waves }`.
    fn dag_info(&self) -> serde_json::Value;
    /// `view: threads` → `{ active, queued, completed }` (advisory).
    fn threads(&self) -> serde_json::Value;
}

/// The no-run-context introspection stand-in: every view honestly reports
/// that live run state is NOT available here, rather than zeros that look
/// like a real (empty) run (`{ nodes: [], … }` is indistinguishable from a
/// genuine empty DAG — the F3 misleading-empty footgun). The `view:`
/// argument is still validated by the builtin; this seam only supplies the
/// answer. The composition root injects a real [`WorkflowIntrospect`] once
/// the runtime exposes its live DAG/records/cost (see the type doc).
/// Construct via [`Default`].
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct NoWorkflow;

impl NoWorkflow {
    /// The honest « no live run state in this context » marker every view
    /// returns — `available: false` is the machine-readable signal a
    /// consumer branches on (vs a zero/empty shape it would mistake for a
    /// real empty run).
    fn unavailable(view: &str) -> serde_json::Value {
        serde_json::json!({
            "available": false,
            "view": view,
            "reason": "nika:inspect has no live run context here — this dispatcher \
                       was composed without a workflow introspection source (the \
                       runtime does not yet expose its live DAG/records/cost to the \
                       in-workflow builtin)",
        })
    }
}

impl WorkflowIntrospect for NoWorkflow {
    fn cost(&self) -> serde_json::Value {
        Self::unavailable("cost")
    }
    fn records(&self) -> serde_json::Value {
        Self::unavailable("records")
    }
    fn dag_info(&self) -> serde_json::Value {
        Self::unavailable("dag_info")
    }
    fn threads(&self) -> serde_json::Value {
        Self::unavailable("threads")
    }
}

// ─── the dispatcher ──────────────────────────────────────────────────────

/// The 24-builtin dispatcher over its injected seams.
///
/// Kernel seams: `F` filesystem (file 5) · `H` http (fetch · notify · the
/// image plane) · `C` clock (wait · date). Local seams: `Em` event content
/// (log · emit) · `P` prompter (prompt) · `W` workflow introspection
/// (inspect).
#[derive(Debug)]
pub struct BuiltinDispatcher<F, H, C, Em, P, W> {
    fs: Arc<F>,
    http: Arc<H>,
    clock: Arc<C>,
    emitter: Arc<Em>,
    prompter: Arc<P>,
    workflow: Arc<W>,
    /// The declared `permits.fs` boundary the file builtins enforce at run
    /// time (spec §permits · `NIKA-SEC-004`). Defaults to
    /// [`FsBoundary::unbounded`] (enforce nothing · the pre-permits floor) —
    /// the composition root injects the workflow's boundary via
    /// [`BuiltinDispatcher::with_fs_boundary`].
    fs_boundary: FsBoundary,
    /// The IMAGE-PLANE http client (`nika:image_generate` provider calls) —
    /// `None` until the composition root wires it via
    /// [`BuiltinDispatcher::with_image_plane`]. Distinct from `http` (the
    /// fetch plane): image endpoints are const studio-fixed strings, so the
    /// provider-plane client (SSRF disabled · long transport ceiling) is the
    /// correct carrier — the fetch client's 30s idle guard would kill
    /// 60–120s image renders.
    image_http: Option<Arc<H>>,
    /// The TTS plane — the same provider-plane philosophy (SSRF-disabled
    /// client · const/config endpoints · wired via [`Self::with_tts_plane`]).
    tts_http: Option<Arc<H>>,
    /// Image-provider credentials (composition-root resolved · never read
    /// from the environment here).
    image_keys: image::ImageKeys,
    tts_keys: tts::TtsKeys,
}

impl<F, H, C, Em, P, W> BuiltinDispatcher<F, H, C, Em, P, W> {
    /// Compose the dispatcher over its seams (no fs boundary — the floor).
    #[must_use]
    pub fn new(
        fs: Arc<F>,
        http: Arc<H>,
        clock: Arc<C>,
        emitter: Arc<Em>,
        prompter: Arc<P>,
        workflow: Arc<W>,
    ) -> Self {
        Self {
            fs,
            http,
            clock,
            emitter,
            prompter,
            workflow,
            fs_boundary: FsBoundary::unbounded(),
            image_http: None,
            tts_http: None,
            image_keys: image::ImageKeys::new(),
            tts_keys: tts::TtsKeys::new(),
        }
    }

    /// Set the `permits.fs` boundary the file builtins enforce (spec
    /// §permits · `NIKA-SEC-004`). The composition root derives it from the
    /// workflow's `permits.fs` block; an UNDECLARED boundary leaves the
    /// pre-permits floor in place. Builder form so the public `new`
    /// signature (and every call site) stays unchanged.
    #[must_use]
    pub fn with_fs_boundary(mut self, boundary: FsBoundary) -> Self {
        self.fs_boundary = boundary;
        self
    }

    /// Wire the IMAGE PLANE — the http client + provider keys that
    /// `nika:image_generate` calls real providers through. The composition
    /// root passes its PROVIDER-plane client (SSRF disabled for the fixed
    /// endpoint allowlist · long transport ceiling) and the env-resolved
    /// [`image::ImageKeys`]. Unwired, the mock provider still works and the
    /// real providers fail with the precise
    /// `NIKA-BUILTIN-IMAGE_GENERATE-002`.
    #[must_use]
    pub fn with_image_plane(mut self, http: Arc<H>, keys: image::ImageKeys) -> Self {
        self.image_http = Some(http);
        self.image_keys = keys;
        self
    }

    /// Wire the TTS plane (provider-plane client + composition-root keys)
    /// — `nika:tts_generate` cloud/local providers need it; mock never does.
    #[must_use]
    pub fn with_tts_plane(mut self, http: Arc<H>, keys: tts::TtsKeys) -> Self {
        self.tts_http = Some(http);
        self.tts_keys = keys;
        self
    }
}

impl<F, H, C, Em, P, W> BuiltinDispatcher<F, H, C, Em, P, W>
where
    F: FsReadDyn + FsWriteDyn + FsListDyn,
    H: HttpGetDyn + HttpPostDyn,
    C: ClockDyn,
    Em: Emitter,
    P: Prompter,
    W: WorkflowIntrospect,
{
    /// Route one call to its builtin. `Err(ToolExecError)` = the
    /// dispatch plane (unknown tool · arg SHAPE the static ladder owns);
    /// `Ok(Err(BuiltinFailure))` = the tool ran and failed (spec codes).
    ///
    /// The `nika:` prefix is REQUIRED (the spec's invocation surface is
    /// `tool: "nika:<name>"`): a bare `read` is `NotFound`, so a model
    /// that drops the prefix gets corrective feedback instead of a
    /// silently-forgiven alias.
    async fn route(
        &self,
        name: &str,
        args: &serde_json::Value,
    ) -> Result<BuiltinOutcome, ToolExecError> {
        let args = args
            .as_object()
            .ok_or_else(|| ToolExecError::ExecutionFailed {
                name: name.to_owned(),
                reason: "builtin args must be a JSON object".to_owned(),
            })?;
        let Some(short) = name.strip_prefix(NAMESPACE) else {
            return Err(ToolExecError::NotFound {
                name: name.to_owned(),
            });
        };
        match short {
            // core 6
            "log" => Ok(core_tools::log(self.emitter.as_ref(), args)),
            "emit" => Ok(core_tools::emit(self.emitter.as_ref(), args)),
            "assert" => Ok(core_tools::assert(args)),
            "prompt" => Ok(core_tools::prompt(self.prompter.as_ref(), args)),
            "done" => Ok(core_tools::done()),
            "wait" => Ok(core_tools::wait(self.clock.as_ref(), args).await),
            // file 5 — each fs op is gated by the declared permits.fs
            // boundary FIRST (spec §permits · NIKA-SEC-004): a path that
            // resolves outside the boundary fails before any I/O. An
            // undeclared boundary is a no-op (the pre-permits floor).
            "read" => Ok(self
                .guarded_fs(args, &[permits::FsAccess::Read], file::read)
                .await),
            "write" => Ok(self
                .guarded_fs(args, &[permits::FsAccess::Write], file::write)
                .await),
            "edit" => Ok(self
                .guarded_fs(
                    args,
                    &[permits::FsAccess::Read, permits::FsAccess::Write],
                    file::edit,
                )
                .await),
            // grep is a recursive READER rooted at `path:` (default `.`).
            "grep" => Ok(self.guarded_grep(args).await),
            // glob walks descendants of `.` (the kernel walk never crosses
            // `..` nor follows symlinked dirs), so its results are confined
            // to the cwd subtree by construction — gate on `.` being readable.
            "glob" => Ok(self.guarded_glob(args).await),
            // data 8
            // jq is synchronous CPU (jaq eval) with model-controlled cost —
            // it runs on the blocking pool so a heavy expression can't
            // starve the async executor (the nika-ocr/a11y precedent).
            "jq" => {
                let owned = args.clone();
                tokio::task::spawn_blocking(move || data::jq(&owned))
                    .await
                    .map_err(|e| ToolExecError::ExecutionFailed {
                        name: name.to_owned(),
                        reason: format!("jq evaluation task failed: {e}"),
                    })
            }
            "json_diff" => Ok(data::json_diff(args)),
            "validate" => Ok(data::validate(args)),
            "json_merge_patch" => Ok(data::json_merge_patch(args)),
            "convert" => Ok(data::convert(args)),
            "uuid" => Ok(date::uuid(args)),
            "date" => Ok(date::date(self.clock.as_ref(), args)),
            "hash" => Ok(data::hash(args)),
            // network 2
            "fetch" => Ok(net::fetch(self.http.as_ref(), args).await),
            "notify" => Ok(net::notify(self.http.as_ref(), args).await),
            // media 1 — provider calls ride the IMAGE PLANE (a dedicated
            // seam · const endpoints); saves ride the permits.fs boundary
            // (each final path is gated inside save_all, before any I/O).
            "image_generate" => Ok(image::generate(
                self.fs.as_ref(),
                self.image_http.as_deref(),
                &self.image_keys,
                self.clock.as_ref(),
                self.emitter.as_ref(),
                &self.fs_boundary,
                args,
            )
            .await),
            "tts_generate" => Ok(self.route_tts(args).await),
            // introspection 2
            "compose" => Ok(core_tools::compose()),
            "inspect" => Ok(inspect::inspect(self.workflow.as_ref(), args)),
            _ => Err(ToolExecError::NotFound {
                name: name.to_owned(),
            }),
        }
    }

    /// Enforce the `permits.fs` boundary for every `access` direction the
    /// op needs on its `path:` arg, THEN run the builtin. A path that
    /// resolves outside the boundary fails (`NIKA-SEC-004`) before the op —
    /// the capability boundary, not the I/O, is the gate. When `path:` is
    /// absent the op runs (its own arg ladder surfaces the missing-arg
    /// error · the boundary has nothing to confine).
    /// The `nika:tts_generate` plumbing (kept out of `route`'s match for
    /// the fn-length budget — pure delegation).
    async fn route_tts(&self, args: &Args) -> BuiltinOutcome {
        tts::generate(
            self.fs.as_ref(),
            self.tts_http.as_deref(),
            &self.tts_keys,
            self.clock.as_ref(),
            self.emitter.as_ref(),
            &self.fs_boundary,
            args,
        )
        .await
    }

    async fn guarded_fs<'a, Fut>(
        &'a self,
        args: &'a Args,
        accesses: &[permits::FsAccess],
        op: impl FnOnce(&'a F, &'a Args) -> Fut,
    ) -> BuiltinOutcome
    where
        Fut: std::future::Future<Output = BuiltinOutcome>,
    {
        if let Some(path) = args.get("path").and_then(serde_json::Value::as_str) {
            for &access in accesses {
                self.fs_boundary
                    .enforce(self.fs.as_ref(), path, access)
                    .await?;
            }
        }
        op(self.fs.as_ref(), args).await
    }

    /// `nika:grep` under the boundary — a recursive READ rooted at `path:`
    /// (default `.`). Gate the root, then run.
    async fn guarded_grep(&self, args: &Args) -> BuiltinOutcome {
        let root = args
            .get("path")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(".");
        self.fs_boundary
            .enforce(self.fs.as_ref(), root, permits::FsAccess::Read)
            .await?;
        file::grep(self.fs.as_ref(), &self.fs_boundary, args).await
    }

    /// `nika:glob` under the boundary — the walk never crosses `..` nor
    /// follows symlinked dirs (kernel `glob` contract), so its hits are
    /// confined to the WALK ROOT's subtree. Gate on that root being readable:
    /// `.` for a relative pattern (the cwd subtree), the absolute literal-dir
    /// prefix for an absolute one — so an absolute glob (F2) that targets a
    /// path OUTSIDE the declared `permits.fs` boundary is refused (NIKA-SEC-004)
    /// before the walk, not silently honoured. When `pattern:` is absent the
    /// gate falls to `.` and the glob's own arg ladder surfaces the error.
    async fn guarded_glob(&self, args: &Args) -> BuiltinOutcome {
        let root = args
            .get("pattern")
            .and_then(serde_json::Value::as_str)
            .map_or(".", file::glob_walk_root);
        self.fs_boundary
            .enforce(self.fs.as_ref(), root, permits::FsAccess::Read)
            .await?;
        file::glob(self.fs.as_ref(), args).await
    }
}

/// Render a builtin outcome into the wire `ToolResult`.
///
/// A success carries BOTH planes (the MCP `content` + `structuredContent`
/// split · see `ToolResult`): `content` is the TEXT view an agent loop /
/// LLM reads (string verbatim · everything else compact-JSON), and
/// `structured` is the builtin's REAL typed value — so a `nika:glob`
/// array reaches `tasks.X.output` as an array, not a stringified one. The
/// two planes never disagree; `content` is just `structured` rendered to
/// text.
fn render(call_id: &str, outcome: BuiltinOutcome) -> ToolResult {
    match outcome {
        Ok(value) => {
            let content = match &value {
                // Strings carry verbatim (a read file is its content, not
                // a JSON-quoted string); everything else compact-JSON.
                serde_json::Value::String(text) => text.clone(),
                other => other.to_string(),
            };
            ToolResult::success(call_id, content).with_structured(value)
        }
        Err(failure) => {
            // Carry the failure's spec code + retry class on the result's
            // error-metadata plane (BUG-D) — the `invoke` verb reads it so a
            // transient `nika:fetch` (HTTP 503/429 · DNS · connection) stays
            // retryable and the author's `on_codes:` filters on the builtin's
            // own code (`NIKA-BUILTIN-FETCH-001`), instead of every tool
            // error flattening to a non-retryable `NIKA-451`. `content` keeps
            // the `<code> · <message>` text view the agent loop reads.
            ToolResult::error(call_id, format!("{} · {}", failure.code, failure.message))
                .with_error_meta(ToolErrorMeta::new(
                    Some(failure.code.to_owned()),
                    failure.transient,
                ))
        }
    }
}

impl<F, H, C, Em, P, W> ToolExecuteDyn for BuiltinDispatcher<F, H, C, Em, P, W>
where
    F: FsReadDyn + FsWriteDyn + FsListDyn + Send + Sync,
    H: HttpGetDyn + HttpPostDyn + Send + Sync,
    C: ClockDyn + Send + Sync,
    Em: Emitter,
    P: Prompter,
    W: WorkflowIntrospect,
{
    async fn execute(&self, call: ToolCall) -> Result<ToolResult, ToolExecError> {
        let outcome = self.route(&call.name, &call.input).await?;
        Ok(render(call.id.as_str(), outcome))
    }
}

impl<F, H, C, Em, P, W> ToolBatchDyn for BuiltinDispatcher<F, H, C, Em, P, W>
where
    F: FsReadDyn + FsWriteDyn + FsListDyn + Send + Sync,
    H: HttpGetDyn + HttpPostDyn + Send + Sync,
    C: ClockDyn + Send + Sync,
    Em: Emitter,
    P: Prompter,
    W: WorkflowIntrospect,
{
    async fn execute_batch(&self, calls: Vec<ToolCall>) -> Vec<Result<ToolResult, ToolExecError>> {
        let mut results = Vec::with_capacity(calls.len());
        for call in calls {
            results.push(self.execute(call).await);
        }
        results
    }
}

impl<F, H, C, Em, P, W> ToolDefinitionProviderDyn for BuiltinDispatcher<F, H, C, Em, P, W>
where
    F: FsReadDyn + FsWriteDyn + FsListDyn + Send + Sync,
    H: HttpGetDyn + HttpPostDyn + Send + Sync,
    C: ClockDyn + Send + Sync,
    Em: Emitter,
    P: Prompter,
    W: WorkflowIntrospect,
{
    async fn tool_defs(
        &self,
    ) -> Result<Vec<nika_kernel::ai::provider::ToolDef>, nika_kernel::ai::tool_defs::ToolDefsError>
    {
        Ok(defs::tool_defs())
    }
}

// ─── shared arg helpers (dispatch-plane shape rules · crate spec §3) ────

pub(crate) type Args = serde_json::Map<String, serde_json::Value>;

/// A required string argument (the static `builtin_shape` ladder catches
/// violations pre-run; this is the runtime defense for the same shape).
pub(crate) fn req_str<'a>(
    args: &'a Args,
    key: &str,
    code: &'static str,
) -> Result<&'a str, BuiltinFailure> {
    args.get(key)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| BuiltinFailure::new(code, format!("`{key}:` (string) is required")))
}

/// An optional string argument (absent OR a string · anything else fails).
pub(crate) fn opt_str<'a>(
    args: &'a Args,
    key: &str,
    code: &'static str,
) -> Result<Option<&'a str>, BuiltinFailure> {
    match args.get(key) {
        None => Ok(None),
        Some(serde_json::Value::String(text)) => Ok(Some(text.as_str())),
        Some(_) => Err(BuiltinFailure::new(
            code,
            format!("`{key}:` must be a string when present"),
        )),
    }
}

/// An optional boolean with a default.
pub(crate) fn opt_bool(args: &Args, key: &str, default: bool) -> bool {
    args.get(key)
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(default)
}

#[cfg(test)]
pub(crate) mod test_rig {
    use super::*;
    use nika_kernel_mock::{MockClock, MockFs, MockHttp};

    /// The standard test dispatcher over kernel mocks + local nulls.
    pub(crate) type TestDispatcher =
        BuiltinDispatcher<MockFs, MockHttp, MockClock, NullEmitter, NonInteractive, NoWorkflow>;

    pub(crate) fn dispatcher(fs: MockFs, http: MockHttp, clock: MockClock) -> TestDispatcher {
        BuiltinDispatcher::new(
            Arc::new(fs),
            Arc::new(http),
            Arc::new(clock),
            Arc::new(NullEmitter),
            Arc::new(NonInteractive),
            Arc::new(NoWorkflow),
        )
    }

    pub(crate) fn call(name: &str, args: serde_json::Value) -> ToolCall {
        ToolCall::new("t-1", name, args)
    }
}

#[cfg(test)]
mod tests {
    use super::test_rig::{call, dispatcher};
    use super::*;
    use nika_kernel_mock::{MockClock, MockFs, MockHttp};

    fn rig() -> test_rig::TestDispatcher {
        dispatcher(MockFs::new(), MockHttp::new(), MockClock::new())
    }

    #[tokio::test]
    async fn unknown_builtin_is_dispatch_plane_not_found() {
        let err = rig()
            .execute(call("nika:ghost", serde_json::json!({})))
            .await
            .expect_err("unknown tool");
        assert!(matches!(err, ToolExecError::NotFound { name } if name == "nika:ghost"));
    }

    #[tokio::test]
    async fn the_namespace_prefix_is_required() {
        // A bare `uuid` (model dropped the prefix) is NotFound — corrective
        // feedback, not a silently-forgiven alias (the spec's invocation
        // surface is `tool: "nika:<name>"`).
        let bare = rig()
            .execute(call("uuid", serde_json::json!({})))
            .await
            .expect_err("bare name");
        assert!(matches!(bare, ToolExecError::NotFound { name } if name == "uuid"));
        // Foreign namespaces never route here either.
        let mcp = rig()
            .execute(call("mcp:server/tool", serde_json::json!({})))
            .await
            .expect_err("foreign namespace");
        assert!(matches!(mcp, ToolExecError::NotFound { .. }));
    }

    #[tokio::test]
    async fn non_object_args_are_dispatch_plane_failures() {
        let err = rig()
            .execute(call("nika:hash", serde_json::json!("not-an-object")))
            .await
            .expect_err("bad arg shape");
        assert!(matches!(err, ToolExecError::ExecutionFailed { .. }));
    }

    #[tokio::test]
    async fn done_is_rejected_outside_an_agent_loop() {
        let result = rig()
            .execute(call("nika:done", serde_json::json!({})))
            .await
            .expect("dispatches");
        assert!(result.is_error);
        assert!(
            result.content.starts_with("NIKA-BUILTIN-DONE-001"),
            "{}",
            result.content
        );
    }

    #[tokio::test]
    async fn every_registered_def_routes_and_every_route_is_defined() {
        // Closure both ways: each advertised ToolDef name dispatches to a
        // real builtin (NotFound would betray a registry/route drift)…
        let rig = rig();
        for def in tool_defs() {
            if def.name == "nika:done" {
                continue; // routes, but always rejects — asserted above
            }
            let outcome = rig.execute(call(&def.name, serde_json::json!({}))).await;
            assert!(outcome.is_ok(), "{} must route (got {outcome:?})", def.name);
        }
        // …and the count is the canonical 25 (stdlib v0.1 · +compose per
        // ADR-096 · +image_generate stdlib §Media · +tts_generate §Audio).
        assert_eq!(tool_defs().len(), 25);
    }

    #[tokio::test]
    async fn batch_is_the_sequential_map() {
        let results = rig()
            .execute_batch(vec![
                call("nika:uuid", serde_json::json!({})),
                call("nika:ghost", serde_json::json!({})),
            ])
            .await;
        assert_eq!(results.len(), 2);
        assert!(results[0].is_ok());
        assert!(results[1].is_err());
    }

    #[tokio::test]
    async fn fetch_through_the_dispatcher_extracts_markdown() {
        // The agent's REAL path: ToolCall → dispatcher → fetch → extract
        // (not the unit fn) — pins the route + the wire ToolResult shape.
        let http = MockHttp::new().enqueue_ok(
            200,
            b"<html><body><h1>Wire</h1><p>Through the dispatcher.</p></body></html>".to_vec(),
        );
        let dispatcher = test_rig::dispatcher(MockFs::new(), http, MockClock::new());
        let result = dispatcher
            .execute(call(
                "nika:fetch",
                serde_json::json!({ "url": "https://x.test" }),
            ))
            .await
            .expect("dispatches");
        assert!(!result.is_error, "{}", result.content);
        assert!(result.content.contains("# Wire"), "{}", result.content);
        assert!(result.content.contains("Through the dispatcher."));

        // And the failure plane: a spec-code failure rides ToolResult,
        // never the dispatch plane.
        let http = MockHttp::new().enqueue_ok(404, Vec::new());
        let dispatcher = test_rig::dispatcher(MockFs::new(), http, MockClock::new());
        let result = dispatcher
            .execute(call(
                "nika:fetch",
                serde_json::json!({ "url": "https://x.test" }),
            ))
            .await
            .expect("dispatches");
        assert!(result.is_error);
        assert!(
            result.content.starts_with("NIKA-BUILTIN-FETCH-001"),
            "{}",
            result.content
        );
    }

    #[tokio::test]
    async fn tool_defs_seam_serves_the_catalog() {
        let defs = ToolDefinitionProviderDyn::tool_defs(&rig())
            .await
            .expect("enumerates");
        assert_eq!(defs.len(), 25);
        assert!(defs.iter().all(|d| d.name.starts_with(NAMESPACE)));
        assert!(defs.iter().all(|d| !d.description.is_empty()));
        assert!(
            defs.iter().all(|d| d.parameters.is_object()),
            "every def carries a JSON-Schema params object"
        );
    }

    #[test]
    fn non_interactive_never_answers() {
        // Pins all three NonInteractive methods at None (the headless
        // contract that forces `default:`-or-fail upstream).
        assert_eq!(NonInteractive.confirm("?"), None);
        assert_eq!(NonInteractive.input("?"), None);
        assert_eq!(NonInteractive.choice("?", &["a".to_owned()]), None);
    }

    #[test]
    fn string_success_carries_verbatim_and_structured_serializes() {
        // content = the TEXT view (string verbatim · everything else
        // compact-JSON); structured = the REAL typed value (the invoke
        // dataflow's signal · BUG#3 fix). The two planes agree.
        let text = render("t", Ok(serde_json::json!("hello")));
        assert_eq!(text.content, "hello");
        assert_eq!(text.structured, Some(serde_json::json!("hello")));
        let value = render("t", Ok(serde_json::json!({"a": 1})));
        assert_eq!(value.content, r#"{"a":1}"#);
        assert_eq!(value.structured, Some(serde_json::json!({"a": 1})));
        let null = render("t", Ok(serde_json::Value::Null));
        assert_eq!(null.content, "null");
        assert_eq!(null.structured, Some(serde_json::Value::Null));
        let failed = render("t", Err(BuiltinFailure::new("NIKA-BUILTIN-X-001", "boom")));
        assert!(failed.is_error);
        assert_eq!(failed.content, "NIKA-BUILTIN-X-001 · boom");
        // A failure has no typed value — the dataflow never sees a structured
        // error (errors travel the task-error channel, not tasks.X.output).
        assert!(failed.structured.is_none());
    }

    #[test]
    fn render_failure_carries_spec_code_and_transient_metadata() {
        // BUG-D: the failure's spec code + retry class ride the result's
        // error-metadata plane so the invoke verb can filter `on_codes:` on
        // the builtin's own code and retry a transient failure — not flatten
        // every tool error to a non-retryable NIKA-451.
        let transient = render(
            "t",
            Err(BuiltinFailure::new("NIKA-BUILTIN-FETCH-001", "HTTP 503").with_transient(true)),
        );
        let meta = transient
            .error_meta
            .expect("transient failure carries metadata");
        assert_eq!(meta.spec_code.as_deref(), Some("NIKA-BUILTIN-FETCH-001"));
        assert!(meta.transient, "a 503 is retryable");

        // A non-transient failure carries its code with transient: false.
        let terminal = render(
            "t",
            Err(BuiltinFailure::new("NIKA-BUILTIN-FETCH-001", "HTTP 404")),
        );
        let meta = terminal
            .error_meta
            .expect("terminal failure carries metadata");
        assert_eq!(meta.spec_code.as_deref(), Some("NIKA-BUILTIN-FETCH-001"));
        assert!(!meta.transient, "a 404 is not retryable");

        // A success carries no error metadata.
        assert!(
            render("t", Ok(serde_json::json!("ok")))
                .error_meta
                .is_none()
        );
    }

    #[tokio::test]
    async fn glob_through_the_dispatcher_carries_a_structured_array() {
        // The BUG#3 seam: a builtin that returns Value::Array must surface
        // its array on the STRUCTURED plane (so for_each iterates it) while
        // content stays the JSON text (the model's view). Pins the dispatcher
        // path end-to-end, not just render(). Files keyed under "./" so the
        // mock's root `.` walk finds them (the file.rs glob idiom).
        let fs = MockFs::new()
            .with_file("./docs/a.md", "alpha")
            .with_file("./docs/b.md", "beta");
        let dispatcher = test_rig::dispatcher(fs, MockHttp::new(), MockClock::new());
        let result = dispatcher
            .execute(call("nika:glob", serde_json::json!({ "pattern": "**" })))
            .await
            .expect("dispatches");
        assert!(!result.is_error, "{}", result.content);
        let structured = result.structured.expect("glob carries a structured value");
        let names = structured
            .as_array()
            .expect("structured glob output is an array");
        assert_eq!(names.len(), 2, "both files: {structured}");
        // And the text view is the same array rendered to compact JSON.
        assert_eq!(result.content, structured.to_string());
    }
}

/// The boundary enforcement through the REAL dispatcher route — proves the
/// `permits.fs` gate fires BEFORE the I/O on the production path, against a
/// true filesystem (`TokioFs` · `MockFs::canonicalize` is a no-op, so an
/// escape can only be demonstrated on the real impl). The DoS/security
/// regression for the `..`/symlink permits bypass.
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod boundary_dispatch_tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    use nika_fs::TokioFs;
    use nika_kernel::runtime::tool_executor::{ToolCall, ToolExecuteDyn};
    use nika_kernel_mock::{MockClock, MockHttp};

    use super::{BuiltinDispatcher, FsBoundary, NoWorkflow, NonInteractive, NullEmitter};

    type RealFsDispatcher =
        BuiltinDispatcher<TokioFs, MockHttp, MockClock, NullEmitter, NonInteractive, NoWorkflow>;

    static SEQ: AtomicU64 = AtomicU64::new(0);

    fn scratch() -> std::path::PathBuf {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("nika-builtin-bnd-{}-{n}", std::process::id()));
        std::fs::create_dir_all(root.join("allowed")).unwrap();
        std::fs::write(root.join("allowed/in.txt"), b"inside").unwrap();
        std::fs::write(root.join("secret.txt"), b"OUTSIDE").unwrap();
        root
    }

    fn dispatcher_with(boundary: FsBoundary) -> RealFsDispatcher {
        BuiltinDispatcher::new(
            Arc::new(TokioFs),
            Arc::new(MockHttp::new()),
            Arc::new(MockClock::new()),
            Arc::new(NullEmitter),
            Arc::new(NonInteractive),
            Arc::new(NoWorkflow),
        )
        .with_fs_boundary(boundary)
    }

    #[tokio::test]
    async fn write_traversal_is_refused_before_the_io() {
        let root = scratch();
        let boundary = FsBoundary::declared(vec![], vec![format!("{}/allowed/**", root.display())]);
        let dispatcher = dispatcher_with(boundary);
        let escape = root.join("allowed/../TRAVERSED.txt");
        let result = dispatcher
            .execute(ToolCall::new(
                "t",
                "nika:write",
                serde_json::json!({
                    "path": escape.to_string_lossy(),
                    "content": "pwn",
                }),
            ))
            .await
            .expect("dispatches");
        assert!(result.is_error, "the boundary must refuse the traversal");
        assert!(
            result.content.starts_with("NIKA-SEC-004"),
            "the permits-denied code: {}",
            result.content
        );
        // …and the I/O never happened (the file is not on disk OUTSIDE).
        assert!(
            !root.join("TRAVERSED.txt").exists(),
            "the gate is BEFORE the write — nothing escaped"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn read_inside_boundary_still_works_through_the_dispatcher() {
        let root = scratch();
        let boundary = FsBoundary::declared(vec![format!("{}/allowed/**", root.display())], vec![]);
        let dispatcher = dispatcher_with(boundary);
        let result = dispatcher
            .execute(ToolCall::new(
                "t",
                "nika:read",
                serde_json::json!({ "path": root.join("allowed/in.txt").to_string_lossy() }),
            ))
            .await
            .expect("dispatches");
        assert!(
            !result.is_error,
            "in-boundary read is allowed: {}",
            result.content
        );
        assert_eq!(result.content, "inside");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn read_traversal_to_a_real_outside_file_is_refused() {
        // The /etc/passwd-class leak: a `..` chain to a real file outside
        // the boundary must NOT be read (it would otherwise succeed).
        let root = scratch();
        let boundary = FsBoundary::declared(vec![format!("{}/allowed/**", root.display())], vec![]);
        let dispatcher = dispatcher_with(boundary);
        let leak = root.join("allowed/../secret.txt");
        let result = dispatcher
            .execute(ToolCall::new(
                "t",
                "nika:read",
                serde_json::json!({ "path": leak.to_string_lossy() }),
            ))
            .await
            .expect("dispatches");
        assert!(result.is_error, "the leak must be refused");
        assert!(
            result.content.starts_with("NIKA-SEC-004"),
            "{}",
            result.content
        );
        assert!(
            !result.content.contains("OUTSIDE"),
            "the outside file's bytes must never surface"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn grep_through_an_escaping_symlink_is_refused() {
        // P0 regression: `nika:grep` walks a directory, finds an IN-boundary
        // symlink leaf, then `read_to_string` FOLLOWS it. A link whose target
        // escapes the declared `permits.fs` must NOT leak that target's bytes
        // (the per-file sibling of `read`'s symlink-escape guard · grep was the
        // one fs builtin missing it · NIKA-SEC-004). Proven BOTH ways so the
        // test is not vacuous: bounded → skipped, unbounded → leaks (the walk
        // genuinely reaches and reads the link).
        let root = scratch();
        let link = root.join("allowed/leak");
        std::os::unix::fs::symlink(root.join("secret.txt"), &link).unwrap();
        let call = || {
            ToolCall::new(
                "t",
                "nika:grep",
                serde_json::json!({
                    "pattern": "OUTSIDE",
                    "path": root.join("allowed").to_string_lossy(),
                }),
            )
        };

        // Bounded to `allowed/**`: the escaping leaf is skipped like any
        // unreadable entry — the grep succeeds with zero hits, the secret's
        // bytes never surface.
        let boundary = FsBoundary::declared(vec![format!("{}/allowed/**", root.display())], vec![]);
        let bounded = dispatcher_with(boundary)
            .execute(call())
            .await
            .expect("dispatches");
        assert!(
            !bounded.is_error,
            "the in-boundary grep itself is fine: {}",
            bounded.content
        );
        assert!(
            !bounded.content.contains("OUTSIDE"),
            "the escaping symlink's target must never leak through grep: {}",
            bounded.content
        );

        // Unbounded (pre-permits floor): the SAME grep DOES follow the link and
        // surface the secret — the bypass the boundary closes.
        let leaked = dispatcher_with(FsBoundary::unbounded())
            .execute(call())
            .await
            .expect("dispatches");
        assert!(
            leaked.content.contains("OUTSIDE"),
            "without a boundary the link is followed (the bug the gate fixes): {}",
            leaked.content
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn grep_does_not_descend_an_escaping_dir_symlink() {
        // Defense-in-depth sibling of the leaf case: a symlink to a DIRECTORY
        // outside the boundary must not leak its contents either. TWO layers
        // stop it — (a) the kernel walk treats a symlinked dir as a
        // non-following leaf (never recurses · the property lives a crate away
        // in nika-fs), and (b) were it to recurse, the per-file guard would
        // refuse each escaped leaf. Run UNBOUNDED to isolate (a): if the walk
        // never follows even with no gate, the secret can never surface.
        let root = scratch();
        std::fs::create_dir_all(root.join("outside")).unwrap();
        std::fs::write(root.join("outside/loot.txt"), b"SECRET-LOOT").unwrap();
        std::os::unix::fs::symlink(root.join("outside"), root.join("allowed/dirlink")).unwrap();
        let result = dispatcher_with(FsBoundary::unbounded())
            .execute(ToolCall::new(
                "t",
                "nika:grep",
                serde_json::json!({
                    "pattern": "SECRET-LOOT",
                    "path": root.join("allowed").to_string_lossy(),
                }),
            ))
            .await
            .expect("dispatches");
        assert!(
            !result.content.contains("SECRET-LOOT"),
            "a symlinked dir outside the boundary must not be descended/leaked: {}",
            result.content
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn no_boundary_leaves_the_pre_permits_floor() {
        // With no boundary declared, the dispatcher does NOT gate fs ops
        // (today's behaviour) — a read outside "allowed" still works.
        let root = scratch();
        let dispatcher = dispatcher_with(FsBoundary::unbounded());
        let result = dispatcher
            .execute(ToolCall::new(
                "t",
                "nika:read",
                serde_json::json!({ "path": root.join("secret.txt").to_string_lossy() }),
            ))
            .await
            .expect("dispatches");
        assert!(
            !result.is_error,
            "no boundary → no gate: {}",
            result.content
        );
        assert_eq!(result.content, "OUTSIDE");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn absolute_glob_outside_the_boundary_is_refused() {
        // F2 security pin: now that an absolute pattern globs from its real
        // root (not silently `[]`), the boundary must gate THAT root — an
        // absolute glob aimed OUTSIDE the declared `permits.fs` must fail
        // before the walk, never silently leak the outside tree.
        let root = scratch();
        let boundary = FsBoundary::declared(vec![format!("{}/allowed/**", root.display())], vec![]);
        let dispatcher = dispatcher_with(boundary);
        // `<root>/**` walks `<root>` — NOT under `allowed/**` → refused.
        let outside = dispatcher
            .execute(ToolCall::new(
                "t",
                "nika:glob",
                serde_json::json!({ "pattern": format!("{}/**", root.display()) }),
            ))
            .await
            .expect("dispatches");
        assert!(outside.is_error, "an out-of-boundary glob must be refused");
        assert!(
            outside.content.starts_with("NIKA-SEC-004"),
            "{}",
            outside.content
        );
        // …while an absolute glob INSIDE the boundary still works.
        let inside = dispatcher
            .execute(ToolCall::new(
                "t",
                "nika:glob",
                serde_json::json!({ "pattern": format!("{}/allowed/**", root.display()) }),
            ))
            .await
            .expect("dispatches");
        assert!(
            !inside.is_error,
            "an in-boundary absolute glob is allowed: {}",
            inside.content
        );
        assert!(
            inside.content.contains("in.txt"),
            "the in-boundary file surfaces: {}",
            inside.content
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
