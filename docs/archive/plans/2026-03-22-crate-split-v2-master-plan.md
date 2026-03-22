# Nika Workspace Crate Split v2 — Master Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Split the monolithic nika crate (267k lines) into 9 focused workspace crates following the Helix/Nushell/Ruff pattern.

**Architecture:** `nika` = thin binary (~200 lines). `nika-engine` = runtime + provider + error + config + dag + store + binding + tools + io + display + secrets + util + AST runtime types (~115k lines). `nika-tui` = full TUI (~89k lines). `nika-cli` = CLI subcommand handlers (~6k lines). Plus existing extracted crates: nika-core, nika-event, nika-mcp, nika-media.

**Tech Stack:** Rust 1.86, Cargo workspace, rig-core 0.32, rmcp 0.16, ratatui 0.30, clap 4.6, tokio 1.49

**References:**
- Helix pattern: `helix-core` → `helix-view` → `helix-tui` → `helix-term` (binary)
- Nushell pattern: `nu-protocol` → `nu-engine` → `nu-cli` → `nu` (binary)
- Deno pattern: `deno_core` → `deno_runtime` → `deno` (binary)
- Research at: agent transcripts from 2026-03-22 session

---

## Final Workspace Architecture

```
nika (binary, ~200 lines)          cargo install nika
├── nika-tui (optional, 89k)       ratatui, crossterm, git2
│   └── nika-engine
├── nika-cli (6k)                  clap subcommands
│   └── nika-engine
└── nika-engine (115k)             Runner, provider, dag, store, error
    ├── nika-core (30k)            AST, types (zero I/O)
    ├── nika-event (4k)            EventLog
    ├── nika-mcp (7.5k)            MCP client, rmcp
    └── nika-media (3.5k)          CAS store
+ nika-lsp-core (9k), nika-lsp (2k)
```

## Dependency Graph (Strictly Acyclic)

```
                      nika-core
                     /    |    \
              nika-event  |     \
                  |       |      \
               nika-mcp   |       |
                  |       |       |
               nika-media |       |
                    \     |      /
                    nika-engine
                   /      |      \
            nika-tui  nika-cli  nika-lsp
                   \      |      /
                     \    |    /
                       nika
```

## What Goes Where

### nika-engine (~115k lines) — The execution engine

**Moves FROM nika/src/ TO nika-engine/src/:**

| Module | Lines | Path in nika-engine |
|--------|-------|---------------------|
| `error.rs` | 2,218 | `src/error.rs` |
| `config.rs` | 400 | `src/config.rs` |
| `runtime/` | 57,348 | `src/runtime/` |
| `provider/` | 5,801 | `src/provider/` |
| `dag/` | 3,946 | `src/dag/` |
| `store/` | 1,598 | `src/store/` |
| `binding/` (runtime parts) | 8,436 | `src/binding/` |
| `tools/` | 3,670 | `src/tools/` |
| `io/` | 2,609 | `src/io/` |
| `display/` | 3,980 | `src/display/` |
| `secrets/` | 1,327 | `src/secrets/` |
| `core/` | 2,968 | `src/core/` |
| `util/` | 705 | `src/util/` |
| `ast/` (runtime types) | 20,247 | `src/ast/` |
| `source/` (re-export) | 3 | `src/source.rs` |

### nika-cli (~6k lines) — CLI subcommand handlers

**Moves FROM nika/src/cli/ TO nika-cli/src/:**

| File | Lines | Description |
|------|-------|-------------|
| `doctor.rs` | 630 | `nika doctor` |
| `workflow.rs` | 577 | `nika check` |
| `init.rs` | 582 | `nika init` |
| `mcp.rs` | 589 | `nika mcp` |
| `media.rs` | 967 | `nika media` |
| `model.rs` | 509 | `nika model` |
| `new_cmd.rs` | 166 | `nika new` |
| `pkg.rs` | 581 | `nika pkg` |
| `provider.rs` | 274 | `nika provider` |
| `schema.rs` | 497 | `nika schema` |
| `trace.rs` | 160 | `nika trace` |
| `config.rs` | 284 | `nika config` |

Also moves: `init/` (8,154 lines), `new/` (4,134 lines), `registry/` (2,507 lines)

### nika-tui (~89k lines) — Terminal UI

**Moves FROM nika/src/tui/ TO nika-tui/src/:**

All 216 files, 88,812 lines. Already `#[cfg(feature = "tui")]`-gated.

### nika (binary) — Thin shell

**Keeps in nika/src/:**

| File | Lines | Description |
|------|-------|-------------|
| `main.rs` | ~200 | Clap parsing + dispatch to nika-cli/nika-tui |
| `lib.rs` | ~20 | Re-exports for backward compat (optional) |

---

## Phases (4 phases, incremental, each independently shippable)

## Phase 1: Create nika-engine (the big move)

**Duration:** 4-6 hours
**Risk:** High (moves 115k lines, touches every import)
**Strategy:** Copy all engine modules to nika-engine, fix `crate::` → local refs, make nika re-export from nika-engine temporarily.

### Task 1.1: Create nika-engine crate skeleton

**Files:**
- Create: `tools/nika-engine/Cargo.toml`
- Create: `tools/nika-engine/src/lib.rs`
- Modify: `tools/Cargo.toml` (add workspace member)

**Step 1: Create Cargo.toml**

```toml
# tools/nika-engine/Cargo.toml
[package]
name = "nika-engine"
version.workspace = true
edition.workspace = true
authors.workspace = true
description = "Workflow execution engine for Nika — embeddable runtime"
license.workspace = true
repository.workspace = true
rust-version.workspace = true

[features]
default = ["native-inference", "media-compression", "media-core",
           "fetch-extract", "fetch-article", "fetch-feed",
           "media-chart", "media-phash", "media-pdf", "media-iqa",
           "media-qr"]
native-inference = ["dep:mistralrs", "dep:async-stream"]
native-keychain = ["dep:keyring"]
nika-daemon = ["dep:spn-client"]
# Media features forwarded to nika-media
media-core = ["media-thumbnail", "media-metadata", "media-optimize", "media-svg"]
media-thumbnail = ["dep:fast_image_resize", "dep:image", "nika-media/media-thumbnail"]
media-metadata = ["dep:nom-exif", "dep:lofty"]
media-optimize = ["dep:oxipng"]
media-svg = ["dep:resvg", "dep:usvg", "dep:tiny-skia", "dep:fontdb"]
media-phash = ["dep:image_hasher", "dep:image"]
media-pdf = ["dep:pdf-extract"]
media-chart = ["dep:charts-rs"]
media-provenance = ["dep:c2pa"]
media-qr = ["dep:qrcode-ai-scanner-core", "dep:image"]
media-compression = ["dep:zstd", "nika-media/media-compression"]
media-iqa = ["dep:dssim-core", "dep:rgb", "dep:image"]
# Fetch features
fetch-html = ["dep:scraper", "dep:psl"]
fetch-markdown = ["dep:htmd"]
fetch-article = ["dep:dom_smoothie", "dep:scraper"]
fetch-feed = ["dep:feed-rs"]
fetch-extract = ["fetch-html", "fetch-markdown"]
# LSP (embedded)
lsp = ["dep:tower-lsp-server", "tokio/io-std", "tokio/io-util"]

[dependencies]
# Internal crates
nika-core = { workspace = true }
nika-event = { workspace = true }
nika-mcp = { workspace = true }
nika-media = { workspace = true }
nika-lsp-core = { workspace = true }

# Async
tokio = { workspace = true }
tokio-util = { workspace = true }
async-trait = { workspace = true }
backon = { workspace = true }

# Serialization
serde = { workspace = true }
serde_json = { workspace = true }
serde-saphyr = { workspace = true }
toml = { workspace = true }

# YAML/JSON
marked-yaml = { workspace = true }
strsim = { workspace = true }
indexmap = { workspace = true }
serde_json_path = { workspace = true }
jsonschema = { workspace = true }

# Errors
thiserror = { workspace = true }
miette = { workspace = true }

# HTTP
reqwest = { workspace = true }
url = { workspace = true }

# LLM
rig-core = { workspace = true }
mistralrs = { version = "0.7", optional = true }
async-stream = { version = "0.3", optional = true }

# Crypto/Security
sha2 = { workspace = true }
keyring = { version = "3", features = ["apple-native", "windows-native", "sync-secret-service"], optional = true }
secrecy = "0.10"
zeroize = "1.8"
nix = { version = "0.29", features = ["fs", "signal", "process"] }
spn-client = { version = "0.3.4", optional = true }

# Concurrency
dashmap = { workspace = true }
parking_lot = { workspace = true }

# Utilities
regex = { workspace = true }
colored = { workspace = true }
dotenvy = "0.15"
semver = { workspace = true }
smallvec = { workspace = true }
rustc-hash = { workspace = true }
uuid = { workspace = true }
dirs = "5.0"
xxhash-rust = { workspace = true }
chrono = { workspace = true }
rand = { workspace = true }
humantime = { workspace = true }
shlex = "1.3"
shellexpand = "3.1"
unicode-normalization = "0.1"
unicode-width = "0.2"
terminal_size = "0.4"
base64 = { workspace = true }
camino = { workspace = true }
ignore = { workspace = true }
globset = { workspace = true }
bytes = "1"
futures = { version = "0.3.32", default-features = false, features = ["alloc"] }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
petgraph = { workspace = true }

# Media pipeline (always-on tier 1)
blake3 = { workspace = true }
reflink-copy = "0.1"
infer = "0.19"
mime_guess = "2.0"
mime = "0.3"
imagesize = "0.14"
thumbhash = "0.1"
color-thief = "0.2"
rayon = "1.10"

# Media tools — Tier 2+ (feature-gated, optional)
fast_image_resize = { version = "6.0", features = ["image"], optional = true }
image = { version = "0.25", optional = true, default-features = false, features = ["png", "jpeg", "webp", "gif"] }
nom-exif = { version = "2.7", optional = true }
lofty = { version = "0.23", optional = true }
oxipng = { version = "10.1", optional = true, default-features = false, features = ["parallel"] }
resvg = { version = "0.47", optional = true }
usvg = { version = "0.47", optional = true }
tiny-skia = { version = "0.12", optional = true }
fontdb = { version = "0.23", optional = true }
image_hasher = { version = "3.1", optional = true }
pdf-extract = { version = "0.10", optional = true }
charts-rs = { version = "0.3", optional = true, features = ["image-encoder"] }
c2pa = { version = "0.78", optional = true, features = ["rust_native_crypto"] }
qrcode-ai-scanner-core = { version = "0.2", optional = true }
zstd = { version = "0.13", optional = true, default-features = false }
dssim-core = { version = "3.4", optional = true }
rgb = { version = "0.8", optional = true }

# Web extraction (feature-gated)
scraper = { version = "0.22", optional = true, features = ["atomic"] }
htmd = { version = "0.5", optional = true }
dom_smoothie = { version = "0.16", optional = true }
feed-rs = { version = "2.3", optional = true }
psl = { version = "2", optional = true }

# LSP (feature-gated)
tower-lsp-server = { version = "0.23", optional = true }

# Package registry
urlencoding = "2.1"
flate2 = "1.1"
tar = "0.4"

[dev-dependencies]
tempfile = { workspace = true }
proptest = { workspace = true }
insta = { workspace = true }
pretty_assertions = { workspace = true }
criterion = { version = "0.5", features = ["async_tokio"] }
serial_test = "3.1"
wiremock = "0.6"
qrcode = "0.14"
```

**Step 2: Create lib.rs**

```rust
// tools/nika-engine/src/lib.rs
//! Nika Engine — Embeddable workflow execution engine
//!
//! This crate contains the core execution logic for Nika workflows.
//! It is designed to be embedded in any Rust application without
//! pulling in CLI or TUI dependencies.

// YAML parsing alias
pub use serde_saphyr as serde_yaml;

// Source tracking
pub mod source;

// Public modules
pub mod ast;
pub mod binding;
pub mod config;
pub mod core;
pub mod dag;
pub mod display;
pub mod error;
pub mod event;
pub mod init;
pub mod io;
pub mod mcp;
pub mod media;
pub mod new;
pub mod provider;
pub mod registry;
pub mod runtime;
pub mod secrets;
pub mod store;
pub mod tools;
pub mod util;

// Feature-gated
#[cfg(feature = "lsp")]
pub mod lsp;

// ── Public API re-exports ────────────────────────────────────
pub use source::{ByteOffset, FileId, SourceFile, SourceRegistry, Span, Spanned};
pub use error::NikaError;
pub use ast::{
    AgentParams, ExecParams, FetchParams, InferParams, InvokeParams,
    Task, TaskAction, Workflow,
};
pub use runtime::{Runner, TaskExecutor};
pub use dag::{validate_bindings, Dag, StableDag};
pub use binding::{validate_task_id, BindingEntry, BindingSpec, ResolvedBindings};
pub use event::{list_traces, prune_traces, Event, EventKind, EventLog};
pub use store::{RunContext, TaskResult};
pub use mcp::{McpClient, McpConfig};
```

**Step 3: Add to workspace**

In `tools/Cargo.toml`, add `"nika-engine"` to members and add:
```toml
nika-engine = { path = "nika-engine", version = "0.37.0" }
```

**Step 4: Verify skeleton compiles**

Run: `cd tools && cargo check -p nika-engine`
Expected: Errors about missing src/ modules (we haven't copied files yet)

**Step 5: Commit**

```
chore(engine): create nika-engine crate skeleton
```

### Task 1.2: Copy all engine modules to nika-engine

**Step 1: Copy all source files**

```bash
cd tools/nika/src
for mod in ast binding config.rs core dag display error.rs io \
           provider runtime secrets source store tools util \
           init new registry; do
    cp -r "$mod" ../../nika-engine/src/
done
```

**Step 2: Copy LSP module (feature-gated)**

```bash
cp -r lsp ../../nika-engine/src/
```

**Step 3: Verify file count**

Run: `find tools/nika-engine/src -name "*.rs" | wc -l`
Expected: ~350+ files

### Task 1.3: Fix all imports in nika-engine

Every `use crate::` reference is correct AS-IS because we copied the entire module tree. But we need to fix references to extracted crates:

**Step 1: Fix event re-export**

The `event/mod.rs` already says `pub use nika_event::*;`. This works because nika-engine depends on nika-event.

**Step 2: Fix mcp re-export**

The `mcp/mod.rs` already says `pub use nika_mcp::*;`. Works.

**Step 3: Fix media re-export**

The `media/mod.rs` already says `pub use nika_media::*;`. Works.

**Step 4: Fix ast re-exports from nika-core**

The `ast/mod.rs` already uses `pub use nika_core::ast::*` patterns. Works.

**Step 5: Fix binding re-exports from nika-core**

The `binding/mod.rs` already uses `pub use nika_core::binding::*`. Works.

**Step 6: Fix source re-export**

The `source/mod.rs` already says `pub use nika_core::source::*;`. Works.

**Step 7: Try to compile**

Run: `cd tools && cargo check -p nika-engine 2>&1 | grep "^error" | sort -u | head -20`
Expected: Should compile (all internal references use `crate::` which resolves within nika-engine)

**Step 8: Fix any issues iteratively**

If there are errors, fix them one by one. Common issues:
- Missing crate dependencies in Cargo.toml
- `pub(crate)` items that need to become `pub`

**Step 9: Run tests**

Run: `cd tools && cargo test --lib -p nika-engine -- --test-threads=4`
Expected: ~5800+ tests pass

**Step 10: Commit**

```
refactor(engine): copy all engine modules to nika-engine

Copy runtime, provider, dag, store, binding, tools, io, display,
secrets, core, util, ast, config, error, source, init, new, registry,
lsp to nika-engine. All 115k lines with tests.
```

### Task 1.4: Make nika depend on nika-engine (temporary re-export)

**Step 1: Add nika-engine dependency to nika/Cargo.toml**

```toml
nika-engine = { workspace = true }
```

**Step 2: Replace nika's lib.rs with re-exports**

```rust
// tools/nika/src/lib.rs
//! Nika — re-exports from nika-engine for backward compatibility.
pub use nika_engine::*;

// Feature-gated modules
#[cfg(feature = "tui")]
pub mod tui;
```

**Step 3: Delete all moved modules from nika/src/**

Keep: `main.rs`, `lib.rs`, `tui/`, `cli/` (cli moves in Phase 2)
Delete: `ast/`, `binding/`, `config.rs`, `core/`, `dag/`, `display/`,
        `error.rs`, `event/`, `init/`, `io/`, `lsp/`, `mcp/`, `media/`,
        `new/`, `provider/`, `registry/`, `runtime/`, `secrets/`, `source/`,
        `store/`, `tools/`, `util/`

**Step 4: Fix main.rs imports**

main.rs uses `use nika::*` which now comes from nika-engine. Should work.

**Step 5: Fix cli/ imports**

cli/ uses `use nika::*` (from main.rs scope). Should work.

**Step 6: Fix tui/ imports**

tui/ uses `use crate::*` which now resolves to nika's lib.rs → nika-engine. Should work.

**Step 7: Remove duplicate deps from nika/Cargo.toml**

Most deps are now in nika-engine. nika only needs:
- nika-engine
- clap, clap_complete (CLI)
- ratatui, crossterm, etc. (TUI, optional)
- nika-lsp-core (for TUI completion)
- tracing-subscriber (for main.rs logging setup)

**Step 8: Verify compilation**

Run: `cd tools && cargo check --workspace`
Expected: All crates compile

**Step 9: Run ALL tests**

Run: `cd tools && cargo test --lib -p nika-engine -- --test-threads=4`
Run: `cd tools && cargo test --lib -p nika -- --test-threads=4`
Expected: Total tests >= 7100

**Step 10: Run clippy**

Run: `cd tools && cargo clippy --all-targets --all-features -- -D warnings`
Expected: Zero errors

**Step 11: Commit**

```
refactor(engine): make nika depend on nika-engine

nika/lib.rs now re-exports from nika-engine. All source modules
deleted from nika except main.rs, cli/, tui/. nika is now a thin
binary+TUI shell over nika-engine.
```

### Task 1.5: Verification checkpoint

**Step 1: Full workspace test**

```bash
cd tools
cargo test -p nika-core --lib
cargo test -p nika-event --lib
cargo test -p nika-mcp --lib
cargo test -p nika-media --lib
cargo test -p nika-engine --lib -- --test-threads=4
cargo test -p nika --lib -- --test-threads=4
```

**Step 2: Clippy**

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

**Step 3: Feature matrix**

```bash
cargo check -p nika-engine --no-default-features
cargo check -p nika --no-default-features
```

**Step 4: Commit + push**

```
refactor(engine): extract nika-engine — Phase 1 complete

nika-engine contains the full execution engine (115k lines).
nika is now a thin binary shell. Zero circular dependencies.
All 7100+ tests pass.
```

---

## Phase 2: Extract nika-cli (CLI subcommands)

**Duration:** 1-2 hours
**Risk:** Low (small module, clean boundaries)

### Task 2.1: Create nika-cli crate

**Files:**
- Create: `tools/nika-cli/Cargo.toml`
- Create: `tools/nika-cli/src/lib.rs`

**Step 1: Create Cargo.toml**

```toml
[package]
name = "nika-cli"
version.workspace = true
edition.workspace = true
authors.workspace = true
description = "CLI subcommand handlers for Nika"
license.workspace = true
repository.workspace = true
rust-version.workspace = true

[features]
default = ["native-inference"]
native-inference = ["nika-engine/native-inference"]

[dependencies]
nika-engine = { workspace = true }
nika-core = { workspace = true }
clap = { workspace = true }
clap_complete = { workspace = true }
colored = { workspace = true }
tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
tracing = { workspace = true }
```

**Step 2: Copy cli/ files**

```bash
cp -r tools/nika/src/cli/* tools/nika-cli/src/
mv tools/nika-cli/src/mod.rs tools/nika-cli/src/lib.rs
```

**Step 3: Fix imports**

Replace `use nika::*` and `use crate::*` with `use nika_engine::*` in all cli files.

**Step 4: Add to workspace, verify, test, commit**

```
refactor(cli): extract nika-cli crate (6k lines)
```

---

## Phase 3: Extract nika-tui (Terminal UI)

**Duration:** 2-3 hours
**Risk:** Medium (89k lines, many imports to fix)
**Prerequisite:** Phase 1 complete (nika-tui depends on nika-engine, NOT nika)

### Task 3.1: Create nika-tui crate

**Step 1: Create Cargo.toml**

```toml
[package]
name = "nika-tui"
version.workspace = true
edition.workspace = true
# ...

[dependencies]
nika-engine = { workspace = true }
nika-core = { workspace = true }
nika-event = { workspace = true }
nika-mcp = { workspace = true }
nika-media = { workspace = true }
nika-lsp-core = { workspace = true }
# TUI deps
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
# ... (all TUI-specific deps)
```

**Step 2: Copy tui/ files**

```bash
cp -r tools/nika/src/tui/* tools/nika-tui/src/
mv tools/nika-tui/src/mod.rs tools/nika-tui/src/lib.rs
```

**Step 3: Fix imports (bulk)**

```bash
# Remove #[cfg(feature = "tui")] gates (always enabled in nika-tui)
find . -name "*.rs" -exec sed -i '' '/#\[cfg(feature = "tui")\]/d' {} +

# Fix crate::tui:: → crate:: (internal TUI refs)
find . -name "*.rs" -exec sed -i '' 's/crate::tui::/crate::/g' {} +

# Fix crate::<module> → nika_engine::<module> for engine modules
for mod in ast error provider runtime config core source store \
           media display binding dag io util secrets new init registry; do
    find . -name "*.rs" -exec sed -i '' "s/crate::${mod}/nika_engine::${mod}/g" {} +
done

# Fix crate::event → nika_event (direct dep)
find . -name "*.rs" -exec sed -i '' 's/crate::event/nika_event/g' {} +

# Fix crate::mcp → nika_mcp (direct dep)
find . -name "*.rs" -exec sed -i '' 's/crate::mcp/nika_mcp/g' {} +

# Fix crate::serde_yaml → nika_engine::serde_yaml
find . -name "*.rs" -exec sed -i '' 's/crate::serde_yaml/nika_engine::serde_yaml/g' {} +
```

**Step 4: Remove #[cfg(not(feature = "tui"))] stubs**

Delete all stub functions that return "TUI feature not enabled" errors.

**Step 5: Fix TUI-internal module references**

Some TUI modules were `crate::tui::providers` which became `crate::providers`.
Others were `crate::tui::utils` which became `crate::utils`. These are now correct.

**Step 6: Compile, fix iteratively, test, commit**

```
refactor(tui): extract nika-tui crate (89k lines, 2053 tests)
```

---

## Phase 4: Slim nika binary

**Duration:** 1 hour
**Risk:** Low

### Task 4.1: Rewrite nika as thin binary

**Step 1: Update nika/Cargo.toml**

```toml
[package]
name = "nika"
version.workspace = true
edition.workspace = true
authors.workspace = true
description = "Semantic YAML workflow engine for AI tasks"
license.workspace = true
repository.workspace = true
publish = true
rust-version.workspace = true

[[bin]]
name = "nika"
path = "src/main.rs"

[features]
default = ["tui"]
tui = ["dep:nika-tui"]

[dependencies]
nika-engine = { workspace = true }
nika-cli = { workspace = true }
nika-tui = { workspace = true, optional = true }
clap = { workspace = true }
clap_complete = { workspace = true }
colored = { workspace = true }
tokio = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
dotenvy = "0.15"
```

**Step 2: Delete lib.rs (or keep minimal re-export)**

**Step 3: Simplify main.rs**

main.rs keeps clap definitions and dispatch. Handlers call `nika_cli::*` and `nika_tui::*`.

**Step 4: Delete tui/ and cli/ from nika/src/**

**Step 5: Verify, test, commit**

```
refactor: complete crate split — nika is now a thin 200-line binary

9 workspace crates:
- nika:         Binary (200 lines)
- nika-engine:  Execution engine (115k)
- nika-tui:     Terminal UI (89k)
- nika-cli:     CLI subcommands (6k)
- nika-core:    AST, types (30k)
- nika-event:   Events (4k)
- nika-mcp:     MCP client (7.5k)
- nika-media:   CAS store (3.5k)
+ nika-lsp-core, nika-lsp
```

---

## Phase 5: Version bump + Release

### Task 5.1: Bump to v0.38.0

Update all Cargo.toml versions and workspace.package.version.

### Task 5.2: Update documentation

- `tools/nika/CLAUDE.md` — new crate structure
- `nika/CLAUDE.md` — updated commands
- `ARCHITECTURE.md` — dependency graph

### Task 5.3: Full verification

```bash
cargo check --workspace
cargo test --workspace --lib
cargo clippy --workspace --all-features -- -D warnings
cargo build --release -p nika
./target/release/nika doctor
./target/release/nika check tests/fixtures/hello.nika.yaml
```

### Task 5.4: Push, tag, release

```bash
git push origin main
git tag -a v0.38.0 -m "v0.38.0: Crate split — 9 workspace members"
git push origin v0.38.0
```

---

## Agent Usage Guide

| Phase | Agent | Purpose |
|-------|-------|---------|
| 1.2-1.3 | `rust-pro` | Fix compilation errors during engine extraction |
| 1.4 | `code-reviewer` | Review the re-export shim for correctness |
| 2.3 | `rust-pro` | Fix CLI import paths |
| 3.3-3.5 | `rust-pro` | Fix TUI bulk import replacement |
| 4.1 | `rust-architect` | Review final binary structure |
| 5.3 | `code-reviewer` | Final quality gate before release |

## Risk Register

| Risk | Mitigation |
|------|------------|
| NikaError has 60+ variants referencing types from many modules | Keep NikaError in nika-engine where all types are available |
| TUI imports 10+ engine modules | Bulk sed replacement + iterative cargo check |
| Feature flag forwarding complexity | Test each feature combo individually |
| Tests reference `crate::` paths | Tests move WITH their modules, paths stay valid |
| Pre-commit hook runs clippy on all features | Fix clippy issues before committing |
| nika-lsp depends on full nika | Update to depend on nika-engine instead |

## Success Criteria

1. `cargo check --workspace` — zero errors
2. `cargo test --workspace --lib` — all 7100+ tests pass
3. `cargo clippy --workspace --all-features -- -D warnings` — zero warnings
4. `nika run`, `nika check`, `nika ui`, `nika doctor` — all work
5. Each crate compiles independently
6. No circular dependencies
7. NIKA-XXX error codes preserved
8. `cargo install nika` produces working binary
