# v0.51 Handoff — Split rig.rs (Task 4.1)

**Copy everything below this line into a new Claude Code session.**

---

```
## Context

You are continuing work on Nika, a semantic YAML workflow engine for AI tasks.
Schema: nika/workflow@0.12 | Workspace: tools/ (Cargo workspace with 10 crates)

### Current State (2026-03-28)
- **Branch:** main, pushed to origin
- **Tests:** 8595 passed, 0 failures, 0 clippy warnings
- **Version:** v0.50.0 (v0.51.0 in progress)
- **Session before this:** 10 commits fixing bugs, telemetry, validation warnings

### What needs to be done
**Task 4.1: Split rig.rs (3640 LOC) into focused modules**

This is a PURE REFACTOR — no behavior change, no new features, no bug fixes.
The only goal is code organization.

### THE PLAN
Read `docs/plans/2026-03-28-split-rig-rs-master-plan.md` — it has EVERYTHING:
- 7 phases with exact line numbers
- Dependency graph between modules
- Consumer map (who uses what)
- Risk checklist
- Commit strategy

### KEY FILES
- **Target:** `tools/nika-engine/src/provider/rig.rs` (3640 LOC → 5 files)
- **Re-export hub:** `tools/nika-engine/src/provider/mod.rs` (line 54: `pub use rig::{...}`)
- **Plan:** `docs/plans/2026-03-28-split-rig-rs-master-plan.md`
- **Tests:** `cargo test --workspace --lib` (8595 tests, ALWAYS use --lib)

---

## YOUR MISSION: Execute the 7-phase split

### Phase 1: Rename rig.rs → rig/mod.rs
```bash
mkdir -p tools/nika-engine/src/provider/rig
mv tools/nika-engine/src/provider/rig.rs tools/nika-engine/src/provider/rig/mod.rs
```
Compile. Commit.

### Phase 2: Extract error.rs
Move from mod.rs:
- Lines 46-120: McpToolError, McpToolErrorKind (struct + enum + impls)
- Lines 1202-1215: RigInferError enum

Add to mod.rs:
```rust
mod error;
pub use error::{McpToolError, McpToolErrorKind, RigInferError};
```

### Phase 3: Extract stream.rs
Move from mod.rs:
- Lines 1217-1332: StreamChunk enum (all ~30 variants)
- Lines 1334-1432: StreamResult struct + consume_rig_stream fn

Add to mod.rs:
```rust
mod stream;
pub use stream::{StreamChunk, StreamResult};
use stream::consume_rig_stream;
```

stream.rs needs: `use super::error::RigInferError;`
consume_rig_stream is `pub(super)` — only used by mod.rs.

### Phase 4: Extract tool.rs
Move from mod.rs:
- Lines 1935-2176: NikaMcpToolDef, AgentMediaStaging, NikaMcpTool, ToolDyn impl

Add to mod.rs:
```rust
mod tool;
pub use tool::{AgentMediaStaging, NikaMcpTool, NikaMcpToolDef};
```

tool.rs needs: `use super::error::McpToolError;`

### Phase 5: Extract tests.rs
Move from mod.rs:
- Lines 2177-3640: entire `#[cfg(test)] mod tests { ... }`

In mod.rs replace with:
```rust
#[cfg(test)]
#[path = "tests.rs"]
mod tests;
```

### Phase 6: Clean up mod.rs imports
Remove now-unused imports. Add `use` for types from submodules.

### Phase 7: Update docs
Add module doc comment to each new file.

---

## CRITICAL RULES

1. **Compile after EVERY phase**: `cd tools && cargo check -p nika-engine`
2. **Test after phases 2-5**: `cd tools && cargo test -p nika-engine --lib -- provider::rig`
3. **1 phase = 1 commit** (7 total)
4. **Re-exports MUST be preserved** — `provider/mod.rs` line 54 must still work
5. **No behavior change** — exact same public API after split
6. **Line numbers are APPROXIMATE** — they're from the session when the plan was written. Always READ the current file to find the actual boundaries.
7. **Dependency DAG**: error ← stream, error ← tool, all ← mod.rs (NO circular deps)

---

## AFTER THE SPLIT — Verification

```bash
# Full test suite
cd tools && cargo test --workspace --lib
# Expected: 8595 passed, 0 failed

# Clippy
cd tools && cargo clippy --workspace -- -D warnings
# Expected: 0 warnings

# Line count verification
wc -l tools/nika-engine/src/provider/rig/*.rs
# Expected total: ~3640 (same as original)

# Push
git push
```

---

## COMMIT FORMAT

Each commit:
```
refactor(provider): <description>

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

---

## SECTION BOUNDARIES (line ranges in original rig.rs)

| Section | Lines | Target File | Key Types |
|---------|-------|-------------|-----------|
| Imports | 1-45 | mod.rs (stays) | use statements |
| Tool errors | 46-120 | error.rs | McpToolError, McpToolErrorKind |
| InferOptions + helpers | 121-188 | mod.rs (stays) | InferOptions, is_reasoning_model() |
| RigProvider enum + impls | 189-1200 | mod.rs (stays) | RigProvider, infer(), verify() |
| RigInferError | 1202-1215 | error.rs | RigInferError |
| StreamChunk | 1217-1332 | stream.rs | StreamChunk (30 variants) |
| StreamResult + consumer | 1334-1432 | stream.rs | StreamResult, consume_rig_stream |
| infer_stream methods | 1434-1934 | mod.rs (stays) | impl RigProvider (streaming) |
| NikaMcpTool | 1935-2176 | tool.rs | NikaMcpToolDef, NikaMcpTool, ToolDyn |
| Tests | 2177-3640 | tests.rs | all test functions |

---

## START

1. Read `docs/plans/2026-03-28-split-rig-rs-master-plan.md`
2. Run `cd tools && cargo test --workspace --lib` to confirm baseline (8595)
3. Execute Phase 1 (directory rename)
4. Execute Phases 2-7 sequentially, committing after each
5. Final verification + push

Go.
```
