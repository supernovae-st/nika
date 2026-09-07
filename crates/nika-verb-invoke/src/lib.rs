// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! # nika-verb-invoke — the `invoke` verb executor (L2)
//!
//! Builtin (`nika:`) and MCP (`mcp:`) tool calls per
//! `nika-spec spec/02-verbs.md §invoke` — the third of the 4 verbs
//! (`infer · exec · invoke · agent` · locked forever per D-2026-05-22-N18).
//!
//! ## Shape
//!
//! - **Dispatcher injected, never owned** — the verb rides the kernel
//!   `ToolExecuteDyn` seam: production wiring injects the engine's
//!   builtin+MCP dispatcher (resolves `nika:*` against the closed builtin
//!   set + `mcp:*` against the configured server registry); tests inject a
//!   mock executor. NO Cargo dep on `nika-builtin` / `nika-mcp`.
//! - **The closed-namespace contract (spec §invoke)** — the tool-ref
//!   namespace set is CLOSED at v1: `nika:` and `mcp:` only. `mcp:` REQUIRES
//!   the slash (`mcp:postgres` alone is unresolvable). The verb does this
//!   lightweight semantic check BEFORE dispatch (the grammar SHAPE is the
//!   upstream `nika-schema` `NIKA-PARSE` concern).
//!
//! ## Fences (what this crate is NOT)
//!
//! The tool implementations (behind the dispatcher) · `${{ }}` resolution
//! (upstream) · args-schema validation (the tool owns its schema · spec
//! NIKA-INVOKE-002 reserved) · batch (`ToolBatchDyn` is the agent surface).
//!
//! ## Example (mock · zero tool)
//!
//! ```
//! use std::sync::Arc;
//! use nika_verb_invoke::{InvokeInput, InvokeVerb};
//! # use nika_kernel::tool_executor::{ToolCall, ToolExecError, ToolExecuteDyn, ToolResult};
//! # struct Echo;
//! # impl ToolExecuteDyn for Echo {
//! #     async fn execute(&self, call: ToolCall) -> Result<ToolResult, ToolExecError> {
//! #         Ok(ToolResult::success(call.id.to_string(), "ok"))
//! #     }
//! # }
//! # async fn demo() -> Result<(), Box<dyn std::error::Error>> {
//! let verb = InvokeVerb::new(Arc::new(Echo));
//! let out = verb.run(InvokeInput::new("nika:read")).await?;
//! # Ok(())
//! # }
//! ```

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

mod errors;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use nika_kernel::tool_executor::{ToolCall, ToolExecuteDyn, ToolRunStart};

pub use errors::VerbInvokeError;

// The closed namespace set moved to `nika_vocab::tool_ref::ToolNamespace`
// with the rest of the grammar — a second copy here is how the two readers
// drifted apart in the first place.

/// The `invoke` task input — spec fields, already CEL-resolved.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct InvokeInput {
    /// The tool id: `nika:<path>` or `mcp:<server>/<tool>` (required).
    pub tool: String,
    /// Tool arguments (default `{}` · tool-specific schema).
    pub args: serde_json::Value,
    /// Engine-supplied `ToolCall` id (derived from the tool id if absent).
    pub call_id: Option<String>,
}

impl InvokeInput {
    /// A no-args invocation of `tool`.
    #[must_use]
    pub fn new(tool: impl Into<String>) -> Self {
        Self {
            tool: tool.into(),
            args: serde_json::Value::Object(serde_json::Map::new()),
            call_id: None,
        }
    }
}

/// The `invoke` verb output.
///
/// `content` is the tool's TEXT response (what an agent loop feeds the
/// model); `structured` is the tool's typed value when it produced one
/// (the MCP `structuredContent` plane · `ToolResult::structured`). The
/// engine routes `structured` into `tasks.X.output` so a `nika:glob` array
/// stays an array for `for_each` / CEL navigation (spec 04 §`tasks.X.output`
/// · "string · object · or bytes · per verb"); a text-only MCP tool leaves
/// it `None` and the output stays a String.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct InvokeOutput {
    /// The tool's response content (`.output` in spec terms · the TEXT view).
    pub content: String,
    /// The tool's typed value when present (the dataflow plane · `None` for
    /// text-only tools).
    pub structured: Option<serde_json::Value>,
    /// The resolved tool id (echo · engine event context).
    pub tool: String,
    /// The tool's non-fatal diagnostic beside a success
    /// (`ToolResult::warning` · the OBS-E lane): the engine puts it on the
    /// task's terminal frame as `warning`; the value planes never carry it.
    pub warning: Option<String>,
}

impl InvokeOutput {
    /// Construct from the tool id and its response content (text-only · no
    /// structured value).
    #[must_use]
    pub fn new(tool: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            structured: None,
            tool: tool.into(),
            warning: None,
        }
    }

    /// Attach the tool's typed value (the dataflow plane). Builder over
    /// [`new`](Self::new): `content` stays the model-facing text view.
    #[must_use]
    pub fn with_structured(mut self, value: Option<serde_json::Value>) -> Self {
        self.structured = value;
        self
    }

    /// Attach the tool's non-fatal diagnostic (the OBS-E lane). Builder
    /// over [`new`](Self::new): the value planes stay exactly what the
    /// tool returned.
    #[must_use]
    pub fn with_warning(mut self, warning: Option<String>) -> Self {
        self.warning = warning;
        self
    }
}

/// Builtin/MCP tool call — the `invoke` verb executor.
#[derive(Debug)]
pub struct InvokeVerb<T> {
    executor: Arc<T>,
}

impl<T> InvokeVerb<T> {
    /// Create the verb over an injected tool executor.
    #[must_use]
    pub fn new(executor: Arc<T>) -> Self {
        Self { executor }
    }
}

impl<T> InvokeVerb<T>
where
    // `Sync` is inherited from the `ToolExecute` supertrait of the Dyn
    // variant — not restated here (review lens 1 · P2).
    T: ToolExecuteDyn,
{
    /// Execute the `invoke` task.
    ///
    /// CANCEL SAFETY: delegates to the tool's own contract
    /// (`ToolExecute::execute`) — per-tool, documented at the dispatcher.
    ///
    /// # Errors
    ///
    /// [`VerbInvokeError::UnresolvableTool`] when the tool id fails the
    /// closed-namespace/slash check or the dispatcher cannot find it ·
    /// [`VerbInvokeError::ToolReportedError`] when the tool returns
    /// `is_error: true` · [`VerbInvokeError::Dispatch`] on a dispatcher
    /// timeout/execution/availability failure.
    pub async fn run(&self, input: InvokeInput) -> Result<InvokeOutput, VerbInvokeError> {
        self.run_with_context(input, None).await
    }

    /// Execute an `invoke` task under one immutable execution-bound instant.
    ///
    /// Every retry constructs a fresh call carrying the same value; shared
    /// executors therefore need no mutable run binding.
    ///
    /// # Errors
    ///
    /// Exactly [`Self::run`]'s errors.
    pub async fn run_at(
        &self,
        input: InvokeInput,
        run_start: ToolRunStart,
    ) -> Result<InvokeOutput, VerbInvokeError> {
        self.run_with_context(input, Some(run_start)).await
    }

    async fn run_with_context(
        &self,
        input: InvokeInput,
        run_start: Option<ToolRunStart>,
    ) -> Result<InvokeOutput, VerbInvokeError> {
        validate_tool_ref(&input.tool)?;

        let call_id = input.call_id.unwrap_or_else(|| derive_call_id(&input.tool));
        let mut call = ToolCall::new(call_id, input.tool.clone(), input.args);
        if let Some(run_start) = run_start {
            call = call.with_run_start(run_start);
        }

        let result = self
            .executor
            .execute(call)
            .await
            .map_err(map_dispatch_error)?;

        if result.is_error {
            // Carry the tool's OWN failure metadata when it surfaced any
            // (BUG-D): the spec code the author filters on in `on_codes:`
            // (`NIKA-BUILTIN-FETCH-001`) + the retry class so a transient
            // tool failure (HTTP 503/429 · DNS · connection) stays
            // retryable. A text-only tool (no metadata) keeps the engine
            // `NIKA-451` code, non-transient (the prior behavior).
            return Err(match result.error_meta {
                Some(meta) => VerbInvokeError::tool_reported_coded(
                    input.tool,
                    &result.content,
                    meta.spec_code,
                    meta.transient,
                ),
                None => VerbInvokeError::tool_reported(input.tool, &result.content),
            });
        }
        // Carry the tool's typed value (when it has one) on the structured
        // plane — the engine routes it into tasks.X.output so a builtin
        // array/object survives as itself; `content` stays the TEXT view.
        Ok(InvokeOutput::new(input.tool, result.content)
            .with_structured(result.structured)
            .with_warning(result.warning))
    }
}

/// The tool-id grammar at the verb boundary (NIKA-450).
///
/// The rules live ONCE, at L0 in `nika_vocab::tool_ref`, because the
/// checker reads the same ids at author time. This function used to
/// carry its own copy, and the two disagreed in BOTH directions
/// (measured 2026-08-15):
///
/// ```text
/// nika:a:b         the CHECK refused it · this one accepted it
/// nika:x\u{7f}y    the CHECK PASSED it  · this one refused it
/// ```
///
/// The second is the half that mattered: a control character in a
/// forwarded tool name is a log-injection vector, and the checker waved
/// it through at the only moment an author is still reading.
///
/// The prior note here said the path "MAY itself contain `:`". It was
/// wrong against the spec (`02-verbs.md` §invoke · « the colon marks the
/// namespace boundary (exactly once) ») and is gone. Nothing checked
/// could carry a second colon anyway — the checker already refused it.
fn validate_tool_ref(tool: &str) -> Result<(), VerbInvokeError> {
    nika_vocab::tool_ref::parse(tool)
        .map(|_| ())
        .map_err(|defect| VerbInvokeError::UnresolvableTool {
            tool: tool.to_owned(),
            detail: defect.teaching().to_owned(),
        })
}

/// Process-monotonic disambiguator for derived call ids.
static DERIVED_CALL_SEQ: AtomicU64 = AtomicU64::new(0);

/// A `ToolCall` id when the engine did not supply one.
///
/// The engine is EXPECTED to supply `call_id` (its event-correlation id);
/// this fallback exists for direct/test use. It appends a process-monotonic
/// counter so two `invoke` tasks calling the SAME tool in one run do not
/// collide on the kernel's "unique call id" contract (review lens 1+3 · P1).
fn derive_call_id(tool: &str) -> String {
    let n = DERIVED_CALL_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("invoke:{tool}:{n}")
}

/// Map the kernel dispatcher error onto the verb surface. `NotFound` is a
/// resolution failure (NIKA-450); the rest are dispatch failures (452).
fn map_dispatch_error(source: nika_kernel::tool_executor::ToolExecError) -> VerbInvokeError {
    use nika_kernel::tool_executor::ToolExecError;
    match source {
        ToolExecError::NotFound { name } => VerbInvokeError::UnresolvableTool {
            tool: name,
            detail: "the dispatcher resolved no such tool".to_owned(),
        },
        other => VerbInvokeError::Dispatch { source: other },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_error::traits::NikaErrorCode;
    use nika_kernel::tool_executor::{
        ToolCall, ToolErrorMeta, ToolExecError, ToolExecuteDyn, ToolResult,
    };
    use std::sync::Mutex;

    /// A scripted tool executor recording the calls it receives. The
    /// scripted result is single-use (`ToolExecError` is not `Clone`); the
    /// run-once tests never re-enter.
    struct MockTool {
        result: Mutex<Option<Result<ToolResult, ToolExecError>>>,
        seen: Mutex<Vec<ToolCall>>,
    }

    impl MockTool {
        fn with(result: Result<ToolResult, ToolExecError>) -> Self {
            Self {
                result: Mutex::new(Some(result)),
                seen: Mutex::new(Vec::new()),
            }
        }
        fn ok(content: &str) -> Self {
            Self::with(Ok(ToolResult::success("tc", content)))
        }
        fn ok_structured(content: &str, value: serde_json::Value) -> Self {
            Self::with(Ok(ToolResult::success("tc", content).with_structured(value)))
        }
        fn err_result(content: &str) -> Self {
            Self::with(Ok(ToolResult::error("tc", content)))
        }
        /// An error result carrying the tool's own failure metadata (spec
        /// code + retry class · BUG-D).
        fn err_coded(content: &str, spec_code: &str, transient: bool) -> Self {
            Self::with(Ok(ToolResult::error("tc", content).with_error_meta(
                ToolErrorMeta::new(Some(spec_code.to_owned()), transient),
            )))
        }
        fn dispatch_err(e: ToolExecError) -> Self {
            Self::with(Err(e))
        }
    }

    impl ToolExecuteDyn for MockTool {
        async fn execute(&self, call: ToolCall) -> Result<ToolResult, ToolExecError> {
            self.seen.lock().unwrap().push(call);
            self.result.lock().unwrap().take().unwrap_or_else(|| {
                Err(ToolExecError::NotAvailable {
                    reason: "exhausted".into(),
                })
            })
        }
    }

    fn verb(mock: MockTool) -> InvokeVerb<MockTool> {
        InvokeVerb::new(Arc::new(mock))
    }

    #[tokio::test]
    async fn nika_builtin_happy_path() {
        let out = verb(MockTool::ok("file contents"))
            .run(InvokeInput::new("nika:read"))
            .await
            .expect("builtin resolves");
        assert_eq!(out, InvokeOutput::new("nika:read", "file contents"));
        // A text-only tool leaves the structured plane empty → the engine
        // keeps tasks.X.output a String (MCP-style tools without a typed value).
        assert!(out.structured.is_none());
    }

    #[tokio::test]
    async fn structured_tool_value_rides_the_output_structured_plane() {
        // BUG#3: a builtin returning a typed value (here an array, as nika:glob
        // does) must carry it on InvokeOutput.structured so the engine routes
        // it into tasks.X.output as an array — content stays the JSON text.
        let arr = serde_json::json!(["a.md", "b.md"]);
        let out = verb(MockTool::ok_structured("[\"a.md\",\"b.md\"]", arr.clone()))
            .run(InvokeInput::new("nika:glob"))
            .await
            .expect("builtin resolves");
        assert_eq!(out.content, "[\"a.md\",\"b.md\"]");
        assert_eq!(out.structured, Some(arr));
    }

    /// A tool's non-fatal diagnostic (`ToolResult::warning` · a `nika:glob`
    /// naming the directories it left out) crosses the seam beside the
    /// value planes, which stay exactly what the tool returned; a tool
    /// with nothing to say crosses with `None`.
    #[tokio::test]
    async fn a_tools_warning_crosses_the_seam_beside_an_unchanged_value() {
        let arr = serde_json::json!(["a.md"]);
        let said = "nika:glob returns files only · 1 directory also matched `*.md` and was left out: ./b.md";
        let out = verb(MockTool::with(Ok(ToolResult::success("tc", "[\"a.md\"]")
            .with_structured(arr.clone())
            .with_warning(said))))
        .run(InvokeInput::new("nika:glob"))
        .await
        .expect("builtin resolves");
        assert_eq!(out.content, "[\"a.md\"]");
        assert_eq!(out.structured, Some(arr.clone()));
        assert_eq!(out.warning.as_deref(), Some(said));
        let quiet = verb(MockTool::ok_structured("[\"a.md\"]", arr))
            .run(InvokeInput::new("nika:glob"))
            .await
            .expect("builtin resolves");
        assert!(quiet.warning.is_none(), "{:?}", quiet.warning);
    }

    #[tokio::test]
    async fn mcp_tool_happy_path_and_args_passthrough() {
        let mock = Arc::new(MockTool::ok("rows"));
        let verb = InvokeVerb::new(Arc::clone(&mock));
        let mut input = InvokeInput::new("mcp:postgres/query");
        input.args = serde_json::json!({ "sql": "SELECT 1" });
        let out = verb.run(input).await.expect("mcp resolves");
        assert_eq!(out.content, "rows");
        let seen = mock.seen.lock().unwrap();
        assert_eq!(seen[0].name, "mcp:postgres/query");
        assert_eq!(seen[0].input, serde_json::json!({ "sql": "SELECT 1" }));
    }

    #[tokio::test]
    async fn supplied_call_id_wins_over_derived() {
        let mock = Arc::new(MockTool::ok("x"));
        let verb = InvokeVerb::new(Arc::clone(&mock));
        let mut input = InvokeInput::new("nika:read");
        input.call_id = Some("engine-tc-42".to_owned());
        verb.run(input).await.expect("ok");
        assert_eq!(mock.seen.lock().unwrap()[0].id.to_string(), "engine-tc-42");
    }

    #[tokio::test]
    async fn derived_call_id_is_prefixed_and_unique() {
        async fn derived_id(content: &str) -> String {
            let mock = Arc::new(MockTool::ok(content));
            let verb = InvokeVerb::new(Arc::clone(&mock));
            verb.run(InvokeInput::new("nika:read")).await.expect("ok");
            mock.seen.lock().unwrap()[0].id.to_string()
        }
        let a = derived_id("1").await;
        let b = derived_id("2").await;
        // Same tool, no supplied id → distinct derived ids (kernel's
        // unique-call-id contract holds across repeated tool ids).
        assert!(a.starts_with("invoke:nika:read:"), "{a}");
        assert!(b.starts_with("invoke:nika:read:"), "{b}");
        assert_ne!(a, b, "derived ids disambiguate repeated tool ids");
    }

    #[tokio::test]
    async fn control_char_in_tool_id_is_rejected_before_dispatch() {
        let mock = Arc::new(MockTool::ok("x"));
        let verb = InvokeVerb::new(Arc::clone(&mock));
        for bad in [
            "nika:read\n",     // trailing whitespace
            "mcp:srv/tool\t",  // trailing whitespace
            " nika:read",      // leading whitespace
            "nika:re\u{07}ad", // BEL in the MIDDLE — only the < 0x20 byte rule catches it
            "nika:re\u{7f}ad", // DEL in the MIDDLE — only the == 0x7f byte rule catches it
        ] {
            let err = verb
                .run(InvokeInput::new(bad))
                .await
                .expect_err("control/whitespace rejected");
            assert!(
                matches!(err, VerbInvokeError::UnresolvableTool { .. }),
                "{bad:?} rejected"
            );
        }
        assert!(mock.seen.lock().unwrap().is_empty(), "zero dispatch");
    }

    #[tokio::test]
    async fn unknown_namespace_is_rejected_before_dispatch() {
        let mock = Arc::new(MockTool::ok("x"));
        let verb = InvokeVerb::new(Arc::clone(&mock));
        let err = verb
            .run(InvokeInput::new("custom:thing"))
            .await
            .expect_err("closed namespace");
        assert!(matches!(err, VerbInvokeError::UnresolvableTool { .. }));
        assert!(mock.seen.lock().unwrap().is_empty(), "zero dispatch");
    }

    #[tokio::test]
    async fn mcp_without_slash_is_rejected_before_dispatch() {
        let mock = Arc::new(MockTool::ok("x"));
        let verb = InvokeVerb::new(Arc::clone(&mock));
        let err = verb
            .run(InvokeInput::new("mcp:postgres"))
            .await
            .expect_err("mcp needs slash");
        assert!(matches!(err, VerbInvokeError::UnresolvableTool { .. }));
        assert!(mock.seen.lock().unwrap().is_empty(), "zero dispatch");
    }

    #[tokio::test]
    async fn tool_reported_error_maps_to_451() {
        let err = verb(MockTool::err_result("permission denied"))
            .run(InvokeInput::new("nika:read"))
            .await
            .expect_err("is_error propagates");
        assert!(matches!(err, VerbInvokeError::ToolReportedError { .. }));
        // A text-only tool error keeps the engine NIKA-451 code · non-transient
        // (the prior behavior — no surfaced metadata).
        assert_eq!(err.nika_code().to_string(), "NIKA-451");
        assert_eq!(err.spec_code(), "NIKA-451");
        assert!(!err.is_transient());
    }

    #[tokio::test]
    async fn transient_tool_failure_carries_its_code_and_is_retryable() {
        // BUG-D: a tool that surfaced its own failure metadata (a transient
        // `nika:fetch` 503) maps to a RETRYABLE error whose user-facing
        // spec_code is the tool's own code (`NIKA-BUILTIN-FETCH-001`) — the
        // identifier the author filters on in `on_codes:`. Previously every
        // tool error flattened to a non-retryable NIKA-451.
        let err = verb(MockTool::err_coded(
            "NIKA-BUILTIN-FETCH-001 · HTTP 503",
            "NIKA-BUILTIN-FETCH-001",
            true,
        ))
        .run(InvokeInput::new("nika:fetch"))
        .await
        .expect_err("is_error propagates");
        assert!(matches!(err, VerbInvokeError::ToolReportedError { .. }));
        assert!(err.is_transient(), "a 503 tool failure is retryable");
        assert_eq!(
            err.spec_code(),
            "NIKA-BUILTIN-FETCH-001",
            "on_codes: filters on the tool's own code"
        );
        // The engine numeric code is still NIKA-451 (the variant identity).
        assert_eq!(err.nika_code().to_string(), "NIKA-451");
    }

    #[tokio::test]
    async fn non_transient_tool_failure_carries_its_code_but_is_not_retryable() {
        // A 404 tool failure surfaces its code but stays non-retryable.
        let err = verb(MockTool::err_coded(
            "NIKA-BUILTIN-FETCH-001 · HTTP 404",
            "NIKA-BUILTIN-FETCH-001",
            false,
        ))
        .run(InvokeInput::new("nika:fetch"))
        .await
        .expect_err("is_error propagates");
        assert!(!err.is_transient(), "a 404 is not retryable");
        assert_eq!(err.spec_code(), "NIKA-BUILTIN-FETCH-001");
    }

    #[tokio::test]
    async fn dispatcher_notfound_maps_to_unresolvable() {
        let err = verb(MockTool::dispatch_err(ToolExecError::NotFound {
            name: "nika:ghost".to_owned(),
        }))
        .run(InvokeInput::new("nika:ghost"))
        .await
        .expect_err("not found");
        assert!(matches!(err, VerbInvokeError::UnresolvableTool { .. }));
    }

    #[tokio::test]
    async fn dispatcher_timeout_maps_to_dispatch() {
        let err = verb(MockTool::dispatch_err(ToolExecError::Timeout {
            name: "mcp:slow/op".to_owned(),
            duration_ms: 30_000,
        }))
        .run(InvokeInput::new("mcp:slow/op"))
        .await
        .expect_err("timeout");
        assert!(matches!(err, VerbInvokeError::Dispatch { .. }));
    }

    #[test]
    fn tool_ref_validation_matrix() {
        // `nika:connectome/recall` is verb-LEVEL valid (grammar shape ok);
        // the injected dispatcher rejects it against the closed builtin
        // set today (spec 02-verbs §invoke · the verb does shape, the
        // dispatcher does the closed-set check).
        assert!(validate_tool_ref("nika:read").is_ok());
        assert!(validate_tool_ref("nika:connectome/recall").is_ok());
        assert!(validate_tool_ref("mcp:db/query").is_ok());
        // ⭐ FLIPPED 2026-08-15. This line used to assert `is_ok()`, with
        // the comment « a second colon is the tool path's business ». That
        // was a design ASSERTION, not a requirement from any real tool, and
        // it contradicted the spec the checker quotes verbatim
        // (`02-verbs.md` §invoke · « the colon marks the namespace boundary
        // (exactly once) »). Two readers, two rules, and this one was lax
        // against its own spec — so the checker refused what the runtime
        // accepted. Nothing CHECKED could carry a second colon, so the
        // tightening breaks nothing that exists.
        assert!(validate_tool_ref("nika:a:b").is_err());
        // A space (0x20) in the MIDDLE is NOT a control char — accepted (only
        // < 0x20 and DEL are rejected; pins the byte-rule boundary exactly).
        assert!(validate_tool_ref("nika:re ad").is_ok());
        for bad in [
            "",
            "noprefix",
            "custom:x",
            "nika:",
            "mcp:",
            "mcp:db",
            "mcp:/q",
            "mcp:db/",
            "NIKA:read",
            "nika:read\n",
            " nika:read",
            "nika:read ",
        ] {
            assert!(validate_tool_ref(bad).is_err(), "{bad:?} must be rejected");
        }
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// The tool-ref validator is total: never panics on arbitrary input,
        /// and a valid `nika:`/`mcp:` shape is exactly the accept set.
        #[test]
        fn validate_tool_ref_is_total(s in ".{0,40}") {
            let ok = validate_tool_ref(&s).is_ok();
            // Cross-check against an independent predicate.
            let clean = !s.starts_with(char::is_whitespace)
                && !s.ends_with(char::is_whitespace)
                && !s.bytes().any(|b| b < 0x20 || b == 0x7f);
            let expected = clean && match s.split_once(':') {
                Some(("nika", path)) => !path.is_empty(),
                Some(("mcp", path)) => path
                    .split_once('/')
                    .is_some_and(|(srv, tool)| !srv.is_empty() && !tool.is_empty()),
                _ => false,
            };
            prop_assert_eq!(ok, expected);
        }
    }
}
