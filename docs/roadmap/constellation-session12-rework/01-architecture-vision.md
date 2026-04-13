# Architecture Vision — End State After Constellation V2.3

> The target architecture for Nika after Sessions 12/13/14.

## One-paragraph summary

`nika-engine` dissolves from a 148k-LOC monolith with a 22-field `TaskExecutor` god struct into a 7-layer diamond where each side-effect trait in `nika-kernel` has exactly one production implementation in an L1 crate, and each of the 5 verbs is a free `pub async fn run()` in its own L2 crate receiving a borrowed typed capability bundle. Dispatch is a 12-line compile-time-exhaustive `match` in a new `nika-runtime` L3 crate. The test boundary is per-verb: each verb crate compiles in ~1 second with only `nika-kernel` + `nika-core` in its dep graph, and is fully unit-testable with `nika-kernel-mock` traits. No trait `Verb`, no `Box<dyn Verb>`, no monolithic `VerbCtx`, no `TaskExecutor`.

---

## The diamond (post-S14)

```
                         nika (L5 binary)
                               |
           cli   tui   serve   lsp   init        [L4]
             \  /       |      /    /
              nika-runtime (L3)   <-- VerbCapabilities + dispatch() + Runner
                   |                   + nika-engine shim (provider::rig, boot)
                   |
   +-------+-------+-------+-------+-------+-------+-------+
   |       |       |       |       |       |       |       |
 v-exec v-inv  v-fetch  v-infer v-agent extract  builtin  ... [L2]
   \       \       |       |       |       |       |    /
                   nika-kernel (L0.5)
                         |
     +------+------+-----+-----+------+------+------+------+
     |      |      |           |      |      |      |     |
  clock    fs    blob      http+str  exec-run event policy shield [L1]
     \      \     \           |      /       /      /     /
                         nika-core (L0)
```

## The 10 nika-kernel traits (post-S14)

| Trait | File | Production impl | Consumers |
|---|---|---|---|
| `ShellExecutor` | `shell.rs` | `nika-exec-runner::TokioShell` | `nika-verb-exec` |
| `HttpClient` + `send_streaming` | `http.rs` | `nika-http::ReqwestClient` | `nika-verb-fetch` (+ provider, registry, webhook) |
| `Provider` (enriched) | `provider.rs` | `nika-engine::provider::rig::RigProvider` via bridge | `nika-verb-infer`, `nika-verb-agent` |
| `BlobStore` | `store.rs` | `nika-blob::DiskBlobStore` | `nika-verb-fetch`, `nika-verb-infer` (vision) |
| `Clock` | `clock.rs` | `nika-clock::SystemClock` | all verbs (cancellation deadlines) |
| `FsRead` + `FsWrite` splinters | `filesystem.rs` | `nika-fs::TokioFs` | `nika-builtin` (file tools) |
| `PolicyChecker` (NEW in S12) | `policy.rs` | `nika-policy::PolicyEnforcer` | `nika-verb-exec`, `nika-verb-fetch`, `nika-verb-agent` |
| `BuiltinTool` (sealed) | `builtin.rs` | 63 impls in `nika-builtin` + engine residuals | `nika-verb-invoke` |
| `EventEmitter` (blanket) | `events.rs` | `nika-event::EventLog` | all |
| `TaskScope` splinters | `scope.rs` | `nika-runtime::RunContext` | all verbs |

## The VerbCapabilities bundle (lives in nika-runtime)

```rust
// tools/nika-runtime/src/caps.rs

use std::sync::Arc;
use nika_kernel::{
    shell::ShellExecutor,
    http::HttpClient,
    provider::Provider,
    store::BlobStore,
    clock::Clock,
    policy::PolicyChecker,
    filesystem::{FsRead, FsWrite},
};
use nika_shield::ShieldContext;
use nika_event::EventLog;

/// Run-scoped side-effect bundle. Constructed ONCE per workflow run by
/// `nika-cli` / `nika-tui` via the Runner. Cheap to clone (each field is Arc).
#[derive(Clone)]
pub struct VerbCapabilities {
    // Core I/O
    pub shell: Arc<dyn ShellExecutor>,
    pub http: Arc<dyn HttpClient>,
    pub provider_registry: Arc<dyn ProviderRegistry>,  // caches Arc<dyn Provider> per name
    pub blob_store: Arc<dyn BlobStore>,
    pub clock: Arc<dyn Clock>,
    pub fs_read: Arc<dyn FsRead>,
    pub fs_write: Arc<dyn FsWrite>,

    // Security + observability
    pub policy: Arc<dyn PolicyChecker>,
    pub shield: ShieldContext,  // cheap Clone, contains Arc<SpotlightFence> + Arc<CanarySystem>
    pub events: EventLog,       // cheap Clone

    // MCP + builtins
    pub mcp_pool: McpClientPool,
    pub builtin_router: Arc<BuiltinToolRouter>,

    // Fetch-specific auxiliaries (bundled to keep the god-bag sane)
    pub fetch_aux: Arc<FetchAux>,

    // Run-scoped context
    pub cancel_token: tokio_util::sync::CancellationToken,
    pub workflow_base_dir: std::path::PathBuf,
    pub skill_injector: Arc<SkillInjector>,
    pub skills_map: Arc<std::collections::HashMap<String, String>>,
    pub resolved_agents: Arc<ResolvedAgents>,
}

impl VerbCapabilities {
    /// Build an ExecCaps slice — exactly what the exec verb crate needs.
    pub fn exec_caps(&self) -> ExecCaps<'_> { /* borrows the right fields */ }
    pub fn fetch_caps(&self) -> FetchCaps<'_> { /* ... */ }
    pub fn infer_caps(&self) -> InferCaps<'_> { /* ... */ }
    pub fn invoke_caps(&self) -> InvokeCaps<'_> { /* ... */ }
    pub fn agent_caps(&self) -> AgentCaps<'_> { /* ... */ }
}
```

## The per-verb typed contexts (defined in nika-kernel, consumed by verb crates)

```rust
// tools/nika-kernel/src/verb_caps.rs (or split across files)

pub struct ExecCaps<'a> {
    pub shell: &'a dyn ShellExecutor,
    pub policy: &'a dyn PolicyChecker,
    pub events: &'a EventLog,
    pub clock: &'a dyn Clock,
    pub shield: &'a ShieldContext,
    pub cancel: &'a CancellationToken,
    pub workflow_base_dir: &'a Path,
    pub default_cwd: Option<&'a Path>,
}

pub struct FetchCaps<'a> {
    pub http: &'a dyn HttpClient,
    pub blob_store: &'a dyn BlobStore,
    pub policy: &'a dyn PolicyChecker,
    pub events: &'a EventLog,
    pub clock: &'a dyn Clock,
    pub shield: &'a ShieldContext,
    pub fetch_aux: &'a FetchAux,  // cookies/cache/rate_limit/robots
    pub cancel: &'a CancellationToken,
}

pub struct InferCaps<'a> {
    pub provider_registry: &'a dyn ProviderRegistry,
    pub blob_store: &'a dyn BlobStore,   // for vision content blocks
    pub shield: &'a ShieldContext,
    pub skill_injector: &'a SkillInjector,
    pub skills_map: &'a HashMap<String, String>,
    pub skills_base_dir: &'a Path,
    pub policy: &'a dyn PolicyChecker,
    pub events: &'a EventLog,
    pub clock: &'a dyn Clock,
    pub cancel: &'a CancellationToken,
    pub workflow_base_dir: &'a Path,
}

pub struct InvokeCaps<'a> {
    pub mcp_pool: &'a McpClientPool,
    pub builtin_router: &'a BuiltinToolRouter,
    pub policy: &'a dyn PolicyChecker,
    pub events: &'a EventLog,
    pub clock: &'a dyn Clock,
    pub shield: &'a ShieldContext,
    pub cancel: &'a CancellationToken,
}

pub struct AgentCaps<'a> {
    pub provider_registry: &'a dyn ProviderRegistry,
    pub mcp_pool: &'a McpClientPool,
    pub builtin_router: &'a BuiltinToolRouter,
    pub policy: &'a dyn PolicyChecker,
    pub shield: &'a ShieldContext,
    pub skill_injector: &'a SkillInjector,
    pub skills_map: &'a HashMap<String, String>,
    pub resolved_agents: &'a ResolvedAgents,
    pub blob_store: &'a dyn BlobStore,
    pub events: &'a EventLog,
    pub clock: &'a dyn Clock,
    pub cancel: &'a CancellationToken,
    pub workflow_base_dir: &'a Path,
}
```

**Key property:** it is a COMPILE ERROR to invoke the exec verb without an `ExecCaps` containing a `ShellExecutor`. The Rust type system enforces least-privilege capability injection. This is the capability-oriented pattern used by Restate SDK and axum::extract.

## The dispatch function (lives in nika-runtime)

```rust
// tools/nika-runtime/src/dispatch.rs

use nika_core::ast::{AnalyzedTask, TaskAction};
use crate::capabilities::VerbCapabilities;
use crate::error::RunError;

pub async fn dispatch(
    task: &AnalyzedTask,
    bindings: &ResolvedBindings,
    rc: &RunContext,
    vc: &VerbCapabilities,
) -> Result<VerbOutput, RunError> {
    let task_id = &task.id;
    match &task.action {
        TaskAction::Exec(p) => {
            nika_verb_exec::run(task_id, p, bindings, rc, vc.exec_caps())
                .await
                .map(VerbOutput::Exec)
                .map_err(Into::into)
        }
        TaskAction::Fetch(p) => {
            nika_verb_fetch::run(task_id, p, bindings, rc, vc.fetch_caps())
                .await
                .map(VerbOutput::Fetch)
                .map_err(Into::into)
        }
        TaskAction::Infer(p) => {
            nika_verb_infer::run(task_id, p, bindings, rc, vc.infer_caps())
                .await
                .map(VerbOutput::Infer)
                .map_err(Into::into)
        }
        TaskAction::Invoke(p) => {
            nika_verb_invoke::run(task_id, p, bindings, rc, vc.invoke_caps())
                .await
                .map(VerbOutput::Invoke)
                .map_err(Into::into)
        }
        TaskAction::Agent(p) => {
            nika_verb_agent::run(task_id, p, bindings, rc, vc.agent_caps())
                .await
                .map(VerbOutput::Agent)
                .map_err(Into::into)
        }
    }
}
```

**12 lines. Compile-time exhaustive** (adding a 6th `TaskAction` variant is a compile error everywhere). **Zero dyn dispatch for verbs.** Zero heap allocation beyond what the verbs themselves do internally.

## A verb crate's Cargo.toml (nika-verb-exec example)

```toml
[package]
name = "nika-verb-exec"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
# L0
nika-core.workspace = true       # AST: ExecParams, ResolvedBindings, RunContext
# L0.5
nika-kernel.workspace = true     # ShellExecutor, PolicyChecker, Clock, ExecCaps
# L1
nika-event.workspace = true      # EventLog + EventKind
# std deps
tokio = { workspace = true, features = ["sync", "macros"] }
tokio-util.workspace = true      # CancellationToken
tracing.workspace = true
thiserror.workspace = true
serde.workspace = true
regex.workspace = true           # BINDING_RE for shell injection detection
shlex.workspace = true           # shell-free parsing
# NOT in this Cargo.toml:
# - tokio/process (moved to nika-exec-runner via ShellExecutor trait)
# - reqwest (fetch concern, not this verb's)
# - rig-core (infer concern)
# - nika-engine (circular — would defeat the whole refactor)

[dev-dependencies]
nika-kernel-mock.workspace = true  # MockShellExecutor, MockPolicyChecker, MockClock
tokio = { workspace = true, features = ["macros", "rt-multi-thread", "test-util"] }
```

**Compile test:** `cargo tree -p nika-verb-exec | grep nika-engine` must be EMPTY. Run as CI gate.

## A verb body (nika-verb-exec shape)

```rust
// tools/nika-verb-exec/src/lib.rs

#![deny(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use nika_core::ast::ExecParams;
use nika_core::binding::{template_resolve, ResolvedBindings};
use nika_core::store::RunContext;
use nika_kernel::{ExecCaps, shell::{ShellCommand, ShellError}, policy::PolicyDecision};
use nika_event::EventKind;

mod sec;  // SEC-2 / SEC-2b / dollar-paren detection — all pure functions
mod error;
pub use error::ExecError;

pub struct ExecOutput {
    pub stdout: String,
    pub exit_code: i32,
    pub duration_ms: u64,
}

/// Shell verb entry point. Pure function — takes parameters and
/// capabilities, returns output. No shared state, no `self`.
///
/// All side effects flow through `caps`. Testable with mock capabilities.
pub async fn run(
    task_id: &Arc<str>,
    params: &ExecParams,
    bindings: &ResolvedBindings,
    rc: &RunContext,
    caps: ExecCaps<'_>,
) -> Result<ExecOutput, ExecError> {
    // 1. Template resolve the command string
    let resolved_cmd = template_resolve(&params.command, bindings, rc)?;
    let is_shell = params.shell == Some(true);

    // 2. Pure security validation (SEC-2, dollar-paren detection, env checks)
    sec::validate(&params.command, &resolved_cmd, is_shell, bindings, rc)?;

    // 3. Policy check (trait call)
    if let PolicyDecision::Block { reason } = caps.policy.check_exec(&resolved_cmd) {
        caps.events.emit(EventKind::PolicyBlocked { /* ... */ });
        return Err(ExecError::Blocked(reason));
    }

    // 4. cwd resolution with boundary check
    let cwd = sec::resolve_cwd(params, bindings, rc, caps.workflow_base_dir, caps.default_cwd)?;

    // 5. Build ShellCommand DTO (no side effects yet)
    let cmd = ShellCommand {
        program: /* ... */,
        args: /* ... */,
        env: sec::resolve_env(params, bindings, rc)?,
        cwd,
        timeout: params.timeout.map(std::time::Duration::from_secs),
        stdin: None,
        shell: is_shell,
        cancel: Some(caps.cancel.clone()),  // S12 addition to the trait
    };

    // 6. Run via trait (the only actual side effect)
    let start = caps.clock.now();
    let result = caps.shell.run(cmd).await?;
    let duration_ms = (caps.clock.now() - start).as_millis() as u64;

    // 7. Emit completion event
    caps.events.emit(EventKind::ExecCompleted { /* ... */ });

    // 8. Return typed output (dispatcher converts to NikaError at the boundary)
    if !result.success() {
        return Err(ExecError::Shell(ShellError::Other { reason: result.stderr }));
    }
    Ok(ExecOutput {
        stdout: sec::truncate_output(result.stdout, params.max_stdout),
        exit_code: result.status,
        duration_ms,
    })
}
```

**Compare to the current 471-line `executor/exec.rs` god method.** This is ~80 lines of verb logic + a `sec` submodule. The raw `tokio::process::Command` usage vanishes. The only async primitive is through `caps.shell.run()` which is a trait call. Mockable, testable, exhaustively typed.

## Test boundary (nika-verb-exec shape)

```rust
// tools/nika-verb-exec/src/lib.rs — tests module

#[cfg(test)]
mod tests {
    use super::*;
    use nika_kernel_mock::{MockShellExecutor, MockPolicyChecker, MockClock};
    use nika_shield::ShieldContext;

    #[tokio::test]
    async fn happy_path() {
        let shell = MockShellExecutor::new().with_output("hello\n", 0);
        let policy = MockPolicyChecker::allow_all();
        let clock = MockClock::fixed(std::time::SystemTime::UNIX_EPOCH);
        let shield = ShieldContext::permissive();
        let events = EventLog::new_in_memory();
        let cancel = CancellationToken::new();

        let caps = ExecCaps {
            shell: &shell,
            policy: &policy,
            events: &events,
            clock: &clock,
            shield: &shield,
            cancel: &cancel,
            workflow_base_dir: std::path::Path::new("/workspace"),
            default_cwd: None,
        };

        let params = ExecParams { command: "echo hello".into(), shell: Some(false), ..Default::default() };
        let task_id = Arc::from("test-1");
        let bindings = ResolvedBindings::empty();
        let rc = RunContext::test_default();

        let out = run(&task_id, &params, &bindings, &rc, caps).await.unwrap();
        assert_eq!(out.stdout, "hello\n");
        assert_eq!(out.exit_code, 0);

        // Event sequence is the test oracle
        let evs = events.snapshot();
        assert_eq!(evs.len(), 1);  // just ExecCompleted (no PolicyBlocked)
        assert!(matches!(evs[0], EventKind::ExecCompleted { .. }));
    }
}
```

**Compile time:** `cargo test -p nika-verb-exec --lib` should complete in under 2 seconds (vs the current full `nika-engine` test compile at ~60+ seconds). **This is the iteration-speed win that makes the refactor worth it beyond LOC.**

## What `nika-engine` becomes

After S14, `nika-engine` is a **thin shim** (target <30k LOC from 148k):

```
nika-engine/
├── Cargo.toml        # marked as dissolution target, Phase 15 deletion
├── src/
│   ├── lib.rs        # pub use re-exports
│   ├── provider/
│   │   └── rig/      # concrete RigProvider + 6 submodules (vision, tool-use, thinking)
│   │                 # consumed by nika-verb-infer via impl Provider bridge
│   │                 # Target: split to nika-provider-rig L1 crate in Phase 15
│   ├── runtime/
│   │   ├── boot.rs            # BootSequence, BootPhase (used by nika-cli, nika-tui)
│   │   ├── structured_output.rs  # StructuredOutputEngine (consumed by nika-verb-infer)
│   │   └── chat_workflow.rs   # nika-cli chat mode
│   └── error.rs      # residual NikaError aggregation
```

**Phase 15 goal:** split `nika-provider-rig` into its own crate, move `boot.rs` to nika-runtime, delete `nika-engine` entirely. Target reached when `ls tools/nika-engine` returns "No such file or directory".

---

## Why this is the cleanest architecture

### It respects Rust idioms

- **Closed sum → enum.** No `Box<dyn Verb>`. Fixed 5 verbs × `match` = zero-cost dispatch.
- **Capabilities → typed borrows.** No `Option<Arc<dyn>>` god-context. Compile-time enforcement.
- **Side effects → traits.** Every I/O behind a mockable contract. Test boundary per verb.
- **No god objects.** Zero `self.` anywhere in verb logic. Functions take parameters, return values.

### It respects the diamond layering invariant

Every new crate depends ONLY downward. `cargo tree -p nika-verb-* | grep nika-engine` is empty (TEMP exceptions documented). L0 (nika-core) has ZERO reverse deps. L0.5 (nika-kernel) has ZERO impls. L1 crates have ONE responsibility each. L2 verb crates are leaves in the effect graph.

### It closes the Constellation V2.3 gap

- Engine LOC: 148k → 138k post-S14 → ≤100k post-Phase 15 (target met)
- Zero-unwrap ratchet: verb crates use `#![deny(clippy::unwrap_used)]`
- nika-macros target: preserved (no new macros introduced)
- Salsa avoidance: preserved (no incremental framework)

### It matches what battle-tested Rust projects do

- **Ruff:** enum `Rule` + free functions + `&mut Checker<'a>` god-context-by-borrow
- **uv:** free functions + enum `Commands` + explicit params per subcommand
- **Restate SDK (Rust):** `Context<'ctx>` trait-sliced borrowed bundle
- **rustc_codegen_*:** pub fns exported from crates, not trait objects — closed set
- **axum::extract:** typed per-handler state extraction

### It enables fast iteration

Current: edit `executor/exec.rs`, run `cargo test --workspace --lib`, wait ~90 seconds.
Target: edit `nika-verb-exec/src/lib.rs`, run `cargo test -p nika-verb-exec --lib`, wait ~2 seconds.
**45x iteration speedup** on the hot path of development.

---

## References

- **ADR-001:** [enum dispatch](02-adr-001-enum-dispatch.md) — why not `trait Verb`
- **ADR-002:** [typed contexts](03-adr-002-typed-contexts.md) — why per-verb borrowed slices
- **ADR-003:** [nika-extract](04-adr-003-nika-extract.md) — why extract is its own crate
- **ADR-004:** [delete TaskExecutor](05-adr-004-delete-task-executor.md) — why delete, not refactor
- **Mega plan:** [00-mega-plan.md](00-mega-plan.md) — session-by-session timeline
- **Session 12:** [06-session12-foundation.md](06-session12-foundation.md) — foundation work
- **Session 13:** [07-session13-extraction-1.md](07-session13-extraction-1.md) — exec/invoke/fetch
- **Session 14:** [08-session14-extraction-2.md](08-session14-extraction-2.md) — infer/agent + dissolution
