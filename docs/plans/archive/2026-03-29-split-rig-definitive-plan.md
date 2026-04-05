# Split rig.rs — Definitive Execution Plan

> **Date:** 2026-03-29 | **File:** `tools/nika-engine/src/provider/rig.rs` | **3675 LOC → 5 files**
> **Baseline:** 8613 tests, 0 clippy warnings, main branch clean

---

## Target Architecture

```
provider/
├── mod.rs            (65 LOC — UNCHANGED, re-exports still resolve)
├── rig/
│   ├── mod.rs        (~1600 LOC — RigProvider + all impls + InferOptions + helpers)
│   ├── error.rs      (~143 LOC — McpToolError + ProviderVerify + RigInferError)
│   ├── stream.rs     (~215 LOC — StreamChunk + StreamResult + consume_rig_stream)
│   ├── tool.rs       (~241 LOC — NikaMcpToolDef + NikaMcpTool + ToolDyn impl)
│   └── tests.rs      (~1464 LOC — all tests, single file)
├── cost.rs
├── endpoints.rs
└── native/
```

## Dependency DAG

```
error.rs  (LEAF — zero nika imports, only std + thiserror)
   ↑
   ├── stream.rs  (imports: super::error::RigInferError)
   ├── tool.rs    (imports: super::error::McpToolError)
   └── mod.rs     (imports: everything from all 3 submodules)

NO CIRCULAR DEPENDENCIES.
```

---

## Exact Section Boundaries (verified 2026-03-29)

| # | Section | Lines | LOC | Target | Key Types |
|---|---------|-------|-----|--------|-----------|
| A | Imports + module doc | 1–44 | 44 | mod.rs | `use` statements |
| B | McpToolError types | 46–120 | 75 | **error.rs** | `McpToolError`, `McpToolErrorKind`, impls |
| C | InferOptions + helpers | 122–187 | 66 | mod.rs | `InferOptions`, `is_reasoning_model()`, `build_response_format_params()`, `supports_native_structured_output()` |
| D | RigProvider enum + core impl | 189–1159 | 971 | mod.rs | `RigProvider` enum (9 variants), `from_name()`, `infer()`, `infer_vision()`, `infer_with_tools()`, `infer_with_options()`, `auto()`, `verify()`, `is_configured()` |
| E | ProviderVerify types | 1161–1216 | 56 | **error.rs** | `ProviderVerifyResult`, `ProviderVerifyError` |
| F | RigInferError | 1218–1229 | 12 | **error.rs** | `RigInferError` |
| G | StreamChunk enum | 1231–1346 | 116 | **stream.rs** | `StreamChunk` (30+ variants) |
| H | StreamResult + consume | 1348–1446 | 99 | **stream.rs** | `StreamResult`, `consume_rig_stream()` |
| I | RigProvider streaming impl | 1448–1968 | 521 | mod.rs | `infer_stream()`, `infer_stream_inner()`, `infer_stream_with_options()`, `infer_stream_with_options_inner()`, `supports_native_structured_output()` (method) |
| J | NikaMcpTool | 1970–2210 | 241 | **tool.rs** | `NikaMcpToolDef`, `AgentMediaStaging`, `NikaMcpTool`, `BoxFuture`, `impl ToolDyn` |
| K | Native vision helper | 2137–2210 | 74 | mod.rs | `extract_native_vision_parts()` (cfg-gated, private) |
| L | Tests | 2212–3675 | 1464 | **tests.rs** | ~65 test functions |

---

## Phase 1: Directory Rename (2 min, ZERO risk)

```bash
mkdir -p tools/nika-engine/src/provider/rig
mv tools/nika-engine/src/provider/rig.rs tools/nika-engine/src/provider/rig/mod.rs
```

**Verify:** `cd tools && cargo check -p nika-engine`

This is a Rust no-op — `rig.rs` and `rig/mod.rs` are semantically identical.

**Commit:** `refactor(provider): convert rig.rs to directory module`

---

## Phase 2: Extract error.rs (sections B + E + F → ~143 LOC)

### 2.1 Create `tools/nika-engine/src/provider/rig/error.rs`

Cut these 3 sections from mod.rs and paste into error.rs:

**Section B (lines 46–120):** McpToolError + McpToolErrorKind
```
Line 46: // ═══════════════════════════════════════════════════════════════
Line 48: // TOOL ERROR TYPES
Line 54: pub struct McpToolError { ... }
Line 61: pub enum McpToolErrorKind { ... }
Line 72: impl McpToolError { ... }     // 4 constructors
Line 106: impl std::fmt::Display for McpToolError { ... }
Line 118: impl std::error::Error for McpToolError {}
Line 120: (end of section)
```

**Section E (lines 1161–1216):** ProviderVerify types
```
Line 1161: // ═══════════════════════════════════════════════════════════════
Line 1163: // VERIFICATION TYPES
Line 1167: pub struct ProviderVerifyResult { ... }
Line 1178: pub enum ProviderVerifyError { ... }
Line 1195: impl ProviderVerifyError { fn suggestion() ... }
Line 1216: (end of section)
```

**Section F (lines 1218–1229):** RigInferError
```
Line 1218: pub enum RigInferError { ... }  // 3 variants: PromptError, Timeout, VisionNotSupported
Line 1229: (end)
```

### 2.2 error.rs header (add at top):

```rust
//! Error types for the rig provider layer.
//!
//! - [`McpToolError`] / [`McpToolErrorKind`] — MCP tool call errors
//! - [`ProviderVerifyResult`] / [`ProviderVerifyError`] — Provider health check
//! - [`RigInferError`] — Inference operation errors

use std::time::Duration;
```

### 2.3 In mod.rs, replace the 3 cut sections with:

At the TOP (after existing `use` statements, ~line 44):
```rust
pub mod error;
pub use error::{McpToolError, McpToolErrorKind, ProviderVerifyError, ProviderVerifyResult, RigInferError};
```

### 2.4 Verify + Test:
```bash
cd tools && cargo check -p nika-engine
cd tools && cargo test -p nika-engine --lib -- provider::rig 2>&1 | tail -5
```

**Commit:** `refactor(provider): extract error types to rig/error.rs`

---

## Phase 3: Extract stream.rs (sections G + H → ~215 LOC)

### 3.1 Create `tools/nika-engine/src/provider/rig/stream.rs`

Cut these 2 sections from mod.rs:

**Section G (lines 1231–1346):** StreamChunk enum
```
Line 1231: // =============================================================================
Line 1233: // StreamChunk - Communication type for streaming responses
Line 1237: pub enum StreamChunk { ... }   // 30+ variants
Line 1346: (end of enum + closing brace)
```

**Section H (lines 1348–1446):** StreamResult + consume_rig_stream
```
Line 1348: // =============================================================================
Line 1350: // StreamResult - Complete streaming response with token usage
Line 1354: pub struct StreamResult { ... }
Line 1371: impl StreamResult { fn from_text() ... }
Line 1390: async fn consume_rig_stream<R>(...) -> Result<(), RigInferError> { ... }
Line 1446: (end)
```

### 3.2 stream.rs header (add at top):

```rust
//! Streaming types and response consumer for rig provider inference.
//!
//! - [`StreamChunk`] — Real-time streaming events for TUI and CLI display
//! - [`StreamResult`] — Complete response after streaming finishes
//! - [`consume_rig_stream`] — Shared loop for all rig-core providers

use futures::StreamExt;
use rig::completion::GetTokenUsage;
use rig::streaming::StreamedAssistantContent;
use std::time::Instant;
use tokio::sync::mpsc;
use tokio::time::timeout;

use crate::util::STREAM_CHUNK_TIMEOUT;
use super::error::RigInferError;
```

### 3.3 CRITICAL: Change `consume_rig_stream` visibility

In the newly created stream.rs, change:
```rust
async fn consume_rig_stream<R>(    // WAS: private
```
to:
```rust
pub(super) async fn consume_rig_stream<R>(    // NOW: visible to mod.rs only
```

### 3.4 In mod.rs, replace the 2 cut sections with:

```rust
pub mod stream;
pub use stream::{StreamChunk, StreamResult};
use stream::consume_rig_stream;
```

### 3.5 Verify + Test:
```bash
cd tools && cargo check -p nika-engine
cd tools && cargo test -p nika-engine --lib -- provider::rig 2>&1 | tail -5
```

**Commit:** `refactor(provider): extract streaming types to rig/stream.rs`

---

## Phase 4: Extract tool.rs (section J → ~241 LOC)

### 4.1 Create `tools/nika-engine/src/provider/rig/tool.rs`

Cut section J from mod.rs:

**Section J (lines 1970–2135):** NikaMcpTool + ToolDyn impl
```
Line 1970: // =============================================================================
Line 1972: // NikaMcpTool - Wrapper for MCP tools implementing rig-core's ToolDyn
Line 1979: pub struct NikaMcpToolDef { ... }
Line 1994: pub type AgentMediaStaging = ...
Line 2004: pub struct NikaMcpTool { ... }
Line 2012: impl NikaMcpTool { ... }     // new, with_client, with_media_staging, tool_name
Line 2050: type BoxFuture<'a, T> = ...  // private type alias
Line 2053: impl ToolDyn for NikaMcpTool { ... }
Line 2135: (end)
```

**NOTE:** Do NOT move section K (native vision helper, lines 2137-2210) — it stays in mod.rs because it's `#[cfg(feature = "native-inference")]` and used by RigProvider methods.

### 4.2 tool.rs header (add at top):

```rust
//! NikaMcpTool — rig-core ToolDyn wrapper for MCP tools.
//!
//! Bridges rmcp 0.16 MCP tools to rig-core's agent system without
//! version conflicts (rig uses rmcp 0.13 internally).
//!
//! Binary content is staged via [`AgentMediaStaging`] side-channel
//! since rig's `ToolDyn::call()` returns `String` only.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use rig::completion::ToolDefinition;
use rig::tool::{ToolDyn, ToolError};

use crate::mcp::McpClient;
use super::error::McpToolError;
```

### 4.3 In mod.rs, replace the cut section with:

```rust
pub mod tool;
pub use tool::{AgentMediaStaging, NikaMcpTool, NikaMcpToolDef};
```

### 4.4 Verify + Test:
```bash
cd tools && cargo check -p nika-engine
cd tools && cargo test -p nika-engine --lib -- provider::rig 2>&1 | tail -5
```

**Commit:** `refactor(provider): extract NikaMcpTool to rig/tool.rs`

---

## Phase 5: Extract tests.rs (section L → ~1464 LOC)

### 5.1 Create `tools/nika-engine/src/provider/rig/tests.rs`

Cut the ENTIRE test module from mod.rs:

```
Line 2212: #[cfg(test)]
Line 2213: mod tests {
...
Line 3675: }  // end of mod tests
```

### 5.2 In the new tests.rs file:

The content is the INNER block of `mod tests { ... }` — remove the wrapping `#[cfg(test)] mod tests {` and the closing `}`. The file itself becomes the test module.

Keep all existing `use` statements from inside the block. Add at top if not present:
```rust
use super::*;
```

### 5.3 In mod.rs, replace the cut block with:

```rust
#[cfg(test)]
#[path = "tests.rs"]
mod tests;
```

### 5.4 Verify (RUN ACTUAL TESTS, not just compile):
```bash
cd tools && cargo test -p nika-engine --lib -- provider::rig 2>&1 | tail -10
```
Expect: all rig tests pass.

**Commit:** `refactor(provider): extract tests to rig/tests.rs`

---

## Phase 6: Clean Up mod.rs Imports

### 6.1 Remove now-unused imports from mod.rs

These imports were ONLY needed by code that moved out:

```rust
// REMOVE if only tool.rs uses them:
use std::future::Future;   // → moved to tool.rs
use std::pin::Pin;          // → moved to tool.rs
use rig::tool::ToolError;   // → moved to tool.rs (but check if mod.rs still needs ToolDyn)
```

**CHECK BEFORE REMOVING:** Does mod.rs still reference `ToolDyn` directly? Search for `ToolDyn` in mod.rs after the split. If the `infer_with_tools()` method uses `dyn ToolDyn` or `.tool()` calls that require the trait in scope, keep the import.

### 6.2 Add internal imports for moved types:

The existing `pub use` re-exports handle the public API. For internal use within mod.rs, you may need:
```rust
use error::RigInferError;
use stream::consume_rig_stream;
```

But since `pub use` already brings them into scope, these may be redundant. Compile will tell you.

### 6.3 Full workspace verification:
```bash
cd tools && cargo test --workspace --lib 2>&1 | grep "test result:"
cd tools && cargo clippy --workspace -- -D warnings 2>&1 | tail -3
```

**Expected:** 8613 tests pass, 0 clippy warnings.

### 6.4 Line count verification:
```bash
wc -l tools/nika-engine/src/provider/rig/*.rs
```
Expected total: ~3675 (±20 from added module headers)

**Commit:** `refactor(provider): clean up rig/mod.rs imports after split`

---

## Re-Export Chain (MUST be preserved)

```
tool.rs defines NikaMcpTool
  → rig/mod.rs: pub use tool::NikaMcpTool;
    → provider/mod.rs line 54: pub use rig::{NikaMcpTool, RigProvider, StreamResult};
      → consumers: use crate::provider::rig::NikaMcpTool  ✓
      → consumers: use nika_engine::provider::rig::NikaMcpTool  ✓
```

`provider/mod.rs` does NOT need changes — `pub use rig::NikaMcpTool` resolves through `rig/mod.rs` re-exports.

---

## Risk Checklist

- [ ] **22 `#[cfg(feature)]` gates** — all in mod.rs (RigProvider variants/methods + native vision helper). None crosses module boundaries.
- [ ] **5 local macros** (`vision_prompt!`, `vision_stream!`, `build_agent_with_tools!`, `build_and_prompt!`, `build_request_with_options!`) — all defined inside method bodies. No cross-module risk.
- [ ] **`consume_rig_stream` generics** — complex signature with `R: Clone + Unpin + GetTokenUsage + Serialize + DeserializeOwned`. Works fine as `pub(super)` in stream.rs.
- [ ] **`BoxFuture` type alias** — moves with tool.rs. Only used by ToolDyn impl.
- [ ] **Tests use `use super::*`** — resolves through mod.rs re-exports. All types accessible.
- [ ] **No circular deps** — DAG: error ← stream, error ← tool, all ← mod.rs.

---

## Commit Strategy (6 commits)

```
1. refactor(provider): convert rig.rs to directory module
2. refactor(provider): extract error types to rig/error.rs
3. refactor(provider): extract streaming types to rig/stream.rs
4. refactor(provider): extract NikaMcpTool to rig/tool.rs
5. refactor(provider): extract tests to rig/tests.rs
6. refactor(provider): clean up rig/mod.rs imports after split

Each commit ends with:
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```
