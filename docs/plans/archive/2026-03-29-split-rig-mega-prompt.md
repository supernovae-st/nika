# Split rig.rs — Mega Handoff Prompt

**Copy everything below the line into a new Claude Code session.**

---

```
## Context

You are working on Nika, a semantic YAML workflow engine for AI tasks.
Workspace: `tools/` (Cargo workspace, 10 crates). Branch: main. Tests: 8613 pass, 0 clippy warnings.

## YOUR MISSION

Split `tools/nika-engine/src/provider/rig.rs` (3675 LOC) into a directory module with 5 files.
This is a PURE REFACTOR — zero behavior change, zero API change, zero test count change.

## MANDATORY FIRST STEPS

1. Read the plan: `docs/plans/2026-03-29-split-rig-definitive-plan.md` — it has exact line numbers, dependency DAG, re-export chain, and risk checklist.
2. Read `tools/nika-engine/src/provider/mod.rs` — understand the re-export chain (line 54: `pub use rig::{NikaMcpTool, RigProvider, StreamResult};`).
3. Run baseline: `cd tools && cargo test --workspace --lib 2>&1 | grep "test result:"` — confirm 8613 tests pass.
4. Read `tools/nika-engine/src/provider/rig.rs` to verify the section boundaries match the plan.

## METHODOLOGY

Use the `rust-core` skill for Rust module patterns.
Use `verification-before-completion` — compile check after EVERY phase.
Use `test-driven-development` mindset — never break a test.

### For EACH of the 6 phases:
1. READ the exact lines to move (verify content matches plan)
2. CREATE the new file with moved content + correct imports
3. REMOVE the content from mod.rs
4. ADD the `pub mod` + `pub use` declarations in mod.rs
5. FIX imports (use `super::error::X` for sibling references)
6. COMPILE: `cd tools && cargo check -p nika-engine`
7. TEST: `cd tools && cargo test -p nika-engine --lib -- provider::rig`
8. COMMIT with conventional format + co-author lines

**NEVER skip the compile check. NEVER combine phases. 1 phase = 1 commit.**

## THE 6 PHASES

### Phase 1: Directory Rename (ZERO risk)
```bash
mkdir -p tools/nika-engine/src/provider/rig
mv tools/nika-engine/src/provider/rig.rs tools/nika-engine/src/provider/rig/mod.rs
```
Compile. Commit: `refactor(provider): convert rig.rs to directory module`

### Phase 2: Extract error.rs (~143 LOC)
**CUT from mod.rs:**
- Lines ~46–120: McpToolError struct + McpToolErrorKind enum + all impls
- Lines ~1161–1216: ProviderVerifyResult + ProviderVerifyError
- Lines ~1218–1229: RigInferError enum

**error.rs needs:** `use std::time::Duration;` (for ProviderVerifyError) — NO nika internal imports.

**Add to mod.rs:**
```rust
pub mod error;
pub use error::{McpToolError, McpToolErrorKind, ProviderVerifyError, ProviderVerifyResult, RigInferError};
```

### Phase 3: Extract stream.rs (~215 LOC)
**CUT from mod.rs:**
- Lines ~1231–1346: StreamChunk enum (30+ variants)
- Lines ~1348–1446: StreamResult struct + consume_rig_stream fn

**stream.rs needs:**
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

**CRITICAL:** Change `async fn consume_rig_stream` to `pub(super) async fn consume_rig_stream`

**Add to mod.rs:**
```rust
pub mod stream;
pub use stream::{StreamChunk, StreamResult};
use stream::consume_rig_stream;
```

### Phase 4: Extract tool.rs (~241 LOC)
**CUT from mod.rs:**
- Lines ~1970–2135: NikaMcpToolDef, AgentMediaStaging, NikaMcpTool, BoxFuture, impl ToolDyn

**DO NOT MOVE** the native vision helper (lines ~2137–2210) — it stays in mod.rs.

**tool.rs needs:**
```rust
use super::error::McpToolError;
use crate::mcp::McpClient;
use rig::completion::ToolDefinition;
use rig::tool::{ToolDyn, ToolError};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
```

**Add to mod.rs:**
```rust
pub mod tool;
pub use tool::{AgentMediaStaging, NikaMcpTool, NikaMcpToolDef};
```

### Phase 5: Extract tests.rs (~1464 LOC)
**CUT from mod.rs:** The entire `#[cfg(test)] mod tests { ... }` block (lines ~2212–3675).
The content of tests.rs is the INNER block — remove the wrapping `mod tests {` and `}`.
Keep `use super::*;` at the top.

**Add to mod.rs:**
```rust
#[cfg(test)]
#[path = "tests.rs"]
mod tests;
```

**Run ACTUAL tests:** `cd tools && cargo test -p nika-engine --lib -- provider::rig`

### Phase 6: Clean up imports
Remove unused imports from mod.rs: `std::future::Future`, `std::pin::Pin`, possibly `rig::tool::ToolError`.
CHECK if `rig::tool::ToolDyn` is still needed in mod.rs before removing.
Run full workspace: `cd tools && cargo test --workspace --lib && cargo clippy --workspace -- -D warnings`

## DEPENDENCY DAG (no circular deps)

```
error.rs  (LEAF — only std + thiserror)
   ↑
   ├── stream.rs  (uses super::error::RigInferError)
   ├── tool.rs    (uses super::error::McpToolError)
   └── mod.rs     (uses everything from all 3)
```

## RE-EXPORT CHAIN (MUST be preserved)

```
provider/mod.rs line 54:
  pub use rig::{NikaMcpTool, RigProvider, StreamResult};

After split, this resolves through:
  rig/mod.rs → pub use tool::NikaMcpTool
  rig/mod.rs → RigProvider (defined here, stays)
  rig/mod.rs → pub use stream::StreamResult

provider/mod.rs does NOT need changes.
```

## WHAT STAYS IN mod.rs

- All imports for RigProvider methods
- InferOptions struct + 3 helper fns (is_reasoning_model, build_response_format_params, supports_native_structured_output)
- RigProvider enum (9 variants) + ALL impl blocks
- extract_native_vision_parts() (cfg-gated, private)
- Module declarations + re-exports

## COMMIT FORMAT

```
refactor(provider): <description>

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

## FINAL VERIFICATION

```bash
cd tools && cargo test --workspace --lib   # 8613 tests, 0 failures
cd tools && cargo clippy --workspace -- -D warnings  # 0 warnings
wc -l tools/nika-engine/src/provider/rig/*.rs  # total ~3675
git push
```

## RULES

- Line numbers are APPROXIMATE — always READ the file to find actual section headers (`// ═══════`)
- Never move more than 1 section between compile checks
- If compilation fails, diagnose immediately — don't proceed
- The `#[cfg(feature = "native-inference")]` blocks (22 of them) ALL stay in mod.rs
- All 5 local macros are method-body-local — they move with their containing method (which stays in mod.rs)
- Tests use `use super::*` — this resolves through mod.rs re-exports

Go.
```
