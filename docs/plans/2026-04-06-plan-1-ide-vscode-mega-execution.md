# Plan 1: IDE / VSCode Extension — Mega Execution Plan

> **Date**: 2026-04-06 | **Version**: v0.73.0 | **Target**: v0.75.0
> **Effort**: 12-15 days | **27 tasks** | **8 phases**
> **Prerequisites**: All 10,190+ tests GREEN

## Executive Summary

Transform Nika VS Code extension from broken 32KB shell into first-class IDE experience.
Two extensions exist today — merge them into one powerhouse.

```
BEFORE: Install → hope binary downloads → hope daemon starts → basic text editing
AFTER:  Install (18MB) → everything works → code lens → inlay hints → DAG panel
        → sidebar tree → Cursor AI writes valid .nika.yaml
```

## Current State

### Two Separate Extensions

| Extension | Path | Version | Status |
|-----------|------|---------|--------|
| `nika-lang` (production) | `editors/vscode/` | v0.51.0 | Published, 759 lines, 8 bugs |
| `nika-vscode` (DAG scaffold) | `tools/nika-vscode/` | v0.1.0 | POC, hardcoded demo data |

### LSP Backend (Excellent, Hidden)

- 13 handlers in `nika-lsp-core/` (745+ tests)
- 16-variant `CursorContext` enum
- Tree-sitter error recovery
- **6 features coded but INVISIBLE** (not declared in package.json)

### 8 Known Bugs

| # | Bug | Severity | Root Cause |
|---|-----|----------|------------|
| 1 | `nika.runWorkflow not found` on Cursor | CRITICAL | Commands registered AFTER async `execFile` (race) |
| 2 | `@ts-ignore` StatusBarAlignment | HIGH | Uses magic number `1` instead of enum |
| 3 | Code Lens arguments always null | HIGH | No file URI passed to command |
| 4 | restartServer loses binary path | MEDIUM | `resolvedServerPath` not persisted |
| 5 | Status interval accumulates | MEDIUM | `setInterval` never cleared on restart |
| 6 | `installExtension` fails on Cursor | MEDIUM | Cursor-only command, no fallback |
| 7 | Path escaping in terminal commands | MEDIUM | `"` and `$` not escaped |
| 8 | Hidden LSP features | HIGH | Not declared in package.json |

---

## Dependency Graph

```
Phase 0 (30 min) ── 8 bug fixes + activate 6 hidden features
     |
     +---> Phase 6 (1 day) ── Decouple nika-lsp from nika-engine
     |
     +---> Phase 1 (1 day) ── Platform-specific VSIX binary bundling
     |         |
     |         +---> Phase 3 (1-2 days) ── Cursor/Windsurf auto-config
     |
     +---> Phase 2 (2-3 days) ── DaemonProvider trait + embedded daemon
     |         |
     |         +---> Phase 4 (3-5 days) ── DAG webview (ELK.js + D3.js)
     |         |
     |         +---> Phase 5 (2 days) ── Sidebar tree view
     |
     +---> Phase 7 (2 days) ── MCP expansion (independent)
```

---

## Phase 0: Bug Fixes + Activate Hidden Features (30 min)

**Files**: `editors/vscode/src/extension.ts`, `editors/vscode/package.json`

### Task 0.1: Fix Command Registration Race (CRITICAL)

**Root cause**: `execFile` at line 636 is async. Commands at lines 682-750 registered
synchronously AFTER it. On Cursor, Code Lens fires before commands exist.

**Fix**: Move all 5 `commands.registerCommand` blocks (lines 682-750) to BEFORE
the `execFile` call at line 636. Place right after line 607.

```
Commit: fix(vscode): register commands before async binary probe
```

### Task 0.2: Fix StatusBarAlignment @ts-ignore

**Fix**: Add `StatusBarAlignment` to import (line 1). Replace magic number with enum.

```typescript
// BEFORE:
// @ts-ignore -- StatusBarAlignment.Left is 1
1, // vscode.StatusBarAlignment.Left

// AFTER:
StatusBarAlignment.Left,
```

```
Commit: fix(vscode): use StatusBarAlignment enum instead of magic number
```

### Task 0.3: Fix Code Lens Arguments (pass URI)

**Fix**: Command handlers accept optional `Uri`:

```typescript
// BEFORE:
commands.registerCommand('nika.runWorkflow', () => {
  const editor = window.activeTextEditor;

// AFTER:
commands.registerCommand('nika.runWorkflow', (uri?: Uri) => {
  const filePath = uri?.fsPath ?? window.activeTextEditor?.document.fileName;
```

Apply to `nika.runWorkflow` and `nika.checkWorkflow`.

```
Commit: fix(vscode): pass file URI to code lens command handlers
```

### Task 0.4: Fix restartServer Losing Binary Path

**Fix**: Add module-scope `let resolvedBinaryPath: string | null = null;`.
Store when resolved. Use in restartServer.

```
Commit: fix(vscode): persist binary path across LSP restart
```

### Task 0.5: Fix Status Interval Accumulation

**Fix**: Add module-scope `let statusPollInterval`. Clear before creating new.

```
Commit: fix(vscode): clear status polling interval on restart
```

### Task 0.6: Fix Cursor installExtension

**Fix**: Add fallback to `env.openExternal(Uri.parse('marketplace URL'))`.

```
Commit: fix(vscode): fallback to marketplace URL on Cursor
```

### Task 0.7: Fix Path Escaping

**Fix**: Escape before interpolation in `runNikaCommand`:

```typescript
const escaped = filePath.replace(/\\/g, '\\\\').replace(/"/g, '\\"').replace(/\$/g, '\\$');
```

```
Commit: fix(vscode): escape special characters in terminal file paths
```

### Task 0.8: Activate Hidden LSP Features

**Add to package.json**:

1. `semanticTokenScopes` — 6 token type mappings
2. `nika.inlayHints.enabled` + `nika.codeLens.enabled` config
3. `"extensionKind": ["workspace"]` — Remote/WSL/Codespaces support
4. Keybindings: `Cmd+Shift+R` (run), `Cmd+Shift+K` (check)

```
Commit: feat(vscode): activate code lens, inlay hints, semantic tokens, keybindings
```

### Phase 0 Verification

```bash
cd editors/vscode && npm run compile  # Must pass
# Manual: open .nika.yaml, verify code lens + inlay hints visible
```

---

## Phase 6: Decouple nika-lsp from nika-engine (1 day)

> See **Plan 2** for full details. Summary here for dependency tracking.

**Problem**: nika-lsp imports ONLY re-exports from nika-core but pulls in full
nika-engine (reqwest, rig-core, petgraph, 157K LOC). Compile: ~90s. Should be: ~15s.

**Fix**: Replace ALL `nika_engine::` imports with `nika_core::` in nika-lsp.
Remove `nika-engine` from `tools/nika-lsp/Cargo.toml`.

**Verification**:
```bash
cd tools && cargo test --workspace --lib
cargo tree -p nika-lsp --no-dedupe | grep -c "nika-engine"  # MUST BE 0
cargo build -p nika-lsp --timings  # Should be ~15s
```

```
Commit: refactor(lsp): decouple nika-lsp from nika-engine — compile 90s to 15s
```

---

## Phase 1: Platform-Specific VSIX Binary Bundling (1 day)

> See **Plan 3** for CI/release details.

### Task 1.1: Add Bundled Binary Discovery

**Pattern** (rust-analyzer): Binary at `editors/vscode/server/nika[.exe]`.

**Add `findBundledBinary()` to extension.ts**:

```typescript
function findBundledBinary(context: ExtensionContext): string | null {
  const bin = process.platform === 'win32' ? 'nika.exe' : 'nika';
  const bundled = path.join(context.extensionPath, 'server', bin);
  return fs.existsSync(bundled) ? bundled : null;
}
```

**Discovery priority**: config > bundled > PATH > cached > download.

**`.vscodeignore`** (deny-all pattern):
```
**
!server/
!server/nika
!server/nika.exe
!out/**/*.js
!syntaxes/**
!snippets/**
!icons/**
!package.json
!LICENSE
```

```
Commit: feat(vscode): add bundled binary discovery (rust-analyzer pattern)
```

### Task 1.2: Update CI for Platform VSIX

Replace single VSIX job with 6-entry matrix:

| Platform | Target | Size |
|----------|--------|------|
| darwin-arm64 | aarch64-apple-darwin | ~18MB |
| darwin-x64 | x86_64-apple-darwin | ~18MB |
| linux-x64 | x86_64-unknown-linux-gnu | ~18MB |
| linux-arm64 | aarch64-unknown-linux-gnu | ~18MB |
| win32-x64 | x86_64-pc-windows-msvc | ~18MB |
| universal | (no binary) | ~32KB |

```bash
npx vsce package --no-git-tag-version --target darwin-arm64
```

```
Commit: feat(ci): platform-specific VSIX with bundled nika binary
```

---

## Phase 2: DaemonProvider Trait + Embedded Daemon (2-3 days)

### Task 2.1: Create DaemonProvider Trait

**New file**: `tools/nika-daemon/src/provider.rs`

```rust
#[async_trait]
pub trait DaemonProvider: Send + Sync + 'static {
    fn is_connected(&self) -> bool;
    async fn provider_status(&self) -> Vec<ProviderStatusInfo>;
    async fn estimate_cost(&self, provider: &str, model: &str,
        input_tokens: u64, output_tokens: u64) -> Option<CostEstimate>;
    async fn workflow_history(&self, workflow: &str) -> Vec<WorkflowRunInfo>;
    async fn capabilities(&self) -> Option<DaemonCapabilities>;
}
```

Types live in `nika-core/src/catalogs/lsp_types.rs` (lines 25-77).

```
Commit: feat(daemon): add DaemonProvider trait with contract tests
```

### Task 2.2: Implement IpcDaemonProvider

**New file**: `tools/nika-daemon/src/provider_ipc.rs`
- Wraps existing `ConnectedClient`
- Extracts caching logic from `daemon_bridge.rs`

```
Commit: feat(daemon): implement IpcDaemonProvider wrapping ConnectedClient
```

### Task 2.3: Implement InProcessDaemonProvider

**New file**: `tools/nika-daemon/src/provider_embedded.rs`

**CRITICAL GOTCHAS**:
- `SecretService`: NOT Clone, wrap in Arc
- `CacheService`: IS Clone (Arc<CacheInner>, DashMap)
- `JobService`: IS Clone. Uses `current_exe()` which returns `nika-lsp` not `nika` — pass explicit `nika_bin: Option<PathBuf>`
- `EventBus`: NOT Clone, wrap in Arc. broadcast::Sender is lock-free
- `estimate_cost()`: Call `nika_core::catalogs::estimate_cost()` directly — no daemon needed
- **NikaVault path**: Use `~/.nika/secrets/` (same as engine), NOT `~/.nika/daemon/secrets/`

```
Commit: feat(daemon): implement InProcessDaemonProvider for embedded mode
```

### Task 2.4: Refactor NikaBackend to Use Trait

Change `NikaBackend.daemon` from `Arc<RwLock<Option<DaemonBridge>>>` to `Arc<dyn DaemonProvider>`.

5 call sites in `backend.rs` to simplify:
- Line 68-73: Construction
- Line 110-129: `daemon_status()` handler
- Line 364-370: `completion()` enrichment
- Line 432-441: `hover()` enrichment
- Line 76: reconnect loop

```
Commit: refactor(lsp): use DaemonProvider trait in NikaBackend
```

### Task 2.5: Wire Extension to Use --embedded-daemon

**main.rs**: Add `--embedded-daemon` flag:
```rust
let embedded = std::env::args().any(|a| a == "--embedded-daemon")
    || std::env::var("NIKA_LSP_EMBEDDED_DAEMON").is_ok();
```

**extension.ts**: Pass `--embedded-daemon` in server args.

```
Commit: feat(lsp): wire --embedded-daemon flag for zero-config IDE
```

### Phase 2 Verification

```bash
cd tools && cargo test --lib -p nika-daemon -- provider
cd tools && cargo test --lib -p nika-lsp
cd tools && cargo test -p nika-lsp --test e2e_harness -- --ignored
```

---

## Phase 3: Cursor/Windsurf Auto-Config (1-2 days)

### Task 3.1: IDE Detection + Cursor Auto-Config

```typescript
function isCursor(): boolean {
  return env.appName === 'Cursor' || env.uriScheme === 'cursor';
}
```

Auto-generate on activation:
- `.cursor/mcp.json` with `mcpServers` key
- `.cursor/rules/nika.mdc` with schema + verbs + rules

**GOTCHA**: VS Code MCP uses `"servers"` key, NOT `"mcpServers"`.

```
Commit: feat(vscode): auto-generate Cursor MCP config on activation
```

### Task 3.2: VS Code MCP Configuration

Auto-generate `.vscode/mcp.json` with `servers` key.
Don't overwrite existing files (check `workspace.fs.stat()`).

```
Commit: feat(vscode): auto-generate VS Code MCP config
```

---

## Phase 4: DAG Webview (3-5 days)

### Task 4.1: Esbuild + Webview Build Infra

- Copy scaffold from `tools/nika-vscode/` to `editors/vscode/`
- Dual targets: Node.js extension + browser webview
- Dependencies: `elkjs`, `d3-selection`, `d3-zoom`, `d3-shape`, `d3-transition`

```
Commit: chore(vscode): add esbuild dual-target build for extension + webview
```

### Task 4.2: Add nika/workflowGraph LSP Request

Custom request in `backend.rs` returning `{ nodes, edges }` JSON.
Register alongside existing `nika/daemonStatus`.

```
Commit: feat(lsp): add nika/workflowGraph custom request for DAG data
```

### Task 4.3: Create Webview Provider + Registration

- `dagPanel.ts`: WebviewPanel with CSP nonce
- Register `nika.showDag` command
- Message protocol: `ExtToWebviewMessage` / `WebviewToExtMessage`

```
Commit: feat(vscode): add DAG webview panel with CSP and message protocol
```

### Task 4.4: Create D3 + ELK Rendering

- ELK.js: Sugiyama layered layout, direction DOWN
- D3.js: SVG rendering with zoom/pan
- **Verb colors**: infer=#7c3aed, exec=#16a34a, fetch=#0891b2, invoke=#db2778, agent=#d97306
- **Status animations** (CSS): running=blue glow, success=green flash, failed=red blink, skipped=dashed 50%

**GOTCHA**: D3 needs `style-src 'unsafe-inline'` for transforms. Use nonce for scripts.

```
Commit: feat(vscode): interactive DAG rendering with ELK layout + D3 SVG
```

---

## Phase 5: Sidebar Tree View (2 days)

### Task 5.1: Register Tree View in package.json

```json
"viewsContainers": {
  "activitybar": [{
    "id": "nika",
    "title": "Nika",
    "icon": "./icons/nika-dark.svg"
  }]
},
"views": {
  "nika": [
    { "id": "nikaWorkflows", "name": "Workflows" },
    { "id": "nikaProviders", "name": "Providers" }
  ]
}
```

```
Commit: feat(vscode): register Nika sidebar with workflow + provider views
```

### Task 5.2: Implement Workflow Tree Data Provider

- Root: `workspace.findFiles('**/*.nika.yaml')`
- Children: regex `/^\s*-\s*id:\s*(\S+)/gm`
- Click: `vscode.open` with selection range
- Refresh: `FileSystemWatcher` on `**/*.nika.yaml`

```
Commit: feat(vscode): workflow tree provider with task navigation
```

---

## Phase 7: MCP Expansion (2 days, independent)

### Task 7.1: Add MCP Prompts

Enable prompts in server capabilities:
```rust
ServerCapabilities::builder()
    .enable_tools()
    .enable_prompts()  // NEW
    .build()
```

3 prompts: `create-workflow`, `add-task`, `fix-error`

```
Commit: feat(mcp): add 3 MCP prompts for AI-assisted workflow authoring
```

### Task 7.2: Add New MCP Tools

3 new tools: `nika_generate_task`, `nika_dag_visualization`, `nika_error_fix`

```
Commit: feat(mcp): add 3 new tools for task generation, DAG viz, error fixing
```

### Task 7.3: Add Event Streaming

LSP subscribes to EventBus, emits `nika/executionEvent` notifications.
Extension forwards to webview via `postMessage`.

**Event flow**:
```
Engine TaskEventGuard -> EventLog (broadcast)
  -> InProcessDaemonProvider -> EventBus
    -> LSP NikaBackend -> nika/executionEvent notification
      -> Extension -> webview.postMessage()
        -> D3 node color update (300ms)
```

```
Commit: feat(lsp): stream execution events to extension webview
```

---

## Guardrails (run after EVERY phase)

```bash
# Rust (always --lib to avoid keychain popups)
cd tools && cargo test --workspace --lib

# Crate boundary (after Phase 6)
cargo tree -p nika-lsp --no-dedupe | grep -c "nika-engine"  # MUST BE 0

# E2E harness
cargo build -p nika-lsp && cargo test -p nika-lsp --test e2e_harness -- --ignored

# DaemonProvider contracts (after Phase 2)
cargo test --lib -p nika-daemon -- provider

# MCP tests (after Phase 7)
cargo test --lib -p nika-mcp

# TypeScript
cd editors/vscode && npm ci && npm run compile

# VSIX build (after Phase 1)
npx vsce package --no-git-tag-version --target darwin-arm64

# Webview (after Phase 4)
test -f out/webview/dag.js
```

---

## Research References

| Report | Path | Key Finding |
|--------|------|-------------|
| Binary Bundling | `docs/research/2026-04-06-vscode-platform-binary-bundling.md` | rust-analyzer pattern, `vsce --target`, 15-20MB |
| DAG Webview | `docs/research/2026-04-06-vscode-dag-webview-research.md` | ELK.js + D3.js, ~400KB bundle |
| MCP Cross-Editor | `docs/research/2026-04-06-mcp-integration-cursor-windsurf-research.md` | 4 editors, different config keys |
| TDD Strategy | `docs/research/2026-04-06-vscode-extension-tdd-research.md` | 4 layers: unit Rust, unit TS, LSP integration, E2E |

## Skills Required

| When | Skill | Why |
|------|-------|-----|
| Every task | `test-driven-development` | Write test first |
| Rust code | `spn-rust:rust-core` | Ownership, error handling |
| Async Rust | `spn-rust:rust-async` | tokio, DaemonProvider |
| Before "done" | `verification-before-completion` | Run tests |
| After phase | `requesting-code-review` | Dispatch reviewer |
| Debugging | `systematic-debugging` | 4-phase root cause |

## Commit Convention

```
type(scope): description

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

Scopes: `vscode`, `lsp`, `daemon`, `mcp`, `ci`

---

## Summary

| Phase | Tasks | Time | Key Deliverable |
|-------|-------|------|-----------------|
| 0 | 8 | 30 min | Bugs fixed, 6 LSP features activated |
| 6 | 1 | 1 day | nika-lsp compiles in 15s (not 90s) |
| 1 | 2 | 1 day | Platform VSIX with bundled binary |
| 2 | 5 | 2-3 days | DaemonProvider trait, embedded daemon |
| 3 | 2 | 1-2 days | Cursor/Windsurf auto-config |
| 4 | 4 | 3-5 days | Interactive DAG webview |
| 5 | 2 | 2 days | Sidebar workflow tree |
| 7 | 3 | 2 days | 7 MCP tools + 3 prompts |
| **Total** | **27** | **12-15 days** | **Full IDE experience** |
