# Constellation Session 12 — Foundation Phase

- **Session ID:** S12-FOUNDATION
- **Date:** 2026-04-10
- **Duration estimate:** 6-9 hours wall clock (each commit 20-60 min)
- **Branch:** `main` (each commit landed independently, green `cargo test --workspace --lib` after every step)
- **Scope:** trait surface + new L1/L2 crates only. No verb extraction, no TaskExecutor deletion.
- **Outcome:** a set of additive commits that make Session 13 (verb extraction + `nika-runtime` creation) mechanical.

## Preconditions

The 6 D/P/E commits already landed on `main`:

| # | Commit | Status |
|---|--------|--------|
| D1 | `fix(agent): hard error on cwd lookup` | landed |
| D2 | `fix(fs): reject `..` in glob patterns` | landed |
| D3+E1 | `fix(cache): canonicalize + hard error` | landed |
| P1 | `feat(builtin): current_is_tainted() + BuiltinError::denied()` | landed |
| P2 | `refactor(builtin): file/limits.rs module` | landed |
| P3 | `test(builtin): file/test_util.rs run_as helper` | landed |

Tree is clean. `cargo test --workspace --lib` green. `cargo clippy --workspace -- -D warnings` clean.

You are on commit `c5ea27438` (or descendant). `nika-engine` ≈ 149k LOC. `TaskExecutor` still a 22-field god struct in `tools/nika-engine/src/runtime/executor/mod.rs`. `extract.rs` still lives under `runtime/executor/`. `PolicyEnforcer` still a concrete struct under `runtime/policy.rs`.

## Shared invariants (sacred — enforce on every commit)

1. **AGPL header** at the top of every new `.rs` file:
   ```
   // SPDX-License-Identifier: AGPL-3.0-or-later
   // Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
   ```
2. **Commit co-author** trailer: `Co-Authored-By: Nika 🦋 <nika@supernovae.studio>` — never Claude, never any human alongside Nika. 1 fix = 1 commit.
3. **Test gate:** `cargo test --workspace --lib` MUST pass after every commit. Always `--lib` to avoid the macOS Keychain popup. Do not run `cargo test --workspace` bare.
4. **Clippy gate:** `cargo clippy --workspace --lib --all-targets -- -D warnings` MUST pass after every commit.
5. **Zero unwrap/expect** in new code in hot paths. Any `expect()` requires a `// REASON:` comment documenting the invariant. Prefer `?` + `NikaError` or the crate-local error type.
6. **Additive only.** Every commit is additive or a mechanical move+re-export. No deletions that break callers, except commits 6 and 8 which delete code that was moved in the immediately preceding commit.
7. **Diamond layering:** new crates compile in isolation without `nika-engine` in their dep graph. Verified via `cargo tree -p <crate> | grep nika-engine` → empty.
8. **No new verbs, no schema bumps.** Schema stays `nika/workflow@0.12`.
9. **No `trait Verb`.** The agreed design uses free `pub async fn run()` per verb crate plus a future `enum TaskAction` + `match` in `nika-runtime`. Nothing in this session contradicts that.
10. **No `runtime.rs` creation.** The `nika-runtime` crate is a Session 13 deliverable.

## Commit map (10 commits)

| # | Type | Subject | LOC | Risk |
|---|------|---------|-----|------|
| 1 | feat | `feat(kernel): add PolicyChecker trait` | +80 | low |
| 2 | feat | `feat(kernel): HttpClient::send_streaming + HttpStreamResponse` | +130 | low-med |
| 3 | feat | `feat(kernel): cancellation support in ShellExecutor` | +90 | low-med |
| 4 | feat | `feat(kernel): split Filesystem into FsRead + FsWrite splinters` | +110 | low |
| 5 | feat | `feat(policy): create nika-policy crate at L1` | +1300 | med |
| 6 | chore | `chore(engine): remove duplicated policy code` | -1263 | med |
| 7 | feat | `feat(extract): create nika-extract crate at L2` | +1350 | med |
| 8 | chore | `chore(engine): delete runtime/executor/extract.rs` | -1327 | low |
| 9 | feat | `feat(kernel): add ExecCaps/FetchCaps/InferCaps/InvokeCaps/AgentCaps typed context structs` | +220 | low |
| 10 | docs | `docs(constellation): ARCHITECTURE.md + session12 memory update` | +150 | none |

Net delta on `nika-engine`: ≈ -2,590 LOC (149k → 146.4k). Net workspace: +2 crates (`nika-policy`, `nika-extract`).

---

## Commit 1 — `feat(kernel): add PolicyChecker trait`

Thin trait surface the `nika-runtime` and verb crates will consume to ask a policy question without pulling in the concrete enforcer. Concrete impl arrives in commit 5. No callers wired yet.

### Files touched

- `tools/nika-kernel/src/policy.rs` — NEW (~80 LOC)
- `tools/nika-kernel/src/lib.rs` — add `pub mod policy;`
- `tools/nika-kernel/Cargo.toml` — no dep changes (uses `thiserror` already)

### Code sketch

```rust
// tools/nika-kernel/src/policy.rs
// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! PolicyChecker trait — abstract security policy questions for verbs.
//!
//! Concrete impl lives in `nika-policy` (L1). Runtime and verb crates
//! consume this trait only, never the concrete `PolicyEnforcer`.

/// Policy decision returned by every check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDecision {
    Allow,
    Block(String),
    RequiresApproval(String),
}

impl PolicyDecision {
    pub fn is_allowed(&self) -> bool { matches!(self, Self::Allow) }
    pub fn is_blocked(&self) -> bool { matches!(self, Self::Block(_)) }
}

/// Errors returned by a policy checker.
#[derive(Debug, thiserror::Error)]
pub enum PolicyError {
    #[error("policy violation: {0}")]
    Violation(String),
    #[error("policy evaluation failed: {0}")]
    Internal(String),
}

/// The four questions a verb can ask the policy layer.
///
/// Object-safe (no generics, no `Self`-returning methods) so callers can
/// hold it as `Arc<dyn PolicyChecker>` in bundles / caps structs.
pub trait PolicyChecker: Send + Sync + std::fmt::Debug {
    fn check_exec(&self, command: &str) -> PolicyDecision;
    fn check_fetch(&self, url: &str) -> PolicyDecision;
    fn check_token_spend(&self, tokens: u64) -> PolicyDecision;
    fn is_host_allowed(&self, host: &str) -> bool;
}
```

### TDD

Failing test first in `tools/nika-kernel/src/policy.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct AlwaysAllow;
    impl PolicyChecker for AlwaysAllow {
        fn check_exec(&self, _: &str) -> PolicyDecision { PolicyDecision::Allow }
        fn check_fetch(&self, _: &str) -> PolicyDecision { PolicyDecision::Allow }
        fn check_token_spend(&self, _: u64) -> PolicyDecision { PolicyDecision::Allow }
        fn is_host_allowed(&self, _: &str) -> bool { true }
    }

    #[test]
    fn allow_is_allowed() {
        assert!(PolicyDecision::Allow.is_allowed());
        assert!(!PolicyDecision::Allow.is_blocked());
    }

    #[test]
    fn block_is_not_allowed() {
        let d = PolicyDecision::Block("denied".into());
        assert!(!d.is_allowed());
        assert!(d.is_blocked());
    }

    #[test]
    fn checker_is_object_safe() {
        let checker: std::sync::Arc<dyn PolicyChecker> = std::sync::Arc::new(AlwaysAllow);
        assert!(checker.check_exec("noop").is_allowed());
        assert!(checker.is_host_allowed("example.com"));
    }

    #[test]
    fn error_display() {
        let e = PolicyError::Violation("policy off".into());
        assert_eq!(format!("{e}"), "policy violation: policy off");
    }
}
```

### Verification

```bash
cargo test -p nika-kernel --lib policy::tests
cargo clippy -p nika-kernel --lib -- -D warnings
cargo test --workspace --lib
```

### Rollback

`git reset --hard c5ea27438` (pre-S12-foundation HEAD). Commit 1 is the first of the session; losing it leaves main in the pre-session state.

---

## Commit 2 — `feat(kernel): HttpClient::send_streaming + HttpStreamResponse`

The `fetch:` verb needs to abort downloads past ~50 MB BEFORE buffering them. The current `HttpClient::send()` returns a fully-buffered `HttpResponse` — unsuitable for early abort. This commit adds an additive streaming method with a default impl returning `HttpError::Unsupported`, so existing `ReqwestClient` and mocks keep compiling without changes.

### Files touched

- `tools/nika-kernel/src/http.rs` — add `HttpStreamResponse`, extend `HttpClient` trait, extend `HttpError`
- `tools/nika-kernel/Cargo.toml` — `futures-core.workspace = true` (already present)
- `tools/nika-http/src/lib.rs` — NOT touched; relies on default impl returning `Unsupported`

### Code sketch

```rust
// tools/nika-kernel/src/http.rs — APPEND to existing file

use futures_core::Stream;
use std::pin::Pin;

/// Streaming HTTP response. Body is delivered as an async stream of chunks
/// so callers can enforce size limits / extract partial content without buffering.
pub struct HttpStreamResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub final_url: String,
    pub content_length: Option<u64>,
    pub body: Pin<Box<dyn Stream<Item = Result<Bytes, HttpError>> + Send>>,
}

impl std::fmt::Debug for HttpStreamResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpStreamResponse")
            .field("status", &self.status)
            .field("headers", &self.headers)
            .field("final_url", &self.final_url)
            .field("content_length", &self.content_length)
            .field("body", &"<stream>")
            .finish()
    }
}

#[async_trait::async_trait]
pub trait HttpClient: Send + Sync {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, HttpError>;

    /// Send a request and return a streaming response. Used by `fetch:` for
    /// early-abort on size limits (50 MB hard cap by default). Default impl
    /// returns `HttpError::Unsupported` so existing mocks compile unchanged.
    async fn send_streaming(
        &self,
        _request: HttpRequest,
    ) -> Result<HttpStreamResponse, HttpError> {
        Err(HttpError::Unsupported {
            feature: "send_streaming".into(),
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("HTTP timeout after {duration_ms}ms")]
    Timeout { duration_ms: u64 },

    #[error("Connection error: {reason}")]
    Connection { reason: String },

    #[error("SSRF blocked: {url}")]
    SsrfBlocked { url: String },

    #[error("response exceeded size limit: {limit_bytes} bytes")]
    TooLarge { limit_bytes: u64 },

    #[error("feature unsupported by this client: {feature}")]
    Unsupported { feature: String },

    #[error("HTTP error: {reason}")]
    Other { reason: String },
}
```

`ReqwestClient` will NOT override `send_streaming` in this session — Session 13 wires the actual reqwest stream. This commit is purely a kernel surface extension.

### TDD

```rust
#[tokio::test]
async fn default_send_streaming_errors_unsupported() {
    use super::{HttpClient, HttpError, HttpRequest, HttpResponse};

    #[derive(Debug)]
    struct DummyClient;
    #[async_trait::async_trait]
    impl HttpClient for DummyClient {
        async fn send(&self, _: HttpRequest) -> Result<HttpResponse, HttpError> {
            unreachable!()
        }
    }

    let err = DummyClient.send_streaming(HttpRequest::get("https://x")).await.unwrap_err();
    assert!(matches!(err, HttpError::Unsupported { .. }));
}

#[test]
fn too_large_error_displays() {
    let e = HttpError::TooLarge { limit_bytes: 52_428_800 };
    assert_eq!(format!("{e}"), "response exceeded size limit: 52428800 bytes");
}
```

### Verification

```bash
cargo build -p nika-kernel
cargo build -p nika-http          # must still compile with default impl
cargo test -p nika-kernel --lib http::
cargo test --workspace --lib
cargo clippy --workspace --lib -- -D warnings
```

### Rollback

`git reset --hard HEAD~1`. Only the `HttpError::TooLarge` + `Unsupported` variants are net-new — no call site depends on them yet.

---

## Commit 3 — `feat(kernel): cancellation support in ShellExecutor`

The `exec:` verb has no cooperative cancellation path — a runaway shell command ignores workflow-level aborts. Add an optional `CancellationToken` (via `tokio_util::sync::CancellationToken`) to `ShellCommand`. Non-breaking: the field defaults to `None`.

### Files touched

- `tools/nika-kernel/Cargo.toml` — add `tokio-util = { workspace = true, features = ["sync"] }`
- `tools/nika-kernel/src/shell.rs` — extend `ShellCommand`, extend `ShellError`
- `tools/nika-exec-runner/Cargo.toml` — add `tokio-util` dep
- `tools/nika-exec-runner/src/lib.rs` — honor `cancel` in `run()` via `tokio::select!`

### Code sketch

```rust
// tools/nika-kernel/src/shell.rs — MODIFY ShellCommand + ShellError

use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone)]
pub struct ShellCommand {
    pub program: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub cwd: Option<PathBuf>,
    pub timeout: Option<Duration>,
    pub stdin: Option<String>,
    pub shell: bool,
    /// Cooperative cancellation token. When triggered, the executor kills the
    /// child process and returns `ShellError::Cancelled`.
    pub cancel: Option<CancellationToken>,
}

impl ShellCommand {
    /// Convenience constructor. `cancel` defaults to `None`.
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: HashMap::new(),
            cwd: None,
            timeout: None,
            stdin: None,
            shell: false,
            cancel: None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ShellError {
    #[error("Command not found: {program}")]
    NotFound { program: String },
    #[error("Command timed out after {duration_ms}ms")]
    Timeout { duration_ms: u64 },
    #[error("Command cancelled")]
    Cancelled,
    #[error("Blocked command (NIKA-053): {reason}")]
    Blocked { reason: String },
    #[error("Shell error: {reason}")]
    Other { reason: String },
}
```

```rust
// tools/nika-exec-runner/src/lib.rs — MODIFY the run() body

let child_fut = async {
    // existing spawn + wait_with_output logic
};
let result = match (command.timeout, command.cancel.clone()) {
    (None, None) => child_fut.await,
    (Some(t), None) => tokio::time::timeout(t, child_fut)
        .await
        .map_err(|_| ShellError::Timeout { duration_ms: t.as_millis() as u64 })?,
    (None, Some(tok)) => tokio::select! {
        biased;
        _ = tok.cancelled() => return Err(ShellError::Cancelled),
        r = child_fut => r,
    },
    (Some(t), Some(tok)) => tokio::select! {
        biased;
        _ = tok.cancelled() => return Err(ShellError::Cancelled),
        _ = tokio::time::sleep(t) => return Err(ShellError::Timeout { duration_ms: t.as_millis() as u64 }),
        r = child_fut => r,
    },
};
```

Call sites in `nika-engine` that build `ShellCommand` via struct literal must add `cancel: None,`. Grep shows ≤ 4 call sites. Fix them in this commit so the tree compiles.

### TDD

```rust
// tools/nika-exec-runner/src/lib.rs — test module

#[tokio::test]
async fn cancelled_before_start_returns_cancelled() {
    use tokio_util::sync::CancellationToken;
    let tok = CancellationToken::new();
    tok.cancel(); // pre-cancel
    let cmd = ShellCommand {
        program: "sleep".into(),
        args: vec!["10".into()],
        cancel: Some(tok),
        ..ShellCommand::new("sleep")
    };
    let res = TokioShell::new().run(cmd).await;
    assert!(matches!(res, Err(ShellError::Cancelled)));
}

#[tokio::test]
async fn cancelled_mid_flight_returns_cancelled() {
    use tokio_util::sync::CancellationToken;
    let tok = CancellationToken::new();
    let tok2 = tok.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        tok2.cancel();
    });
    let cmd = ShellCommand {
        program: "sleep".into(),
        args: vec!["5".into()],
        cancel: Some(tok),
        ..ShellCommand::new("sleep")
    };
    let res = TokioShell::new().run(cmd).await;
    assert!(matches!(res, Err(ShellError::Cancelled)));
}
```

### Verification

```bash
cargo test -p nika-kernel --lib shell::
cargo test -p nika-exec-runner --lib
cargo test --workspace --lib   # watch for broken struct literals
cargo clippy --workspace --lib -- -D warnings
```

### Rollback

`git reset --hard HEAD~1`. If engine struct-literal fixes are extensive, use `git revert <sha>` instead to keep history linear.

---

## Commit 4 — `feat(kernel): split Filesystem into FsRead + FsWrite splinters`

Per the scope rules in `nika-kernel/src/scope.rs`, verbs should depend only on the capabilities they actually use. Today every FS consumer pulls the entire `Filesystem` trait — including verbs that only read (`infer` context loading) or only write (`agent` trace writer). Split into two splinter traits and keep a blanket impl so every existing `Filesystem` impl satisfies the new contract.

### Files touched

- `tools/nika-kernel/src/filesystem.rs` — add `FsRead`, `FsWrite`, keep `Filesystem` as a convenience bound
- `tools/nika-fs/src/lib.rs` — split `impl Filesystem for TokioFs` into two `impl` blocks
- `tools/nika-kernel-mock/src/lib.rs` — same treatment for `InMemoryFs`

### Code sketch

```rust
// tools/nika-kernel/src/filesystem.rs — REPLACE existing trait

#[async_trait::async_trait]
pub trait FsRead: Send + Sync {
    async fn read(&self, path: &Path) -> std::io::Result<Bytes>;
    async fn read_to_string(&self, path: &Path) -> std::io::Result<String>;
    async fn metadata(&self, path: &Path) -> std::io::Result<FileMetadata>;
    async fn exists(&self, path: &Path) -> bool;
    async fn glob(&self, root: &Path, pattern: &str) -> std::io::Result<Vec<PathBuf>>;
    async fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf>;
}

#[async_trait::async_trait]
pub trait FsWrite: Send + Sync {
    async fn write(&self, path: &Path, contents: &[u8]) -> std::io::Result<()>;
    async fn create_dir_all(&self, path: &Path) -> std::io::Result<()>;
    async fn remove_file(&self, path: &Path) -> std::io::Result<()>;
}

/// Convenience alias — every existing `Filesystem` impl still satisfies this.
pub trait Filesystem: FsRead + FsWrite {}
impl<T: FsRead + FsWrite + ?Sized> Filesystem for T {}
```

### TDD

```rust
#[cfg(test)]
mod narrowing_tests {
    use super::*;

    fn take_read_only<R: FsRead>(_r: &R) {}
    fn take_both<F: Filesystem>(_f: &F) {}

    struct OnlyRead;
    #[async_trait::async_trait]
    impl FsRead for OnlyRead {
        async fn read(&self, _: &Path) -> std::io::Result<Bytes> { Ok(Bytes::new()) }
        async fn read_to_string(&self, _: &Path) -> std::io::Result<String> { Ok(String::new()) }
        async fn metadata(&self, _: &Path) -> std::io::Result<FileMetadata> {
            Ok(FileMetadata { len: 0, is_file: true, is_dir: false })
        }
        async fn exists(&self, _: &Path) -> bool { false }
        async fn glob(&self, _: &Path, _: &str) -> std::io::Result<Vec<PathBuf>> { Ok(vec![]) }
        async fn canonicalize(&self, _: &Path) -> std::io::Result<PathBuf> { Ok(PathBuf::new()) }
    }

    #[test]
    fn only_read_compiles_as_fs_read() {
        take_read_only(&OnlyRead);
    }
}
```

### Verification

```bash
cargo test -p nika-kernel --lib filesystem::
cargo build -p nika-fs
cargo build -p nika-kernel-mock
cargo test --workspace --lib
cargo clippy --workspace --lib -- -D warnings
```

### Rollback

`git reset --hard HEAD~1`. Because of the `Filesystem: FsRead + FsWrite` alias, no caller needs to change bounds.

---

## Commit 5 — `feat(policy): create nika-policy crate at L1`

Move `PolicyEnforcer` + SSRF helpers from `nika-engine/src/runtime/policy.rs` verbatim into a new L1 crate. Implement `nika_kernel::policy::PolicyChecker` for the relocated enforcer. `nika-engine` gains a dependency on `nika-policy` and re-exports the concrete type for now (commit 6 deletes the duplicate).

### Why nika-policy can compile without nika-engine

`policy.rs` imports `crate::error::NikaError` and `crate::runtime::boot::PolicyConfig`. Neither is viable in an L1 crate. Resolution:

- **`PolicyConfig`**: already re-exports `SecurityPolicyConfig`/`TaintMode` from `nika_core::policy`. Move the `PolicyConfig` struct itself into `nika-core::policy` in this commit (L0, zero-I/O, serde-only). `nika-engine::runtime::boot` keeps a `pub use nika_core::policy::PolicyConfig` re-export for backward compatibility.
- **`NikaError::PolicyViolation`**: define a dedicated `PolicyError` enum inside `nika-policy` that does NOT depend on `NikaError`. Add a `From<nika_policy::PolicyError> for NikaError` impl inside `nika-engine` (one-way conversion, engine-side, no circular dep).

### Files touched

- `tools/nika-policy/Cargo.toml` — NEW
- `tools/nika-policy/src/lib.rs` — NEW
- `tools/nika-policy/src/enforcer.rs` — copy of `runtime/policy.rs` (enforcer + budget)
- `tools/nika-policy/src/ssrf.rs` — extracted SSRF helpers (`is_ssrf_blocked`, `resolve_and_pin_ssrf`, `ssrf_safe_redirect_policy`)
- `tools/nika-core/src/policy.rs` — add `PolicyConfig` struct (moved from `nika-engine/runtime/boot.rs`)
- `tools/nika-engine/Cargo.toml` — add `nika-policy.workspace = true`
- `tools/nika-engine/src/error.rs` — add `From<nika_policy::PolicyError> for NikaError`
- `tools/nika-engine/src/runtime/boot.rs` — replace `pub struct PolicyConfig` with `pub use nika_core::policy::PolicyConfig;`
- `tools/Cargo.toml` — add `"nika-policy"` to `members` + `[workspace.dependencies] nika-policy = { path = "nika-policy", version = "0.79.0" }`

### Cargo.toml for new crate

```toml
# tools/nika-policy/Cargo.toml
[package]
name = "nika-policy"
version.workspace = true
edition.workspace = true
authors.workspace = true
description = "Nika security policy enforcement (SSRF, exec blocklist, token budgets)"
license.workspace = true
repository.workspace = true
rust-version.workspace = true
publish = true

[dependencies]
nika-core = { workspace = true }
nika-kernel = { workspace = true }
thiserror = { workspace = true }
tracing = { workspace = true }
tokio = { workspace = true, features = ["net", "time"] }
parking_lot = { workspace = true }
reqwest = { workspace = true }    # only for ssrf_safe_redirect_policy
url = { workspace = true }

[lints]
workspace = true
```

### Code sketch

```rust
// tools/nika-policy/src/lib.rs
// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Nika security policy enforcement (L1).
//!
//! - `PolicyEnforcer` — concrete stateful enforcer bound to a `PolicyConfig`.
//! - `ssrf` — SSRF blocklist, IPv4/IPv6 range checks, DNS pinning.
//! - impl `nika_kernel::policy::PolicyChecker for PolicyEnforcer`.

mod enforcer;
mod ssrf;

pub use enforcer::{PolicyEnforcer, TokenBudget};
pub use ssrf::{is_ssrf_blocked, resolve_and_pin_ssrf, ssrf_safe_redirect_policy};

/// Policy evaluation error, independent of `NikaError` so this crate stays L1.
#[derive(Debug, thiserror::Error)]
pub enum PolicyError {
    #[error("policy violation: {0}")]
    Violation(String),
    #[error("dns resolution failed for ssrf check: {0}")]
    DnsFailed(String),
}
```

```rust
// tools/nika-policy/src/enforcer.rs — verbatim move from runtime/policy.rs
// but with:
//   - use nika_core::policy::PolicyConfig;   (NEW canonical home)
//   - use crate::ssrf::*                     (instead of inline)
//   - no `use crate::error::NikaError`
//   - impl nika_kernel::policy::PolicyChecker for PolicyEnforcer
impl nika_kernel::policy::PolicyChecker for PolicyEnforcer {
    fn check_exec(&self, command: &str) -> nika_kernel::policy::PolicyDecision {
        match self.check_exec_inner(command) {
            PolicyDecision::Allow => nika_kernel::policy::PolicyDecision::Allow,
            PolicyDecision::Block(r) => nika_kernel::policy::PolicyDecision::Block(r),
            PolicyDecision::RequiresApproval(r) =>
                nika_kernel::policy::PolicyDecision::RequiresApproval(r),
        }
    }
    fn check_fetch(&self, url: &str) -> nika_kernel::policy::PolicyDecision {
        // delegate to existing check_fetch_url() logic
        todo!("call existing private method after rename to avoid clash")
    }
    fn check_token_spend(&self, tokens: u64) -> nika_kernel::policy::PolicyDecision {
        if self.token_budget.can_spend(tokens) {
            nika_kernel::policy::PolicyDecision::Allow
        } else {
            nika_kernel::policy::PolicyDecision::Block("token budget exceeded".into())
        }
    }
    fn is_host_allowed(&self, host: &str) -> bool {
        self.config.allowed_hosts.iter().any(|h| h.eq_ignore_ascii_case(host))
    }
}
```

> Note: the existing `PolicyEnforcer::check_exec(&self, ...)` method name clashes with the trait method. Rename the inherent methods to `check_exec_inner` / `check_fetch_inner` during the move.

```rust
// tools/nika-engine/src/error.rs — APPEND
impl From<nika_policy::PolicyError> for NikaError {
    fn from(e: nika_policy::PolicyError) -> Self {
        match e {
            nika_policy::PolicyError::Violation(m) => NikaError::PolicyViolation { reason: m },
            nika_policy::PolicyError::DnsFailed(m) => NikaError::FetchError { reason: m },
        }
    }
}
```

### TDD

Copy the existing `#[cfg(test)] mod tests` block from `runtime/policy.rs` into `nika-policy/src/enforcer.rs` verbatim. Add one new test verifying the trait impl:

```rust
#[test]
fn policy_checker_trait_impl_routes_through_concrete() {
    use nika_kernel::policy::{PolicyChecker, PolicyDecision as KD};
    let enforcer = PolicyEnforcer::new(PolicyConfig {
        allow_exec: false,
        ..PolicyConfig::default()
    });
    let d: KD = enforcer.check_exec("noop cmd");
    assert!(matches!(d, KD::Block(_)));
}
```

### Verification

```bash
cargo build -p nika-policy
cargo tree -p nika-policy | grep nika-engine && echo "LAYER BROKEN" || echo "layer clean"
cargo test -p nika-policy --lib
cargo test -p nika-engine --lib
cargo test --workspace --lib
cargo clippy --workspace --lib -- -D warnings
```

The `grep nika-engine` line MUST print `layer clean`. If it prints `LAYER BROKEN`, abort and investigate (likely a stray `nika_engine::` use statement you copied by accident).

### Rollback

`git reset --hard HEAD~1`. The new crate is isolated; removing it + the workspace member line is clean. Engine-side `boot.rs` re-export can go back to the inline struct in one step.

---

## Commit 6 — `chore(engine): remove duplicated policy code`

After commit 5, `nika-engine/src/runtime/policy.rs` contains 1263 LOC duplicated in `nika-policy`. Delete the file and replace every `use crate::runtime::policy::*` with `use nika_policy::*`.

### Files touched

- `tools/nika-engine/src/runtime/policy.rs` — DELETED
- `tools/nika-engine/src/runtime/mod.rs` — remove `pub mod policy;`, add `pub use nika_policy as policy;` if any downstream expects that path (audit first)
- Every file that `use`d `crate::runtime::policy::*` — rewrite to `use nika_policy::{PolicyEnforcer, PolicyDecision, TokenBudget, ssrf_safe_redirect_policy, ...};`

### Audit command (run BEFORE the commit)

```bash
rg 'crate::runtime::policy|runtime::policy::' tools/nika-engine/src/ -l
rg 'PolicyEnforcer|TokenBudget|is_ssrf_blocked|resolve_and_pin_ssrf' tools/nika-engine/src/ -l
```

Expected hit count: ~15-25 files. Rewrite each one.

### TDD

No new tests. Existing engine tests (moved to `nika-policy` in commit 5, copy-preserved in-place here) must still pass:

```bash
cargo test -p nika-engine --lib runtime::
cargo test -p nika-policy --lib
```

### Verification

```bash
test ! -f tools/nika-engine/src/runtime/policy.rs
cargo test --workspace --lib
cargo clippy --workspace --lib -- -D warnings
tokei tools/nika-engine/src | head -5
```

Expected: `nika-engine` down ≈ 1263 LOC.

### Rollback

`git revert HEAD` (not reset — the file delete is the interesting part; a revert restores it cleanly). Then also revert commit 5 if you want to keep history monotonic.

---

## Commit 7 — `feat(extract): create nika-extract crate at L2`

Move `runtime/executor/extract.rs` (1327 LOC) into a new L2 crate. This is pure byte→structured-output transformation — no I/O, no async, no locks. Perfect extraction target.

### Why nika-extract can compile without nika-engine

`extract.rs` only imports:
- `nika_core::ast::extract::ExtractMode` — OK, L0
- `crate::error::NikaError` — engine-only, must be removed
- `crate::error_domains::ExecutionError` — engine-only, must be removed (only used in a `#[cfg(not(all(feature = ...)))]` fallback branch)

Resolution:
- Define `ExtractError` enum inside `nika-extract` (thiserror, no deps on engine).
- Add `From<nika_extract::ExtractError> for NikaError` inside `nika-engine::error`.
- All helper functions (`parse_link_header_hreflang`, `strip_non_content_tags`, `extract_text`, `extract_html_by_selector`, `extract_metadata_json`, `extract_links_json`, `extract_jsonpath`, `extract_sitemap_xml`, `apply_extract_with_base`) travel with the crate — they are all private to `extract.rs` today.

### Files touched

- `tools/nika-extract/Cargo.toml` — NEW
- `tools/nika-extract/src/lib.rs` — NEW, contains everything from `extract.rs` minus the two engine imports
- `tools/nika-extract/src/error.rs` — NEW, defines `ExtractError`
- `tools/nika-engine/Cargo.toml` — add `nika-extract.workspace = true`
- `tools/nika-engine/src/error.rs` — add `From<nika_extract::ExtractError> for NikaError`
- `tools/Cargo.toml` — add `"nika-extract"` member + workspace dep

### Cargo.toml for new crate

```toml
# tools/nika-extract/Cargo.toml
[package]
name = "nika-extract"
version.workspace = true
edition.workspace = true
authors.workspace = true
description = "Nika fetch: post-processing — HTML/markdown/article/feed/metadata/jsonpath extraction"
license.workspace = true
repository.workspace = true
rust-version.workspace = true
publish = true

[features]
default = ["fetch-markdown", "fetch-html", "fetch-article", "fetch-feed"]
fetch-markdown = ["htmd"]
fetch-html = ["scraper"]
fetch-article = ["readability"]
fetch-feed = ["feed-rs"]

[dependencies]
nika-core = { workspace = true }
thiserror = { workspace = true }
serde_json = { workspace = true }
url = { workspace = true }
jsonpath_lib = { workspace = true }

htmd = { workspace = true, optional = true }
scraper = { workspace = true, optional = true }
readability = { workspace = true, optional = true }
feed-rs = { workspace = true, optional = true }

[lints]
workspace = true
```

Feature flags mirror today's `nika-engine` feature flags for fetch modes. Engine's `Cargo.toml` will forward its existing `fetch-*` features to `nika-extract`.

### Code sketch

```rust
// tools/nika-extract/src/error.rs
// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

#[derive(Debug, thiserror::Error)]
pub enum ExtractError {
    #[error("extract error: {0}")]
    Failed(String),
    #[error("mode '{0}' not compiled (feature flag disabled)")]
    FeatureDisabled(&'static str),
    #[error("selector required for mode '{0}'")]
    SelectorRequired(&'static str),
    #[error("jsonpath error: {0}")]
    JsonPath(String),
}
```

```rust
// tools/nika-extract/src/lib.rs
// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Nika fetch: extraction pipeline (L2, pure, zero I/O).
//!
//! 9 extract modes: markdown, article, text, selector, metadata, links,
//! jsonpath, feed, llm_txt. Moved verbatim from `nika-engine/runtime/executor/extract.rs`
//! in Session 12 (Constellation refactor).

pub mod error;
pub use error::ExtractError;

use nika_core::ast::extract::ExtractMode;

/// Apply an extraction mode to an HTTP response body.
///
/// - `body`: the response body as UTF-8 text (binary handled by CAS path, not here)
/// - `extract`: the mode selected on the fetch: verb
/// - `selector`: CSS selector or JSONPath expression (mode-dependent)
/// - `base_url`: for resolving relative links in extracted HTML
pub fn extract(
    body: &str,
    extract: Option<ExtractMode>,
    selector: Option<&str>,
    base_url: Option<&str>,
) -> Result<String, ExtractError> {
    // verbatim body of apply_extract_with_base, with error types rewritten
    let mode = match extract {
        None => return Ok(body.to_string()),
        Some(m) => m,
    };
    match mode {
        #[cfg(feature = "fetch-markdown")]
        ExtractMode::Markdown => {
            let clean = strip_non_content_tags(body);
            htmd::convert(&clean).map_err(|e| ExtractError::Failed(format!("HTML to markdown: {e}")))
        }
        // ... rest of the arms, each mapping into ExtractError variants
        _ => Err(ExtractError::FeatureDisabled("unknown")),
    }
}

// All helpers below are pub(crate) or private:
pub(crate) fn parse_link_header_hreflang(values: &[String]) -> Vec<serde_json::Value> { /* verbatim */ }
fn split_link_entries(value: &str) -> Vec<&str> { /* verbatim */ }
fn strip_non_content_tags(html: &str) -> String { /* verbatim */ }
fn extract_text(html: &str, selector: Option<&str>) -> Result<String, ExtractError> { /* verbatim */ }
fn extract_html_by_selector(html: &str, css: &str) -> Result<String, ExtractError> { /* verbatim */ }
fn extract_metadata_json(html: &str, base_url: Option<&str>) -> Result<String, ExtractError> { /* verbatim */ }
fn extract_links_json(html: &str, base_url: Option<&str>) -> Result<String, ExtractError> { /* verbatim */ }
fn extract_jsonpath(body: &str, path: &str) -> Result<String, ExtractError> { /* verbatim */ }
fn extract_sitemap_xml(body: &str) -> Result<String, ExtractError> { /* verbatim */ }
```

Every `Result<_, NikaError>` becomes `Result<_, ExtractError>`. Every `NikaError::ExtractError { reason }` construction becomes `ExtractError::Failed(reason)`. Every call site inside the file that produces errors is mechanical `s/NikaError::ExtractError { reason: x }/ExtractError::Failed(x)/`.

```rust
// tools/nika-engine/src/error.rs — APPEND
impl From<nika_extract::ExtractError> for NikaError {
    fn from(e: nika_extract::ExtractError) -> Self {
        NikaError::ExtractError { reason: e.to_string() }
    }
}
```

Engine still has its own `apply_extract_with_base` function wrapping the new crate during commit 7:

```rust
// tools/nika-engine/src/runtime/executor/extract.rs — SHRINK to a thin wrapper
// (this file is DELETED in commit 8; for commit 7 we keep a thin wrapper)
pub(crate) fn apply_extract_with_base(
    body: &str,
    extract: Option<ExtractMode>,
    selector: Option<&str>,
    base_url: Option<&str>,
) -> Result<String, NikaError> {
    nika_extract::extract(body, extract, selector, base_url).map_err(Into::into)
}

#[allow(unused_imports)]
pub(crate) use nika_extract::parse_link_header_hreflang;
```

### TDD

Copy the existing `#[cfg(test)] mod tests` from the old `extract.rs` into `nika-extract/src/lib.rs` (or a sibling `tests.rs`). The tests are pure — no fixtures, no I/O — so they port cleanly. Add one smoke test:

```rust
#[test]
fn error_converts_to_display() {
    let e = ExtractError::Failed("boom".into());
    assert!(e.to_string().contains("boom"));
}
```

### Verification

```bash
cargo build -p nika-extract
cargo tree -p nika-extract | grep nika-engine && echo "LAYER BROKEN" || echo "layer clean"
cargo test -p nika-extract --lib
cargo test -p nika-engine --lib runtime::executor::extract
cargo test --workspace --lib
cargo clippy --workspace --lib -- -D warnings
```

### Rollback

`git reset --hard HEAD~1`. Because commit 7 keeps a thin wrapper in engine, nothing downstream changes API-wise.

---

## Commit 8 — `chore(engine): delete runtime/executor/extract.rs`

Now that commit 7 put a wrapper in place, delete the wrapper and route callers directly to `nika_extract::extract()`. Engine shrinks by the remaining ≈ 1300 LOC of the original file (now ~20 LOC of wrapper).

### Files touched

- `tools/nika-engine/src/runtime/executor/extract.rs` — DELETED
- `tools/nika-engine/src/runtime/executor/mod.rs` — remove `mod extract;`
- `tools/nika-engine/src/runtime/executor/fetch.rs` (wherever the fetch verb calls `apply_extract_with_base`) — replace call with `nika_extract::extract(body, extract, selector, base_url)?`
- Any test importing `crate::runtime::executor::extract::*` — rewrite to `nika_extract::*`

### Audit command

```bash
rg 'runtime::executor::extract|apply_extract_with_base|parse_link_header_hreflang' tools/nika-engine/src/ -l
```

Rewrite each hit.

### TDD

No new tests — just verify that engine fetch tests still pass:

```bash
cargo test -p nika-engine --lib runtime::executor::fetch
cargo test -p nika-engine --lib --features "fetch-markdown fetch-html fetch-article fetch-feed"
```

### Verification

```bash
test ! -f tools/nika-engine/src/runtime/executor/extract.rs
cargo test --workspace --lib
cargo clippy --workspace --lib -- -D warnings
tokei tools/nika-engine/src | head -5
```

Expected: `nika-engine` total ≈ 146.4k LOC (was 149k pre-session, minus 1263 from commit 6, minus 1327 here — the thin wrapper from commit 7 counts for ≈ 20 LOC).

### Rollback

`git revert HEAD`. The deleted file is recoverable.

---

## Commit 9 — `feat(kernel): add ExecCaps/FetchCaps/InferCaps/InvokeCaps/AgentCaps typed context structs`

Define the five per-verb borrowed-slice capability structs that Session 13 will thread through free `pub async fn run()` verb functions. NOT wired yet. This commit is pure type definitions + rustdoc + compile-time tests asserting the structs are `Send + Sync`.

### Files touched

- `tools/nika-kernel/src/caps.rs` — NEW (~220 LOC)
- `tools/nika-kernel/src/lib.rs` — add `pub mod caps;`

### Code sketch

```rust
// tools/nika-kernel/src/caps.rs
// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Per-verb capability slices.
//!
//! In Session 13, `nika-runtime` will build a run-scoped `VerbCapabilities`
//! bundle once per workflow invocation. For each task, it borrows slices
//! of that bundle into one of the five `*Caps<'a>` structs below and passes
//! it to the relevant `pub async fn run()` in the verb crate.
//!
//! These structs are intentionally NOT wired in Session 12 — they exist
//! so Session 13 is mechanical: every field here already has a home.

use std::sync::Arc;

use crate::clock::Clock;
use crate::filesystem::{FsRead, FsWrite};
use crate::http::HttpClient;
use crate::policy::PolicyChecker;
use crate::provider::Provider;
use crate::shell::ShellExecutor;
use crate::store::BlobStore;

/// Capabilities available to an `exec:` task.
pub struct ExecCaps<'a> {
    pub shell: &'a dyn ShellExecutor,
    pub policy: &'a dyn PolicyChecker,
    pub clock: &'a dyn Clock,
    pub fs_read: &'a dyn FsRead,
}

/// Capabilities available to a `fetch:` task.
pub struct FetchCaps<'a> {
    pub http: &'a dyn HttpClient,
    pub policy: &'a dyn PolicyChecker,
    pub blobs: &'a dyn BlobStore,
    pub clock: &'a dyn Clock,
}

/// Capabilities available to an `infer:` task.
pub struct InferCaps<'a> {
    pub provider: Arc<dyn Provider>,
    pub fs_read: &'a dyn FsRead,
    pub policy: &'a dyn PolicyChecker,
    pub clock: &'a dyn Clock,
}

/// Capabilities available to an `invoke:` task (MCP or builtin).
pub struct InvokeCaps<'a> {
    pub fs_read: &'a dyn FsRead,
    pub fs_write: &'a dyn FsWrite,
    pub http: &'a dyn HttpClient,
    pub blobs: &'a dyn BlobStore,
    pub policy: &'a dyn PolicyChecker,
    pub clock: &'a dyn Clock,
}

/// Capabilities available to an `agent:` task (multi-turn loop).
pub struct AgentCaps<'a> {
    pub provider: Arc<dyn Provider>,
    pub invoke: InvokeCaps<'a>,
    pub policy: &'a dyn PolicyChecker,
    pub clock: &'a dyn Clock,
}
```

`BlobStore` and `Provider` live in the existing `nika-kernel::{store, provider}` modules per `lib.rs`. Import and wire them in. If the auto-`Send + Sync` inference fails on any trait object, add `Send + Sync` supertraits to the corresponding trait in the source file and re-run.

### TDD

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}

    #[test]
    fn exec_caps_is_send_sync() {
        assert_send::<&ExecCaps<'_>>();
        assert_sync::<&ExecCaps<'_>>();
    }

    #[test]
    fn fetch_caps_is_send_sync() {
        assert_send::<&FetchCaps<'_>>();
        assert_sync::<&FetchCaps<'_>>();
    }

    #[test]
    fn infer_caps_is_send_sync() {
        assert_send::<&InferCaps<'_>>();
        assert_sync::<&InferCaps<'_>>();
    }

    #[test]
    fn invoke_caps_is_send_sync() {
        assert_send::<&InvokeCaps<'_>>();
        assert_sync::<&InvokeCaps<'_>>();
    }

    #[test]
    fn agent_caps_is_send_sync() {
        assert_send::<&AgentCaps<'_>>();
        assert_sync::<&AgentCaps<'_>>();
    }
}
```

If a field's trait object is not auto-`Send + Sync`, the test fails at compile time — immediately flagging which trait needs a `: Send + Sync` super-bound.

### Verification

```bash
cargo test -p nika-kernel --lib caps::
cargo clippy -p nika-kernel --lib -- -D warnings
cargo test --workspace --lib
```

### Rollback

`git reset --hard HEAD~1`. Types are not wired anywhere; no downstream depends on them.

---

## Commit 10 — `docs(constellation): ARCHITECTURE.md + session12 memory update`

Document what landed. No code.

### Files touched

- `tools/nika/ARCHITECTURE.md` — update crate count (28 → 30), update LOC (149k → ~146.4k), add `nika-policy` (L1) and `nika-extract` (L2) rows, add "Session 12 Foundation complete" bullet
- `docs/plans/constellation-session12-rework/06-session12-foundation.md` — this file, mark status at top
- `~/.claude/projects/-Users-thibaut-dev-supernovae-nika/memory/project_constellation_session12.md` — update Quick State + Status blocks (Nika co-author private memory file, not checked in)
- `CHANGELOG.md` — one line under `## [Unreleased]`: `Constellation S12 foundation: +2 crates (nika-policy, nika-extract), engine -2,590 LOC, kernel surface extended (PolicyChecker, HttpClient::send_streaming, cancellation, FsRead/FsWrite split, 5 *Caps structs).`

### TDD

N/A (docs only).

### Verification

```bash
cargo test --workspace --lib   # still green
git diff --stat HEAD~1 HEAD
grep -q 'nika-policy' tools/nika/ARCHITECTURE.md
grep -q 'nika-extract' tools/nika/ARCHITECTURE.md
```

### Rollback

`git reset --hard HEAD~1`. Docs only, zero risk.

---

## What this session explicitly does NOT do

- No verb extraction. The five verbs (`exec`, `fetch`, `infer`, `invoke`, `agent`) remain inside `nika-engine/runtime/executor/`. Session 13 creates `nika-verb-exec`, `nika-verb-fetch`, etc.
- No `TaskExecutor` deletion. The 22-field god struct stays in `runtime/executor/mod.rs`. Sessions 13-14 delete it.
- No `nika-runtime` crate. That crate is the Session 13 deliverable. The L3 layer is empty of new crates in S12.
- No wiring of `ExecCaps` / `FetchCaps` / etc. The structs are defined but not borrowed by anyone. Session 13 wires them.
- No `VerbCapabilities` bundle. The run-scoped bundle is Session 13.
- No `enum TaskAction` + dispatch `match`. Session 13.
- No `trait Verb`. Ever. Decided — the design uses free functions per crate, dispatched via `match`, not a trait object.
- No schema bump. `nika/workflow@0.12` stays.
- No `ReqwestClient::send_streaming` implementation. Only the trait surface + default error. Session 13 writes the reqwest stream path.
- No cancellation wiring in `nika-engine` executor loop. Only the `ShellCommand::cancel` field + `TokioShell` honors it. Runtime-level broadcast is Session 13.
- No performance work. No benchmarks, no tuning, no SIMD. Correctness + layering only.

Sessions 13-14 will: create `nika-runtime`, delete `TaskExecutor`, extract the five verb crates (`nika-verb-exec/fetch/infer/invoke/agent`), wire `VerbCapabilities` + `ExecCaps`/etc., and hook the new `enum TaskAction` dispatcher.

---

## Done when all of these are green

- [ ] Commit 1 on `main`: `PolicyChecker` trait compiles + 4 unit tests pass
- [ ] Commit 2 on `main`: `HttpClient::send_streaming` default errors `Unsupported`, `HttpError::TooLarge` exists, `nika-http` still compiles without override
- [ ] Commit 3 on `main`: `ShellCommand::cancel` field present, `TokioShell` honors pre-cancel + mid-flight cancel, 2 tests pass
- [ ] Commit 4 on `main`: `FsRead` + `FsWrite` exist, `Filesystem` alias still works, every existing `Filesystem` impl compiles, narrowing test passes
- [ ] Commit 5 on `main`: `nika-policy` crate exists, `cargo tree -p nika-policy | grep nika-engine` is empty, `impl PolicyChecker for PolicyEnforcer` present, `From<PolicyError> for NikaError` lives engine-side only
- [ ] Commit 6 on `main`: `tools/nika-engine/src/runtime/policy.rs` deleted, all call sites rewritten to `nika_policy::*`, `nika-engine` LOC down ≈ 1263
- [ ] Commit 7 on `main`: `nika-extract` crate exists, `cargo tree -p nika-extract | grep nika-engine` is empty, `ExtractError` defined, engine wrapper keeps fetch verb green
- [ ] Commit 8 on `main`: `tools/nika-engine/src/runtime/executor/extract.rs` deleted, engine LOC down another ~1300
- [ ] Commit 9 on `main`: `ExecCaps`, `FetchCaps`, `InferCaps`, `InvokeCaps`, `AgentCaps` exist in `nika-kernel::caps`, 5 `Send + Sync` compile-time tests pass
- [ ] Commit 10 on `main`: `ARCHITECTURE.md` shows 30 crates, ~146.4k engine LOC, `CHANGELOG.md` has the S12 line
- [ ] After every commit: `cargo test --workspace --lib` GREEN
- [ ] After every commit: `cargo clippy --workspace --lib --all-targets -- -D warnings` CLEAN
- [ ] Zero `unwrap`/`expect` added in hot paths (grep new .rs files)
- [ ] Every new `.rs` has the AGPL header
- [ ] Every commit message ends with `Co-Authored-By: Nika 🦋 <nika@supernovae.studio>` and NO Claude trailer
- [ ] `nika-engine` total LOC: 149k → ≈ 146.4k (measured via `tokei tools/nika-engine/src`)
- [ ] Workspace crate count: 28 → 30 (add `nika-policy`, `nika-extract`)
- [ ] `cargo tree -p nika-policy | grep nika-engine` → empty
- [ ] `cargo tree -p nika-extract | grep nika-engine` → empty
- [ ] `cargo build --workspace --all-features` succeeds
- [ ] `nika run tests/e2e/golden-verbs.nika.yaml` golden regression (from S12.13.0 baseline) matches exactly
- [ ] Final `git log --oneline c5ea27438..HEAD` shows 10 commits, all with `Co-Authored-By: Nika 🦋`

When every box is ticked, Session 12 Foundation is complete and Session 13 (verb extraction + `nika-runtime` creation) can begin. Session 13 will be mechanical: each of the five `*Caps` structs has a home, the policy + extraction crates already live at their target layers, and the only remaining engine debt is the `TaskExecutor` god struct itself.
