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
use crate::provider::rig::{NikaMcpTool, NikaMcpToolDef};
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
/// let result = agent.run_claude().await?;
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
    /// Connected MCP clients (used in run_claude for tool result callbacks)
    #[allow(dead_code)] // Will be used when run_claude is fully implemented
    mcp_clients: FxHashMap<String, Arc<McpClient>>,
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
}

impl std::fmt::Debug for RigAgentLoop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RigAgentLoop")
            .field("task_id", &self.task_id)
            .field("params", &self.params)
            .field("tool_count", &self.tools.len())
            .field("history_len", &self.history.len())
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
    /// Create a new rig-based agent loop
    ///
    /// # Errors
    /// - NIKA-113: Empty prompt
    /// - NIKA-113: Invalid max_turns (0 or > 100)
    pub fn new(
        task_id: String,
        params: AgentParams,
        event_log: EventLog,
        mcp_clients: FxHashMap<String, Arc<McpClient>>,
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

        // Build tools from MCP clients
        let mut tools = Self::build_tools(&params.mcp, &mcp_clients)?;

        // Add spawn_agent tool if depth_limit allows spawning (MVP 8 Phase 2)
        // Default depth is 1 (root agent). Child agents get higher depths via spawn_agent.
        let current_depth = 1_u32;
        let max_depth = params.effective_depth_limit();
        if current_depth < max_depth {
            let spawn_tool = super::spawn::SpawnAgentTool::with_mcp(
                current_depth,
                max_depth,
                Arc::from(task_id.as_str()),
                event_log.clone(),
                mcp_clients.clone(),
                params.mcp.clone(),
            );
            tools.push(Arc::new(spawn_tool));
        }

        // Add builtin nika:* tools
        // If params.tools is non-empty, only add tools that are explicitly requested.
        // If params.tools is empty, add all core tools.
        use super::builtin::{
            AssertTool, CompleteTool, EmitTool, LogTool, NikaBuiltinToolAdapter, PromptTool,
            RunTool, SleepTool,
        };

        // Create Arc wrappers for sharing with builtin tools
        // EventLog is Clone with Arc internals, so this is cheap.
        let event_log_arc = Arc::new(event_log.clone());
        let task_id_arc: Arc<str> = task_id.as_str().into();

        // Filter builtin tools based on params.tools
        // Extract the nika:* tools from params.tools for filtering
        let requested_nika_tools: Vec<&str> = params
            .tools
            .iter()
            .filter(|t| t.starts_with("nika:"))
            .map(|t| t.as_str())
            .collect();

        // Helper: check if a tool should be added
        // If no nika:* tools requested, add all core tools
        // Otherwise, only add if explicitly requested
        let should_add = |name: &str| -> bool {
            if requested_nika_tools.is_empty() {
                true // No filter specified, add all
            } else {
                let full_name = format!("nika:{}", name);
                requested_nika_tools.contains(&full_name.as_str())
            }
        };

        // Core builtin tools (only add if requested or no filter)
        if should_add("sleep") {
            tools.push(Arc::new(NikaBuiltinToolAdapter::new(Arc::new(SleepTool))));
        }
        if should_add("log") {
            tools.push(Arc::new(
                NikaBuiltinToolAdapter::new(Arc::new(LogTool))
                    .with_event_log(Arc::clone(&event_log_arc), Arc::clone(&task_id_arc)),
            ));
        }
        if should_add("emit") {
            tools.push(Arc::new(
                NikaBuiltinToolAdapter::new(Arc::new(EmitTool))
                    .with_event_log(Arc::clone(&event_log_arc), Arc::clone(&task_id_arc)),
            ));
        }
        if should_add("assert") {
            tools.push(Arc::new(NikaBuiltinToolAdapter::new(Arc::new(AssertTool))));
        }
        if should_add("prompt") {
            tools.push(Arc::new(NikaBuiltinToolAdapter::new(Arc::new(
                PromptTool::default(),
            ))));
        }
        if should_add("run") {
            tools.push(Arc::new(NikaBuiltinToolAdapter::new(Arc::new(RunTool))));
        }
        if should_add("complete") {
            tools.push(Arc::new(NikaBuiltinToolAdapter::new(Arc::new(
                CompleteTool,
            ))));
        }

        // Add file tools (nika:read, nika:write, nika:edit, nika:glob, nika:grep)
        // File tools require a ToolContext for security boundaries
        // Only add if explicitly requested in params.tools
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

        if !file_tools_requested.is_empty() {
            // Create ToolContext with current working directory and YoloMode
            // (agents need full access to perform their tasks)
            let working_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let tool_ctx = Arc::new(ToolContext::new(working_dir, PermissionMode::YoloMode));

            use super::builtin::FileToolAdapter;

            if file_tools_requested.contains(&"nika:read") {
                tools.push(Arc::new(NikaBuiltinToolAdapter::new(Arc::new(
                    FileToolAdapter::new(ReadTool::new(Arc::clone(&tool_ctx))),
                ))));
            }
            if file_tools_requested.contains(&"nika:write") {
                tools.push(Arc::new(NikaBuiltinToolAdapter::new(Arc::new(
                    FileToolAdapter::new(WriteTool::new(Arc::clone(&tool_ctx))),
                ))));
            }
            if file_tools_requested.contains(&"nika:edit") {
                tools.push(Arc::new(NikaBuiltinToolAdapter::new(Arc::new(
                    FileToolAdapter::new(EditTool::new(Arc::clone(&tool_ctx))),
                ))));
            }
            if file_tools_requested.contains(&"nika:glob") {
                tools.push(Arc::new(NikaBuiltinToolAdapter::new(Arc::new(
                    FileToolAdapter::new(GlobTool::new(Arc::clone(&tool_ctx))),
                ))));
            }
            if file_tools_requested.contains(&"nika:grep") {
                tools.push(Arc::new(NikaBuiltinToolAdapter::new(Arc::new(
                    FileToolAdapter::new(GrepTool::new(tool_ctx)),
                ))));
            }
        }

        // PERF: Pre-allocate history capacity based on max_turns.
        // Each turn adds 2 messages (user + assistant), so capacity = max_turns * 2.
        // This reduces reallocations during conversation.
        let history_capacity = params.max_turns.unwrap_or(10) as usize * 2;

        Ok(Self {
            task_id,
            params,
            event_log,
            mcp_clients,
            tools,
            history: Vec::with_capacity(history_capacity),
            turn_count: 0,
            stream_tx: None,
            skill_injector: None,
            skills_map: None,
            base_dir: None,
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
        let base_prompt = self.params.system.as_deref();

        // Check if skill injection is configured
        let (Some(injector), Some(skills_map), Some(base_dir)) =
            (&self.skill_injector, &self.skills_map, &self.base_dir)
        else {
            // Not configured - return base prompt as-is
            return Ok(base_prompt.unwrap_or_default().to_string());
        };

        // Check if agent has skills defined
        let Some(skill_names) = &self.params.skills else {
            // No skills on this agent - return base prompt
            return Ok(base_prompt.unwrap_or_default().to_string());
        };

        if skill_names.is_empty() {
            return Ok(base_prompt.unwrap_or_default().to_string());
        }

        // Convert Vec<String> to &[&str] for the inject() API
        let skill_refs: Vec<&str> = skill_names.iter().map(|s| s.as_str()).collect();

        // Inject skills into the prompt
        injector
            .inject(base_prompt, &skill_refs, skills_map, base_dir)
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

    /// Build NikaMcpTool instances from MCP clients
    fn build_tools(
        mcp_names: &[String],
        mcp_clients: &FxHashMap<String, Arc<McpClient>>,
    ) -> Result<Vec<Arc<dyn rig::tool::ToolDyn>>, NikaError> {
        let mut tools: Vec<Arc<dyn rig::tool::ToolDyn>> = Vec::new();

        for mcp_name in mcp_names {
            let client = mcp_clients
                .get(mcp_name)
                .ok_or_else(|| NikaError::McpNotConnected {
                    name: mcp_name.clone(),
                })?;

            // Get tool definitions from MCP client
            // For now, we'll get mock tools if client is in mock mode
            let tool_defs = client.get_tool_definitions();

            for def in tool_defs {
                let tool = NikaMcpTool::with_client(
                    NikaMcpToolDef {
                        name: def.name.clone(),
                        description: def.description.clone().unwrap_or_default(),
                        input_schema: def
                            .input_schema
                            .clone()
                            .unwrap_or_else(|| serde_json::json!({"type": "object"})),
                    },
                    client.clone(),
                );
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
