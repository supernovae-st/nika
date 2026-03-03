# 3 Critical Fixes Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix 3 critical issues identified from the 10-agent ultrathink audit

**Architecture:** Direct code fixes in spn CLI and Nika runtime

**Tech Stack:** Rust, serde_yaml, reqwest

---

## Summary

| # | Issue | Location | Severity |
|---|-------|----------|----------|
| 1 | `spn info` path mangling | `supernovae-cli/src/index/types.rs` | Medium |
| 2 | `nika init` templates with obsolete `stop_conditions` | `tools/nika/src/main.rs` | High |
| 3 | `fetch:` retry not implemented | `tools/nika/src/runtime/executor.rs` | High |

---

## Issue #1: SPN Info Path Mangling (INVESTIGATION NEEDED)

### Analysis

After reviewing the code:
- `PackageScope::parse()` correctly splits `@workflows/data/json-transformer` into scope=`workflows`, path=`data/json-transformer`
- `index_path()` correctly maps `workflows` → `w`, resulting in `@w/data/json-transformer`
- Tests in `types.rs` confirm this behavior

**Hypothesis:** The bug may be in how URLs are constructed or encoded, not in path parsing.

### Potential Issues to Check

1. URL encoding of `@` character in GitHub raw URLs
2. File existence in the actual registry
3. HTTP response handling

**Status:** Marked for further investigation if issue persists after other fixes.

---

## Issue #2: nika init Templates - Remove stop_conditions

### Problem

Templates `WORKFLOW_03_AGENT` and `WORKFLOW_04_PRODUCTION` contain `stop_conditions` field which was removed in v0.16.3.

### Files to Modify

- `/Users/thibaut/dev/supernovae/nika/tools/nika/src/main.rs`

### Step 1: Remove stop_conditions from WORKFLOW_03_AGENT

**Location:** Lines 1607-1609

**Before:**
```yaml
    stop_conditions:
      - "Report generation complete"
      - "Research concluded"
```

**After:** Remove these 3 lines entirely.

### Step 2: Remove stop_conditions from WORKFLOW_04_PRODUCTION

**Location:** Lines 1806-1808

**Before:**
```yaml
    stop_conditions:
      - "All locales processed"
      - "Content generation complete"
```

**After:** Remove these 3 lines entirely.

### Step 3: Verify templates are valid

Run: `cargo test --lib -- --test-threads=1 2>&1 | grep -i init`

---

## Issue #3: fetch: retry Logic Implementation

### Problem

Documentation claims `fetch:` supports retry with exponential backoff, but the actual implementation has no retry logic.

### Files to Modify

1. `/Users/thibaut/dev/supernovae/nika/tools/nika/src/ast/action.rs` - Add retry field to FetchParams
2. `/Users/thibaut/dev/supernovae/nika/tools/nika/src/runtime/executor.rs` - Implement retry logic

### Step 1: Add RetryConfig struct and retry field to FetchParams

**File:** `src/ast/action.rs`

```rust
/// Configuration for retry behavior
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (default: 3)
    #[serde(default = "default_max_attempts")]
    pub max_attempts: u32,

    /// Initial backoff in milliseconds (default: 1000)
    #[serde(default = "default_backoff_ms")]
    pub backoff_ms: u64,

    /// Backoff multiplier (default: 2.0)
    #[serde(default = "default_multiplier")]
    pub multiplier: f64,
}

fn default_max_attempts() -> u32 { 3 }
fn default_backoff_ms() -> u64 { 1000 }
fn default_multiplier() -> f64 { 2.0 }

// Add to FetchParams:
pub struct FetchParams {
    // ... existing fields ...

    /// Optional retry configuration
    #[serde(default)]
    pub retry: Option<RetryConfig>,
}
```

### Step 2: Implement retry logic in run_fetch

**File:** `src/runtime/executor.rs`

```rust
// In run_fetch function, wrap the request in a retry loop:

let mut attempts = 0;
let max_attempts = params.retry.as_ref().map_or(1, |r| r.max_attempts);
let backoff_ms = params.retry.as_ref().map_or(1000, |r| r.backoff_ms);
let multiplier = params.retry.as_ref().map_or(2.0, |r| r.multiplier);

loop {
    attempts += 1;

    match request_builder.try_clone() {
        Some(req) => {
            match req.send().await {
                Ok(response) => {
                    // Handle success
                    break;
                }
                Err(e) if attempts < max_attempts && is_retryable(&e) => {
                    let delay = backoff_ms * (multiplier.powi((attempts - 1) as i32) as u64);
                    tokio::time::sleep(Duration::from_millis(delay)).await;
                    continue;
                }
                Err(e) => return Err(NikaError::Execution(...)),
            }
        }
        None => return Err(NikaError::Execution("Cannot clone request for retry")),
    }
}
```

### Step 3: Add tests for retry logic

```rust
#[tokio::test]
async fn test_fetch_retry_on_failure() {
    // Test that retry logic works with exponential backoff
}

#[tokio::test]
async fn test_fetch_no_retry_by_default() {
    // Test that fetch works without retry config
}
```

---

## Execution Order

1. Fix #2 first (simplest - just remove lines)
2. Fix #3 second (add retry logic)
3. Investigate #1 if time permits

## Verification

```bash
# After all fixes:
cd tools/nika
cargo test
cargo clippy -- -D warnings
cargo run -- check examples/03-agent-advanced.nika.yaml
```
