//! BuiltinToolRouter for nika:* tool dispatch.
//!
//! Provides routing for 28+ builtin tools across 5 categories:
//!
//! **Core (7):** sleep, log, emit, assert, prompt, run, complete
//! **File (5):** read, write, edit, glob, grep (requires ToolContext)
//! **Data (12):** jq, tree_data, inject, map, filter, group_by, enrich,
//!   json_merge, set_diff, zip, chunk, token_count
//! **Sprint2 (6):** json_verify, yaml_validate, locale_lookup, aggregate,
//!   json_flatten, json_unflatten
//! **Media (N):** import, dimensions, thumbhash, dominant_color, pipeline, ...

use super::media::{context::MediaToolContext, create_media_tool_adapters};
use super::{
    create_file_tool_adapters, AssertTool, BuiltinTool, CompleteTool, CostTool, EmitTool, LogTool,
    PromptTool, RecordsTool, RunTool, SleepTool,
};
use crate::error::NikaError;
use crate::tools::ToolContext;
use nika_event::EventLog;
use rustc_hash::FxHashMap;
use std::sync::Arc;

/// Router for builtin nika:* tools.
///
/// Dispatches tool calls to appropriate builtin implementations based on
/// the nika: prefix.
///
/// # Example
///
/// ```ignore
/// let router = BuiltinToolRouter::new();
///
/// // Check if tool is builtin
/// if BuiltinToolRouter::is_builtin("nika:sleep") {
///     let result = router.dispatch("nika:sleep", r#"{"duration":"1s"}"#).await?;
/// }
/// ```
pub struct BuiltinToolRouter {
    tools: FxHashMap<&'static str, Arc<dyn BuiltinTool>>,
}

impl BuiltinToolRouter {
    /// Create a new router with 7 core builtin tools (no file tools).
    ///
    /// For file tools (read, write, edit, glob, grep), use `with_file_tools()`.
    pub fn new() -> Self {
        let mut tools: FxHashMap<&'static str, Arc<dyn BuiltinTool>> = FxHashMap::default();

        // Register 7 core builtin tools
        tools.insert("sleep", Arc::new(SleepTool));
        tools.insert("log", Arc::new(LogTool));
        tools.insert("emit", Arc::new(EmitTool));
        tools.insert("assert", Arc::new(AssertTool));
        tools.insert("prompt", Arc::new(PromptTool::default()));
        tools.insert("run", Arc::new(RunTool));
        tools.insert("complete", Arc::new(CompleteTool));

        // Register 12 data processing tools (split into builtin/data/)
        use super::data::{
            ChunkTool, EnrichTool, FilterTool, GroupByTool, InjectTool, JqTool, JsonMergeTool,
            MapTool, SetDiffTool, TokenCountTool, TreeDataTool, ZipTool,
        };
        tools.insert("json_merge", Arc::new(JsonMergeTool));
        tools.insert("set_diff", Arc::new(SetDiffTool));
        tools.insert("zip", Arc::new(ZipTool));
        tools.insert("map", Arc::new(MapTool));
        tools.insert("filter", Arc::new(FilterTool));
        tools.insert("group_by", Arc::new(GroupByTool));
        tools.insert("chunk", Arc::new(ChunkTool));
        tools.insert("token_count", Arc::new(TokenCountTool));
        tools.insert("enrich", Arc::new(EnrichTool));
        tools.insert("jq", Arc::new(JqTool));
        tools.insert("tree_data", Arc::new(TreeDataTool));
        tools.insert("inject", Arc::new(InjectTool));

        // Register 6 Sprint 2 data tools
        use super::{
            AggregateTool, JsonFlattenTool, JsonUnflattenTool, JsonVerifyTool, LocaleLookupTool,
            YamlValidateTool,
        };
        tools.insert("json_verify", Arc::new(JsonVerifyTool));
        tools.insert("yaml_validate", Arc::new(YamlValidateTool));
        tools.insert("locale_lookup", Arc::new(LocaleLookupTool));
        tools.insert("aggregate", Arc::new(AggregateTool));
        tools.insert("json_flatten", Arc::new(JsonFlattenTool));
        tools.insert("json_unflatten", Arc::new(JsonUnflattenTool));

        Self { tools }
    }

    /// Create a router with all 12 builtin tools (7 core + 5 file tools).
    ///
    /// File tools require a `ToolContext` for working directory and permissions.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use std::sync::Arc;
    /// use nika::tools::{ToolContext, PermissionMode};
    ///
    /// let ctx = Arc::new(ToolContext::new(
    ///     std::env::current_dir().unwrap(),
    ///     PermissionMode::YoloMode,
    /// ));
    /// let router = BuiltinToolRouter::with_file_tools(ctx);
    ///
    /// // Now supports nika:read, nika:write, etc.
    /// assert!(router.has_tool("read"));
    /// assert!(router.has_tool("write"));
    /// ```
    pub fn with_file_tools(ctx: Arc<ToolContext>) -> Self {
        let mut router = Self::new();

        // Register 5 file tools via adapter
        for tool in create_file_tool_adapters(ctx) {
            router.tools.insert(tool.name(), Arc::from(tool));
        }

        router
    }

    /// Create a router with all builtin tools (7 core + 5 file + N media).
    ///
    /// Media tools require a `MediaToolContext` for CAS access, budget, and compute pool.
    pub fn with_all_tools(file_ctx: Arc<ToolContext>, media_ctx: Arc<MediaToolContext>) -> Self {
        let mut router = Self::with_file_tools(file_ctx);

        // Register media tools via adapter
        for tool in create_media_tool_adapters(media_ctx) {
            router.tools.insert(tool.name(), Arc::from(tool));
        }

        router
    }

    /// Check if a tool name is a builtin (has nika: prefix).
    ///
    /// # Example
    /// ```ignore
    /// assert!(BuiltinToolRouter::is_builtin("nika:sleep"));
    /// assert!(!BuiltinToolRouter::is_builtin("novanet:describe"));
    /// ```
    #[inline]
    pub fn is_builtin(tool_name: &str) -> bool {
        tool_name.starts_with("nika:")
    }

    /// Extract the tool name from a nika: prefixed string.
    ///
    /// Returns None if the string doesn't start with "nika:".
    ///
    /// # Example
    /// ```ignore
    /// assert_eq!(BuiltinToolRouter::extract_name("nika:sleep"), Some("sleep"));
    /// assert_eq!(BuiltinToolRouter::extract_name("novanet:x"), None);
    /// ```
    #[inline]
    pub fn extract_name(tool_name: &str) -> Option<&str> {
        tool_name.strip_prefix("nika:")
    }

    /// Check if the router has a specific tool registered.
    pub fn has_tool(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// Get all registered tool names.
    pub fn tool_names(&self) -> Vec<&'static str> {
        self.tools.keys().copied().collect()
    }

    /// Get a reference to a tool by name (without nika: prefix).
    pub fn get_tool(&self, name: &str) -> Option<&dyn BuiltinTool> {
        self.tools.get(name).map(|t| t.as_ref())
    }

    /// Register a builtin tool.
    pub fn register<T: BuiltinTool + 'static>(&mut self, tool: T) {
        self.tools.insert(tool.name(), Arc::new(tool));
    }

    /// Add nika:cost introspection tool (requires EventLog for token/cost queries).
    pub fn with_cost_tool(mut self, event_log: EventLog) -> Self {
        self.register(CostTool::new(event_log));
        self
    }

    /// Add nika:records introspection tool (requires RunContext for record queries).
    pub fn with_records_tool(mut self, datastore: Arc<crate::store::RunContext>) -> Self {
        self.register(RecordsTool::new(datastore));
        self
    }

    /// Add 4 introspection tools (dag_info, task_status, threads, orchestrate).
    pub fn with_introspection(
        mut self,
        event_log: EventLog,
        datastore: Arc<crate::store::RunContext>,
    ) -> Self {
        use super::{DagInfoTool, OrchestrateTool, TaskStatusTool, ThreadsTool};
        self.register(DagInfoTool::new(event_log.clone()));
        self.register(TaskStatusTool::new(
            event_log.clone(),
            Arc::clone(&datastore),
        ));
        self.register(ThreadsTool::new(event_log.clone()));
        self.register(OrchestrateTool::new(event_log, datastore));
        self
    }

    /// Dispatch a tool call to the appropriate builtin tool.
    ///
    /// # Arguments
    /// * `tool_name` - Full tool name with nika: prefix (e.g., "nika:sleep")
    /// * `args` - JSON-encoded arguments
    ///
    /// # Returns
    /// * `Ok(String)` - JSON-encoded result from the tool
    /// * `Err(NikaError)` - If tool not found or execution fails
    pub async fn dispatch(&self, tool_name: &str, args: String) -> Result<String, NikaError> {
        let name = Self::extract_name(tool_name).ok_or_else(|| NikaError::BuiltinToolError {
            tool: tool_name.into(),
            reason: "Not a builtin tool (missing nika: prefix)".into(),
        })?;

        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| NikaError::BuiltinToolError {
                tool: tool_name.into(),
                reason: format!("Unknown builtin tool: {}", name),
            })?;

        tool.call(args).await
    }
}

impl Default for BuiltinToolRouter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::PermissionMode;
    use tempfile::TempDir;

    fn setup_test_context() -> (TempDir, Arc<ToolContext>) {
        let temp_dir = TempDir::new().unwrap();
        let ctx = Arc::new(ToolContext::new(
            temp_dir.path().to_path_buf(),
            PermissionMode::YoloMode,
        ));
        (temp_dir, ctx)
    }

    #[test]
    fn test_router_is_builtin() {
        assert!(BuiltinToolRouter::is_builtin("nika:sleep"));
        assert!(BuiltinToolRouter::is_builtin("nika:log"));
        assert!(BuiltinToolRouter::is_builtin("nika:emit"));
        assert!(BuiltinToolRouter::is_builtin("nika:assert"));
        assert!(BuiltinToolRouter::is_builtin("nika:prompt"));
        assert!(BuiltinToolRouter::is_builtin("nika:run"));
        // File tools
        assert!(BuiltinToolRouter::is_builtin("nika:read"));
        assert!(BuiltinToolRouter::is_builtin("nika:write"));
        assert!(BuiltinToolRouter::is_builtin("nika:edit"));
        assert!(BuiltinToolRouter::is_builtin("nika:glob"));
        assert!(BuiltinToolRouter::is_builtin("nika:grep"));
        // Non-builtin
        assert!(!BuiltinToolRouter::is_builtin("novanet:describe"));
        assert!(!BuiltinToolRouter::is_builtin("sleep"));
        assert!(!BuiltinToolRouter::is_builtin(""));
    }

    #[test]
    fn test_router_extract_name() {
        assert_eq!(BuiltinToolRouter::extract_name("nika:sleep"), Some("sleep"));
        assert_eq!(BuiltinToolRouter::extract_name("nika:log"), Some("log"));
        assert_eq!(BuiltinToolRouter::extract_name("nika:emit"), Some("emit"));
        assert_eq!(
            BuiltinToolRouter::extract_name("nika:assert"),
            Some("assert")
        );
        assert_eq!(
            BuiltinToolRouter::extract_name("nika:prompt"),
            Some("prompt")
        );
        assert_eq!(BuiltinToolRouter::extract_name("nika:run"), Some("run"));
        assert_eq!(BuiltinToolRouter::extract_name("novanet:x"), None);
        assert_eq!(BuiltinToolRouter::extract_name("sleep"), None);
        assert_eq!(BuiltinToolRouter::extract_name(""), None);
    }

    #[test]
    fn test_router_new_has_6_core_tools() {
        let router = BuiltinToolRouter::new();
        assert!(router.has_tool("sleep"));
        assert!(router.has_tool("log"));
        assert!(router.has_tool("emit"));
        assert!(router.has_tool("assert"));
        assert!(router.has_tool("prompt"));
        assert!(router.has_tool("run"));
        assert!(router.has_tool("complete"));
        // new() does NOT include file tools
        assert!(!router.has_tool("read"));
        assert!(!router.has_tool("write"));
        // 7 core + 9 data tools + 6 sprint2 tools
        assert!(router.has_tool("json_merge"));
        assert!(router.has_tool("set_diff"));
        assert!(router.has_tool("zip"));
        assert!(router.has_tool("map"));
        assert!(router.has_tool("filter"));
        assert!(router.has_tool("group_by"));
        assert!(router.has_tool("chunk"));
        assert!(router.has_tool("token_count"));
        assert!(router.has_tool("enrich"));
        assert!(router.has_tool("json_verify"));
        assert!(router.has_tool("yaml_validate"));
        assert!(router.has_tool("locale_lookup"));
        assert!(router.has_tool("aggregate"));
        assert!(router.has_tool("json_flatten"));
        assert!(router.has_tool("json_unflatten"));
        assert!(router.has_tool("jq"));
        assert!(router.has_tool("tree_data"));
        assert_eq!(router.tool_names().len(), 25); // 7 core + 12 data + 6 sprint2
    }

    #[test]
    fn test_router_with_file_tools_has_16_tools() {
        let (_temp, ctx) = setup_test_context();
        let router = BuiltinToolRouter::with_file_tools(ctx);

        // 7 core tools (6 original + complete)
        assert!(router.has_tool("sleep"));
        assert!(router.has_tool("log"));
        assert!(router.has_tool("emit"));
        assert!(router.has_tool("assert"));
        assert!(router.has_tool("prompt"));
        assert!(router.has_tool("run"));
        assert!(router.has_tool("complete"));

        // 3 data tools
        assert!(router.has_tool("json_merge"));
        assert!(router.has_tool("set_diff"));
        assert!(router.has_tool("zip"));

        // 5 file tools
        assert!(router.has_tool("read"));
        assert!(router.has_tool("write"));
        assert!(router.has_tool("edit"));
        assert!(router.has_tool("glob"));
        assert!(router.has_tool("grep"));

        assert_eq!(router.tool_names().len(), 30); // 7 core + 12 data + 6 sprint2 + 5 file
    }

    #[test]
    fn test_router_register_tool() {
        struct TestTool;

        impl BuiltinTool for TestTool {
            fn name(&self) -> &'static str {
                "test"
            }

            fn call<'a>(
                &'a self,
                _args: String,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<String, NikaError>> + Send + 'a>,
            > {
                Box::pin(async { Ok("test result".to_string()) })
            }
        }

        let mut router = BuiltinToolRouter::new();
        router.register(TestTool);

        assert!(router.has_tool("test"));
        assert!(!router.has_tool("unknown"));
    }

    #[tokio::test]
    async fn test_router_dispatch_registered_tool() {
        struct TestTool;

        impl BuiltinTool for TestTool {
            fn name(&self) -> &'static str {
                "test"
            }

            fn call<'a>(
                &'a self,
                args: String,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<String, NikaError>> + Send + 'a>,
            > {
                Box::pin(async move { Ok(format!("received: {}", args)) })
            }
        }

        let mut router = BuiltinToolRouter::new();
        router.register(TestTool);

        let result = router
            .dispatch("nika:test", r#"{"hello":"world"}"#.to_string())
            .await;

        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        assert_eq!(result.unwrap(), r#"received: {"hello":"world"}"#);
    }

    #[tokio::test]
    async fn test_router_dispatch_unknown_tool() {
        let router = BuiltinToolRouter::new();

        let result = router.dispatch("nika:unknown", "{}".to_string()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Unknown builtin tool"));
    }

    #[tokio::test]
    async fn test_router_dispatch_not_builtin() {
        let router = BuiltinToolRouter::new();

        let result = router.dispatch("novanet:describe", "{}".to_string()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Not a builtin tool"));
    }

    #[test]
    fn test_router_default() {
        let router = BuiltinToolRouter::default();
        // Default router has 7 core + 12 data + 6 sprint2 tools
        assert_eq!(router.tool_names().len(), 25);
    }

    #[tokio::test]
    async fn test_router_dispatch_sleep() {
        let router = BuiltinToolRouter::new();
        let result = router
            .dispatch("nika:sleep", r#"{"duration":"1ms"}"#.to_string())
            .await;

        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(response["slept_for_ms"], 1);
    }

    #[tokio::test]
    async fn test_router_dispatch_log() {
        let router = BuiltinToolRouter::new();
        let result = router
            .dispatch(
                "nika:log",
                r#"{"level":"info","message":"test"}"#.to_string(),
            )
            .await;

        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(response["logged"], true);
    }

    #[tokio::test]
    async fn test_router_dispatch_emit() {
        let router = BuiltinToolRouter::new();
        let result = router
            .dispatch(
                "nika:emit",
                r#"{"name":"test_event","payload":{}}"#.to_string(),
            )
            .await;

        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(response["emitted"], true);
    }

    #[tokio::test]
    async fn test_router_dispatch_assert_true() {
        let router = BuiltinToolRouter::new();
        let result = router
            .dispatch("nika:assert", r#"{"condition":true}"#.to_string())
            .await;

        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(response["passed"], true);
    }

    #[tokio::test]
    async fn test_router_dispatch_assert_false() {
        let router = BuiltinToolRouter::new();
        let result = router
            .dispatch("nika:assert", r#"{"condition":false}"#.to_string())
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Assertion failed"));
    }

    #[tokio::test]
    async fn test_router_dispatch_prompt_headless() {
        let router = BuiltinToolRouter::new();
        // In headless mode with default, should use default
        let result = router
            .dispatch(
                "nika:prompt",
                r#"{"message":"Test?","default":"yes"}"#.to_string(),
            )
            .await;

        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(response["response"], "yes");
        assert_eq!(response["default_used"], true);
    }

    #[tokio::test]
    async fn test_router_dispatch_run_nonexistent_file() {
        let router = BuiltinToolRouter::new();
        let result = router
            .dispatch("nika:run", r#"{"workflow":"test.nika.yaml"}"#.to_string())
            .await;

        // Path canonicalization gives "resolve workflow path" error
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("resolve workflow path")
                || err.to_string().contains("not found")
        );
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // FILE TOOL DISPATCH TESTS (via with_file_tools router)
    // ═══════════════════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_router_dispatch_write_then_read() {
        let (temp_dir, ctx) = setup_test_context();
        let router = BuiltinToolRouter::with_file_tools(ctx);
        let file_path = temp_dir.path().join("test.txt");

        // Write file via router
        let write_args = serde_json::json!({
            "file_path": file_path.to_string_lossy(),
            "content": "Hello from router!"
        })
        .to_string();

        let result = router.dispatch("nika:write", write_args).await;
        assert!(result.is_ok(), "Write failed: {:?}", result);

        // Read file via router
        let read_args = serde_json::json!({
            "file_path": file_path.to_string_lossy()
        })
        .to_string();

        let result = router.dispatch("nika:read", read_args).await;
        assert!(result.is_ok(), "Read failed: {:?}", result);
        assert!(result.unwrap().contains("Hello from router!"));
    }

    #[tokio::test]
    async fn test_router_dispatch_glob() {
        let (temp_dir, ctx) = setup_test_context();
        let router = BuiltinToolRouter::with_file_tools(ctx);

        // Create test files
        std::fs::write(temp_dir.path().join("a.txt"), "a").unwrap();
        std::fs::write(temp_dir.path().join("b.txt"), "b").unwrap();
        std::fs::write(temp_dir.path().join("c.md"), "c").unwrap();

        let glob_args = serde_json::json!({
            "pattern": "*.txt",
            "path": temp_dir.path().to_string_lossy()
        })
        .to_string();

        let result = router.dispatch("nika:glob", glob_args).await;
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        let output = result.unwrap();
        assert!(output.contains("a.txt"));
        assert!(output.contains("b.txt"));
        assert!(!output.contains("c.md"));
    }

    #[tokio::test]
    async fn test_router_dispatch_grep() {
        let (temp_dir, ctx) = setup_test_context();
        let router = BuiltinToolRouter::with_file_tools(ctx);

        // Create test file
        std::fs::write(
            temp_dir.path().join("search.txt"),
            "Line 1: foo\nLine 2: bar\nLine 3: foo bar",
        )
        .unwrap();

        let grep_args = serde_json::json!({
            "pattern": "foo",
            "path": temp_dir.path().to_string_lossy()
        })
        .to_string();

        let result = router.dispatch("nika:grep", grep_args).await;
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        assert!(result.unwrap().contains("search.txt"));
    }

    #[tokio::test]
    async fn test_router_dispatch_file_tool_not_found_without_context() {
        // Router without file tools
        let router = BuiltinToolRouter::new();

        let result = router
            .dispatch(
                "nika:write",
                r#"{"file_path":"x","content":"y"}"#.to_string(),
            )
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Unknown builtin tool"));
    }

    /// Verify that KNOWN_BUILTIN_TOOLS in nika-core stays in sync with the router.
    ///
    /// Every tool in the base router (core + data + sprint2) must be in the catalog.
    /// Every non-contextual tool in the catalog must be in the base router.
    #[test]
    fn builtins_catalog_matches_router() {
        let router = BuiltinToolRouter::new();
        let router_names: std::collections::HashSet<&str> =
            router.tool_names().into_iter().collect();

        // Every tool in the router must be in KNOWN_BUILTIN_TOOLS
        for name in &router_names {
            assert!(
                nika_core::catalogs::builtins::is_known_builtin(name),
                "Router has tool '{}' not in KNOWN_BUILTIN_TOOLS catalog",
                name
            );
        }

        // Tools that require runtime context (not in base router)
        let context_dependent: std::collections::HashSet<&str> = [
            // File tools (need ToolContext)
            "read",
            "write",
            "edit",
            "glob",
            "grep",
            // Introspection tools (need DAG/EventLog)
            "dag_info",
            "task_status",
            "threads",
            "orchestrate",
            // Cost/Records (need EventLog/Datastore)
            "cost",
            "records",
            // Media always-on (need MediaToolContext)
            "import",
            "decode",
            "dimensions",
            "thumbhash",
            "dominant_color",
            // Media core
            "thumbnail",
            "convert",
            "strip",
            // Media opt-in
            "metadata",
            "optimize",
            "svg_render",
            "chart",
            "phash",
            "compare",
            "pdf_extract",
            "provenance",
            "verify",
            "qr_validate",
            "quality",
            "html_to_md",
            "css_select",
            "extract_metadata",
            "extract_links",
            "readability",
            "pipeline",
        ]
        .into_iter()
        .collect();

        // Every non-contextual tool in KNOWN_BUILTIN_TOOLS must be in the base router
        for name in nika_core::catalogs::builtins::KNOWN_BUILTIN_TOOLS {
            if context_dependent.contains(name) {
                continue;
            }
            assert!(
                router_names.contains(name),
                "Catalog has '{}' not in base router — add it to BuiltinToolRouter::new() \
                 or to the context_dependent set in this test",
                name
            );
        }
    }

    /// Guard: every tool registered in with_file_tools router exists in the catalog.
    ///
    /// This catches the case where a new tool is added to the router (core, data,
    /// sprint2, or file) but not to KNOWN_BUILTIN_TOOLS in nika-core.
    #[test]
    fn builtins_catalog_covers_router_tools() {
        let (_temp, ctx) = setup_test_context();
        let router = BuiltinToolRouter::with_file_tools(ctx);
        let router_names: Vec<&str> = router.tool_names();

        for name in &router_names {
            assert!(
                nika_core::catalogs::builtins::is_known_builtin(name),
                "Router tool '{}' missing from KNOWN_BUILTIN_TOOLS in nika-core — \
                 add it to nika-core/src/catalogs/builtins.rs",
                name
            );
        }

        // Sanity: file tools must be present (they are context-dependent)
        assert!(
            router_names.contains(&"read"),
            "with_file_tools router must include 'read'"
        );
        assert!(
            router_names.contains(&"write"),
            "with_file_tools router must include 'write'"
        );
    }

    /// Guard: explicit category-by-category check that known tools exist in the router.
    ///
    /// Unlike builtins_catalog_matches_router (which uses an allowlist for context-dependent
    /// tools), this test explicitly names every tool we expect and verifies the router has
    /// it. If a tool is removed from the router, this test fails loudly.
    #[test]
    fn builtins_catalog_core_in_router() {
        let (_temp, ctx) = setup_test_context();
        let router = BuiltinToolRouter::with_file_tools(ctx);

        // Core (7)
        let core_tools = [
            "sleep", "log", "emit", "assert", "prompt", "run", "complete",
        ];
        for name in &core_tools {
            assert!(
                router.has_tool(name),
                "Core tool '{}' missing from router",
                name
            );
        }

        // File (5)
        let file_tools = ["read", "write", "edit", "glob", "grep"];
        for name in &file_tools {
            assert!(
                router.has_tool(name),
                "File tool '{}' missing from router",
                name
            );
        }

        // Data (12)
        let data_tools = [
            "json_merge",
            "set_diff",
            "zip",
            "map",
            "filter",
            "group_by",
            "chunk",
            "token_count",
            "enrich",
            "jq",
            "tree_data",
            "inject",
        ];
        for name in &data_tools {
            assert!(
                router.has_tool(name),
                "Data tool '{}' missing from router",
                name
            );
        }

        // Sprint 2 (6)
        let sprint2_tools = [
            "json_verify",
            "yaml_validate",
            "locale_lookup",
            "aggregate",
            "json_flatten",
            "json_unflatten",
        ];
        for name in &sprint2_tools {
            assert!(
                router.has_tool(name),
                "Sprint2 tool '{}' missing from router",
                name
            );
        }

        // Verify expected tool count: 7 core + 5 file + 12 data + 6 sprint2 = 30
        assert_eq!(
            router.tool_names().len(),
            30,
            "Expected 30 tools (7 core + 5 file + 12 data + 6 sprint2), got {}",
            router.tool_names().len()
        );
    }
}
