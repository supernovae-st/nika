# Nika v0.9.1 — Builtin Tools Specification

**Date:** 2026-02-24
**Status:** Draft
**Authors:** Claude (Spec), Thibaut (Review)
**Depends On:** B1.1 StableGraph Migration

---

## Executive Summary

This document specifies the 6 TIER 1 builtin tools (`nika:*` prefix) for Nika v0.9.4.
Builtin tools extend workflow capabilities without adding new verbs, preserving ADR-001.

**Key Decisions:**
- Builtin tools implement `BuiltinTool` trait (extends `rig::tool::ToolDyn`)
- Routing: `nika:*` prefix → builtin, `server:tool` → MCP
- All tools have access to `EventLog` and `DataStore` (stateful)
- Error codes: NIKA-200 to NIKA-299 range

---

## Part 1: Architecture Overview

### 1.1 MCP vs Builtin Distinction

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  invoke: Routing Decision                                                        │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  invoke:                                                                         │
│    tool: "novanet:describe"    → MCP Client → novanet server → response         │
│    tool: "nika:prompt"         → BuiltinToolRouter → PromptTool → TUI/pause     │
│                                                                                  │
│  Prefix Detection:                                                               │
│  ├── "nika:" prefix → BuiltinToolRouter.dispatch()                              │
│  ├── "server:" prefix → McpClient.call_tool()                                   │
│  └── No prefix → Error (ambiguous, must specify)                                │
│                                                                                  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

### 1.2 Design Principles

| Principle | Description |
|-----------|-------------|
| **Stateful Access** | Builtin tools can read/write EventLog, DataStore |
| **TUI Integration** | `nika:prompt` triggers TUI widgets, pauses execution |
| **No Network** | Builtin tools are local-only, no retry/timeout needed |
| **Composable** | `nika:run` enables workflow-in-workflow |
| **Observable** | All tools emit events for tracing |

### 1.3 Tool Hierarchy

```rust
// rig's base trait (external tools)
pub trait ToolDyn: Send + Sync {
    fn name(&self) -> &str;
    fn definition(&self) -> ToolDefinition;
    fn call(&self, args: String) -> BoxFuture<'_, Result<String, ToolError>>;
}

// Nika's extended trait (builtin tools)
pub trait BuiltinTool: ToolDyn {
    /// Access to shared event log
    fn event_log(&self) -> &EventLog;

    /// Access to shared data store
    fn data_store(&self) -> &DataStore;

    /// Tool category for routing
    fn category(&self) -> BuiltinCategory {
        BuiltinCategory::ControlFlow
    }
}

pub enum BuiltinCategory {
    ControlFlow,  // nika:sleep, nika:assert
    HumanInLoop,  // nika:prompt
    Composition,  // nika:run
    Observability, // nika:log, nika:emit
}
```

---

## Part 2: BuiltinToolRouter

### 2.1 Router Implementation

```rust
// src/runtime/builtin/router.rs

use std::sync::Arc;
use rustc_hash::FxHashMap;
use crate::event::EventLog;
use crate::store::DataStore;

/// Routes `nika:*` tool calls to builtin implementations
pub struct BuiltinToolRouter {
    tools: FxHashMap<&'static str, Arc<dyn BuiltinTool>>,
    event_log: EventLog,
    data_store: DataStore,
}

impl BuiltinToolRouter {
    /// Create router with all TIER 1 tools registered
    pub fn new(event_log: EventLog, data_store: DataStore) -> Self {
        let mut tools = FxHashMap::default();

        // Register TIER 1 tools
        tools.insert("prompt", Arc::new(PromptTool::new(event_log.clone())) as _);
        tools.insert("run", Arc::new(RunTool::new(event_log.clone(), data_store.clone())) as _);
        tools.insert("sleep", Arc::new(SleepTool::new(event_log.clone())) as _);
        tools.insert("log", Arc::new(LogTool::new(event_log.clone())) as _);
        tools.insert("assert", Arc::new(AssertTool::new(event_log.clone())) as _);
        tools.insert("emit", Arc::new(EmitTool::new(event_log.clone())) as _);

        Self { tools, event_log, data_store }
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
                code: BuiltinErrorCode::InvalidPrefix,
            })?;

        let tool = self.tools.get(name)
            .ok_or_else(|| NikaError::BuiltinToolError {
                tool: tool_name.to_string(),
                reason: format!("Unknown builtin tool: {}", name),
                code: BuiltinErrorCode::UnknownTool,
            })?;

        // Emit BuiltinInvoke event
        self.event_log.emit(EventKind::BuiltinInvoke {
            tool: Arc::from(tool_name),
            args: args.clone(),
        });

        // Call tool
        let result = tool.call(args).await
            .map_err(|e| NikaError::BuiltinToolError {
                tool: tool_name.to_string(),
                reason: e.to_string(),
                code: BuiltinErrorCode::ExecutionFailed,
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
        self.tools.values().map(|t| t.definition()).collect()
    }
}
```

### 2.2 Integration with Executor

```rust
// src/runtime/executor.rs (modified)

impl Executor {
    async fn execute_invoke(&self, task: &Task) -> Result<TaskResult, NikaError> {
        let invoke = task.action.as_invoke().unwrap();

        // Route based on prefix
        if BuiltinToolRouter::is_builtin(&invoke.tool) {
            // Builtin tool
            self.builtin_router.dispatch(&invoke.tool, invoke.params_json()).await
        } else {
            // MCP tool (existing logic)
            self.mcp_client.call_tool(&invoke.server, &invoke.tool, invoke.params).await
        }
    }
}
```

---

## Part 3: TIER 1 Tool Specifications

### 3.1 nika:prompt — Human-in-the-Loop Input

**Purpose:** Pause execution and request user input via TUI widget.

```rust
// src/runtime/builtin/prompt.rs

use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptParams {
    /// Input type: confirm, text, select, multiselect
    #[serde(rename = "type")]
    pub prompt_type: PromptType,
    /// Message to display to user
    pub message: String,
    /// Options for select/multiselect
    #[serde(default)]
    pub options: Vec<String>,
    /// Default value
    #[serde(default)]
    pub default: Option<String>,
    /// Timeout in seconds (0 = no timeout)
    #[serde(default)]
    pub timeout: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PromptType {
    Confirm,      // Yes/No → bool
    Text,         // Free text → string
    Select,       // Single choice → string
    Multiselect,  // Multiple choices → string[]
}

pub struct PromptTool {
    event_log: EventLog,
    /// Channel to TUI for prompt requests
    tui_tx: Option<mpsc::Sender<PromptRequest>>,
}

impl PromptTool {
    pub fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "nika:prompt".to_string(),
            description: "Request input from the user. Pauses execution until response.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "type": {
                        "type": "string",
                        "enum": ["confirm", "text", "select", "multiselect"],
                        "description": "Type of input to request"
                    },
                    "message": {
                        "type": "string",
                        "description": "Message to display to user"
                    },
                    "options": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Options for select/multiselect"
                    },
                    "default": {
                        "type": "string",
                        "description": "Default value"
                    },
                    "timeout": {
                        "type": "integer",
                        "description": "Timeout in seconds (0 = no timeout)"
                    }
                },
                "required": ["type", "message"],
                "additionalProperties": false
            }),
        }
    }

    pub async fn call(&self, args: String) -> Result<String, ToolError> {
        let params: PromptParams = serde_json::from_str(&args)
            .map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        // Validate options for select/multiselect
        if matches!(params.prompt_type, PromptType::Select | PromptType::Multiselect) {
            if params.options.is_empty() {
                return Err(ToolError::InvalidArguments(
                    "options required for select/multiselect".into()
                ));
            }
        }

        // Emit event (workflow paused)
        self.event_log.emit(EventKind::WorkflowPaused {
            reason: format!("Waiting for user input: {}", params.message),
        });

        // Send to TUI and wait for response
        let (response_tx, response_rx) = oneshot::channel();
        let request = PromptRequest {
            params: params.clone(),
            response_tx,
        };

        if let Some(tx) = &self.tui_tx {
            tx.send(request).await
                .map_err(|_| ToolError::ExecutionError("TUI disconnected".into()))?;
        } else {
            // Headless mode - use defaults or error
            return self.handle_headless(&params);
        }

        // Wait for response (with optional timeout)
        let response = if params.timeout > 0 {
            tokio::time::timeout(
                Duration::from_secs(params.timeout as u64),
                response_rx
            ).await
            .map_err(|_| ToolError::ExecutionError("Prompt timeout".into()))?
        } else {
            response_rx.await
        }.map_err(|_| ToolError::ExecutionError("Response channel closed".into()))?;

        // Emit event (workflow resumed)
        self.event_log.emit(EventKind::WorkflowResumed {
            response: response.clone(),
        });

        Ok(response)
    }

    fn handle_headless(&self, params: &PromptParams) -> Result<String, ToolError> {
        // In headless mode, use defaults or error
        match params.default.as_ref() {
            Some(default) => Ok(default.clone()),
            None => Err(ToolError::ExecutionError(
                "nika:prompt requires TUI or default value in headless mode".into()
            )),
        }
    }
}
```

**YAML Usage:**

```yaml
tasks:
  - id: confirm_deploy
    invoke:
      tool: nika:prompt
      params:
        type: confirm
        message: "Deploy to production?"
        default: "false"
    use.confirmed: result

  - id: select_env
    invoke:
      tool: nika:prompt
      params:
        type: select
        message: "Select environment"
        options: ["dev", "staging", "prod"]
        timeout: 60
    use.env: result
```

### 3.2 nika:run — Workflow Composition

**Purpose:** Execute a sub-workflow and return its result.

```rust
// src/runtime/builtin/run.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunParams {
    /// Path to workflow file (relative to project root)
    pub path: String,
    /// Input bindings for sub-workflow
    #[serde(default)]
    pub inputs: FxHashMap<String, Value>,
    /// Whether to isolate data store (default: true)
    #[serde(default = "default_true")]
    pub isolated: bool,
}

fn default_true() -> bool { true }

pub struct RunTool {
    event_log: EventLog,
    data_store: DataStore,
    project_root: PathBuf,
}

impl RunTool {
    pub fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "nika:run".to_string(),
            description: "Execute a sub-workflow and return its result.".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to workflow file (e.g., './sub/workflow.nika.yaml')"
                    },
                    "inputs": {
                        "type": "object",
                        "description": "Input bindings for sub-workflow"
                    },
                    "isolated": {
                        "type": "boolean",
                        "description": "Isolate data store (default: true)"
                    }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
        }
    }

    pub async fn call(&self, args: String) -> Result<String, ToolError> {
        let params: RunParams = serde_json::from_str(&args)
            .map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        // Resolve path relative to project root
        let workflow_path = self.project_root.join(&params.path);

        // SECURITY: Path traversal protection
        // Canonicalize to resolve ../.. and ensure path stays within project
        let canonical_path = workflow_path.canonicalize()
            .map_err(|_| ToolError::ExecutionError(
                format!("Workflow not found: {}", params.path)
            ))?;
        let canonical_root = self.project_root.canonicalize()
            .map_err(|e| ToolError::ExecutionError(
                format!("Failed to resolve project root: {}", e)
            ))?;

        if !canonical_path.starts_with(&canonical_root) {
            return Err(ToolError::ExecutionError(
                format!("Path traversal denied: {} escapes project root", params.path)
            ));
        }

        if !canonical_path.exists() {
            return Err(ToolError::ExecutionError(
                format!("Workflow not found: {}", params.path)
            ));
        }

        // Parse sub-workflow
        let yaml = tokio::fs::read_to_string(&workflow_path).await
            .map_err(|e| ToolError::ExecutionError(e.to_string()))?;
        let workflow = Workflow::from_yaml(&yaml)
            .map_err(|e| ToolError::ExecutionError(e.to_string()))?;

        // Create data store for sub-workflow
        let sub_store = if params.isolated {
            let mut store = DataStore::new();
            // Inject inputs
            for (key, value) in params.inputs {
                store.set(Arc::from(key.as_str()), value);
            }
            store
        } else {
            // Share parent store (with inputs overlaid)
            let mut store = self.data_store.clone();
            for (key, value) in params.inputs {
                store.set(Arc::from(key.as_str()), value);
            }
            store
        };

        // Emit event
        self.event_log.emit(EventKind::SubWorkflowStarted {
            path: Arc::from(params.path.as_str()),
            inputs: params.inputs.clone(),
        });

        // Execute sub-workflow
        let runner = Runner::new(workflow, sub_store, self.event_log.clone());
        let result = runner.run().await
            .map_err(|e| ToolError::ExecutionError(e.to_string()))?;

        // Emit event
        self.event_log.emit(EventKind::SubWorkflowCompleted {
            path: Arc::from(params.path.as_str()),
        });

        // Return final task result as JSON
        serde_json::to_string(&result)
            .map_err(|e| ToolError::ExecutionError(e.to_string()))
    }
}
```

**YAML Usage:**

```yaml
tasks:
  - id: run_tests
    invoke:
      tool: nika:run
      params:
        path: ./workflows/test-suite.nika.yaml
        inputs:
          env: "staging"
          verbose: true
    use.test_result: result

  - id: check_tests
    infer: "Analyze test results: {{use.test_result}}"
```

### 3.3 nika:sleep — Delay/Wait

**Purpose:** Pause execution for a specified duration.

```rust
// src/runtime/builtin/sleep.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SleepParams {
    /// Duration string: "5s", "1m", "500ms", "1h30m"
    pub duration: String,
}

pub struct SleepTool {
    event_log: EventLog,
}

impl SleepTool {
    pub fn definition(&self) -> ToolDefinition {
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

    pub async fn call(&self, args: String) -> Result<String, ToolError> {
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

/// Parse duration string like "5s", "1m", "500ms", "1h30m"
fn parse_duration(s: &str) -> Result<Duration, String> {
    // Use humantime crate for robust parsing
    humantime::parse_duration(s)
        .map_err(|e| format!("Invalid duration '{}': {}", s, e))
}
```

**YAML Usage:**

```yaml
tasks:
  - id: deploy
    exec: "./deploy.sh"

  - id: wait_for_startup
    invoke:
      tool: nika:sleep
      params:
        duration: "30s"

  - id: health_check
    fetch:
      url: "https://api.example.com/health"
      method: GET
```

### 3.4 nika:log — Debug Output

**Purpose:** Emit structured log messages for debugging.

```rust
// src/runtime/builtin/log.rs

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
    pub fn definition(&self) -> ToolDefinition {
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

    pub async fn call(&self, args: String) -> Result<String, ToolError> {
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
            LogLevel::Debug => tracing::debug!(message = %params.message, data = ?params.data),
            LogLevel::Info => tracing::info!(message = %params.message, data = ?params.data),
            LogLevel::Warn => tracing::warn!(message = %params.message, data = ?params.data),
            LogLevel::Error => tracing::error!(message = %params.message, data = ?params.data),
        }

        Ok(json!({ "logged": true }).to_string())
    }
}
```

**YAML Usage:**

```yaml
tasks:
  - id: log_start
    invoke:
      tool: nika:log
      params:
        level: info
        message: "Starting workflow"
        data:
          workflow: "generate-pages"
          timestamp: "{{now}}"

  - id: process
    infer: "Generate content"
    use.content: result

  - id: log_result
    invoke:
      tool: nika:log
      params:
        level: debug
        message: "Content generated"
        data:
          length: "{{use.content.length}}"
```

### 3.5 nika:assert — Validation Gate

**Purpose:** Validate conditions and fail fast if not met.

```rust
// src/runtime/builtin/assert.rs

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
    pub fn definition(&self) -> ToolDefinition {
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

    pub async fn call(&self, args: String) -> Result<String, ToolError> {
        let params: AssertParams = serde_json::from_str(&args)
            .map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        // Resolve bindings in condition
        let resolved = self.data_store.resolve_template(&params.condition)
            .map_err(|e| ToolError::ExecutionError(e.to_string()))?;

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

/// Evaluate simple condition expressions
/// Supports: ==, !=, >, <, >=, <=, &&, ||, !
fn evaluate_condition(expr: &str) -> Result<bool, String> {
    // Use evalexpr crate for safe expression evaluation
    evalexpr::eval_boolean(expr)
        .map_err(|e| format!("Invalid condition '{}': {}", expr, e))
}
```

**YAML Usage:**

```yaml
tasks:
  - id: fetch_data
    fetch:
      url: "https://api.example.com/items"
    use.items: result.data

  - id: validate_items
    invoke:
      tool: nika:assert
      params:
        condition: "{{use.items.length}} > 0"
        message: "No items returned from API"

  - id: process_items
    for_each: "{{use.items}}"
    infer: "Process item: {{use.item}}"
```

### 3.6 nika:emit — Custom Event

**Purpose:** Emit custom events for observability and external integrations.

```rust
// src/runtime/builtin/emit.rs

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
    pub fn definition(&self) -> ToolDefinition {
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

    pub async fn call(&self, args: String) -> Result<String, ToolError> {
        let params: EmitParams = serde_json::from_str(&args)
            .map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

        self.event_log.emit(EventKind::CustomEvent {
            name: Arc::from(params.event.as_str()),
            data: params.data.clone(),
        });

        Ok(json!({ "emitted": params.event }).to_string())
    }
}
```

**YAML Usage:**

```yaml
tasks:
  - id: generate_page
    infer: "Generate landing page for {{use.entity}}"
    use.content: result

  - id: emit_generated
    invoke:
      tool: nika:emit
      params:
        event: "page_generated"
        data:
          entity: "{{use.entity}}"
          locale: "{{use.locale}}"
          content_length: "{{use.content.length}}"
```

---

## Part 4: Error Codes

### 4.1 Builtin Tool Error Range

```rust
// src/error.rs (additions)

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinErrorCode {
    // NIKA-200: General
    InvalidPrefix = 200,      // Missing nika: prefix
    UnknownTool = 201,        // Tool not found
    InvalidArguments = 202,   // Bad params
    ExecutionFailed = 203,    // Runtime error

    // NIKA-210: nika:prompt
    PromptTimeout = 210,      // User didn't respond
    PromptCancelled = 211,    // User cancelled
    TuiDisconnected = 212,    // TUI not available

    // NIKA-220: nika:run
    WorkflowNotFound = 220,   // File doesn't exist
    WorkflowParseFailed = 221, // Invalid YAML
    SubWorkflowFailed = 222,  // Child workflow error
    PathTraversalDenied = 223, // Path escapes project root (../../)

    // NIKA-230: nika:sleep
    DurationTooLong = 230,    // Exceeds 1 hour
    InvalidDuration = 231,    // Can't parse duration

    // NIKA-240: nika:assert
    AssertionFailed = 240,    // Condition false
    InvalidCondition = 241,   // Can't evaluate

    // NIKA-250: nika:emit
    InvalidEventName = 250,   // Bad event name
}

impl From<BuiltinErrorCode> for u16 {
    fn from(code: BuiltinErrorCode) -> u16 {
        code as u16
    }
}
```

### 4.2 Error Messages

| Code | Message Template |
|------|-----------------|
| NIKA-200 | `[NIKA-200] Invalid builtin tool: {tool} (missing nika: prefix)` |
| NIKA-201 | `[NIKA-201] Unknown builtin tool: {tool}` |
| NIKA-210 | `[NIKA-210] Prompt timeout after {timeout}s waiting for user input` |
| NIKA-220 | `[NIKA-220] Workflow not found: {path}` |
| NIKA-223 | `[NIKA-223] Path traversal denied: {path} escapes project root` |
| NIKA-230 | `[NIKA-230] Duration exceeds maximum (1 hour): {duration}` |
| NIKA-240 | `[NIKA-240] Assertion failed: {condition}` |

---

## Part 5: New Event Variants

### 5.1 EventKind Additions

```rust
// src/event/log.rs (additions)

pub enum EventKind {
    // ... existing variants ...

    // Builtin tool events
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
        data: Option<Value>,
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
        data: Value,
    },
    SubWorkflowStarted {
        path: Arc<str>,
        inputs: FxHashMap<String, Value>,
    },
    SubWorkflowCompleted {
        path: Arc<str>,
    },
}
```

---

## Part 6: Test Plan

### 6.1 Unit Tests (Per Tool)

| Tool | Test Cases | Count |
|------|------------|-------|
| `nika:prompt` | types (confirm/text/select/multiselect), timeout, headless, validation | 12 |
| `nika:run` | valid path, invalid path, inputs, isolated/shared store, path traversal denied | 11 |
| `nika:sleep` | durations (s/m/ms), max limit, invalid format | 8 |
| `nika:log` | levels, data serialization, tracing integration | 6 |
| `nika:assert` | operators (==, >, <, etc.), binding resolution, failure | 10 |
| `nika:emit` | event names, data payloads, EventLog verification | 6 |
| **Router** | prefix detection, dispatch, unknown tool, definitions | 8 |
| **Total** | | **61** |

### 6.2 Integration Tests

```rust
#[tokio::test]
async fn test_workflow_with_builtin_tools() {
    let yaml = r#"
schema: nika/workflow@0.5
workflow: test-builtins
tasks:
  - id: log_start
    invoke:
      tool: nika:log
      params:
        level: info
        message: "Starting test"

  - id: sleep_briefly
    invoke:
      tool: nika:sleep
      params:
        duration: "100ms"

  - id: validate
    invoke:
      tool: nika:assert
      params:
        condition: "1 == 1"

  - id: emit_done
    invoke:
      tool: nika:emit
      params:
        event: "test_complete"
"#;

    let workflow = Workflow::from_yaml(yaml).unwrap();
    let result = Runner::new(workflow, DataStore::new(), EventLog::new())
        .run()
        .await;

    assert!(result.is_ok());
}
```

---

## Part 7: Dependencies

### 7.1 New Crate Dependencies

```toml
# Cargo.toml additions

[dependencies]
humantime = "2.1"      # Duration parsing ("5s", "1m")
evalexpr = "11.0"      # Safe expression evaluation for nika:assert
```

### 7.2 Feature Flags

```toml
[features]
default = ["builtin-tools"]
builtin-tools = []  # Can disable for minimal builds
```

---

## Part 8: Files Changed Summary

### 8.1 New Files

| File | Lines | Purpose |
|------|-------|---------|
| `src/runtime/builtin/mod.rs` | ~50 | Module exports |
| `src/runtime/builtin/router.rs` | ~150 | BuiltinToolRouter |
| `src/runtime/builtin/prompt.rs` | ~200 | nika:prompt |
| `src/runtime/builtin/run.rs` | ~150 | nika:run |
| `src/runtime/builtin/sleep.rs` | ~80 | nika:sleep |
| `src/runtime/builtin/log.rs` | ~100 | nika:log |
| `src/runtime/builtin/assert.rs` | ~120 | nika:assert |
| `src/runtime/builtin/emit.rs` | ~70 | nika:emit |
| **Total New** | **~920** | |

### 8.2 Modified Files

| File | Changes |
|------|---------|
| `src/runtime/mod.rs` | Export `builtin` module |
| `src/runtime/executor.rs` | Add BuiltinToolRouter, route `nika:*` calls |
| `src/event/log.rs` | Add 9 new EventKind variants |
| `src/error.rs` | Add BuiltinErrorCode enum |
| `Cargo.toml` | Add humantime, evalexpr deps |

---

## Part 9: Migration Notes

### 9.1 Backward Compatibility

- No breaking changes to existing workflows
- `invoke:` verb continues to work for MCP tools
- New `nika:*` tools are additive

### 9.2 Schema Update

```yaml
# Schema version update needed
schema: nika/workflow@0.6  # Adds builtin tool support

# Or explicitly document in @0.5
# No schema change required - invoke: already supports tool names
```

---

## Appendix A: Tool Comparison Table

| Tool | Stateful | Async | TUI | Errors |
|------|----------|-------|-----|--------|
| `nika:prompt` | Read EventLog | Yes (channel) | Required* | 210-212 |
| `nika:run` | Read/Write DataStore | Yes (sub-runner) | No | 220-222 |
| `nika:sleep` | Read EventLog | Yes (sleep) | No | 230-231 |
| `nika:log` | Write EventLog | No | No | None |
| `nika:assert` | Read DataStore | No | No | 240-241 |
| `nika:emit` | Write EventLog | No | No | 250 |

*`nika:prompt` works in headless mode with `default` value

---

## Appendix B: YAML Quick Reference

```yaml
# nika:prompt - Human input
- invoke:
    tool: nika:prompt
    params:
      type: confirm|text|select|multiselect
      message: "string"
      options: ["a", "b"]  # for select/multiselect
      default: "fallback"
      timeout: 60

# nika:run - Sub-workflow
- invoke:
    tool: nika:run
    params:
      path: "./workflows/sub.nika.yaml"
      inputs:
        key: value
      isolated: true

# nika:sleep - Delay
- invoke:
    tool: nika:sleep
    params:
      duration: "5s"  # 500ms, 1m, 1h30m

# nika:log - Debug output
- invoke:
    tool: nika:log
    params:
      level: debug|info|warn|error
      message: "Log message"
      data: { key: "value" }

# nika:assert - Validation
- invoke:
    tool: nika:assert
    params:
      condition: "{{use.count}} > 0"
      message: "Custom error message"

# nika:emit - Custom event
- invoke:
    tool: nika:emit
    params:
      event: "event_name"
      data: { key: "value" }
```

---

## References

- [ADR-001: 5 Semantic Verbs](../../../tools/nika/.claude/rules/adr/adr-001-5-semantic-verbs.md)
- [INDEX.md: Builtin Tools by Tier](./INDEX.md#builtin-tools-by-tier-2026-02-24)
- [SpawnAgentTool Implementation](../../../tools/nika/src/runtime/spawn.rs)
- [rig::tool::ToolDyn](https://docs.rs/rig-core/latest/rig/tool/trait.ToolDyn.html)
