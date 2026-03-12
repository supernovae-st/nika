# Rust Workspace Best Practices for Nika v0.28.0 Restructure

**Research Date:** 2026-03-12
**Target:** Split Nika from monolithic 210K LoC crate into 3 crates
**Sources:** Context7 Cargo docs, Helix editor, Zed editor, cargo-rail, corrode.dev

---

## Executive Summary

This document synthesizes best practices from major Rust projects (Helix: 14 crates, Zed: 180+ crates) and official Cargo documentation for restructuring Nika into:

| Crate | Size | Purpose | Dependencies |
|-------|------|---------|--------------|
| `nika-core` | ~30K LoC | Pure types, zero runtime deps | None (leaf crate) |
| `nika-runtime` | ~90K LoC | Execution + providers | nika-core |
| `nika-tui` | ~90K LoC | TUI + CLI + LSP | nika-core, nika-runtime |

---

## 1. Workspace Configuration

### 1.1 Root Cargo.toml Structure

Based on Helix and Zed patterns:

```toml
[workspace]
resolver = "2"
members = [
    "crates/nika-core",
    "crates/nika-runtime",
    "crates/nika-tui",
]
default-members = ["crates/nika-tui"]

# ─────────────────────────────────────────────────────────────────────────────
# WORKSPACE PACKAGE METADATA (inherited by members)
# ─────────────────────────────────────────────────────────────────────────────
[workspace.package]
version = "0.28.0"
edition = "2024"
authors = ["SuperNovae Studio <studio@supernovae.dev>"]
license = "MIT"
repository = "https://github.com/SuperNovae-studio/nika"
rust-version = "1.87"

# ─────────────────────────────────────────────────────────────────────────────
# WORKSPACE DEPENDENCIES (single source of truth)
# ─────────────────────────────────────────────────────────────────────────────
[workspace.dependencies]
# Internal crates
nika-core = { path = "crates/nika-core" }
nika-runtime = { path = "crates/nika-runtime" }

# Async runtime
tokio = { version = "1.48", features = ["full"] }

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
serde_yaml = "0.9"

# Error handling
thiserror = "1.0"
miette = { version = "7.5", features = ["fancy"] }

# Tracing
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# TUI
ratatui = "0.29"
crossterm = "0.28"

# LLM providers
rig-core = "0.32"

# MCP
rmcp = "0.16"

# CLI
clap = { version = "4.5", features = ["derive"] }

# Testing
insta = { version = "1.42", features = ["yaml"] }
proptest = "1.6"
tokio-test = "0.4"
```

### 1.2 Member Crate Configuration

Each member inherits from workspace:

```toml
# crates/nika-core/Cargo.toml
[package]
name = "nika-core"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true
rust-version.workspace = true

[dependencies]
serde.workspace = true
thiserror.workspace = true

# NO tokio, NO async - pure types only
```

```toml
# crates/nika-runtime/Cargo.toml
[package]
name = "nika-runtime"
version.workspace = true
edition.workspace = true
# ... inherit all

[dependencies]
nika-core.workspace = true
tokio.workspace = true
rig-core.workspace = true
rmcp.workspace = true
serde.workspace = true
tracing.workspace = true
```

```toml
# crates/nika-tui/Cargo.toml
[package]
name = "nika-tui"
version.workspace = true
edition.workspace = true
# ... inherit all

[dependencies]
nika-core.workspace = true
nika-runtime.workspace = true
ratatui.workspace = true
crossterm.workspace = true
clap.workspace = true
tokio.workspace = true
tracing.workspace = true
```

---

## 2. Feature Flag Patterns

### 2.1 Feature Propagation Strategy

From Cargo documentation and Zed patterns:

```toml
# Root Cargo.toml
[workspace.dependencies]
tokio = { version = "1.48", default-features = false }

# nika-runtime/Cargo.toml - adds specific features
[dependencies]
tokio = { workspace = true, features = ["rt-multi-thread", "macros", "fs", "net"] }

# nika-tui/Cargo.toml - different features
[dependencies]
tokio = { workspace = true, features = ["rt-multi-thread", "macros", "io-std"] }
```

### 2.2 Optional Features for Conditional Compilation

```toml
# nika-core/Cargo.toml
[features]
default = []
json-schema = ["dep:schemars"]  # For JSON schema generation
serde = ["dep:serde"]           # Serialization support

[dependencies]
schemars = { version = "0.8", optional = true }
serde = { workspace = true, optional = true }
```

```toml
# nika-runtime/Cargo.toml
[features]
default = ["native-inference"]
native-inference = ["dep:mistral-rs"]  # Local LLM via mistral.rs
spn-daemon = ["dep:spn-client"]        # Keychain daemon integration

[dependencies]
mistral-rs = { version = "0.5", optional = true }
spn-client = { version = "0.2", optional = true }
```

```toml
# nika-tui/Cargo.toml
[features]
default = ["tui"]
tui = ["dep:ratatui", "dep:crossterm"]
lsp = ["dep:tower-lsp"]
headless = []  # CLI-only, no TUI

[dependencies]
ratatui = { workspace = true, optional = true }
crossterm = { workspace = true, optional = true }
tower-lsp = { version = "0.20", optional = true }
```

### 2.3 Feature Unification

From cargo-rail documentation - prevent feature inconsistency:

```toml
# Root Cargo.toml - ensure consistent feature activation
[workspace.dependencies]
# Define with minimal features, crates add what they need
regex = { version = "1.11", default-features = false }

# Each crate activates features explicitly
# nika-core: regex = { workspace = true, features = ["std"] }
# nika-runtime: regex = { workspace = true, features = ["std", "unicode"] }
```

---

## 3. Re-Export Patterns

### 3.1 Clean Public API Surface

From rust-lang/api-guidelines discussion:

```rust
// nika-core/src/lib.rs
//! Core types for Nika workflow engine.
//!
//! This crate provides the foundational types with zero runtime dependencies.

// Re-export primary types at crate root
pub use self::error::{NikaError, Result};
pub use self::workflow::{Workflow, Task, TaskId};
pub use self::action::{Action, InferParams, ExecParams, FetchParams, InvokeParams, AgentParams};
pub use self::binding::{UseEntry, WiringSpec};

// Module structure
pub mod error;
pub mod workflow;
pub mod action;
pub mod binding;
pub mod dag;

// Internal modules (not re-exported)
mod internal;
```

### 3.2 Facade Pattern for Runtime

```rust
// nika-runtime/src/lib.rs
//! Nika workflow execution engine.

// Re-export core types for convenience
pub use nika_core::{NikaError, Result, Workflow, Task};

// Runtime-specific exports
pub use self::executor::Executor;
pub use self::runner::Runner;
pub use self::provider::RigProvider;

pub mod executor;
pub mod runner;
pub mod provider;
pub mod mcp;
pub mod event;
```

### 3.3 Binary Crate Entry Point

```rust
// nika-tui/src/main.rs
use nika_runtime::{Executor, Runner, RigProvider};
use nika_core::{Workflow, NikaError};

// nika-tui/src/lib.rs (if exposing TUI as library)
pub use nika_core;      // Re-export entire crate
pub use nika_runtime;   // Re-export entire crate

pub mod tui;
pub mod cli;
pub mod lsp;
```

---

## 4. Compile Time Optimization

### 4.1 Development Profile (Fast Iteration)

From Zed and corrode.dev patterns:

```toml
[profile.dev]
# Faster linking
# macOS: use unpacked debuginfo
split-debuginfo = "unpacked"

# More codegen units = faster parallel compilation
codegen-units = 16

# Enable incremental compilation
incremental = true

# Lower optimization for faster builds
opt-level = 0

# Build dependencies with same settings
[profile.dev.build-override]
codegen-units = 16
split-debuginfo = "unpacked"
debug = true
```

### 4.2 Proc-Macro Optimization (Critical for Large Projects)

From Zed patterns - proc-macros are single-threaded, optimize them:

```toml
[profile.dev.package]
# Proc-macro crates - optimize heavily (they're single-threaded)
proc-macro2 = { opt-level = 3 }
syn = { opt-level = 3 }
quote = { opt-level = 3 }
serde_derive = { opt-level = 3 }
clap_derive = { opt-level = 3 }
thiserror-impl = { opt-level = 3 }
tracing-attributes = { opt-level = 3 }
tokio-macros = { opt-level = 3 }

# Heavy runtime crates - optimize for dev build speed
regex-automata = { opt-level = 3 }
regex-syntax = { opt-level = 3 }
```

### 4.3 Release Profiles

From Helix patterns:

```toml
[profile.release]
lto = "thin"
codegen-units = 1
strip = true
panic = "abort"

# Maximum optimization profile
[profile.opt]
inherits = "release"
lto = "fat"
codegen-units = 1
strip = true
opt-level = 3

# Integration test profile (balanced)
[profile.integration]
inherits = "test"
# Optimize heavy crates even in test mode
package.nika-core.opt-level = 2
package.nika-runtime.opt-level = 2
```

### 4.4 Linker Configuration

For `.cargo/config.toml`:

```toml
# macOS (Apple Silicon)
[target.aarch64-apple-darwin]
linker = "clang"
rustflags = ["-C", "link-arg=-fuse-ld=/opt/homebrew/bin/mold"]

# macOS (Intel)
[target.x86_64-apple-darwin]
linker = "clang"
rustflags = ["-C", "link-arg=-fuse-ld=/opt/homebrew/bin/mold"]

# Linux
[target.x86_64-unknown-linux-gnu]
linker = "clang"
rustflags = ["-C", "link-arg=-fuse-ld=mold"]

# Common build settings
[build]
# Use sccache for caching
rustc-wrapper = "sccache"

# Parallel jobs
jobs = 8

[env]
# Enable Cranelift for faster debug builds (nightly)
# RUSTFLAGS = "-Zcodegen-backend=cranelift"
```

### 4.5 CI/CD Build Optimization

```toml
# Fast CI builds
[profile.ci]
inherits = "dev"
debug = 0
incremental = false

# CI test profile
[profile.ci-test]
inherits = "test"
debug = 0
incremental = false
```

---

## 5. Migration Strategy

### 5.1 Phase 1: Create Crate Structure (Week 1)

```bash
# 1. Create directory structure
mkdir -p crates/{nika-core,nika-runtime,nika-tui}/src

# 2. Initialize Cargo.toml files
# - Root with workspace config
# - Each member with workspace inheritance

# 3. Create minimal lib.rs in each crate
echo 'pub fn placeholder() {}' > crates/nika-core/src/lib.rs

# 4. Verify workspace compiles
cargo check --workspace
```

### 5.2 Phase 2: Extract nika-core (Week 2)

Move pure types first (no breaking changes to existing code):

```bash
# Files to move to nika-core:
# src/ast/ → crates/nika-core/src/ast/
# src/dag/ → crates/nika-core/src/dag/
# src/error.rs → crates/nika-core/src/error.rs
# src/core/ → crates/nika-core/src/core/

# Steps:
1. Copy files to nika-core
2. Add re-exports in nika-core/src/lib.rs
3. Update original files to re-export from nika-core
4. Run tests after each file move
5. Remove original files once tests pass
```

### 5.3 Phase 3: Extract nika-runtime (Week 3)

Move execution engine:

```bash
# Files to move to nika-runtime:
# src/runtime/ → crates/nika-runtime/src/runtime/
# src/provider/ → crates/nika-runtime/src/provider/
# src/mcp/ → crates/nika-runtime/src/mcp/
# src/event/ → crates/nika-runtime/src/event/
# src/binding/ → crates/nika-runtime/src/binding/
# src/store/ → crates/nika-runtime/src/store/
```

### 5.4 Phase 4: Migrate nika-tui (Week 4)

Remaining code stays in nika-tui:

```bash
# Files that stay in nika-tui:
# src/tui/ → crates/nika-tui/src/tui/
# src/main.rs → crates/nika-tui/src/main.rs
# src/cli/ → crates/nika-tui/src/cli/
```

### 5.5 Migration Verification Checklist

After each phase:

```bash
# 1. Compile check
cargo check --workspace

# 2. Run all tests
cargo nextest run --workspace

# 3. Verify no circular dependencies
cargo tree --workspace | grep -E "nika-core|nika-runtime|nika-tui"

# 4. Check for unused dependencies
cargo machete --workspace

# 5. Verify public API surface
cargo doc --workspace --no-deps
```

---

## 6. Dependency Graph

### 6.1 Expected Dependency Direction

```
                    ┌──────────────────┐
                    │    nika-tui      │
                    │   (binary crate) │
                    └────────┬─────────┘
                             │
              ┌──────────────┴──────────────┐
              │                             │
              ▼                             ▼
    ┌─────────────────┐          ┌─────────────────┐
    │  nika-runtime   │          │   (direct use)  │
    │ (execution eng) │          │                 │
    └────────┬────────┘          └────────┬────────┘
             │                            │
             ▼                            ▼
    ┌─────────────────────────────────────────────┐
    │                 nika-core                    │
    │            (pure types, no deps)             │
    └─────────────────────────────────────────────┘
```

### 6.2 Forbidden Dependencies

| Crate | MUST NOT depend on |
|-------|-------------------|
| nika-core | tokio, async-trait, rmcp, rig-core, ratatui |
| nika-runtime | ratatui, crossterm, clap |
| nika-tui | (no restrictions, top of hierarchy) |

---

## 7. Risk Mitigation

### 7.1 Breaking Change Prevention

```rust
// In nika-core/src/lib.rs - maintain backward compatibility
#[doc(hidden)]
pub mod _private {
    // Internal types that may change
}

// Stable public API
pub mod v1 {
    pub use super::workflow::Workflow;
    pub use super::error::NikaError;
    // ... stable types
}
```

### 7.2 Feature Detection

```rust
// nika-runtime/src/lib.rs
#[cfg(feature = "native-inference")]
pub mod native;

#[cfg(not(feature = "native-inference"))]
pub mod native {
    pub fn is_available() -> bool { false }
}
```

### 7.3 Version Compatibility

```toml
# Ensure all crates stay in sync
[workspace.dependencies]
nika-core = { path = "crates/nika-core", version = "=0.28.0" }
nika-runtime = { path = "crates/nika-runtime", version = "=0.28.0" }
```

---

## 8. Tools and Commands

### 8.1 Dependency Analysis

```bash
# Check for unused dependencies
cargo machete --workspace
cargo udeps --workspace --all-targets

# Visualize dependency tree
cargo tree --workspace --depth 2

# Check feature unification
cargo tree --workspace -e features -i tokio
```

### 8.2 Build Performance Monitoring

```bash
# Measure compile times
cargo build --workspace --timings

# Profile compilation
cargo build --workspace -Z timings

# Check with minimal features
cargo check --workspace --no-default-features
```

### 8.3 Continuous Integration

```yaml
# .github/workflows/ci.yml
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - run: cargo check --workspace --all-features

  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - uses: taiki-e/install-action@nextest
      - run: cargo nextest run --workspace

  deny:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: EmbarkStudios/cargo-deny-action@v2
```

---

## 9. Summary of Recommendations

### 9.1 Configuration Priorities

1. **Use `resolver = "2"`** - Required for proper feature resolution
2. **Centralize deps in `[workspace.dependencies]`** - Single source of truth
3. **Inherit package metadata via `[workspace.package]`** - DRY configuration
4. **Define features explicitly per crate** - No hidden feature activation

### 9.2 Build Optimization Priorities

1. **Install mold linker** - 2-5x faster linking
2. **Optimize proc-macros in dev profile** - Major impact on compile times
3. **Use `split-debuginfo = "unpacked"` on macOS** - Faster incremental builds
4. **Set `codegen-units = 16` for dev** - More parallel compilation
5. **Use sccache** - Cache compiled crates across builds

### 9.3 Migration Priorities

1. **Start with leaf crate (nika-core)** - No internal dependencies
2. **Move one module at a time** - Verify tests after each move
3. **Maintain re-exports during migration** - Avoid breaking existing code
4. **Document public API surface** - Prevent accidental breakage

---

## 10. References

| Source | URL | Notes |
|--------|-----|-------|
| Cargo Workspace Docs | docs.rs/cargo | Workspace inheritance |
| Helix Editor | github.com/helix-editor/helix | 14-crate workspace |
| Zed Editor | github.com/zed-industries/zed | 180+ crate workspace |
| cargo-rail | docs.rs/cargo-rail | Dependency unification |
| Compile Time Tips | corrode.dev/blog/tips-for-faster-rust-compile-times | Optimization guide |
| rust-lang/api-guidelines | github.com/rust-lang/api-guidelines | Re-export patterns |

---

*Report generated for Nika v0.28.0 restructure planning.*
