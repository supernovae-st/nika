# Nika IDE — Complete Extension Redesign Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Transform the Nika VS Code extension from a thin LSP wrapper into a first-class IDE experience — zero-friction install, embedded daemon, interactive DAG visualization, and native Cursor/Windsurf integration.

**Architecture:** 6 phases building on each other. Phase 0 fixes critical bugs. Phase 1 bundles the binary (like rust-analyzer). Phase 2 unifies LSP+daemon via `DaemonProvider` trait. Phase 3 adds Cursor/Windsurf auto-config. Phase 4 adds interactive DAG webview. Phase 5 adds sidebar tree view.

**Tech Stack:** Rust (tower-lsp, tokio, DashMap), TypeScript (vscode-languageclient), D3.js + ELK.js (webview), esbuild (bundling)

**Research Reports (read before implementing):**
- `docs/research/2026-04-06-vscode-platform-binary-bundling.md` — rust-analyzer pattern
- `docs/research/2026-04-06-vscode-dag-webview-research.md` — webview patterns
- `docs/research/2026-04-06-mcp-integration-cursor-windsurf-research.md` — MCP across editors

---

## Philosophy: The v0 Nuke

Nika has zero users on the extension. Zero backward compatibility debt. This is the moment to get it RIGHT — not incrementally patch a broken thin wrapper, but redesign from first principles.

**The problem today:**

The Nika LSP server is genuinely excellent — 3,400+ tests, 13 fully-implemented handlers, 16-variant context-aware cursor detection, 5-phase validation pipeline. But the user sees NONE of this because the VS Code extension is a 32KB shell that:

1. Breaks on Cursor (commands registered after async probe — race condition)
2. Ignores 6 out of 13 LSP capabilities (code lens, inlay hints, semantic tokens, document links, folding, rename — all CODED but not declared in package.json)
3. Requires a separate daemon process that users never start
4. Downloads a binary from GitHub that fails on half the platforms
5. Has zero visual feedback — no DAG view, no sidebar, no task tree
6. Doesn't integrate with AI editors (Cursor, Windsurf, Claude Code) despite Nika being an AI workflow engine

**What we're building:**

```
BEFORE:
  Install extension → hope binary downloads → hope daemon starts → text editing with basic highlighting

AFTER:
  Install extension → everything works → code lens with run/validate → inlay hints with costs
  → interactive DAG panel → sidebar workflow tree → Cursor AI writes valid .nika.yaml
```

**The 3 principles:**

1. **Zero config**: Install the extension, open a `.nika.yaml` file. Everything works. No PATH, no daemon, no download, no setup. The binary is embedded (like rust-analyzer). The daemon runs in-process.

2. **Intelligence surfaced**: Every LSP feature that exists in the Rust backend must be visible in the extension. The LSP has 13 handlers — the extension must activate all 13. Code lens shows run status. Inlay hints show costs. Semantic tokens color verbs.

3. **AI-native**: Nika is an AI workflow engine running inside AI editors. The extension should feed schema knowledge to Cursor's AI, auto-configure MCP tools, and generate `.cursor/rules/nika.mdc` so the AI can write valid workflows.

---

## What the 7-Agent Audit Found

### Agent 1: Gap Analysis (LSP vs Extension)

**6 features coded but invisible:**

| LSP Capability | Implemented in Rust | Declared in package.json | Status |
|---|---|---|---|
| Code Lens (4 types: Run, Validate, TaskCount, LastRun) | backend.rs:235-237 | MISSING | **BROKEN** |
| Inlay Hints (timeout, costs, bindings, deps) | backend.rs:239 | MISSING | **BROKEN** |
| Semantic Tokens (7 types, 2 modifiers) | backend.rs:210-229 | MISSING | **BROKEN** |
| Document Links (URLs, file paths) | backend.rs:248-251 | MISSING | **BROKEN** |
| Folding Ranges | backend.rs:253 | MISSING | **BROKEN** |
| Rename (with prepare) | backend.rs:243-246 | No keybinding | Degraded |

The LSP server advertises all these in `ServerCapabilities`. The client ignores them.

### Agent 2: Code Review (8 Bugs)

| # | Bug | Severity | Root Cause |
|---|-----|----------|------------|
| 1 | `nika.runWorkflow not found` on Cursor | CRITICAL | Commands at L682 registered after async `execFile` at L636 |
| 2 | `@ts-ignore` magic number for StatusBarAlignment | HIGH | Uses `1` instead of `StatusBarAlignment.Left` enum |
| 3 | Code Lens arguments always null | HIGH | No file URI passed → click fails when editor loses focus |
| 4 | restartServer loses binary path | MEDIUM | `resolvedServerPath` not persisted across restart |
| 5 | Status interval accumulates | MEDIUM | setInterval never cleared on restart |
| 6 | `workbench.extensions.installExtension` fails on Cursor | MEDIUM | Cursor-only command, no fallback |
| 7 | Path escaping in terminal commands | MEDIUM | `"` and `$` not escaped |
| 8 | No Code Lens/Inlay Hints config in package.json | HIGH | Features invisible to users |

### Agent 3: Rust Architect (DaemonProvider Design)

**Recommended architecture:** `DaemonProvider` trait with two impls.

```rust
// The trait — 5 methods, async, Send + Sync
#[async_trait]
pub trait DaemonProvider: Send + Sync + 'static {
    fn is_connected(&self) -> bool;
    async fn provider_status(&self) -> Vec<ProviderStatusInfo>;
    async fn estimate_cost(&self, ...) -> Option<CostEstimate>;
    async fn workflow_history(&self, workflow: &str) -> Vec<WorkflowRunInfo>;
    async fn capabilities(&self) -> Option<DaemonCapabilities>;
}

// Impl 1: IPC — wraps existing DaemonBridge (Unix socket to standalone daemon)
// Impl 2: InProcess — runs SecretService, CacheService, JobService, EventBus directly
```

**Why this is better:**
- `NikaBackend.daemon` goes from `Arc<RwLock<Option<DaemonBridge>>>` (3 levels of unwrapping) to `Arc<dyn DaemonProvider>` (direct method calls)
- Embedded mode: `fn is_connected(&self) -> bool { true }` — we ARE the daemon
- Windows support as a free bonus (no Unix socket needed in embedded mode)
- Testing: trait-based contract tests verify both modes behave identically
- Net LOC: +450 new, -280 deleted = +170 net

**Thread safety verified:** All daemon services already use `DashMap` (lock-free concurrent map), `Arc<Mutex<>>`, `broadcast::Sender` (lock-free). Running them in-process alongside tower-lsp introduces zero new concurrency concerns.

**Risk: SQLite contention.** If standalone daemon AND embedded LSP both open `jobs.db` → WAL handles concurrent readers but only one writer. Mitigation: detect running daemon and skip embedded boot, or use separate DB path.

**Risk: `current_exe()`.** `JobService::start_job()` uses `current_exe()` which returns `nika-lsp` not `nika`. Fix: accept explicit `nika_bin: Option<PathBuf>`, default to `which::which("nika")`.

### Agent 4: rust-analyzer Research

**Key findings:**
- rust-analyzer is THE only major extension bundling binaries. Deno, Go, clangd all download at runtime.
- Pattern: `cargo xtask dist` → copy binary to `editors/code/server/` → `vsce package --target darwin-arm64`
- `.vscodeignore` uses deny-all: `**` then `!server` to include only the binary
- Size: 15-20 MB per platform .vsix. rust-analyzer is 17-20 MB. This is normal.
- Always publish a "no-server" universal fallback (0.88 MB) for unsupported platforms
- Publish to BOTH VS Code Marketplace + Open VSX with `vsce publish --packagePath` + `ovsx publish --packagePath`
- Discovery chain: explicit config → bundled binary → PATH → error with install prompt

### Agent 5: Cursor Deep Dive

**Key findings:**
- **No public Cursor API.** No `cursor.` namespace. No extension point for AI features.
- **MCP is THE integration path.** Cursor's agent mode calls MCP tools from `.cursor/mcp.json`.
- **Detection:** `vscode.env.appName === 'Cursor'` or `vscode.env.uriScheme === 'cursor'`
- **PATH broken on Cursor.** Extensions that spawn binaries via child_process may fail. MUST use absolute paths or bundled binary.
- **`.cursor/rules/*.mdc`** with frontmatter (globs, alwaysApply) — primary way to teach Cursor about Nika
- **Diagnostic-AI feedback loop:** When LSP reports errors, Cursor AI sees them and suggests fixes. This is the tight loop: user writes YAML → LSP validates → errors appear → AI suggests fixes → loop.
- **Code Actions amplified:** Cursor AI can suggest applying Code Actions. Every diagnostic with a quick fix becomes AI-assisted.

### Agent 6: MCP Cross-Editor Research

**All 4 editors support MCP but with different configs:**

| Editor | Config File | Top Key | Resources | Instructions |
|--------|-------------|---------|-----------|-------------|
| Cursor | `.cursor/mcp.json` | `mcpServers` | No | No |
| Windsurf | `~/.codeium/windsurf/mcp_config.json` | `mcpServers` | Yes | No |
| VS Code | `.vscode/mcp.json` | `servers` (DIFFERENT!) | Yes | No |
| Claude Code | `.mcp.json` | `mcpServers` | Yes | Yes |

**VS Code Agent Plugins** — the new big opportunity:
- Bundle MCP server + skills + agents + hooks in a single installable package
- `plugin.json` + `skills/nika/SKILL.md` + `agents/nika-expert.agent.md`
- When enabled, MCP server starts automatically

### Agent 7: DAG Webview Research

**Recommended stack: ELK.js (layout) + D3.js (rendering)**
- ELK.js: best Sugiyama layered algorithm, `elk.direction: DOWN` for pipeline DAGs, ~150KB
- D3.js: full SVG control, click handlers, zoom/pan, ~250KB
- Total: ~400KB, zero framework dependency
- SVG over Canvas: Nika DAGs are 5-50 nodes, SVG is perfect
- `retainContextWhenHidden: true` is CRITICAL for webview persistence
- CSP with nonce, no `unsafe-inline` or `unsafe-eval`
- Click-to-source: `vscode.window.showTextDocument(uri, { selection: range })`
- State persistence: `acquireVsCodeApi().getState()`/`setState()` for zoom/pan

---

## Crate Architecture: Everything Connected

### Current Dependency Graph (11 agents confirmed)

```
                    nika-vault (leaf — XChaCha20Poly1305)
                        │
           ┌────────────┤
           │             │
       nika-core      nika-daemon ─── nika-storage (SQLite)
       (types, AST,   (services, IPC,
        catalogs,      EventBus)
        source spans)     │
           │              │
       nika-event      nika-mcp ─── nika-media
       (58 EventKind,  (4 tools,    (CAS, blake3)
        EventLog,       rmcp SDK)
        broadcast)        │
           │              │
       nika-display    nika-lsp-core (13 handlers, 3400 tests)
           │              │
       nika-engine ◄──────┘ (re-exports nika-core types)
       (runtime, DAG,     │
        providers,      nika-lsp ←── DEPENDS ON ENGINE (WRONG!)
        157K LOC)         │
           │              │
    ┌──────┼──────┐       │
    cli   tui   serve    sdk
    └──────┴──────┴───────┘
              │
           nika (binary)
```

### Problem: nika-lsp depends on nika-engine for NO reason

The crate audit revealed that **every import** nika-lsp uses from nika-engine is a re-export from nika-core:

| nika-lsp imports from nika-engine | Actually lives in |
|---|---|
| `ast::raw::parse`, `RawWorkflow` | nika-core |
| `ast::analyzed::AnalyzedWorkflow` | nika-core |
| `ast::analyzer::analyze` | nika-core |
| `source::{SourceRegistry, FileId, Span}` | nika-core |

**Impact**: nika-lsp pulls in reqwest, rig-core, petgraph, and the full 157K LOC runtime. Compile: ~90s. Should be: ~15s.

### Target Architecture (after this plan)

```
                    nika-vault (leaf)
                        │
           ┌────────────┼────────┐
           │            │        │
       nika-core     nika-daemon  nika-storage
       (types, AST,  (DaemonProvider
        DaemonProvider  impls: IPC +
        TRAIT)          InProcess)
           │               │
       nika-event ─────────┘
       (unified EventBus,
        58 EventKind +
        DaemonEvent merged)
           │
    ┌──────┼──────────────────┐
    │      │                  │
nika-engine  nika-lsp         nika-mcp
(runtime)    (NO engine dep!) (7+ tools)
    │        │                │
    │     nika-lsp-core       │
    │        │                │
 ┌──┼────┬───┼───┬────────────┘
 cli tui serve  sdk
 └──┴────┴───┴───┘
         │
      nika (binary: wires DaemonClient, selects platform)
```

**Key changes:**
1. **R1**: nika-lsp drops nika-engine → compile 15s instead of 90s
2. **R2**: DaemonProvider trait in nika-core (sync), impls in nika-daemon
3. **R3**: Event bus unified in nika-event (engine events + daemon events)
4. **R4**: `cfg(unix)` removed from library crates → lives only in binary
5. **R5**: nika-mcp expanded from 4 to 7+ tools

### How Features Propagate (Guardrail)

When adding a new feature, check this table:

| Feature | Crates Changed | Verification |
|---|---|---|
| New LSP handler | nika-lsp-core only | `cargo test --lib -p nika-lsp-core` |
| New builtin tool | nika-engine only | `cargo test --lib -p nika-engine` |
| New event type | nika-event → consumers | `cargo test --lib -p nika-event` |
| New MCP tool | nika-mcp only | `cargo test --lib -p nika-mcp` |
| New daemon service | nika-daemon only | `cargo test --lib -p nika-daemon` |
| New provider | nika-engine only | `cargo test --lib -p nika-engine` |
| Extension UI change | editors/vscode only | `npm run compile && npm test` |
| Live DAG update | nika-event + nika-lsp + extension | Full E2E harness |

**Rule**: If a feature change touches more than 2 crates, the architecture is wrong. Fix the boundaries first.

---

## MCP Server Evolution: From 4 Tools to Full AI Partner

### Current State (4 tools, minimal)

| Tool | Quality | Gap |
|---|---|---|
| `nika_check` | Good — subprocess, path validation | Output is raw text, not structured |
| `nika_list_workflows` | Good — recursive scan, limits | No project context |
| `nika_schema` | Good — 277-line reference | Hardcoded, no per-topic help |
| `nika_error_lookup` | Basic — category only | No fix suggestions |

### Missing: Zero MCP Prompts, Zero MCP Resources

The server doesn't even call `enable_prompts()` in capabilities. Cursor can't use prompt templates.

### Target State (7+ tools, Prompts, Resources)

**New tools to add (Phase 3.5):**

| Tool | Purpose | Impact |
|---|---|---|
| `nika_generate_task` | Describe → valid YAML task | AI generates correct tasks |
| `nika_dag_visualization` | Return Mermaid DAG | AI understands workflow structure |
| `nika_error_fix` | NIKA-XXX → suggested code fix | AI auto-repairs workflows |
| `nika_project_summary` | List workflows + describe project | AI has full project context |
| `nika_tool_info` | Full docs for a specific builtin | AI uses correct tool params |

**MCP Prompts to add:**

| Prompt | Template |
|---|---|
| `create-workflow` | "Create a workflow that {description}" → scaffold |
| `add-task` | "Add a {verb} task that {description}" → task YAML |
| `fix-error` | "Fix error {code} in {file}" → corrected YAML |

**MCP Resources to add (for VS Code/Windsurf — Cursor doesn't support):**

| Resource URI | Content |
|---|---|
| `nika://schema/verbs` | 5 verbs with full docs |
| `nika://schema/transforms` | 63 transforms reference |
| `nika://schema/tools` | 62 builtin tools reference |

---

## Event Pipeline: Engine → Webview (Live DAG)

### Current Flow

```
Engine TaskEventGuard ──emit──→ EventLog (Vec + broadcast)
                                    │
                                    ├──→ TUI (poll_runtime_events)
                                    │
                                    └──→ NDJSON trace file

Daemon JobService ──publish──→ EventBus (broadcast)
                                    │
                                    ├──→ EventSubscribe clients (CLI)
                                    │
                                    └──→ (LSP DOES NOT SUBSCRIBE)
```

### Target Flow (after Phase 4)

```
Engine TaskEventGuard ──emit──→ EventLog (broadcast)
                                    │
                                    ├──→ TUI (existing)
                                    │
                                    └──→ InProcessDaemonProvider
                                              │
                                              ├──→ EventBus (unified)
                                              │         │
                                              │         └──→ LSP NikaBackend
                                              │                   │
                                              │              nika/executionEvent
                                              │              (LSP notification)
                                              │                   │
                                              │              Extension handler
                                              │                   │
                                              │              webview.postMessage()
                                              │                   │
                                              │              D3.js node color update
                                              │              (300ms transition)
                                              │
                                              └──→ IPC clients (CLI, standalone)
```

**Key design**: The `InProcessDaemonProvider` subscribes to `EventLog::subscribe()` and forwards events through the unified `EventBus`. The LSP spawns a background task that subscribes and emits `nika/executionEvent` LSP notifications. The extension listens and forwards to webview.

---

## Guardrails & Verification Strategy

### Rust-Side Guardrails

**1. Trait contract tests (DaemonProvider)**

Both IPC and InProcess impls must pass the SAME test suite:

```rust
async fn verify_provider_contract(p: &dyn DaemonProvider) {
    // Always returns valid state
    let _ = p.is_connected();
    
    // Never panics on unknown workflow
    assert!(p.workflow_history("nonexistent").await.is_empty());
    
    // Cost estimate returns None for unknown model (not panic)
    assert!(p.estimate_cost("fake", "fake", 0, 0).await.is_none());
    
    // Capabilities is always Some when connected
    if p.is_connected() {
        assert!(p.capabilities().await.is_some());
    }
}
```

**2. E2E harness extensions**

Add to `e2e_harness.rs` (already has 17 scenarios):

```rust
#[test]
#[ignore]
fn test_code_lens_has_run_command_with_uri_argument() {
    // Verify code lens arguments contain the document URI
}

#[test]
#[ignore]
fn test_inlay_hints_contain_cost_for_model() {
    // Verify model cost hint appears
}

#[test]
#[ignore]
fn test_embedded_daemon_status_connected() {
    // Start with --embedded-daemon, verify nika/daemonStatus returns connected: true
}

#[test]
#[ignore]
fn test_workflow_graph_custom_request() {
    // Send nika/workflowGraph, verify nodes and edges
}
```

**3. Compilation guardrail**

CI check that nika-lsp does NOT depend on nika-engine:

```bash
# In CI pipeline
cargo tree -p nika-lsp --no-dedupe | grep -q "nika-engine" && \
  echo "FAIL: nika-lsp must not depend on nika-engine" && exit 1
```

### TypeScript-Side Guardrails

**4. Extension smoke test (new)**

Create `editors/vscode/src/test/smoke.test.ts`:

```typescript
// Verify all commands are registered
test('all commands registered', async () => {
  const commands = await vscode.commands.getCommands(true);
  const nikaCommands = ['nika.runWorkflow', 'nika.checkWorkflow', 
    'nika.newWorkflow', 'nika.showTasks', 'nika.restartServer',
    'nika.showOutput', 'nika.showDag'];
  for (const cmd of nikaCommands) {
    assert(commands.includes(cmd), `Missing command: ${cmd}`);
  }
});

// Verify bundled binary exists
test('bundled binary exists', () => {
  const ext = process.platform === 'win32' ? '.exe' : '';
  const binary = path.join(__dirname, '..', '..', 'server', `nika${ext}`);
  // Only check if platform-specific build
  if (fs.existsSync(path.join(__dirname, '..', '..', 'server'))) {
    assert(fs.existsSync(binary), 'Bundled nika binary missing');
  }
});
```

**5. MCP config guardrail**

After Cursor auto-config, verify the generated JSON is valid:

```typescript
test('cursor mcp config is valid JSON', async () => {
  const config = JSON.parse(fs.readFileSync('.cursor/mcp.json', 'utf-8'));
  assert(config.mcpServers.nika.command);
  assert(Array.isArray(config.mcpServers.nika.args));
});
```

### End-to-End Verification Checklist

After ALL phases complete, run this sequence:

```bash
# 1. Rust tests (all crates)
cd tools && cargo test --workspace --lib

# 2. Verify nika-lsp has no engine dependency
cargo tree -p nika-lsp --no-dedupe | grep -c "nika-engine"  # Must be 0

# 3. E2E LSP harness
cargo build -p nika-lsp
cargo test -p nika-lsp --test e2e_harness -- --ignored

# 4. MCP server tests
cargo test --lib -p nika-mcp

# 5. Extension compilation
cd editors/vscode && npm ci && npm run compile

# 6. Extension smoke tests
cd editors/vscode && npm test

# 7. Platform VSIX build (dry run)
npx vsce package --no-git-tag-version --target darwin-arm64

# 8. Verify webview builds
ls -la out/webview/dag.js  # Must exist

# 9. Manual: open extension on Cursor
#    - Code lens visible? ▶ Run, ✓ Validate, N tasks
#    - Inlay hints visible? timeout (10s), model ($3/$15)
#    - Click code lens → terminal runs nika?
#    - Show DAG → webview renders?
#    - Sidebar tree → workflows listed?
#    - .cursor/mcp.json generated?
```

---

## Phase 0: Critical Bug Fixes + Activate Hidden Features (30 min)

8 bugs to fix, 6 LSP capabilities to activate. All in `editors/vscode/`.

---

### Task 0.1: Fix Command Registration Race Condition

**Root cause:** Commands are registered at lines 682-750, AFTER the async `execFile` call at line 636. On Cursor, the Code Lens fires before commands exist → `nika.runWorkflow not found`.

**Files:**
- Modify: `editors/vscode/src/extension.ts:585-750`

**Step 1: Move all command registrations to the TOP of `activate()`, before any async work**

In `extension.ts`, restructure `activate()` so that all 5 `commands.registerCommand` calls (lines 682-750) move to right after line 607 (after `nika.showOutput` registration), BEFORE the `execFile` call at line 636.

The commands `nika.runWorkflow`, `nika.checkWorkflow`, `nika.newWorkflow`, `nika.showTasks`, `nika.restartServer` must all be registered synchronously before any async binary probe.

**Step 2: Verify on Cursor**

Open a `.nika.yaml` file on Cursor. Right-click → "Nika: Run Current Workflow" should appear and work even if the binary isn't found yet.

**Step 3: Commit**

```bash
git add editors/vscode/src/extension.ts
git commit -m "fix(vscode): register commands before async binary probe

Fixes 'nika.runWorkflow not found' on Cursor. Commands must be
registered synchronously in activate() before execFile callback."
```

---

### Task 0.2: Fix StatusBarAlignment @ts-ignore

**Files:**
- Modify: `editors/vscode/src/extension.ts:1,591-595`

**Step 1: Import StatusBarAlignment and use the enum instead of magic number**

Line 1 — add `StatusBarAlignment` to the import:
```typescript
import {
  workspace,
  commands,
  ExtensionContext,
  window,
  Uri,
  ProgressLocation,
  env,
  StatusBarAlignment,
} from 'vscode';
```

Lines 591-595 — replace the `@ts-ignore` hack:
```typescript
statusBarItem = window.createStatusBarItem(StatusBarAlignment.Left, 100);
```

**Step 2: Commit**

```bash
git add editors/vscode/src/extension.ts
git commit -m "fix(vscode): use StatusBarAlignment enum instead of @ts-ignore magic number"
```

---

### Task 0.3: Fix Code Lens Arguments (pass file URI)

**Files:**
- Modify: `tools/nika-lsp/src/backend.rs` — where `Command` is built for code lens
- Modify: `editors/vscode/src/extension.ts:684-703` — command handlers

**Step 1: In `backend.rs`, pass document URI as code lens command argument**

Find where `CodeLens` items are created (in the `code_lens` handler, around line 700). Change `arguments: None` to:

```rust
arguments: Some(vec![serde_json::json!(uri.as_str())]),
```

**Step 2: In `extension.ts`, accept optional URI argument in command handlers**

```typescript
commands.registerCommand('nika.runWorkflow', (fileUri?: Uri) => {
  const filePath = fileUri?.toString()
    ? Uri.parse(fileUri.toString()).fsPath
    : window.activeTextEditor?.document.fileName;
  if (!filePath?.endsWith('.nika.yaml')) {
    window.showWarningMessage('Open a .nika.yaml file first.');
    return;
  }
  runNikaCommand('run', filePath);
}),
```

Same pattern for `nika.checkWorkflow`.

**Step 3: Commit**

```bash
git add tools/nika-lsp/src/backend.rs editors/vscode/src/extension.ts
git commit -m "fix(vscode): pass document URI in code lens arguments

Code lens clicks now work even when focus shifts away from the editor."
```

---

### Task 0.4: Fix restartServer Losing Cached Binary Path

**Files:**
- Modify: `editors/vscode/src/extension.ts`

**Step 1: Store resolved binary path in module scope**

Add a module-level variable:
```typescript
let resolvedServerPath: string | undefined;
```

In `tryStartWithBinary()` (line 619), store it:
```typescript
const tryStartWithBinary = (binaryPath: string): void => {
  resolvedServerPath = binaryPath;
  startClient(context, binaryPath);
};
```

In `nika.restartServer` (line 742), use it:
```typescript
commands.registerCommand('nika.restartServer', async () => {
  if (client) {
    await client.stop();
    client = undefined;
  }
  startClient(context, resolvedServerPath);
  window.showInformationMessage('Nika language server restarted.');
}),
```

**Step 2: Commit**

```bash
git add editors/vscode/src/extension.ts
git commit -m "fix(vscode): preserve cached binary path across server restarts"
```

---

### Task 0.5: Fix Status Interval Accumulation on Restart

**Files:**
- Modify: `editors/vscode/src/extension.ts`

**Step 1: Store interval handle in module scope and clear before creating new**

Add module-level:
```typescript
let statusIntervalHandle: ReturnType<typeof setInterval> | undefined;
```

In `startClient()`, before `setInterval` (around line 489):
```typescript
if (statusIntervalHandle !== undefined) {
  clearInterval(statusIntervalHandle);
}
statusIntervalHandle = setInterval(async () => {
  // ... existing polling logic
}, 30000);
```

Remove the `context.subscriptions.push({ dispose: ... })` for the interval (line 510) since we now manage it explicitly.

**Step 2: Commit**

```bash
git add editors/vscode/src/extension.ts
git commit -m "fix(vscode): prevent status poll interval accumulation on LSP restart"
```

---

### Task 0.6: Fix Cursor-Incompatible Extension Update Command

**Files:**
- Modify: `editors/vscode/src/extension.ts:570-576`

**Step 1: Add fallback for Cursor**

```typescript
commands.executeCommand(
  'workbench.extensions.installExtension',
  'supernovae.nika-lang',
).then(undefined, () => {
  // Cursor / other hosts: open marketplace URL
  env.openExternal(Uri.parse(
    'https://marketplace.visualstudio.com/items?itemName=supernovae.nika-lang'
  ));
});
```

**Step 2: Commit**

```bash
git add editors/vscode/src/extension.ts
git commit -m "fix(vscode): fallback to marketplace URL for extension update on Cursor"
```

---

### Task 0.7: Fix runNikaCommand Path Escaping

**Files:**
- Modify: `editors/vscode/src/extension.ts:531-536`

**Step 1: Escape special characters in file path**

```typescript
function runNikaCommand(subcmd: string, filePath: string): void {
  const nika = resolvedServerPath ?? getNikaPath();
  const escaped = filePath.replace(/\\/g, '\\\\').replace(/"/g, '\\"');
  const terminal = window.createTerminal({ name: `Nika: ${subcmd}` });
  terminal.show();
  terminal.sendText(`"${nika}" ${subcmd} "${escaped}"`);
}
```

**Step 2: Commit**

```bash
git add editors/vscode/src/extension.ts
git commit -m "fix(vscode): escape file paths with special characters in terminal commands"
```

---

### Task 0.8: Activate All Hidden LSP Features in package.json

**Files:**
- Modify: `editors/vscode/package.json`

**Step 1: Add semantic token declarations to `contributes`**

After the `"snippets"` section (line 63), add:

```json
"semanticTokenScopes": [
  {
    "language": "nika",
    "scopes": {
      "keyword": ["keyword.control.nika"],
      "macro": ["entity.name.function.verb.nika"],
      "property": ["variable.other.property.nika"],
      "variable": ["variable.other.template.nika"],
      "function": ["entity.name.function.nika"],
      "type": ["entity.name.type.nika"],
      "comment": ["comment.nika"]
    }
  }
],
```

**Step 2: Add user configuration for hints**

In the `"properties"` section (after line 127), add:

```json
"nika.inlayHints.enabled": {
  "type": "boolean",
  "default": true,
  "description": "Show inlay hints (timeout annotations, model costs, binding labels)."
},
"nika.codeLens.enabled": {
  "type": "boolean",
  "default": true,
  "description": "Show code lens actions (Run, Validate, task count, last run status)."
}
```

**Step 3: Commit**

```bash
git add editors/vscode/package.json
git commit -m "feat(vscode): activate semantic tokens, inlay hints, and code lens in package.json

These features were implemented in the LSP server but not declared in
the extension manifest, making them invisible to users."
```

---

## Phase 1: Platform-Specific VSIX — Binary Bundling (1 day)

Following the rust-analyzer pattern: embed the compiled `nika` binary inside platform-specific `.vsix` files. Zero-friction install.

---

### Task 1.1: Add Binary Discovery for Bundled Server

**Files:**
- Modify: `editors/vscode/src/extension.ts`
- Modify: `editors/vscode/.vscodeignore`

**Step 1: Add bundled binary discovery function**

Add at top of `extension.ts`:

```typescript
import * as os from 'os';

function findBundledBinary(context: ExtensionContext): string | null {
  const binaryName = process.platform === 'win32' ? 'nika.exe' : 'nika';
  const bundled = path.join(context.extensionPath, 'server', binaryName);
  if (fs.existsSync(bundled)) {
    return bundled;
  }
  return null;
}

function isCursor(): boolean {
  return env.appName === 'Cursor' || env.uriScheme === 'cursor';
}
```

**Step 2: Update activation flow — bundled binary takes priority**

In `activate()`, replace the binary resolution logic (lines 636-680) with this priority chain:

```
1. nika.server.path setting (explicit override)
2. Bundled binary in extension/server/ (platform-specific VSIX)
3. nika in PATH
4. Cached binary in globalStorage
5. Auto-download from GitHub (fallback)
```

```typescript
// 1. Check explicit setting
const configPath = getNikaPath();
if (configPath !== 'nika') {
  // User set an explicit path — use it directly
  startClient(context, configPath);
  return;
}

// 2. Check bundled binary (platform-specific VSIX)
const bundled = findBundledBinary(context);
if (bundled) {
  log('INFO', `Using bundled binary: ${bundled}`);
  tryStartWithBinary(bundled);
  return;
}

// 3. Check PATH (existing logic from execFile)
// 4. Check cached binary (existing logic)
// 5. Auto-download (existing logic)
```

**Step 3: Update `.vscodeignore` to include `server/` directory**

```
.vscode/**
src/**
node_modules/**
tsconfig.json
**/*.ts
**/*.map
.gitignore
!server/
```

**Step 4: Commit**

```bash
git add editors/vscode/src/extension.ts editors/vscode/.vscodeignore
git commit -m "feat(vscode): support bundled binary in server/ directory

Platform-specific VSIX files will contain the nika binary in server/.
Falls back to PATH and auto-download if no bundled binary found."
```

---

### Task 1.2: Update CI Pipeline for Platform-Specific Extensions

**Files:**
- Modify: `.github/workflows/release.yml` — vscode-publish job (lines 809-904)

**Step 1: Replace single VSIX build with platform matrix**

Replace the `vscode-publish` job with a matrix strategy that:
1. Downloads the correct nika binary artifact for each platform
2. Places it in `editors/vscode/server/`
3. Runs `npx vsce package --no-git-tag-version --target <platform>`
4. Publishes all platform-specific `.vsix` files + one universal fallback

Platform matrix:
- `darwin-arm64` → artifact `nika-macos-arm64`
- `darwin-x64` → artifact `nika-macos-x64`
- `linux-x64` → artifact `nika-linux-x64`
- `linux-arm64` → artifact `nika-linux-arm64`
- `win32-x64` → artifact `nika-windows-x64`
- `universal` → no binary (fallback for unsupported platforms)

Key CI steps per matrix entry:
```yaml
- name: Download binary artifact
  if: matrix.artifact != ''
  uses: actions/download-artifact@v4
  with:
    name: ${{ matrix.artifact }}
    path: editors/vscode/server/

- name: Make binary executable
  if: matrix.artifact != '' && matrix.vscode-target != 'win32-x64'
  run: chmod +x editors/vscode/server/nika

- name: Package platform-specific VSIX
  working-directory: editors/vscode
  run: npx vsce package --no-git-tag-version --target ${{ matrix.vscode-target }}

- name: Publish to VS Code Marketplace
  working-directory: editors/vscode
  run: npx vsce publish --no-git-tag-version --packagePath *.vsix -p "$VSCE_PAT"

- name: Publish to Open VSX
  working-directory: editors/vscode
  run: npx ovsx publish --no-git-tag-version --packagePath *.vsix -p "$OVSX_PAT"
```

**Step 2: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "feat(ci): build platform-specific VSIX with bundled nika binary

Each platform gets its own .vsix with the compiled binary in server/.
Universal fallback VSIX without binary for unsupported platforms.
Publishes to both VS Code Marketplace and Open VSX."
```

---

## Phase 2: DaemonProvider Trait — LSP/Daemon Unification (2-3 days)

Merge daemon services into the LSP process via a trait abstraction. The LSP becomes self-sufficient — no separate daemon needed for IDE use.

---

### Task 2.1: Create DaemonProvider Trait

**Files:**
- Create: `tools/nika-daemon/src/provider.rs`
- Modify: `tools/nika-daemon/src/lib.rs`
- Modify: `tools/nika-daemon/Cargo.toml`

**Step 1: Write the trait test**

```rust
// tools/nika-daemon/src/provider.rs

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_provider_contract(provider: &dyn DaemonProvider) {
        // is_connected returns consistent state
        let _connected = provider.is_connected();

        // provider_status returns a vec
        let status = provider.provider_status().await;
        assert!(status.is_empty() || !status.is_empty()); // type check

        // workflow_history returns empty for unknown
        let history = provider.workflow_history("nonexistent.nika.yaml").await;
        assert!(history.is_empty());

        // capabilities returns Option
        let _caps = provider.capabilities().await;
    }
}
```

**Step 2: Implement the trait**

```rust
use async_trait::async_trait;
use nika_core::catalogs::{CostEstimate, DaemonCapabilities, ProviderStatusInfo, WorkflowRunInfo};

/// Abstraction over daemon services — IPC or in-process.
#[async_trait]
pub trait DaemonProvider: Send + Sync + 'static {
    fn is_connected(&self) -> bool;
    async fn provider_status(&self) -> Vec<ProviderStatusInfo>;
    async fn estimate_cost(
        &self, provider: &str, model: &str, input_tokens: u64, output_tokens: u64,
    ) -> Option<CostEstimate>;
    async fn workflow_history(&self, workflow: &str) -> Vec<WorkflowRunInfo>;
    async fn capabilities(&self) -> Option<DaemonCapabilities>;
}
```

**Step 3: Add `async-trait` to Cargo.toml and export module in lib.rs**

`tools/nika-daemon/Cargo.toml`: add `async-trait = "0.1"` to `[dependencies]`

`tools/nika-daemon/src/lib.rs`: add `pub mod provider;`

**Step 4: Run tests**

```bash
cd tools && cargo test --lib -p nika-daemon -- provider
```

**Step 5: Commit**

```bash
git add tools/nika-daemon/src/provider.rs tools/nika-daemon/src/lib.rs tools/nika-daemon/Cargo.toml
git commit -m "feat(daemon): add DaemonProvider trait for LSP/daemon unification"
```

---

### Task 2.2: Implement IpcDaemonProvider

**Files:**
- Create: `tools/nika-daemon/src/provider_ipc.rs`
- Modify: `tools/nika-daemon/src/lib.rs`

**Step 1: Write test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::DaemonProvider;

    #[tokio::test]
    async fn disconnected_satisfies_contract() {
        let provider = IpcDaemonProvider::disconnected();
        assert!(!provider.is_connected());
        assert!(provider.provider_status().await.is_empty());
        assert!(provider.capabilities().await.is_none());
        assert!(provider.workflow_history("test").await.is_empty());
    }
}
```

**Step 2: Implement by extracting logic from `daemon_bridge.rs`**

`IpcDaemonProvider` wraps the existing `DaemonClient`/`ConnectedClient` with the trait interface. Move the caching logic (provider status 60s TTL) from `daemon_bridge.rs` into this impl.

Key: `IpcDaemonProvider::disconnected()` returns a provider that always returns empty/None — the starting state before daemon connection.

**Step 3: Run tests**

```bash
cd tools && cargo test --lib -p nika-daemon -- provider_ipc
```

**Step 4: Commit**

```bash
git add tools/nika-daemon/src/provider_ipc.rs tools/nika-daemon/src/lib.rs
git commit -m "feat(daemon): IpcDaemonProvider wraps existing daemon client with trait"
```

---

### Task 2.3: Implement InProcessDaemonProvider

**Files:**
- Create: `tools/nika-daemon/src/provider_embedded.rs`
- Modify: `tools/nika-daemon/src/lib.rs`

**Step 1: Write test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::DaemonProvider;

    #[tokio::test]
    async fn embedded_satisfies_contract() {
        let provider = InProcessDaemonProvider::boot_test().await.unwrap();
        assert!(provider.is_connected()); // Always connected — we ARE the daemon
        let caps = provider.capabilities().await;
        assert!(caps.is_some());
        assert!(provider.workflow_history("nonexistent").await.is_empty());
    }
}
```

**Step 2: Implement with in-process services**

```rust
pub struct InProcessDaemonProvider {
    started_at: Instant,
    secret_service: SecretService,
    cache_service: CacheService,
    job_service: JobService,
    event_bus: EventBus,
}

impl InProcessDaemonProvider {
    pub async fn boot() -> Result<Self, DaemonError> {
        // Open SQLite, start cache reaper, start cron scheduler
    }

    /// Test constructor with in-memory storage
    #[cfg(test)]
    pub async fn boot_test() -> Result<Self, DaemonError> {
        // Same but with memory storage
    }
}

#[async_trait]
impl DaemonProvider for InProcessDaemonProvider {
    fn is_connected(&self) -> bool { true }
    // ... direct method calls to services, no IPC
}
```

**Note on thread safety:** All services are already `Send + Sync` (DashMap, Arc, broadcast channels). No new concurrency concerns.

**Note on `current_exe()`:** `JobService::start_job()` spawns `nika run` — when embedded in LSP, `current_exe()` returns `nika-lsp`, not `nika`. Pass an explicit `nika_bin: Option<PathBuf>` parameter, defaulting to `which::which("nika")`.

**Step 3: Run tests**

```bash
cd tools && cargo test --lib -p nika-daemon -- provider_embedded
```

**Step 4: Commit**

```bash
git add tools/nika-daemon/src/provider_embedded.rs tools/nika-daemon/src/lib.rs
git commit -m "feat(daemon): InProcessDaemonProvider runs services in LSP process"
```

---

### Task 2.4: Refactor NikaBackend to Use DaemonProvider Trait

**Files:**
- Modify: `tools/nika-lsp/src/backend.rs:42-56` — replace `Arc<RwLock<Option<DaemonBridge>>>` with `Arc<dyn DaemonProvider>`
- Delete or gut: `tools/nika-lsp/src/daemon_bridge.rs`
- Modify: `tools/nika-lsp/src/main.rs` — add `--embedded-daemon` flag

**Step 1: Update NikaBackend struct**

```rust
use nika_daemon::provider::DaemonProvider;

pub struct NikaBackend {
    client: Client,
    documents: DashMap<Uri, DocumentState>,
    validation_tx: mpsc::Sender<ValidationRequest>,
    handler: DefaultHandler,
    daemon: Arc<dyn DaemonProvider>,
    parse_success_tx: mpsc::Sender<ParseSuccessNotification>,
}
```

**Step 2: Add `with_embedded_daemon` constructor**

```rust
impl NikaBackend {
    pub fn new(client: Client) -> Self {
        Self::with_provider(client, false)
    }

    pub fn with_embedded_daemon(client: Client) -> Self {
        Self::with_provider(client, true)
    }
}
```

**Step 3: Simplify all daemon call sites**

Before:
```rust
let guard = self.daemon.read().await;
match guard.as_ref() {
    Some(bridge) if bridge.is_connected() => bridge.provider_status().await,
    _ => vec![],
}
```

After:
```rust
self.daemon.provider_status().await
```

**Step 4: Update `main.rs` to parse `--embedded-daemon` flag**

```rust
let embedded = std::env::args().any(|a| a == "--embedded-daemon")
    || std::env::var("NIKA_LSP_EMBEDDED_DAEMON").is_ok();

let builder = if embedded {
    LspService::build(|client| NikaBackend::with_embedded_daemon(client))
} else {
    LspService::build(NikaBackend::new)
};
```

**Step 5: Run ALL LSP tests**

```bash
cd tools && cargo test --lib -p nika-lsp -p nika-lsp-core
```

**Step 6: Commit**

```bash
git add tools/nika-lsp/src/backend.rs tools/nika-lsp/src/main.rs
git rm tools/nika-lsp/src/daemon_bridge.rs
git commit -m "refactor(lsp): use DaemonProvider trait, support embedded daemon

NikaBackend now takes Arc<dyn DaemonProvider> instead of
Arc<RwLock<Option<DaemonBridge>>>. All call sites simplified.
--embedded-daemon flag runs services in-process."
```

---

### Task 2.5: Wire Extension to Use --embedded-daemon

**Files:**
- Modify: `editors/vscode/src/extension.ts` — pass `--embedded-daemon` in server args

**Step 1: Add `--embedded-daemon` to default extra args**

In `startClient()`, update `serverOptions`:

```typescript
const serverOptions: ServerOptions = {
  command: serverPath,
  args: ['lsp', '--embedded-daemon', ...extraArgs],
  transport: TransportKind.stdio,
};
```

**Step 2: Update status bar to show "Engine" instead of "Daemon"**

Since the LSP now IS the daemon, change status text:
```typescript
statusBarItem.text = '$(zap) Nika: Ready';
```

**Step 3: Remove daemon polling interval** (no longer needed — always connected)

Delete the `setInterval` block that polls `nika/daemonStatus` every 30s.

**Step 4: Commit**

```bash
git add editors/vscode/src/extension.ts
git commit -m "feat(vscode): use embedded daemon by default — single process, zero config"
```

---

## Phase 3: Cursor + Windsurf Integration (1-2 days)

Auto-configure MCP server and AI rules for Cursor, Windsurf, and Claude Code.

---

### Task 3.1: Add IDE Detection + Auto-Config

**Files:**
- Modify: `editors/vscode/src/extension.ts`

**Step 1: Add IDE detection and MCP auto-configuration**

```typescript
function isCursor(): boolean {
  return env.appName === 'Cursor' || env.uriScheme === 'cursor';
}

async function ensureCursorMcpConfig(context: ExtensionContext): Promise<void> {
  const folder = workspace.workspaceFolders?.[0];
  if (!folder) return;

  const cursorDir = Uri.joinPath(folder.uri, '.cursor');
  const mcpPath = Uri.joinPath(cursorDir, 'mcp.json');

  try {
    await workspace.fs.stat(mcpPath);
    return; // Already exists — don't overwrite
  } catch {
    // File doesn't exist — create it
  }

  const nikaPath = resolvedServerPath ?? 'nika';
  const mcpConfig = {
    mcpServers: {
      nika: {
        command: nikaPath,
        args: ['mcp', 'serve', '--stdio'],
        env: {},
      },
    },
  };

  await workspace.fs.createDirectory(cursorDir);
  await workspace.fs.writeFile(mcpPath, Buffer.from(JSON.stringify(mcpConfig, null, 2)));
  log('INFO', 'Auto-generated .cursor/mcp.json for Cursor MCP integration');
}

async function ensureCursorRules(context: ExtensionContext): Promise<void> {
  const folder = workspace.workspaceFolders?.[0];
  if (!folder) return;

  const rulesDir = Uri.joinPath(folder.uri, '.cursor', 'rules');
  const rulePath = Uri.joinPath(rulesDir, 'nika.mdc');

  try {
    await workspace.fs.stat(rulePath);
    return; // Already exists
  } catch {
    // Create
  }

  const content = [
    '---',
    'description: Nika workflow engine rules for AI assistance',
    'globs: ["**/*.nika.yaml"]',
    'alwaysApply: false',
    '---',
    '',
    '# Nika Workflow Engine',
    '',
    'Schema: nika/workflow@0.12. Extension: .nika.yaml.',
    '',
    '## 5 Verbs',
    '- infer: LLM generation',
    '- exec: Shell command',
    '- fetch: HTTP request',
    '- invoke: MCP/builtin tool call',
    '- agent: Multi-turn loop',
    '',
    '## Key Rules',
    '- Bindings: with: { alias: $task_id } then {{with.alias}}',
    '- Always start with schema: "nika/workflow@0.12"',
    '- depends_on is always an array: depends_on: [task_id]',
    '- for_each output is always an array',
    '- shell: true requires | shell transform on bindings',
    '- Secrets via $env.VAR_NAME, never hardcode',
    '- timeout is always in seconds (not milliseconds)',
    '',
    '## Providers',
    'anthropic, openai, mistral, groq, deepseek, gemini, xai, native, mock',
    '',
    'Refer to AGENTS.md for complete documentation.',
  ].join('\n');

  await workspace.fs.createDirectory(rulesDir);
  await workspace.fs.writeFile(rulePath, Buffer.from(content));
  log('INFO', 'Auto-generated .cursor/rules/nika.mdc');
}
```

**Step 2: Call from activate() after LSP starts**

```typescript
client.start().then(() => {
  // ... existing success handler ...

  if (isCursor()) {
    ensureCursorMcpConfig(context);
    ensureCursorRules(context);
  }
});
```

**Step 3: Commit**

```bash
git add editors/vscode/src/extension.ts
git commit -m "feat(vscode): auto-configure Cursor MCP + rules on activation

Detects Cursor via env.appName. Generates .cursor/mcp.json for MCP
tool integration and .cursor/rules/nika.mdc for AI context."
```

---

### Task 3.2: Add VS Code MCP Configuration

**Files:**
- Modify: `editors/vscode/src/extension.ts`

**Step 1: Auto-generate `.vscode/mcp.json` for VS Code Agent Plugin compatibility**

Note: VS Code uses `"servers"` key, not `"mcpServers"`.

```typescript
async function ensureVscodeMcpConfig(context: ExtensionContext): Promise<void> {
  const folder = workspace.workspaceFolders?.[0];
  if (!folder) return;

  const vscodeDir = Uri.joinPath(folder.uri, '.vscode');
  const mcpPath = Uri.joinPath(vscodeDir, 'mcp.json');

  try {
    await workspace.fs.stat(mcpPath);
    return;
  } catch {
    // Create
  }

  const nikaPath = resolvedServerPath ?? 'nika';
  const mcpConfig = {
    servers: {
      nika: {
        type: 'stdio',
        command: nikaPath,
        args: ['mcp', 'serve', '--stdio'],
      },
    },
  };

  await workspace.fs.createDirectory(vscodeDir);
  await workspace.fs.writeFile(mcpPath, Buffer.from(JSON.stringify(mcpConfig, null, 2)));
  log('INFO', 'Auto-generated .vscode/mcp.json for VS Code MCP integration');
}
```

**Step 2: Commit**

```bash
git add editors/vscode/src/extension.ts
git commit -m "feat(vscode): auto-generate .vscode/mcp.json for VS Code agent plugins"
```

---

## Phase 4: DAG Webview Panel (3-5 days)

Interactive workflow graph visualization with click-to-source and live execution status.

---

### Task 4.1: Add esbuild + Webview Build Infrastructure

**Files:**
- Create: `editors/vscode/esbuild.mjs`
- Modify: `editors/vscode/package.json` — add esbuild scripts and dependencies
- Create: `editors/vscode/src/webview/dag.ts` — webview entry point

**Step 1: Install esbuild and D3/ELK dependencies**

Add to `devDependencies`:
```json
"esbuild": "^0.21.0"
```

Add to `dependencies`:
```json
"elkjs": "^0.9.3",
"d3": "^7.9.0"
```

Add to `devDependencies`:
```json
"@types/d3": "^7.4.3"
```

**Step 2: Create esbuild config**

`editors/vscode/esbuild.mjs`:
```javascript
import * as esbuild from 'esbuild';

// Extension (Node.js)
await esbuild.build({
  entryPoints: ['src/extension.ts'],
  bundle: true,
  outfile: 'out/extension.js',
  external: ['vscode'],
  format: 'cjs',
  platform: 'node',
  target: 'node18',
  sourcemap: true,
});

// Webview (browser)
await esbuild.build({
  entryPoints: ['src/webview/dag.ts'],
  bundle: true,
  outfile: 'out/webview/dag.js',
  format: 'iife',
  platform: 'browser',
  target: 'es2022',
  sourcemap: false,
  minify: true,
});
```

**Step 3: Update package.json scripts**

```json
"scripts": {
  "vscode:prepublish": "node esbuild.mjs",
  "compile": "node esbuild.mjs",
  "watch": "tsc -watch -p ./"
}
```

**Step 4: Commit**

```bash
git add editors/vscode/esbuild.mjs editors/vscode/package.json
git commit -m "feat(vscode): add esbuild for extension + webview dual-target bundling"
```

---

### Task 4.2: Add Custom LSP Request for Workflow Graph Data

**Files:**
- Modify: `tools/nika-lsp/src/backend.rs` — add `nika/workflowGraph` handler
- Modify: `tools/nika-lsp/src/main.rs` — register custom method

**Step 1: Add the handler**

In `backend.rs`, add a `workflow_graph` method that:
1. Receives a `{ "uri": "..." }` parameter
2. Reads the document from `self.documents`
3. Parses with `nika_engine::ast::raw::parse()`
4. Extracts nodes (id, verb, line number, description) and edges (depends_on)
5. Returns `{ "nodes": [...], "edges": [...] }` JSON

**Step 2: Register in `main.rs`**

```rust
let (service, socket) = builder
    .custom_method("nika/daemonStatus", backend::NikaBackend::daemon_status)
    .custom_method("nika/workflowGraph", backend::NikaBackend::workflow_graph)
    .finish();
```

**Step 3: Run tests**

```bash
cd tools && cargo test --lib -p nika-lsp
```

**Step 4: Commit**

```bash
git add tools/nika-lsp/src/backend.rs tools/nika-lsp/src/main.rs
git commit -m "feat(lsp): add nika/workflowGraph custom request for DAG visualization"
```

---

### Task 4.3: Create Webview Provider + Registration

**Files:**
- Create: `editors/vscode/src/dagPanel.ts` — webview provider
- Modify: `editors/vscode/src/extension.ts` — register command + provider
- Modify: `editors/vscode/package.json` — add command

**Step 1: Create DAG panel provider**

`editors/vscode/src/dagPanel.ts` — a class that:
- Creates a `WebviewPanel` with `enableScripts: true` and `retainContextWhenHidden: true`
- Sets `localResourceRoots` to `out/webview/`
- Uses CSP with nonce (no `unsafe-inline` or `unsafe-eval`)
- Handles `jumpToLine` messages from webview → `showTextDocument(uri, { selection })`
- Provides `update()` method that calls `client.sendRequest('nika/workflowGraph')` and forwards data via `postMessage`
- Colors nodes by verb: infer=purple, exec=green, fetch=cyan, invoke=pink, agent=orange

**Step 2: Register command in extension.ts**

```typescript
commands.registerCommand('nika.showDag', () => {
  const editor = window.activeTextEditor;
  if (!editor || !editor.document.fileName.endsWith('.nika.yaml')) {
    window.showWarningMessage('Open a .nika.yaml file first.');
    return;
  }
  if (!client) return;
  DagPanel.createOrShow(context.extensionUri, client, editor.document);
}),
```

**Step 3: Register in package.json**

Add to `"commands"`:
```json
{ "command": "nika.showDag", "title": "Nika: Show Workflow DAG", "icon": "$(type-hierarchy)" }
```

Add to `"editor/title"` menu:
```json
{ "command": "nika.showDag", "when": "resourceLangId == 'nika'", "group": "navigation" }
```

**Step 4: Commit**

```bash
git add editors/vscode/src/dagPanel.ts editors/vscode/src/extension.ts editors/vscode/package.json
git commit -m "feat(vscode): add interactive DAG webview panel for workflow visualization"
```

---

### Task 4.4: Create Webview D3 + ELK Rendering

**Files:**
- Create: `editors/vscode/src/webview/dag.ts`

**Step 1: Implement the webview rendering**

The webview script:
1. Listens for `updateGraph` messages from the extension
2. Uses ELK.js for layered layout (`elk.algorithm: layered`, direction: DOWN)
3. Uses D3.js for SVG rendering (nodes as rounded rects, edges with arrow markers)
4. Supports zoom/pan via `d3.zoom()`
5. Click on node → `postMessage({ type: 'jumpToLine', uri, line })`
6. Auto-fits the graph to viewport on load
7. Supports `updateStatus` messages for live execution coloring

Node colors by verb:
- infer: `#7c3aed` (purple)
- exec: `#16a34a` (green)
- fetch: `#0891b2` (cyan)
- invoke: `#db2778` (pink)
- agent: `#d97306` (orange)

Status colors:
- running: `#3b82f6` (blue) + spinner animation
- success: `#16a34a` (green)
- failed: `#dc2626` (red)
- skipped: `#6b7280` (gray)

**Step 2: Build and test**

```bash
cd editors/vscode && npm run compile
```

**Step 3: Commit**

```bash
git add editors/vscode/src/webview/dag.ts
git commit -m "feat(vscode): D3 + ELK DAG renderer for workflow webview

Interactive graph: click nodes to jump to source, zoom/pan,
color-coded by verb type, arrow markers for dependencies."
```

---

## Phase 5: Sidebar Tree View (2 days)

Workflow explorer with task tree, verb icons, and provider status.

---

### Task 5.1: Register Tree View in package.json

**Files:**
- Modify: `editors/vscode/package.json`

**Step 1: Add viewsContainers and views**

```json
"viewsContainers": {
  "activitybar": [
    {
      "id": "nika",
      "title": "Nika",
      "icon": "./icons/nika-dark.svg"
    }
  ]
},
"views": {
  "nika": [
    {
      "id": "nikaWorkflows",
      "name": "Workflows"
    },
    {
      "id": "nikaProviders",
      "name": "Providers"
    }
  ]
}
```

**Step 2: Commit**

```bash
git add editors/vscode/package.json
git commit -m "feat(vscode): register Nika sidebar with Workflows and Providers views"
```

---

### Task 5.2: Implement Workflow Tree Data Provider

**Files:**
- Create: `editors/vscode/src/workflowTree.ts`
- Modify: `editors/vscode/src/extension.ts`

**Step 1: Create tree data provider**

`editors/vscode/src/workflowTree.ts` — implements `TreeDataProvider<WorkflowItem>`:
- Root level: lists all `*.nika.yaml` files via `workspace.findFiles()`
- Child level: extracts task IDs from file content (regex: `/^\s*-\s*id:\s*(\S+)/gm`)
- Each task item has a `command` that opens the file at the task's line number
- Tree refreshes on file create/delete/change via `FileSystemWatcher`

**Step 2: Register in extension.ts**

```typescript
const workflowTree = new WorkflowTreeProvider();
window.registerTreeDataProvider('nikaWorkflows', workflowTree);

const watcher = workspace.createFileSystemWatcher('**/*.nika.yaml');
watcher.onDidCreate(() => workflowTree.refresh());
watcher.onDidDelete(() => workflowTree.refresh());
watcher.onDidChange(() => workflowTree.refresh());
context.subscriptions.push(watcher);
```

**Step 3: Commit**

```bash
git add editors/vscode/src/workflowTree.ts editors/vscode/src/extension.ts
git commit -m "feat(vscode): add workflow tree view in Nika sidebar

Lists all .nika.yaml files with expandable task trees.
Click task to jump to source line. Auto-refreshes on file changes."
```

---

## Phase 6: Crate Decoupling — nika-lsp Drops nika-engine (1 day)

The crate audit proved that nika-lsp's dependency on nika-engine is accidental — all imports are re-exports from nika-core. This phase eliminates the dependency.

---

### Task 6.1: Rewire nika-lsp Imports from nika-engine to nika-core

**Files:**
- Modify: `tools/nika-lsp/Cargo.toml` — remove nika-engine, add nika-core
- Modify: `tools/nika-lsp/src/diagnostics.rs` — change import paths
- Modify: `tools/nika-lsp/src/ast_integration.rs` — change import paths
- Modify: `tools/nika-lsp/src/backend.rs` — change import paths

**Step 1: Update Cargo.toml**

```toml
[dependencies]
nika-core = { workspace = true }       # AST, analyzer, source spans
nika-lsp-core = { workspace = true }   # LSP intelligence
nika-daemon = { workspace = true }     # DaemonProvider trait + impls
# nika-engine REMOVED
```

**Step 2: Change all imports**

Find and replace across nika-lsp/src/:
- `nika_engine::ast::raw::` → `nika_core::ast::raw::`
- `nika_engine::ast::analyzer::` → `nika_core::ast::analyzer::`
- `nika_engine::ast::analyzed::` → `nika_core::ast::analyzed::`
- `nika_engine::source::` → `nika_core::source::`

**Step 3: Add CI guardrail**

Add to `.github/workflows/ci.yml`:
```yaml
- name: Verify nika-lsp has no engine dependency
  run: |
    cd tools
    cargo tree -p nika-lsp --no-dedupe | grep -c "nika-engine" && \
      echo "FAIL: nika-lsp must not depend on nika-engine" && exit 1 || true
```

**Step 4: Verify compile time**

```bash
cargo build -p nika-lsp --timings
# Should be ~15s instead of ~90s
```

**Step 5: Run all tests**

```bash
cd tools && cargo test --workspace --lib
```

**Step 6: Commit**

```bash
git add tools/nika-lsp/
git commit -m "refactor(lsp): drop nika-engine dependency — use nika-core directly

nika-lsp now compiles in ~15s instead of ~90s.
All imports were re-exports from nika-core anyway."
```

---

## Phase 7: MCP Server Expansion — AI Writes Valid Nika (2 days)

Expand the MCP server from 4 basic tools to a full AI workflow partner.

---

### Task 7.1: Add MCP Prompts

**Files:**
- Modify: `tools/nika-mcp/src/server.rs` — enable_prompts(), add prompt handlers

**Step 1: Enable prompts in capabilities**

```rust
ServerCapabilities::builder()
    .enable_tools()
    .enable_prompts()  // NEW
    .build()
```

**Step 2: Add 3 prompt templates**

| Prompt | Description | Parameters |
|---|---|---|
| `create-workflow` | Generate complete workflow | `description: string`, `provider?: string` |
| `add-task` | Generate single task | `verb: string`, `description: string` |
| `fix-error` | Fix NIKA-XXX error | `code: string`, `file: string` |

**Step 3: Commit**

```bash
git add tools/nika-mcp/src/server.rs
git commit -m "feat(mcp): add 3 prompt templates for workflow generation"
```

---

### Task 7.2: Add New MCP Tools

**Files:**
- Modify: `tools/nika-mcp/src/server.rs` — add 3 new tools

**New tools:**

1. **`nika_generate_task`**: Takes natural language description → returns valid YAML task
   - Uses nika_schema reference internally
   - Validates output with `nika check` before returning

2. **`nika_dag_visualization`**: Takes workflow path → returns Mermaid DAG
   - Parses with nika-core AST
   - Generates `graph TD` with verb-colored nodes

3. **`nika_error_fix`**: Takes NIKA-XXX code + file content → returns suggested fix
   - Maps error codes to fix patterns
   - Returns corrected YAML, not just description

**Step 2: Run tests**

```bash
cd tools && cargo test --lib -p nika-mcp
```

**Step 3: Commit**

```bash
git add tools/nika-mcp/src/server.rs
git commit -m "feat(mcp): add generate_task, dag_visualization, error_fix tools

MCP server now has 7 tools + 3 prompts. AI editors can generate,
visualize, and auto-fix Nika workflows."
```

---

### Task 7.3: Add Event Streaming to LSP (for Live DAG)

**Files:**
- Modify: `tools/nika-lsp/src/backend.rs` — subscribe to events, emit notifications
- Modify: `tools/nika-lsp/src/main.rs` — register notification
- Modify: `editors/vscode/src/extension.ts` — listen for events, forward to webview

**Step 1: In NikaBackend, subscribe to EventBus when using embedded daemon**

```rust
// In with_provider() when embedded=true
if let Some(bus) = embedded_provider.event_bus() {
    let client = client_clone;
    let mut rx = bus.subscribe();
    tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            let _ = client.send_notification::<serde_json::Value>(
                "nika/executionEvent",
                serde_json::to_value(&event).unwrap_or_default(),
            ).await;
        }
    });
}
```

**Step 2: In extension.ts, forward to webview**

```typescript
client.onNotification('nika/executionEvent', (event: any) => {
  if (DagPanel.currentPanel) {
    DagPanel.currentPanel.panel.webview.postMessage({
      type: 'updateStatus',
      taskId: event.task_id,
      status: event.kind === 'TaskCompleted' ? 'success'
            : event.kind === 'TaskFailed' ? 'failed'
            : event.kind === 'TaskStarted' ? 'running'
            : 'pending',
    });
  }
});
```

**Step 3: Commit**

```bash
git add tools/nika-lsp/src/backend.rs editors/vscode/src/extension.ts
git commit -m "feat(lsp): stream execution events to webview for live DAG updates

Events flow: Engine → EventBus → LSP notification → Extension → Webview.
DAG nodes animate: gray → spinner → green/red in real time."
```

---

## Verification Checklist

### Automated (CI)

```bash
# 1. All Rust tests
cd tools && cargo test --workspace --lib

# 2. nika-lsp has NO engine dependency (GUARDRAIL)
cargo tree -p nika-lsp --no-dedupe | grep -c "nika-engine"  # Must be 0

# 3. E2E LSP harness (17+ scenarios)
cargo build -p nika-lsp
cargo test -p nika-lsp --test e2e_harness -- --ignored

# 4. MCP server tests
cargo test --lib -p nika-mcp

# 5. DaemonProvider contract tests (both impls)
cargo test --lib -p nika-daemon -- provider

# 6. Extension compilation
cd editors/vscode && npm ci && npm run compile

# 7. Extension smoke tests
cd editors/vscode && npm test

# 8. Platform VSIX build
npx vsce package --no-git-tag-version --target darwin-arm64

# 9. Webview bundle exists
test -f out/webview/dag.js
```

### Manual (Human Verification)

- [ ] Install extension on VS Code — zero setup, bundled binary works
- [ ] Install extension on Cursor — no "command not found"
- [ ] Code Lens: "▶ Run Workflow" above `tasks:` — click runs in terminal
- [ ] Code Lens: "✓ Validate" on schema line — click runs nika check
- [ ] Code Lens: "5 tasks" badge — correct count
- [ ] Inlay Hints: `timeout: 10` shows ` (10s)`
- [ ] Inlay Hints: `model: claude-sonnet-4` shows ` ($3/$15 per 1M)`
- [ ] Inlay Hints: `with: $research` shows ` ← research output`
- [ ] Semantic Tokens: verbs colored differently from properties
- [ ] Document Links: URLs in fetch clickable
- [ ] DAG Webview: renders, nodes colored by verb
- [ ] DAG Webview: click node → jumps to source line
- [ ] DAG Webview: during run, nodes animate gray→blue→green/red
- [ ] Sidebar Tree: all .nika.yaml files listed with tasks
- [ ] Sidebar Tree: click task → jumps to line
- [ ] `.cursor/mcp.json` auto-generated on Cursor
- [ ] `.cursor/rules/nika.mdc` auto-generated on Cursor
- [ ] Restart server preserves binary path
- [ ] Status bar shows "Nika: Ready" (not "Daemon $(x)")
- [ ] MCP: Cursor AI can call nika_check via agent mode
- [ ] MCP: `nika_generate_task` returns valid YAML

---

## Summary

| Phase | What | Tasks | Time | LOC |
|-------|------|-------|------|-----|
| **0** | Bug fixes + activate hidden LSP features | 8 | 30 min | ~100 |
| **1** | Platform-specific VSIX (binary bundling) | 2 | 1 day | ~150 TS+CI |
| **2** | DaemonProvider trait (LSP/daemon unification) | 5 | 2-3 days | +170 Rust |
| **3** | Cursor/Windsurf auto-config (MCP + rules) | 2 | 1-2 days | ~200 TS |
| **4** | DAG webview (ELK.js + D3.js) | 4 | 3-5 days | ~550 TS+Rust |
| **5** | Sidebar tree view | 2 | 2 days | ~150 TS |
| **6** | Crate decoupling (drop nika-engine from LSP) | 1 | 1 day | ~-50 Rust |
| **7** | MCP expansion (7 tools + 3 prompts + events) | 3 | 2 days | ~400 Rust |
| **Total** | | **27 tasks** | **~12-15 days** | **~1,670 net** |

### Execution Order (Dependencies)

```
Phase 0 (quick wins) ──→ Phase 6 (crate decoupling)
                     ──→ Phase 1 (binary bundling) ──→ Phase 3 (Cursor)
                     ──→ Phase 2 (DaemonProvider)  ──→ Phase 7.3 (events)
                                                    ──→ Phase 4 (DAG webview)
                                                    ──→ Phase 5 (sidebar)
Phase 7.1-7.2 (MCP tools) — independent, can run in parallel
```

### Research Reports

| Report | Location |
|--------|----------|
| rust-analyzer bundling pattern | `docs/research/2026-04-06-vscode-platform-binary-bundling.md` |
| DAG webview patterns | `docs/research/2026-04-06-vscode-dag-webview-research.md` |
| MCP cross-editor integration | `docs/research/2026-04-06-mcp-integration-cursor-windsurf-research.md` |
