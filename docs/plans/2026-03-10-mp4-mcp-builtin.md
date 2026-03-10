# MP4: MCP & Builtin Tools Fix

**Parent**: `2026-03-10-v0.24.0-bugfix-masterplan.md`
**Priority**: 🔴 HIGH
**Files**: `src/runtime/executor.rs`, `src/runtime/builtin/sleep.rs`, `src/mcp/rmcp_adapter.rs`
**Estimated**: 2-3 hours

---

## Problem Statement

Several bugs in the MCP and builtin tool systems:

### Bug 1: nika:sleep Has No Timeout Protection

**File**: `src/runtime/executor.rs` (around line 1180)

```rust
// CURRENT (NO TIMEOUT):
let result = builtin_router.dispatch(tool_name, params_json).await?;
// If tool_name = "nika:sleep" with duration="1000h", this blocks forever
```

**Impact**: An agent can call `nika:sleep` with an arbitrarily long duration, blocking the entire workflow forever.

### Bug 2: Multiple MCP Calls Can Exceed Task Deadline

Each MCP call has a 30-second timeout, but if a task makes N calls, total time = N × 30s, potentially exceeding any reasonable task deadline.

### Bug 3: MCP Error Code Extraction is Regex-Based

**File**: `src/mcp/rmcp_adapter.rs`

Error codes are extracted via regex from error messages, which is fragile if the format changes.

### Bug 4: MCP Reconnection Missing Timeout

When MCP connection is lost and reconnection is attempted, there's no timeout on the reconnection attempt.

---

## Solution: Bug 1 (nika:sleep Timeout)

### Implementation

**File**: `src/runtime/builtin/sleep.rs`

```rust
use std::time::Duration;
use tokio::time::timeout;

/// Maximum allowed sleep duration (5 minutes)
pub const MAX_SLEEP_DURATION: Duration = Duration::from_secs(5 * 60);

/// Builtin sleep tool with maximum duration protection
pub async fn execute_sleep(params: SleepParams) -> Result<Value, NikaError> {
    let duration = parse_duration(&params.duration)?;

    // Enforce maximum sleep duration
    if duration > MAX_SLEEP_DURATION {
        return Err(NikaError::BuiltinToolError {
            tool: "nika:sleep".to_string(),
            details: format!(
                "Sleep duration {} exceeds maximum allowed {} seconds",
                humantime::format_duration(duration),
                MAX_SLEEP_DURATION.as_secs()
            ),
        });
    }

    // Sleep with timeout (belt and suspenders)
    match timeout(MAX_SLEEP_DURATION, tokio::time::sleep(duration)).await {
        Ok(()) => Ok(serde_json::json!({
            "slept_for": humantime::format_duration(duration).to_string()
        })),
        Err(_) => Err(NikaError::BuiltinToolError {
            tool: "nika:sleep".to_string(),
            details: "Sleep timed out (internal error)".to_string(),
        }),
    }
}
```

**File**: `src/runtime/executor.rs`

Add timeout wrapper around ALL builtin tool dispatches:

```rust
const BUILTIN_TOOL_TIMEOUT: Duration = Duration::from_secs(5 * 60); // 5 minutes

async fn dispatch_builtin_tool(
    &self,
    tool_name: &str,
    params: Value,
) -> Result<Value, NikaError> {
    let params_json = serde_json::to_string(&params)?;

    // Wrap ALL builtin tool calls in timeout
    let result = timeout(
        BUILTIN_TOOL_TIMEOUT,
        self.builtin_router.dispatch(tool_name, params_json)
    ).await;

    match result {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(e)) => Err(e),
        Err(_elapsed) => Err(NikaError::BuiltinToolTimeout {
            tool: tool_name.to_string(),
            timeout_secs: BUILTIN_TOOL_TIMEOUT.as_secs(),
        }),
    }
}
```

Add new error variant:

**File**: `src/error.rs`

```rust
#[error("[NIKA-057] Builtin tool '{tool}' timed out after {timeout_secs} seconds")]
BuiltinToolTimeout {
    tool: String,
    timeout_secs: u64,
},
```

---

## Solution: Bug 2 (Task-Level MCP Deadline)

### Analysis

Individual MCP calls have timeouts, but there's no overall deadline for a task's total MCP usage.

### Implementation

**File**: `src/runtime/executor.rs`

Add task-level deadline tracking:

```rust
/// Maximum total time for all MCP calls in a single task
const TASK_MCP_DEADLINE: Duration = Duration::from_secs(5 * 60); // 5 minutes total

struct McpCallTracker {
    start_time: Instant,
    deadline: Duration,
    call_count: u32,
}

impl McpCallTracker {
    fn new(deadline: Duration) -> Self {
        Self {
            start_time: Instant::now(),
            deadline,
            call_count: 0,
        }
    }

    fn remaining_time(&self) -> Option<Duration> {
        let elapsed = self.start_time.elapsed();
        if elapsed >= self.deadline {
            None
        } else {
            Some(self.deadline - elapsed)
        }
    }

    fn check_deadline(&self) -> Result<(), NikaError> {
        if self.remaining_time().is_none() {
            Err(NikaError::TaskMcpDeadlineExceeded {
                call_count: self.call_count,
                deadline_secs: self.deadline.as_secs(),
            })
        } else {
            Ok(())
        }
    }
}

// In agent loop when calling MCP:
async fn call_mcp_tool(
    &self,
    tracker: &mut McpCallTracker,
    tool_name: &str,
    params: Value,
) -> Result<Value, NikaError> {
    // Check deadline before starting
    tracker.check_deadline()?;
    tracker.call_count += 1;

    // Use remaining time as this call's timeout
    let remaining = tracker.remaining_time()
        .ok_or_else(|| NikaError::TaskMcpDeadlineExceeded {
            call_count: tracker.call_count,
            deadline_secs: tracker.deadline.as_secs(),
        })?;

    // Call with bounded timeout
    let call_timeout = remaining.min(MCP_CALL_TIMEOUT);

    timeout(call_timeout, self.mcp_client.call_tool(tool_name, params))
        .await
        .map_err(|_| NikaError::Timeout {
            operation: format!("MCP tool call: {}", tool_name),
            timeout_ms: call_timeout.as_millis() as u64,
        })?
}
```

Add new error variant:

```rust
#[error("[NIKA-108] Task exceeded MCP deadline after {call_count} calls ({deadline_secs}s total)")]
TaskMcpDeadlineExceeded {
    call_count: u32,
    deadline_secs: u64,
},
```

---

## Solution: Bug 3 (MCP Error Code Extraction)

### Analysis

Currently using regex to extract error codes from error messages. Should use structured JSON-RPC error codes.

### Implementation

**File**: `src/mcp/rmcp_adapter.rs`

```rust
use rmcp::error::JsonRpcError;

/// Extract structured error code from MCP response
fn extract_mcp_error_code(error: &rmcp::Error) -> McpErrorCode {
    match error {
        rmcp::Error::JsonRpc(json_rpc_error) => {
            // Use the structured code field directly
            McpErrorCode::from_code(json_rpc_error.code)
        }
        rmcp::Error::Transport(e) => {
            McpErrorCode::TransportError(e.to_string())
        }
        rmcp::Error::Timeout => {
            McpErrorCode::Timeout
        }
        _ => McpErrorCode::Unknown(-1),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum McpErrorCode {
    ParseError,        // -32700
    InvalidRequest,    // -32600
    MethodNotFound,    // -32601
    InvalidParams,     // -32602
    InternalError,     // -32603
    ServerError(i32),  // -32000 to -32099
    TransportError(String),
    Timeout,
    Unknown(i32),
}

impl McpErrorCode {
    pub fn from_code(code: i32) -> Self {
        match code {
            -32700 => Self::ParseError,
            -32600 => Self::InvalidRequest,
            -32601 => Self::MethodNotFound,
            -32602 => Self::InvalidParams,
            -32603 => Self::InternalError,
            c if (-32099..=-32000).contains(&c) => Self::ServerError(c),
            c => Self::Unknown(c),
        }
    }

    pub fn is_retryable(&self) -> bool {
        matches!(self,
            McpErrorCode::InternalError |
            McpErrorCode::ServerError(_) |
            McpErrorCode::TransportError(_) |
            McpErrorCode::Timeout
        )
    }
}
```

---

## Solution: Bug 4 (MCP Reconnection Timeout)

### Implementation

**File**: `src/mcp/client.rs`

```rust
const MCP_RECONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const MCP_RECONNECT_MAX_ATTEMPTS: u32 = 3;

async fn reconnect(&mut self) -> Result<(), NikaError> {
    let mut attempts = 0;

    while attempts < MCP_RECONNECT_MAX_ATTEMPTS {
        attempts += 1;

        tracing::info!(
            server = %self.server_name,
            attempt = attempts,
            "Attempting MCP server reconnection"
        );

        match timeout(MCP_RECONNECT_TIMEOUT, self.connect_internal()).await {
            Ok(Ok(())) => {
                tracing::info!(
                    server = %self.server_name,
                    "MCP server reconnected successfully"
                );
                return Ok(());
            }
            Ok(Err(e)) => {
                tracing::warn!(
                    server = %self.server_name,
                    error = %e,
                    attempt = attempts,
                    "Reconnection attempt failed"
                );
            }
            Err(_) => {
                tracing::warn!(
                    server = %self.server_name,
                    attempt = attempts,
                    "Reconnection attempt timed out"
                );
            }
        }

        // Exponential backoff
        if attempts < MCP_RECONNECT_MAX_ATTEMPTS {
            let backoff = Duration::from_secs(2u64.pow(attempts));
            tokio::time::sleep(backoff).await;
        }
    }

    Err(NikaError::McpReconnectFailed {
        server: self.server_name.clone(),
        attempts: MCP_RECONNECT_MAX_ATTEMPTS,
    })
}
```

Add new error variant:

```rust
#[error("[NIKA-109] MCP server '{server}' reconnection failed after {attempts} attempts")]
McpReconnectFailed {
    server: String,
    attempts: u32,
},
```

---

## Test Plan

### Bug 1 Tests (nika:sleep Timeout)

```rust
#[tokio::test]
async fn sleep_rejects_excessive_duration() {
    let params = SleepParams {
        duration: "1000h".to_string(),
    };

    let result = execute_sleep(params).await;

    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.to_string().contains("exceeds maximum"));
}

#[tokio::test]
async fn sleep_accepts_valid_duration() {
    let params = SleepParams {
        duration: "100ms".to_string(),
    };

    let result = execute_sleep(params).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn builtin_dispatch_has_timeout() {
    // This would hang forever without timeout
    let executor = create_test_executor();

    let start = Instant::now();
    let result = timeout(
        Duration::from_secs(1),
        executor.dispatch_builtin_tool("nika:sleep", json!({"duration": "1h"}))
    ).await;

    // Should fail quickly due to validation, not timeout
    assert!(result.is_ok()); // timeout didn't fire
    assert!(result.unwrap().is_err()); // validation rejected
}
```

### Bug 2 Tests (Task MCP Deadline)

```rust
#[tokio::test]
async fn task_mcp_deadline_enforced() {
    let tracker = McpCallTracker::new(Duration::from_secs(1));

    // Simulate 100 quick calls (should be fine)
    for _ in 0..100 {
        tracker.call_count += 1;
    }

    // After 1 second, deadline should be exceeded
    tokio::time::sleep(Duration::from_secs(2)).await;

    let result = tracker.check_deadline();
    assert!(result.is_err());
}
```

### Bug 3 Tests (Error Code Extraction)

```rust
#[test]
fn mcp_error_code_from_json_rpc() {
    let code = McpErrorCode::from_code(-32602);
    assert_eq!(code, McpErrorCode::InvalidParams);
    assert!(!code.is_retryable());

    let code = McpErrorCode::from_code(-32603);
    assert_eq!(code, McpErrorCode::InternalError);
    assert!(code.is_retryable());
}
```

### Bug 4 Tests (Reconnection Timeout)

```rust
#[tokio::test]
async fn mcp_reconnect_times_out() {
    let mut client = create_test_mcp_client();
    client.disconnect();

    let start = Instant::now();
    let result = client.reconnect().await;
    let elapsed = start.elapsed();

    assert!(result.is_err());
    // Should timeout, not hang forever
    assert!(elapsed < Duration::from_secs(120));
}
```

---

## Success Criteria

### Bug 1 (nika:sleep)

- [ ] Maximum sleep duration enforced (5 minutes)
- [ ] Clear error message for excessive duration
- [ ] All builtin tools wrapped in timeout

### Bug 2 (Task MCP Deadline)

- [ ] Per-task MCP deadline (5 minutes total)
- [ ] Remaining time used for subsequent calls
- [ ] Clear error when deadline exceeded

### Bug 3 (Error Code Extraction)

- [ ] Use structured JSON-RPC error codes
- [ ] No regex parsing of error messages
- [ ] `is_retryable()` method for error handling

### Bug 4 (Reconnection Timeout)

- [ ] 30-second timeout per reconnection attempt
- [ ] Maximum 3 reconnection attempts
- [ ] Exponential backoff between attempts

### Combined

- [ ] All existing MCP tests pass
- [ ] New timeout tests added
- [ ] No hanging in any failure scenario

---

## New Error Codes

| Code | Error | Description |
|------|-------|-------------|
| NIKA-057 | BuiltinToolTimeout | Builtin tool exceeded timeout |
| NIKA-108 | TaskMcpDeadlineExceeded | Task exceeded total MCP time budget |
| NIKA-109 | McpReconnectFailed | MCP reconnection failed after retries |

---

## Related Issues

- Audit finding: "nika:sleep has no timeout protection"
- Audit finding: "Multiple MCP calls can exceed task deadline"
- Audit finding: "MCP error code extraction is regex-based"
- Audit finding: "MCP reconnection missing timeout"
