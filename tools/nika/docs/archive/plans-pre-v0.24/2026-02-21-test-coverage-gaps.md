# Test Coverage Gaps Implementation Plan

**Date:** 2026-02-21
**Status:** Active
**Priority:** High - Production hardening

---

## Overview

This plan addresses 5 critical test coverage gaps identified by the 10-agent audit.
Each gap represents error paths that are currently untested in production code.

---

## Gap 1: INFER Verb Error Paths

**File:** `src/runtime/executor.rs` (L460-517, `run_infer()`)
**Current Coverage:** 0 error cases tested

### Missing Tests

| Test Case | Error Type | Priority |
|-----------|------------|----------|
| Provider not configured | `ProviderNotConfigured` | HIGH |
| Missing API key | `MissingApiKey` | HIGH |
| LLM API failure (rate limit) | `ProviderApiError` | HIGH |
| Empty/null response | `EmptyResponse` | MEDIUM |
| Template resolution failure | `TemplateError` | MEDIUM |
| Invalid model name | `InvalidModel` | LOW |

### Implementation

```rust
// tests/executor_infer_errors_test.rs

#[tokio::test]
async fn test_infer_missing_api_key() {
    // Clear env vars, expect MissingApiKey error
}

#[tokio::test]
async fn test_infer_empty_response() {
    // Mock provider returns empty, expect EmptyResponse
}

#[tokio::test]
async fn test_infer_template_resolution_failure() {
    // Use {{use.missing}} in prompt, expect TemplateError
}
```

---

## Gap 2: FETCH Verb Timeout Scenarios

**File:** `src/runtime/executor.rs` (L562-610, `run_fetch()`)
**Current Coverage:** Happy-path only with external httpbin.org

### Missing Tests

| Test Case | Error Type | Priority |
|-----------|------------|----------|
| HTTP timeout | `FetchTimeout` | HIGH |
| SSL/TLS error | `TlsError` | HIGH |
| Invalid URL format | `InvalidUrl` | HIGH |
| DNS resolution failure | `DnsError` | MEDIUM |
| Non-2xx HTTP status | `HttpError` | MEDIUM |
| Malformed JSON response | `JsonParseError` | MEDIUM |
| Connection refused | `ConnectionError` | LOW |

### Implementation

```rust
// tests/executor_fetch_errors_test.rs

#[tokio::test]
async fn test_fetch_timeout() {
    // Use mock server with delay > timeout
}

#[tokio::test]
async fn test_fetch_invalid_url() {
    // Use "not-a-url" as URL
}

#[tokio::test]
async fn test_fetch_non_2xx_status() {
    // Mock server returns 500
}

#[tokio::test]
async fn test_fetch_malformed_json() {
    // Mock server returns invalid JSON
}
```

---

## Gap 3: MCP Client Race Conditions

**File:** `src/mcp/client.rs` (1476 lines)
**Current Coverage:** Basic sequential + 2 concurrent tests

### Missing Tests

| Test Case | Scenario | Priority |
|-----------|----------|----------|
| Concurrent OnceCell init | Two tasks init same client | HIGH |
| Connection pool exhaustion | Max concurrent calls | HIGH |
| Server disconnect mid-request | Connection drops | MEDIUM |
| Schema cache invalidation | Update during calls | MEDIUM |
| Rate limiting | Too many requests | MEDIUM |

### Implementation

```rust
// tests/mcp_race_conditions_test.rs

#[tokio::test]
async fn test_concurrent_client_init() {
    // Spawn 10 tasks that all try to get_or_init same client
    // Verify only ONE client is created
}

#[tokio::test]
async fn test_concurrent_calls_same_tool() {
    // 20 concurrent calls to same tool
    // Verify all complete successfully
}

#[tokio::test]
async fn test_for_each_parallel_mcp_calls() {
    // Simulate for_each with concurrency=5 calling MCP
    // Verify no race conditions
}
```

---

## Gap 4: RigAgentLoop Provider Selection

**File:** `src/runtime/rig_agent_loop.rs` (L461-505, `run_auto()`)
**Current Coverage:** 0 tests for provider selection

### Missing Tests

| Test Case | Scenario | Priority |
|-----------|----------|----------|
| No API keys available | Both ANTHROPIC/OPENAI unset | HIGH |
| ANTHROPIC_API_KEY only | Should use Claude | HIGH |
| OPENAI_API_KEY only | Should use OpenAI | HIGH |
| Both keys available | Should prefer Claude | MEDIUM |
| Invalid provider string | AgentParams.provider = "invalid" | MEDIUM |
| Rate limited during run | Mid-execution rate limit | LOW |

### Implementation

```rust
// tests/rig_provider_selection_test.rs

#[tokio::test]
async fn test_run_auto_no_keys() {
    // Unset all keys, expect clear error
}

#[tokio::test]
async fn test_run_auto_anthropic_only() {
    // Set only ANTHROPIC_API_KEY
    // Verify run_auto() uses Claude
}

#[tokio::test]
async fn test_run_auto_openai_only() {
    // Set only OPENAI_API_KEY
    // Verify run_auto() uses OpenAI
}

#[tokio::test]
async fn test_run_auto_both_keys_prefers_claude() {
    // Set both keys
    // Verify Claude is preferred
}
```

---

## Gap 5: Lazy Binding Edge Cases

**File:** `src/binding/` (1754 lines total)
**Current Coverage:** Basic lazy patterns, no edge cases

### Missing Tests

| Test Case | Scenario | Priority |
|-----------|----------|----------|
| Missing upstream task | Lazy refs non-existent task | HIGH |
| Circular dependency | A → B.lazy, B → A | HIGH |
| Deeply nested path | 10+ level JSON path | MEDIUM |
| Null in path | `task.data.null_field.value` | MEDIUM |
| Unicode in alias | `使用.中文` binding alias | LOW |
| Large JSON payload | 10MB binding value | LOW |

### Implementation

```rust
// tests/lazy_binding_edge_cases_test.rs

#[test]
fn test_lazy_missing_upstream_task() {
    // Reference task that doesn't exist
    // Verify clear error message
}

#[test]
fn test_lazy_circular_dependency_detection() {
    // Create circular lazy bindings
    // Verify cycle detected with helpful error
}

#[test]
fn test_lazy_deeply_nested_path() {
    // 20-level deep JSON path
    // Verify resolution works
}

#[test]
fn test_lazy_null_in_path() {
    // Path traverses null value
    // Verify sensible error or default
}
```

---

## Execution Plan

### Phase 1: Setup (Now)
- [ ] Create test files structure
- [ ] Add mock HTTP server for fetch tests
- [ ] Add helper functions for env var manipulation

### Phase 2: High Priority Tests
- [ ] Gap 1: 3 INFER error tests
- [ ] Gap 2: 4 FETCH timeout tests
- [ ] Gap 3: 3 MCP race tests
- [ ] Gap 4: 4 provider selection tests
- [ ] Gap 5: 2 lazy binding tests

### Phase 3: Medium Priority Tests
- [ ] Remaining INFER tests
- [ ] Remaining FETCH tests
- [ ] Schema cache tests
- [ ] Remaining lazy binding tests

### Phase 4: Validation
- [ ] Run full test suite
- [ ] Verify no regressions
- [ ] Update test count in CLAUDE.md

---

## Success Criteria

- All 5 gaps have at least HIGH priority tests implemented
- Zero test failures
- Test count increases by ~20 tests
- All error paths return NikaError (not panic)
