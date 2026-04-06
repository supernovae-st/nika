# Nika IDE — Roadmap & Document Index

> Master index for the Nika IDE redesign. All documents, research, scaffolds, and findings organized.

---

## Quick Start

**To launch the autonomous session:**
Read `docs/plans/2026-04-06-nika-ide-session-prompt.md` and copy the prompt into a new Claude Code terminal.

---

## Documents Map

```
docs/plans/
├── 2026-04-06-nika-ide-roadmap.md          ← YOU ARE HERE (index)
├── 2026-04-06-nika-ide-plan.md             ← Main plan (800 lines, 8 phases, 27 tasks)
├── 2026-04-06-nika-ide-mega-session.md     ← Autonomous session handoff (400 lines)
└── 2026-04-06-nika-ide-session-prompt.md   ← Copy-paste prompt for new session

docs/research/
├── 2026-04-06-vscode-platform-binary-bundling.md   ← rust-analyzer pattern
├── 2026-04-06-vscode-dag-webview-research.md        ← ELK.js + D3.js patterns
├── 2026-04-06-mcp-integration-cursor-windsurf-research.md ← MCP across 4 editors
└── 2026-04-06-vscode-extension-tdd-research.md      ← Test strategy

tools/nika-vscode/                         ← DAG webview scaffold (copy to editors/vscode/)
├── esbuild.mjs        (125 lines, dual target Node+Browser)
├── src/dagPanel.ts     (340 lines, WebviewPanel + CSP nonce)
├── src/webview/dag.ts  (641 lines, ELK layout + D3 rendering)
├── src/webview/dag.css (362 lines, theme-adaptive + animations)
├── src/extension.ts    (65 lines, demo wiring)
├── package.json        (dependencies)
└── tsconfig.json       (TS config)
```

---

## Execution Phases

| # | Phase | Tasks | Time | Depends On | Key Deliverable |
|---|-------|-------|------|------------|-----------------|
| 0 | Bug Fixes + Activate Hidden Features | 8 | 30 min | — | 8 bugs fixed, 6 LSP features visible |
| 6 | Crate Decoupling | 1 | 1 day | Phase 0 | nika-lsp compiles in 15s (was 90s) |
| 1 | Platform-Specific VSIX | 2 | 1 day | Phase 0 | Binary bundled, zero-friction install |
| 2 | DaemonProvider Trait | 5 | 2-3 days | Phase 0 | Single process, embedded daemon |
| 3 | Cursor/Windsurf Auto-Config | 2 | 1-2 days | Phase 1 | .cursor/mcp.json + rules auto-gen |
| 4 | DAG Webview | 4 | 3-5 days | Phase 2 | Interactive graph, click-to-source |
| 5 | Sidebar Tree View | 2 | 2 days | Phase 2 | Workflow explorer, task navigation |
| 7 | MCP Expansion | 3 | 2 days | Independent | 7 tools + 3 prompts |

```
Phase 0 ──→ Phase 6 (crate decouple)
       ──→ Phase 1 (VSIX) ──→ Phase 3 (Cursor)
       ──→ Phase 2 (DaemonProvider) ──→ Phase 4 (DAG webview)
                                    ──→ Phase 5 (sidebar)
Phase 7 (MCP) — runs in parallel, independent
```

---

## 16 Agent Findings Summary

### Vague 1 — Exploration (3 agents)
| Agent | Key Finding |
|-------|------------|
| LSP backend | 3,400 tests, 13 handlers, 16 cursor contexts, all mature |
| VS Code extension | 759 lines, 32KB, 8 bugs, 6/13 features invisible |
| Daemon architecture | 6,400 LOC, 5 services, all async/tokio, all Send+Sync |

### Vague 2 — Research + Audit (5 agents)
| Agent | Key Finding |
|-------|------------|
| rust-analyzer pattern | `vsce --target`, binary in `server/`, 15-20MB, deny-all .vscodeignore |
| DAG webview patterns | ELK.js (Sugiyama) + D3.js (SVG), ~400KB, retainContextWhenHidden |
| Code review | Root cause Cursor: commands after async probe (race condition) |
| Rust architect | DaemonProvider trait, IPC + InProcess impls, +170 LOC net |
| Gap analysis | 6 LSP capabilities registered but not declared in package.json |

### Vague 3 — Cursor (2 agents)
| Agent | Key Finding |
|-------|------------|
| Cursor APIs | No public API. MCP = only path. Detect via env.appName |
| MCP cross-editor | 4 editors support MCP but different config formats |

### Vague 4 — Architecture (4 agents)
| Agent | Key Finding |
|-------|------------|
| MCP server | Only 4 tools, zero Prompts, zero Resources server-side |
| LSP tests | E2E harness (17 scenarios, 1000 lines), 9,361 test lines, 30 fixtures |
| Event system | 58 types, RAII guard, broadcast. LSP doesn't subscribe (gap) |
| Crate graph | nika-lsp can drop nika-engine — compile 15s vs 90s |

### Vague 5 — Execution Prep (5 agents)
| Agent | Key Finding |
|-------|------------|
| TDD patterns | vitest for TS, tokio::io::duplex for MCP, rust-analyzer = minimal TS tests |
| Context doc | Exact state of every file, line numbers, function map |
| DaemonProvider types | Types in nika-core/catalogs/lsp_types.rs. CostEstimate = no daemon needed |
| Bug-fix diffs | Surgical old→new for all 8 bugs + extensionKind missing |
| DAG scaffold | 7 complete files: esbuild, dagPanel, dag.ts, dag.css, etc. |

---

## Critical Gotchas (from agent findings)

### Rust
1. **NikaVault path divergence**: daemon uses `~/.nika/daemon/secrets/`, engine uses `~/.nika/secrets/`. InProcessDaemonProvider MUST use `~/.nika/secrets/`.
2. **current_exe() in JobService**: Returns `nika-lsp` not `nika`. Pass explicit `nika_bin: Option<PathBuf>`.
3. **CostEstimate**: Computed from static KNOWN_PRICING table. No daemon needed. Call `nika_core::catalogs::estimate_cost()` directly.
4. **SecretService**: NOT Clone, wrap in Arc. CacheService IS Clone. JobService IS Clone.
5. **EventBus**: NOT Clone, wrap in Arc. broadcast::Sender is lock-free.

### TypeScript
6. **extensionKind: ["workspace"]** missing — breaks Remote/Codespaces/WSL.
7. **VS Code MCP uses `"servers"` key**, not `"mcpServers"` like Cursor/Windsurf/Claude.
8. **Cursor PATH broken** — never rely on `execFile('nika')`, always use absolute path.
9. **webview CSP** — D3 needs `style-src 'unsafe-inline'` for transforms. Use nonce for scripts.

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

## Skills Reference

| When | Skill | Why |
|------|-------|-----|
| Every task | `test-driven-development` | Write test first, watch fail, implement |
| Rust code | `spn-rust:rust-core` | Ownership, error handling, type-state |
| Async Rust | `spn-rust:rust-async` | tokio, channels, DaemonProvider |
| Before "done" | `verification-before-completion` | Run tests, check output |
| After each phase | `requesting-code-review` | Dispatch reviewer agent |
| Debugging | `systematic-debugging` | 4-phase: root cause → pattern → hypothesis → fix |
| Parallel tasks | `dispatching-parallel-agents` | 3+ independent fixes |
| Plan execution | `executing-plans` | Batch tasks, checkpoints |

---

## Commit Convention

```
type(scope): description

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

Types: `fix`, `feat`, `refactor`, `test`, `chore`
Scopes: `vscode`, `lsp`, `daemon`, `mcp`, `ci`

**ABSOLUTE RULE: NEVER use Claude/Anthropic co-author. ALWAYS Nika 🦋.**
