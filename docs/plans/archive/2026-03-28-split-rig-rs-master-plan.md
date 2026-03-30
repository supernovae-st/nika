# Task 4.1: Split rig.rs into Focused Modules — Master Plan

> **For Claude:** This is an EXECUTION plan. Read every section. Follow it exactly.
> Use `cargo check -p nika-engine` after EVERY file move. Commit after each phase.

**Goal:** Split `tools/nika-engine/src/provider/rig.rs` (3640 LOC) into a directory module with 5 focused files, preserving 100% backward compatibility via re-exports.

**Baseline:** 8595 tests pass, 0 clippy warnings. No test count change allowed.

---

## Architecture: Before → After

```
BEFORE:
  provider/
  ├── mod.rs          (65 LOC — re-exports)
  ├── rig.rs          (3640 LOC — EVERYTHING)
  ├── cost.rs
  ├── endpoints.rs
  └── native/

AFTER:
  provider/
  ├── mod.rs          (65 LOC — unchanged, re-exports still resolve)
  ├── rig/
  │   ├── mod.rs      (~1580 LOC — RigProvider + infer + verify + helpers + InferOptions)
  │   ├── error.rs    (~120 LOC — McpToolError + RigInferError + ProviderVerify types)
  │   ├── stream.rs   (~215 LOC — StreamChunk + StreamResult + consume_rig_stream)
  │   ├── tool.rs     (~245 LOC — NikaMcpToolDef + NikaMcpTool + ToolDyn impl)
  │   └── tests.rs    (~1460 LOC — all tests in single file)
  ├── cost.rs
  ├── endpoints.rs
  └── native/
```

---

## Dependency DAG (No Circular Dependencies — Confirmed by Architect Agent)

```
error.rs  (leaf — no nika imports)
    ↑
    ├── stream.rs  (uses RigInferError)
    ├── tool.rs    (uses McpToolError)
    └── mod.rs     (uses everything from all 3)
```

All macros are method-local (no cross-module macro risk).
All `#[cfg(feature = "native-inference")]` gates stay in mod.rs (22 gates, none crosses boundaries).

---

## Section Map (from Architect Agent analysis)

| Section | Lines | LOC | Target File | Key Types |
|---------|-------|-----|-------------|-----------|
| Imports + doc | 1-44 | 44 | mod.rs (stays) | `use` statements |
| Tool errors | 46-118 | 73 | **error.rs** | `McpToolError`, `McpToolErrorKind` |
| InferOptions + helpers | 120-182 | 63 | mod.rs (stays) | `InferOptions`, `is_reasoning_model()`, `build_response_format_params()`, `supports_native_structured_output()` |
| RigProvider enum + core impls | 184-1145 | 962 | mod.rs (stays) | `RigProvider`, `from_name()`, `infer()`, `verify()`, `auto()` |
| ProviderVerify types | 1147-1200 | 54 | **error.rs** | `ProviderVerifyResult`, `ProviderVerifyError` |
| RigInferError | 1202-1215 | 14 | **error.rs** | `RigInferError` |
| StreamChunk enum | 1217-1332 | 116 | **stream.rs** | `StreamChunk` (30+ variants) |
| StreamResult + consume | 1334-1432 | 99 | **stream.rs** | `StreamResult`, `consume_rig_stream()` |
| infer_stream methods | 1434-1933 | 500 | mod.rs (stays) | `impl RigProvider` streaming block |
| NikaMcpTool | 1935-2100 | 166 | **tool.rs** | `NikaMcpToolDef`, `NikaMcpTool`, `AgentMediaStaging`, `ToolDyn` impl |
| Native vision helper | 2102-2175 | 74 | mod.rs (stays) | `extract_native_vision_parts()` (cfg-gated) |
| Tests | 2177-3640 | 1463 | **tests.rs** | ~65 test functions |

---

## Consumer Map (from Consumer Agent — most important for re-export decisions)

### Most-consumed types (MUST remain accessible at `crate::provider::rig::X`):

| Type | External Files | Crates |
|------|---------------|--------|
| `StreamChunk` | **13 files** | nika-engine, nika-tui (6 files), tests |
| `RigProvider` | **14 files** | nika-engine, nika-cli, nika-tui, tests |
| `InferOptions` | 1 file | nika-engine (executor/infer.rs) |
| `NikaMcpTool`/`NikaMcpToolDef`/`AgentMediaStaging` | 1 file | nika-engine (rig_agent_loop/mod.rs) |
| `build_response_format_params` | 1 file | nika-engine (executor/infer.rs) |
| `is_reasoning_model` | 1 file | nika-engine (rig_agent_loop/streaming.rs) |

### Zero-external-consumer types (only used within rig.rs itself):

| Type | Notes |
|------|-------|
| `McpToolError` / `McpToolErrorKind` | Only in NikaMcpTool ToolDyn impl |
| `ProviderVerifyResult` / `ProviderVerifyError` | Called via `.verify()`, return types not imported by name |
| `consume_rig_stream` | Private fn, only called by RigProvider methods |
| `RigInferError` | Only consumed by integration tests (not lib code) |

---

## Phase 1: Create Directory Structure (5 min)

### Step 1.1: Create the rig/ directory and rename
```bash
mkdir -p tools/nika-engine/src/provider/rig
mv tools/nika-engine/src/provider/rig.rs tools/nika-engine/src/provider/rig/mod.rs
```

### Step 1.2: Verify compilation
```bash
cd tools && cargo check -p nika-engine
```
This MUST pass — renaming to a directory module is a no-op in Rust.

### Step 1.3: Commit
```
refactor(provider): convert rig.rs to directory module
```

---

## Phase 2: Extract error.rs (~120 LOC)

### What moves:
From mod.rs, cut these sections:
1. **Lines ~46-118**: `McpToolError` struct, `McpToolErrorKind` enum, all impls (Display, Error, constructors)
2. **Lines ~1147-1200**: `ProviderVerifyResult` struct, `ProviderVerifyError` enum + `suggestion()` method
3. **Lines ~1202-1215**: `RigInferError` enum

### Create `error.rs` with:
```rust
//! Error types for the rig provider layer.
//!
//! Contains MCP tool errors, inference errors, and provider verification results.

use std::fmt;
use std::time::Duration;

// [paste McpToolError + McpToolErrorKind + all impls]
// [paste ProviderVerifyResult + ProviderVerifyError + suggestion()]
// [paste RigInferError]
```

### In mod.rs, add at top (after existing `use` statements):
```rust
pub mod error;
pub use error::{McpToolError, McpToolErrorKind, ProviderVerifyError, ProviderVerifyResult, RigInferError};
```

### Verify:
```bash
cd tools && cargo check -p nika-engine
```

### Commit:
```
refactor(provider): extract error types to rig/error.rs
```

---

## Phase 3: Extract stream.rs (~215 LOC)

### What moves:
From mod.rs, cut these sections:
1. **Lines ~1217-1332**: `StreamChunk` enum (all ~30 variants including Mcp*, Infer*, Exec*, Fetch*, Agent*, Provider*, NativeModel*)
2. **Lines ~1334-1365**: `StreamResult` struct + `from_text()` method
3. **Lines ~1367-1432**: `consume_rig_stream()` async function

### Create `stream.rs` with:
```rust
//! Streaming types and response consumer for rig provider inference.
//!
//! [`StreamChunk`] carries real-time streaming events to the TUI and CLI.
//! [`StreamResult`] is the complete response after streaming finishes.
//! [`consume_rig_stream`] is the shared loop for all rig-core providers.

use futures::StreamExt;
use rig::completion::GetTokenUsage;
use rig::streaming::StreamedAssistantContent;
use std::time::Instant;
use tokio::sync::mpsc;
use tokio::time::timeout;

use crate::util::STREAM_CHUNK_TIMEOUT;
use super::error::RigInferError;

// [paste StreamChunk enum]
// [paste StreamResult struct + impl]

/// Consume a rig-core streaming response, forwarding chunks to the channel.
///
/// Shared streaming loop for all rig-core providers. Handles:
/// - Per-chunk timeout via `STREAM_CHUNK_TIMEOUT`
/// - Token text forwarding via `StreamChunk::Token`
/// - Optional thinking/reasoning capture (Claude only)
/// - Token usage extraction from `Final` response
pub(super) async fn consume_rig_stream<R>(
    // ... exact same signature and body
) -> Result<(), RigInferError>
where
    R: Clone + Unpin + GetTokenUsage + serde::Serialize + serde::de::DeserializeOwned,
{ ... }
```

### Key visibility: `consume_rig_stream` is `pub(super)` — only mod.rs calls it.

### In mod.rs, replace the cut sections with:
```rust
pub mod stream;
pub use stream::{StreamChunk, StreamResult};
use stream::consume_rig_stream;
```

### Verify:
```bash
cd tools && cargo check -p nika-engine
```

### Commit:
```
refactor(provider): extract streaming types to rig/stream.rs
```

---

## Phase 4: Extract tool.rs (~245 LOC)

### What moves:
From mod.rs, cut:
1. **Lines ~1935-1951**: `NikaMcpToolDef` struct
2. **Line ~1959**: `AgentMediaStaging` type alias
3. **Lines ~1969-2013**: `NikaMcpTool` struct + methods (new, with_client, with_media_staging, tool_name)
4. **Line ~2016**: `BoxFuture` type alias
5. **Lines ~2018-2100**: `impl ToolDyn for NikaMcpTool`

### Create `tool.rs` with:
```rust
//! NikaMcpTool — rig-core ToolDyn wrapper for MCP tools.
//!
//! Bridges rmcp 0.16 MCP tools to rig-core's agent system
//! without version conflicts (rig uses rmcp 0.13 internally).
//!
//! Binary content from tool results is staged via [`AgentMediaStaging`]
//! side-channel since rig's `ToolDyn::call()` returns `String` only.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use rig::completion::ToolDefinition;
use rig::tool::{ToolDyn, ToolError};

use crate::mcp::McpClient;
use super::error::McpToolError;

// [paste NikaMcpToolDef]
// [paste AgentMediaStaging type alias]
// [paste NikaMcpTool struct + impls]
// [paste BoxFuture type alias]
// [paste ToolDyn impl]
```

### In mod.rs, replace the cut sections with:
```rust
pub mod tool;
pub use tool::{AgentMediaStaging, NikaMcpTool, NikaMcpToolDef};
```

### Verify:
```bash
cd tools && cargo check -p nika-engine
```

### Commit:
```
refactor(provider): extract NikaMcpTool to rig/tool.rs
```

---

## Phase 5: Extract tests.rs (~1460 LOC)

### What moves:
From mod.rs, cut the ENTIRE block:
- **Lines ~2177-3640**: `#[cfg(test)] mod tests { ... }`

### Create `tests.rs`:
```rust
//! Tests for the rig provider module.

use super::*;
// ... keep all existing test imports from inside the mod tests block
```

**Important:** The test module uses `use super::*;` which imports everything from mod.rs. Since mod.rs re-exports from error/stream/tool, all types remain accessible.

### In mod.rs, replace the test block with:
```rust
#[cfg(test)]
#[path = "tests.rs"]
mod tests;
```

### Verify (run the actual tests, not just compile):
```bash
cd tools && cargo test -p nika-engine --lib -- provider::rig 2>&1 | tail -5
```

### Commit:
```
refactor(provider): extract tests to rig/tests.rs
```

---

## Phase 6: Clean Up mod.rs Imports

### After all extractions, mod.rs should look like:
```rust
//! Rig-core provider wrapper
//! ... (existing doc comment)

// Submodules
pub mod error;
pub mod stream;
pub mod tool;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

// Re-exports for backward compat (all consumers use crate::provider::rig::X)
pub use error::{McpToolError, McpToolErrorKind, ProviderVerifyError, ProviderVerifyResult, RigInferError};
pub use stream::{StreamChunk, StreamResult};
pub use tool::{AgentMediaStaging, NikaMcpTool, NikaMcpToolDef};

// Internal re-imports for use within this module
use error::RigInferError;
use stream::{consume_rig_stream, StreamChunk, StreamResult};

// External imports (remove any that were only needed by moved code)
use crate::error_domains::ProviderError;
use crate::mcp::McpClient;
use crate::util::STREAM_CHUNK_TIMEOUT;
use futures::StreamExt;
use std::time::Instant;
// ... etc.

#[cfg(feature = "native-inference")]
use crate::provider::native::InferenceBackend;
use rig::client::{CompletionClient, ProviderClient};
use rig::completion::{CompletionModel as _, GetTokenUsage, Prompt, PromptError, ToolDefinition};
use rig::providers::{anthropic, deepseek, gemini, groq, mistral, openai, xai};
use rig::streaming::StreamedAssistantContent;
use rig::tool::ToolDyn;  // may no longer be needed if only tool.rs uses it
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::timeout;

// InferOptions, helper fns, RigProvider enum, all impl blocks remain here
```

### Step 6.1: Remove unused imports from mod.rs
After moving types, these imports are likely no longer needed in mod.rs:
- `std::future::Future` → moved to tool.rs
- `std::pin::Pin` → moved to tool.rs
- `rig::tool::{ToolDyn, ToolError}` → moved to tool.rs (unless RigProvider still needs ToolDyn)
- `dashmap` → moved to tool.rs (via AgentMediaStaging)

**CHECK**: Does mod.rs still use `ToolDyn`? It uses it in `infer_with_tools()` where it calls `.tool()` on agent builders. If so, keep the import. If not, remove.

### Step 6.2: Run full workspace verification
```bash
cd tools && cargo test --workspace --lib
cd tools && cargo clippy --workspace -- -D warnings
```

### Commit:
```
refactor(provider): clean up rig/mod.rs imports after split
```

---

## Phase 7: Final Verification

### 7.1: Line count check
```bash
wc -l tools/nika-engine/src/provider/rig/*.rs
```
Expected total: ~3640 (same as original, ±10 from added module headers)

### 7.2: Test count check
```bash
cd tools && cargo test --workspace --lib 2>&1 | grep "test result:"
```
Expected: 8595 passed, 0 failed.

### 7.3: Clippy
```bash
cd tools && cargo clippy --workspace -- -D warnings
```
Expected: 0 warnings.

### 7.4: Import path verification
```bash
# Every consumer must still resolve
grep -rn "provider::rig::" tools/ --include="*.rs" | grep -v "target/" | grep -v "tests.rs"
```
Every match should compile cleanly.

### 7.5: Push
```bash
git push
```

---

## Re-export Chain Verification

The critical chain that MUST be preserved:

```
provider/rig/tool.rs        → defines NikaMcpTool
provider/rig/mod.rs          → pub use tool::NikaMcpTool
provider/mod.rs (line 54)   → pub use rig::NikaMcpTool       ← still resolves ✓
```

Same for RigProvider (defined in mod.rs, re-exported by provider/mod.rs) and StreamResult (stream.rs → rig/mod.rs → provider/mod.rs).

**No consumer uses `crate::provider::rig::stream::StreamChunk`** — they all use `crate::provider::rig::StreamChunk`. The re-export in rig/mod.rs makes this work.

---

## Risk Checklist

- [ ] `#[cfg(feature = "native-inference")]`: 22 gates, ALL stay in mod.rs. No risk.
- [ ] Local macros (`vision_prompt!`, `vision_stream!`, `build_agent_with_tools!`, `build_and_prompt!`, `build_request_with_options!`): All defined inside method bodies. No cross-module risk.
- [ ] `consume_rig_stream` generic bounds: Complex signature, but it stays as `pub(super)` in stream.rs. Only mod.rs calls it.
- [ ] `BoxFuture` type alias: Moves to tool.rs. Only used by ToolDyn impl.
- [ ] `extract_native_vision_parts()`: Private cfg-gated fn. Stays in mod.rs.
- [ ] Re-exports from `provider/mod.rs` line 54: `pub use rig::{NikaMcpTool, RigProvider, StreamResult}` — all still resolve via rig/mod.rs re-exports.
- [ ] Tests use `use super::*`: This imports from rig/mod.rs which re-exports everything. Tests remain valid.

---

## Commit Strategy (6 commits)

```
1. refactor(provider): convert rig.rs to directory module
2. refactor(provider): extract error types to rig/error.rs
3. refactor(provider): extract streaming types to rig/stream.rs
4. refactor(provider): extract NikaMcpTool to rig/tool.rs
5. refactor(provider): extract tests to rig/tests.rs
6. refactor(provider): clean up rig/mod.rs imports after split
```

Each commit must:
- Pass `cargo check -p nika-engine` (phases 1-6)
- Pass `cargo test -p nika-engine --lib` (phases 2-6)
- Pass `cargo clippy --workspace -- -D warnings` (phase 6 only — full check)
- End with co-author lines

---

## Priority Matrix

| Phase | Risk | Effort | Dependencies |
|-------|------|--------|-------------|
| 1. Directory rename | ZERO | 2 min | None |
| 2. error.rs | LOW | 15 min | Phase 1 |
| 3. stream.rs | MEDIUM | 20 min | Phase 2 (needs RigInferError) |
| 4. tool.rs | MEDIUM | 20 min | Phase 2 (needs McpToolError) |
| 5. tests.rs | LOW | 10 min | Phases 2-4 |
| 6. Cleanup | LOW | 15 min | Phase 5 |

**Total estimated effort:** ~1.5 hours

---

## Research Sources

This plan was informed by 4 parallel research agents:
1. **rust-architect**: Deep section map, dependency graph, 22 cfg gates identified, no circular deps confirmed
2. **rust-pro (consumer map)**: StreamChunk = 13 external files (most consumed), all import paths mapped
3. **rig-core explorer**: rig-core uses flat per-provider modules; we don't need per-provider split
4. **web-researcher**: Rust module split patterns, `pub(super)` visibility, `#[path]` attribute for tests
