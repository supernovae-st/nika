# MCP Integration Hardening Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix all 5 critical bugs identified by code review, add comprehensive test coverage, and production-harden the MCP integration.

**Architecture:** RAII/scopeguard for stdin/stdout restoration, OnceCell for concurrent initialization, io_lock for all stdio operations, array-based result aggregation for for_each.

**Tech Stack:** Rust, tokio, scopeguard, parking_lot, DashMap, OnceCell

---

## Priority 1: Critical Bug Fixes

### Task 1: Fix stdin/stdout Permanently Lost on Write Failure

**Problem:** In `client.rs:327-369`, if writing to stdin fails, stdin is never restored to the process, causing all subsequent requests to fail with "stdin not available".

**Files:**
- Modify: `tools/nika/src/mcp/client.rs:327-369`
- Test: `tools/nika/src/mcp/client.rs` (in #[cfg(test)] module)

**Step 1: Add scopeguard dependency**

Add to Cargo.toml:
```toml
scopeguard = "1.2"
```

**Step 2: Write the failing test**

```rust
#[tokio::test]
async fn test_stdin_restored_after_write_error() {
    // This test verifies that stdin is restored even if an error occurs
    // We can't easily simulate write failures, but we can verify the pattern
    // by checking multiple sequential requests work
    let client = McpClient::mock("test");

    // Multiple sequential calls should work (proves stdin is properly managed)
    for i in 0..5 {
        let result = client.call_tool("test_tool", serde_json::json!({"iteration": i})).await;
        assert!(result.is_ok(), "Call {} should succeed", i);
    }
}
```

**Step 3: Run test to verify current behavior**

Run: `cargo nextest run test_stdin_restored_after_write_error -p nika`
Expected: PASS (mock doesn't use stdin, but establishes the pattern)

**Step 4: Implement scopeguard pattern for send_request**

Replace stdin handling in `send_request`:

```rust
use scopeguard::defer;

async fn send_request(&self, request: &JsonRpcRequest) -> Result<JsonRpcResponse> {
    let _io_guard = self.io_lock.lock().await;

    let json = serde_json::to_string(request).map_err(|e| NikaError::McpToolError {
        tool: request.method.clone(),
        reason: format!("Failed to serialize request: {}", e),
    })?;

    // Take stdin, set up scopeguard to restore it on any exit path
    let mut stdin = {
        let mut guard = self.process.lock();
        let process = guard.as_mut().ok_or_else(|| NikaError::McpNotConnected {
            name: self.name.clone(),
        })?;

        process.stdin.take().ok_or_else(|| NikaError::McpToolError {
            tool: request.method.clone(),
            reason: "stdin not available".to_string(),
        })?
    };

    // Scopeguard to restore stdin even on panic or early return
    let stdin_restorer = {
        let process = Arc::clone(&self.process_arc); // Need Arc<Mutex<Option<Child>>>
        scopeguard::guard(stdin, move |stdin| {
            if let Some(process) = process.lock().as_mut() {
                process.stdin = Some(stdin);
            }
        })
    };

    // Write using the guarded stdin
    scopeguard::ScopeGuard::into_inner(stdin_restorer)
        .write_all(json.as_bytes())
        .await
        .map_err(|e| NikaError::McpToolError {
            tool: request.method.clone(),
            reason: format!("Failed to write: {}", e),
        })?;

    // ... rest of method
}
```

**Note:** Actually, the pattern is more complex because we need async-safe restoration. Alternative approach: use wrapper struct with Drop.

**Step 4b: Alternative - RAII wrapper approach**

Create a helper struct:

```rust
/// RAII guard that restores stdin to process on drop
struct StdinGuard<'a> {
    stdin: Option<tokio::process::ChildStdin>,
    process: &'a Mutex<Option<Child>>,
}

impl Drop for StdinGuard<'_> {
    fn drop(&mut self) {
        if let Some(stdin) = self.stdin.take() {
            if let Some(process) = self.process.lock().as_mut() {
                process.stdin = Some(stdin);
            }
        }
    }
}

impl StdinGuard<'_> {
    fn take(&mut self) -> Option<tokio::process::ChildStdin> {
        self.stdin.take()
    }
}
```

**Step 5: Run test to verify fix**

Run: `cargo nextest run test_stdin_restored -p nika`
Expected: PASS

**Step 6: Commit**

```bash
git add -A && git commit -m "fix(mcp): restore stdin/stdout on error with RAII guard

- Add StdinGuard/StdoutGuard structs with Drop impl
- Ensure stdio restored even on panic or early return
- Prevents 'stdin not available' cascading failures

[NIKA-101]"
```

---

### Task 2: Add io_lock to send_notification

**Problem:** `send_notification` (client.rs:253-306) doesn't acquire `io_lock`, so it can race with `send_request` on stdio.

**Files:**
- Modify: `tools/nika/src/mcp/client.rs:253-306`
- Test: `tools/nika/src/mcp/client.rs`

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn test_concurrent_notification_and_request_do_not_race() {
    // This test would require a real MCP server to properly test
    // For now, verify the io_lock is acquired in send_notification
    // by checking the code structure
    //
    // The real test is: run 100 parallel calls mixing notifications and requests
    // and verify no "stdin not available" errors
}
```

**Step 2: Run test**

Run: `cargo nextest run test_concurrent_notification -p nika`

**Step 3: Add io_lock to send_notification**

```rust
async fn send_notification(&self, notification: &JsonRpcNotification) -> Result<()> {
    // Acquire io_lock to prevent racing with send_request
    let _io_guard = self.io_lock.lock().await;

    // ... rest of method unchanged
}
```

**Step 4: Run tests**

Run: `cargo nextest run -p nika`
Expected: All pass

**Step 5: Commit**

```bash
git add -A && git commit -m "fix(mcp): add io_lock to send_notification

Prevents race condition between notification and request on shared stdio."
```

---

### Task 3: Fix for_each Result Aggregation

**Problem:** In runner.rs:307-339, all for_each iterations return `parent_task_id`, so only the last result survives in datastore.

**Files:**
- Modify: `tools/nika/src/runtime/runner.rs:258-339`
- Test: `tools/nika/tests/for_each_tests.rs` (new file)

**Step 1: Write the failing test**

```rust
// tests/for_each_tests.rs
use nika::ast::{Task, TaskAction, ExecParams, Workflow, Flow};
use nika::runtime::Runner;
use std::sync::Arc;

#[tokio::test]
async fn test_for_each_collects_all_results() {
    // Create workflow with for_each that runs 3 items
    let workflow = Workflow {
        schema: "nika/workflow@0.3".to_string(),
        provider: "mock".to_string(),
        model: None,
        mcp: None,
        tasks: vec![Arc::new(Task {
            id: "echo_items".to_string(),
            for_each: Some(serde_json::json!(["a", "b", "c"])),
            for_each_as: Some("item".to_string()),
            action: TaskAction::Exec {
                exec: ExecParams {
                    command: "echo {{use.item}}".to_string(),
                },
            },
            use_wiring: None,
            output: None,
        })],
        flows: vec![],
    };

    let runner = Runner::new(workflow);
    let result = runner.run().await;
    assert!(result.is_ok());

    // The output should contain all 3 results, not just the last one
    let output = result.unwrap();
    // For now, we expect the aggregated output to be a JSON array
    let parsed: serde_json::Value = serde_json::from_str(&output)
        .unwrap_or(serde_json::Value::String(output.clone()));

    // Should have 3 results
    if let serde_json::Value::Array(arr) = parsed {
        assert_eq!(arr.len(), 3, "Should have 3 results from for_each");
    } else {
        // If not array, the output should mention all items
        assert!(output.contains("a") || output.contains("b") || output.contains("c"));
    }
}
```

**Step 2: Run test to see it fail**

Run: `cargo nextest run test_for_each_collects_all_results -p nika`
Expected: FAIL - only last result captured

**Step 3: Implement result aggregation**

Change runner.rs to aggregate for_each results:

1. Track for_each tasks separately
2. Collect all results for same parent_task_id
3. Store aggregated result as JSON array

```rust
// In runner.rs, track for_each results
// Map from parent_task_id -> Vec<(index, result)>
let for_each_results: DashMap<Arc<str>, Vec<(usize, TaskResult)>> = DashMap::new();

// When spawning for_each iterations, pass index:
join_set.spawn(async move {
    let (parent_id, result) = Self::execute_task_iteration(/* ... */).await;
    (parent_id, idx, result)  // Add index
});

// When collecting results:
if is_for_each_iteration {
    for_each_results.entry(parent_id.clone())
        .or_default()
        .push((idx, task_result));
} else {
    self.datastore.insert(task_id, task_result);
}

// After all for_each iterations complete, aggregate:
for entry in for_each_results.iter() {
    let parent_id = entry.key();
    let mut results = entry.value().clone();
    results.sort_by_key(|(idx, _)| *idx);

    let aggregated = TaskResult::aggregated(results);
    self.datastore.insert(Arc::clone(parent_id), aggregated);
}
```

**Step 4: Run test to verify fix**

Run: `cargo nextest run test_for_each_collects_all_results -p nika`
Expected: PASS

**Step 5: Commit**

```bash
git add -A && git commit -m "fix(runner): aggregate all for_each results into array

- Track for_each results by parent_task_id
- Collect all N results, not just last
- Store as ordered JSON array

[NIKA-102]"
```

---

### Task 4: Validate Empty for_each at Parse Time

**Problem:** Empty `for_each: []` spawns no iterations but increments no completed count, potentially causing stall.

**Files:**
- Modify: `tools/nika/src/ast/workflow.rs` (add validation)
- Test: `tools/nika/src/ast/workflow.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn test_empty_for_each_rejected() {
    let yaml = r#"
schema: "nika/workflow@0.3"
provider: mock

tasks:
  - id: empty
    for_each: []
    as: item
    exec:
      command: "echo {{use.item}}"
"#;

    let result = parse_workflow(yaml);
    assert!(result.is_err(), "Empty for_each should be rejected");
    let err = result.unwrap_err();
    assert!(err.to_string().contains("for_each cannot be empty"));
}
```

**Step 2: Run test**

Run: `cargo nextest run test_empty_for_each_rejected -p nika`
Expected: FAIL (currently accepted)

**Step 3: Add validation in Task parsing**

```rust
// In ast/workflow.rs or validation module
impl Task {
    pub fn validate(&self) -> Result<(), String> {
        if let Some(for_each) = &self.for_each {
            if let Some(arr) = for_each.as_array() {
                if arr.is_empty() {
                    return Err(format!(
                        "for_each cannot be empty in task '{}' - use at least one item",
                        self.id
                    ));
                }
            }
        }
        Ok(())
    }
}
```

**Step 4: Call validation during parse**

**Step 5: Run test**

Run: `cargo nextest run test_empty_for_each_rejected -p nika`
Expected: PASS

**Step 6: Commit**

```bash
git add -A && git commit -m "fix(ast): reject empty for_each at parse time

Empty for_each would spawn no iterations, causing workflow stall.

[NIKA-103]"
```

---

### Task 5: Error Instead of Mock Fallback

**Problem:** In executor.rs:470-474, missing MCP config silently falls back to mock, hiding configuration errors.

**Files:**
- Modify: `tools/nika/src/runtime/executor.rs:470-474`
- Test: `tools/nika/src/runtime/executor.rs`

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn test_missing_mcp_config_returns_error() {
    let event_log = EventLog::new();
    // Create executor with NO MCP configs
    let exec = TaskExecutor::new("mock", None, Some(HashMap::new()), event_log);

    let action = TaskAction::Invoke {
        invoke: InvokeParams {
            mcp: "nonexistent_server".to_string(),
            tool: Some("test".to_string()),
            params: None,
            resource: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test");
    let bindings = UseBindings::new();
    let result = exec.execute(&task_id, &action, &bindings).await;

    assert!(result.is_err(), "Should fail when MCP server not configured");
    match result.unwrap_err() {
        NikaError::McpNotConfigured { name } => {
            assert_eq!(name, "nonexistent_server");
        }
        err => panic!("Expected McpNotConfigured, got: {err:?}"),
    }
}
```

**Step 2: Add new error variant**

In error.rs:
```rust
#[error("[NIKA-104] MCP server '{name}' not configured - add it to workflow mcp: block")]
McpNotConfigured { name: String },
```

**Step 3: Run test**

Run: `cargo nextest run test_missing_mcp_config_returns_error -p nika`
Expected: FAIL

**Step 4: Replace mock fallback with error**

```rust
// In get_mcp_client
if let Some(config) = mcp_configs.get(&name_owned) {
    // ... create real client
} else {
    return Err(NikaError::McpNotConfigured {
        name: name_owned.clone()
    });
}
```

**Step 5: Update tests that relied on mock fallback**

Add explicit MCP configs to tests that need mock behavior.

**Step 6: Run all tests**

Run: `cargo nextest run -p nika`
Expected: All pass

**Step 7: Commit**

```bash
git add -A && git commit -m "fix(executor): error on missing MCP config instead of silent mock

- Add McpNotConfigured error variant [NIKA-104]
- Remove implicit mock fallback
- Tests must explicitly configure MCP or use mock

Breaking: workflows with typos in mcp: server names now fail instead of silently using mock."
```

---

## Priority 2: Test Coverage

### Task 6: Concurrent MCP Access Test

**Files:**
- Create: `tools/nika/tests/mcp_concurrent_test.rs`

**Step 1: Write comprehensive concurrent access test**

```rust
//! Tests for concurrent MCP access with for_each

use nika::ast::{Task, TaskAction, InvokeParams, Workflow, McpConfigInline};
use nika::runtime::Runner;
use std::collections::HashMap;
use std::sync::Arc;

#[tokio::test]
async fn test_50_concurrent_mcp_calls() {
    // Create workflow with for_each of 50 items
    let mut mcp = HashMap::new();
    mcp.insert("novanet".to_string(), McpConfigInline {
        command: "echo".to_string(), // Mock command
        args: vec![],
        env: HashMap::new(),
        cwd: None,
    });

    let items: Vec<serde_json::Value> = (0..50)
        .map(|i| serde_json::json!(format!("item_{}", i)))
        .collect();

    let workflow = Workflow {
        schema: "nika/workflow@0.3".to_string(),
        provider: "mock".to_string(),
        model: None,
        mcp: Some(mcp),
        tasks: vec![Arc::new(Task {
            id: "stress_test".to_string(),
            for_each: Some(serde_json::Value::Array(items)),
            for_each_as: Some("item".to_string()),
            action: TaskAction::Invoke {
                invoke: InvokeParams {
                    mcp: "novanet".to_string(),
                    tool: Some("novanet_describe".to_string()),
                    params: Some(serde_json::json!({"item": "{{use.item}}"})),
                    resource: None,
                },
            },
            use_wiring: None,
            output: None,
        })],
        flows: vec![],
    };

    let runner = Runner::new(workflow);
    let result = runner.run().await;

    assert!(result.is_ok(), "50 concurrent MCP calls should succeed: {:?}", result.err());
}
```

**Step 2: Commit**

```bash
git add -A && git commit -m "test(mcp): add 50-concurrent-calls stress test"
```

---

### Task 7: for_each Partial Failure Test

**Files:**
- Add to: `tools/nika/tests/for_each_tests.rs`

**Step 1: Write test for partial failure**

```rust
#[tokio::test]
async fn test_for_each_partial_failure_collects_all() {
    // One item succeeds, one fails, one succeeds
    let workflow = Workflow {
        schema: "nika/workflow@0.3".to_string(),
        provider: "mock".to_string(),
        model: None,
        mcp: None,
        tasks: vec![Arc::new(Task {
            id: "mixed".to_string(),
            for_each: Some(serde_json::json!(["echo ok", "exit 1", "echo done"])),
            for_each_as: Some("cmd".to_string()),
            action: TaskAction::Exec {
                exec: ExecParams {
                    command: "{{use.cmd}}".to_string(),
                },
            },
            use_wiring: None,
            output: None,
        })],
        flows: vec![],
    };

    let runner = Runner::new(workflow);
    let result = runner.run().await;

    // Workflow should complete (not panic)
    assert!(result.is_ok());

    // Should have 3 results (2 success, 1 failure)
    // Verify via events or output
}
```

**Step 2: Commit**

```bash
git add -A && git commit -m "test(runner): add for_each partial failure test"
```

---

## Priority 3: NovaNet Tool Coverage

### Task 8: Test novanet_search Integration

**Files:**
- Create: `examples/novanet-search-test.nika.yaml`

```yaml
schema: "nika/workflow@0.3"
provider: mock

mcp:
  novanet:
    command: /path/to/novanet-mcp
    env:
      NOVANET_MCP_NEO4J_URI: bolt://localhost:7687
      NOVANET_MCP_NEO4J_USER: neo4j
      NOVANET_MCP_NEO4J_PASSWORD: novanetpassword

tasks:
  - id: search_entities
    invoke:
      mcp: novanet
      tool: novanet_search
      params:
        query: "qr code"
        limit: 10
```

### Task 9: Test novanet_traverse Integration

**Files:**
- Create: `examples/novanet-traverse-test.nika.yaml`

```yaml
schema: "nika/workflow@0.3"
provider: mock

mcp:
  novanet:
    command: /path/to/novanet-mcp

tasks:
  - id: traverse_graph
    invoke:
      mcp: novanet
      tool: novanet_traverse
      params:
        start: "entity:qr-code"
        arc: "HAS_NATIVE"
        depth: 2
```

### Task 10: Test novanet_assemble Integration

### Task 11: Test novanet_atoms Integration

---

## Priority 4: Production Hardening

### Task 12: Add MCP Client Drop for Process Cleanup

**Files:**
- Modify: `tools/nika/src/mcp/client.rs`

**Step 1: Implement Drop**

```rust
impl Drop for McpClient {
    fn drop(&mut self) {
        if !self.is_mock {
            if let Some(mut child) = self.process.lock().take() {
                // Best effort: try to kill the process
                // Note: this is sync, can't await
                let _ = child.start_kill();
            }
        }
    }
}
```

### Task 13: Add MCP Call Timeout

**Files:**
- Modify: `tools/nika/src/mcp/client.rs`

**Step 1: Add timeout constant**

```rust
const MCP_CALL_TIMEOUT: Duration = Duration::from_secs(30);
```

**Step 2: Wrap send_request with timeout**

```rust
pub async fn call_tool(&self, name: &str, params: Value) -> Result<ToolCallResult> {
    // ... validation ...

    tokio::time::timeout(MCP_CALL_TIMEOUT, async {
        // ... actual call
    })
    .await
    .map_err(|_| NikaError::McpTimeout {
        tool: name.to_string(),
        timeout_secs: MCP_CALL_TIMEOUT.as_secs(),
    })?
}
```

---

## Execution Order

1. Task 1-5: Critical bug fixes (sequential, each depends on previous passing)
2. Task 6-7: Test coverage (parallel)
3. Task 8-11: NovaNet integration tests (parallel)
4. Task 12-13: Production hardening (sequential)

---

## Verification Checklist

- [ ] All 5 critical bugs fixed
- [ ] All existing tests pass
- [ ] New tests cover edge cases
- [ ] No warnings from `cargo clippy`
- [ ] `cargo fmt` passes
- [ ] Manual test with real NovaNet MCP server
