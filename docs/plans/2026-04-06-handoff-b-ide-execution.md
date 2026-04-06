# Handoff B: IDE / VSCode Extension Execution

> **Copy-paste this into a new Claude Code session to execute.**
> Estimated time: 12-15 days | Tests baseline: 10,315
> **Prereq**: Handoff A (LSP decoupling) for Phases 1-7. Phase 0 can start NOW.

## Mission

Transform Nika VS Code extension from broken 32KB shell into first-class IDE.
8 phases, 27 tasks. Two extensions merged into one.

```
BEFORE: Install -> hope binary downloads -> basic text editing
AFTER:  Install (18MB) -> everything works -> code lens -> inlay hints
        -> DAG panel -> sidebar tree -> Cursor AI writes valid .nika.yaml
```

## Context

- Extension at `editors/vscode/` is v0.51.0 (published, 759 lines, 8 bugs)
- DAG scaffold at `tools/nika-vscode/` (POC with ELK.js + D3.js, hardcoded demo)
- LSP backend is excellent: 13 handlers, 745+ tests in nika-lsp-core
- 6 features coded but INVISIBLE (not in package.json)
- Model/provider refactor DONE — slash syntax, endpoints in nika.toml, no base_url

## Pre-Flight

```bash
cd /Users/thibaut/dev/supernovae/nika
git status  # Should be clean
cd tools && cargo test --workspace --lib  # 10,315+ tests
cd editors/vscode && npm ci && npm run compile
```

## Mandatory Skills

- `test-driven-development` — every task
- `verification-before-completion` — before "done"
- `spn-rust:rust-core` — Rust code
- `spn-rust:rust-async` — DaemonProvider (Phase 2)
- `executing-plans` — batch execution

## Documents to Read First

1. `docs/plans/2026-04-06-plan-1-ide-vscode-mega-execution.md` — Full plan (27 tasks)
2. `docs/research/2026-04-06-vscode-platform-binary-bundling.md` — rust-analyzer pattern
3. `docs/research/2026-04-06-vscode-dag-webview-research.md` — ELK.js + D3.js
4. `docs/research/2026-04-06-mcp-integration-cursor-windsurf-research.md` — Cross-editor MCP
5. `docs/research/2026-04-06-vscode-extension-tdd-research.md` — 4-layer testing

## Execution Order

```
Phase 0 (30 min) --- 8 bug fixes + activate 6 hidden features
     |
     +---> Phase 6 (1 day) --- SKIP IF HANDOFF A DONE
     |
     +---> Phase 1 (1 day) --- Platform-specific VSIX
     |         |
     |         +---> Phase 3 (1-2 days) --- Cursor/Windsurf auto-config
     |
     +---> Phase 2 (2-3 days) --- DaemonProvider trait + embedded daemon
     |         |
     |         +---> Phase 4 (3-5 days) --- DAG webview
     |         |
     |         +---> Phase 5 (2 days) --- Sidebar tree view
     |
     +---> Phase 7 (2 days) --- MCP expansion (independent)
```

## Phase 0: Bug Fixes (30 min — START HERE)

### Bug 1: Command Registration Race (CRITICAL)
**File**: `editors/vscode/src/extension.ts`
Move all 5 `commands.registerCommand` blocks (around lines 682-750) to BEFORE
the async `execFile` call (around line 636). Place right after `nika.showOutput` registration.

### Bug 2: StatusBarAlignment
Replace `@ts-ignore` magic number `1` with `StatusBarAlignment.Left` enum.
Add `StatusBarAlignment` to imports.

### Bug 3: Code Lens Arguments
Change command handlers to accept optional `Uri`:
```typescript
commands.registerCommand('nika.runWorkflow', (uri?: Uri) => {
  const filePath = uri?.fsPath ?? window.activeTextEditor?.document.fileName;
```

### Bug 4: restartServer Binary Path
Add module-scope `let resolvedBinaryPath: string | null = null;`.
Store when resolved. Use in restartServer.

### Bug 5: Status Interval
Add module-scope interval ref. Clear before creating new setInterval.

### Bug 6: Cursor installExtension
Add fallback to `env.openExternal(Uri.parse('marketplace-url'))`.

### Bug 7: Path Escaping
Escape `\`, `"`, `$` in file paths before terminal interpolation.

### Bug 8: Hidden Features
See Handoff A Phase 4 — add to package.json.

**8 commits, one per bug.**

## Phase 1: Binary Bundling (1 day)

### Task 1.1: findBundledBinary()
```typescript
function findBundledBinary(context: ExtensionContext): string | null {
  const bin = process.platform === 'win32' ? 'nika.exe' : 'nika';
  const bundled = path.join(context.extensionPath, 'server', bin);
  return fs.existsSync(bundled) ? bundled : null;
}
```

Priority: config > bundled > PATH > cached > download.

### Task 1.2: CI Matrix
6 platform VSIX + 1 universal. See Plan 1 for full matrix.

## Phase 2: DaemonProvider (2-3 days)

### Critical Gotchas
- SecretService: NOT Clone → Arc
- CacheService: IS Clone
- JobService: IS Clone BUT current_exe() returns "nika-lsp" not "nika" → pass nika_bin
- EventBus: NOT Clone → Arc
- NikaVault: Use `~/.nika/secrets/` NOT `~/.nika/daemon/secrets/`
- estimate_cost(): Call nika_core directly, no daemon needed

### Trait
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

Types in `nika-core/src/catalogs/lsp_types.rs`.

### 5 tasks (see Plan 1 for details)

## Phase 3: Cursor Auto-Config (1-2 days)
- Detect IDE: `env.appName === 'Cursor'`
- Auto-generate `.cursor/mcp.json` (mcpServers key)
- Auto-generate `.vscode/mcp.json` (servers key — DIFFERENT!)
- Don't overwrite existing files

## Phase 4: DAG Webview (3-5 days)
- Copy scaffold from `tools/nika-vscode/` to `editors/vscode/`
- Wire to real workflow data via `nika/workflowGraph` LSP request
- ELK.js layout + D3.js rendering
- Verb colors: infer=#7c3aed, exec=#16a34a, fetch=#0891b2, invoke=#db2778, agent=#d97306
- CSP: nonce for scripts, `style-src 'unsafe-inline'` for D3 transforms

## Phase 5: Sidebar Tree (2 days)
- Activity bar icon + 2 views (Workflows, Providers)
- `workspace.findFiles('**/*.nika.yaml')` + regex for task IDs
- FileSystemWatcher for auto-refresh

## Phase 7: MCP Expansion (2 days)
- Enable prompts: `ServerCapabilities::builder().enable_prompts()`
- 3 new tools: generate_task, dag_visualization, error_fix
- 3 prompts: create-workflow, add-task, fix-error
- Event streaming: EventBus → LSP notification → webview postMessage

## Guardrails (after EVERY phase)

```bash
cd tools && cargo test --workspace --lib
cd editors/vscode && npm ci && npm run compile
cargo tree -p nika-lsp --no-dedupe | grep -c "nika-engine"  # 0 (after Phase 6)
```

## Commit Convention

```
type(scope): description

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

Scopes: `vscode`, `lsp`, `daemon`, `mcp`, `ci`
**NEVER Claude/Anthropic co-author. ALWAYS Nika.**
