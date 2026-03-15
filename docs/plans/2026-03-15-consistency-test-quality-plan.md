# Consistency Cleanup + Test Quality Plan

**Date:** 2026-03-15
**Scope:** B (Consistency) + C (Test Quality) from Swarm Intelligence Report
**Status:** Ready for execution

---

## Overview

7 items total (B2 HashMap variants skipped — already well-architected).

## Execution Batches

### Batch 1 — Foundation (no dependencies)

| Item | Description | Risk | Files |
|------|-------------|------|-------|
| B1 | Normalize validate() → `Result<(), NikaError>` | Low | 7 AST files, 14 methods |
| B3 | Rename NikaConfig collision → BootstrapConfig | Low | boot.rs + callers |
| C2 | Add `#[serial]` to env var tests | Low | 8 test files, 42+ tests |

### Batch 2 — Structural (builds on batch 1)

| Item | Description | Risk | Files |
|------|-------------|------|-------|
| B4 | Introduce TaskId newtype at runtime boundaries | HIGH | 50+ files |
| C4 | Consolidate checkpoint test files | Low | 10 test files |

### Batch 3 — Quality (independent)

| Item | Description | Risk | Files |
|------|-------------|------|-------|
| C1 | Strengthen 130+ weak assertions | Low | 60+ test sections |
| C3 | Add missing unit tests to critical files | Medium | 6 priority files |

## Detailed Design

### B1: Normalize validate() Return Types

**Current:** 14 methods return `Result<(), String>`, 2 return `Result<(), NikaError>`, 1 returns `AnalyzeResult`.

**Target:** All → `Result<(), NikaError>` (AnalyzeResult stays for analyzer).

**Pattern:**
```rust
// Before
pub fn validate(&self) -> Result<(), String> {
    Err("message".to_string())
}

// After
pub fn validate(&self) -> Result<(), NikaError> {
    Err(NikaError::ValidationError { reason: "message".into() })
}
```

**Files:** action.rs, raw/workflow.rs, raw/mcp.rs, context.rs, include.rs, decompose.rs, output.rs

### B3: Fix NikaConfig Name Collision

- `src/config.rs` → stays `NikaConfig` (user-facing, canonical)
- `src/runtime/boot.rs` → rename to `BootstrapConfig`
- Update all callers

### B4: TaskId Newtype (HIGH RISK)

Extend existing `TaskId` from ast/analyzed with `Display`/`From<&str>` and propagate through:
- runtime/executor.rs
- runtime/runner.rs
- event/log.rs
- binding/resolve.rs

Strategy: Start at executor boundary, expand outward.

### C1: Strengthen Weak Assertions

Top files: security.rs, runner.rs, dag/validate.rs, binding/validate.rs

Pattern: `assert!(x.is_ok())` → `x.unwrap()` or `assert_eq!` on actual values.

### C2: Env Var Test Isolation

Add `serial_test` crate + `#[serial]` to env-mutating tests.
Critical: tui/chat_agent.rs (30+ tests), secrets/mod.rs, provider/rig.rs.

### C3: Missing Unit Tests

Priority 1: runtime/executor/verbs.rs, runtime/runner.rs, runtime/output.rs
Priority 2: core/providers.rs, core/models.rs, core/mcp_aliases.rs

### C4: Checkpoint Consolidation

Audit 10 wiring_checkpoint files, merge overlapping tests, reduce to 3-4 focused files.

---

## Success Criteria

- `cargo test` passes with same or higher count
- `cargo clippy -- -D warnings` clean
- Zero regressions
- Each batch = 1 commit group
