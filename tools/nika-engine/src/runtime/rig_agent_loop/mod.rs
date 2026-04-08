// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Rig-based Agent Loop
//!
//! This module implements agentic execution using rig-core's AgentBuilder.
//! It replaces the custom agent_loop.rs with rig's native multi-turn support.
//!
//! ## Key Benefits
//! - Native tool calling via rig's ToolDyn trait
//! - Simpler codebase (rig handles the loop)
//! - Better provider abstraction (rig handles Claude/OpenAI/etc)
//!
//! ## Architecture
//! ```text
//! RigAgentLoop
//!   ├── Creates rig::Agent via AgentBuilder
//!   ├── Converts MCP tools to NikaMcpTool (implements ToolDyn)
//!   ├── Runs agent.chat() for multi-turn execution
//!   └── Emits events to EventLog for observability
//! ```
//!
//! ## Module Organization
//! - `types`: Status enums, result types, ToolChoice conversion
//! - `chat`: Chat history management and multi-turn conversation
//! - `streaming`: Streaming execution helpers for token tracking
//! - `thinking`: Extended thinking, guardrails, confidence routing
//! - `providers`: Provider-specific execution methods (run_*)

mod chat;
mod providers;
mod streaming;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_shield_mcp_wrap;
mod thinking;
pub mod types;

// Re-export public types
pub use types::{RigAgentLoopResult, RigAgentStatus};

use std::path::PathBuf;
use std::sync::Arc;

use rig::message::Message;
use rustc_hash::FxHashMap;

use crate::ast::AgentParams;
use crate::error::NikaError;
use crate::event::EventLog;
use crate::mcp::McpClient;
use crate::provider::rig::{AgentMediaStaging, NikaMcpTool, NikaMcpToolDef};
use crate::runtime::limit_tracker::LimitTracker;
use crate::runtime::submit_tool::DynamicSubmitTool;
use crate::runtime::SkillInjector;
use crate::tools::{
    EditTool, GlobTool, GrepTool, PermissionMode, ReadTool, ToolContext, WriteTool,
};

// ═══════════════════════════════════════════════════════════════════════════
// RigAgentLoop
// ═══════════════════════════════════════════════════════════════════════════

/// Rig-based agentic execution loop
///
/// Uses rig-core's AgentBuilder for multi-turn execution with MCP tools.
///
/// ## Chat History
///
/// The agent loop now supports conversation history for multi-turn interactions:
///
/// ```rust,ignore
/// let mut agent = RigAgentLoop::new(...)?;
///
/// // First turn
/// let result = agent.run().await?;
///
/// // Continue conversation with history
/// agent.add_to_history("What's the capital of France?", &result.final_output.to_string());
/// let result2 = agent.chat_continue("And what about Germany?").await?;
/// ```
pub struct RigAgentLoop {
    /// Task identifier for event logging
    task_id: String,
    /// Agent parameters from workflow YAML
    params: AgentParams,
    /// Event log for observability
    event_log: EventLog,
    _mcp_clients: FxHashMap<String, Arc<McpClient>>,
    /// Pre-built tools from MCP clients
    tools: Vec<Arc<dyn rig::tool::ToolDyn>>,
    /// Conversation history for multi-turn chat.
    ///
    /// NOTE: This Vec is cloned on each `chat()` call because rig-core's API
    /// takes ownership. The clone is necessary to preserve history for future turns.
    /// Pre-allocated with capacity based on `max_turns` to minimize reallocations.
    history: Vec<Message>,
    /// Monotonically incrementing turn counter.
    /// Incremented in `add_to_history()` (one complete user+assistant exchange = one turn).
    /// Replaces the ambiguous `(history.len() / 2 + 1)` formula which yields identical
    /// values for even and odd history lengths due to integer division.
    turn_count: u32,
    /// Optional streaming channel for real-time token display
    stream_tx: Option<tokio::sync::mpsc::Sender<crate::provider::rig::StreamChunk>>,
    /// Skill injector for loading and caching skills
    skill_injector: Option<Arc<SkillInjector>>,
    /// Skills map from workflow definition (skill_name -> path)
    skills_map: Option<std::collections::HashMap<String, String>>,
    /// Base directory for resolving skill paths
    base_dir: Option<PathBuf>,
    /// Shared media staging for agent tool calls (H1 side-channel).
    /// Binary content blocks from MCP tools are collected here since
    /// rig's ToolDyn::call() returns String only.
    pub media_staging: AgentMediaStaging,
    /// Runtime limit tracker for cost/token/duration enforcement.
    /// Instantiated from `AgentParams.limits` if configured, otherwise unlimited.
    limit_tracker: LimitTracker,
    /// Cancellation token for cooperative shutdown.
    /// Root agents own a token; child agents (via SpawnAgentTool) receive
    /// a child_token() so cancelling the parent cascades to children.
    /// Stored to keep the token alive — child_token() refs depend on it.
    _cancel_token: tokio_util::sync::CancellationToken,
    /// Per-run Nika Shield context (Sprint 2 Item 2). When set, the
    /// agent loop runs canary detection on the assistant response AND
    /// the extended-thinking trace at the end of every streaming session.
    pub(crate) shield: Option<crate::runtime::shield::SecurityContext>,
}

impl std::fmt::Debug for RigAgentLoop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RigAgentLoop")
            .field("task_id", &self.task_id)
            .field("params", &self.params)
            .field("tool_count", &self.tools.len())
            .field("history_len", &self.history.len())
            .field("media_staged", &self.media_staging.len())
            .finish_non_exhaustive()
    }
}

// ═══════════════════════════════════════════════════════════
// ArcToolAdapter: wrap Arc<dyn ToolDyn> as Box<dyn ToolDyn>
// ═══════════════════════════════════════════════════════════

/// Adapter that wraps `Arc<dyn ToolDyn>` so it can be used as `Box<dyn ToolDyn>`.
///
/// rig-core's `AgentBuilder::tools()` takes `Vec<Box<dyn ToolDyn>>`.
/// We store tools as `Vec<Arc<dyn ToolDyn>>` so they survive across multiple
/// agent runs (chat_continue, retries). This adapter clones the Arc cheaply
/// and delegates all trait methods.
struct ArcToolAdapter(Arc<dyn rig::tool::ToolDyn>);

impl rig::tool::ToolDyn for ArcToolAdapter {
    fn name(&self) -> String {
        self.0.name()
    }

    fn definition<'a>(
        &'a self,
        prompt: String,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = rig::completion::ToolDefinition> + Send + 'a>,
    > {
        self.0.definition(prompt)
    }

    fn call<'a>(
        &'a self,
        args: String,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<String, rig::tool::ToolError>> + Send + 'a>,
    > {
        self.0.call(args)
    }
}

impl RigAgentLoop {
    /// Strip known provider prefix from model name.
    ///
    /// Users may write `model: openai/gpt-4o` or `model: claude/claude-sonnet-4-6`.
    /// The API expects just `gpt-4o` or `claude-sonnet-4-6`.
    fn strip_model_prefix(model: &str) -> &str {
        const PREFIXES: &[&str] = &[
            "anthropic/",
            "claude/",
            "openai/",
            "mistral/",
            "groq/",
            "deepseek/",
            "gemini/",
            "google/",
            "xai/",
        ];
        for prefix in PREFIXES {
            if let Some(stripped) = model.strip_prefix(prefix) {
                return stripped;
            }
        }
        model
    }

    /// Build `additional_params` JSON for stop_sequences injection.
    ///
    /// rig-core has no native `.stop_sequences()` on AgentBuilder, but
    /// `additional_params` is `#[serde(flatten)]`-ed into the request body,
    /// so we inject the provider-specific key directly.
    fn stop_sequences_params(provider: &str, sequences: &[String]) -> Option<serde_json::Value> {
        if sequences.is_empty() {
            return None;
        }
        // Gemini nests stopSequences inside generationConfig
        if provider == "gemini" {
            return Some(serde_json::json!({
                "generationConfig": { "stopSequences": sequences }
            }));
        }
        let key = match provider {
            "anthropic" | "claude" => "stop_sequences",
            // OpenAI, Mistral, Groq, DeepSeek, xAI all use "stop"
            _ => "stop",
        };
        Some(serde_json::json!({ key: sequences }))
    }

    /// Create a new rig-based agent loop
    ///
    /// # Arguments
    /// * `workflow_base_dir` - Working directory for builtin file tools (glob, read, etc.).
    ///   When `Some`, file tools operate relative to this directory (the workflow file's parent).
    ///   When `None`, falls back to `std::env::current_dir()`.
    ///
    /// # Errors
    /// - NIKA-113: Empty prompt
    /// - NIKA-113: Invalid max_turns (0 or > 100)
    pub fn new(
        task_id: String,
        params: AgentParams,
        event_log: EventLog,
        mcp_clients: FxHashMap<String, Arc<McpClient>>,
        policy_enforcer: Option<Arc<parking_lot::RwLock<crate::runtime::policy::PolicyEnforcer>>>,
        workflow_base_dir: Option<PathBuf>,
    ) -> Result<Self, NikaError> {
        Self::new_with_shield(
            task_id,
            params,
            event_log,
            mcp_clients,
            policy_enforcer,
            workflow_base_dir,
            None,
            None,
        )
    }

    /// Sprint 2 Item 3c: same as `new()` but plumbs the per-run Nika Shield
    /// SecurityContext + the trusted MCP server allowlist so MCP tool
    /// descriptions from non-trusted servers are wrapped with the spotlight
    /// fence at agent construction time.
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_shield(
        task_id: String,
        params: AgentParams,
        event_log: EventLog,
        mcp_clients: FxHashMap<String, Arc<McpClient>>,
        policy_enforcer: Option<Arc<parking_lot::RwLock<crate::runtime::policy::PolicyEnforcer>>>,
        workflow_base_dir: Option<PathBuf>,
        shield: Option<crate::runtime::shield::SecurityContext>,
        trusted_mcp_servers: Option<Arc<[String]>>,
    ) -> Result<Self, NikaError> {
        // Validate params
        if params.prompt.is_empty() {
            return Err(NikaError::AgentValidationError {
                reason: format!("Agent prompt cannot be empty (task: {})", task_id),
            });
        }

        if let Some(max_turns) = params.max_turns {
            if max_turns == 0 {
                return Err(NikaError::AgentValidationError {
                    reason: format!("max_turns must be at least 1 (task: {})", task_id),
                });
            }
            if max_turns > 100 {
                return Err(NikaError::AgentValidationError {
                    reason: format!("max_turns cannot exceed 100 (task: {})", task_id),
                });
            }
        }

        // Create shared media staging for agent tool calls (H1 side-channel)
        let media_staging: AgentMediaStaging = Arc::new(dashmap::DashMap::new());

        // Root agents own a CancellationToken; child agents (via SpawnAgentTool)
        // will receive a child_token() so parent cancellation cascades.
        let cancel_token = tokio_util::sync::CancellationToken::new();

        // Build tools from MCP clients (with media staging, policy enforcement,
        // and Nika Shield Item 3c MCP description wrapping).
        let mut tools = Self::build_tools(
            &params.mcp,
            &mcp_clients,
            &media_staging,
            policy_enforcer.as_ref(),
            &event_log,
            shield.as_ref(),
            trusted_mcp_servers.as_deref(),
        )?;

        // Add spawn_agent tool if depth_limit allows spawning (MVP 8 Phase 2)
        // Default depth is 1 (root agent). Child agents get higher depths via spawn_agent.
        let current_depth = 1_u32;
        let max_depth = params.effective_depth_limit();
        if current_depth < max_depth {
            // BUG-1 fix: Use child_token() so cancelling the parent cascades to spawned agents.
            let spawn_tool = super::spawn::SpawnAgentTool::with_mcp(
                current_depth,
                max_depth,
                Arc::from(task_id.as_str()),
                event_log.clone(),
                mcp_clients.clone(),
                params.mcp.clone(),
                cancel_token.child_token(),
            )
            .with_parent_config(
                params.model.clone(),
                params.provider.clone(),
                params.temperature,
                params.tools.clone(),
            )
            .with_workflow_base_dir(workflow_base_dir.clone());
            // Propagate security policies to child agents
            let spawn_tool = if let Some(ref enforcer) = policy_enforcer {
                spawn_tool.with_policy(Arc::clone(enforcer))
            } else {
                spawn_tool
            };
            tools.push(Arc::new(spawn_tool));
        }

        // Agent scope controls which preset tool set is available.
        // Explicit `tools:` list ALWAYS takes priority over scope.
        // Scope only matters when no explicit nika:* tools are listed.
        //
        //   - "full" (default): all core + file tools
        //   - "minimal": only nika:complete + nika:log (simple Q&A agents)
        //   - "debug": all core + file + introspection tools
        let scope = params.scope.as_deref().unwrap_or("full");

        // Add builtin nika:* tools
        use super::builtin::{
            AssertTool, CompleteTool, CostTool, DagInfoTool, EmitTool, LogTool,
            NikaBuiltinToolAdapter, PromptTool, RunTool, SleepTool, TaskStatusTool, ThreadsTool,
        };

        // Create Arc wrappers for sharing with builtin tools
        // EventLog is Clone with Arc internals, so this is cheap.
        let event_log_arc = Arc::new(event_log.clone());
        let task_id_arc: Arc<str> = task_id.as_str().into();

        // Filter builtin tools based on params.tools
        // "builtin" keyword means ALL builtin tools (core + file)
        let all_builtins_requested = params.tools.iter().any(|t| t == "builtin");

        // Extract the nika:* tools from params.tools for filtering
        let requested_nika_tools: Vec<&str> = params
            .tools
            .iter()
            .filter(|t| t.starts_with("nika:"))
            .map(|t| t.as_str())
            .collect();

        // Determine if explicit tool filtering is active
        let has_explicit_filter = !requested_nika_tools.is_empty();

        // Helper: check if a tool should be added based on scope + explicit filter
        let should_add_core = |name: &str| -> bool {
            if has_explicit_filter {
                let full_name = format!("nika:{}", name);
                return requested_nika_tools.contains(&full_name.as_str());
            }
            // No explicit filter — use scope
            match scope {
                "minimal" => matches!(name, "complete" | "log"),
                _ => true, // "full" and "debug" get all core tools
            }
        };

        // Core builtin tools (filtered by scope or explicit list)
        if should_add_core("sleep") {
            tools.push(Arc::new(NikaBuiltinToolAdapter::new(Arc::new(SleepTool))));
        }
        if should_add_core("log") {
            tools.push(Arc::new(
                NikaBuiltinToolAdapter::new(Arc::new(LogTool))
                    .with_event_log(Arc::clone(&event_log_arc), Arc::clone(&task_id_arc)),
            ));
        }
        if should_add_core("emit") {
            tools.push(Arc::new(
                NikaBuiltinToolAdapter::new(Arc::new(EmitTool))
                    .with_event_log(Arc::clone(&event_log_arc), Arc::clone(&task_id_arc)),
            ));
        }
        if should_add_core("assert") {
            tools.push(Arc::new(NikaBuiltinToolAdapter::new(Arc::new(AssertTool))));
        }
        if should_add_core("prompt") {
            tools.push(Arc::new(NikaBuiltinToolAdapter::new(Arc::new(
                PromptTool::default(),
            ))));
        }
        if should_add_core("run") {
            tools.push(Arc::new(NikaBuiltinToolAdapter::new(Arc::new(RunTool))));
        }
        if should_add_core("complete") {
            tools.push(Arc::new(NikaBuiltinToolAdapter::new(Arc::new(
                CompleteTool,
            ))));
        }

        // nika:fetch — HTTP requests for agents (requires policy enforcer for SSRF protection)
        if should_add_core("fetch") {
            if let Some(ref enforcer) = policy_enforcer {
                use super::builtin::FetchTool;
                tools.push(Arc::new(NikaBuiltinToolAdapter::new(Arc::new(
                    FetchTool::new(Arc::clone(enforcer), event_log.clone()),
                ))));
            }
        }

        // Introspection tools — available in "debug" scope or when explicitly requested
        let add_introspection = scope == "debug" && !has_explicit_filter;
        let should_add_introspect = |name: &str| -> bool {
            if has_explicit_filter {
                let full_name = format!("nika:{}", name);
                return requested_nika_tools.contains(&full_name.as_str());
            }
            add_introspection
        };

        if should_add_introspect("dag_info") {
            tools.push(Arc::new(NikaBuiltinToolAdapter::new(Arc::new(
                DagInfoTool::new(event_log.clone()),
            ))));
        }
        if should_add_introspect("task_status") {
            // TaskStatusTool needs a RunContext — create a minimal one for the agent.
            // Cli source is fine: the agent ctx is empty (no inputs/with bindings),
            // so the trust floor never matters.
            let agent_ctx = Arc::new(crate::store::RunContext::new(
                nika_core::trust::InvocationSource::Cli,
            ));
            tools.push(Arc::new(NikaBuiltinToolAdapter::new(Arc::new(
                TaskStatusTool::new(event_log.clone(), agent_ctx),
            ))));
        }
        if should_add_introspect("threads") {
            tools.push(Arc::new(NikaBuiltinToolAdapter::new(Arc::new(
                ThreadsTool::new(event_log.clone()),
            ))));
        }
        if should_add_introspect("cost") {
            tools.push(Arc::new(NikaBuiltinToolAdapter::new(Arc::new(
                CostTool::new(event_log.clone()),
            ))));
        }

        // File tools (nika:read, nika:write, nika:edit, nika:glob, nika:grep)
        // Available in "full" and "debug" scopes, or when explicitly requested.
        // NOT available in "minimal" scope (unless explicitly listed).
        let file_tools_requested: Vec<&str> = requested_nika_tools
            .iter()
            .filter(|t| {
                matches!(
                    **t,
                    "nika:read" | "nika:write" | "nika:edit" | "nika:glob" | "nika:grep"
                )
            })
            .copied()
            .collect();

        let add_file_tools = all_builtins_requested
            || !file_tools_requested.is_empty()
            || (scope != "minimal" && !has_explicit_filter);

        if add_file_tools {
            // Agent file tools (glob, read, grep) use the process cwd (project root)
            // so they can see all project files — same as invoke: nika:glob.
            // workflow_base_dir is only used for spawn tool propagation (line 295).
            let working_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/tmp"));
            let tool_ctx = Arc::new(ToolContext::new(working_dir, PermissionMode::Plan));

            use super::builtin::FileToolAdapter;

            let should_add_file = |name: &str| -> bool {
                if has_explicit_filter {
                    return all_builtins_requested || file_tools_requested.contains(&name);
                }
                // No explicit filter — scope controls (full/debug get all file tools)
                scope != "minimal"
            };

            if should_add_file("nika:read") {
                tools.push(Arc::new(NikaBuiltinToolAdapter::new(Arc::new(
                    FileToolAdapter::new(ReadTool::new(Arc::clone(&tool_ctx))),
                ))));
            }
            if should_add_file("nika:write") {
                tools.push(Arc::new(NikaBuiltinToolAdapter::new(Arc::new(
                    FileToolAdapter::new(WriteTool::new(Arc::clone(&tool_ctx))),
                ))));
            }
            if should_add_file("nika:edit") {
                tools.push(Arc::new(NikaBuiltinToolAdapter::new(Arc::new(
                    FileToolAdapter::new(EditTool::new(Arc::clone(&tool_ctx))),
                ))));
            }
            if should_add_file("nika:glob") {
                tools.push(Arc::new(NikaBuiltinToolAdapter::new(Arc::new(
                    FileToolAdapter::new(GlobTool::new(Arc::clone(&tool_ctx))),
                ))));
            }
            if should_add_file("nika:grep") {
                tools.push(Arc::new(NikaBuiltinToolAdapter::new(Arc::new(
                    FileToolAdapter::new(GrepTool::new(tool_ctx)),
                ))));
            }
        }

        // PERF: Pre-allocate history capacity based on max_turns.
        // Each turn adds 2 messages (user + assistant), so capacity = max_turns * 2.
        // This reduces reallocations during conversation.
        let history_capacity = params.max_turns.unwrap_or(10) as usize * 2;

        // Initialize limit tracker from AgentParams.limits (or unlimited)
        let limit_tracker = match &params.limits {
            Some(limits_config) => LimitTracker::new(limits_config.clone()),
            None => LimitTracker::unlimited(),
        };

        Ok(Self {
            task_id,
            params,
            event_log,
            _mcp_clients: mcp_clients,
            tools,
            history: Vec::with_capacity(history_capacity),
            turn_count: 0,
            stream_tx: None,
            skill_injector: None,
            skills_map: None,
            base_dir: None,
            media_staging,
            limit_tracker,
            _cancel_token: cancel_token,
            shield,
        })
    }

    /// Set streaming channel for real-time token display
    ///
    /// When set, tokens will be sent to this channel as they arrive during streaming.
    /// This enables Claude Code-like real-time text display in the TUI.
    pub fn with_stream_tx(
        mut self,
        tx: tokio::sync::mpsc::Sender<crate::provider::rig::StreamChunk>,
    ) -> Self {
        self.stream_tx = Some(tx);
        self
    }

    /// Configure skill injection for this agent
    ///
    /// When set, skills defined in the workflow are loaded and prepended to
    /// the agent's system prompt before LLM calls.
    ///
    /// # Arguments
    /// * `injector` - Shared SkillInjector instance (with DashMap cache)
    /// * `skills_map` - Mapping of skill names to file paths from workflow YAML
    /// * `base_dir` - Base directory for resolving relative skill paths
    ///
    /// # Example
    /// ```ignore
    /// let agent = RigAgentLoop::new(task_id, params, log, mcp)?
    ///     .with_skills(
    ///         Arc::new(SkillInjector::new()),
    ///         skills_map,
    ///         PathBuf::from("/path/to/workflow"),
    ///     );
    /// ```
    pub fn with_skills(
        mut self,
        injector: Arc<SkillInjector>,
        skills_map: std::collections::HashMap<String, String>,
        base_dir: PathBuf,
    ) -> Self {
        self.skill_injector = Some(injector);
        self.skills_map = Some(skills_map);
        self.base_dir = Some(base_dir);
        self
    }

    /// Inject a `DynamicSubmitTool` for structured output enforcement.
    ///
    /// When the task has an output policy with a JSON schema, this adds
    /// `submit_result` as an available tool. Unlike `infer:` (which forces
    /// `tool_choice: Required`), the agent can call `submit_result` when
    /// ready — it's available but not forced.
    ///
    /// # Arguments
    /// * `schema` - JSON Schema as `serde_json::Value` for the expected output
    pub fn with_structured_output(mut self, schema: serde_json::Value) -> Self {
        // Validate schema is a proper JSON Schema object to prevent rig-core panics
        let schema = if schema.get("type").is_none() {
            tracing::warn!(
                task_id = %self.task_id,
                "output.schema missing 'type' field, wrapping in object schema"
            );
            // Wrap bare schema in a proper object schema
            serde_json::json!({
                "type": "object",
                "properties": {
                    "result": schema
                },
                "required": ["result"]
            })
        } else {
            schema
        };

        let submit_tool = DynamicSubmitTool::new(schema);
        self.tools.push(Arc::new(submit_tool));
        tracing::debug!(
            task_id = %self.task_id,
            "Added DynamicSubmitTool (submit_result) to agent tools"
        );
        self
    }

    // =========================================================================
    // Skill Injection
    // =========================================================================

    /// Inject skills into the system prompt
    ///
    /// If skills are configured via `with_skills()` and the agent has skills
    /// defined in `AgentParams.skills`, this method loads and prepends skill
    /// content to the base system prompt.
    ///
    /// # Returns
    /// - Enhanced prompt with skill content prepended, or
    /// - Original system prompt if no skills configured
    async fn inject_skills_into_prompt(&self) -> Result<String, NikaError> {
        let mut preamble = self
            .params
            .system
            .as_deref()
            .unwrap_or_default()
            .to_string();

        // Add tool routing guide when builtin tools are available
        if !self.tools.is_empty() {
            let tool_names: Vec<String> = self
                .tools
                .iter()
                .filter_map(|t| {
                    let name = t.name();
                    if name.starts_with("nika_") {
                        Some(name)
                    } else {
                        None
                    }
                })
                .collect();

            if !tool_names.is_empty() {
                preamble.push_str("\n\n## Available Tools\n");
                for name in &tool_names {
                    match name.as_str() {
                        "nika_read" => {
                            preamble.push_str("- nika_read: Read file contents from disk\n")
                        }
                        "nika_write" => {
                            preamble.push_str("- nika_write: Create a NEW file (fails if exists)\n")
                        }
                        "nika_edit" => preamble
                            .push_str("- nika_edit: Edit an EXISTING file by replacing text\n"),
                        "nika_glob" => {
                            preamble.push_str("- nika_glob: Find files matching a pattern\n")
                        }
                        "nika_grep" => {
                            preamble.push_str("- nika_grep: Search file contents with regex\n")
                        }
                        "nika_complete" => preamble.push_str(
                            "- nika_complete: Signal task completion with structured result\n",
                        ),
                        "nika_log" => preamble.push_str(
                            "- nika_log: Emit a log message (for observability only, not output)\n",
                        ),
                        "nika_emit" => {
                            preamble.push_str("- nika_emit: Emit a named event with payload\n")
                        }
                        "nika_run" => preamble.push_str("- nika_run: Execute a sub-workflow\n"),
                        _ => {}
                    }
                }
                preamble.push_str(
                    "\nUse the MOST SPECIFIC tool for each action. Call nika_complete when done.\n",
                );
            }
        }

        // Inject completion instructions from CompletionConfig
        // This tells the agent how to signal completion (explicit/pattern/natural)
        if let Some(ref completion_config) = self.params.completion {
            let instruction = completion_config.generate_system_instruction();
            if !instruction.is_empty() {
                preamble.push_str("\n\n## Completion Instructions\n");
                preamble.push_str(&instruction);
            }
        }

        // Check if skill injection is configured
        let (Some(injector), Some(skills_map), Some(base_dir)) =
            (&self.skill_injector, &self.skills_map, &self.base_dir)
        else {
            return Ok(preamble);
        };

        // Check if agent has skills defined
        let Some(skill_names) = &self.params.skills else {
            return Ok(preamble);
        };

        if skill_names.is_empty() {
            return Ok(preamble);
        }

        // Convert Vec<String> to &[&str] for the inject() API
        let skill_refs: Vec<&str> = skill_names.iter().map(|s| s.as_str()).collect();

        // Inject skills into the preamble
        let preamble_ref = if preamble.is_empty() {
            None
        } else {
            Some(preamble.as_str())
        };
        injector
            .inject(preamble_ref, &skill_refs, skills_map, base_dir)
            .await
    }

    /// Create boxed tool copies from Arc-stored tools.
    ///
    /// Each call produces fresh `Box<dyn ToolDyn>` wrappers around the same
    /// `Arc<dyn ToolDyn>` instances, so tools are never consumed.
    fn tools_as_boxed(&self) -> Vec<Box<dyn rig::tool::ToolDyn>> {
        self.tools
            .iter()
            .map(|t| Box::new(ArcToolAdapter(Arc::clone(t))) as Box<dyn rig::tool::ToolDyn>)
            .collect()
    }

    /// Drain collected media content blocks from all agent tool calls.
    ///
    /// Returns all ContentBlocks that were staged during the agent loop.
    /// The DashMap is drained (emptied) after this call.
    pub fn drain_media(&self) -> Vec<crate::mcp::types::ContentBlock> {
        let mut all_blocks = Vec::new();
        // Drain all entries from the staging map
        for entry in self.media_staging.iter() {
            all_blocks.extend(entry.value().iter().cloned());
        }
        self.media_staging.clear();
        all_blocks
    }

    /// Build NikaMcpTool instances from MCP clients with media staging and policy
    #[allow(clippy::too_many_arguments)]
    fn build_tools(
        mcp_names: &[String],
        mcp_clients: &FxHashMap<String, Arc<McpClient>>,
        media_staging: &AgentMediaStaging,
        policy_enforcer: Option<&Arc<parking_lot::RwLock<crate::runtime::policy::PolicyEnforcer>>>,
        event_log: &EventLog,
        shield: Option<&crate::runtime::shield::SecurityContext>,
        trusted_mcp_servers: Option<&[String]>,
    ) -> Result<Vec<Arc<dyn rig::tool::ToolDyn>>, NikaError> {
        let mut tools: Vec<Arc<dyn rig::tool::ToolDyn>> = Vec::new();

        for mcp_name in mcp_names {
            let client = mcp_clients
                .get(mcp_name)
                .ok_or_else(|| NikaError::McpNotConnected {
                    name: mcp_name.clone(),
                })?;

            // Get tool definitions from MCP client
            let tool_defs = client.get_tool_definitions();

            // Nika Shield Item 3c: wrap MCP tool descriptions with the
            // spotlight fence when the server is NOT in the trusted list.
            // Defeats malicious tool description injection (Attack #11).
            // Trusted servers (declared in [mcp.trusted] of nika.toml) bypass
            // the wrap so their descriptions stay clean for the LLM.
            let server_trusted = trusted_mcp_servers
                .map(|list| list.iter().any(|s| s == mcp_name))
                .unwrap_or(false);
            let wrap_descriptions =
                matches!(shield, Some(s) if s.spotlight_enabled()) && !server_trusted;

            for def in tool_defs {
                let raw_description = def.description.clone().unwrap_or_default();
                let description = if wrap_descriptions && !raw_description.is_empty() {
                    let fence = shield.unwrap().fence();
                    fence.wrap_untrusted(
                        &raw_description,
                        &format!("mcp:{mcp_name}/{}", def.name),
                        nika_core::trust::TrustLevel::Untrusted,
                    )
                } else {
                    raw_description
                };

                let mut tool = NikaMcpTool::with_media_staging(
                    NikaMcpToolDef {
                        name: def.name.clone(),
                        description,
                        input_schema: def
                            .input_schema
                            .clone()
                            .unwrap_or_else(|| serde_json::json!({"type": "object"})),
                    },
                    client.clone(),
                    Arc::clone(media_staging),
                );
                // Attach policy enforcer for security checks on agent tool calls
                if let Some(enforcer) = policy_enforcer {
                    tool = tool.with_policy(Arc::clone(enforcer), event_log.clone());
                }
                tools.push(Arc::new(tool));
            }
        }

        Ok(tools)
    }

    /// Get the number of tools available
    pub fn tool_count(&self) -> usize {
        self.tools.len()
    }
}
