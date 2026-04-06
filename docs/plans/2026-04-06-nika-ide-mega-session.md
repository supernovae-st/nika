# Nika IDE — Mega Session Autonome

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan.
> Use superpowers:test-driven-development for EVERY code change.
> Use superpowers:verification-before-completion before claiming any task done.
> Use superpowers:requesting-code-review after each phase.

**This document is self-contained.** Everything needed for autonomous execution is here.
No external context needed. Read top to bottom before starting.

---

## What We're Building

Transform the Nika VS Code extension from a broken 32KB shell into a first-class IDE.

**Before**: Extension installs → downloads binary from GitHub → fails on Cursor → 6/13 LSP features invisible → no visual feedback → AI editors don't understand Nika.

**After**: Extension installs (18MB, binary included) → everything works → code lens, inlay hints, semantic tokens → interactive DAG panel → sidebar tree → Cursor AI writes valid .nika.yaml natively.

**Plan**: `docs/plans/2026-04-06-nika-ide-plan.md` (500+ lines, 27 tasks, 8 phases)

**Research reports** (READ before implementing):
- `docs/research/2026-04-06-vscode-platform-binary-bundling.md`
- `docs/research/2026-04-06-vscode-dag-webview-research.md`
- `docs/research/2026-04-06-mcp-integration-cursor-windsurf-research.md`
- `docs/research/2026-04-06-vscode-extension-tdd-research.md`

---

## Skills to Use (MANDATORY)

| When | Skill |
|------|-------|
| Starting each task | `superpowers:test-driven-development` |
| Any Rust code | `spn-rust:rust-core` |
| Async Rust (DaemonProvider) | `spn-rust:rust-async` |
| Before claiming done | `superpowers:verification-before-completion` |
| After each phase | `superpowers:requesting-code-review` |
| Debugging failures | `superpowers:systematic-debugging` |
| Multiple independent fixes | `superpowers:dispatching-parallel-agents` |

---

## Execution Order

```
Phase 0 (30 min) ── 8 bug fixes + activate 6 hidden features
     │
     ├──→ Phase 6 (1 day) ── Decouple nika-lsp from nika-engine
     │
     ├──→ Phase 1 (1 day) ── Platform-specific VSIX binary bundling
     │         │
     │         └──→ Phase 3 (1-2 days) ── Cursor/Windsurf auto-config
     │
     ├──→ Phase 2 (2-3 days) ── DaemonProvider trait + embedded daemon
     │         │
     │         ├──→ Phase 4 (3-5 days) ── DAG webview (ELK.js + D3.js)
     │         │
     │         └──→ Phase 5 (2 days) ── Sidebar tree view
     │
     └──→ Phase 7 (2 days) ── MCP expansion (parallel, independent)
```

---

## Commit Convention

```
type(scope): description

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

Types: `fix`, `feat`, `refactor`, `test`, `chore`
Scopes: `vscode`, `lsp`, `daemon`, `mcp`, `ci`

**NEVER use Claude/Anthropic co-author. ALWAYS use Nika 🦋.**

---

## Testing Strategy

**Rust (10,000+ existing tests):**
```bash
cd tools && cargo test --workspace --lib  # Always --lib to avoid keychain popups
```

**LSP E2E harness (17 scenarios):**
```bash
cd tools && cargo build -p nika-lsp
cd tools && cargo test -p nika-lsp --test e2e_harness -- --ignored
```

**Extension (NEW — add vitest):**
```bash
cd editors/vscode && npm test
```

**MCP server:**
```bash
cd tools && cargo test --lib -p nika-mcp
```

**Guardrail — nika-lsp must NOT depend on nika-engine:**
```bash
cd tools && cargo tree -p nika-lsp --no-dedupe | grep -c "nika-engine"  # Must be 0
```

---

## Phase 0: Bug Fixes + Activate Hidden Features

### Key Files
- `editors/vscode/src/extension.ts` (759 lines)
- `editors/vscode/package.json` (184 lines)

### Bug 1: Commands After Async Probe (CRITICAL — Cursor crash)

**Root cause**: `execFile` at line 636 is async. Commands at lines 682-750 are registered synchronously AFTER it, but on Cursor the Code Lens fires before commands exist.

**Fix**: Move all 5 `commands.registerCommand` blocks (lines 682-750) to BEFORE the `execFile` call at line 636. Place them right after line 607 (nika.showOutput registration).

### Bug 2: @ts-ignore StatusBarAlignment

**Fix**: Add `StatusBarAlignment` to import (line 1). Replace lines 591-595:
```typescript
// BEFORE:
// @ts-ignore -- StatusBarAlignment.Left is 1
1, // vscode.StatusBarAlignment.Left

// AFTER:
StatusBarAlignment.Left,
```

### Bug 3: Code Lens Arguments Null

**Fix**: Change command handlers to accept optional `Uri`:
```typescript
// BEFORE:
commands.registerCommand('nika.runWorkflow', () => {
  const editor = window.activeTextEditor;

// AFTER:
commands.registerCommand('nika.runWorkflow', (uri?: Uri) => {
  const filePath = uri?.fsPath ?? window.activeTextEditor?.document.fileName;
```
Same for `nika.checkWorkflow`.

### Bug 4: restartServer Loses Binary Path

**Fix**: Add module-scope `let resolvedBinaryPath: string | null = null;` (after line 26).
Store it in `tryStartWithBinary` and when PATH found. Use in restartServer:
```typescript
startClient(context, resolvedBinaryPath ?? undefined);
```

### Bug 5: setInterval Accumulates

**Fix**: Add module-scope `let statusPollInterval: ReturnType<typeof setInterval> | undefined;`.
Clear before creating new: `if (statusPollInterval) clearInterval(statusPollInterval);`

### Bug 6: installExtension Fails on Cursor

**Fix**: Add `.then(undefined, () => env.openExternal(Uri.parse('marketplace URL')))` to the `executeCommand` call at line 572.

### Bug 7: Path Escaping

**Fix**: In `runNikaCommand`, escape before interpolation:
```typescript
const escaped = filePath.replace(/\\/g, '\\\\').replace(/"/g, '\\"').replace(/\$/g, '\\$');
```

### Bug 8: Activate Hidden LSP Features

**Add to package.json `contributes`:**

```json
"semanticTokenScopes": [
  {
    "language": "nika",
    "scopes": {
      "keyword": ["keyword.control.nika"],
      "macro": ["entity.name.function.verb.nika"],
      "property": ["variable.other.property.nika"],
      "variable": ["variable.other.template.nika"],
      "type": ["entity.name.type.nika"],
      "comment": ["comment.nika"]
    }
  }
]
```

**Add to configuration properties:**
```json
"nika.inlayHints.enabled": { "type": "boolean", "default": true },
"nika.codeLens.enabled": { "type": "boolean", "default": true }
```

**Add to root of package.json:**
```json
"extensionKind": ["workspace"]
```

**Add keybindings:**
```json
"keybindings": [
  { "command": "nika.runWorkflow", "key": "ctrl+shift+r", "mac": "cmd+shift+r", "when": "resourceLangId == 'nika'" },
  { "command": "nika.checkWorkflow", "key": "ctrl+shift+k", "mac": "cmd+shift+k", "when": "resourceLangId == 'nika'" }
]
```

### Verification (Phase 0)
```bash
cd editors/vscode && npm run compile  # Must pass
# Manual: open .nika.yaml on VS Code, verify code lens + inlay hints visible
```

### Commit
One commit per bug (8 commits total). Then one commit for package.json activation.

---

## Phase 6: Decouple nika-lsp from nika-engine

### Why
nika-lsp imports ONLY re-exports from nika-core. Dropping nika-engine cuts compile from ~90s to ~15s.

### Key Files
- `tools/nika-lsp/Cargo.toml`
- `tools/nika-lsp/src/diagnostics.rs`
- `tools/nika-lsp/src/ast_integration.rs`
- `tools/nika-lsp/src/backend.rs`

### What to Change
Replace ALL `nika_engine::` imports with `nika_core::`:
- `nika_engine::ast::raw::` → `nika_core::ast::raw::`
- `nika_engine::ast::analyzer::` → `nika_core::ast::analyzer::`
- `nika_engine::ast::analyzed::` → `nika_core::ast::analyzed::`
- `nika_engine::source::` → `nika_core::source::`

Remove `nika-engine` from `tools/nika-lsp/Cargo.toml`, keep `nika-core`.

### Verification
```bash
cd tools && cargo test --workspace --lib
cargo tree -p nika-lsp --no-dedupe | grep -c "nika-engine"  # MUST BE 0
cargo build -p nika-lsp --timings  # Should be ~15s
```

---

## Phase 1: Platform-Specific VSIX

### Pattern (from rust-analyzer research)
- Binary goes in `editors/vscode/server/nika[.exe]`
- `.vscodeignore` uses deny-all: `**` then `!server`
- `vsce package --target darwin-arm64` stamps the .vsix
- 5 targets: darwin-arm64, darwin-x64, linux-x64, win32-x64, linux-arm64
- Plus 1 universal "no-server" fallback

### Key Changes

**extension.ts** — Add `findBundledBinary()`:
```typescript
function findBundledBinary(context: ExtensionContext): string | null {
  const bin = process.platform === 'win32' ? 'nika.exe' : 'nika';
  const bundled = path.join(context.extensionPath, 'server', bin);
  return fs.existsSync(bundled) ? bundled : null;
}
```

Discovery priority: config → bundled → PATH → cached → download.

**release.yml** — Replace single VSIX job with 6-entry matrix (5 platforms + universal).

### Verification
```bash
cd editors/vscode && npx vsce package --no-git-tag-version --target darwin-arm64
ls -la *.vsix  # Should be ~18MB
```

---

## Phase 2: DaemonProvider Trait

### Architecture

Types live in `nika-core/src/catalogs/lsp_types.rs`:
- `ProviderStatusInfo` (line 25-39)
- `WorkflowRunInfo` (line 42-58)
- `DaemonCapabilities` (line 61-77)
- `CostEstimate` in `cost.rs` (line 17-24)

### Trait Definition (new file: `tools/nika-daemon/src/provider.rs`)

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

### IpcDaemonProvider (new file: `tools/nika-daemon/src/provider_ipc.rs`)
Wraps existing `ConnectedClient`. Extracts caching logic from `daemon_bridge.rs`.

### InProcessDaemonProvider (new file: `tools/nika-daemon/src/provider_embedded.rs`)
Directly calls services. Key details:
- `CacheService`: already `Clone` via `Arc<CacheInner>`, lock-free DashMap
- `SecretService`: NOT Clone, wrap in Arc. **GOTCHA**: Use `~/.nika/secrets/` path (same as engine), NOT `~/.nika/daemon/secrets/`
- `JobService`: already `Clone`. Uses `std::env::current_exe()` — **GOTCHA**: returns `nika-lsp`, not `nika`. Pass explicit `nika_bin: Option<PathBuf>`.
- `EventBus`: NOT Clone, wrap in Arc. `broadcast::Sender` is lock-free.
- `estimate_cost()`: Call `nika_core::catalogs::estimate_cost()` directly (no daemon needed!)

### Backend Refactor
Change `NikaBackend.daemon` from `Arc<RwLock<Option<DaemonBridge>>>` to `Arc<dyn DaemonProvider>`.

5 call sites in backend.rs to simplify:
- Line 68-73: Construction (spawn try_connect)
- Line 110-129: daemon_status() handler
- Line 364-370: completion() enrichment
- Line 432-441: hover() enrichment
- Line 76: reconnect loop

### main.rs
Add `--embedded-daemon` flag:
```rust
let embedded = std::env::args().any(|a| a == "--embedded-daemon")
    || std::env::var("NIKA_LSP_EMBEDDED_DAEMON").is_ok();
```

### Contract Tests
```rust
async fn verify_provider_contract(p: &dyn DaemonProvider) {
    let _ = p.is_connected();
    assert!(p.workflow_history("nonexistent").await.is_empty());
    assert!(p.estimate_cost("fake", "fake", 0, 0).await.is_none());
    if p.is_connected() {
        assert!(p.capabilities().await.is_some());
    }
}
```

### Verification
```bash
cd tools && cargo test --lib -p nika-daemon -- provider
cd tools && cargo test --lib -p nika-lsp
cd tools && cargo test -p nika-lsp --test e2e_harness -- --ignored
```

---

## Phase 3: Cursor/Windsurf Auto-Config

### IDE Detection
```typescript
function isCursor(): boolean {
  return env.appName === 'Cursor' || env.uriScheme === 'cursor';
}
```

### Auto-generate on activation (after LSP starts)
- `.cursor/mcp.json` with `mcpServers` key
- `.cursor/rules/nika.mdc` with schema + verbs + rules
- `.vscode/mcp.json` with `servers` key (DIFFERENT from Cursor!)

### Don't overwrite existing files
Check with `workspace.fs.stat()` before creating.

---

## Phase 4: DAG Webview

### Tech Stack
- **ELK.js** (`elkjs/lib/elk.bundled.js`): Sugiyama layered layout, direction DOWN
- **D3.js** (selective: d3-selection, d3-zoom, d3-shape, d3-transition): SVG rendering
- **esbuild**: Dual target (Node.js extension + browser webview)

### Files to Create
- `editors/vscode/esbuild.mjs` — Dual target config
- `editors/vscode/src/dagPanel.ts` — WebviewPanel with CSP nonce
- `editors/vscode/src/webview/dag.ts` — ELK layout + D3 rendering
- `editors/vscode/src/webview/dag.css` — Theme-adaptive styles

### LSP Custom Request
Add `nika/workflowGraph` to backend.rs — returns `{ nodes, edges }` JSON.
Register in main.rs alongside `nika/daemonStatus`.

### Scaffold exists
Full working scaffold was created by the DAG webview agent at `tools/nika-vscode/`.
Copy and adapt to `editors/vscode/`.

### Verb Colors
- infer: `#7c3aed` (purple)
- exec: `#16a34a` (green)
- fetch: `#0891b2` (cyan)
- invoke: `#db2778` (pink)
- agent: `#d97306` (orange)

### Status Animations (CSS only)
- running: blue glow + pulse + spinner
- success: green + flash
- failed: red + blink
- skipped: dashed border, 50% opacity

---

## Phase 5: Sidebar Tree View

### package.json additions
```json
"viewsContainers": { "activitybar": [{ "id": "nika", "title": "Nika", "icon": "./icons/nika-dark.svg" }] },
"views": { "nika": [{ "id": "nikaWorkflows", "name": "Workflows" }, { "id": "nikaProviders", "name": "Providers" }] }
```

### WorkflowTreeProvider
- Root: `workspace.findFiles('**/*.nika.yaml')`
- Children: regex `/^\s*-\s*id:\s*(\S+)/gm` on file content
- Click: `vscode.open` with selection range
- Refresh: `FileSystemWatcher` on `**/*.nika.yaml`

---

## Phase 7: MCP Expansion

### Current State (4 tools, zero prompts)
- `nika_check`, `nika_list_workflows`, `nika_schema`, `nika_error_lookup`

### Add to server capabilities
```rust
ServerCapabilities::builder()
    .enable_tools()
    .enable_prompts()  // NEW
    .build()
```

### New Tools
- `nika_generate_task`: description → valid YAML task
- `nika_dag_visualization`: workflow path → Mermaid DAG
- `nika_error_fix`: NIKA-XXX + content → corrected YAML

### New Prompts
- `create-workflow`: scaffold from description
- `add-task`: single task from description
- `fix-error`: fix from error code

### Event Streaming (Task 7.3)
LSP subscribes to EventBus, emits `nika/executionEvent` notifications.
Extension forwards to webview via `postMessage`.

---

## Final Verification Checklist

### Automated (run ALL before claiming done)
```bash
# Rust
cd tools && cargo test --workspace --lib
cargo tree -p nika-lsp --no-dedupe | grep -c "nika-engine"  # 0
cargo build -p nika-lsp
cargo test -p nika-lsp --test e2e_harness -- --ignored
cargo test --lib -p nika-mcp
cargo test --lib -p nika-daemon -- provider

# TypeScript
cd editors/vscode && npm ci && npm run compile
npm test
npx vsce package --no-git-tag-version --target darwin-arm64
test -f out/webview/dag.js
```

### Manual
- [ ] VS Code: install, code lens visible, inlay hints visible
- [ ] Cursor: install, NO "command not found"
- [ ] Cursor: .cursor/mcp.json auto-generated
- [ ] Click code lens → terminal runs nika
- [ ] Show DAG → webview renders, click jumps to source
- [ ] Sidebar tree → workflows and tasks listed
- [ ] Restart server → preserves binary path
- [ ] Status bar: "Nika: Ready"

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
