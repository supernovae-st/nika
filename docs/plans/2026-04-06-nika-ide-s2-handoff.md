# Nika IDE — Sprint 2 Mega Handoff

> **For Claude:** Copy this into a new session. Full autonomy mode.
> REQUIRED SKILLS: `test-driven-development`, `spn-rust:rust-core`, `verification-before-completion`
> **Co-Author**: `Co-Authored-By: Nika 🦋 <nika@supernovae.studio>`

---

## Context

Sprint 1 delivered 35 commits transforming the VS Code extension from a broken 32KB shell into a demo-ready IDE. Sprint 2 is 3 targeted improvements: architecture cleanup, NikaVault integration, and rmcp assessment. **May 5 launch.**

### What Exists Now (35 commits, all green)

- **editors/vscode/src/extension.ts** (~1061 lines) — monolith with binary installer, MCP config, LSP client, commands, sidebar, DAG panel
- **editors/vscode/src/dagPanel.ts** (340 lines) — DAG webview with CSP nonce, typed messages
- **editors/vscode/src/workflowTree.ts** — sidebar tree (imports from workflowParser.ts)
- **editors/vscode/src/workflowParser.ts** — pure parser (tested with vitest, 12 tests)
- **editors/vscode/src/webview/dag.ts** (641 lines) — ELK.js + D3.js renderer
- **editors/vscode/esbuild.mjs** — webview bundler (dag.js = 1.5MB)
- **tools/nika-lsp/src/backend.rs** (~1400 lines) — NikaBackend with DaemonProvider, workflow_graph (9 tests)
- **tools/nika-lsp/src/daemon_bridge.rs** — DaemonBridge (Clone, impl DaemonProvider)
- **tools/nika-daemon/src/provider.rs** — DaemonProvider trait (5 methods)
- **tools/nika-daemon/src/provider_embedded.rs** — EmbeddedDaemonProvider (env vars only, 7 tests)
- **tools/nika-mcp/src/server.rs** — 7 tools + 3 prompts, manual ServerHandler (283 tests)

### Test Baseline

```bash
cargo test --workspace --lib   # 10,331+ pass
cargo test --lib -p nika-mcp   # 283 pass
cargo test -p nika-lsp -- tests # 66 pass
cargo test --lib -p nika-daemon -- provider # 16 pass
cd editors/vscode && npx vitest run  # 12 pass
```

---

## Phase A: extension.ts Decomposition (1-2h)

### Goal
Split 1061-line monolith into 4 modules. Pure refactoring — zero behavior change.

### Module Map (from 10-agent analysis)

| Module | Functions | LOC | Key Exports |
|--------|-----------|-----|-------------|
| **binaryInstaller.ts** | getArtifactName, httpGet, readBody, downloadToFile, extractBinaryFromTarGz, extractBinaryFromZip, downloadNikaBinary, isBinaryWorking, findBundledBinary | ~300 | `downloadNikaBinary`, `isBinaryWorking`, `findBundledBinary`, `getArtifactName` |
| **mcpConfig.ts** | isCursor, isWindsurf, ensureCursorMcpConfig, ensureCursorRules, ensureVscodeMcpConfig, ensureWindsurfMcpConfig | ~165 | All 6 functions |
| **lspClient.ts** | getNikaPath, startClient, checkVersionMismatch, runNikaCommand | ~165 | All 4 functions |
| **extension.ts** | activate(), deactivate(), log(), command handlers | ~270 | activate, deactivate |

### Shared State Pattern

Module-level variables in current extension.ts:
```typescript
let client: LanguageClient | undefined;           // Used by lspClient, commands
let statusBarItem: StatusBarItem | undefined;      // Used by lspClient
let outputChannel: OutputChannel | undefined;      // Used by log()
let resolvedServerPath: string | undefined;        // Used by mcpConfig, lspClient
let statusPollInterval: NodeJS.Timeout | undefined; // Used by lspClient
let activeDagPanel: DagPanel | undefined;          // Used by event handler
```

**Approach**: Keep these in extension.ts. Pass as parameters to module functions. `log()` stays in extension.ts (exported) since all modules use it.

### Steps

1. Create `src/binaryInstaller.ts` — move lines 35-460 (pure Node.js, only `window.withProgress` from vscode)
2. Create `src/mcpConfig.ts` — move lines 462-624 (pass `resolvedServerPath` and `log` as params)
3. Create `src/lspClient.ts` — move lines 626-790 (pass state object with client, statusBar, etc.)
4. Update `src/extension.ts` — import from new modules, remove moved code
5. Verify: `npm run compile:ts && npx vitest run` — must pass unchanged
6. Commit: `refactor(vscode): decompose extension.ts into 4 modules`

### Import Cleanup

After decomposition, extension.ts should only import:
```typescript
import { workspace, commands, ExtensionContext, window, Uri, Position, Range, ProgressLocation, env, StatusBarAlignment } from 'vscode';
import { WorkflowTreeProvider } from './workflowTree';
import { DagPanel, DagGraph } from './dagPanel';
import { downloadNikaBinary, isBinaryWorking, findBundledBinary, getArtifactName } from './binaryInstaller';
import { isCursor, isWindsurf, ensureCursorMcpConfig, ensureCursorRules, ensureVscodeMcpConfig, ensureWindsurfMcpConfig } from './mcpConfig';
import { startClient, checkVersionMismatch, runNikaCommand, getNikaPath } from './lspClient';
```

---

## Phase B: NikaVault in EmbeddedDaemonProvider (30m)

### Goal
EmbeddedDaemonProvider currently only checks env vars for API keys. Users who stored keys via `nika keys set` see "no key" in embedded mode. Fix: check NikaVault as fallback.

### Key Facts (from agent research)

- `nika-daemon` already depends on `nika-vault` (Cargo.toml line 19)
- `NikaVault::new(secrets_dir: &Path) -> Self` — no passphrase at construction
- Key derivation: `NIKA_VAULT_PASSPHRASE` env var OR machine fingerprint (auto)
- `vault.get(provider) -> Result<Option<SecretString>>` — read-only, no locks
- NikaVault is Send + Sync — safe for OnceCell
- Secrets dir: `~/.nika/daemon/secrets/` (via `daemon_dir().join("secrets")`)

### Implementation

File: `tools/nika-daemon/src/provider_embedded.rs`

```rust
use std::sync::OnceLock;
use nika_vault::NikaVault;

pub struct EmbeddedDaemonProvider {
    started_at: Instant,
    vault: OnceLock<Option<NikaVault>>,  // Lazy, fallible init
}

impl EmbeddedDaemonProvider {
    pub fn new() -> Self {
        Self {
            started_at: Instant::now(),
            vault: OnceLock::new(),
        }
    }

    fn vault(&self) -> Option<&NikaVault> {
        self.vault.get_or_init(|| {
            let secrets_dir = crate::daemon_dir().join("secrets");
            if secrets_dir.exists() {
                Some(NikaVault::new(&secrets_dir))
            } else {
                None
            }
        }).as_ref()
    }
}

// In provider_status():
let (has_key, source) = if p.has_env_key() {
    (true, KeySource::Env)
} else if self.vault().and_then(|v| v.get(p.id).ok().flatten()).is_some() {
    (true, KeySource::Vault)
} else {
    (false, KeySource::NotFound)
};
```

### Tests
```rust
#[tokio::test]
async fn embedded_provider_status_checks_env() {
    // Already exists — verify it still passes
}

#[tokio::test]
async fn embedded_vault_lazy_init() {
    let p = EmbeddedDaemonProvider::new();
    // Vault should be None if ~/.nika/daemon/secrets doesn't exist
    // This test just verifies no panic on missing vault
    let status = p.provider_status().await;
    assert!(!status.is_empty());
}
```

### Commit
```
feat(daemon): check NikaVault in EmbeddedDaemonProvider for API keys

provider_status() now falls back to NikaVault after env var check.
Users who stored keys via `nika keys set` now see them in embedded mode.
Vault is lazily initialized via OnceLock, read-only, no passphrase needed.
```

---

## Phase C: DaemonBridge Always-Valid Refactor (1h)

### Goal
Eliminate RwLockDaemonProvider adapter by making DaemonBridge always-valid (starts disconnected, reconnects in-place).

### Design (from rust-architect agent)

**Current**: `Arc<RwLock<Option<DaemonBridge>>>` → `RwLockDaemonProvider` → `DaemonProvider`
**After**: `Arc<DaemonBridge>` → `DaemonProvider` (DaemonBridge already impls the trait)

### Changes

**daemon_bridge.rs** — Add `reconnect()` method:
```rust
#[cfg(unix)]
pub async fn reconnect(&self) -> bool {
    let daemon_client = nika_daemon::client::DaemonClient::default_path();
    if !daemon_client.socket_exists() {
        return false;
    }
    match tokio::time::timeout(REQUEST_TIMEOUT, daemon_client.connect()).await {
        Ok(Ok(connected)) => {
            *self.client.write().await = Some(connected);
            self.connected.store(true, Ordering::Relaxed);
            // Reset cache to force refresh
            let mut cache = self.providers_cache.write().await;
            cache.fetched_at = Instant::now() - CACHE_TTL;
            true
        }
        _ => false,
    }
}
```

**daemon_bridge.rs** — Update `spawn_reconnect_loop` to take `Arc<DaemonBridge>`:
```rust
pub fn spawn_reconnect_loop(bridge: Arc<DaemonBridge>) {
    #[cfg(unix)]
    tokio::spawn(async move {
        let mut delay = RECONNECT_INITIAL;
        loop {
            tokio::time::sleep(delay).await;
            if !bridge.is_connected() {
                if bridge.reconnect().await {
                    delay = RECONNECT_INITIAL;
                } else {
                    delay = (delay * 2).min(RECONNECT_MAX);
                }
            } else {
                delay = RECONNECT_MAX;
            }
        }
    });
}
```

**backend.rs** — Delete `RwLockDaemonProvider` entirely (~60 lines), simplify `new()`:
```rust
#[cfg(unix)]
let daemon: Arc<dyn DaemonProvider> = {
    let bridge = Arc::new(DaemonBridge::disconnected());
    let b = bridge.clone();
    tokio::spawn(async move { b.reconnect().await; });
    DaemonBridge::spawn_reconnect_loop(bridge.clone());
    bridge
};
```

### Verification
```bash
cargo test -p nika-lsp -- tests  # 66 pass
cargo test --lib -p nika-daemon -- provider  # 16 pass
cargo test -p nika-lsp -- tests::workflow_graph  # 9 pass
```

### Commit
```
refactor(lsp): eliminate RwLockDaemonProvider — DaemonBridge always-valid

DaemonBridge starts disconnected and reconnects in-place via new
reconnect() method. NikaBackend holds Arc<DaemonBridge> directly.
Removes 60-line adapter. One less layer of indirection.
```

---

## Phase D: rmcp Assessment (research-only, 30m)

### Context

rmcp is pinned at 0.16. Latest is 1.3.0. BUT rig-core may also depend on rmcp.

### Research Questions (agents investigating)

1. Does rig-core depend on rmcp? → Check Cargo.lock
2. If yes, can nika-mcp use a different rmcp version? → Cargo allows duplicate versions
3. What breaking changes in rmcp 1.x? → #[tool_handler] + #[prompt_handler] coexist natively
4. Is the upgrade worth the risk before May 5? → Probably NO (too risky, current setup works)

### Decision Framework

- If rig-core pins rmcp 0.16 → **defer upgrade** (version conflict risk)
- If rig-core doesn't use rmcp → **upgrade in Phase D** (safe)
- If upgrade is safe but complex → **document migration plan for post-launch**

### Expected Output
- Decision: UPGRADE / DEFER / DOCUMENT
- If DEFER: write `docs/plans/rmcp-1x-migration.md` with migration steps

---

## Phase E: tower-lsp Custom Notification (30m)

### Goal
Define and send `nika/executionEvent` notifications from the LSP backend.

### Pattern (from agent research)

```rust
// Define the notification type
use tower_lsp_server::ls_types::notification::Notification;

pub struct ExecutionEventNotification;

impl Notification for ExecutionEventNotification {
    const METHOD: &'static str = "nika/executionEvent";
    type Params = serde_json::Value;
}

// Send from backend (when embedded daemon runs a workflow)
client.send_notification::<ExecutionEventNotification>(serde_json::json!({
    "taskId": task_id,
    "status": "running", // or "success", "failed", "skipped"
}));
```

### Where to wire
In `backend.rs`, the `EmbeddedDaemonProvider` could expose an `event_rx` channel. The backend spawns a task that reads events and forwards via `send_notification`. BUT: for the initial wiring, just define the type and test that it compiles. Actual event emission requires in-process engine execution (post-launch).

### Commit
```
feat(lsp): define nika/executionEvent notification type

Prepares the server→client notification channel for live DAG updates.
Actual emission requires in-process engine execution (post-launch).
```

---

## Execution Order

```
Phase A (1-2h) → extension.ts decomposition (pure refactoring, zero risk)
Phase B (30m)  → NikaVault in EmbeddedDaemonProvider (adds vault fallback)
Phase C (1h)   → DaemonBridge always-valid refactor (removes 60 lines)
Phase D (30m)  → rmcp assessment (research → decision document)
Phase E (30m)  → tower-lsp notification type definition
```

Total: ~3-4 hours. Each phase is independent. Commit after each. Push when done.

---

## Verification (after ALL phases)

```bash
# Rust
cd tools && cargo test --workspace --lib
cargo tree -p nika-lsp --no-dedupe | grep -c "nika-engine"  # MUST be 0
cargo clippy --workspace

# TypeScript
cd editors/vscode && npm run compile
npx vitest run

# Webview
test -f out/webview/dag.js

# Manual
# - Open .nika.yaml on VS Code → code lens visible
# - Click code lens → terminal runs nika
# - Show DAG → webview renders
# - Click DAG node → jumps to source line
# - Sidebar → workflows listed with verb icons
```

---

## Key Gotchas

1. **NikaVault path**: Use `crate::daemon_dir().join("secrets")` — NOT `~/.nika/secrets/`
2. **DaemonBridge::reconnect()**: Must reset cache `fetched_at` to force first provider_status refresh
3. **extension.ts decomposition**: Keep module-level variables in extension.ts, pass as params
4. **rmcp**: If rig-core pins 0.16, DO NOT upgrade. Version conflicts will break the build.
5. **tower-lsp notifications**: `send_notification` only works when server state is `Initialized`
6. **Tests**: Always `cargo test --workspace --lib` (--lib avoids keychain popups)
7. **Commits**: `Co-Authored-By: Nika 🦋 <nika@supernovae.studio>` — NEVER Claude/Anthropic

---

## Skills to Use

| Phase | Skill |
|-------|-------|
| All | `test-driven-development` (write test first) |
| A, C | `spn-rust:rust-core` (ownership, error handling) |
| A | No skill needed (pure TS refactoring) |
| B | `spn-rust:rust-core` (OnceLock, Option chaining) |
| C | `spn-rust:rust-async` (Arc, RwLock, tokio::spawn) |
| D | `spn-search:search` (rmcp changelog) |
| E | `spn-rust:rust-core` (trait impl) |
| All | `verification-before-completion` (run tests before claiming done) |
