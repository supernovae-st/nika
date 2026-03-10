# Nika v0.5.2 Cleanup & DX Update Plan

**Date:** 2026-02-21
**Status:** In Progress
**Scope:** Dead code removal, documentation sync, dependency cleanup

---

## Summary of 10-Agent Verification

| Agent | Focus | Critical Issues |
|-------|-------|-----------------|
| 1 | TODOs | 2 low-priority items (tui-textarea, DAG height) |
| 2 | Mockups/Stubs | None - all intentional |
| 3 | Dead code (src/) | 6 questionable items to verify |
| 4 | Dead code (tests/) | None - 111 ignored tests have valid reasons |
| 5 | CLAUDE.md | Animation widgets don't exist, test count wrong |
| 6 | Test coverage | 5 critical gaps identified |
| 7 | API consistency | 2 naming issues |
| 8 | Dependencies | 3 unused deps to remove |
| 9 | supernovae-agi docs | Version/count mismatches |
| 10 | Skills/rules | ADR references Nika v0.4.1 |

---

## Phase 1: Documentation Fixes (HIGH PRIORITY)

### 1.1 Fix Nika CLAUDE.md
- [ ] Remove non-existent animation widgets (PulseText, ParticleBurst, ShakeText)
- [ ] Update test count from "1130+" to "1133 tests"
- [ ] Add all 17 test files to examples section (currently only 5)

### 1.2 Fix supernovae-agi Root Docs
- [ ] Update `.claude/rules/adr-quick-reference.md` line 1: v0.4.1 → v0.5.2
- [ ] Update `.claude/rules/adr-quick-reference.md` line 5: MVP 8 "next" → "complete"
- [ ] Update root CLAUDE.md test count: 1750+ → 1133

### 1.3 Fix Skills/Rules
- [ ] Update `.claude/skills/release.md` example version: v0.4.1 → v0.5.2

---

## Phase 2: Dependency Cleanup (MEDIUM PRIORITY)

### 2.1 Remove Unused Dependencies
```toml
# Remove from Cargo.toml [dependencies]
walkdir = "2.5"  # 0 usages

# Remove from [dev-dependencies]
tokio-test = "0.4"  # 0 usages
```

### 2.2 Keep But Document
- `tui-textarea` - blocked on ratatui 0.30 (keep, document status)
- `notify-rust` feature - check if desktop notifications planned

---

## Phase 3: API Consistency (LOW PRIORITY)

### 3.1 Naming Fixes
- [ ] Consider renaming `is_connected_async()` → `is_connected()` in McpClient
- [ ] Consider renaming `set_permission_mode()` → `with_permission_mode()` in ToolContext

Note: These are minor and may break API compatibility. Defer to v0.6.

---

## Phase 4: Future Work (NOT IN SCOPE)

### Test Coverage Gaps (v0.6+)
1. INFER verb error paths in executor.rs
2. FETCH verb timeout/error scenarios
3. MCP client race conditions
4. RigAgentLoop provider selection errors
5. Lazy binding edge cases

### Dead Code Audit Items (verify later)
- `dag/flow.rs` task_set field
- `dag/flow.rs` get_successors() method
- `runtime/rig_agent_loop.rs` mcp_clients field usage

---

## Execution Order

1. **Now:** Phase 1.1 (Nika CLAUDE.md fixes)
2. **Now:** Phase 1.2 (supernovae-agi docs)
3. **Now:** Phase 2.1 (Remove unused deps)
4. **Later:** Phase 3 (API consistency - v0.6)
5. **Later:** Phase 4 (Test coverage - v0.6+)

---

## Validation

After changes:
```bash
cargo build
cargo test --lib
cargo clippy --all-targets
```
