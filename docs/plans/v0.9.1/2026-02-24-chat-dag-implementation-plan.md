# Chat as Workflow DAG — Implementation Plan

**Date:** 2026-02-24
**Version:** v0.9.0 Target
**Design Document:** [2026-02-24-chat-as-workflow-dag.md](./2026-02-24-chat-as-workflow-dag.md)

---

## Overview

Implementation plan for unifying Chat TUI with Workflow DAG system. Each chat message becomes a Task with full DataStore, EventLog, and binding support.

---

## Phase 1: Infrastructure (No UI Changes)

**Goal:** Wire DataStore and EventLog into chat execution without changing UX.

### Task 1.1: Create ChatWorkflow Struct

**File:** `src/tui/chat_workflow.rs` (NEW)

```rust
use crate::ast::{Workflow, Task, TaskAction};
use crate::store::DataStore;
use crate::event::EventLog;
use crate::dag::FlowGraph;

pub struct ChatWorkflow {
    /// Incremental workflow being built
    pub workflow: Workflow,
    /// DAG representation
    pub dag: FlowGraph,
    /// Result storage
    pub store: DataStore,
    /// Event log for observability
    pub log: EventLog,
    /// Message counter for ID generation
    pub message_counter: u32,
}

impl ChatWorkflow {
    pub fn new(session_id: &str) -> Self {
        Self {
            workflow: Workflow {
                schema: "nika/workflow@0.5".into(),
                workflow: format!("chat-session-{}", session_id),
                description: Some("Interactive chat session".into()),
                tasks: Vec::new(),
                flows: Vec::new(),
                mcp: None,
            },
            dag: FlowGraph::new(),
            store: DataStore::new(),
            log: EventLog::new(),
            message_counter: 0,
        }
    }

    /// Generate next message ID (msg-001, msg-002, ...)
    pub fn next_message_id(&mut self) -> String {
        self.message_counter += 1;
        format!("msg-{:03}", self.message_counter)
    }

    /// Add a task to the workflow and DAG
    pub fn add_task(&mut self, task: Task) {
        self.dag.add_node(&task);
        self.workflow.tasks.push(task);
    }

    /// Add a flow (edge) between tasks
    pub fn add_flow(&mut self, source: &str, target: &str) {
        use crate::ast::Flow;
        self.workflow.flows.push(Flow {
            source: source.into(),
            target: target.into(),
        });
        self.dag.add_edge(source, target);
    }
}
```

**Tests:** `src/tui/chat_workflow.rs` (10+ unit tests)
- `test_new_creates_empty_workflow`
- `test_next_message_id_increments`
- `test_add_task_updates_dag`
- `test_add_flow_creates_edge`

### Task 1.2: Create ChatTask Builder

**File:** `src/tui/chat_task.rs` (NEW)

```rust
use crate::ast::{Task, TaskAction, InferParams, WiringSpec};

pub struct ChatTaskBuilder {
    id: String,
    action: TaskAction,
    use_wiring: Option<WiringSpec>,
    depends_on: Vec<String>,
}

impl ChatTaskBuilder {
    pub fn new(id: String, action: TaskAction) -> Self {
        Self {
            id,
            action,
            use_wiring: None,
            depends_on: Vec::new(),
        }
    }

    /// Create infer task from chat message
    pub fn from_message(id: String, prompt: &str) -> Self {
        Self::new(id, TaskAction::Infer(InferParams {
            prompt: prompt.to_string(),
            model: None, // Use default
            ..Default::default()
        }))
    }

    pub fn depends_on(mut self, task_id: &str) -> Self {
        self.depends_on.push(task_id.to_string());
        self
    }

    pub fn with_wiring(mut self, wiring: WiringSpec) -> Self {
        self.use_wiring = Some(wiring);
        self
    }

    pub fn build(self) -> Task {
        Task {
            id: self.id,
            action: self.action,
            use_wiring: self.use_wiring,
            output: None,
            condition: None,
            for_each: None,
            decompose: None,
        }
    }
}
```

### Task 1.3: Wire DataStore into ChatAgent

**File:** `src/tui/chat_agent.rs` (MODIFY)

```rust
// Before (current)
pub struct ChatAgent {
    provider: RigProvider,
    history: Vec<ChatMessage>,
    // ...
}

// After (add ChatWorkflow)
pub struct ChatAgent {
    provider: RigProvider,
    history: Vec<ChatMessage>,
    workflow: ChatWorkflow,  // NEW
    // ...
}

impl ChatAgent {
    pub fn new(provider: RigProvider) -> Self {
        Self {
            provider,
            history: Vec::new(),
            workflow: ChatWorkflow::new(&Uuid::new_v4().to_string()),
        }
    }

    pub async fn infer(&mut self, prompt: &str) -> Result<String, NikaError> {
        // 1. Create task
        let task_id = self.workflow.next_message_id();
        let task = ChatTaskBuilder::from_message(task_id.clone(), prompt)
            .build();

        // 2. Add to workflow
        self.workflow.add_task(task.clone());

        // 3. Emit TaskStarted event
        self.workflow.log.emit(EventKind::TaskStarted {
            task_id: task_id.clone().into(),
            action_type: "infer".into(),
        });

        // 4. Execute via provider (existing code)
        let result = self.provider.infer(prompt, None).await?;

        // 5. Store result in DataStore
        self.workflow.store.insert(&task_id, serde_json::json!({
            "output": result.clone(),
            "prompt": prompt,
        }));

        // 6. Emit TaskCompleted event
        self.workflow.log.emit(EventKind::TaskCompleted {
            task_id: task_id.into(),
            duration_ms: 0, // TODO: track actual duration
        });

        // 7. Update history (existing code)
        self.history.push(ChatMessage::user(prompt));
        self.history.push(ChatMessage::assistant(&result));

        Ok(result)
    }
}
```

### Task 1.4: Export EventLog

**File:** `src/tui/chat_agent.rs` (MODIFY)

```rust
impl ChatAgent {
    /// Export session as NDJSON trace
    pub fn export_trace(&self, path: &Path) -> Result<(), NikaError> {
        use crate::event::TraceWriter;
        let writer = TraceWriter::new(path)?;
        for event in self.workflow.log.events() {
            writer.write_event(event)?;
        }
        Ok(())
    }

    /// Get all events (for DAG panel)
    pub fn events(&self) -> &[Event] {
        self.workflow.log.events()
    }
}
```

### Verification Phase 1

```bash
# 1. Run tests
cargo test chat_workflow
cargo test chat_task

# 2. Verify DataStore integration
cargo run -- chat
# Send a message, then check:
# - EventLog has TaskStarted + TaskCompleted events
# - DataStore has message result

# 3. Export trace
# After chat session, verify .nika/traces/ has NDJSON file
```

---

## Phase 2: Binding System

**Goal:** Implement @mention parsing and binding resolution.

### Task 2.1: Mention Parser

**File:** `src/tui/mention_parser.rs` (NEW)

```rust
use regex::Regex;
use lazy_static::lazy_static;

lazy_static! {
    /// Match @1, @2, @last, @prev, @all, @msg-001
    static ref MENTION_RE: Regex = Regex::new(
        r"@((\d+)|last|prev|all|msg-\d{3})"
    ).unwrap();
}

#[derive(Debug, Clone, PartialEq)]
pub enum Mention {
    /// @1, @2, etc. (1-indexed)
    Number(u32),
    /// @last - last message
    Last,
    /// @prev - previous message (same as @last in sequential)
    Prev,
    /// @all - all previous messages
    All,
    /// @msg-001 - explicit ID
    Explicit(String),
}

impl Mention {
    /// Resolve to actual task ID given message count
    pub fn resolve(&self, message_count: u32) -> Vec<String> {
        match self {
            Mention::Number(n) => vec![format!("msg-{:03}", n)],
            Mention::Last => vec![format!("msg-{:03}", message_count)],
            Mention::Prev => vec![format!("msg-{:03}", message_count)],
            Mention::All => (1..=message_count)
                .map(|n| format!("msg-{:03}", n))
                .collect(),
            Mention::Explicit(id) => vec![id.clone()],
        }
    }
}

/// Parse all @mentions from a message
pub fn parse_mentions(text: &str) -> Vec<Mention> {
    MENTION_RE.captures_iter(text)
        .filter_map(|cap| {
            let m = cap.get(1)?.as_str();
            Some(match m {
                "last" => Mention::Last,
                "prev" => Mention::Prev,
                "all" => Mention::All,
                s if s.starts_with("msg-") => Mention::Explicit(s.into()),
                s => Mention::Number(s.parse().ok()?),
            })
        })
        .collect()
}

/// Check if message starts with // (parallel prefix)
pub fn is_parallel(text: &str) -> bool {
    text.trim_start().starts_with("//")
}

/// Strip prefix commands from message
pub fn strip_prefix(text: &str) -> &str {
    let text = text.trim_start();
    if text.starts_with("//") {
        text[2..].trim_start()
    } else if text.starts_with("/infer") {
        text[6..].trim_start()
    } else if text.starts_with("/exec") {
        text[5..].trim_start()
    } else if text.starts_with("/fetch") {
        text[6..].trim_start()
    } else if text.starts_with("/invoke") {
        text[7..].trim_start()
    } else if text.starts_with("/agent") {
        text[6..].trim_start()
    } else {
        text
    }
}
```

**Tests:** `src/tui/mention_parser.rs` (15+ unit tests)
- `test_parse_numeric_mention`
- `test_parse_last_mention`
- `test_parse_multiple_mentions`
- `test_resolve_to_task_ids`
- `test_is_parallel_detection`

### Task 2.2: MentionToBinding Converter

**File:** `src/tui/mention_binding.rs` (NEW)

```rust
use crate::ast::WiringSpec;
use crate::binding::UseEntry;
use super::mention_parser::{Mention, parse_mentions, is_parallel};

/// Convert @mentions to WiringSpec
pub fn mentions_to_wiring(
    text: &str,
    message_count: u32,
    prev_task_id: Option<&str>,
) -> WiringSpec {
    let mentions = parse_mentions(text);

    // If message is parallel (//), no dependencies
    if is_parallel(text) {
        return WiringSpec::default();
    }

    // If explicit @mentions, use those
    if !mentions.is_empty() {
        let entries: Vec<UseEntry> = mentions
            .iter()
            .enumerate()
            .flat_map(|(i, m)| {
                m.resolve(message_count)
                    .into_iter()
                    .map(move |id| UseEntry {
                        alias: format!("m{}", i + 1),
                        path: format!("{}.output", id).parse().unwrap(),
                        lazy: false,
                        default: None,
                    })
            })
            .collect();

        return WiringSpec { entries };
    }

    // Default: depend on previous message (sequential)
    if let Some(prev_id) = prev_task_id {
        WiringSpec {
            entries: vec![UseEntry {
                alias: "prev".into(),
                path: format!("{}.output", prev_id).parse().unwrap(),
                lazy: false,
                default: None,
            }],
        }
    } else {
        WiringSpec::default()
    }
}
```

### Task 2.3: Integrate Bindings into ChatAgent

**File:** `src/tui/chat_agent.rs` (MODIFY)

```rust
impl ChatAgent {
    pub async fn infer(&mut self, prompt: &str) -> Result<String, NikaError> {
        // 1. Parse @mentions and determine dependencies
        let prev_task_id = self.workflow.workflow.tasks.last()
            .map(|t| t.id.as_str());
        let wiring = mentions_to_wiring(
            prompt,
            self.workflow.message_counter,
            prev_task_id,
        );

        // 2. Create task with wiring
        let task_id = self.workflow.next_message_id();
        let task = ChatTaskBuilder::from_message(task_id.clone(), prompt)
            .with_wiring(wiring.clone())
            .build();

        // 3. Add flows for dependencies
        for entry in &wiring.entries {
            let source_id = entry.path.task_id();
            self.workflow.add_flow(&source_id, &task_id);
        }

        // 4. Add to workflow (existing code)
        self.workflow.add_task(task);

        // ... rest of execution
    }
}
```

### Verification Phase 2

```bash
# 1. Run tests
cargo test mention_parser
cargo test mention_binding

# 2. Integration test
cargo run -- chat
> "Hello"                    # msg-001
> "Expand on that"           # msg-002 depends on msg-001
> // "Independent task"      # msg-003 no dependencies
> "Combine @1 and @3"        # msg-004 depends on msg-001 and msg-003
```

---

## Phase 3: Builtin Tools

**Goal:** Implement TIER 1 `nika:*` builtin tools for chat and workflow integration.

**Reference:** [2026-02-24-builtin-tools-spec.md](./2026-02-24-builtin-tools-spec.md)

### Task 3.1: Create BuiltinTool Trait

**File:** `src/runtime/builtin/mod.rs` (NEW)

```rust
//! Builtin tools for Nika workflows
//!
//! These tools use the `nika:*` prefix and have access to internal state
//! (EventLog, DataStore) unlike MCP tools which are stateless.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use rig::completion::ToolDefinition;
use rig::tool::{ToolDyn, ToolError};
use serde_json::Value;

use crate::event::EventLog;
use crate::store::DataStore;

pub mod prompt;
pub mod run;
pub mod sleep;
pub mod log;
pub mod assert;
pub mod emit;
pub mod router;

/// Extended trait for builtin tools with internal state access
pub trait BuiltinTool: ToolDyn + Send + Sync {
    /// Get reference to event log
    fn event_log(&self) -> &EventLog;

    /// Get reference to data store
    fn data_store(&self) -> Option<&DataStore> {
        None
    }

    /// Tool category for routing
    fn category(&self) -> BuiltinCategory {
        BuiltinCategory::ControlFlow
    }

    /// Whether this tool can pause execution (e.g., nika:prompt)
    fn can_pause(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinCategory {
    /// Control flow tools (sleep, assert)
    ControlFlow,
    /// Human-in-the-loop tools (prompt)
    HumanInLoop,
    /// Workflow composition (run)
    Composition,
    /// Observability tools (log, emit)
    Observability,
}

/// Helper macro for implementing ToolDyn on builtin tools
#[macro_export]
macro_rules! impl_tool_dyn {
    ($tool:ty, $name:expr) => {
        impl rig::tool::ToolDyn for $tool {
            fn name(&self) -> &str {
                $name
            }

            fn definition(&self) -> rig::completion::ToolDefinition {
                self.tool_definition()
            }

            fn call(
                &self,
                args: String,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<String, rig::tool::ToolError>> + Send + '_>,
            > {
                Box::pin(self.execute(args))
            }
        }
    };
}
```

### Task 3.2: Create BuiltinToolRouter

**File:** `src/runtime/builtin/router.rs` (NEW)

```rust
use std::sync::Arc;
use rustc_hash::FxHashMap;
use rig::completion::ToolDefinition;

use crate::error::NikaError;
use crate::event::{EventKind, EventLog};
use crate::store::DataStore;

use super::{BuiltinTool, BuiltinCategory};
use super::prompt::PromptTool;
use super::run::RunTool;
use super::sleep::SleepTool;
use super::log::LogTool;
use super::assert::AssertTool;
use super::emit::EmitTool;

/// Routes `nika:*` tool calls to builtin implementations
pub struct BuiltinToolRouter {
    tools: FxHashMap<&'static str, Arc<dyn BuiltinTool>>,
    event_log: EventLog,
}

impl BuiltinToolRouter {
    /// Create router with all TIER 1 tools registered
    pub fn new(event_log: EventLog, data_store: DataStore) -> Self {
        let mut tools: FxHashMap<&'static str, Arc<dyn BuiltinTool>> = FxHashMap::default();

        // Register TIER 1 tools
        tools.insert("prompt", Arc::new(PromptTool::new(event_log.clone())));
        tools.insert("run", Arc::new(RunTool::new(event_log.clone(), data_store.clone())));
        tools.insert("sleep", Arc::new(SleepTool::new(event_log.clone())));
        tools.insert("log", Arc::new(LogTool::new(event_log.clone())));
        tools.insert("assert", Arc::new(AssertTool::new(event_log.clone(), data_store)));
        tools.insert("emit", Arc::new(EmitTool::new(event_log.clone())));

        Self { tools, event_log }
    }

    /// Check if tool name has nika: prefix
    pub fn is_builtin(tool_name: &str) -> bool {
        tool_name.starts_with("nika:")
    }

    /// Extract tool name after nika: prefix
    fn extract_name(tool_name: &str) -> Option<&str> {
        tool_name.strip_prefix("nika:")
    }

    /// Dispatch to builtin tool
    pub async fn dispatch(&self, tool_name: &str, args: String) -> Result<String, NikaError> {
        let name = Self::extract_name(tool_name)
            .ok_or_else(|| NikaError::BuiltinToolError {
                tool: tool_name.to_string(),
                reason: "Missing nika: prefix".into(),
                code: 200,
            })?;

        let tool = self.tools.get(name)
            .ok_or_else(|| NikaError::BuiltinToolError {
                tool: tool_name.to_string(),
                reason: format!("Unknown builtin tool: {}", name),
                code: 201,
            })?;

        // Emit BuiltinInvoke event
        self.event_log.emit(EventKind::BuiltinInvoke {
            tool: Arc::from(tool_name),
            args: args.clone(),
        });

        // Call tool via ToolDyn interface
        use rig::tool::ToolDyn;
        let result = tool.call(args).await
            .map_err(|e| NikaError::BuiltinToolError {
                tool: tool_name.to_string(),
                reason: e.to_string(),
                code: 203,
            })?;

        // Emit BuiltinResponse event
        self.event_log.emit(EventKind::BuiltinResponse {
            tool: Arc::from(tool_name),
            result: result.clone(),
        });

        Ok(result)
    }

    /// Get all tool definitions for agent discovery
    pub fn definitions(&self) -> Vec<ToolDefinition> {
        use rig::tool::ToolDyn;
        self.tools.values()
            .map(|t| t.definition())
            .collect()
    }

    /// Get tool by name (without nika: prefix)
    pub fn get(&self, name: &str) -> Option<&Arc<dyn BuiltinTool>> {
        self.tools.get(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_builtin_with_prefix() {
        assert!(BuiltinToolRouter::is_builtin("nika:prompt"));
        assert!(BuiltinToolRouter::is_builtin("nika:sleep"));
    }

    #[test]
    fn test_is_builtin_without_prefix() {
        assert!(!BuiltinToolRouter::is_builtin("novanet:describe"));
        assert!(!BuiltinToolRouter::is_builtin("prompt"));
    }

    #[test]
    fn test_extract_name() {
        assert_eq!(BuiltinToolRouter::extract_name("nika:prompt"), Some("prompt"));
        assert_eq!(BuiltinToolRouter::extract_name("nika:sleep"), Some("sleep"));
        assert_eq!(BuiltinToolRouter::extract_name("novanet:describe"), None);
    }

    #[tokio::test]
    async fn test_dispatch_unknown_tool() {
        let router = BuiltinToolRouter::new(EventLog::new(), DataStore::new());
        let result = router.dispatch("nika:unknown", "{}".into()).await;
        assert!(result.is_err());
    }
}
```

### Task 3.3: Implement nika:sleep Tool

**File:** `src/runtime/builtin/sleep.rs` (NEW)

```rust
use std::time::Duration;

use rig::completion::ToolDefinition;
use rig::tool::ToolError;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::event::{EventKind, EventLog};
use crate::impl_tool_dyn;
use super::{BuiltinTool, BuiltinCategory};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SleepParams {
    /// Duration string: "5s", "1m", "500ms", "1h30m"
    pub duration: String,
}

pub struct SleepTool {
    event_log: EventLog,
}

impl SleepTool {
    pub fn new(event_log: EventLog) -> Self {
        Self { event_log }
    }

    pub fn tool_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "nika:sleep".to_string(),
            description: "Pause execution for a specified duration.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "duration": {
                        "type": "string",
                        "description": "Duration (e.g., '5s', '1m', '500ms', '1h30m')"
                    }
                },
                "required": ["duration"],
                "additionalProperties": false
            }),
        }
    }

    pub async fn execute(&self, args: String) -> Result<String, ToolError> {
        let params: SleepParams = serde_json::from_str(&args)
            .map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        let duration = parse_duration(&params.duration)
            .map_err(|e| ToolError::InvalidArguments(e))?;

        // Cap at 1 hour to prevent runaway
        if duration > Duration::from_secs(3600) {
            return Err(ToolError::InvalidArguments(
                "Duration cannot exceed 1 hour".into()
            ));
        }

        self.event_log.emit(EventKind::BuiltinSleep {
            duration_ms: duration.as_millis() as u64,
        });

        tokio::time::sleep(duration).await;

        Ok(json!({ "slept_ms": duration.as_millis() }).to_string())
    }
}

impl BuiltinTool for SleepTool {
    fn event_log(&self) -> &EventLog {
        &self.event_log
    }

    fn category(&self) -> BuiltinCategory {
        BuiltinCategory::ControlFlow
    }
}

impl_tool_dyn!(SleepTool, "nika:sleep");

/// Parse duration string like "5s", "1m", "500ms", "1h30m"
fn parse_duration(s: &str) -> Result<Duration, String> {
    humantime::parse_duration(s)
        .map_err(|e| format!("Invalid duration '{}': {}", s, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration_seconds() {
        assert_eq!(parse_duration("5s").unwrap(), Duration::from_secs(5));
        assert_eq!(parse_duration("30s").unwrap(), Duration::from_secs(30));
    }

    #[test]
    fn test_parse_duration_milliseconds() {
        assert_eq!(parse_duration("500ms").unwrap(), Duration::from_millis(500));
        assert_eq!(parse_duration("100ms").unwrap(), Duration::from_millis(100));
    }

    #[test]
    fn test_parse_duration_minutes() {
        assert_eq!(parse_duration("1m").unwrap(), Duration::from_secs(60));
        assert_eq!(parse_duration("5m").unwrap(), Duration::from_secs(300));
    }

    #[test]
    fn test_parse_duration_combined() {
        assert_eq!(parse_duration("1m30s").unwrap(), Duration::from_secs(90));
    }

    #[test]
    fn test_parse_duration_invalid() {
        assert!(parse_duration("invalid").is_err());
        assert!(parse_duration("5").is_err());
    }

    #[tokio::test]
    async fn test_sleep_executes() {
        let tool = SleepTool::new(EventLog::new());
        let start = std::time::Instant::now();

        let result = tool.execute(r#"{"duration": "100ms"}"#.into()).await;

        assert!(result.is_ok());
        assert!(start.elapsed() >= Duration::from_millis(90)); // Allow some slack
    }

    #[tokio::test]
    async fn test_sleep_rejects_long_duration() {
        let tool = SleepTool::new(EventLog::new());

        let result = tool.execute(r#"{"duration": "2h"}"#.into()).await;

        assert!(result.is_err());
    }
}
```

### Task 3.4: Implement nika:log Tool

**File:** `src/runtime/builtin/log.rs` (NEW)

```rust
use std::sync::Arc;

use rig::completion::ToolDefinition;
use rig::tool::ToolError;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::event::{EventKind, EventLog};
use crate::impl_tool_dyn;
use super::{BuiltinTool, BuiltinCategory};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogParams {
    /// Log level
    pub level: LogLevel,
    /// Message to log
    pub message: String,
    /// Optional structured data
    #[serde(default)]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

pub struct LogTool {
    event_log: EventLog,
}

impl LogTool {
    pub fn new(event_log: EventLog) -> Self {
        Self { event_log }
    }

    pub fn tool_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "nika:log".to_string(),
            description: "Emit a structured log message.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "level": {
                        "type": "string",
                        "enum": ["debug", "info", "warn", "error"],
                        "description": "Log level"
                    },
                    "message": {
                        "type": "string",
                        "description": "Log message"
                    },
                    "data": {
                        "type": "object",
                        "description": "Optional structured data"
                    }
                },
                "required": ["level", "message"],
                "additionalProperties": false
            }),
        }
    }

    pub async fn execute(&self, args: String) -> Result<String, ToolError> {
        let params: LogParams = serde_json::from_str(&args)
            .map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        // Emit to EventLog
        self.event_log.emit(EventKind::BuiltinLog {
            level: params.level,
            message: Arc::from(params.message.as_str()),
            data: params.data.clone(),
        });

        // Also emit via tracing
        match params.level {
            LogLevel::Debug => tracing::debug!(message = %params.message, data = ?params.data, "nika:log"),
            LogLevel::Info => tracing::info!(message = %params.message, data = ?params.data, "nika:log"),
            LogLevel::Warn => tracing::warn!(message = %params.message, data = ?params.data, "nika:log"),
            LogLevel::Error => tracing::error!(message = %params.message, data = ?params.data, "nika:log"),
        }

        Ok(json!({ "logged": true }).to_string())
    }
}

impl BuiltinTool for LogTool {
    fn event_log(&self) -> &EventLog {
        &self.event_log
    }

    fn category(&self) -> BuiltinCategory {
        BuiltinCategory::Observability
    }
}

impl_tool_dyn!(LogTool, "nika:log");

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_log_info() {
        let event_log = EventLog::new();
        let tool = LogTool::new(event_log.clone());

        let result = tool.execute(r#"{"level": "info", "message": "Test message"}"#.into()).await;

        assert!(result.is_ok());
        assert!(event_log.events().iter().any(|e| matches!(&e.kind, EventKind::BuiltinLog { level, .. } if *level == LogLevel::Info)));
    }

    #[tokio::test]
    async fn test_log_with_data() {
        let tool = LogTool::new(EventLog::new());

        let result = tool.execute(r#"{"level": "debug", "message": "Test", "data": {"key": "value"}}"#.into()).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_log_invalid_level() {
        let tool = LogTool::new(EventLog::new());

        let result = tool.execute(r#"{"level": "invalid", "message": "Test"}"#.into()).await;

        assert!(result.is_err());
    }
}
```

### Task 3.5: Implement nika:assert Tool

**File:** `src/runtime/builtin/assert.rs` (NEW)

```rust
use std::sync::Arc;

use rig::completion::ToolDefinition;
use rig::tool::ToolError;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::event::{EventKind, EventLog};
use crate::store::DataStore;
use crate::impl_tool_dyn;
use super::{BuiltinTool, BuiltinCategory};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssertParams {
    /// Condition expression (must evaluate to truthy)
    pub condition: String,
    /// Optional custom error message
    #[serde(default)]
    pub message: Option<String>,
}

pub struct AssertTool {
    event_log: EventLog,
    data_store: DataStore,
}

impl AssertTool {
    pub fn new(event_log: EventLog, data_store: DataStore) -> Self {
        Self { event_log, data_store }
    }

    pub fn tool_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "nika:assert".to_string(),
            description: "Validate a condition. Fails workflow if false.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "condition": {
                        "type": "string",
                        "description": "Condition to evaluate (e.g., '{{use.count}} > 0')"
                    },
                    "message": {
                        "type": "string",
                        "description": "Custom error message on failure"
                    }
                },
                "required": ["condition"],
                "additionalProperties": false
            }),
        }
    }

    pub async fn execute(&self, args: String) -> Result<String, ToolError> {
        let params: AssertParams = serde_json::from_str(&args)
            .map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        // Resolve bindings in condition
        let resolved = self.data_store.resolve_template(&params.condition)
            .unwrap_or_else(|_| params.condition.clone());

        // Evaluate condition
        let result = evaluate_condition(&resolved)
            .map_err(|e| ToolError::ExecutionError(e))?;

        if result {
            self.event_log.emit(EventKind::BuiltinAssertPassed {
                condition: Arc::from(params.condition.as_str()),
            });
            Ok(json!({ "passed": true }).to_string())
        } else {
            let message = params.message.unwrap_or_else(|| {
                format!("Assertion failed: {}", params.condition)
            });

            self.event_log.emit(EventKind::BuiltinAssertFailed {
                condition: Arc::from(params.condition.as_str()),
                message: Arc::from(message.as_str()),
            });

            Err(ToolError::ExecutionError(message))
        }
    }
}

impl BuiltinTool for AssertTool {
    fn event_log(&self) -> &EventLog {
        &self.event_log
    }

    fn data_store(&self) -> Option<&DataStore> {
        Some(&self.data_store)
    }

    fn category(&self) -> BuiltinCategory {
        BuiltinCategory::ControlFlow
    }
}

impl_tool_dyn!(AssertTool, "nika:assert");

/// Evaluate simple condition expressions
/// Supports: ==, !=, >, <, >=, <=, &&, ||, !
fn evaluate_condition(expr: &str) -> Result<bool, String> {
    evalexpr::eval_boolean(expr)
        .map_err(|e| format!("Invalid condition '{}': {}", expr, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evaluate_equality() {
        assert!(evaluate_condition("1 == 1").unwrap());
        assert!(!evaluate_condition("1 == 2").unwrap());
    }

    #[test]
    fn test_evaluate_comparison() {
        assert!(evaluate_condition("5 > 3").unwrap());
        assert!(!evaluate_condition("3 > 5").unwrap());
        assert!(evaluate_condition("5 >= 5").unwrap());
    }

    #[test]
    fn test_evaluate_boolean() {
        assert!(evaluate_condition("true && true").unwrap());
        assert!(!evaluate_condition("true && false").unwrap());
        assert!(evaluate_condition("true || false").unwrap());
    }

    #[test]
    fn test_evaluate_string() {
        assert!(evaluate_condition(r#""hello" == "hello""#).unwrap());
    }

    #[tokio::test]
    async fn test_assert_passes() {
        let tool = AssertTool::new(EventLog::new(), DataStore::new());

        let result = tool.execute(r#"{"condition": "1 == 1"}"#.into()).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_assert_fails() {
        let tool = AssertTool::new(EventLog::new(), DataStore::new());

        let result = tool.execute(r#"{"condition": "1 == 2"}"#.into()).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_assert_custom_message() {
        let tool = AssertTool::new(EventLog::new(), DataStore::new());

        let result = tool.execute(r#"{"condition": "false", "message": "Custom error"}"#.into()).await;

        match result {
            Err(ToolError::ExecutionError(msg)) => assert_eq!(msg, "Custom error"),
            _ => panic!("Expected execution error"),
        }
    }
}
```

### Task 3.6: Implement nika:emit Tool

**File:** `src/runtime/builtin/emit.rs` (NEW)

```rust
use std::sync::Arc;

use rig::completion::ToolDefinition;
use rig::tool::ToolError;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::event::{EventKind, EventLog};
use crate::impl_tool_dyn;
use super::{BuiltinTool, BuiltinCategory};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmitParams {
    /// Event name
    pub event: String,
    /// Event data
    #[serde(default)]
    pub data: Value,
}

pub struct EmitTool {
    event_log: EventLog,
}

impl EmitTool {
    pub fn new(event_log: EventLog) -> Self {
        Self { event_log }
    }

    pub fn tool_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "nika:emit".to_string(),
            description: "Emit a custom event for observability.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "event": {
                        "type": "string",
                        "description": "Event name (e.g., 'page_generated')"
                    },
                    "data": {
                        "type": "object",
                        "description": "Event payload"
                    }
                },
                "required": ["event"],
                "additionalProperties": false
            }),
        }
    }

    pub async fn execute(&self, args: String) -> Result<String, ToolError> {
        let params: EmitParams = serde_json::from_str(&args)
            .map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        self.event_log.emit(EventKind::CustomEvent {
            name: Arc::from(params.event.as_str()),
            data: params.data.clone(),
        });

        Ok(json!({ "emitted": params.event }).to_string())
    }
}

impl BuiltinTool for EmitTool {
    fn event_log(&self) -> &EventLog {
        &self.event_log
    }

    fn category(&self) -> BuiltinCategory {
        BuiltinCategory::Observability
    }
}

impl_tool_dyn!(EmitTool, "nika:emit");

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_emit_event() {
        let event_log = EventLog::new();
        let tool = EmitTool::new(event_log.clone());

        let result = tool.execute(r#"{"event": "test_event", "data": {"key": "value"}}"#.into()).await;

        assert!(result.is_ok());
        assert!(event_log.events().iter().any(|e| matches!(&e.kind, EventKind::CustomEvent { name, .. } if name.as_ref() == "test_event")));
    }

    #[tokio::test]
    async fn test_emit_without_data() {
        let tool = EmitTool::new(EventLog::new());

        let result = tool.execute(r#"{"event": "simple_event"}"#.into()).await;

        assert!(result.is_ok());
    }
}
```

### Task 3.7: Wire Builtin Tools into Executor

**File:** `src/runtime/executor.rs` (MODIFY)

```rust
use crate::runtime::builtin::router::BuiltinToolRouter;

pub struct Executor {
    // Existing fields...
    builtin_router: BuiltinToolRouter,
}

impl Executor {
    pub fn new(/* ... */) -> Self {
        Self {
            // Existing initialization...
            builtin_router: BuiltinToolRouter::new(event_log.clone(), data_store.clone()),
        }
    }

    async fn execute_invoke(&mut self, task: &Task) -> Result<TaskResult, NikaError> {
        let invoke = task.action.as_invoke().unwrap();

        // Route based on prefix
        if BuiltinToolRouter::is_builtin(&invoke.tool) {
            // Builtin tool
            let params_json = serde_json::to_string(&invoke.params)
                .map_err(|e| NikaError::SerializationError(e.to_string()))?;

            let result = self.builtin_router.dispatch(&invoke.tool, params_json).await?;

            Ok(TaskResult {
                task_id: task.id.clone(),
                output: serde_json::from_str(&result).unwrap_or(serde_json::Value::String(result)),
                duration_ms: 0, // TODO: track duration
            })
        } else {
            // MCP tool (existing logic)
            self.execute_mcp_invoke(task).await
        }
    }
}
```

### Task 3.8: Add New EventKind Variants

**File:** `src/event/log.rs` (MODIFY)

```rust
use crate::runtime::builtin::log::LogLevel;

pub enum EventKind {
    // Existing variants...

    // NEW: Builtin tool events
    BuiltinInvoke {
        tool: Arc<str>,
        args: String,
    },
    BuiltinResponse {
        tool: Arc<str>,
        result: String,
    },
    BuiltinSleep {
        duration_ms: u64,
    },
    BuiltinLog {
        level: LogLevel,
        message: Arc<str>,
        data: Option<serde_json::Value>,
    },
    BuiltinAssertPassed {
        condition: Arc<str>,
    },
    BuiltinAssertFailed {
        condition: Arc<str>,
        message: Arc<str>,
    },
    CustomEvent {
        name: Arc<str>,
        data: serde_json::Value,
    },
}
```

### Verification Phase 3

```bash
# 1. Run tests
cargo test builtin::
cargo test router

# 2. Integration test - nika:sleep
cargo run -- run examples/test-sleep.nika.yaml

# 3. Integration test - nika:log
cargo run -- run examples/test-log.nika.yaml

# 4. Integration test - nika:assert
cargo run -- run examples/test-assert.nika.yaml

# 5. Verify events in trace
nika trace list
nika trace show <id>
# Should see BuiltinInvoke, BuiltinResponse, CustomEvent
```

### Test Files for Phase 3

**File:** `examples/test-sleep.nika.yaml`

```yaml
schema: nika/workflow@0.6
workflow: test-sleep

tasks:
  - id: log-start
    invoke:
      tool: nika:log
      params:
        level: info
        message: "Starting sleep test"

  - id: sleep-100ms
    invoke:
      tool: nika:sleep
      params:
        duration: "100ms"

  - id: log-done
    invoke:
      tool: nika:log
      params:
        level: info
        message: "Sleep completed"

flows:
  - log-start -> sleep-100ms
  - sleep-100ms -> log-done
```

**File:** `examples/test-assert.nika.yaml`

```yaml
schema: nika/workflow@0.6
workflow: test-assert

tasks:
  - id: assert-true
    invoke:
      tool: nika:assert
      params:
        condition: "1 == 1"

  - id: assert-math
    invoke:
      tool: nika:assert
      params:
        condition: "5 > 3"
        message: "Math should work"

flows:
  - assert-true -> assert-math
```

---

## Phase 4: DAG Panel

**Goal:** Add live DAG visualization sidebar to chat view.

### Task 3.1: Create ChatDagPanel Widget

**File:** `src/tui/widgets/chat_dag_panel.rs` (NEW)

```rust
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Widget},
};
use crate::dag::FlowGraph;
use super::dag_node_box::{NodeBox, NodeBoxMode};

pub struct ChatDagPanel<'a> {
    /// DAG to render
    dag: &'a FlowGraph,
    /// Currently running task ID
    running_task: Option<&'a str>,
    /// Width mode
    expanded: bool,
}

impl<'a> ChatDagPanel<'a> {
    pub fn new(dag: &'a FlowGraph) -> Self {
        Self {
            dag,
            running_task: None,
            expanded: false,
        }
    }

    pub fn running(mut self, task_id: Option<&'a str>) -> Self {
        self.running_task = task_id;
        self
    }

    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = expanded;
        self
    }
}

impl Widget for ChatDagPanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Border
        let block = Block::default()
            .title(" DAG Live ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));
        let inner = block.inner(area);
        block.render(area, buf);

        // Render nodes vertically
        let node_height = if self.expanded { 7 } else { 3 };
        let mut y = inner.y;

        for (i, node_id) in self.dag.topological_order().iter().enumerate() {
            if y + node_height > inner.bottom() {
                break; // No more space
            }

            let node_area = Rect::new(inner.x, y, inner.width, node_height);
            let is_running = self.running_task == Some(node_id.as_str());

            // Render node box
            NodeBox::new(node_id)
                .mode(if self.expanded {
                    NodeBoxMode::Full
                } else {
                    NodeBoxMode::Expanded
                })
                .running(is_running)
                .render(node_area, buf);

            // Render edge to next node
            if i < self.dag.node_count() - 1 {
                y += node_height;
                if y < inner.bottom() {
                    let edge_char = if is_running { '▼' } else { '│' };
                    buf.set_string(
                        inner.x + inner.width / 2,
                        y,
                        edge_char.to_string(),
                        Style::default().fg(Color::DarkGray),
                    );
                    y += 1;
                }
            } else {
                y += node_height;
            }
        }

        // Footer: stats
        let stats = format!(
            " {} tasks {} layers ",
            self.dag.node_count(),
            self.dag.layer_count(),
        );
        buf.set_string(
            inner.right() - stats.len() as u16 - 1,
            area.bottom() - 1,
            stats,
            Style::default().fg(Color::DarkGray),
        );
    }
}
```

### Task 3.2: Add Sidebar Layout to Chat View

**File:** `src/tui/views/chat.rs` (MODIFY)

```rust
impl ChatView {
    fn render_inner(&self, area: Rect, buf: &mut Buffer) {
        // Split into chat (left) and DAG (right)
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Min(40),              // Chat (flexible)
                Constraint::Length(self.dag_width), // DAG panel (fixed)
            ])
            .split(area);

        // Render chat messages (existing code)
        self.render_chat(chunks[0], buf);

        // Render DAG panel (NEW)
        ChatDagPanel::new(&self.agent.workflow.dag)
            .running(self.running_task.as_deref())
            .expanded(self.dag_expanded)
            .render(chunks[1], buf);
    }
}
```

### Task 3.3: Wire Live Updates

**File:** `src/tui/views/chat.rs` (MODIFY)

```rust
impl ChatView {
    /// Called when a task starts
    fn on_task_started(&mut self, task_id: &str) {
        self.running_task = Some(task_id.to_string());
        // Trigger re-render
    }

    /// Called when a task completes
    fn on_task_completed(&mut self, task_id: &str) {
        self.running_task = None;
        // Trigger re-render
    }
}
```

### Task 3.4: Node Click → Scroll to Message

**File:** `src/tui/views/chat.rs` (MODIFY)

```rust
impl ChatView {
    fn handle_dag_click(&mut self, x: u16, y: u16) {
        // Determine which node was clicked
        if let Some(task_id) = self.get_node_at(x, y) {
            // Find message index
            if let Some(msg_idx) = self.get_message_index(&task_id) {
                self.scroll_to_message(msg_idx);
            }
        }
    }
}
```

### Verification Phase 3

```bash
# 1. Visual test
cargo run -- chat
# Verify sidebar appears on right
# Verify nodes appear as messages are sent
# Verify running spinner while task executes

# 2. Keyboard test
# Press Ctrl+D to toggle DAG width
# Press Ctrl+E to expand/collapse nodes
```

---

## Phase 4: Enhanced NodeBox

**Goal:** Remove Minimal mode, enhance Expanded to Full with more info.

### Task 4.1: Add Full Mode to NodeBox

**File:** `src/tui/widgets/dag_node_box.rs` (MODIFY)

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NodeBoxMode {
    /// Compact 3-line view (DEPRECATED - remove)
    // Minimal,
    /// Standard 5-line view
    Expanded,
    /// Full 7-line view with tokens, output, bindings
    Full,
}

pub struct NodeBox<'a> {
    task_id: &'a str,
    mode: NodeBoxMode,
    // NEW fields for Full mode
    tokens_in: Option<u32>,
    tokens_out: Option<u32>,
    output_preview: Option<&'a str>,
    bindings: Vec<&'a str>,
    duration_ms: Option<u64>,
}

impl<'a> NodeBox<'a> {
    pub fn tokens(mut self, input: u32, output: u32) -> Self {
        self.tokens_in = Some(input);
        self.tokens_out = Some(output);
        self
    }

    pub fn output(mut self, preview: &'a str) -> Self {
        self.output_preview = Some(preview);
        self
    }

    pub fn bindings(mut self, bindings: Vec<&'a str>) -> Self {
        self.bindings = bindings;
        self
    }

    pub fn duration(mut self, ms: u64) -> Self {
        self.duration_ms = Some(ms);
        self
    }
}

impl Widget for NodeBox<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        match self.mode {
            NodeBoxMode::Expanded => self.render_expanded(area, buf),
            NodeBoxMode::Full => self.render_full(area, buf),
        }
    }
}

impl NodeBox<'_> {
    fn render_full(&self, area: Rect, buf: &mut Buffer) {
        // Line 1: Icon + ID + Duration + Status
        // Line 2: ────────────────────────────
        // Line 3: Model + Tokens
        // Line 4: ────────────────────────────
        // Line 5: Prompt preview
        // Line 6: ────────────────────────────
        // Line 7: Output preview
        // Line 8: ────────────────────────────
        // Line 9: Bindings

        let lines = vec![
            self.format_header(),
            "─".repeat(area.width as usize - 2),
            self.format_model_tokens(),
            "─".repeat(area.width as usize - 2),
            self.format_prompt(),
            "─".repeat(area.width as usize - 2),
            self.format_output(),
            "─".repeat(area.width as usize - 2),
            self.format_bindings(),
        ];

        // Render with box borders
        // ...
    }

    fn format_header(&self) -> String {
        let icon = self.get_verb_icon();
        let duration = self.duration_ms
            .map(|ms| format!("{:.1}s", ms as f64 / 1000.0))
            .unwrap_or_default();
        let status = self.get_status_badge();
        format!("{} {}  {:>8}  {}", icon, self.task_id, duration, status)
    }

    fn format_model_tokens(&self) -> String {
        let model = "🧠 claude-sonnet-4";
        let tokens = match (self.tokens_in, self.tokens_out) {
            (Some(i), Some(o)) => format!("📊 {}→{}", format_tokens(i), format_tokens(o)),
            _ => String::new(),
        };
        format!("{}  {}", model, tokens)
    }

    fn format_output(&self) -> String {
        self.output_preview
            .map(|s| format!("📤 \"{}\"", truncate(s, 50)))
            .unwrap_or_default()
    }

    fn format_bindings(&self) -> String {
        if self.bindings.is_empty() {
            return String::new();
        }
        format!("🔗 use: {}", self.bindings.join(", "))
    }
}

fn format_tokens(n: u32) -> String {
    if n >= 1000 {
        format!("{:.1}K", n as f64 / 1000.0)
    } else {
        n.to_string()
    }
}
```

### Task 4.2: Remove Minimal Mode

**Files to modify:**
- `src/tui/widgets/dag_node_box.rs` - Remove Minimal variant
- `src/tui/widgets/dag_ascii.rs` - Update to use Expanded/Full only
- Any tests using NodeBoxMode::Minimal

### Verification Phase 4

```bash
# 1. Run tests
cargo test dag_node_box

# 2. Visual test
cargo run -- chat
# Send messages and verify:
# - Tokens show in Full mode
# - Output preview shows result
# - Bindings display @mentions
```

---

## Phase 5: Polish

**Goal:** Animations, shortcuts, persistence, export.

### Task 5.1: Animations

**File:** `src/tui/widgets/dag_node_box.rs` (MODIFY)

```rust
impl NodeBox<'_> {
    /// Get animated border style based on status
    fn get_border_style(&self, frame: u64) -> Style {
        match self.status {
            TaskStatus::Pending => Style::default().fg(Color::DarkGray),
            TaskStatus::Running => {
                // Pulse animation
                let intensity = ((frame % 20) as f64 / 20.0 * std::f64::consts::PI).sin();
                let color = if intensity > 0.5 { Color::Yellow } else { Color::LightYellow };
                Style::default().fg(color)
            }
            TaskStatus::Completed => Style::default().fg(Color::Green),
            TaskStatus::Failed => {
                // Shake animation (handled in render)
                Style::default().fg(Color::Red)
            }
        }
    }
}
```

### Task 5.2: Keyboard Shortcuts

**File:** `src/tui/views/chat.rs` (MODIFY)

```rust
impl ChatView {
    fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        match (key.modifiers, key.code) {
            // Existing shortcuts...

            // NEW: DAG panel shortcuts
            (KeyModifiers::CONTROL, KeyCode::Char('d')) => {
                self.dag_width = if self.dag_width == 24 { 40 } else { 24 };
                None
            }
            (KeyModifiers::CONTROL, KeyCode::Char('e')) => {
                self.dag_expanded = !self.dag_expanded;
                None
            }

            _ => None,
        }
    }
}
```

### Task 5.3: Session Persistence for DAG State

**File:** `src/tui/session.rs` (MODIFY)

```rust
#[derive(Serialize, Deserialize)]
pub struct ChatSession {
    // Existing fields...

    /// DAG state for restoration
    pub dag_state: Option<ChatDagState>,
}

#[derive(Serialize, Deserialize)]
pub struct ChatDagState {
    /// Task IDs in order
    pub tasks: Vec<String>,
    /// Edges (source, target)
    pub edges: Vec<(String, String)>,
    /// Results per task
    pub results: HashMap<String, serde_json::Value>,
}
```

### Task 5.4: Export to YAML

**File:** `src/tui/chat_agent.rs` (MODIFY)

```rust
impl ChatAgent {
    /// Export chat session as .nika.yaml workflow
    pub fn export_yaml(&self, path: &Path) -> Result<(), NikaError> {
        let yaml = serde_yaml::to_string(&self.workflow.workflow)?;
        std::fs::write(path, yaml)?;
        Ok(())
    }
}
```

### Verification Phase 5

```bash
# 1. Animation test
cargo run -- chat
# Verify spinning badge on running tasks
# Verify green glow on completion

# 2. Shortcut test
# Press Ctrl+D, verify width changes
# Press Ctrl+E, verify nodes expand/collapse

# 3. Persistence test
# Exit chat, restart, verify DAG restored

# 4. Export test
cargo run -- chat
> "Hello"
> "Continue"
# Export as YAML, verify valid .nika.yaml
```

---

## Test Coverage

| Module | Tests Required | Priority |
|--------|---------------|----------|
| `chat_workflow.rs` | 10 | P0 |
| `chat_task.rs` | 8 | P0 |
| `mention_parser.rs` | 15 | P0 |
| `mention_binding.rs` | 10 | P0 |
| `chat_dag_panel.rs` | 8 | P1 |
| `dag_node_box.rs` (Full mode) | 12 | P1 |
| Integration tests | 10 | P1 |

**Total new tests:** ~73

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| DataStore performance with many messages | Limit to 1000 messages per session |
| DAG panel too wide | Min width 24, max 60, responsive |
| Complex mention parsing | Extensive regex tests, fallback to sequential |
| Session persistence corruption | Atomic writes, backup on load |

---

## Success Metrics

1. **All tests pass:** 100% of new tests green
2. **No regressions:** Existing 1,902 tests pass
3. **Performance:** <50ms frame time with 100 messages
4. **Coverage:** 80%+ on new code
5. **User feedback:** Chat feels same, DAG adds value

---

## Timeline Summary

| Phase | Focus | Deliverables |
|-------|-------|--------------|
| Phase 1 | Infrastructure | ChatWorkflow, DataStore wiring, EventLog |
| Phase 2 | Bindings | @mention parser, MentionToBinding |
| Phase 3 | DAG Panel | ChatDagPanel widget, sidebar layout |
| Phase 4 | NodeBox | Full mode, remove Minimal |
| Phase 5 | Polish | Animations, shortcuts, persistence |

---

## References

- Design Document: [2026-02-24-chat-as-workflow-dag.md](./2026-02-24-chat-as-workflow-dag.md)
- ADR-001: 5 Semantic Verbs
- ADR-002: YAML-First Workflow Definition
- `src/tui/widgets/dag_node_box.rs` — Current NodeBox
- `src/tui/widgets/dag_ascii.rs` — Current DAG rendering
