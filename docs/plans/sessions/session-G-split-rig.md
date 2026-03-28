# Session G: Split rig.rs into Module Directory (~2-3h)

## Context
Nika workflow engine. Workspace: `tools/` (12 Rust crates). Main branch, 8600+ tests.
Source plan: `docs/plans/2026-03-29-split-rig-definitive-plan.md` -- READ IT FIRST.
Dev reference: `tools/nika/CLAUDE.md` for crate layout and conventions.

## Mission: Decompose a 3675-LOC monolith into 5 focused modules with zero behavior change

`nika-engine/src/provider/rig.rs` is the single largest file in the codebase. It contains
error types, streaming types, MCP tool wrappers, the RigProvider enum, and 1464 lines of
tests -- all in one file. This session splits it into a `rig/` directory module with proper
separation of concerns while preserving every re-export path and all 65+ test functions.

### Methodology
Pure refactor: NO logic changes. Move code, add module declarations, verify re-exports.
After EACH phase: `cargo check -p nika-engine`, then `cargo test -p nika-engine --lib -- provider::rig`.
Full workspace verification at the end. 1 phase = 1 commit. Always `--lib` to avoid keychain.

---

## VERIFIED STRUCTURE (Line numbers confirmed 2026-03-29)

### Current file: `tools/nika-engine/src/provider/rig.rs` (3675 LOC)

| # | Section | Lines | LOC | Target File | Key Types |
|---|---------|-------|-----|-------------|-----------|
| A | Imports + module doc | 1-44 | 44 | mod.rs | `use` statements |
| B | McpToolError types | 46-120 | 75 | **error.rs** | `McpToolError`, `McpToolErrorKind` |
| C | InferOptions + helpers | 122-187 | 66 | mod.rs | `InferOptions`, `is_reasoning_model()` |
| D | RigProvider enum + core impl | 189-1159 | 971 | mod.rs | `RigProvider` (9 variants), `infer()`, `auto()` |
| E | ProviderVerify types | 1161-1216 | 56 | **error.rs** | `ProviderVerifyResult`, `ProviderVerifyError` |
| F | RigInferError | 1218-1229 | 12 | **error.rs** | `RigInferError` |
| G | StreamChunk enum | 1231-1346 | 116 | **stream.rs** | `StreamChunk` (30+ variants) |
| H | StreamResult + consume | 1348-1446 | 99 | **stream.rs** | `StreamResult`, `consume_rig_stream()` |
| I | RigProvider streaming impl | 1448-1968 | 521 | mod.rs | `infer_stream()`, `infer_stream_inner()` |
| J | NikaMcpTool | 1970-2135 | 166 | **tool.rs** | `NikaMcpToolDef`, `NikaMcpTool`, `ToolDyn` impl |
| K | Native vision helper | 2137-2210 | 74 | mod.rs | `extract_native_vision_parts()` (cfg-gated) |
| L | Tests | 2212-3675 | 1464 | **tests.rs** | ~65 test functions |

### Target architecture:

```
provider/
  mod.rs            (65 LOC -- UNCHANGED, re-exports still resolve)
  rig/
    mod.rs          (~1600 LOC -- RigProvider + impls + helpers)
    error.rs        (~143 LOC -- McpToolError + ProviderVerify + RigInferError)
    stream.rs       (~215 LOC -- StreamChunk + StreamResult + consume_rig_stream)
    tool.rs         (~166 LOC -- NikaMcpToolDef + NikaMcpTool + ToolDyn impl)
    tests.rs        (~1464 LOC -- all tests, single file)
  cost.rs
  endpoints.rs
  native/
```

### Dependency DAG (no cycles):

```
error.rs  (LEAF -- zero nika imports)
   ^
   |-- stream.rs  (imports super::error::RigInferError)
   |-- tool.rs    (imports super::error::McpToolError)
   +-- mod.rs     (imports everything from all 3 submodules)
```

---

## Phase 1: Directory Rename (ZERO risk)

**Action**: Convert `rig.rs` to `rig/mod.rs`. Semantically identical in Rust.
```bash
mkdir -p tools/nika-engine/src/provider/rig
mv tools/nika-engine/src/provider/rig.rs tools/nika-engine/src/provider/rig/mod.rs
```

**Verify**: `cd tools && cargo check -p nika-engine`
**Test**: `cd tools && cargo test -p nika-engine --lib -- provider::rig 2>&1 | tail -5`
**Commit**: `refactor(provider): convert rig.rs to directory module`

---

## Phase 2: Extract error.rs (Sections B + E + F -- ~143 LOC)

### 2.1 Create `tools/nika-engine/src/provider/rig/error.rs`

Cut these 3 sections from mod.rs:
- **Section B** (lines 46-120): `McpToolError`, `McpToolErrorKind`, constructors, Display, Error impl
- **Section E** (lines 1161-1216): `ProviderVerifyResult`, `ProviderVerifyError`, `suggestion()` method
- **Section F** (lines 1218-1229): `RigInferError` enum (3 variants)

### 2.2 error.rs header:
```rust
//! Error types for the rig provider layer.
use std::time::Duration;
```

### 2.3 In mod.rs, replace cut sections with:
```rust
pub mod error;
pub use error::{McpToolError, McpToolErrorKind, ProviderVerifyError, ProviderVerifyResult, RigInferError};
```

### TDD approach:
No new tests needed -- all existing tests exercise these types. If `cargo test` passes, the
move was correct.

**Verify**: `cd tools && cargo check -p nika-engine && cargo test -p nika-engine --lib -- provider::rig`
**Commit**: `refactor(provider): extract error types to rig/error.rs`

---

## Phase 3: Extract stream.rs (Sections G + H -- ~215 LOC)

### 3.1 Create `tools/nika-engine/src/provider/rig/stream.rs`

Cut:
- **Section G** (lines 1231-1346): `StreamChunk` enum (30+ variants)
- **Section H** (lines 1348-1446): `StreamResult` struct + `consume_rig_stream()` function

### 3.2 stream.rs header:
```rust
//! Streaming types and response consumer for rig provider inference.
use futures::StreamExt;
use rig::completion::GetTokenUsage;
use rig::streaming::StreamedAssistantContent;
use std::time::Instant;
use tokio::sync::mpsc;
use tokio::time::timeout;
use crate::util::STREAM_CHUNK_TIMEOUT;
use super::error::RigInferError;
```

### 3.3 CRITICAL visibility change:
```rust
// WAS: async fn consume_rig_stream<R>(
// NOW: pub(super) async fn consume_rig_stream<R>(
```

### 3.4 In mod.rs:
```rust
pub mod stream;
pub use stream::{StreamChunk, StreamResult};
use stream::consume_rig_stream;
```

**Verify**: `cd tools && cargo check -p nika-engine && cargo test -p nika-engine --lib -- provider::rig`
**Commit**: `refactor(provider): extract streaming types to rig/stream.rs`

---

## Phase 4: Extract tool.rs (Section J -- ~166 LOC)

### 4.1 Create `tools/nika-engine/src/provider/rig/tool.rs`

Cut section J (lines 1970-2135): `NikaMcpToolDef`, `AgentMediaStaging`, `NikaMcpTool`,
`BoxFuture` type alias, `impl ToolDyn for NikaMcpTool`.

Do NOT move section K (native vision helper, lines 2137-2210) -- stays in mod.rs
because it is `#[cfg(feature = "native-inference")]` and used by RigProvider methods.

### 4.2 tool.rs header:
```rust
//! NikaMcpTool -- rig-core ToolDyn wrapper for MCP tools.
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use rig::completion::ToolDefinition;
use rig::tool::{ToolDyn, ToolError};
use crate::mcp::McpClient;
use super::error::McpToolError;
```

### 4.3 In mod.rs:
```rust
pub mod tool;
pub use tool::{AgentMediaStaging, NikaMcpTool, NikaMcpToolDef};
```

**Verify**: `cd tools && cargo check -p nika-engine && cargo test -p nika-engine --lib -- provider::rig`
**Commit**: `refactor(provider): extract NikaMcpTool to rig/tool.rs`

---

## Phase 5: Extract tests.rs (Section L -- ~1464 LOC)

### 5.1 Create `tools/nika-engine/src/provider/rig/tests.rs`

Cut entire `#[cfg(test)] mod tests { ... }` block (lines 2212-3675).
The file content is the INNER block -- remove the wrapping `#[cfg(test)] mod tests {` and closing `}`.

### 5.2 Ensure `use super::*;` is at the top of tests.rs.

### 5.3 In mod.rs:
```rust
#[cfg(test)]
#[path = "tests.rs"]
mod tests;
```

**Verify (RUN ACTUAL TESTS)**: `cd tools && cargo test -p nika-engine --lib -- provider::rig 2>&1 | tail -10`
**Commit**: `refactor(provider): extract tests to rig/tests.rs`

---

## Phase 6: Clean Up mod.rs Imports

### 6.1 Remove now-unused imports from mod.rs
After split, mod.rs no longer directly uses `std::future::Future`, `std::pin::Pin`, `rig::tool::ToolError`.
Check each with `cargo check` -- the compiler will tell you.

### 6.2 Line count verification:
```bash
wc -l tools/nika-engine/src/provider/rig/*.rs
```
Expected total: ~3675 (+/- 20 from added module headers).

### 6.3 Full workspace verification:
```bash
cd tools && cargo test --workspace --lib 2>&1 | grep "test result:"
cd tools && cargo clippy --workspace -- -D warnings 2>&1 | tail -3
```
Expected: all 8600+ tests pass, 0 clippy warnings.

**Commit**: `refactor(provider): clean up rig/mod.rs imports after split`

---

## Re-Export Chain (MUST be preserved)

```
tool.rs defines NikaMcpTool
  -> rig/mod.rs: pub use tool::NikaMcpTool;
    -> provider/mod.rs line 54: pub use rig::{NikaMcpTool, RigProvider, StreamResult};
      -> consumers: use crate::provider::rig::NikaMcpTool  OK
      -> consumers: use nika_engine::provider::rig::NikaMcpTool  OK
```

`provider/mod.rs` (64 LOC) does NOT need changes -- `pub use rig::NikaMcpTool` resolves
through `rig/mod.rs` re-exports.

---

## Risk Checklist

- [ ] 22 `#[cfg(feature)]` gates -- all in mod.rs. None crosses module boundaries.
- [ ] 5 local macros (`vision_prompt!`, `build_agent_with_tools!`, etc.) -- all defined inside method bodies.
- [ ] `consume_rig_stream` generics -- complex signature. Works as `pub(super)` in stream.rs.
- [ ] `BoxFuture` type alias -- moves with tool.rs. Only used by ToolDyn impl.
- [ ] Tests use `use super::*` -- resolves through mod.rs re-exports.
- [ ] No circular deps -- DAG: error <- stream, error <- tool, all <- mod.rs.

---

## E2E Verification

No `.nika.yaml` needed -- this is a pure refactor. The verification is:

```bash
# 1. All rig tests pass
cd tools && cargo test -p nika-engine --lib -- provider::rig

# 2. Full workspace compiles
cd tools && cargo check --workspace

# 3. Full test suite passes
cd tools && cargo test --workspace --lib

# 4. Zero clippy warnings
cd tools && cargo clippy --workspace -- -D warnings

# 5. Line count sanity
wc -l tools/nika-engine/src/provider/rig/*.rs
# Expected: ~3675 total across 5 files
```

---

## Commit Strategy (6 commits)

```
refactor(provider): convert rig.rs to directory module
refactor(provider): extract error types to rig/error.rs
refactor(provider): extract streaming types to rig/stream.rs
refactor(provider): extract NikaMcpTool to rig/tool.rs
refactor(provider): extract tests to rig/tests.rs
refactor(provider): clean up rig/mod.rs imports after split
```
