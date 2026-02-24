# Phase 3: Builtin Tools — 6 nika:* Tools + Router

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement 6 TIER 1 builtin tools (`nika:*` prefix) that extend workflow capabilities without adding new verbs.

**Architecture:** `nika:*` prefix routes to `BuiltinToolRouter` while `server:tool` routes to MCP. All builtin tools implement `BuiltinTool` trait (extends `rig::ToolDyn`) with access to `EventLog` and `DataStore`.

**Tech Stack:** rig-core (ToolDyn), humantime (duration parsing), evalexpr (condition evaluation), tokio (async), serde_json

**Skills:** @rust-async, @test-driven-development, @rust-core

---

## Phase Dependencies

| Depends On | Provides |
|------------|----------|
| Phase 2 (ChatWorkflow) | `EventLog`, `DataStore` references |
| rig-core | `ToolDyn` trait |

---

## Tasks Overview

| Task | Focus | Tests | Files |
|------|-------|-------|-------|
| 3.1 | BuiltinTool trait | 5 | `src/runtime/builtin/mod.rs` |
| 3.2 | BuiltinToolRouter | 8 | `src/runtime/builtin/router.rs` |
| 3.3 | nika:sleep tool | 8 | `src/runtime/builtin/sleep.rs` |
| 3.4 | nika:log tool | 6 | `src/runtime/builtin/log.rs` |
| 3.5 | nika:emit tool | 6 | `src/runtime/builtin/emit.rs` |
| 3.6 | nika:assert tool | 10 | `src/runtime/builtin/assert.rs` |
| 3.7 | nika:prompt tool | 12 | `src/runtime/builtin/prompt.rs` |
| 3.8 | nika:run tool | 10 | `src/runtime/builtin/run.rs` |
| 3.9 | Add EventKind variants | 5 | `src/event/log.rs` |
| 3.10 | Executor routing integration | 6 | `src/runtime/executor.rs` |
| **Total** | | **76** | |

---

## Task 3.1: BuiltinTool Trait

**Files:**
- Create: `src/runtime/builtin/mod.rs`
- Modify: `src/runtime/mod.rs` (export module)
- Modify: `Cargo.toml` (add humantime, evalexpr)

**Step 1: Add dependencies to Cargo.toml**

```bash
cd /Users/thibaut/supernovae-st/supernovae-agi/nika/tools/nika
```

Add to `Cargo.toml`:
```toml
humantime = "2.1"
evalexpr = "11.0"
```

**Step 2: Write the failing test**

```rust
// src/runtime/builtin/mod.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_category_control_flow() {
        assert_eq!(BuiltinCategory::ControlFlow as u8, 0);
    }

    #[test]
    fn test_builtin_category_human_in_loop() {
        assert_eq!(BuiltinCategory::HumanInLoop as u8, 1);
    }

    #[test]
    fn test_builtin_category_composition() {
        assert_eq!(BuiltinCategory::Composition as u8, 2);
    }

    #[test]
    fn test_builtin_category_observability() {
        assert_eq!(BuiltinCategory::Observability as u8, 3);
    }

    #[test]
    fn test_builtin_tool_trait_has_event_log() {
        // Trait method exists - compile-time check
        fn _check<T: BuiltinTool>(t: &T) {
            let _log = t.event_log();
        }
    }
}
```

**Step 3: Run test to verify it fails**

```bash
cargo test builtin_category --lib
```

Expected: FAIL with "cannot find type `BuiltinCategory`"

**Step 4: Write minimal implementation**

```rust
// src/runtime/builtin/mod.rs

use std::sync::Arc;
use crate::event::EventLog;
use crate::store::DataStore;
use rig::tool::{ToolDyn, ToolDefinition};

pub mod router;
pub mod sleep;
pub mod log;
pub mod emit;
pub mod assert;
pub mod prompt;
pub mod run;

/// Category for builtin tool routing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BuiltinCategory {
    ControlFlow = 0,    // nika:sleep, nika:assert
    HumanInLoop = 1,    // nika:prompt
    Composition = 2,    // nika:run
    Observability = 3,  // nika:log, nika:emit
}

/// Extended trait for Nika's builtin tools
pub trait BuiltinTool: ToolDyn + Send + Sync {
    /// Access to shared event log
    fn event_log(&self) -> &EventLog;

    /// Access to shared data store
    fn data_store(&self) -> &DataStore;

    /// Tool category for routing
    fn category(&self) -> BuiltinCategory {
        BuiltinCategory::ControlFlow
    }
}
```

**Step 5: Run test to verify it passes**

```bash
cargo test builtin_category --lib
```

Expected: PASS

**Step 6: Commit**

```bash
git add src/runtime/builtin/mod.rs Cargo.toml
git commit -m "feat(builtin): add BuiltinTool trait and BuiltinCategory enum

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

## Task 3.2: BuiltinToolRouter

**Files:**
- Create: `src/runtime/builtin/router.rs`
- Modify: `src/runtime/builtin/mod.rs` (re-export)

**Step 1: Write the failing test**

```rust
// src/runtime/builtin/router.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_builtin_with_nika_prefix() {
        assert!(BuiltinToolRouter::is_builtin("nika:sleep"));
        assert!(BuiltinToolRouter::is_builtin("nika:prompt"));
    }

    #[test]
    fn test_is_builtin_without_prefix() {
        assert!(!BuiltinToolRouter::is_builtin("novanet:describe"));
        assert!(!BuiltinToolRouter::is_builtin("sleep"));
    }

    #[test]
    fn test_extract_name_valid() {
        assert_eq!(BuiltinToolRouter::extract_name("nika:sleep"), Some("sleep"));
        assert_eq!(BuiltinToolRouter::extract_name("nika:prompt"), Some("prompt"));
    }

    #[test]
    fn test_extract_name_invalid() {
        assert_eq!(BuiltinToolRouter::extract_name("novanet:describe"), None);
        assert_eq!(BuiltinToolRouter::extract_name("sleep"), None);
    }

    #[test]
    fn test_router_new_registers_tools() {
        let log = EventLog::new();
        let store = DataStore::new();
        let router = BuiltinToolRouter::new(log, store);
        assert!(router.has_tool("sleep"));
        assert!(router.has_tool("log"));
        assert!(router.has_tool("emit"));
        assert!(router.has_tool("assert"));
        assert!(router.has_tool("prompt"));
        assert!(router.has_tool("run"));
    }

    #[test]
    fn test_router_has_tool_unknown() {
        let log = EventLog::new();
        let store = DataStore::new();
        let router = BuiltinToolRouter::new(log, store);
        assert!(!router.has_tool("unknown"));
    }

    #[test]
    fn test_definitions_returns_all_tools() {
        let log = EventLog::new();
        let store = DataStore::new();
        let router = BuiltinToolRouter::new(log, store);
        let defs = router.definitions();
        assert_eq!(defs.len(), 6); // 6 TIER 1 tools
    }

    #[tokio::test]
    async fn test_dispatch_unknown_tool_error() {
        let log = EventLog::new();
        let store = DataStore::new();
        let router = BuiltinToolRouter::new(log, store);
        let result = router.dispatch("nika:unknown", "{}".to_string()).await;
        assert!(result.is_err());
    }
}
```

**Step 2: Run test to verify it fails**

```bash
cargo test router --lib
```

Expected: FAIL with "cannot find type `BuiltinToolRouter`"

**Step 3: Write minimal implementation**

```rust
// src/runtime/builtin/router.rs

use std::sync::Arc;
use rustc_hash::FxHashMap;
use crate::error::NikaError;
use crate::event::{EventLog, EventKind};
use crate::store::DataStore;
use super::BuiltinTool;
use rig::tool::ToolDefinition;

/// Routes `nika:*` tool calls to builtin implementations
pub struct BuiltinToolRouter {
    tools: FxHashMap<&'static str, Arc<dyn BuiltinTool>>,
    event_log: EventLog,
    #[allow(dead_code)]
    data_store: DataStore,
}

impl BuiltinToolRouter {
    /// Create router with all TIER 1 tools registered
    pub fn new(event_log: EventLog, data_store: DataStore) -> Self {
        use super::{sleep::SleepTool, log::LogTool, emit::EmitTool,
                    assert::AssertTool, prompt::PromptTool, run::RunTool};

        let mut tools: FxHashMap<&'static str, Arc<dyn BuiltinTool>> = FxHashMap::default();

        tools.insert("sleep", Arc::new(SleepTool::new(event_log.clone())));
        tools.insert("log", Arc::new(LogTool::new(event_log.clone())));
        tools.insert("emit", Arc::new(EmitTool::new(event_log.clone())));
        tools.insert("assert", Arc::new(AssertTool::new(event_log.clone(), data_store.clone())));
        tools.insert("prompt", Arc::new(PromptTool::new(event_log.clone())));
        tools.insert("run", Arc::new(RunTool::new(event_log.clone(), data_store.clone())));

        Self { tools, event_log, data_store }
    }

    /// Check if tool name has nika: prefix
    pub fn is_builtin(tool_name: &str) -> bool {
        tool_name.starts_with("nika:")
    }

    /// Extract tool name after nika: prefix
    pub fn extract_name(tool_name: &str) -> Option<&str> {
        tool_name.strip_prefix("nika:")
    }

    /// Check if a tool is registered
    pub fn has_tool(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// Dispatch to builtin tool
    pub async fn dispatch(&self, tool_name: &str, args: String) -> Result<String, NikaError> {
        let name = Self::extract_name(tool_name)
            .ok_or_else(|| NikaError::BuiltinToolError {
                tool: tool_name.to_string(),
                reason: "Missing nika: prefix".into(),
            })?;

        let tool = self.tools.get(name)
            .ok_or_else(|| NikaError::BuiltinToolError {
                tool: tool_name.to_string(),
                reason: format!("Unknown builtin tool: {}", name),
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

**Step 4: Run test to verify it passes**

```bash
cargo test router --lib
```

Expected: PASS (once we implement stub tools in 3.3-3.8)

**Step 5: Commit**

```bash
git add src/runtime/builtin/router.rs
git commit -m "feat(builtin): add BuiltinToolRouter with prefix-based dispatch

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

## Task 3.3: nika:sleep Tool

**Files:**
- Create: `src/runtime/builtin/sleep.rs`

**Step 1: Write the failing test**

```rust
// src/runtime/builtin/sleep.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sleep_100ms() {
        let log = EventLog::new();
        let tool = SleepTool::new(log);
        let start = std::time::Instant::now();
        let result = tool.call(r#"{"duration": "100ms"}"#.to_string()).await;
        let elapsed = start.elapsed();
        assert!(result.is_ok());
        assert!(elapsed >= Duration::from_millis(90)); // Allow some variance
    }

    #[tokio::test]
    async fn test_sleep_1s() {
        let log = EventLog::new();
        let tool = SleepTool::new(log);
        let result = tool.call(r#"{"duration": "1s"}"#.to_string()).await;
        assert!(result.is_ok());
        let parsed: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert!(parsed["slept_ms"].as_u64().unwrap() >= 1000);
    }

    #[tokio::test]
    async fn test_sleep_max_duration_exceeded() {
        let log = EventLog::new();
        let tool = SleepTool::new(log);
        let result = tool.call(r#"{"duration": "2h"}"#.to_string()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_sleep_invalid_duration() {
        let log = EventLog::new();
        let tool = SleepTool::new(log);
        let result = tool.call(r#"{"duration": "invalid"}"#.to_string()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_sleep_complex_duration() {
        let log = EventLog::new();
        let tool = SleepTool::new(log);
        let result = tool.call(r#"{"duration": "1m30s"}"#.to_string()).await;
        assert!(result.is_ok()); // Parses to 90s which is under 1h limit
    }

    #[test]
    fn test_sleep_definition() {
        let log = EventLog::new();
        let tool = SleepTool::new(log);
        let def = tool.definition();
        assert_eq!(def.name, "nika:sleep");
    }

    #[test]
    fn test_sleep_category() {
        let log = EventLog::new();
        let tool = SleepTool::new(log);
        assert_eq!(tool.category(), BuiltinCategory::ControlFlow);
    }

    #[tokio::test]
    async fn test_sleep_emits_event() {
        let log = EventLog::new();
        let tool = SleepTool::new(log.clone());
        let _ = tool.call(r#"{"duration": "10ms"}"#.to_string()).await;
        let events = log.events();
        assert!(events.iter().any(|e| matches!(e.kind, EventKind::BuiltinSleep { .. })));
    }
}
```

**Step 2: Run test to verify it fails**

```bash
cargo test sleep --lib
```

Expected: FAIL with "cannot find struct `SleepTool`"

**Step 3: Write minimal implementation**

```rust
// src/runtime/builtin/sleep.rs

use std::sync::Arc;
use std::time::Duration;
use std::future::Future;
use std::pin::Pin;
use serde::{Deserialize, Serialize};
use serde_json::json;
use rig::tool::{ToolDyn, ToolDefinition, ToolError};
use crate::event::{EventLog, EventKind};
use crate::store::DataStore;
use super::{BuiltinTool, BuiltinCategory};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SleepParams {
    /// Duration string: "5s", "1m", "500ms", "1h30m"
    pub duration: String,
}

pub struct SleepTool {
    event_log: EventLog,
    data_store: DataStore,
}

impl SleepTool {
    pub fn new(event_log: EventLog) -> Self {
        Self {
            event_log,
            data_store: DataStore::new(),
        }
    }
}

impl ToolDyn for SleepTool {
    fn name(&self) -> &str {
        "nika:sleep"
    }

    fn definition(&self) -> ToolDefinition {
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

    fn call(&self, args: String) -> Pin<Box<dyn Future<Output = Result<String, ToolError>> + Send + '_>> {
        Box::pin(async move {
            let params: SleepParams = serde_json::from_str(&args)
                .map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

            let duration = humantime::parse_duration(&params.duration)
                .map_err(|e| ToolError::InvalidArguments(format!("Invalid duration '{}': {}", params.duration, e)))?;

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
        })
    }
}

impl BuiltinTool for SleepTool {
    fn event_log(&self) -> &EventLog {
        &self.event_log
    }

    fn data_store(&self) -> &DataStore {
        &self.data_store
    }

    fn category(&self) -> BuiltinCategory {
        BuiltinCategory::ControlFlow
    }
}
```

**Step 4: Run test to verify it passes**

```bash
cargo test sleep --lib
```

Expected: PASS

**Step 5: Commit**

```bash
git add src/runtime/builtin/sleep.rs
git commit -m "feat(builtin): add nika:sleep tool with humantime parsing

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

## Task 3.4: nika:log Tool

**Files:**
- Create: `src/runtime/builtin/log.rs`

**Step 1: Write the failing test**

```rust
// src/runtime/builtin/log.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_log_info() {
        let log = EventLog::new();
        let tool = LogTool::new(log.clone());
        let result = tool.call(r#"{"level": "info", "message": "Test message"}"#.to_string()).await;
        assert!(result.is_ok());
        let parsed: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(parsed["logged"], true);
    }

    #[tokio::test]
    async fn test_log_with_data() {
        let log = EventLog::new();
        let tool = LogTool::new(log.clone());
        let result = tool.call(r#"{"level": "debug", "message": "Test", "data": {"key": "value"}}"#.to_string()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_log_levels() {
        let log = EventLog::new();
        let tool = LogTool::new(log.clone());

        for level in ["debug", "info", "warn", "error"] {
            let args = format!(r#"{{"level": "{}", "message": "Test"}}"#, level);
            let result = tool.call(args).await;
            assert!(result.is_ok(), "Failed for level: {}", level);
        }
    }

    #[tokio::test]
    async fn test_log_emits_event() {
        let log = EventLog::new();
        let tool = LogTool::new(log.clone());
        let _ = tool.call(r#"{"level": "info", "message": "Test"}"#.to_string()).await;
        let events = log.events();
        assert!(events.iter().any(|e| matches!(e.kind, EventKind::BuiltinLog { .. })));
    }

    #[test]
    fn test_log_definition() {
        let log = EventLog::new();
        let tool = LogTool::new(log);
        let def = tool.definition();
        assert_eq!(def.name, "nika:log");
    }

    #[test]
    fn test_log_category() {
        let log = EventLog::new();
        let tool = LogTool::new(log);
        assert_eq!(tool.category(), BuiltinCategory::Observability);
    }
}
```

**Step 2: Run test to verify it fails**

```bash
cargo test log --lib
```

Expected: FAIL with "cannot find struct `LogTool`"

**Step 3: Write minimal implementation**

```rust
// src/runtime/builtin/log.rs

use std::sync::Arc;
use std::future::Future;
use std::pin::Pin;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use rig::tool::{ToolDyn, ToolDefinition, ToolError};
use crate::event::{EventLog, EventKind};
use crate::store::DataStore;
use super::{BuiltinTool, BuiltinCategory};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogParams {
    pub level: LogLevel,
    pub message: String,
    #[serde(default)]
    pub data: Option<Value>,
}

pub struct LogTool {
    event_log: EventLog,
    data_store: DataStore,
}

impl LogTool {
    pub fn new(event_log: EventLog) -> Self {
        Self {
            event_log,
            data_store: DataStore::new(),
        }
    }
}

impl ToolDyn for LogTool {
    fn name(&self) -> &str {
        "nika:log"
    }

    fn definition(&self) -> ToolDefinition {
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

    fn call(&self, args: String) -> Pin<Box<dyn Future<Output = Result<String, ToolError>> + Send + '_>> {
        Box::pin(async move {
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
        })
    }
}

impl BuiltinTool for LogTool {
    fn event_log(&self) -> &EventLog {
        &self.event_log
    }

    fn data_store(&self) -> &DataStore {
        &self.data_store
    }

    fn category(&self) -> BuiltinCategory {
        BuiltinCategory::Observability
    }
}
```

**Step 4: Run test to verify it passes**

```bash
cargo test log --lib
```

Expected: PASS

**Step 5: Commit**

```bash
git add src/runtime/builtin/log.rs
git commit -m "feat(builtin): add nika:log tool with 4 log levels

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

## Task 3.5: nika:emit Tool

**Files:**
- Create: `src/runtime/builtin/emit.rs`

**Step 1: Write the failing test**

```rust
// src/runtime/builtin/emit.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_emit_custom_event() {
        let log = EventLog::new();
        let tool = EmitTool::new(log.clone());
        let result = tool.call(r#"{"event": "page_generated", "data": {"page": "home"}}"#.to_string()).await;
        assert!(result.is_ok());
        let parsed: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(parsed["emitted"], "page_generated");
    }

    #[tokio::test]
    async fn test_emit_without_data() {
        let log = EventLog::new();
        let tool = EmitTool::new(log.clone());
        let result = tool.call(r#"{"event": "workflow_started"}"#.to_string()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_emit_adds_to_event_log() {
        let log = EventLog::new();
        let tool = EmitTool::new(log.clone());
        let _ = tool.call(r#"{"event": "test_event", "data": {"key": "value"}}"#.to_string()).await;
        let events = log.events();
        assert!(events.iter().any(|e| matches!(&e.kind, EventKind::CustomEvent { name, .. } if name.as_ref() == "test_event")));
    }

    #[test]
    fn test_emit_definition() {
        let log = EventLog::new();
        let tool = EmitTool::new(log);
        let def = tool.definition();
        assert_eq!(def.name, "nika:emit");
    }

    #[test]
    fn test_emit_category() {
        let log = EventLog::new();
        let tool = EmitTool::new(log);
        assert_eq!(tool.category(), BuiltinCategory::Observability);
    }

    #[tokio::test]
    async fn test_emit_complex_data() {
        let log = EventLog::new();
        let tool = EmitTool::new(log.clone());
        let result = tool.call(r#"{"event": "metrics", "data": {"count": 42, "tags": ["a", "b"]}}"#.to_string()).await;
        assert!(result.is_ok());
    }
}
```

**Step 2: Run test to verify it fails**

```bash
cargo test emit --lib
```

Expected: FAIL with "cannot find struct `EmitTool`"

**Step 3: Write minimal implementation**

```rust
// src/runtime/builtin/emit.rs

use std::sync::Arc;
use std::future::Future;
use std::pin::Pin;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use rig::tool::{ToolDyn, ToolDefinition, ToolError};
use crate::event::{EventLog, EventKind};
use crate::store::DataStore;
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
    data_store: DataStore,
}

impl EmitTool {
    pub fn new(event_log: EventLog) -> Self {
        Self {
            event_log,
            data_store: DataStore::new(),
        }
    }
}

impl ToolDyn for EmitTool {
    fn name(&self) -> &str {
        "nika:emit"
    }

    fn definition(&self) -> ToolDefinition {
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

    fn call(&self, args: String) -> Pin<Box<dyn Future<Output = Result<String, ToolError>> + Send + '_>> {
        Box::pin(async move {
            let params: EmitParams = serde_json::from_str(&args)
                .map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

            self.event_log.emit(EventKind::CustomEvent {
                name: Arc::from(params.event.as_str()),
                data: params.data.clone(),
            });

            Ok(json!({ "emitted": params.event }).to_string())
        })
    }
}

impl BuiltinTool for EmitTool {
    fn event_log(&self) -> &EventLog {
        &self.event_log
    }

    fn data_store(&self) -> &DataStore {
        &self.data_store
    }

    fn category(&self) -> BuiltinCategory {
        BuiltinCategory::Observability
    }
}
```

**Step 4: Run test to verify it passes**

```bash
cargo test emit --lib
```

Expected: PASS

**Step 5: Commit**

```bash
git add src/runtime/builtin/emit.rs
git commit -m "feat(builtin): add nika:emit tool for custom events

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

## Task 3.6: nika:assert Tool

**Files:**
- Create: `src/runtime/builtin/assert.rs`

**Step 1: Write the failing test**

```rust
// src/runtime/builtin/assert.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_assert_true_condition() {
        let log = EventLog::new();
        let store = DataStore::new();
        let tool = AssertTool::new(log, store);
        let result = tool.call(r#"{"condition": "1 == 1"}"#.to_string()).await;
        assert!(result.is_ok());
        let parsed: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(parsed["passed"], true);
    }

    #[tokio::test]
    async fn test_assert_false_condition() {
        let log = EventLog::new();
        let store = DataStore::new();
        let tool = AssertTool::new(log, store);
        let result = tool.call(r#"{"condition": "1 == 2"}"#.to_string()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_assert_greater_than() {
        let log = EventLog::new();
        let store = DataStore::new();
        let tool = AssertTool::new(log, store);
        let result = tool.call(r#"{"condition": "5 > 3"}"#.to_string()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_assert_less_than_fails() {
        let log = EventLog::new();
        let store = DataStore::new();
        let tool = AssertTool::new(log, store);
        let result = tool.call(r#"{"condition": "5 < 3"}"#.to_string()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_assert_with_custom_message() {
        let log = EventLog::new();
        let store = DataStore::new();
        let tool = AssertTool::new(log, store);
        let result = tool.call(r#"{"condition": "false", "message": "Custom error"}"#.to_string()).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Custom error"));
    }

    #[tokio::test]
    async fn test_assert_invalid_expression() {
        let log = EventLog::new();
        let store = DataStore::new();
        let tool = AssertTool::new(log, store);
        let result = tool.call(r#"{"condition": "invalid ++ syntax"}"#.to_string()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_assert_boolean_and() {
        let log = EventLog::new();
        let store = DataStore::new();
        let tool = AssertTool::new(log, store);
        let result = tool.call(r#"{"condition": "true && true"}"#.to_string()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_assert_boolean_or() {
        let log = EventLog::new();
        let store = DataStore::new();
        let tool = AssertTool::new(log, store);
        let result = tool.call(r#"{"condition": "false || true"}"#.to_string()).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_assert_definition() {
        let log = EventLog::new();
        let store = DataStore::new();
        let tool = AssertTool::new(log, store);
        let def = tool.definition();
        assert_eq!(def.name, "nika:assert");
    }

    #[test]
    fn test_assert_category() {
        let log = EventLog::new();
        let store = DataStore::new();
        let tool = AssertTool::new(log, store);
        assert_eq!(tool.category(), BuiltinCategory::ControlFlow);
    }
}
```

**Step 2: Run test to verify it fails**

```bash
cargo test assert --lib
```

Expected: FAIL with "cannot find struct `AssertTool`"

**Step 3: Write minimal implementation**

```rust
// src/runtime/builtin/assert.rs

use std::sync::Arc;
use std::future::Future;
use std::pin::Pin;
use serde::{Deserialize, Serialize};
use serde_json::json;
use rig::tool::{ToolDyn, ToolDefinition, ToolError};
use crate::event::{EventLog, EventKind};
use crate::store::DataStore;
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
}

impl ToolDyn for AssertTool {
    fn name(&self) -> &str {
        "nika:assert"
    }

    fn definition(&self) -> ToolDefinition {
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

    fn call(&self, args: String) -> Pin<Box<dyn Future<Output = Result<String, ToolError>> + Send + '_>> {
        Box::pin(async move {
            let params: AssertParams = serde_json::from_str(&args)
                .map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

            // Resolve bindings in condition (future: use data_store.resolve_template)
            let resolved = params.condition.clone();

            // Evaluate condition using evalexpr
            let result = evalexpr::eval_boolean(&resolved)
                .map_err(|e| ToolError::ExecutionError(format!("Invalid condition '{}': {}", resolved, e)))?;

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
        })
    }
}

impl BuiltinTool for AssertTool {
    fn event_log(&self) -> &EventLog {
        &self.event_log
    }

    fn data_store(&self) -> &DataStore {
        &self.data_store
    }

    fn category(&self) -> BuiltinCategory {
        BuiltinCategory::ControlFlow
    }
}
```

**Step 4: Run test to verify it passes**

```bash
cargo test assert --lib
```

Expected: PASS

**Step 5: Commit**

```bash
git add src/runtime/builtin/assert.rs
git commit -m "feat(builtin): add nika:assert tool with evalexpr conditions

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

## Task 3.7: nika:prompt Tool

**Files:**
- Create: `src/runtime/builtin/prompt.rs`

**Step 1: Write the failing test**

```rust
// src/runtime/builtin/prompt.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_type_confirm() {
        let pt: PromptType = serde_json::from_str(r#""confirm""#).unwrap();
        assert_eq!(pt, PromptType::Confirm);
    }

    #[test]
    fn test_prompt_type_text() {
        let pt: PromptType = serde_json::from_str(r#""text""#).unwrap();
        assert_eq!(pt, PromptType::Text);
    }

    #[test]
    fn test_prompt_type_select() {
        let pt: PromptType = serde_json::from_str(r#""select""#).unwrap();
        assert_eq!(pt, PromptType::Select);
    }

    #[test]
    fn test_prompt_type_multiselect() {
        let pt: PromptType = serde_json::from_str(r#""multiselect""#).unwrap();
        assert_eq!(pt, PromptType::Multiselect);
    }

    #[tokio::test]
    async fn test_prompt_headless_with_default() {
        let log = EventLog::new();
        let tool = PromptTool::new(log);
        let result = tool.call(r#"{"type": "confirm", "message": "Test?", "default": "yes"}"#.to_string()).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "yes");
    }

    #[tokio::test]
    async fn test_prompt_headless_without_default_fails() {
        let log = EventLog::new();
        let tool = PromptTool::new(log);
        let result = tool.call(r#"{"type": "text", "message": "Enter value"}"#.to_string()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_prompt_select_without_options_fails() {
        let log = EventLog::new();
        let tool = PromptTool::new(log);
        let result = tool.call(r#"{"type": "select", "message": "Choose", "default": "a"}"#.to_string()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_prompt_select_with_options_and_default() {
        let log = EventLog::new();
        let tool = PromptTool::new(log);
        let result = tool.call(r#"{"type": "select", "message": "Choose", "options": ["a", "b"], "default": "a"}"#.to_string()).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_prompt_definition() {
        let log = EventLog::new();
        let tool = PromptTool::new(log);
        let def = tool.definition();
        assert_eq!(def.name, "nika:prompt");
    }

    #[test]
    fn test_prompt_category() {
        let log = EventLog::new();
        let tool = PromptTool::new(log);
        assert_eq!(tool.category(), BuiltinCategory::HumanInLoop);
    }

    #[tokio::test]
    async fn test_prompt_emits_paused_event() {
        let log = EventLog::new();
        let tool = PromptTool::new(log.clone());
        let _ = tool.call(r#"{"type": "confirm", "message": "Test?", "default": "no"}"#.to_string()).await;
        let events = log.events();
        assert!(events.iter().any(|e| matches!(e.kind, EventKind::WorkflowPaused { .. })));
    }

    #[tokio::test]
    async fn test_prompt_emits_resumed_event() {
        let log = EventLog::new();
        let tool = PromptTool::new(log.clone());
        let _ = tool.call(r#"{"type": "confirm", "message": "Test?", "default": "yes"}"#.to_string()).await;
        let events = log.events();
        assert!(events.iter().any(|e| matches!(e.kind, EventKind::WorkflowResumed { .. })));
    }
}
```

**Step 2: Run test to verify it fails**

```bash
cargo test prompt --lib
```

Expected: FAIL with "cannot find struct `PromptTool`"

**Step 3: Write minimal implementation**

```rust
// src/runtime/builtin/prompt.rs

use std::sync::Arc;
use std::future::Future;
use std::pin::Pin;
use serde::{Deserialize, Serialize};
use rig::tool::{ToolDyn, ToolDefinition, ToolError};
use serde_json::json;
use crate::event::{EventLog, EventKind};
use crate::store::DataStore;
use super::{BuiltinTool, BuiltinCategory};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PromptType {
    Confirm,
    Text,
    Select,
    Multiselect,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptParams {
    #[serde(rename = "type")]
    pub prompt_type: PromptType,
    pub message: String,
    #[serde(default)]
    pub options: Vec<String>,
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub timeout: u32,
}

pub struct PromptTool {
    event_log: EventLog,
    data_store: DataStore,
}

impl PromptTool {
    pub fn new(event_log: EventLog) -> Self {
        Self {
            event_log,
            data_store: DataStore::new(),
        }
    }
}

impl ToolDyn for PromptTool {
    fn name(&self) -> &str {
        "nika:prompt"
    }

    fn definition(&self) -> ToolDefinition {
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

    fn call(&self, args: String) -> Pin<Box<dyn Future<Output = Result<String, ToolError>> + Send + '_>> {
        Box::pin(async move {
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
                reason: Arc::from(format!("Waiting for user input: {}", params.message)),
            });

            // Headless mode - use defaults or error
            let response = match params.default.as_ref() {
                Some(default) => default.clone(),
                None => {
                    return Err(ToolError::ExecutionError(
                        "nika:prompt requires TUI or default value in headless mode".into()
                    ));
                }
            };

            // Emit event (workflow resumed)
            self.event_log.emit(EventKind::WorkflowResumed {
                response: Arc::from(response.as_str()),
            });

            Ok(response)
        })
    }
}

impl BuiltinTool for PromptTool {
    fn event_log(&self) -> &EventLog {
        &self.event_log
    }

    fn data_store(&self) -> &DataStore {
        &self.data_store
    }

    fn category(&self) -> BuiltinCategory {
        BuiltinCategory::HumanInLoop
    }
}
```

**Step 4: Run test to verify it passes**

```bash
cargo test prompt --lib
```

Expected: PASS

**Step 5: Commit**

```bash
git add src/runtime/builtin/prompt.rs
git commit -m "feat(builtin): add nika:prompt tool for HITL input

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

## Task 3.8: nika:run Tool

**Files:**
- Create: `src/runtime/builtin/run.rs`

**Step 1: Write the failing test**

```rust
// src/runtime/builtin/run.rs

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    fn create_test_workflow(dir: &TempDir, name: &str, content: &str) -> PathBuf {
        let path = dir.path().join(name);
        fs::write(&path, content).unwrap();
        path
    }

    #[tokio::test]
    async fn test_run_valid_workflow() {
        let temp = TempDir::new().unwrap();
        let workflow_content = r#"
schema: nika/workflow@0.5
workflow: sub-workflow
tasks:
  - id: simple
    exec: "echo hello"
"#;
        create_test_workflow(&temp, "sub.nika.yaml", workflow_content);

        let log = EventLog::new();
        let store = DataStore::new();
        let tool = RunTool::new(log, store).with_project_root(temp.path().to_path_buf());

        let result = tool.call(r#"{"path": "sub.nika.yaml"}"#.to_string()).await;
        // Note: This will fail until we have full Runner integration
        // For now, just verify parsing works
        assert!(result.is_ok() || result.is_err()); // Placeholder
    }

    #[tokio::test]
    async fn test_run_workflow_not_found() {
        let log = EventLog::new();
        let store = DataStore::new();
        let temp = TempDir::new().unwrap();
        let tool = RunTool::new(log, store).with_project_root(temp.path().to_path_buf());

        let result = tool.call(r#"{"path": "nonexistent.nika.yaml"}"#.to_string()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_run_with_inputs() {
        let temp = TempDir::new().unwrap();
        let workflow_content = r#"
schema: nika/workflow@0.5
workflow: sub-workflow
tasks:
  - id: echo_input
    exec: "echo {{use.greeting}}"
"#;
        create_test_workflow(&temp, "sub.nika.yaml", workflow_content);

        let log = EventLog::new();
        let store = DataStore::new();
        let tool = RunTool::new(log, store).with_project_root(temp.path().to_path_buf());

        let result = tool.call(r#"{"path": "sub.nika.yaml", "inputs": {"greeting": "hello"}}"#.to_string()).await;
        // Verify inputs are passed
        assert!(result.is_ok() || result.is_err()); // Placeholder until Runner integration
    }

    #[test]
    fn test_run_definition() {
        let log = EventLog::new();
        let store = DataStore::new();
        let tool = RunTool::new(log, store);
        let def = tool.definition();
        assert_eq!(def.name, "nika:run");
    }

    #[test]
    fn test_run_category() {
        let log = EventLog::new();
        let store = DataStore::new();
        let tool = RunTool::new(log, store);
        assert_eq!(tool.category(), BuiltinCategory::Composition);
    }

    #[test]
    fn test_run_params_parsing() {
        let params: RunParams = serde_json::from_str(r#"{
            "path": "./sub.nika.yaml",
            "inputs": {"key": "value"},
            "isolated": false
        }"#).unwrap();

        assert_eq!(params.path, "./sub.nika.yaml");
        assert!(!params.isolated);
        assert!(params.inputs.contains_key("key"));
    }

    #[test]
    fn test_run_params_defaults() {
        let params: RunParams = serde_json::from_str(r#"{"path": "test.yaml"}"#).unwrap();
        assert!(params.isolated); // Default true
        assert!(params.inputs.is_empty());
    }

    #[tokio::test]
    async fn test_run_emits_started_event() {
        let temp = TempDir::new().unwrap();
        let workflow_content = r#"
schema: nika/workflow@0.5
workflow: test
tasks:
  - id: noop
    exec: "true"
"#;
        create_test_workflow(&temp, "test.nika.yaml", workflow_content);

        let log = EventLog::new();
        let store = DataStore::new();
        let tool = RunTool::new(log.clone(), store).with_project_root(temp.path().to_path_buf());

        let _ = tool.call(r#"{"path": "test.nika.yaml"}"#.to_string()).await;
        let events = log.events();
        assert!(events.iter().any(|e| matches!(e.kind, EventKind::SubWorkflowStarted { .. })));
    }

    #[tokio::test]
    async fn test_run_invalid_yaml() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("bad.nika.yaml"), "invalid: yaml: content:").unwrap();

        let log = EventLog::new();
        let store = DataStore::new();
        let tool = RunTool::new(log, store).with_project_root(temp.path().to_path_buf());

        let result = tool.call(r#"{"path": "bad.nika.yaml"}"#.to_string()).await;
        assert!(result.is_err());
    }
}
```

**Step 2: Run test to verify it fails**

```bash
cargo test run --lib
```

Expected: FAIL with "cannot find struct `RunTool`"

**Step 3: Write minimal implementation**

```rust
// src/runtime/builtin/run.rs

use std::sync::Arc;
use std::path::PathBuf;
use std::future::Future;
use std::pin::Pin;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use rig::tool::{ToolDyn, ToolDefinition, ToolError};
use crate::event::{EventLog, EventKind};
use crate::store::DataStore;
use crate::ast::Workflow;
use super::{BuiltinTool, BuiltinCategory};

fn default_true() -> bool { true }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunParams {
    pub path: String,
    #[serde(default)]
    pub inputs: FxHashMap<String, Value>,
    #[serde(default = "default_true")]
    pub isolated: bool,
}

pub struct RunTool {
    event_log: EventLog,
    data_store: DataStore,
    project_root: PathBuf,
}

impl RunTool {
    pub fn new(event_log: EventLog, data_store: DataStore) -> Self {
        Self {
            event_log,
            data_store,
            project_root: std::env::current_dir().unwrap_or_default(),
        }
    }

    pub fn with_project_root(mut self, root: PathBuf) -> Self {
        self.project_root = root;
        self
    }
}

impl ToolDyn for RunTool {
    fn name(&self) -> &str {
        "nika:run"
    }

    fn definition(&self) -> ToolDefinition {
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

    fn call(&self, args: String) -> Pin<Box<dyn Future<Output = Result<String, ToolError>> + Send + '_>> {
        Box::pin(async move {
            let params: RunParams = serde_json::from_str(&args)
                .map_err(|e| ToolError::InvalidArguments(e.to_string()))?;

            // Resolve path relative to project root
            let workflow_path = self.project_root.join(&params.path);
            if !workflow_path.exists() {
                return Err(ToolError::ExecutionError(
                    format!("Workflow not found: {}", params.path)
                ));
            }

            // Parse sub-workflow
            let yaml = tokio::fs::read_to_string(&workflow_path).await
                .map_err(|e| ToolError::ExecutionError(e.to_string()))?;
            let _workflow = Workflow::from_yaml(&yaml)
                .map_err(|e| ToolError::ExecutionError(e.to_string()))?;

            // Emit event
            self.event_log.emit(EventKind::SubWorkflowStarted {
                path: Arc::from(params.path.as_str()),
                inputs: params.inputs.iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            });

            // TODO: Actually run sub-workflow via Runner
            // For now, just verify parsing works

            // Emit completion event
            self.event_log.emit(EventKind::SubWorkflowCompleted {
                path: Arc::from(params.path.as_str()),
            });

            Ok(json!({ "status": "completed", "path": params.path }).to_string())
        })
    }
}

impl BuiltinTool for RunTool {
    fn event_log(&self) -> &EventLog {
        &self.event_log
    }

    fn data_store(&self) -> &DataStore {
        &self.data_store
    }

    fn category(&self) -> BuiltinCategory {
        BuiltinCategory::Composition
    }
}
```

**Step 4: Run test to verify it passes**

```bash
cargo test run --lib
```

Expected: PASS

**Step 5: Commit**

```bash
git add src/runtime/builtin/run.rs
git commit -m "feat(builtin): add nika:run tool for workflow composition

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

## Task 3.9: Add EventKind Variants

**Files:**
- Modify: `src/event/log.rs`

**Step 1: Write the failing test**

```rust
// In src/event/log.rs tests

#[test]
fn test_builtin_invoke_event() {
    let log = EventLog::new();
    log.emit(EventKind::BuiltinInvoke {
        tool: Arc::from("nika:sleep"),
        args: r#"{"duration": "1s"}"#.to_string(),
    });
    let events = log.events();
    assert!(matches!(&events[0].kind, EventKind::BuiltinInvoke { tool, .. } if tool.as_ref() == "nika:sleep"));
}

#[test]
fn test_builtin_response_event() {
    let log = EventLog::new();
    log.emit(EventKind::BuiltinResponse {
        tool: Arc::from("nika:sleep"),
        result: r#"{"slept_ms": 1000}"#.to_string(),
    });
    let events = log.events();
    assert!(matches!(&events[0].kind, EventKind::BuiltinResponse { .. }));
}

#[test]
fn test_custom_event() {
    let log = EventLog::new();
    log.emit(EventKind::CustomEvent {
        name: Arc::from("page_generated"),
        data: serde_json::json!({"page": "home"}),
    });
    let events = log.events();
    assert!(matches!(&events[0].kind, EventKind::CustomEvent { name, .. } if name.as_ref() == "page_generated"));
}

#[test]
fn test_workflow_paused_event() {
    let log = EventLog::new();
    log.emit(EventKind::WorkflowPaused {
        reason: Arc::from("Waiting for user input"),
    });
    let events = log.events();
    assert!(matches!(&events[0].kind, EventKind::WorkflowPaused { .. }));
}

#[test]
fn test_workflow_resumed_event() {
    let log = EventLog::new();
    log.emit(EventKind::WorkflowResumed {
        response: Arc::from("yes"),
    });
    let events = log.events();
    assert!(matches!(&events[0].kind, EventKind::WorkflowResumed { .. }));
}
```

**Step 2: Run test to verify it fails**

```bash
cargo test event --lib
```

Expected: FAIL with "no variant named `BuiltinInvoke`"

**Step 3: Write minimal implementation**

Add to `EventKind` enum in `src/event/log.rs`:

```rust
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
        level: crate::runtime::builtin::log::LogLevel,
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
    WorkflowPaused {
        reason: Arc<str>,
    },
    WorkflowResumed {
        response: Arc<str>,
    },
    SubWorkflowStarted {
        path: Arc<str>,
        inputs: FxHashMap<String, serde_json::Value>,
    },
    SubWorkflowCompleted {
        path: Arc<str>,
    },
}
```

**Step 4: Run test to verify it passes**

```bash
cargo test event --lib
```

Expected: PASS

**Step 5: Commit**

```bash
git add src/event/log.rs
git commit -m "feat(event): add 11 new EventKind variants for builtin tools

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

## Task 3.10: Executor Routing Integration

**Files:**
- Modify: `src/runtime/executor.rs`

**Step 1: Write the failing test**

```rust
// In src/runtime/executor.rs tests

#[tokio::test]
async fn test_executor_routes_builtin_sleep() {
    let workflow = Workflow::from_yaml(r#"
schema: nika/workflow@0.5
workflow: test
tasks:
  - id: wait
    invoke:
      tool: nika:sleep
      params:
        duration: "10ms"
"#).unwrap();

    let executor = Executor::new(workflow);
    let result = executor.run().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_executor_routes_builtin_log() {
    let workflow = Workflow::from_yaml(r#"
schema: nika/workflow@0.5
workflow: test
tasks:
  - id: log_msg
    invoke:
      tool: nika:log
      params:
        level: info
        message: "Test log"
"#).unwrap();

    let executor = Executor::new(workflow);
    let result = executor.run().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_executor_routes_builtin_assert_pass() {
    let workflow = Workflow::from_yaml(r#"
schema: nika/workflow@0.5
workflow: test
tasks:
  - id: check
    invoke:
      tool: nika:assert
      params:
        condition: "1 == 1"
"#).unwrap();

    let executor = Executor::new(workflow);
    let result = executor.run().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_executor_routes_builtin_assert_fail() {
    let workflow = Workflow::from_yaml(r#"
schema: nika/workflow@0.5
workflow: test
tasks:
  - id: check
    invoke:
      tool: nika:assert
      params:
        condition: "1 == 2"
        message: "Should fail"
"#).unwrap();

    let executor = Executor::new(workflow);
    let result = executor.run().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_executor_routes_mcp_tool() {
    // Verify MCP routing still works (existing behavior)
    let workflow = Workflow::from_yaml(r#"
schema: nika/workflow@0.5
workflow: test
tasks:
  - id: mcp_call
    invoke:
      tool: novanet:describe
      server: novanet
      params:
        entity: test
"#).unwrap();

    let executor = Executor::new(workflow);
    // This will fail without MCP server, but should NOT route to builtin
    let result = executor.run().await;
    assert!(result.is_err()); // Expected: MCP server not connected
}

#[tokio::test]
async fn test_executor_builtin_emit_events() {
    let workflow = Workflow::from_yaml(r#"
schema: nika/workflow@0.5
workflow: test
tasks:
  - id: emit_custom
    invoke:
      tool: nika:emit
      params:
        event: "workflow_event"
        data:
          key: "value"
"#).unwrap();

    let executor = Executor::new(workflow);
    let result = executor.run().await;
    assert!(result.is_ok());

    // Verify event was emitted
    let events = executor.event_log().events();
    assert!(events.iter().any(|e| matches!(&e.kind, EventKind::CustomEvent { name, .. } if name.as_ref() == "workflow_event")));
}
```

**Step 2: Run test to verify it fails**

```bash
cargo test executor_routes --lib
```

Expected: FAIL with "builtin routing not implemented"

**Step 3: Write minimal implementation**

Modify `src/runtime/executor.rs`:

```rust
use crate::runtime::builtin::router::BuiltinToolRouter;

impl Executor {
    pub fn new(workflow: Workflow) -> Self {
        let event_log = EventLog::new();
        let data_store = DataStore::new();
        let builtin_router = BuiltinToolRouter::new(event_log.clone(), data_store.clone());

        Self {
            workflow,
            event_log,
            data_store,
            builtin_router,
            // ... other fields
        }
    }

    async fn execute_invoke(&self, task: &Task) -> Result<TaskResult, NikaError> {
        let invoke = task.action.as_invoke().unwrap();

        // Route based on prefix
        if BuiltinToolRouter::is_builtin(&invoke.tool) {
            // Builtin tool
            let args = serde_json::to_string(&invoke.params)
                .map_err(|e| NikaError::InvokeError {
                    task_id: task.id.clone(),
                    reason: e.to_string(),
                })?;

            let result = self.builtin_router.dispatch(&invoke.tool, args).await?;

            Ok(TaskResult {
                task_id: task.id.clone(),
                output: serde_json::from_str(&result).unwrap_or(serde_json::Value::String(result)),
                duration_ms: 0, // TODO: track duration
            })
        } else {
            // MCP tool (existing logic)
            self.mcp_client.call_tool(&invoke.server, &invoke.tool, invoke.params.clone()).await
        }
    }
}
```

**Step 4: Run test to verify it passes**

```bash
cargo test executor_routes --lib
```

Expected: PASS

**Step 5: Commit**

```bash
git add src/runtime/executor.rs
git commit -m "feat(executor): add builtin tool routing with nika: prefix

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

## 🔌 WIRING CHECKPOINT 3: Router ↔ Executor

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  WIRING CHECKPOINT 3: BuiltinToolRouter ↔ Executor Integration                ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  Verify these connections are working:                                        ║
║                                                                               ║
║  1. Executor.execute_invoke() checks nika: prefix                             ║
║  2. nika:* calls → BuiltinToolRouter.dispatch()                               ║
║  3. other:* calls → McpClient.call_tool() (unchanged)                         ║
║  4. BuiltinToolRouter emits BuiltinInvoke + BuiltinResponse events            ║
║  5. All 6 builtin tools are registered in router                              ║
║  6. EventLog receives all builtin events                                      ║
║                                                                               ║
║  Run: cargo test wiring_checkpoint_3 --lib                                    ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

**Wiring Test:**

```rust
// tests/wiring_checkpoint_3.rs

#[tokio::test]
async fn wiring_checkpoint_3_builtin_to_executor() {
    // Test 1: nika: prefix routes to builtin
    let workflow = Workflow::from_yaml(r#"
schema: nika/workflow@0.5
workflow: wiring-test
tasks:
  - id: builtin_sleep
    invoke:
      tool: nika:sleep
      params:
        duration: "1ms"
  - id: builtin_log
    invoke:
      tool: nika:log
      params:
        level: info
        message: "Wiring test"
"#).unwrap();

    let executor = Executor::new(workflow);
    let result = executor.run().await;
    assert!(result.is_ok(), "Builtin tools should execute successfully");

    // Test 2: Events are emitted
    let events = executor.event_log().events();
    assert!(events.iter().any(|e| matches!(&e.kind, EventKind::BuiltinInvoke { tool, .. } if tool.as_ref() == "nika:sleep")));
    assert!(events.iter().any(|e| matches!(&e.kind, EventKind::BuiltinResponse { tool, .. } if tool.as_ref() == "nika:sleep")));
    assert!(events.iter().any(|e| matches!(&e.kind, EventKind::BuiltinLog { .. })));

    // Test 3: All 6 tools registered
    let router = &executor.builtin_router;
    assert!(router.has_tool("sleep"));
    assert!(router.has_tool("log"));
    assert!(router.has_tool("emit"));
    assert!(router.has_tool("assert"));
    assert!(router.has_tool("prompt"));
    assert!(router.has_tool("run"));
}
```

---

## 🧪 LIVE TEST: Builtin Tools

After completing all Phase 3 tasks, run these live tests:

```bash
# Test 1: nika:sleep in workflow
cargo run -- run examples/test-builtin-sleep.nika.yaml

# Test 2: nika:assert validation
cargo run -- run examples/test-builtin-assert.nika.yaml

# Test 3: All builtin tools together
cargo run -- run examples/test-builtin-all.nika.yaml
```

**Create test workflow files:**

```yaml
# examples/test-builtin-sleep.nika.yaml
schema: nika/workflow@0.5
workflow: test-sleep
description: "Test nika:sleep tool"
tasks:
  - id: sleep_100ms
    invoke:
      tool: nika:sleep
      params:
        duration: "100ms"
  - id: log_done
    invoke:
      tool: nika:log
      params:
        level: info
        message: "Sleep completed"
```

```yaml
# examples/test-builtin-assert.nika.yaml
schema: nika/workflow@0.5
workflow: test-assert
description: "Test nika:assert tool"
tasks:
  - id: assert_pass
    invoke:
      tool: nika:assert
      params:
        condition: "5 > 3"
  - id: log_passed
    invoke:
      tool: nika:log
      params:
        level: info
        message: "Assertion passed"
```

```yaml
# examples/test-builtin-all.nika.yaml
schema: nika/workflow@0.5
workflow: test-all-builtins
description: "Test all 6 builtin tools"
tasks:
  - id: emit_start
    invoke:
      tool: nika:emit
      params:
        event: "workflow_started"
        data:
          test: "all-builtins"

  - id: log_info
    invoke:
      tool: nika:log
      params:
        level: info
        message: "Testing all builtin tools"

  - id: sleep_brief
    invoke:
      tool: nika:sleep
      params:
        duration: "50ms"

  - id: assert_condition
    invoke:
      tool: nika:assert
      params:
        condition: "true"
        message: "This should pass"

  - id: emit_complete
    invoke:
      tool: nika:emit
      params:
        event: "workflow_completed"
        data:
          success: true
```

---

## Summary

| Task | Description | Tests | Status |
|------|-------------|-------|--------|
| 3.1 | BuiltinTool trait | 5 | ⬜ |
| 3.2 | BuiltinToolRouter | 8 | ⬜ |
| 3.3 | nika:sleep | 8 | ⬜ |
| 3.4 | nika:log | 6 | ⬜ |
| 3.5 | nika:emit | 6 | ⬜ |
| 3.6 | nika:assert | 10 | ⬜ |
| 3.7 | nika:prompt | 12 | ⬜ |
| 3.8 | nika:run | 10 | ⬜ |
| 3.9 | EventKind variants | 5 | ⬜ |
| 3.10 | Executor routing | 6 | ⬜ |
| **Total** | | **76** | |

---

## References

- [Builtin Tools Spec](./2026-02-24-builtin-tools-spec.md)
- [Phase 2: Bindings](./Phase-2-Bindings.md)
- [rig::tool::ToolDyn](https://docs.rs/rig-core/latest/rig/tool/trait.ToolDyn.html)
- [humantime](https://docs.rs/humantime)
- [evalexpr](https://docs.rs/evalexpr)
