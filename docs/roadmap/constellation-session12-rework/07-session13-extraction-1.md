The `*Caps` structs from Session 12 don't exist yet (they're part of S12's work). Now I have a complete picture of the codebase. Let me write the blueprint.

---

# Nika Constellation — Session 13: Verb Extraction Pass 1

**Target crates created:** `nika-runtime`, `nika-verb-exec`, `nika-verb-invoke`, `nika-verb-fetch`
**Estimated LOC removed from nika-engine:** ~2,400 (exec 471 + invoke 522 + fetch 1,399 = 2,392 + deletions)
**Prerequisite:** Session 12 complete — `*Caps<'a>` structs defined (but not wired), `nika-extract` created, `PolicyChecker` trait in `nika-kernel`, `nika-policy` crate live.

---

## Patterns and Conventions Found

**EventKind lives in nika-event (L1), not nika-engine.** `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/event/mod.rs` is a one-line `pub use nika_event::*;` re-export. This means verb crates can depend on `nika-event` directly for event emission without pulling in `nika-engine`. This is the critical finding that unblocks the extraction. No prerequisite EventKind migration is needed.

**PolicyEnforcer is concrete, lives in nika-engine.** `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/policy.rs` at line 273. The `PolicyChecker` trait is being added to `nika-kernel` in S12. The coercion `&*self.policy_enforcer.read() as &dyn PolicyChecker` requires `PolicyEnforcer: PolicyChecker`, which S12 must wire.

**TaskExecutor has 22 fields.** The full struct is documented at `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/executor/mod.rs` lines 69–137. The exec-relevant fields are: `cancel_token`, `policy_enforcer`, `event_log`, `workflow_base_dir`, `working_dir_mode`, `project_root`. The fetch-relevant fields additionally include: `http_client`, `robots_cache`, `domain_rate_limiter`, `cookie_jar`, `fetch_cache`. The invoke-relevant fields are: `mcp_pool`, `builtin_router`, `event_log`, `cancel_token`.

**exec.rs is 471 LOC, invoke.rs is ~330 LOC visible (likely ~522 total), fetch.rs is 1,399 LOC.** The exec file is the cleanest — zero imports outside nika-engine/nika-core. Fetch imports `reqwest::Client` directly and builds a custom one-off client inside the function body for DNS-pinned requests. This is the key reason fetch needs a `reqwest` dep even after extracting; the `HttpClient` trait cannot express the SSRF-aware redirect closure that captures `allowed_hosts` at construction time. That exception must be documented.

**`for_each` spawns tasks via `tokio::spawn`.** Confirmed in `task_dispatch.rs`. Borrowed `ExecCaps<'a>` cannot cross `spawn` because it borrows from the TaskExecutor which is not `'static`. The `ExecCapsOwned` pattern solves this.

**No `trait Verb` — dispatch via `match on TaskAction`.** `TaskAction` is already defined in `nika-core::ast`. The `task_dispatch.rs` file imports `crate::ast::TaskAction` and already matches on it at line 18.

**Workspace Cargo.toml** at `/Users/thibaut/dev/supernovae/nika/tools/Cargo.toml` — both `nika-runtime` and the verb crates must be added to `[workspace.members]` and `[workspace.dependencies]`.

---

## Architecture Decision

The extraction follows a strict bridge-first pattern. Each verb is extracted in three sub-commits: (1) create the new crate with the free function, (2) make `TaskExecutor`'s existing method delegate to the free function, (3) verify via test suite and delete the old file. This lets the workspace remain green after every single commit. Session 14 will then delete `TaskExecutor` entirely after infer/agent are extracted.

`nika-runtime` sits at L3 (above nika-policy, nika-event, nika-kernel, nika-builtin, nika-extract). The verb crates sit at L2, consuming L0.5 traits and `nika-event` for emission. `nika-engine` remains at L2 as a compat shim during S13, depending on `nika-runtime` for the Runner (which moves up) and on the verb crates for delegation.

---

## Part 0 — Prerequisite Check (Not a commit, but a gate)

Before starting S13, verify these S12 outputs compile:

```
cargo check -p nika-policy
cargo check -p nika-extract
```

And verify `PolicyChecker` is in nika-kernel:

```
grep -r 'PolicyChecker' /Users/thibaut/dev/supernovae/nika/tools/nika-kernel/src/
```

And verify the `*Caps<'a>` structs exist somewhere (they can be in `nika-engine` temporarily — S13 will move them to `nika-runtime`).

---

## Part 1 — `nika-runtime` Scaffold (4 commits)

### Commit 1.1 — `feat(runtime): create nika-runtime L3 crate with VerbCapabilities`

**Files to create:**

`/Users/thibaut/dev/supernovae/nika/tools/nika-runtime/Cargo.toml`

```toml
[package]
name = "nika-runtime"
version.workspace = true
edition.workspace = true
authors.workspace = true
description = "Nika runtime — DAG runner, verb dispatch, and capability bundles (L3)"
license.workspace = true
repository.workspace = true
rust-version.workspace = true
publish = true

[dependencies]
nika-core = { workspace = true }
nika-kernel = { workspace = true }
nika-event = { workspace = true }
nika-policy = { workspace = true }       # S12 crate
tokio = { workspace = true }
tokio-util = { workspace = true }
async-trait = { workspace = true }
parking_lot = { workspace = true }
thiserror = { workspace = true }
tracing = { workspace = true }
serde_json = { workspace = true }

[lints]
workspace = true
```

`/Users/thibaut/dev/supernovae/nika/tools/nika-runtime/src/lib.rs`

```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Nika Runtime — DAG runner, verb dispatch, and capability bundles.
//!
//! L3 in the diamond layer. Depends on nika-kernel (L0.5), nika-event (L1),
//! nika-policy (L1), and indirectly on effect crates (L2) via trait objects.
//!
//! ## Key exports
//! - `VerbCapabilities` — all Arc<dyn Trait> dependencies for verb execution
//! - `dispatch()` — 12-line match on TaskAction, calls per-verb free functions
//! - `Runner` — DAG execution loop (moved from nika-engine)

pub mod capabilities;
pub mod dispatch;
// pub mod runner; // Commit 1.3
```

`/Users/thibaut/dev/supernovae/nika/tools/nika-runtime/src/capabilities.rs`

```rust
use std::path::PathBuf;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

use nika_event::EventLog;
use nika_kernel::{
    builtin::BuiltinRouter,
    filesystem::{FsRead, FsWrite},
    http::HttpClient,
    shell::ShellExecutor,
    store::BlobStore,
};
use nika_policy::PolicyEnforcer;

/// All side-effect dependencies needed to execute any verb.
///
/// Constructed once per workflow run by the Runner, then passed
/// into `dispatch()` by reference. Each verb accessor returns only
/// the subset needed by that verb, keeping the field set minimal.
///
/// `Arc<dyn Trait>` fields are cheap to clone for `tokio::spawn` (see ExecCapsOwned).
#[derive(Clone)]
pub struct VerbCapabilities {
    // ── Core ──────────────────────────────────────────────────────────
    pub(crate) event_log: EventLog,
    pub(crate) cancel_token: CancellationToken,
    pub(crate) policy_enforcer: Arc<parking_lot::RwLock<PolicyEnforcer>>,

    // ── Exec ──────────────────────────────────────────────────────────
    pub(crate) shell: Arc<dyn ShellExecutor>,
    pub(crate) workflow_base_dir: PathBuf,
    pub(crate) skills_base_dir: PathBuf,
    pub(crate) project_root: Option<PathBuf>,
    pub(crate) working_dir_mode: Option<String>,

    // ── Fetch ─────────────────────────────────────────────────────────
    pub(crate) http: Arc<dyn HttpClient>,
    pub(crate) blob: Arc<dyn BlobStore>,
    pub(crate) fetch_aux: FetchAux,

    // ── Invoke ────────────────────────────────────────────────────────
    pub(crate) builtin_router: Arc<dyn BuiltinRouter>,
    pub(crate) mcp_pool: Arc<dyn MpcPool>,   // nika-mcp trait (S12 or S13)
}

impl VerbCapabilities {
    /// Build ExecCaps borrowed from this struct.
    /// Lifetime tied to `&self` — cannot cross tokio::spawn.
    pub fn exec<'a>(&'a self) -> ExecCaps<'a> {
        ExecCaps {
            shell: &*self.shell,
            event_log: &self.event_log,
            cancel_token: &self.cancel_token,
            policy: &*self.policy_enforcer.read(),
            workflow_base_dir: &self.workflow_base_dir,
            working_dir_mode: self.working_dir_mode.as_deref(),
            project_root: self.project_root.as_deref(),
        }
    }

    /// Clone Arcs for use across tokio::spawn.
    /// Only called at the spawn point inside for_each loops.
    pub fn exec_owned(&self) -> ExecCapsOwned {
        ExecCapsOwned {
            shell: Arc::clone(&self.shell),
            event_log: self.event_log.clone(),
            cancel_token: self.cancel_token.clone(),
            policy_enforcer: Arc::clone(&self.policy_enforcer),
            workflow_base_dir: self.workflow_base_dir.clone(),
            working_dir_mode: self.working_dir_mode.clone(),
            project_root: self.project_root.clone(),
        }
    }

    pub fn fetch<'a>(&'a self) -> FetchCaps<'a> { ... }
    pub fn fetch_owned(&self) -> FetchCapsOwned { ... }

    pub fn invoke<'a>(&'a self) -> InvokeCaps<'a> { ... }
    pub fn invoke_owned(&self) -> InvokeCapsOwned { ... }
}

// ── Borrowed caps (for regular task dispatch) ─────────────────────────────

/// Borrowed capability set for exec: verb.
/// Fields map 1:1 to the TaskExecutor fields consumed by run_exec.
pub struct ExecCaps<'a> {
    pub shell: &'a dyn ShellExecutor,
    pub event_log: &'a EventLog,
    pub cancel_token: &'a CancellationToken,
    pub policy: &'a PolicyEnforcer,
    pub workflow_base_dir: &'a std::path::Path,
    pub working_dir_mode: Option<&'a str>,
    pub project_root: Option<&'a std::path::Path>,
}

/// Owned version — clones all Arcs. Only for tokio::spawn crossing.
pub struct ExecCapsOwned {
    pub shell: Arc<dyn ShellExecutor>,
    pub event_log: EventLog,
    pub cancel_token: CancellationToken,
    pub policy_enforcer: Arc<parking_lot::RwLock<PolicyEnforcer>>,
    pub workflow_base_dir: PathBuf,
    pub working_dir_mode: Option<String>,
    pub project_root: Option<PathBuf>,
}

impl ExecCapsOwned {
    /// Borrow back into ExecCaps for the actual run call.
    pub fn borrow(&self) -> ExecCaps<'_> {
        ExecCaps {
            shell: &*self.shell,
            event_log: &self.event_log,
            cancel_token: &self.cancel_token,
            policy: &*self.policy_enforcer.read(),
            workflow_base_dir: &self.workflow_base_dir,
            working_dir_mode: self.working_dir_mode.as_deref(),
            project_root: self.project_root.as_deref(),
        }
    }
}

/// fetch: borrowed caps
pub struct FetchCaps<'a> {
    pub http: &'a dyn HttpClient,
    pub blob: &'a dyn BlobStore,
    pub event_log: &'a EventLog,
    pub cancel_token: &'a CancellationToken,
    pub policy: &'a PolicyEnforcer,
    pub aux: &'a FetchAux,
}

pub struct FetchCapsOwned { ... } // same Arc pattern

/// invoke: borrowed caps
pub struct InvokeCaps<'a> {
    pub builtin_router: &'a dyn BuiltinRouter,
    pub mcp_pool: &'a dyn MpcPool,
    pub event_log: &'a EventLog,
    pub cancel_token: &'a CancellationToken,
}

pub struct InvokeCapsOwned { ... }

// ── FetchAux ─────────────────────────────────────────────────────────────

/// Auxiliary fetch dependencies that are optional or fetch-specific.
/// Bundled to avoid a 7-argument function signature.
#[derive(Clone)]
pub struct FetchAux {
    pub robots: Option<Arc<dyn RobotsChecker>>,
    pub rate_limiter: Option<Arc<dyn DomainRateLimiter>>,
    pub cookie_jar: Arc<dyn CookieJar>,
    pub cache: Arc<dyn ResponseCache>,
    pub allowed_hosts: Arc<Vec<String>>,  // for SSRF redirect closure
}
```

**Workspace changes** — `/Users/thibaut/dev/supernovae/nika/tools/Cargo.toml`:
- Add `"nika-runtime"` to `[workspace.members]`
- Add `nika-runtime = { path = "nika-runtime", version = "0.79.0" }` to `[workspace.dependencies]`

**TDD tests** in `nika-runtime/src/capabilities.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_caps_owned_round_trip() {
        // ExecCapsOwned::borrow() must return a valid ExecCaps
        // Test: build a minimal ExecCapsOwned with mock shell, call borrow(),
        // verify the workflow_base_dir reference is correct.
    }

    #[test]
    fn fetch_aux_clone_is_cheap() {
        // FetchAux clone does not allocate new Arc contents
        // Test: clone a FetchAux, verify Arc::ptr_eq on all fields
    }
}
```

Expected failure before implementation: `error[E0432]: unresolved import 'nika_policy'` — confirms nika-policy dependency wired.

**Verification:** `cargo check -p nika-runtime`

**Rollback:** `git reset HEAD~1`

---

### Commit 1.2 — `feat(runtime): dispatch() skeleton with todo!() arms`

**File:** `/Users/thibaut/dev/supernovae/nika/tools/nika-runtime/src/dispatch.rs`

```rust
use nika_core::ast::TaskAction;
use nika_core::binding::ResolvedBindings;
use nika_event::EventLog;

use crate::capabilities::VerbCapabilities;
use crate::error::RuntimeError;

// RunContext lives in nika-engine today; it will move to nika-runtime in S14.
// For now we re-export it via a type alias so verb crates can use the canonical path.
// This is the ONLY allowed cross-dependency on nika-engine from nika-runtime during S13.
// It will be dissolved in S14.
pub use nika_engine::store::RunContext;

/// Dispatch a task action to the appropriate verb crate.
///
/// This is the 12-line heart of nika-runtime. Each arm builds per-verb caps
/// and calls the free function from the corresponding verb crate.
/// During S13, infer and agent arms remain todo!() — they will be filled in S14.
pub async fn dispatch(
    action: &TaskAction,
    task_id: &std::sync::Arc<str>,
    bindings: &ResolvedBindings,
    rc: &RunContext,
    vc: &VerbCapabilities,
) -> Result<String, RuntimeError> {
    match action {
        TaskAction::Exec(params) => {
            todo!("S13.P2 — filled when nika-verb-exec is wired")
        }
        TaskAction::Invoke(params) => {
            todo!("S13.P3 — filled when nika-verb-invoke is wired")
        }
        TaskAction::Fetch(params) => {
            todo!("S13.P4 — filled when nika-verb-fetch is wired")
        }
        TaskAction::Infer(_) | TaskAction::Agent(_) => {
            todo!("S14 — infer and agent extraction")
        }
    }
}
```

Add `RuntimeError` type:

`/Users/thibaut/dev/supernovae/nika/tools/nika-runtime/src/error.rs`

```rust
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error(transparent)]
    Exec(#[from] nika_verb_exec::ExecError),
    #[error(transparent)]
    Fetch(#[from] nika_verb_fetch::FetchError),
    #[error(transparent)]
    Invoke(#[from] nika_verb_invoke::InvokeError),
    // Infer/Agent errors added in S14
}
```

Note: The `From<*Error> for NikaError` impls live in `nika-engine` (which still owns `NikaError`). `RuntimeError` is `nika-runtime`'s own error. `nika-engine`'s task dispatch adds `impl From<RuntimeError> for NikaError` to bridge. No circular dep: nika-engine depends on nika-runtime, never the reverse.

**Cargo.toml update:** Add `nika-engine = { workspace = true }` to nika-runtime deps with a comment: `# TEMPORARY: RunContext lives here until S14 moves it. Remove in S14.`

**Verification:** `cargo check -p nika-runtime`

**Rollback:** `git reset HEAD~1`

---

### Commit 1.3 — `feat(runtime): move Runner from nika-engine to nika-runtime`

This is the largest commit in Part 1. The Runner is the DAG execution loop.

**Files to move:**
- `nika-engine/src/runtime/runner/mod.rs` → `nika-runtime/src/runner/mod.rs`
- `nika-engine/src/runtime/task_dispatch.rs` → `nika-runtime/src/task_dispatch.rs`
- Any sub-files in `runner/` if present

**Process:**
1. Copy files verbatim into nika-runtime/src/
2. Fix all `crate::` prefixes — many will need to become `nika_engine::` or references to types that must be re-exported
3. Key imports that need updating:
   - `crate::runtime::executor::TaskExecutor` — stays in nika-engine for now; nika-runtime takes `Arc<TaskExecutor>` via a new `ExecutorAdapter` trait OR (simpler for S13) the Runner struct keeps a `Arc<TaskExecutor>` field typed as `nika_engine::runtime::executor::TaskExecutor`. This means nika-runtime temporarily depends on nika-engine for the executor. Document this as temporary.
   - `crate::store::RunContext` — nika-runtime exposes the re-export defined in commit 1.2
   - `crate::event::*` — replace with `nika_event::*`
   - `crate::error::NikaError` — keeps as `nika_engine::error::NikaError` for now

**Compat shim in nika-engine:**

`/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/runner/mod.rs` becomes:

```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
// Shim: Runner has moved to nika-runtime. Re-export for callers during S13.
// TODO(S14): Remove this file when all callers updated.
pub use nika_runtime::runner::*;
```

`/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/task_dispatch.rs` becomes a similar one-liner re-export shim.

**Cargo.toml — nika-engine/Cargo.toml:** Add `nika-runtime = { workspace = true }`.

**Cargo.toml — nika-runtime/Cargo.toml:** Add all deps needed by runner:
```toml
nika-engine = { workspace = true }   # Temporary for TaskExecutor + RunContext
nika-mcp = { workspace = true }      # McpClientPool (runner uses it for MCP shutdown)
nika-media = { workspace = true }    # CasStore + lockfile
indexmap = { workspace = true }
petgraph = { workspace = true }
colored = { workspace = true }
tokio = { workspace = true }
futures = { workspace = true }
tracing = { workspace = true }
```

**Circular dep check:** nika-runtime depends on nika-engine (for TaskExecutor, RunContext). nika-engine depends on nika-runtime (for Runner re-export). This is circular. **Resolution:** Move `RunContext` and `store::*` out of nika-engine to break the cycle. This is a prerequisite micro-move within this commit. `RunContext` + `TaskResult` (~800 LOC) move to nika-runtime, and nika-engine re-exports from nika-runtime. Alternatively, `RunContext` moves to nika-core (L0). Given RunContext has no I/O (it's a data store backed by a `DashMap<String, TaskResult>`), moving it to nika-core is the correct architectural decision — it's a pure data structure.

**Concrete plan for circular dep resolution (within commit 1.3):**
1. Move `nika-engine/src/store.rs` to `nika-core/src/store.rs`
2. nika-engine re-exports: `pub use nika_core::store::*;`
3. nika-runtime imports from `nika_core::store::RunContext`
4. Circular dep dissolved: nika-runtime → nika-core, nika-engine → nika-runtime (no cycle)

**TDD test name:** `runner_moves_to_nika_runtime` — run `cargo test -p nika-runtime --lib` and verify the Runner tests (copied from nika-engine) pass.

**Verification:** `cargo test --workspace --lib` — must have same count as before.

**Rollback:** `git reset HEAD~1` (git history is the checkpoint)

---

### Commit 1.4 — `chore(engine): wire nika-engine to use nika-runtime Runner`

After commit 1.3 sets up the shim, verify the full workspace compiles and tests pass cleanly.

**Files modified:**
- `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/mod.rs` — any direct instantiation of `Runner` struct is now via `nika_runtime::runner::Runner`
- Verify no test in nika-engine imports `crate::runtime::runner` directly (they should use the re-export)

**Verification:** `cargo test --workspace --lib` — zero regressions.

---

## Part 2 — `nika-verb-exec` Extraction (4 commits)

### Commit 2.1 — `feat(verb-exec): create nika-verb-exec L2 crate`

**Files to create:**

`/Users/thibaut/dev/supernovae/nika/tools/nika-verb-exec/Cargo.toml`

```toml
[package]
name = "nika-verb-exec"
version.workspace = true
edition.workspace = true
authors.workspace = true
description = "Exec verb — shell command execution for Nika workflows (L2)"
license.workspace = true
repository.workspace = true
rust-version.workspace = true
publish = true

[dependencies]
nika-core = { workspace = true }
nika-kernel = { workspace = true }
nika-event = { workspace = true }
nika-policy = { workspace = true }
tokio = { workspace = true }
tokio-util = { workspace = true }
tracing = { workspace = true }
thiserror = { workspace = true }
regex = { workspace = true }
shlex = { workspace = true }

[dev-dependencies]
tokio = { workspace = true, features = ["rt", "macros"] }
nika-kernel-mock = { workspace = true }

[lints]
workspace = true
```

`/Users/thibaut/dev/supernovae/nika/tools/nika-verb-exec/src/lib.rs`

```rust
pub mod error;
pub mod run;
pub mod sec;    // shell injection / security validation functions

pub use error::ExecError;
pub use run::run;
```

`/Users/thibaut/dev/supernovae/nika/tools/nika-verb-exec/src/error.rs`

```rust
#[derive(Debug, thiserror::Error)]
pub enum ExecError {
    #[error("Command failed: {reason}")]
    ExecFailed { reason: String },
    #[error("Blocked command (NIKA-053): {reason}")]
    BlockedCommand { command: String, reason: String },
    #[error("Policy violation: {reason}")]
    PolicyViolation { reason: String },
    #[error("Task cancelled: {task_id}")]
    TaskCancelled { task_id: String, reason: String },
    #[error("Template error: {0}")]
    Template(String),
}
```

`/Users/thibaut/dev/supernovae/nika/tools/nika-verb-exec/src/sec.rs`

Move the three free functions currently defined in `nika-engine/src/runtime/security.rs` that are exec-specific:
- `validate_exec_command_full(cmd, is_shell, raw_template) -> Result<(), ExecError>`
- `check_shell_data_injection(raw_template, resolved) -> Result<(), ExecError>`
- `strip_sensitive_env_vars(cmd: &mut tokio::process::Command)`
- `validate_env_vars(pairs: &[(String, String)]) -> Result<(), ExecError>`

Also move `is_inside_single_quotes` and `value_safe_in_single_quotes` from `exec.rs` into `sec.rs`.

`/Users/thibaut/dev/supernovae/nika/tools/nika-verb-exec/src/run.rs`

```rust
use std::sync::Arc;
use std::time::Instant;

use nika_core::ast::ExecParams;
use nika_core::binding::ResolvedBindings;
use nika_core::store::RunContext;       // moved to nika-core in commit 1.3
use nika_kernel::shell::ShellExecutor;
use nika_event::{EventKind, EventLog};
use nika_policy::{PolicyChecker, PolicyDecision};
use tokio_util::sync::CancellationToken;

use crate::error::ExecError;
use crate::sec::*;

/// Execute a shell command defined by `params`.
///
/// This is a line-for-line port of `TaskExecutor::run_exec` with the receiver
/// replaced by `ExecCaps<'_>`. All security checks are preserved verbatim.
pub async fn run(
    task_id: &Arc<str>,
    params: &ExecParams,
    bindings: &ResolvedBindings,
    datastore: &RunContext,
    shell: &dyn ShellExecutor,
    event_log: &EventLog,
    cancel_token: &CancellationToken,
    policy: &dyn PolicyChecker,
    workflow_base_dir: &std::path::Path,
    working_dir_mode: Option<&str>,
    project_root: Option<&std::path::Path>,
) -> Result<String, ExecError> {
    // [body is the line-for-line port from exec.rs, replacing self.* with the
    //  explicit parameter names. The body is NOT reproduced here — the
    //  implementor copies exec.rs verbatim and mechanically replaces:]
    //  self.event_log       → event_log
    //  self.policy_enforcer.read().check_exec(...)  → policy.check_exec(...)
    //  self.cancel_token    → cancel_token
    //  self.workflow_base_dir → workflow_base_dir
    //  self.resolve_default_exec_cwd() → resolve_default_exec_cwd(working_dir_mode, workflow_base_dir, project_root)
    //
    // NOTE: The `shell` parameter is the ShellExecutor TRAIT, but exec.rs
    // currently uses tokio::process::Command directly. During S13, the function
    // continues to use tokio::process::Command (same as before). The ShellExecutor
    // trait will be fully wired in S14/S15 when TokioShell is fully adopted.
    // Document this in a comment: "// TODO(S14): use shell.run() instead of tokio::process"
    todo!()
}

/// Resolve the default exec working directory from config.
/// Extracted from TaskExecutor::resolve_default_exec_cwd().
fn resolve_default_exec_cwd<'a>(
    working_dir_mode: Option<&'a str>,
    workflow_base_dir: &'a std::path::Path,
    project_root: Option<&'a std::path::Path>,
) -> Option<&'a std::path::Path> {
    match working_dir_mode {
        Some("project") => project_root,
        Some("none") => None,
        _ => Some(workflow_base_dir),  // "workflow" or None → workflow default
    }
}
```

**TDD test names** in `nika-verb-exec/src/run.rs`:

```rust
#[cfg(test)]
mod tests {
    // test_exec_shell_mode_runs_command
    //   Build a mock ExecParams with shell:true, command:"echo hello"
    //   Call run(), assert Ok("hello")

    // test_exec_blocked_by_policy
    //   PolicyEnforcer configured to block "rm -rf"
    //   Call run() with command:"rm -rf /", assert Err(ExecError::PolicyViolation)

    // test_exec_unescaped_shell_binding_rejected
    //   params.command = "echo {{with.val}}", shell:true, no |shell transform
    //   Call run(), assert Err(ExecError::BlockedCommand) with reason containing "shell_injection"

    // test_exec_single_quote_context_safe
    //   params.command = "jq --arg x '{{with.val}}' '.'", val="no-single-quotes"
    //   Call run(), assert Ok(_)

    // test_exec_single_quote_breakout_blocked
    //   params.command = "jq --arg x '{{with.val}}' '.'", val="it's a trap"
    //   Call run(), assert Err(ExecError::BlockedCommand)

    // test_exec_cancellation
    //   cancel_token already cancelled before calling run()
    //   Call run() with a sleep command, assert Err(ExecError::TaskCancelled)
}
```

All tests use mock components from `nika-kernel-mock` or construct minimal structs inline.

**Verification:** `cargo test -p nika-verb-exec --lib`

**Rollback:** `git reset HEAD~1`

---

### Commit 2.2 — `feat(runtime): wire dispatch() Exec arm to nika_verb_exec::run()`

**File:** `/Users/thibaut/dev/supernovae/nika/tools/nika-runtime/src/dispatch.rs`

Replace the `TaskAction::Exec(params)` `todo!()`:

```rust
TaskAction::Exec(params) => {
    let caps = vc.exec();
    nika_verb_exec::run(
        task_id, params, bindings, rc,
        caps.shell, caps.event_log, caps.cancel_token, caps.policy,
        caps.workflow_base_dir, caps.working_dir_mode, caps.project_root,
    ).await.map_err(RuntimeError::Exec)
}
```

Add `nika-verb-exec = { workspace = true }` to `nika-runtime/Cargo.toml`.

**Verification:** `cargo check -p nika-runtime`

---

### Commit 2.3 — `refactor(engine): TaskExecutor::run_exec delegates to nika_verb_exec::run()`

This is the bridge commit. `exec.rs` in nika-engine is NOT deleted yet. Instead, the body is replaced with a delegation call.

**File:** `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/executor/exec.rs`

The `impl TaskExecutor { pub(super) async fn run_exec(…) }` body becomes:

```rust
pub(super) async fn run_exec(
    &self,
    task_id: &Arc<str>,
    params: &ExecParams,
    bindings: &ResolvedBindings,
    datastore: &RunContext,
) -> Result<String, NikaError> {
    // Bridge to nika-verb-exec. self.* fields projected into the free-function signature.
    // Remove this impl block in S13.P2 commit 2.4 after golden tests confirm parity.
    nika_verb_exec::run(
        task_id, params, bindings, datastore,
        &*self.shell,              // Arc<dyn ShellExecutor> — requires self.shell field (S12)
        &self.event_log,
        &self.cancel_token,
        &*self.policy_enforcer.read() as &dyn nika_kernel::policy::PolicyChecker,
        &self.workflow_base_dir,
        self.working_dir_mode.as_deref(),
        self.project_root.as_deref(),
    ).await.map_err(|e| NikaError::from_exec_error(e))
}
```

The coercion `&*self.policy_enforcer.read() as &dyn PolicyChecker` is safe because `parking_lot::RwLockReadGuard<PolicyEnforcer>` implements `Deref<Target = PolicyEnforcer>`, and S12 has added `impl PolicyChecker for PolicyEnforcer`. The guard is held for the duration of the borrow, which is fine because `run_exec` takes it only to pass a `&dyn PolicyChecker` reference and the exec verb doesn't hold the lock.

Add `nika-verb-exec = { workspace = true }` to `nika-engine/Cargo.toml`.

**Verification:** `cargo test --workspace --lib` — all exec tests must continue passing.

---

### Commit 2.4 — `chore(engine): delete exec.rs from nika-engine (-471 LOC)`

**Precondition:** Golden e2e tests pass. `cargo test --workspace --lib` zero failures.

**Files deleted:**
- `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/executor/exec.rs`

**File modified:**
- `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/executor/mod.rs` — remove `mod exec;` line

Also move `is_inside_single_quotes` and `value_safe_in_single_quotes` tests: they now live in `nika-verb-exec/src/sec.rs` (moved in commit 2.1), so the test coverage is preserved.

**Verification:** `cargo test --workspace --lib` — same count as before deletion, confirming no tests were lost.

---

## Part 3 — `nika-verb-invoke` Extraction (3 commits)

Invoke is the cleanest of the three. Its dependencies are: `mcp_pool` (McpClientPool), `builtin_router` (BuiltinToolRouter), `event_log`, `cancel_token`. No policy, no filesystem, no HTTP (it calls BuiltinRouter which may do HTTP, but that's inside the router, not in invoke.rs itself).

### Commit 3.1 — `feat(verb-invoke): create nika-verb-invoke L2 crate`

`/Users/thibaut/dev/supernovae/nika/tools/nika-verb-invoke/Cargo.toml`

```toml
[package]
name = "nika-verb-invoke"
# ... workspace fields ...

[dependencies]
nika-core = { workspace = true }
nika-kernel = { workspace = true }
nika-event = { workspace = true }
nika-mcp = { workspace = true }
nika-builtin = { workspace = true }
tokio = { workspace = true }
tokio-util = { workspace = true }
tracing = { workspace = true }
thiserror = { workspace = true }
serde_json = { workspace = true }
uuid = { workspace = true }
```

`/Users/thibaut/dev/supernovae/nika/tools/nika-verb-invoke/src/lib.rs`

```rust
pub mod error;
pub mod run;
pub use error::InvokeError;
pub use run::run;
```

`/Users/thibaut/dev/supernovae/nika/tools/nika-verb-invoke/src/run.rs`

```rust
/// Execute an invoke action (MCP tool call OR resource read).
///
/// invoke: supports two paths:
///   1. tool: "nika:*" → builtin_router.dispatch()
///   2. tool: "server::name" or resource: "uri" → mcp_pool.call()
///
/// Both paths are fully ported from TaskExecutor::run_invoke.
pub async fn run(
    task_id: &Arc<str>,
    invoke: &InvokeParams,
    bindings: &ResolvedBindings,
    datastore: &RunContext,
    builtin_router: &dyn BuiltinRouter,
    mcp_pool: &dyn McpPool,         // nika-kernel trait, S12
    event_log: &EventLog,
    cancel_token: &CancellationToken,
) -> Result<String, InvokeError> {
    todo!()
}
```

**Subtlety — MCP resource reads:** `invoke.resource` is set when the user writes `resource: "novanet://entity/123"` instead of `tool:`. The run function must handle both branches. The existing `run_invoke` body already handles this at lines 93–100 in invoke.rs with `resolved_resource`. Port both branches faithfully.

**TDD tests:**

```rust
// test_invoke_builtin_dispatches_to_router
//   invoke.tool = "nika:log", params = {"message": "hello"}
//   mock router returns Ok(r#"{"ok": true}"#)
//   assert Ok(contains "ok")

// test_invoke_mcp_tool_calls_pool
//   invoke.tool = "search::find", params = {"q": "rust"}
//   mock mcp_pool returns Ok(r#"{"results": []}"#)
//   assert Ok(contains "results")

// test_invoke_resource_read
//   invoke.resource = "novanet://entity/42", invoke.tool = None
//   mock mcp_pool.read_resource() returns Ok("entity data")
//   assert Ok("entity data")

// test_invoke_cancellation_races_deadline
//   cancel_token pre-cancelled
//   mcp call would block for 30s (INVOKE_TASK_DEADLINE)
//   assert Err(InvokeError::TaskCancelled) arrives quickly (< 100ms)
```

**Verification:** `cargo test -p nika-verb-invoke --lib`

---

### Commit 3.2 — `refactor(engine): TaskExecutor::run_invoke bridges to nika_verb_invoke::run()`

Same bridge pattern as exec. The body of `run_invoke` becomes a delegation. The coercion for `mcp_pool` depends on whether `McpClientPool` implements the `dyn McpPool` trait from nika-kernel. S12 should have added this; if not, add it here as a prerequisite micro-commit.

**File:** `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/executor/invoke.rs`

```rust
pub(super) async fn run_invoke(...) -> Result<String, NikaError> {
    nika_verb_invoke::run(
        task_id, invoke, bindings, datastore,
        &*self.builtin_router,
        &self.mcp_pool as &dyn nika_kernel::mcp::McpPool,
        &self.event_log,
        &self.cancel_token,
    ).await.map_err(NikaError::from_invoke_error)
}
```

Wire dispatch() invoke arm:

```rust
TaskAction::Invoke(params) => {
    let caps = vc.invoke();
    nika_verb_invoke::run(
        task_id, params, bindings, rc,
        caps.builtin_router, caps.mcp_pool, caps.event_log, caps.cancel_token,
    ).await.map_err(RuntimeError::Invoke)
}
```

**Verification:** `cargo test --workspace --lib`

---

### Commit 3.3 — `chore(engine): delete invoke.rs from nika-engine (-522 LOC)`

Same pattern as exec deletion. Remove `mod invoke;` from executor/mod.rs.

**Verification:** `cargo test --workspace --lib` — zero regressions.

---

## Part 4 — `nika-verb-fetch` Extraction (5 commits)

Fetch is the most complex: 1,399 LOC, custom reqwest client construction, SSRF redirect closure, binary CAS path, 4 optional aux dependencies. It's handled in more granular commits.

### Commit 4.1 — `feat(kernel): CookieJar + ResponseCache + DomainRateLimiter + RobotsChecker traits`

**File:** `/Users/thibaut/dev/supernovae/nika/tools/nika-kernel/src/fetch_aux.rs`

```rust
//! Auxiliary fetch traits — optional per-domain and session behaviors.

use async_trait::async_trait;

/// Cookie storage for session-persistent fetch tasks.
#[async_trait]
pub trait CookieJar: Send + Sync {
    fn cookies_for_url(&self, url: &str) -> Vec<(String, String)>;
    fn store_cookies(&self, url: &str, cookies: Vec<(String, String)>);
}

/// HTTP response cache (ETag / If-Modified-Since conditional requests).
#[async_trait]
pub trait ResponseCache: Send + Sync {
    fn get(&self, url: &str) -> Option<CachedResponse>;
    fn put(&self, url: &str, response: CachedResponse);
}

#[derive(Clone)]
pub struct CachedResponse {
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub body: String,
    pub status: u16,
}

/// Per-domain rate limiter for polite crawling.
#[async_trait]
pub trait DomainRateLimiter: Send + Sync {
    /// Acquire a permit for the given domain, waiting if necessary.
    async fn acquire(&self, domain: &str);
}

/// robots.txt compliance checker.
#[async_trait]
pub trait RobotsChecker: Send + Sync {
    /// Returns true if the URL is allowed by robots.txt.
    async fn is_allowed(&self, url: &str, user_agent: &str) -> bool;
}
```

Add `pub mod fetch_aux;` to `nika-kernel/src/lib.rs`.

**Design tradeoff note (must appear in commit message):** The `HttpClient` trait cannot express the SSRF-aware redirect closure that captures `allowed_hosts` at construction time (reqwest's redirect policy API requires a concrete `Fn` at builder time, not an async trait method). Therefore `nika-verb-fetch` retains a direct `reqwest` dependency for the per-request DNS-pinned client construction. This is the only justified `reqwest` use outside `nika-http`. All other HTTP goes through `dyn HttpClient`.

**Verification:** `cargo check -p nika-kernel`

---

### Commit 4.2 — `feat(runtime): FetchAux concrete wrappers in nika-engine`

Wire the four concrete structs that implement the four new traits against the existing concrete types in nika-engine:

- `impl CookieJar for reqwest_cookie_store::CookieStoreRwLock`
- `impl ResponseCache for crate::runtime::fetch_cache::FetchCache`
- `impl DomainRateLimiter for crate::runtime::rate_limit::DomainRateLimiter`
- `impl RobotsChecker for crate::runtime::robots::RobotsCache`

**File:** `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/fetch_adapters.rs`

These impls stay in nika-engine (the concrete types live there). `VerbCapabilities::fetch_aux` in nika-runtime holds `Arc<dyn CookieJar>` etc., pointing to these concrete instances via type-erasure. The construction happens in the Runner's `VerbCapabilities::new()`.

**Verification:** `cargo check -p nika-engine`

---

### Commit 4.3 — `feat(verb-fetch): create nika-verb-fetch L2 crate`

`/Users/thibaut/dev/supernovae/nika/tools/nika-verb-fetch/Cargo.toml`

```toml
[package]
name = "nika-verb-fetch"
# ... workspace fields ...

[dependencies]
nika-core = { workspace = true }
nika-kernel = { workspace = true }
nika-event = { workspace = true }
nika-policy = { workspace = true }
nika-extract = { workspace = true }   # S12 crate — content extraction
nika-blob = { workspace = true }      # BlobStore impl via trait
tokio = { workspace = true }
tokio-util = { workspace = true }
tracing = { workspace = true }
thiserror = { workspace = true }
futures = { workspace = true }
reqwest = { workspace = true }        # EXCEPTION — see below
url = { workspace = true }
serde_json = { workspace = true }

# REQWEST EXCEPTION: nika-verb-fetch retains a direct reqwest dependency
# because the SSRF-aware DNS-pinned redirect closure must be constructed
# at reqwest::Client::builder() time. The nika-kernel::HttpClient trait
# cannot express this construction-time policy. All other HTTP in the
# codebase uses dyn HttpClient. This is the ONLY exception.
```

`/Users/thibaut/dev/supernovae/nika/tools/nika-verb-fetch/src/lib.rs`

```rust
pub mod error;
pub mod run;
pub mod ssrf;     // SSRF closure construction (was nika-engine/runtime/policy.rs::ssrf_*)

pub use error::FetchError;
pub use run::run;
```

`/Users/thibaut/dev/supernovae/nika/tools/nika-verb-fetch/src/run.rs` — free function signature:

```rust
pub async fn run(
    task_id: &Arc<str>,
    fetch: &FetchParams,
    bindings: &ResolvedBindings,
    datastore: &RunContext,
    // Core HTTP — uses the shared reqwest::Client from VerbCapabilities
    // (which is NOT a trait object — it's the actual reqwest::Client).
    // For DNS-pinned requests a new one-off client is constructed inline (see comment above).
    shared_http_client: &reqwest::Client,
    event_log: &EventLog,
    cancel_token: &CancellationToken,
    policy: &dyn PolicyChecker,
    blob: &dyn BlobStore,
    // Aux
    aux: &FetchAux,
) -> Result<String, FetchError>
```

Note: `shared_http_client` is `&reqwest::Client`, NOT `&dyn HttpClient`. This is intentional because the fetch verb constructs one-off clients by calling `reqwest::Client::builder()` for DNS pinning, and needs to share the same connection pool for the common case. The `HttpClient` trait path is used for the standard (non-pinned) fetch. Both paths exist in the ported code.

**TDD tests:**

```rust
// test_fetch_ssrf_localhost_blocked
//   url = "http://localhost/secret"
//   policy blocks it → assert Err(FetchError::PolicyViolation)

// test_fetch_robots_txt_blocks_disallowed_url
//   mock RobotsChecker returns false for the URL
//   assert Err(FetchError::PolicyViolation { reason: contains "robots.txt" })

// test_fetch_binary_response_stored_in_cas
//   response body = b"PNG bytes"
//   fetch.response = Some(ResponseMode::Binary)
//   mock BlobStore receives store() call
//   assert Ok(json containing "hash")

// test_fetch_response_full_includes_headers
//   response headers = {"content-type": "text/html"}
//   fetch.response = Some(ResponseMode::Full)
//   assert Ok(json containing "headers" and "status")

// test_fetch_rate_limit_delay_emitted
//   mock DomainRateLimiter adds 100ms delay
//   assert EventLog contains RateLimitDelay event

// test_fetch_cancellation
//   cancel_token pre-cancelled
//   assert Err(FetchError::TaskCancelled) immediately

// test_fetch_redirect_ssrf_bypass_blocked
//   First request → 301 to 169.254.169.254
//   assert Err blocked (SSRF redirect hook fires)
```

**Verification:** `cargo test -p nika-verb-fetch --lib`

---

### Commit 4.4 — `refactor(engine): TaskExecutor::run_fetch bridges to nika_verb_fetch::run()`

**File:** `/Users/thibaut/dev/supernovae/nika/tools/nika-engine/src/runtime/executor/fetch.rs`

The 1,399-LOC body is replaced by:

```rust
pub(super) async fn run_fetch(
    &self,
    task_id: &Arc<str>,
    fetch: &FetchParams,
    bindings: &ResolvedBindings,
    datastore: &RunContext,
) -> Result<String, NikaError> {
    let fetch_aux = FetchAux {
        robots: self.robots_cache.as_ref().map(|r| r.clone() as Arc<dyn RobotsChecker>),
        rate_limiter: self.domain_rate_limiter.as_ref().map(|l| l.clone() as Arc<dyn DomainRateLimiter>),
        cookie_jar: Arc::clone(&self.cookie_jar) as Arc<dyn CookieJar>,
        cache: Arc::clone(&self.fetch_cache) as Arc<dyn ResponseCache>,
        allowed_hosts: Arc::new(self.policy_enforcer.read().allowed_hosts().to_vec()),
    };
    nika_verb_fetch::run(
        task_id, fetch, bindings, datastore,
        &self.http_client,
        &self.event_log,
        &self.cancel_token,
        &*self.policy_enforcer.read() as &dyn PolicyChecker,
        &*self.cas as &dyn BlobStore,
        &fetch_aux,
    ).await.map_err(NikaError::from_fetch_error)
}
```

The helper free functions (`safe_backoff_delay`, `read_body_with_limit`, `read_bytes_with_limit`, `is_html_content_type`) move to `nika-verb-fetch` as `pub(crate)` functions.

**Verification:** `cargo test --workspace --lib`

---

### Commit 4.5 — `chore(engine): delete fetch.rs from nika-engine (-1,399 LOC)`

Remove `mod fetch;` from executor/mod.rs. Confirm no other file in nika-engine imports from `executor::fetch` directly (they access via `TaskExecutor::run_fetch` only).

**Verification:**
1. `cargo test --workspace --lib` — zero regressions
2. `wc -l tools/nika-engine/src/**/*.rs | tail -1` — confirm nika-engine has shrunk by ~1,400 LOC

---

## Part 5 — Session Close (2 commits)

### Commit 5.1 — `docs(constellation): update ARCHITECTURE.md — S13 complete, engine -2392 LOC`

**File:** `/Users/thibaut/dev/supernovae/nika/docs/ARCHITECTURE.md`

Update the crate diagram to show:
- `nika-runtime (L3)` added with Runner + dispatch
- `nika-verb-exec (L2)`, `nika-verb-invoke (L2)`, `nika-verb-fetch (L2)` added
- `nika-engine` LOC reduced from 149k to ~147k (exec+invoke+fetch removed, shims added)
- "S13: 3/5 verb crates done" annotation
- Note: infer and agent remain in nika-engine pending S14

Update `[workspace.members]` and `[workspace.dependencies]` count from 28 to 32 crates.

### Commit 5.2 — `docs: session13 memory + MEMORY.md update`

Create `/Users/thibaut/dev/supernovae/docs/sprints/CONSTELLATION-V2.3-AGGRESSIVE-TARGETS.md` update (private, not in nika/) or the memory file per the existing pattern at `/Users/thibaut/.claude/projects/-Users-thibaut-dev-supernovae-nika/memory/`. The key session notes:
- New crates: nika-runtime, nika-verb-exec, nika-verb-invoke, nika-verb-fetch
- nika-engine LOC: 149k → ~147k
- RunContext moved to nika-core
- `ExecCapsOwned` / `FetchCapsOwned` pattern documented
- FetchAux 4 traits in nika-kernel
- reqwest exception documented

---

## Special Concerns — Detailed Resolutions

### 1. Bridge Pattern

The bridge is commit 2.3 (exec), 3.2 (invoke), and 4.4 (fetch). In each case: the old `impl TaskExecutor { pub(super) async fn run_*(…) }` body is replaced with a delegation to the new free function. The existing callers in `task_dispatch.rs` continue calling `executor.run_exec(…)` without any changes. The bridge is fully transparent. This means S13 can land in 15–18 commits while `cargo test --workspace --lib` stays green throughout.

### 2. policy_enforcer Coercion

The `&*self.policy_enforcer.read() as &dyn PolicyChecker` construct works as follows:
- `self.policy_enforcer` is `Arc<parking_lot::RwLock<PolicyEnforcer>>`
- `.read()` returns `parking_lot::RwLockReadGuard<'_, PolicyEnforcer>`
- `*guard` dereferences to `PolicyEnforcer`
- `&*guard` is `&PolicyEnforcer`
- The coercion `as &dyn PolicyChecker` requires `PolicyEnforcer: PolicyChecker` (S12 establishes this)
- The guard is dropped when `run_exec` returns — safe because the exec verb calls `policy.check_exec()` synchronously before the async part begins, so no guard survives an `.await` point

If `check_exec` needs to be called across an `await` (e.g., inside a retry loop), clone the decision before the await: `let decision = policy.check_exec(&cmd); drop(guard);` then match on `decision`. This is already done in the existing exec.rs body — the pattern carries over.

### 3. EventKind Migration

No migration needed. `EventKind` and `EventLog` already live in `nika-event` (L1). `nika-engine/src/event/mod.rs` is already a one-line re-export (`pub use nika_event::*;`). Verb crates add `nika-event = { workspace = true }` as a direct dependency. They emit `EventKind::ExecCompleted`, `EventKind::PolicyBlocked`, `EventKind::McpInvoke` etc. directly without touching nika-engine.

### 4. Error Conversion at Dispatch Boundary

The dependency graph for errors:
- `nika-verb-exec` defines `ExecError`
- `nika-verb-invoke` defines `InvokeError`
- `nika-verb-fetch` defines `FetchError`
- `nika-runtime` defines `RuntimeError { Exec(ExecError), Fetch(FetchError), Invoke(InvokeError) }`
- `nika-engine` defines `NikaError` and adds:
  ```rust
  impl From<RuntimeError> for NikaError { ... }
  // or equivalently:
  fn from_exec_error(e: ExecError) -> NikaError { ... }
  ```

Dependency direction: `nika-engine → nika-runtime → nika-verb-*`. No cycle. `NikaError` is only in nika-engine (for now). Verb crates never import `NikaError`.

### 5. for_each and tokio::spawn

The `for_each` with `concurrency > 1` path in `task_dispatch.rs` uses `tokio::spawn`. Borrowed `ExecCaps<'a>` cannot cross `spawn` because `'a` is tied to `&TaskExecutor` which is stack-local. The solution:

In `task_dispatch.rs` (now in nika-runtime), when `concurrency > 1`:
```rust
let caps_owned = vc.exec_owned();  // clones all Arcs
tokio::spawn(async move {
    let caps = caps_owned.borrow();  // re-borrows from the owned version
    nika_verb_exec::run(task_id, params, bindings, rc, caps.shell, ...).await
});
```

`ExecCapsOwned` is `'static` because it contains only `Arc<_>`, `String`, `PathBuf` — all owned types. `borrow()` returns `ExecCaps<'_>` with lifetime tied to the `ExecCapsOwned`, which lives inside the `async move` closure. This is sound.

The same `*Owned` pattern applies to `FetchCapsOwned` and `InvokeCapsOwned`.

### 6. Cancellation Propagation

The root `CancellationToken` is created in the Runner. It is stored in `VerbCapabilities.cancel_token`. Each verb crate receives it as `&CancellationToken`. The `tokio::select!` blocks in each verb use `cancel_token.cancelled()` as the cancellation arm. In the `exec_owned()` clone path: `CancellationToken::clone()` creates a child token linked to the same tree — cancelling the parent cancels all children. The cloned token inside `ExecCapsOwned` is therefore correctly cancelled when the workflow is cancelled.

---

## Build Sequence Checklist

```
Part 0 — Gate check
[ ] cargo check -p nika-policy (S12 output)
[ ] cargo check -p nika-extract (S12 output)
[ ] grep PolicyChecker nika-kernel/src/**/*.rs
[ ] Verify *Caps structs exist (in nika-engine or nika-runtime from S12)

Part 1 — nika-runtime scaffold
[ ] Commit 1.1 — nika-runtime crate + VerbCapabilities struct + ExecCapsOwned/FetchCapsOwned/InvokeCapsOwned
[ ] cargo check -p nika-runtime
[ ] Commit 1.2 — dispatch() skeleton + RuntimeError
[ ] cargo check -p nika-runtime
[ ] Commit 1.3 — Move Runner; move RunContext to nika-core; compat shims in nika-engine
[ ] cargo test --workspace --lib (must pass, same count)
[ ] Commit 1.4 — Verify engine uses nika-runtime::Runner cleanly
[ ] cargo test --workspace --lib

Part 2 — nika-verb-exec
[ ] Commit 2.1 — Create nika-verb-exec, port exec.rs body, write 6 TDD tests
[ ] cargo test -p nika-verb-exec --lib (6 tests green)
[ ] Commit 2.2 — Wire dispatch() Exec arm
[ ] cargo check -p nika-runtime
[ ] Commit 2.3 — Bridge TaskExecutor::run_exec → nika_verb_exec::run()
[ ] cargo test --workspace --lib (zero regressions)
[ ] Commit 2.4 — Delete exec.rs from nika-engine
[ ] cargo test --workspace --lib (same count)

Part 3 — nika-verb-invoke
[ ] Commit 3.1 — Create nika-verb-invoke, port invoke.rs body, write 4 TDD tests
[ ] cargo test -p nika-verb-invoke --lib (4 tests green)
[ ] Commit 3.2 — Bridge run_invoke + wire dispatch() Invoke arm
[ ] cargo test --workspace --lib
[ ] Commit 3.3 — Delete invoke.rs from nika-engine
[ ] cargo test --workspace --lib

Part 4 — nika-verb-fetch
[ ] Commit 4.1 — 4 new traits in nika-kernel::fetch_aux
[ ] cargo check -p nika-kernel
[ ] Commit 4.2 — Concrete wrappers (fetch_adapters.rs) in nika-engine
[ ] cargo check -p nika-engine
[ ] Commit 4.3 — Create nika-verb-fetch (reqwest exception documented), write 7 TDD tests
[ ] cargo test -p nika-verb-fetch --lib (7 tests green)
[ ] Commit 4.4 — Bridge run_fetch + wire dispatch() Fetch arm
[ ] cargo test --workspace --lib (zero regressions)
[ ] Commit 4.5 — Delete fetch.rs from nika-engine
[ ] cargo test --workspace --lib (same count, verify -1399 LOC)

Part 5 — Close
[ ] Commit 5.1 — ARCHITECTURE.md update (S13 done, 4 new crates, engine -2392 LOC)
[ ] Commit 5.2 — Memory file update
[ ] Final: cargo test --workspace --lib; cargo clippy --workspace -- -D warnings
```

---

## Critical Details

**Workspace count:** 28 → 32 members. Update `[workspace.members]` in `/Users/thibaut/dev/supernovae/nika/tools/Cargo.toml` in commit 1.1 (add nika-runtime) with separate additions for each verb crate in their respective first commits.

**No `pub use nika_engine::*` in verb crates.** Each verb crate's public API is exactly: the `run()` free function and the `*Error` type. Nothing else is exported.

**`nika-exec-runner` crate already exists** at `/Users/thibaut/dev/supernovae/nika/tools/nika-exec-runner/`. This is the `TokioShell` concrete impl of `ShellExecutor`. `nika-verb-exec` does NOT depend on it directly — the verb crate receives `&dyn ShellExecutor`. The engine wires the concrete `TokioShell` when building `VerbCapabilities`. This preserves the L2 diamond layering.

**The `resolve_default_exec_cwd` helper** is currently a private method on `TaskExecutor`. It moves to a free function in `nika-verb-exec/src/run.rs`. Its logic (match on `working_dir_mode`) is ~10 lines and has no external deps.

**`redact_for_event`** is currently in `executor/verbs.rs`. It needs to move to `nika-event` (or be re-implemented inline) so verb crates can use it without depending on nika-engine. Moving it to `nika-event` is the right call — it's a utility for safe event emission and belongs there.

**Golden e2e tests:** Assume S12 added golden tests in `nika-engine/src/runtime/tests_e2e_workflow.rs` covering exec, fetch, and invoke verbs. Run these explicitly before each deletion commit: `cargo test -p nika-engine --lib -- tests_e2e_workflow`. If any golden test fails after a bridge commit, the bridge has a defect — do not proceed to the deletion commit.

**S13 does NOT touch infer or agent.** Those are left as `todo!()` in `dispatch()`. The shim in nika-engine (`task_dispatch.rs` → runner code) continues calling `executor.run_infer()` and `executor.run_agent()` directly, bypassing `dispatch()` entirely during S13. This is intentional and documented.