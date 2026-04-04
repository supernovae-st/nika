# Plan 3: Code Quality & Rust Idioms

**Date**: 2026-04-04 | **Version**: v0.68.0 (feature freeze)
**Priority**: MEDIUM — Improves maintainability and developer experience
**Source**: 7-agent mega audit (Rust Pro, Rust Architect, Async Expert)

---

## Metrics Snapshot

| Metric | Current | Target | Notes |
|--------|---------|--------|-------|
| `#[allow(dead_code)]` | 119 | < 20 | Zero dead code philosophy |
| `assert!(!x.is_empty())` | 335 | 0 | Replace with structural assertions |
| `unwrap()` in non-test engine | 2,583 | Audit needed | Many are in builders/parsers, but prod paths need `?` |
| `std::collections::HashMap` in engine | 62 | 0 (use FxHashMap) | Consistency with the 248 FxHashMap usages |
| Blocking I/O in async | 3 spots | 0 | `nika:inject`, artifact manifest, trace write |
| String-typed enums | 2+ | 0 | `working_dir_mode`, other config strings |

---

## QA-1: Eliminate Blocking I/O in Async Context

### Problem

Three locations use `std::fs` in async functions, blocking the Tokio runtime:

| Location | Operation | Risk |
|----------|-----------|------|
| `runtime/builtin/data/io.rs:94,132` | `nika:inject` reads/writes files | MEDIUM — user-controlled file size |
| `runtime/artifact_processor.rs:1064-1076` | manifest write at workflow end | LOW — small JSON, once per workflow |
| `runtime/runner.rs:675` → `event/trace.rs` | trace flush at workflow end | LOW — once per workflow |

### Fix for `nika:inject` (MEDIUM priority)

**File**: `tools/nika-engine/src/runtime/builtin/data/io.rs`

```rust
// Line 94 — Replace:
let template = std::fs::read_to_string(&template_path).map_err(|e| ...)?;

// With:
let template = tokio::fs::read_to_string(&template_path).await.map_err(|e| ...)?;

// Line 132 — Replace:
std::fs::write(&output_path, &output).map_err(|e| ...)?;

// With:
tokio::fs::write(&output_path, &output).await.map_err(|e| ...)?;
```

Also update `std::fs::create_dir_all` on line 69 to `tokio::fs::create_dir_all`.

### Fix for artifact manifest (LOW priority)

**File**: `tools/nika-engine/src/runtime/artifact_processor.rs:1064`

Wrap in `spawn_blocking` since it's called from the runner's async context:

```rust
pub async fn write_artifact_manifest(artifacts_dir: &Path, entries: Vec<ArtifactEntry>) {
    let dir = artifacts_dir.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let manifest = serde_json::json!({ "version": 1, "artifacts": entries });
        let manifest_path = dir.join("artifacts.json");
        if let Some(parent) = manifest_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(&manifest) {
            let _ = std::fs::write(&manifest_path, json);
        }
    })
    .await
    .ok();
}
```

### Verification

```bash
# Ensure no std::fs in async tool implementations:
rg "std::fs::" tools/nika-engine/src/runtime/builtin/ --glob '!*test*' -l
# Should only show files that use std::fs in synchronous helpers
```

---

## QA-2: `WorkingDirMode` Enum

### Problem

`runner.rs:504` stores `working_dir_mode: Option<String>` matched against string literals.

### Fix

**File**: `tools/nika-engine/src/runtime/runner/mod.rs` (or wherever Runner is defined)

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkingDirMode {
    #[default]
    Workflow,
    Project,
    None,
}
```

Update Runner struct:
```rust
working_dir_mode: WorkingDirMode,  // was: Option<String>
```

Update `with_working_dir_mode`:
```rust
pub fn with_working_dir_mode(mut self, mode: WorkingDirMode) -> Self {
    self.working_dir_mode = mode;
    self
}
```

Update all `self.working_dir_mode.as_deref() == Some("project")` matches:
```rust
match self.working_dir_mode {
    WorkingDirMode::Project => { ... }
    WorkingDirMode::Workflow => { ... }
    WorkingDirMode::None => { ... }
}
```

Search pattern: `rg "working_dir_mode" tools/nika-engine/src/`

---

## QA-3: FxHashMap Consistency in DAG

### Problem

`dag/flow.rs` uses `std::collections::HashMap` three times while every other DAG
function uses `FxHashMap`. Inconsistency + slightly slower for small key sets.

### Fix

**File**: `tools/nika-engine/src/dag/flow.rs`

```rust
// Line 35-40 — Replace:
) -> std::collections::HashMap<&'a str, usize> {
    let mut in_degree: std::collections::HashMap<&'a str, usize> = ...
    let mut successors: std::collections::HashMap<&'a str, Vec<&'a str>> = std::collections::HashMap::new();

// With:
use rustc_hash::FxHashMap;

) -> FxHashMap<&'a str, usize> {
    let mut in_degree: FxHashMap<&'a str, usize> = ...
    let mut successors: FxHashMap<&'a str, Vec<&'a str>> = FxHashMap::default();
```

Also line 50 and line 79 — update the return types.

---

## QA-4: Audit `#[allow(dead_code)]` (119 annotations)

### Philosophy

From memory: "Zero dead code philosophy" — `#[allow(dead_code)]` is acceptable ONLY for:
- RAII guards (fields kept for Drop behavior, e.g., `_flock`)
- `#[cfg(test)]` helpers
- FFI types that must match C layouts

### Top offenders

| File | Count | Action |
|------|-------|--------|
| `nika-lsp/src/position.rs` | 7 | Audit: if functions unused since v0.48, delete |
| `nika-lsp/src/ast_integration.rs` | 5 | Likely dead — LSP migration to lsp-core |
| `nika-sdk/src/types.rs` | 3 | Audit: SDK types for future use? |
| `nika-lsp/src/daemon_bridge.rs` | 3 | Audit: daemon integration stubs |
| `nika-engine/src/runtime/builtin/media/tests_*.rs` | 5 | Test helpers — OK if `#[cfg(test)]` |
| `nika-media/src/types.rs` | 2 | Audit: media type stubs |
| Other files | ~94 | Audit individually |

### Process

For each file:
1. Remove `#[allow(dead_code)]`
2. Run `cargo check` — does it produce a warning?
3. If warning: the code IS dead. Delete it.
4. If no warning: the annotation was unnecessary. Keep removed.
5. Exception: RAII fields → re-add with comment `// RAII: kept for Drop`

### Verification

```bash
# Count remaining after cleanup:
rg 'allow.*dead_code' tools/ --glob '*.rs' -c | awk -F: '{sum+=$2} END {print sum}'
# Target: < 20 (only legitimate RAII/FFI cases)
```

---

## QA-5: Replace `assert!(!x.is_empty())` in Tests (335 occurrences)

### Problem

335 test assertions use `assert!(!x.is_empty())` which provides zero information
about the actual content. CLAUDE.md explicitly calls this an anti-pattern.

### Strategy

**Not a bulk find-replace** — each assertion needs context-aware improvement.

### Categories of replacement

#### Category A: Replace with length check (when count matters)
```rust
// Before:
assert!(!results.is_empty());

// After:
assert!(results.len() >= 2, "expected at least 2 results, got {}", results.len());
```

#### Category B: Replace with structural check (when content matters)
```rust
// Before:
assert!(!output.is_empty());

// After:
assert!(output.contains("expected_keyword"), "output should contain expected_keyword: {output}");
```

#### Category C: Replace with specific field check (for JSON/structured data)
```rust
// Before:
assert!(!json_result.is_empty());

// After:
let parsed: serde_json::Value = serde_json::from_str(&json_result).unwrap();
assert!(parsed.get("name").is_some(), "result should have 'name' field");
```

### Execution order (highest impact first)

1. `nika-lsp-core/tests/` (43 occurrences) — LSP completion tests
2. `nika-engine/src/runtime/` (20+ occurrences) — runner/executor tests
3. `nika-engine/src/display/` (10+ occurrences) — display tests
4. `nika-engine/src/secrets/` (1 occurrence) — `test_load_result_summary_not_empty`
5. Remaining scattered across workspace

### Verification

```bash
rg 'assert!\(!.*is_empty' tools/ --glob '*.rs' -c | awk -F: '{sum+=$2} END {print sum}'
# Target: 0
```

---

## QA-6: `serde_saphyr` Alias Consolidation

### Problem

The alias `use serde_saphyr as serde_yaml` appears in 3 places independently.

### Fix

Since `nika-core` already has `pub use serde_saphyr as serde_yaml`, nika-engine should
re-use it:

**File**: `tools/nika-engine/src/lib.rs:29`

```rust
// Before:
use serde_saphyr as serde_yaml;

// After:
use nika_core::serde_yaml;
```

**File**: `tools/nika-mcp/src/types.rs:462` (test only — lower priority)

---

## QA-7: Replace Manual `as_str()`/`Display`/`parse()` with `strum`

### Problem

Enums like `GuardrailType`, `Severity`, `AgentTurnKind`, `JobStatus` manually implement
three methods that repeat the same variant-to-string mapping. New variants require touching
all three, risking divergence.

### Fix

Add `strum` to workspace deps:
```toml
strum = { version = "0.26", features = ["derive"] }
```

Replace manual impls:
```rust
#[derive(Debug, Clone, Copy, strum::Display, strum::EnumString)]
#[strum(serialize_all = "snake_case")]
pub enum GuardrailType {
    Length,
    Schema,
    Regex,
    Llm,
}
```

This auto-generates `Display`, `FromStr` (replaces `parse()`), and `as_ref()` (replaces `as_str()`).

### Files to update

Search: `rg "fn as_str.*->.*str" tools/nika-event/src/ tools/nika-engine/src/`

---

## QA-8: `format!("...").as_str()` → Direct `Arc::from`

### Problem

Pattern `Arc::from(format!("task_{i}").as_str())` round-trips through `&str` unnecessarily.

### Fix

```rust
// Before:
Arc::from(format!("task_{i}").as_str())

// After:
Arc::from(format!("task_{i}"))  // Arc::from(String) works directly
```

Search: `rg 'format!\(.*\.as_str\(\)' tools/ --glob '*.rs'`

---

## QA-9: Cookie Jar and Fetch Cache Dead Fields

### Problem

`TaskExecutor` carries two `#[allow(dead_code)]` fields that are allocated for every
task dispatch but never used:

```rust
cookie_jar: Arc<reqwest_cookie_store::CookieStoreRwLock>,
fetch_cache: Arc<crate::runtime::fetch_cache::FetchCache>,
```

### Fix

Since we're in feature freeze, gate behind a feature flag:

```rust
#[cfg(feature = "cookie-jar")]
cookie_jar: Arc<reqwest_cookie_store::CookieStoreRwLock>,
#[cfg(feature = "fetch-cache")]
fetch_cache: Arc<crate::runtime::fetch_cache::FetchCache>,
```

Or simply remove if these are genuinely planned-but-not-started features.

---

## QA-10: `unwrap()` Audit Strategy

### Scale

2,583 `unwrap()` calls in non-test engine code. Not all are problematic:
- Builder patterns: `unwrap()` on `Client::builder().build()` — OK
- Regex captures after `.captures()` — defensible but noisy
- Config parsing — should propagate errors

### Prioritized audit

**Priority 1: Template resolution** (18 unwraps in `binding/template.rs`)
- All on `cap.get(0).unwrap()` after regex captures
- Fix: Create helper `fn cap_zero(cap: &Captures) -> &str`

**Priority 2: DAG flow** (1 unwrap in `dag/flow.rs:66`)
- `in_degree.get_mut(succ).unwrap()` — panics if DAG is malformed
- Fix: Return `Err(DagError::MissingDependency)` instead

**Priority 3: AST lowering** (1 unwrap in `ast/lower.rs:931`)
- `items.as_str().unwrap()` during Value → String conversion
- Fix: Use `.as_str().unwrap_or_default()` or propagate error

**Priority 4: Bulk scan** — Lower priority, module by module

### Process per module

```bash
rg '\.unwrap\(\)' tools/nika-engine/src/binding/ --glob '!*test*' -n
# Review each: is it reachable with bad input? Can it panic?
# If yes: replace with ? or .expect("reason")
# If defensive (regex guarantee): .expect("regex guarantees match")
```

---

## Execution Order

```
Immediate (< 1 day):
├── QA-1  Fix nika:inject blocking I/O (30m)
├── QA-2  WorkingDirMode enum (30m)
├── QA-3  FxHashMap in dag/flow.rs (15m)
├── QA-6  serde_saphyr alias (10m)
├── QA-8  format!().as_str() cleanup (15m)
└── QA-9  Dead executor fields (15m)

Short term (1-2 days):
├── QA-7  strum derives (1h)
├── QA-10 unwrap() Priority 1-3 (2h)
└── QA-4  dead_code audit — nika-lsp first (2h)

Medium term (1 week):
├── QA-5  is_empty assertions (4h — spread across sessions)
├── QA-4  dead_code audit — remaining files (2h)
└── QA-10 unwrap() Priority 4 (ongoing)
```

## Verification Checklist

- [ ] `cargo test --workspace --lib` passes after each change
- [ ] `cargo clippy --workspace -- -D warnings` clean
- [ ] No new `#[allow(dead_code)]` without comment
- [ ] No new `unwrap()` in non-test code
- [ ] No `std::fs` in async builtin tools
- [ ] All `HashMap` in engine are `FxHashMap` (except public API boundaries)
