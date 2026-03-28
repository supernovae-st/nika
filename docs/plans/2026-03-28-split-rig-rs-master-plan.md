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
  ├── mod.rs          (65 LOC — unchanged re-exports still work)
  ├── rig/
  │   ├── mod.rs      (~1630 LOC — RigProvider + infer + verify + helpers + InferOptions)
  │   ├── error.rs    (~90 LOC — McpToolError + RigInferError)
  │   ├── stream.rs   (~215 LOC — StreamChunk + StreamResult + consume_rig_stream)
  │   ├── tool.rs     (~245 LOC — NikaMcpToolDef + NikaMcpTool + ToolDyn impl)
  │   └── tests.rs    (~1460 LOC — all tests)
  ├── cost.rs
  ├── endpoints.rs
  └── native/
```

---

## Phase 1: Create Directory Structure (5 min)

### Step 1.1: Create the rig/ directory
```bash
mkdir -p tools/nika-engine/src/provider/rig
```

### Step 1.2: Rename rig.rs → rig/mod.rs
```bash
mv tools/nika-engine/src/provider/rig.rs tools/nika-engine/src/provider/rig/mod.rs
```

### Step 1.3: Verify compilation
```bash
cd tools && cargo check -p nika-engine
```
This MUST pass — renaming to a directory module is a no-op in Rust.

### Step 1.4: Commit
```
refactor(provider): rename rig.rs → rig/mod.rs (directory module)
```

---

## Phase 2: Extract error.rs (~90 LOC)

### Step 2.1: Create `tools/nika-engine/src/provider/rig/error.rs`

Move these items from mod.rs (lines 46-120 and 1202-1215):

```rust
// tools/nika-engine/src/provider/rig/error.rs

//! Error types for the rig provider layer.

use std::fmt;

// ═══════════════════════════════════════════════════════════════════════════
// TOOL ERROR TYPES
// ═══════════════════════════════════════════════════════════════════════════

/// MCP tool call error with semantic error kinds
#[derive(Debug)]
pub struct McpToolError {
    pub(super) kind: McpToolErrorKind,
    pub(super) message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpToolErrorKind {
    NotConnected,
    ToolNotFound,
    InvalidArgs,
    ExecutionFailed,
    Timeout,
}

// ... all McpToolError impls (Display, Error, constructors)

/// Error type for RigProvider infer operations
#[derive(Debug, thiserror::Error)]
pub enum RigInferError {
    #[error("Completion error: {0}")]
    PromptError(String),

    #[error("Stream timeout: no chunk received for {duration_ms}ms")]
    Timeout { duration_ms: u64 },

    #[error("Vision not supported: {0}")]
    VisionNotSupported(String),
}
```

### Step 2.2: In mod.rs, replace the moved code with:
```rust
mod error;
pub use error::{McpToolError, McpToolErrorKind, RigInferError};
```

### Step 2.3: Verify
```bash
cd tools && cargo check -p nika-engine
```

### Step 2.4: Commit
```
refactor(provider): extract error types to rig/error.rs
```

---

## Phase 3: Extract stream.rs (~215 LOC)

### Step 3.1: Create `tools/nika-engine/src/provider/rig/stream.rs`

Move these items from mod.rs (lines 1217-1432):

```rust
// tools/nika-engine/src/provider/rig/stream.rs

//! Streaming types and helpers for rig provider inference.

use futures::StreamExt;
use rig::completion::GetTokenUsage;
use rig::streaming::StreamedAssistantContent;
use std::time::Instant;
use tokio::sync::mpsc;
use tokio::time::timeout;

use crate::util::STREAM_CHUNK_TIMEOUT;
use super::error::RigInferError;

/// Chunk of streaming response for real-time display
#[derive(Debug, Clone)]
pub enum StreamChunk {
    // ... ALL variants (Token, Thinking, Done, Error, Metrics, Mcp*, Infer*, Exec*, Fetch*, Agent*, Provider*, NativeModel*)
}

/// Complete streaming response with text and token usage metrics
#[derive(Debug, Clone, Default)]
pub struct StreamResult {
    pub text: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cached_input_tokens: u64,
    pub ttft_ms: Option<u64>,
    pub request_id: Option<String>,
}

impl StreamResult {
    pub fn from_text(text: impl Into<String>) -> Self { ... }
}

/// Consume a rig-core streaming response, forwarding chunks to the channel.
pub(super) async fn consume_rig_stream<R>(
    stream: &mut rig::streaming::StreamingCompletionResponse<R>,
    tx: &mpsc::Sender<StreamChunk>,
    response_parts: &mut Vec<String>,
    result: &mut StreamResult,
    capture_thinking: bool,
    stream_start: Instant,
) -> Result<(), RigInferError>
where
    R: Clone + Unpin + GetTokenUsage + serde::Serialize + serde::de::DeserializeOwned,
{ ... }
```

### Key decisions:
- `consume_rig_stream` is `pub(super)` — only used by `mod.rs` (RigProvider::infer_stream methods)
- StreamChunk and StreamResult are `pub` — used across the engine

### Step 3.2: In mod.rs, replace moved code with:
```rust
mod stream;
pub use stream::{StreamChunk, StreamResult};
use stream::consume_rig_stream;
```

### Step 3.3: Verify
```bash
cd tools && cargo check -p nika-engine
```

### Step 3.4: Commit
```
refactor(provider): extract streaming types to rig/stream.rs
```

---

## Phase 4: Extract tool.rs (~245 LOC)

### Step 4.1: Create `tools/nika-engine/src/provider/rig/tool.rs`

Move these items from mod.rs (lines 1935-2176):

```rust
// tools/nika-engine/src/provider/rig/tool.rs

//! NikaMcpTool — rig-core ToolDyn wrapper for MCP tools.
//!
//! Bridges rmcp 0.16 MCP tools to rig-core's agent system
//! without version conflicts (rig uses rmcp 0.13 internally).

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use rig::completion::ToolDefinition;
use rig::tool::{ToolDyn, ToolError};

use crate::mcp::McpClient;
use super::error::McpToolError;

/// Tool definition for Nika MCP tools.
#[derive(Debug, Clone)]
pub struct NikaMcpToolDef { ... }

/// Shared media staging for agent tool calls.
pub type AgentMediaStaging = Arc<dashmap::DashMap<String, Vec<crate::mcp::types::ContentBlock>>>;

/// MCP tool wrapper implementing rig-core's `ToolDyn` trait.
#[derive(Debug, Clone)]
pub struct NikaMcpTool { ... }

impl NikaMcpTool { ... }

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

impl ToolDyn for NikaMcpTool { ... }
```

### Step 4.2: In mod.rs, replace moved code with:
```rust
mod tool;
pub use tool::{AgentMediaStaging, NikaMcpTool, NikaMcpToolDef};
```

### Step 4.3: Verify
```bash
cd tools && cargo check -p nika-engine
```

### Step 4.4: Commit
```
refactor(provider): extract NikaMcpTool to rig/tool.rs
```

---

## Phase 5: Extract tests.rs (~1460 LOC)

### Step 5.1: Create `tools/nika-engine/src/provider/rig/tests.rs`

Move the entire `#[cfg(test)] mod tests { ... }` block (lines 2177-3640).

### Step 5.2: In the test file, add super imports:
```rust
// At the top of tests.rs
#[cfg(test)]
mod tests {
    use super::*;
    // ... existing test imports
}
```

### Step 5.3: In mod.rs, replace the test module with:
```rust
#[cfg(test)]
#[path = "tests.rs"]
mod tests;
```

### Step 5.4: Verify
```bash
cd tools && cargo test -p nika-engine --lib -- provider::rig 2>&1 | tail -5
```

### Step 5.5: Commit
```
refactor(provider): extract tests to rig/tests.rs
```

---

## Phase 6: Clean Up mod.rs

### Step 6.1: Organize imports in mod.rs
After extraction, mod.rs should have:
- Module doc comment
- `mod error; mod stream; mod tool;` + `#[cfg(test)]` test module
- `pub use` re-exports
- RigProvider enum definition
- InferOptions struct + helper functions
- All `impl RigProvider` blocks (infer, infer_stream, verify, etc.)

### Step 6.2: Remove unused imports
After moving code, some imports will be unused in mod.rs. Remove them.

### Step 6.3: Add module-level imports for moved types
Where mod.rs uses types from submodules internally:
```rust
use error::RigInferError;
use stream::{consume_rig_stream, StreamChunk, StreamResult};
```

### Step 6.4: Verify full workspace
```bash
cd tools && cargo test --workspace --lib
cd tools && cargo clippy --workspace -- -D warnings
```

### Step 6.5: Commit
```
refactor(provider): clean up rig/mod.rs imports after split
```

---

## Phase 7: Verify Re-exports (CRITICAL)

### The re-export chain must be preserved:
```
nika-engine/src/provider/mod.rs line 54:
  pub use rig::{NikaMcpTool, RigProvider, StreamResult};

nika-engine/src/lib.rs line 93:
  pub use provider::*;  (or specific types)
```

Since `rig/mod.rs` does `pub use tool::NikaMcpTool;` etc., the chain:
- `provider::rig::NikaMcpTool` → works (re-exported from rig/mod.rs)
- `provider::NikaMcpTool` → works (re-exported from provider/mod.rs)
- External users see no change

### Step 7.1: Grep for all import paths
```bash
grep -rn "provider::rig::" tools/nika-engine/src/ --include="*.rs" | grep -v "tests"
grep -rn "use crate::provider::rig" tools/nika-engine/src/ --include="*.rs"
```

### Step 7.2: Verify no broken paths
```bash
cd tools && cargo test --workspace --lib 2>&1 | grep "test result:"
```
Expect: 8595 passed, 0 failed.

---

## Consumer Map (types that move → who uses them)

| Type | New Location | Consumers |
|------|-------------|-----------|
| `McpToolError` | error.rs | tool.rs (ToolDyn impl), agent loop |
| `McpToolErrorKind` | error.rs | tool.rs |
| `RigInferError` | error.rs | stream.rs (consume_rig_stream), mod.rs (infer methods), executor/infer.rs |
| `StreamChunk` | stream.rs | mod.rs (infer_stream), runtime/executor/infer.rs, display/, TUI |
| `StreamResult` | stream.rs | mod.rs (infer_stream), executor/infer.rs, agent loop |
| `consume_rig_stream` | stream.rs (pub(super)) | mod.rs only |
| `NikaMcpToolDef` | tool.rs | agent loop (providers.rs, thinking.rs) |
| `NikaMcpTool` | tool.rs | agent loop, provider/mod.rs re-export |
| `AgentMediaStaging` | tool.rs | agent loop (mod.rs) |
| `RigProvider` | mod.rs (stays) | everywhere — enum + all impls stay together |
| `InferOptions` | mod.rs (stays) | executor/infer.rs, agent loop |

---

## Dependency Graph (submodule → what it imports)

```
error.rs:
  └── std::fmt, thiserror (no nika imports)

stream.rs:
  ├── error::RigInferError (super::error)
  ├── crate::util::STREAM_CHUNK_TIMEOUT
  ├── rig::streaming, rig::completion
  ├── futures::StreamExt
  └── tokio (mpsc, timeout)

tool.rs:
  ├── error::McpToolError (super::error)
  ├── crate::mcp::McpClient
  ├── rig::tool::{ToolDyn, ToolError}
  ├── rig::completion::ToolDefinition
  └── dashmap (for AgentMediaStaging)

mod.rs:
  ├── error::{McpToolError, RigInferError}
  ├── stream::{StreamChunk, StreamResult, consume_rig_stream}
  ├── tool::{NikaMcpTool, NikaMcpToolDef, AgentMediaStaging}
  ├── rig::providers::{anthropic, openai, mistral, ...}
  ├── rig::client::{CompletionClient, ProviderClient}
  ├── crate::error_domains::ProviderError
  └── crate::mcp::McpClient
```

**No circular dependencies.** The DAG is: error ← stream, error ← tool, all three ← mod.rs.

---

## Risk Checklist

- [ ] **`#[cfg(feature = "native-inference")]` blocks**: Lines 33-34, 1617-1655 in original. These STAY in mod.rs (they're inside RigProvider impl). No risk.
- [ ] **Private helper functions**: `is_reasoning_model()`, `supports_native_structured_output()`, `build_response_format_params()` — these are used by mod.rs only, stay there.
- [ ] **Re-exports from provider/mod.rs**: Line 54 `pub use rig::{NikaMcpTool, RigProvider, StreamResult};` — must still resolve after split.
- [ ] **`BoxFuture` type alias**: Used only in tool.rs ToolDyn impl — moves with it.
- [ ] **Tests reference private items**: Tests use `use super::*;` — verify they can still access everything via mod.rs re-exports.

---

## Commit Strategy (7 commits)

```
1. refactor(provider): rename rig.rs → rig/mod.rs (directory module)
2. refactor(provider): extract error types to rig/error.rs
3. refactor(provider): extract streaming types to rig/stream.rs
4. refactor(provider): extract NikaMcpTool to rig/tool.rs
5. refactor(provider): extract tests to rig/tests.rs
6. refactor(provider): clean up rig/mod.rs imports after split
7. chore(provider): update module documentation
```

Each commit must:
- Pass `cargo check -p nika-engine`
- Pass `cargo test -p nika-engine --lib`
- End with co-author lines

---

## Execution Methodology

For EACH phase:
1. **Read** the lines to move (verify content matches this plan)
2. **Create** the new file with the moved content
3. **Remove** the content from mod.rs
4. **Add** the `mod` declaration + `pub use` re-exports
5. **Fix** imports in the new file (`use super::` for sibling types)
6. **Compile**: `cargo check -p nika-engine`
7. **Test**: `cargo test -p nika-engine --lib -- provider::rig`
8. **Commit**

**NEVER skip the compile check between phases.** A compilation failure cascades and becomes much harder to diagnose.

---

## Final Verification

After all 7 commits:
```bash
# Full workspace
cd tools && cargo test --workspace --lib
cd tools && cargo clippy --workspace -- -D warnings

# Verify line counts
wc -l tools/nika-engine/src/provider/rig/*.rs
# Expected:
#   mod.rs    ~1630
#   error.rs  ~90
#   stream.rs ~215
#   tool.rs   ~245
#   tests.rs  ~1460
#   Total:    ~3640 (same as original)

# Verify test count unchanged
# Expected: 8595 passed
```

---

## Priority Matrix

| Phase | Risk | Effort | Dependencies |
|-------|------|--------|-------------|
| 1. Directory rename | ZERO | 5 min | None |
| 2. error.rs | LOW | 15 min | Phase 1 |
| 3. stream.rs | MEDIUM | 30 min | Phase 2 (needs RigInferError) |
| 4. tool.rs | MEDIUM | 30 min | Phase 2 (needs McpToolError) |
| 5. tests.rs | LOW | 15 min | Phases 2-4 |
| 6. Cleanup | LOW | 15 min | Phase 5 |
| 7. Docs | ZERO | 5 min | Phase 6 |

**Total estimated effort:** ~2 hours (conservative)
