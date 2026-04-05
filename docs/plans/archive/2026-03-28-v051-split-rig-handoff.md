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
- **Version:** v0.50.0 (v0.51.0 in progress — 10 bug/telemetry commits already done)

### What needs to be done
**Task 4.1: Split rig.rs (3640 LOC) into 5 focused modules**

This is a PURE REFACTOR — no behavior change, no new features, no bug fixes.
The only goal is code organization. Every public API path must be preserved.

### THE PLAN
Read `docs/plans/2026-03-28-split-rig-rs-master-plan.md` — it has EVERYTHING:
- 6 phases with exact section boundaries
- Dependency DAG (error ← stream, error ← tool, all ← mod.rs)
- Consumer map from 4 research agents (StreamChunk = 13 external files!)
- Risk checklist (22 cfg gates, 5 local macros, re-export chain)
- Commit strategy (6 commits)

---

## YOUR MISSION: Execute the 6-phase split

### Phase 1: Convert to directory module (2 min)
```bash
mkdir -p tools/nika-engine/src/provider/rig
mv tools/nika-engine/src/provider/rig.rs tools/nika-engine/src/provider/rig/mod.rs
cd tools && cargo check -p nika-engine
```
This is a no-op rename. MUST compile unchanged.
Commit: `refactor(provider): convert rig.rs to directory module`

### Phase 2: Extract error.rs (~120 LOC)
Cut from mod.rs → new `rig/error.rs`:
- McpToolError struct + McpToolErrorKind enum + all impls (~lines 46-118)
- ProviderVerifyResult struct + ProviderVerifyError enum (~lines 1147-1200)
- RigInferError enum (~lines 1202-1215)

In mod.rs add:
```rust
pub mod error;
pub use error::{McpToolError, McpToolErrorKind, ProviderVerifyError, ProviderVerifyResult, RigInferError};
```

error.rs needs: `use std::fmt;`, `use std::time::Duration;`, `thiserror` — NO nika internal imports.

### Phase 3: Extract stream.rs (~215 LOC)
Cut from mod.rs → new `rig/stream.rs`:
- StreamChunk enum (30+ variants, ~lines 1217-1332)
- StreamResult struct + from_text() (~lines 1334-1365)
- consume_rig_stream() async fn (~lines 1367-1432)

In mod.rs add:
```rust
pub mod stream;
pub use stream::{StreamChunk, StreamResult};
use stream::consume_rig_stream;
```

stream.rs needs:
```rust
use super::error::RigInferError;
use crate::util::STREAM_CHUNK_TIMEOUT;
use futures::StreamExt;
use rig::completion::GetTokenUsage;
use rig::streaming::StreamedAssistantContent;
use std::time::Instant;
use tokio::sync::mpsc;
use tokio::time::timeout;
```

**CRITICAL:** `consume_rig_stream` must be `pub(super)` — only mod.rs calls it.

### Phase 4: Extract tool.rs (~245 LOC)
Cut from mod.rs → new `rig/tool.rs`:
- NikaMcpToolDef struct
- AgentMediaStaging type alias
- NikaMcpTool struct + all methods
- BoxFuture type alias
- impl ToolDyn for NikaMcpTool

In mod.rs add:
```rust
pub mod tool;
pub use tool::{AgentMediaStaging, NikaMcpTool, NikaMcpToolDef};
```

tool.rs needs:
```rust
use super::error::McpToolError;
use crate::mcp::McpClient;
use rig::completion::ToolDefinition;
use rig::tool::{ToolDyn, ToolError};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
```

### Phase 5: Extract tests.rs (~1460 LOC)
Cut ENTIRE `#[cfg(test)] mod tests { ... }` block (~lines 2177-3640).

In mod.rs replace with:
```rust
#[cfg(test)]
#[path = "tests.rs"]
mod tests;
```

Tests use `use super::*;` which resolves through mod.rs re-exports.

### Phase 6: Clean up mod.rs imports
Remove unused imports (std::pin::Pin, std::future::Future, rig::tool::*, dashmap).
Keep all rig provider imports, all crate internal imports.
Run full workspace: `cargo test --workspace --lib && cargo clippy --workspace -- -D warnings`

---

## CRITICAL RULES

1. **Compile after EVERY phase**: `cd tools && cargo check -p nika-engine`
2. **Test after phases 2-5**: `cd tools && cargo test -p nika-engine --lib -- provider::rig`
3. **1 phase = 1 commit** (6 total)
4. **Re-exports MUST be preserved**: `provider/mod.rs` line 54 still does `pub use rig::{NikaMcpTool, RigProvider, StreamResult};` — this resolves through rig/mod.rs re-exports
5. **No behavior change** — exact same public API
6. **Line numbers are APPROXIMATE** — always READ the file to find actual boundaries (look for section comment headers like `// ═══════`)
7. **Never skip compile check** — failures cascade and become much harder to diagnose

---

## CONSUMER MAP (who uses what — validated by research agent)

| Type | # External Files | Key Consumers |
|------|-----------------|---------------|
| `StreamChunk` | **13** | executor/infer.rs, rig_agent_loop/streaming.rs, 6 TUI files |
| `RigProvider` | **14** | executor/infer.rs, executor/mod.rs, nika-cli, nika-tui |
| `NikaMcpTool`/`Def`/`Staging` | **1** | rig_agent_loop/mod.rs |
| `InferOptions` | **1** | executor/infer.rs |
| `McpToolError` | **0** | Only used inside tool.rs ToolDyn impl |
| `ProviderVerify*` | **0** | Used indirectly via .verify(), never imported by name |
| `consume_rig_stream` | **0** | Private, only used by RigProvider methods in mod.rs |

All consumers use path `crate::provider::rig::X` → resolved by rig/mod.rs `pub use`.

---

## WHAT STAYS IN mod.rs (everything else)

- `InferOptions` struct + `is_reasoning_model()` + `build_response_format_params()` + `supports_native_structured_output()`
- `RigProvider` enum definition
- ALL `impl RigProvider` blocks (from_name, infer, infer_stream, verify, auto, etc.)
- `extract_native_vision_parts()` (cfg-gated)
- Re-export declarations

---

## DEPENDENCY DAG (confirmed: NO circular deps)

```
error.rs  ←── stream.rs  (uses RigInferError)
    ↑
    └── tool.rs    (uses McpToolError)

mod.rs uses ALL three submodules
```

---

## COMMIT FORMAT

```
refactor(provider): <description>

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

---

## FINAL VERIFICATION

```bash
# Full test suite
cd tools && cargo test --workspace --lib
# Expected: 8595 passed, 0 failed

# Clippy
cd tools && cargo clippy --workspace -- -D warnings
# Expected: 0 warnings

# Line count
wc -l tools/nika-engine/src/provider/rig/*.rs
# Expected total: ~3640

# Push
git push
```

---

## START

1. Read `docs/plans/2026-03-28-split-rig-rs-master-plan.md`
2. Run `cd tools && cargo test --workspace --lib` to confirm baseline (8595)
3. Execute Phase 1 → commit
4. Execute Phases 2-6 → commit each
5. Final verification + push

Go.
```
