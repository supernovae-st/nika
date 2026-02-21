# Test Infrastructure Improvements Plan

**Date:** 2026-02-21
**Status:** In Progress
**Priority:** High - Developer Experience

---

## Overview

This plan addresses 4 improvements identified by the 5-agent audit to reduce code duplication, improve test isolation, and simplify architecture.

---

## Task 1: Extract Test Helpers (TDD)

**Goal:** Create `src/test_utils.rs` to consolidate ~200 lines of duplicated test setup code.

### Current State (Duplication)

```rust
// Found in 30+ test files:
let event_log = EventLog::new();
let exec = TaskExecutor::new("mock", None, None, event_log.clone());
exec.inject_mock_mcp_client("novanet");
```

### Target API

```rust
// src/test_utils.rs
pub mod builders {
    pub fn mock_executor() -> TaskExecutor;
    pub fn mock_executor_with_mcp(servers: &[&str]) -> TaskExecutor;
    pub fn test_context() -> (ResolvedBindings, DataStore);
}

pub mod fixtures {
    pub fn weather_data() -> Value;
    pub fn entity_context() -> Value;
}

pub mod assertions {
    pub fn assert_event_emitted(log: &EventLog, task_id: &str, kind: &str);
    pub fn assert_task_succeeded(log: &EventLog, task_id: &str);
}
```

### TDD Steps

1. Write failing test for `mock_executor()`
2. Implement minimal `mock_executor()`
3. Write failing test for `mock_executor_with_mcp()`
4. Implement
5. Migrate 3 high-traffic test files
6. Run full test suite

---

## Task 2: Add wiremock for HTTP Tests

**Goal:** Replace external httpbin.org calls with isolated mock server.

### Current State

```rust
// tests/executor_fetch_errors_test.rs - uses manual TcpListener
async fn start_status_server(status: u16, body: &str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    // ... manual HTTP response construction
}
```

### Target API (wiremock)

```rust
use wiremock::{MockServer, Mock, ResponseTemplate};
use wiremock::matchers::{method, path};

#[tokio::test]
async fn test_fetch_json_response() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/data"))
        .respond_with(ResponseTemplate::new(200)
            .set_body_json(json!({"key": "value"})))
        .mount(&mock_server)
        .await;

    // Test with mock_server.uri()
}
```

### TDD Steps

1. Add `wiremock = "0.6"` to dev-dependencies
2. Write failing test with wiremock
3. Refactor `start_status_server()` to use wiremock
4. Add request verification tests

---

## Task 3: Simplify OnceCell in MCP Client

**Goal:** Remove unnecessary `Arc` wrapping around `OnceCell`.

### Current State

```rust
// src/runtime/executor.rs:37
mcp_client_cache: Arc<DashMap<String, Arc<OnceCell<Arc<McpClient>>>>>,
```

### Target State

```rust
// Simplified - DashMap handles concurrency, no outer Arc needed
mcp_client_cache: DashMap<String, OnceCell<Arc<McpClient>>>,
```

### TDD Steps

1. Write test verifying concurrent client init still works
2. Change type signature
3. Update `get_mcp_client()` method
4. Run MCP race condition tests

---

## Task 4: Real Use Case Verification

**Goal:** Run actual workflow tests to verify improvements.

### Test Workflows

1. `examples/test-user-journey-api-etl.nika.yaml`
2. `examples/test-user-journey-blog-pipeline.nika.yaml`
3. `examples/test-user-journey-code-gen.nika.yaml`
4. `examples/test-user-journey-multilingual.nika.yaml`

### Verification Steps

1. Run `nika check` on all workflows
2. Run with `--dry-run` flag
3. Verify no regressions in existing tests

---

## Execution Order

| Task | Priority | Effort | Dependencies |
|------|----------|--------|--------------|
| 1. Test helpers | HIGH | 2h | None |
| 2. wiremock | MEDIUM | 1h | Task 1 |
| 3. OnceCell | MEDIUM | 30m | Task 1 |
| 4. Verification | HIGH | 30m | Tasks 1-3 |

---

## Success Criteria

- [ ] `src/test_utils.rs` created with 3 modules
- [ ] At least 5 test files migrated to use new helpers
- [ ] wiremock integrated for fetch tests
- [ ] OnceCell simplified (single Arc layer)
- [ ] All 1200+ tests pass
- [ ] User journey workflows validate
