// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-builtin` — the 22 canonical stdlib v0.1 builtins (L1.5).
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
pub mod defs;
pub mod file;
pub mod inspect;
pub mod net;

use std::sync::Arc;

use nika_kernel::ai::tool_defs::ToolDefinitionProviderDyn;
use nika_kernel::io::clock::ClockDyn;
use nika_kernel::io::fs::{FsListDyn, FsReadDyn, FsWriteDyn};
use nika_kernel::io::http::{HttpGetDyn, HttpPostDyn};
use nika_kernel::runtime::tool_executor::{
    ToolBatchDyn, ToolCall, ToolExecError, ToolExecuteDyn, ToolResult,
};

pub use defs::tool_defs;

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
}

impl BuiltinFailure {
    /// A non-transient failure (the default for every builtin).
    #[must_use]
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            transient: false,
        }
    }

    /// Mark this failure transient (retryable per the spec contract).
    #[must_use]
    pub fn with_transient(mut self, transient: bool) -> Self {
        self.transient = transient;
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

/// The empty-run introspection (no workflow context): every view returns
/// its shape with zero contents — honest emptiness, never an error (the
/// VIEW argument is still validated). Construct via [`Default`].
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct NoWorkflow;

impl WorkflowIntrospect for NoWorkflow {
    fn cost(&self) -> serde_json::Value {
        serde_json::json!({ "total_usd": 0.0, "by_task": {}, "by_provider": {} })
    }
    fn records(&self) -> serde_json::Value {
        serde_json::json!({ "tasks": [] })
    }
    fn dag_info(&self) -> serde_json::Value {
        serde_json::json!({ "nodes": [], "edges": [], "waves": [] })
    }
    fn threads(&self) -> serde_json::Value {
        serde_json::json!({ "active": 0, "queued": 0, "completed": 0 })
    }
}

// ─── the dispatcher ──────────────────────────────────────────────────────

/// The 22-builtin dispatcher over its six injected seams.
///
/// Kernel seams: `F` filesystem (file 5) · `H` http (fetch · notify) ·
/// `C` clock (wait · date). Local seams: `Em` event content (log · emit)
/// · `P` prompter (prompt) · `W` workflow introspection (inspect).
#[derive(Debug)]
pub struct BuiltinDispatcher<F, H, C, Em, P, W> {
    fs: Arc<F>,
    http: Arc<H>,
    clock: Arc<C>,
    emitter: Arc<Em>,
    prompter: Arc<P>,
    workflow: Arc<W>,
}

impl<F, H, C, Em, P, W> BuiltinDispatcher<F, H, C, Em, P, W> {
    /// Compose the dispatcher over its seams.
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
        }
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
            // file 5
            "read" => Ok(file::read(self.fs.as_ref(), args).await),
            "write" => Ok(file::write(self.fs.as_ref(), args).await),
            "edit" => Ok(file::edit(self.fs.as_ref(), args).await),
            "glob" => Ok(file::glob(self.fs.as_ref(), args).await),
            "grep" => Ok(file::grep(self.fs.as_ref(), args).await),
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
            "uuid" => Ok(data::uuid(args)),
            "date" => Ok(data::date(self.clock.as_ref(), args)),
            "hash" => Ok(data::hash(args)),
            // network 2
            "fetch" => Ok(net::fetch(self.http.as_ref(), args).await),
            "notify" => Ok(net::notify(self.http.as_ref(), args).await),
            // introspection 1
            "inspect" => Ok(inspect::inspect(self.workflow.as_ref(), args)),
            _ => Err(ToolExecError::NotFound {
                name: name.to_owned(),
            }),
        }
    }
}

/// Render a builtin outcome into the wire `ToolResult`.
fn render(call_id: &str, outcome: BuiltinOutcome) -> ToolResult {
    match outcome {
        Ok(value) => {
            let content = match &value {
                // Strings carry verbatim (a read file is its content, not
                // a JSON-quoted string); everything else compact-JSON.
                serde_json::Value::String(text) => text.clone(),
                other => other.to_string(),
            };
            ToolResult::success(call_id, content)
        }
        Err(failure) => {
            ToolResult::error(call_id, format!("{} · {}", failure.code, failure.message))
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
        // …and the count is the canonical 22 (stdlib v0.1).
        assert_eq!(tool_defs().len(), 22);
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
    async fn tool_defs_seam_serves_the_catalog() {
        let defs = ToolDefinitionProviderDyn::tool_defs(&rig())
            .await
            .expect("enumerates");
        assert_eq!(defs.len(), 22);
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
        let text = render("t", Ok(serde_json::json!("hello")));
        assert_eq!(text.content, "hello");
        let value = render("t", Ok(serde_json::json!({"a": 1})));
        assert_eq!(value.content, r#"{"a":1}"#);
        let null = render("t", Ok(serde_json::Value::Null));
        assert_eq!(null.content, "null");
        let failed = render("t", Err(BuiltinFailure::new("NIKA-BUILTIN-X-001", "boom")));
        assert!(failed.is_error);
        assert_eq!(failed.content, "NIKA-BUILTIN-X-001 · boom");
    }
}
