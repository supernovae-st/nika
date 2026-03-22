# Nika Crate Split — Master Plan

**Date**: 2026-03-22
**Status**: In Progress — Session 1 complete (AST blocker resolved)
**Estimated duration**: 25-35 hours across 5-7 sessions
**Approach**: Autonomous execution with code review checkpoints

## Session 1 Progress (2026-03-22)

### Completed
- [x] Phase 0: Workspace setup (tools/Cargo.toml, unified deps across 4 crates)
- [x] Phase 0: AST/binding dedup (11 files deleted, 5084 lines eliminated)
- [x] Phase 0: Bug fixes ported to nika-core (Bug 30: |length unicode, Bug 46: |sort numeric)
- [x] Phase 1: nika-event extracted (4278 lines, 128 tests, EventError defined)

### Blocked — AST Divergence
nika's AST (raw/, analyzed/, analyzer/) has DIVERGED from nika-core's copies:
- Field renames: `thinking` → `extended_thinking`, `working_dir` → `cwd`, `max_iterations` → `max_turns`
- Extra fields in nika: `response_format`, `guardrails`, `tool_choice`
- `Agent(Spanned<>)` vs `Agent(Box<Spanned<>>)` (boxing difference)

This divergence blocks extraction of: nika-dag, nika-mcp, nika-media, nika-runtime.
All these modules depend on nika's local AST types, creating circular deps if extracted.

### AST Divergence — RESOLVED
Synced nika-core's AST with nika's version (nika is source of truth):
- Copied raw/, analyzed/, analyzer/ from nika to nika-core
- Added guardrails.rs, completion.rs, schema.rs to nika-core
- Added regex dependency to nika-core
- Deleted 20 more files from nika, replaced with re-exports
- Added From<CoreError> for NikaError conversion
- Total: 31 duplicate files eliminated, ~15k lines removed from nika

**Test counts after full dedup:**
- nika-core: 689 tests
- nika-event: 128 tests
- nika: 6288 tests
- Total: 7105 (up from 7045 due to regression tests added)

### Session 1 Continued — nika-mcp + nika-media extracted
- [x] Phase 3: nika-mcp extracted (7490 lines, 272 tests, McpError, isolates rmcp)
- [x] Phase 4: nika-media extracted (3469 lines, 120 tests, CAS + processor)
- [x] Phase 6: nika-tui attempted — BLOCKED by circular dependency
  - nika-tui → nika (for runtime/provider) → nika-tui = CYCLE
  - Cargo does NOT allow cycles even with feature gating
  - **Requires nika-runtime extraction first** (breaks the cycle)

**Final test counts:**
- nika-core: 689 tests
- nika-event: 128 tests
- nika-mcp: 272 tests
- nika-media: 120 tests
- nika: 5895 tests
- Total: 7104

### Extraction Order (revised, based on learnings)
1. ~~Phase 0: Workspace + dedup~~ DONE
2. ~~Phase 1: nika-event~~ DONE
3. ~~Phase 3: nika-mcp~~ DONE
4. ~~Phase 4: nika-media~~ DONE
5. Phase 2: nika-dag — BLOCKED (depends on Workflow/TaskAction runtime types)
6. **Phase 5: nika-runtime** — CRITICAL PATH (unblocks dag + tui)
7. Phase 6: nika-tui — BLOCKED until nika-runtime exists
8. Phase 7-8: Cleanup + release

## Target Architecture

```
nika-core         (exists, expand)    ~20k lines   Zero-dep types, AST phases 1+2, binding types, catalogs
nika-event        (NEW)               ~4k lines    EventLog, EventKind, TraceWriter
nika-dag          (NEW)               ~4k lines    DAG validation, cycle detection, petgraph
nika-mcp          (NEW)               ~9k lines    MCP client, pool, rmcp adapter, retry
nika-media        (NEW)               ~30k lines   CAS store + 26 media builtin tools + fetch extraction
nika-runtime      (NEW)               ~70k lines   Runner, executor, provider (rig-core), agent loop, binding resolve, store, tools
nika-tui          (NEW)               ~89k lines   Full TUI application (ratatui)
nika-lsp-core     (exists)            ~9k lines    Protocol-agnostic LSP intelligence
nika-lsp          (exists)            ~2k lines    Standalone LSP binary
nika              (slim binary)       ~25k lines   CLI, config, display, init, secrets, registry, main.rs
```

## Dependency Graph (new crate topology)

```
                    nika-core
                   /    |    \
            nika-event  |  nika-dag
                |       |       |
              nika-mcp  |       |
                |       |       |
             nika-media |       |
                 \      |      /
                  nika-runtime
                   /         \
             nika-tui       nika (binary)
                           /
                     nika-lsp-core
                          |
                       nika-lsp
```

## Error Architecture

Each crate defines its own error type. The binary's NikaError wraps them all via `From<*>`.
NIKA-XXX error codes are preserved via a shared trait in nika-core.

```rust
// nika-core: shared trait
pub trait ErrorCode {
    fn code(&self) -> u16;
    fn message(&self) -> &str;
}

// Each crate: its own error
// nika-event:   EventError    (codes: none needed, or a few)
// nika-dag:     DagError      (codes: 020-029, 070-089)
// nika-mcp:     McpError      (codes: 100-109)
// nika-media:   MediaError    (codes: 251-259, 280-297)
// nika-runtime: RuntimeError  (codes: 030-069, 090-099, 110-179, 200-219, 270-279, 300-309)
// nika:         NikaError     (wraps all + codes 000-019, 130-139, 140-151, 160-164, 260-269)
```

## Workspace Configuration

```toml
# /tools/Cargo.toml (NEW workspace root)
[workspace]
resolver = "2"
members = [
    "nika",
    "nika-core",
    "nika-event",
    "nika-dag",
    "nika-mcp",
    "nika-media",
    "nika-runtime",
    "nika-tui",
    "nika-lsp-core",
    "nika-lsp",
]

[workspace.package]
version = "0.37.0"
edition = "2021"
authors = ["Thibaut MÉLEN <thibaut@supernovae.studio>", "SuperNovae Studio <contact@supernovae.studio>"]
license = "AGPL-3.0-or-later"
repository = "https://github.com/supernovae-st/nika"
rust-version = "1.86"

[workspace.dependencies]
# Internal crates
nika-core = { path = "nika-core", version = "0.37.0" }
nika-event = { path = "nika-event", version = "0.37.0" }
nika-dag = { path = "nika-dag", version = "0.37.0" }
nika-mcp = { path = "nika-mcp", version = "0.37.0" }
nika-media = { path = "nika-media", version = "0.37.0" }
nika-runtime = { path = "nika-runtime", version = "0.37.0" }
nika-tui = { path = "nika-tui", version = "0.37.0" }
nika-lsp-core = { path = "nika-lsp-core", version = "0.37.0" }

# Shared deps (unified versions)
tokio = { version = "1.49", features = ["rt-multi-thread", "macros", "process", "sync", "time", "fs", "signal"] }
tokio-util = "0.7"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
serde-saphyr = "0.0.20"
thiserror = "1.0"
miette = { version = "7.6", features = ["fancy"] }
tracing = "0.1"
indexmap = "2.7"
rustc-hash = "2.1"
dashmap = "6.1"
parking_lot = "0.12"
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1.0", features = ["v4"] }
semver = "1.0"
regex = "1.11"
async-trait = "0.1"
petgraph = { version = "0.6", features = ["serde-1"] }
reqwest = { version = "0.12", default-features = false, features = ["json", "stream", "rustls-tls", "gzip", "brotli", "deflate"] }
rmcp = { version = "0.16", features = ["client", "transport-child-process"] }
rig-core = { version = "0.32", features = ["rmcp"] }
blake3 = { version = "1.8", features = ["mmap"] }
camino = "1.1"
globset = "0.4"
ignore = "0.4"
base64 = "0.22"
colored = "2.1"
clap = { version = "4.6", features = ["derive"] }
image = { version = "0.25", default-features = false, features = ["png", "jpeg", "webp", "gif"] }

# Dev deps
tempfile = "3.27"
insta = { version = "1.34", features = ["yaml"] }
pretty_assertions = "1.4"
proptest = "1.4"
```

---

## Phase 0 — Workspace Setup + Deduplication

**Goal**: Create Cargo workspace, fix nika↔nika-core duplication
**Duration**: 2-3 hours
**Prerequisite**: Commit any uncommitted changes first

### Task 0.1 — Stash/commit pending changes
```bash
cd /Users/thibaut/dev/supernovae/nika
git stash  # or commit if they're ready
```

### Task 0.2 — Create workspace Cargo.toml
- [ ] Create `/tools/Cargo.toml` with workspace definition (see above)
- [ ] Update each crate's Cargo.toml to use `workspace = true` for shared fields
- [ ] Update each crate's deps to use `workspace = true` where applicable
- [ ] Verify: `cargo check --workspace`

**Files to create:**
- `tools/Cargo.toml`

**Files to modify:**
- `tools/nika/Cargo.toml` — add `version.workspace = true`, `edition.workspace = true`, etc. + workspace dep references
- `tools/nika-core/Cargo.toml` — same
- `tools/nika-lsp-core/Cargo.toml` — same
- `tools/nika-lsp/Cargo.toml` — same

### Task 0.3 — Fix AST duplication (nika re-exports from nika-core)

Currently nika has FULL COPIES of nika-core's ast/ modules. Replace with re-exports.

**Files to DELETE from `tools/nika/src/ast/`:**
- `raw/` (entire directory — parser.rs, workflow.rs, task.rs, action.rs, mcp.rs, mod.rs)
- `analyzed/` (entire directory — ids.rs, task.rs, workflow.rs, mod.rs)
- `analyzer/` (entire directory — analyze.rs, errors.rs, suggestions.rs, mod.rs)
- `schema.rs`
- `budget.rs`

**Files to KEEP in `tools/nika/src/ast/` (runtime-specific):**
- `mod.rs` (REWRITE to re-export from nika-core + declare local modules)
- `lower.rs`
- `workflow.rs` (runtime Workflow/Task types)
- `action.rs` (TaskAction, InferParams, ExecParams, FetchParams)
- `invoke.rs` (InvokeParams)
- `agent.rs` (AgentParams, ToolChoice)
- `guardrails.rs`
- `completion.rs`
- `loader.rs`
- `include_loader.rs`
- `import_loader.rs`
- `pkg_resolver.rs`
- `schema_validator.rs`
- `skill_def.rs`
- `content.rs` (ONLY if it has runtime-specific ContentPart; check if nika-core's version suffices)
- `decompose.rs` (ONLY if runtime adds beyond nika-core's version)
- `logging.rs`, `artifact.rs`, `limits.rs`, `context.rs`, `agent_def.rs`, `include.rs`, `output.rs`, `structured.rs`
  → CHECK each: if identical to nika-core version → DELETE and re-export
  → if has NikaError validation → KEEP for now, migrate in Phase 5

**New `tools/nika/src/ast/mod.rs`:**
```rust
// Re-export all core AST types
pub use nika_core::ast::*;
// Re-export analyzer
pub use nika_core::ast::analyzer;
pub use nika_core::ast::raw;
pub use nika_core::ast::analyzed;

// Runtime-specific modules (depend on NikaError, serde Deserialize, Arc)
pub mod action;
pub mod agent;
mod completion;
mod guardrails;
pub mod invoke;
pub mod loader;
pub mod include_loader;
pub mod import_loader;
pub mod lower;
pub mod pkg_resolver;
pub mod schema_validator;
pub mod skill_def;
pub mod workflow;

// Re-export runtime types
pub use action::{TaskAction, InferParams, ExecParams, FetchParams};
pub use agent::{AgentParams, ToolChoice};
pub use invoke::InvokeParams;
pub use workflow::{Workflow, Task, McpConfigInline};
```

### Task 0.4 — Fix binding duplication

**Files to DELETE from `tools/nika/src/binding/`:**
- `types.rs`
- `entry.rs`
- `transform.rs`

**Files to KEEP:**
- `mod.rs` (REWRITE)
- `resolve.rs`
- `template.rs`
- `jsonpath.rs`
- `mention.rs`
- `validate.rs`

**New `tools/nika/src/binding/mod.rs`:**
```rust
// Re-export core binding types
pub use nika_core::binding::*;

// Runtime-specific modules
pub mod jsonpath;
pub mod mention;
pub(crate) mod resolve;
pub(crate) mod template;
pub(crate) mod validate;

// Re-export runtime types
pub use resolve::{LazyBinding, ResolvedBindings};
pub use template::resolve as template_resolve;
pub use mention::{Mention, MentionResolutionError, ResolvedMention};
```

### Task 0.5 — Fix all broken imports

After deleting duplicated files, many `use crate::ast::*` and `use crate::binding::*` imports will
reference types that now come from nika-core via re-exports. Most should work automatically since
the re-exports preserve the same paths. Fix any that don't.

**Verification:**
```bash
cd tools && cargo check -p nika 2>&1 | head -100
# Fix errors iteratively until clean
cargo test --lib -p nika -- --test-threads=4
cargo test -p nika-core
```

### Task 0.6 — Checkpoint commit
```
refactor(ast): replace duplicated modules with nika-core re-exports

Remove ~15 duplicated files from nika that were identical copies of
nika-core modules. AST raw/analyzed/analyzer, binding types/entry/transform
now re-exported from nika-core instead.
```

**Verification checklist:**
- [ ] `cargo check --workspace` passes
- [ ] `cargo test --lib -p nika` passes (7400+ tests)
- [ ] `cargo test -p nika-core` passes
- [ ] `cargo clippy --workspace -- -D warnings` passes
- [ ] No duplicate type definitions remain

---

## Phase 1 — Extract nika-event

**Goal**: Extract event module into its own crate
**Duration**: 2 hours
**Depends on**: Phase 0

### Task 1.1 — Create nika-event crate

**Create directory:** `tools/nika-event/src/`

**Create `tools/nika-event/Cargo.toml`:**
```toml
[package]
name = "nika-event"
description = "Event log and trace system for Nika workflows"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true
rust-version.workspace = true

[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
chrono = { workspace = true }
uuid = { workspace = true }
tracing = { workspace = true }
tokio = { workspace = true, features = ["sync"] }
thiserror = { workspace = true }
```

### Task 1.2 — Move event files

**Move** `tools/nika/src/event/` → `tools/nika-event/src/`

Files to move:
- `mod.rs` → `lib.rs` (rename + adjust)
- `types.rs` → `types.rs`
- `log.rs` → `log.rs`
- `trace.rs` → `trace.rs`

### Task 1.3 — Define EventError

In `tools/nika-event/src/error.rs`:
```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EventError {
    #[error("Failed to write trace: {0}")]
    TraceWrite(#[from] std::io::Error),

    #[error("Failed to serialize event: {0}")]
    Serialization(#[from] serde_json::Error),
}
```

Replace `crate::error::Result` usage in trace.rs with `Result<_, EventError>`.

### Task 1.4 — Update nika to depend on nika-event

In `tools/nika/Cargo.toml`:
```toml
nika-event = { workspace = true }
```

Replace `tools/nika/src/event/` with:
```rust
// src/event/mod.rs (or just src/event.rs)
pub use nika_event::*;
```

### Task 1.5 — Fix all `use crate::event::*` imports

These files import from event (from agent analysis):
- `runtime/runner.rs`
- `runtime/executor/verbs.rs`
- `runtime/rig_agent_loop/mod.rs`
- `runtime/rig_agent_loop/providers.rs`
- `runtime/rig_agent_loop/streaming.rs`
- `runtime/rig_agent_loop/thinking.rs`
- `runtime/rig_agent_loop/chat.rs`
- `runtime/spawn.rs`
- `runtime/artifact_processor.rs`
- `runtime/builtin/rig_adapter.rs`
- `mcp/pool.rs`
- `mcp/client.rs`
- `tui/` (multiple files)
- `display/` (renderer.rs)

Most should work via the re-export. Verify with `cargo check`.

### Task 1.6 — Add NikaError conversion

In `tools/nika/src/error.rs`, add:
```rust
impl From<nika_event::EventError> for NikaError {
    fn from(e: nika_event::EventError) -> Self {
        // Map to appropriate NIKA-XXX code
        NikaError::TraceError { source: e.to_string() }
    }
}
```

### Task 1.7 — Checkpoint commit + verify
```
refactor(event): extract nika-event crate

Move event module (EventLog, EventKind, TraceWriter) to standalone
nika-event crate. Zero internal dependencies, clean leaf module.
```

**Verification:**
- [ ] `cargo check --workspace`
- [ ] `cargo test --lib -p nika`
- [ ] `cargo test -p nika-event`
- [ ] `cargo clippy --workspace -- -D warnings`

---

## Phase 2 — Extract nika-dag

**Goal**: Extract DAG validation into its own crate
**Duration**: 2 hours
**Depends on**: Phase 0 (needs nika-core AST types)

### Task 2.1 — Create nika-dag crate

**Create `tools/nika-dag/Cargo.toml`:**
```toml
[package]
name = "nika-dag"
description = "DAG validation and cycle detection for Nika workflows"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true
rust-version.workspace = true

[dependencies]
nika-core = { workspace = true }
petgraph = { workspace = true }
rustc-hash = { workspace = true }
indexmap = { workspace = true }
thiserror = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
tracing = { workspace = true }
```

### Task 2.2 — Move dag files

**Move** `tools/nika/src/dag/` → `tools/nika-dag/src/`

Files:
- `mod.rs` → `lib.rs`
- `flow.rs` → `flow.rs`
- `indexed.rs` → `indexed.rs`
- `validate.rs` → `validate.rs`
- `stable.rs` → `stable.rs` (if exists)

### Task 2.3 — Define DagError

```rust
use thiserror::Error;
use nika_core::ast::analyzed::TaskId;

#[derive(Debug, Error)]
pub enum DagError {
    #[error("[NIKA-020] Cyclic dependency detected: {cycle}")]
    CyclicDependency { cycle: String },

    #[error("[NIKA-021] Unknown task in depends_on: {task_name}")]
    UnknownDependency { task_name: String },

    #[error("[NIKA-022] DAG validation failed: {reason}")]
    ValidationFailed { reason: String },

    // ... migrate relevant NIKA-02x variants from NikaError
}
```

### Task 2.4 — Replace NikaError with DagError in dag module

Replace all `use crate::error::NikaError` with local `DagError`.
Replace `crate::binding::validate_*` calls with nika-core equivalents.
Replace `crate::util::intern` with either inline or nika-core re-export.

### Task 2.5 — Update nika crate

Add dep, create re-export module, add `From<DagError> for NikaError`.

### Task 2.6 — Checkpoint commit + verify
```
refactor(dag): extract nika-dag crate

Move DAG validation (cycle detection, flow computation, StableDag)
to standalone nika-dag crate. Depends only on nika-core + petgraph.
```

**Verification:**
- [ ] `cargo check --workspace`
- [ ] `cargo test --lib -p nika`
- [ ] `cargo test -p nika-dag`

---

## Phase 3 — Extract nika-mcp

**Goal**: Extract MCP client into its own crate
**Duration**: 3-4 hours
**Depends on**: Phase 1 (needs nika-event)

### Task 3.1 — Create nika-mcp crate

**Create `tools/nika-mcp/Cargo.toml`:**
```toml
[package]
name = "nika-mcp"
description = "MCP client, connection pool, and rmcp adapter for Nika"
version.workspace = true
edition.workspace = true
# ...workspace fields...

[dependencies]
nika-core = { workspace = true }
nika-event = { workspace = true }
rmcp = { workspace = true }
tokio = { workspace = true }
tokio-util = { workspace = true }
backon = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
dashmap = { workspace = true }
tracing = { workspace = true }
thiserror = { workspace = true }
serde-saphyr = { workspace = true }
```

### Task 3.2 — Move mcp files

**Move** `tools/nika/src/mcp/` → `tools/nika-mcp/src/`

Files (12 total):
- `mod.rs` → `lib.rs`
- `client.rs`, `pool.rs`, `rmcp_adapter.rs`, `retry.rs`, `validation.rs`
- `types.rs`, `nika_config.rs`
- Any test files

### Task 3.3 — Define McpError

```rust
#[derive(Debug, Error)]
pub enum McpError {
    #[error("[NIKA-100] MCP connection failed: {server}: {reason}")]
    ConnectionFailed { server: String, reason: String },

    #[error("[NIKA-101] MCP tool call failed: {tool}: {reason}")]
    ToolCallFailed { tool: String, reason: String },

    #[error("[NIKA-102] MCP timeout: {server}")]
    Timeout { server: String },

    #[error("[NIKA-103] MCP protocol error: {reason}")]
    ProtocolError { reason: String },

    // ... migrate NIKA-10x variants
}
```

### Task 3.4 — Replace NikaError references in mcp module

Replace all `crate::error::NikaError` with `McpError`.
Replace `crate::event::*` with `nika_event::*`.
Replace `crate::ast::McpConfigInline` — this type needs to either:
- Move to nika-core (if it's just a data type)
- Or be defined in nika-mcp
- Or nika-mcp accepts a generic config

### Task 3.5 — Replace `crate::util::*` constants

Move MCP-relevant constants (CONNECT_TIMEOUT, MCP_CALL_TIMEOUT, RECONNECT_TIMEOUT)
to nika-mcp directly.

### Task 3.6 — Update nika crate + From conversion

### Task 3.7 — Checkpoint commit + verify
```
refactor(mcp): extract nika-mcp crate

Move MCP client (McpClient, McpClientPool, rmcp adapter, retry logic)
to standalone nika-mcp crate. Isolates rmcp (170 transitive deps).
```

**Verification:**
- [ ] `cargo check --workspace`
- [ ] `cargo test --lib -p nika`
- [ ] `cargo test -p nika-mcp`

---

## Phase 4 — Extract nika-media

**Goal**: Extract CAS store + all media/fetch builtin tools
**Duration**: 4-5 hours (largest feature-gated surface)
**Depends on**: Phase 0 (nika-core)

### Task 4.1 — Create nika-media crate

**Create `tools/nika-media/Cargo.toml`:**
```toml
[package]
name = "nika-media"
description = "Content-addressable storage and media tools for Nika"
# ...workspace fields...

[features]
default = ["media-core", "fetch-extract", "fetch-article", "fetch-feed",
           "media-chart", "media-phash", "media-pdf", "media-iqa", "media-qr", "media-compression"]
media-core = ["media-thumbnail", "media-metadata", "media-optimize", "media-svg"]
media-thumbnail = ["dep:fast_image_resize", "dep:image"]
# ... ALL media/fetch feature flags move here from nika/Cargo.toml ...

[dependencies]
nika-core = { workspace = true }
tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
tracing = { workspace = true }
blake3 = { workspace = true }
base64 = { workspace = true }
rayon = "1.10"
infer = "0.19"
mime = "0.3"
mime_guess = "2.0"
bytes = "1"
imagesize = "0.14"
thumbhash = "0.1"
color-thief = "0.2"
reflink-copy = "0.1"
camino = { workspace = true }
async-trait = { workspace = true }
# ... all optional media deps ...
```

### Task 4.2 — Move media files

**Move** `tools/nika/src/media/` → `tools/nika-media/src/media/`
**Move** `tools/nika/src/runtime/builtin/media/` → `tools/nika-media/src/tools/`

Structure:
```
nika-media/src/
├── lib.rs
├── error.rs          (MediaError — already exists)
├── media/
│   ├── cas.rs        (CasStore)
│   ├── detect.rs     (content type detection)
│   ├── processor.rs  (media processing)
│   └── ...
├── tools/
│   ├── mod.rs        (MediaToolRouter)
│   ├── import.rs     (nika:import)
│   ├── thumbnail.rs  (nika:thumbnail)
│   └── ... (all 26 tools)
└── safety.rs         (decode_image_safe, sanitize_svg)
```

### Task 4.3 — Break mcp::ContentBlock dependency

The media processor.rs imports `crate::mcp::types::ContentBlock`.
**Solution**: Accept `serde_json::Value` or `Vec<u8>` at the boundary instead.
The conversion from ContentBlock happens in nika-runtime, not in nika-media.

### Task 4.4 — Define MediaError (expand existing)

MediaError already exists in `media/error.rs`. Expand it with the NIKA-25x, 28x, 29x codes.
Remove MediaError from NikaError's `#[from]` and add manual `From<MediaError> for NikaError`.

### Task 4.5 — Move feature flags from nika to nika-media

In `tools/nika/Cargo.toml`:
- Remove all `media-*` and `fetch-*` feature definitions
- Remove all optional media deps
- Add: `nika-media = { workspace = true }`
- Forward features: `media-core = ["nika-media/media-core"]` etc.

### Task 4.6 — Update runtime/builtin/ to use nika-media

The `runtime/builtin/router.rs` and `runtime/builtin/rig_adapter.rs` need to import
media tools from `nika_media` instead of `crate::runtime::builtin::media`.

### Task 4.7 — Checkpoint commit + verify
```
refactor(media): extract nika-media crate

Move CAS store, media detection, and 26 builtin media/fetch tools
to standalone nika-media crate. All media-*/fetch-* feature flags
now owned by nika-media. Isolates image/resvg/oxipng dependencies.
```

**Verification:**
- [ ] `cargo check --workspace`
- [ ] `cargo check --workspace --no-default-features`
- [ ] `cargo test --lib -p nika`
- [ ] `cargo test -p nika-media`
- [ ] All feature combinations: `cargo check -p nika-media --features media-thumbnail`

---

## Phase 5 — Extract nika-runtime

**Goal**: Extract the execution engine as an embeddable crate (with rig-core inside)
**Duration**: 6-8 hours (the biggest and hardest extraction)
**Depends on**: Phases 1-4 (needs nika-event, nika-dag, nika-mcp, nika-media)

### Task 5.0 — Design decisions

rig-core stays INSIDE nika-runtime (per Thibaut's decision).
This means nika-runtime includes:
- Runtime engine (runner, executor, verbs)
- Provider layer (rig-core integration, RigProvider)
- Agent loop (rig_agent_loop)
- Binding resolution (resolve, template, jsonpath)
- Store (RunContext, TaskResult)
- Tools (file tools, submit tool)
- IO (atomic writes, artifact writer)
- Security (blocklist, command validation)

### Task 5.1 — Create nika-runtime crate

**Create `tools/nika-runtime/Cargo.toml`:**
```toml
[package]
name = "nika-runtime"
description = "Embeddable workflow execution engine for Nika"
# ...workspace fields...

[features]
default = ["native-inference"]
native-inference = ["dep:mistralrs", "dep:async-stream"]

[dependencies]
nika-core = { workspace = true }
nika-event = { workspace = true }
nika-dag = { workspace = true }
nika-mcp = { workspace = true }
nika-media = { workspace = true }
# Async
tokio = { workspace = true }
tokio-util = { workspace = true }
async-trait = { workspace = true }
# Provider
rig-core = { workspace = true }
mistralrs = { version = "0.7", optional = true }
async-stream = { version = "0.3", optional = true }
# HTTP (fetch verb)
reqwest = { workspace = true }
url = "2"
# Data
serde = { workspace = true }
serde_json = { workspace = true }
serde-saphyr = { workspace = true }
# Validation
jsonschema = "0.26"
regex = { workspace = true }
# Concurrency
dashmap = { workspace = true }
parking_lot = { workspace = true }
# Utilities
uuid = { workspace = true }
chrono = { workspace = true }
indexmap = { workspace = true }
rustc-hash = { workspace = true }
camino = { workspace = true }
globset = { workspace = true }
ignore = { workspace = true }
shlex = "1.3"
unicode-normalization = "0.1"
smallvec = "1.13"
humantime = "2.1"
base64 = { workspace = true }
tracing = { workspace = true }
thiserror = { workspace = true }
# Native deps
sha2 = "0.10"
nix = { version = "0.29", features = ["fs", "signal", "process"] }
serde_json_path = "0.7"
xxhash-rust = { version = "0.8", features = ["xxh3"] }
futures = { version = "0.3.32", default-features = false, features = ["alloc"] }
rand = "0.8"
```

### Task 5.2 — Move runtime files

**Move from `tools/nika/src/` to `tools/nika-runtime/src/`:**

```
runtime/          → runtime/     (runner.rs, context.rs, boot.rs, policy.rs, security.rs, etc.)
  executor/       → executor/    (mod.rs, verbs.rs, decompose.rs, extract.rs)
  rig_agent_loop/ → agent_loop/  (rename for clarity)
  builtin/        → builtin/     (core tools only — media tools already in nika-media)
binding/          → binding/     (resolve.rs, template.rs, jsonpath.rs, mention.rs, validate.rs)
store/            → store/       (run_context.rs, task_result.rs)
tools/            → tools/       (file tools: read, write, edit, glob, grep, submit)
io/               → io/          (atomic writes, security, writer)
provider/         → provider/    (rig.rs, cost.rs, native/)
core/             → core/        (backend, storage, models — native inference support)
```

### Task 5.3 — Break the binding↔store cycle

**Current cycle:** binding::resolve → store::RunContext, store::run_context → binding::jsonpath

**Solution:** Define trait in nika-runtime:
```rust
// nika-runtime/src/store/traits.rs
pub trait TaskResultStore: Send + Sync {
    fn get_result(&self, task_id: &str) -> Option<&serde_json::Value>;
    fn query_jsonpath(&self, task_id: &str, path: &str) -> Option<serde_json::Value>;
}
```

RunContext implements this trait. binding::resolve accepts `&dyn TaskResultStore` instead
of `&RunContext` directly. Since both modules live in nika-runtime, the cycle is broken
at the module level even though they're in the same crate.

### Task 5.4 — Remove display logic from Runner

**Currently in runner.rs:** Direct usage of `crate::display::CliRenderer`, `print_done_summary`, etc.

**Solution:** Runner emits events via EventLog. Display is handled by the consumer (CLI/TUI).

Remove from `runner.rs`:
- `use crate::display::*` lines
- All `CliRenderer` calls
- `print_done_summary` calls
- `print_static_dag` calls
- `colored` string formatting

Replace with `EventKind` emissions:
- `EventKind::WorkflowStarted`
- `EventKind::TaskStarted { task_id }`
- `EventKind::TaskCompleted { task_id, result }`
- `EventKind::WorkflowCompleted { summary }`

### Task 5.5 — Define RuntimeError

```rust
#[derive(Debug, Error)]
pub enum RuntimeError {
    // Execution errors (030-039)
    #[error("[NIKA-030] Provider error: {reason}")]
    Provider { reason: String },

    #[error("[NIKA-031] Model not found: {model}")]
    ModelNotFound { model: String },

    // Template/binding errors (040-049)
    #[error("[NIKA-040] Template error: {reason}")]
    Template { reason: String },

    // Security errors (050-059)
    #[error("[NIKA-050] Command blocked: {command}")]
    CommandBlocked { command: String },

    // ... all runtime NIKA-XXX codes
    // MCP errors (delegated)
    #[error(transparent)]
    Mcp(#[from] nika_mcp::McpError),

    // Media errors (delegated)
    #[error(transparent)]
    Media(#[from] nika_media::MediaError),

    // DAG errors (delegated)
    #[error(transparent)]
    Dag(#[from] nika_dag::DagError),

    // Event errors (delegated)
    #[error(transparent)]
    Event(#[from] nika_event::EventError),
}
```

### Task 5.6 — Move AST runtime types to nika-runtime

These AST types depend on NikaError/RuntimeError and belong in nika-runtime:
- `ast/lower.rs` → `nika-runtime/src/ast/lower.rs`
- `ast/workflow.rs` → `nika-runtime/src/ast/workflow.rs`
- `ast/action.rs` → `nika-runtime/src/ast/action.rs`
- `ast/invoke.rs` → `nika-runtime/src/ast/invoke.rs`
- `ast/agent.rs` → `nika-runtime/src/ast/agent.rs`
- `ast/guardrails.rs` → `nika-runtime/src/ast/guardrails.rs`
- `ast/completion.rs` → `nika-runtime/src/ast/completion.rs`
- `ast/loader.rs` → `nika-runtime/src/ast/loader.rs`
- `ast/include_loader.rs` → `nika-runtime/src/ast/include_loader.rs`
- `ast/import_loader.rs` → `nika-runtime/src/ast/import_loader.rs`
- `ast/pkg_resolver.rs` → `nika-runtime/src/ast/pkg_resolver.rs`
- `ast/schema_validator.rs` → `nika-runtime/src/ast/schema_validator.rs`
- `ast/skill_def.rs` → `nika-runtime/src/ast/skill_def.rs`

Also check: content.rs, decompose.rs, logging.rs, artifact.rs, limits.rs, output.rs, structured.rs
— if these have NikaError-based validation, they move to nika-runtime.
— if they're pure data types, they stay in nika-core.

### Task 5.7 — Move util constants

Move relevant constants from `tools/nika/src/util/` to nika-runtime:
- `EXEC_TIMEOUT`, `FETCH_TIMEOUT`, `CONNECT_TIMEOUT`, `INVOKE_TASK_DEADLINE`
- `STREAM_CHUNK_TIMEOUT`, `DECOMPOSE_TIMEOUT`, `REDIRECT_LIMIT`
- `intern` (string interner) — either move or make public in nika-core

### Task 5.8 — Update nika binary crate

After extraction, nika's `src/` becomes:
```
src/
├── main.rs
├── lib.rs           (public API re-exports from nika-runtime + others)
├── error.rs         (NikaError wraps RuntimeError + all sub-errors)
├── config.rs        (NikaConfig, settings)
├── display/         (CLI rendering — consumed events from runner)
├── cli/             (clap subcommands)
├── init/            (nika init templates)
├── new/             (nika new wizard)
├── secrets/         (keychain, daemon)
├── registry/        (package registry client)
└── util/            (remaining utils)
```

### Task 5.9 — Checkpoint commit + verify
```
refactor(runtime): extract nika-runtime crate

Move execution engine (Runner, TaskExecutor, RigAgentLoop, provider,
binding resolution, store, file tools, security) to embeddable
nika-runtime crate. Includes rig-core for LLM inference.
Removes display coupling from Runner (event-driven instead).
```

**Verification:**
- [ ] `cargo check --workspace`
- [ ] `cargo test --lib -p nika-runtime`
- [ ] `cargo test --lib -p nika`
- [ ] `cargo test -p nika-core`
- [ ] `cargo test -p nika-event`
- [ ] `cargo test -p nika-dag`
- [ ] `cargo test -p nika-mcp`
- [ ] `cargo test -p nika-media`
- [ ] `cargo clippy --workspace -- -D warnings`
- [ ] Runner can be instantiated from nika-runtime without CLI deps

---

## Phase 6 — Extract nika-tui

**Goal**: Extract TUI into its own crate
**Duration**: 4-5 hours
**Depends on**: Phase 5 (needs nika-runtime)

### Task 6.1 — Create nika-tui crate

**Create `tools/nika-tui/Cargo.toml`:**
```toml
[package]
name = "nika-tui"
description = "Terminal UI for Nika workflow engine"
# ...workspace fields...

[dependencies]
nika-core = { workspace = true }
nika-event = { workspace = true }
nika-mcp = { workspace = true }
nika-runtime = { workspace = true }
nika-lsp-core = { workspace = true }
# TUI
ratatui = "0.30"
crossterm = { version = "0.29", features = ["event-stream"] }
tui-input = { version = "0.15", features = ["crossterm"] }
arboard = "3.4"
nucleo = "0.5"
tree-sitter = "0.24"
tree-sitter-yaml = "0.7"
streaming-iterator = "0.1"
git2 = "0.19"
openssl = { version = "0.10", features = ["vendored"] }
# Shared
tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
tracing = { workspace = true }
chrono = { workspace = true }
colored = { workspace = true }
# ... other deps currently used by tui/
```

### Task 6.2 — Move tui files

**Move** `tools/nika/src/tui/` → `tools/nika-tui/src/`

This is ~216 files, ~89k lines. The largest move.

- `mod.rs` → `lib.rs`
- All subdirectories maintain their structure

### Task 6.3 — Update all tui imports

Replace `crate::ast::*` → `nika_core::ast::*` or `nika_runtime::ast::*`
Replace `crate::error::*` → local TuiError or nika_runtime::RuntimeError
Replace `crate::runtime::*` → `nika_runtime::*`
Replace `crate::mcp::*` → `nika_mcp::*`
Replace `crate::event::*` → `nika_event::*`
Replace `crate::provider::*` → `nika_runtime::provider::*`
Replace `crate::config::*` → accept config via constructor parameter

### Task 6.4 — Remove tui feature from nika binary

In `tools/nika/Cargo.toml`:
- Remove `tui` feature definition
- Remove all optional TUI deps
- Add: `nika-tui = { workspace = true }`
- In main.rs: `use nika_tui::run_tui;` (or similar entry point)

### Task 6.5 — Checkpoint commit + verify
```
refactor(tui): extract nika-tui crate

Move entire TUI application (89k lines, 216 files) to standalone
nika-tui crate. Eliminates ratatui/crossterm/git2/tree-sitter from
the core binary's dependency tree.
```

**Verification:**
- [ ] `cargo check --workspace`
- [ ] `cargo test --lib -p nika-tui`
- [ ] `cargo test --lib -p nika`
- [ ] `nika ui` works (manual test)

---

## Phase 7 — Binary Cleanup + Final Polish

**Goal**: Slim nika binary, clean error hierarchy, final verification
**Duration**: 2-3 hours
**Depends on**: All previous phases

### Task 7.1 — Redesign NikaError as wrapper

```rust
// tools/nika/src/error.rs
#[derive(Debug, Error)]
pub enum NikaError {
    // Core errors
    #[error(transparent)]
    Core(#[from] nika_core::CoreError),

    #[error(transparent)]
    Parse(#[from] nika_core::ast::raw::ParseError),

    #[error(transparent)]
    Analyze(#[from] nika_core::ast::analyzer::AnalyzeError),

    // Runtime errors
    #[error(transparent)]
    Runtime(#[from] nika_runtime::RuntimeError),

    // CLI-specific errors (130-139)
    #[error("[NIKA-130] Config error: {reason}")]
    Config { reason: String },

    #[error("[NIKA-131] CLI error: {reason}")]
    Cli { reason: String },

    // Package/registry errors (260-269)
    #[error("[NIKA-260] Package error: {reason}")]
    Package { reason: String },
}
```

### Task 7.2 — Clean up nika/src/lib.rs

The public API of the `nika` crate should re-export from sub-crates:
```rust
pub use nika_core;
pub use nika_event;
pub use nika_dag;
pub use nika_mcp;
pub use nika_media;
pub use nika_runtime;
```

### Task 7.3 — Update nika-lsp dependency

`nika-lsp` currently depends on the full `nika` crate.
After the split, it should depend on `nika-runtime` + `nika-lsp-core` instead.

### Task 7.4 — Final verification

```bash
# Full workspace
cargo check --workspace
cargo test --workspace --lib
cargo clippy --workspace -- -D warnings

# Individual crates (dependency order)
cargo test -p nika-core
cargo test -p nika-event
cargo test -p nika-dag
cargo test -p nika-mcp
cargo test -p nika-media
cargo test --lib -p nika-runtime
cargo test --lib -p nika-tui
cargo test --lib -p nika
cargo test -p nika-lsp-core

# Feature combinations
cargo check -p nika-runtime --no-default-features
cargo check -p nika-media --no-default-features
cargo check -p nika --no-default-features

# Binary works
cargo run -p nika -- check tests/fixtures/hello.nika.yaml
cargo run -p nika -- doctor
```

### Task 7.5 — Update documentation

- Update `tools/nika/CLAUDE.md` with new crate structure
- Update `nika/CLAUDE.md`
- Update workspace README if exists

### Task 7.6 — Final commit
```
refactor: complete crate split — 10 workspace members

Split monolithic nika (282k lines) into 10 focused crates:
- nika-core:    AST, binding types, catalogs (18k)
- nika-event:   Event system (4k)
- nika-dag:     DAG validation (4k)
- nika-mcp:     MCP client (9k)
- nika-media:   CAS + media tools (30k)
- nika-runtime: Execution engine (70k)
- nika-tui:     Terminal UI (89k)
- nika:         CLI binary (25k)
+ nika-lsp-core, nika-lsp (existing)
```

---

## Execution Order Summary

```
Phase 0:  Workspace + dedup        [2-3h]  → commit
Phase 1:  nika-event               [2h]    → commit
Phase 2:  nika-dag                 [2h]    → commit
Phase 3:  nika-mcp                 [3-4h]  → commit
Phase 4:  nika-media               [4-5h]  → commit
────────────────── CODE REVIEW CHECKPOINT ──────────────────
Phase 5:  nika-runtime             [6-8h]  → commit
────────────────── CODE REVIEW CHECKPOINT ──────────────────
Phase 6:  nika-tui                 [4-5h]  → commit
Phase 7:  Binary cleanup           [2-3h]  → commit
────────────────── FINAL CODE REVIEW ──────────────────
```

## Risk Register

| Risk | Impact | Mitigation |
|------|--------|------------|
| NikaError has 60+ variants spread across 200+ files | Every phase touches error.rs | Incremental: each phase migrates its own variants |
| Circular deps (binding↔store) | Blocks nika-runtime compilation | Break with trait (TaskResultStore) in Phase 5 |
| Feature flag forwarding complexity | nika-media has 15 features | Test each combination individually |
| rig-core types leak into 13 files | Makes runtime extraction harder | Keep rig-core IN runtime (decided) |
| 89k TUI lines with deep coupling | Phase 6 is massive | TUI already feature-gated, mostly import rewrites |
| Tests reference `crate::` paths | Tests break on module moves | Update test imports phase by phase |
| Uncommitted changes in working tree | Could conflict | Phase 0 starts with stash/commit |

---

## Phase 8 — Push, Release, Homebrew, Full Verification

**Goal**: Push all commits, create release v0.38.0, update Homebrew, verify everything works
**Duration**: 1-2 hours
**Depends on**: All phases green

### Task 8.1 — Version bump to v0.38.0

Update ALL crate versions from 0.37.0 to 0.38.0:
- `tools/Cargo.toml` (workspace.package.version + workspace.dependencies)
- Each crate's Cargo.toml if version is not workspace-inherited
- `nika-lsp-core/Cargo.toml` and `nika-lsp/Cargo.toml`

### Task 8.2 — Changelog update

Create/update `CHANGELOG.md` with:
```markdown
## [0.38.0] - 2026-03-22

### Changed
- **BREAKING**: Split monolithic nika crate into 10 workspace members
- nika-core: AST, binding types, catalogs (unchanged)
- nika-event: Event system (new crate)
- nika-dag: DAG validation (new crate)
- nika-mcp: MCP client (new crate)
- nika-media: CAS + media tools (new crate)
- nika-runtime: Execution engine with rig-core (new crate)
- nika-tui: Terminal UI (new crate)
- nika: Slim CLI binary
```

### Task 8.3 — Final full test suite

```bash
# Full workspace
cargo check --workspace
cargo test --workspace --lib
cargo clippy --workspace -- -D warnings

# Feature matrix
cargo check -p nika --no-default-features
cargo check -p nika-runtime --no-default-features
cargo check -p nika-media --no-default-features
cargo check -p nika-media --features media-provenance

# Binary integration
cargo build --release -p nika
./target/release/nika doctor
./target/release/nika check tests/fixtures/hello.nika.yaml
```

### Task 8.4 — Push to main

```bash
git push origin main
```

### Task 8.5 — Create GitHub release

```bash
git tag -a v0.38.0 -m "v0.38.0: Crate split — 10 workspace members"
git push origin v0.38.0
gh release create v0.38.0 --title "v0.38.0: Crate Split" --notes "..."
```

### Task 8.6 — Update Homebrew tap

Update the Homebrew formula with new version, SHA256, and any build changes.
The formula likely needs to build the workspace now.

### Task 8.7 — Post-release verification

```bash
# Install from tap and verify
brew update
brew upgrade nika  # or brew install
nika --version     # should show 0.38.0
nika doctor
nika check <test-workflow>
```

---

## Success Criteria

1. `cargo check --workspace` — zero errors
2. `cargo test --workspace --lib` — all 7400+ tests pass
3. `cargo clippy --workspace -- -D warnings` — zero warnings
4. `nika run`, `nika check`, `nika ui`, `nika doctor` — all work
5. Each crate compiles independently
6. No circular dependencies between crates
7. NIKA-XXX error codes preserved
8. Feature flags work correctly (media-*, fetch-*, tui, lsp, native-inference)
