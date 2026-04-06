# SESSION PROMPT — Nika IDE / VS Code Extension

> **Copy-paste this into a new Claude Code session.**
> **Mode**: Full autonomy, TDD, multi-commit, push when done.

---

## WHO YOU ARE

Senior engineer on **Nika** — AI workflow engine (Rust, 15 crates). Transform the VS Code extension from a broken 32KB shell into a first-class IDE. Franglais conversations, EN code/commits.

---

## ETAPE 1 — Analyze Current State

```bash
# Extension (759 lines, 8 bugs)
cat editors/vscode/src/extension.ts

# Manifest (missing 6 LSP features)
cat editors/vscode/package.json

# LSP (wrong nika-engine dep at line 42)
cat tools/nika-lsp/Cargo.toml
grep -r "nika_engine::" tools/nika-lsp/src/

# DAG scaffold (complete, ready to copy)
ls tools/nika-vscode/src/

# Detailed plans (READ THESE)
cat docs/plans/2026-04-06-nika-ide-plan.md        # 500+ lines — detailed implementation
cat docs/plans/2026-04-06-nika-ide-mega-session.md # Execution guide with phases
cat docs/plans/2026-04-06-nika-ide-roadmap.md      # Phase roadmap
```

---

## PHASE ORDER

```
Phase 0 (30 min) → 8 bug fixes + activate 6 hidden LSP features
Phase 6 (1 day)  → Decouple nika-lsp from nika-engine (90s→15s compile)
Phase 1 (1 day)  → Platform-specific VSIX binary bundling
Phase 3 (1-2d)   → Cursor/Windsurf MCP auto-config
Phase 2 (2-3d)   → DaemonProvider trait + embedded daemon
Phase 4 (3-5d)   → DAG webview (ELK.js + D3.js) — scaffold in tools/nika-vscode/
Phase 5 (2d)     → Sidebar tree view
Phase 7 (2d)     → MCP expansion (3 tools + 3 prompts)
```

**START WITH PHASE 0.** Details for each phase in `docs/plans/2026-04-06-nika-ide-plan.md`.

---

## 8 BUGS (Phase 0)

1. **CRITICAL**: Commands at lines 682-750 registered AFTER async execFile at 636 → Cursor crash. Fix: move commands before probe.
2. `extension.ts:592` — `@ts-ignore` StatusBarAlignment magic number → use enum
3. `extension.ts:684` — Code lens ignores URI arg → accept `uri?: Uri` param
4. `extension.ts:741` — restartServer loses binary path → persist in module scope
5. `extension.ts:489` — setInterval accumulates on restart → clear before creating
6. `extension.ts:571` — installExtension fails on Cursor → fallback to marketplace URL
7. `extension.ts:534` — Path not escaped in terminal commands → escape special chars
8. `package.json` — Missing: semanticTokenScopes, keybindings, inlayHints.enabled, codeLens.enabled, extensionKind

---

## KEY FACTS (from 16-agent codebase audit)

- LSP backend advertises 13 capabilities (`backend.rs:177-264`) — extension activates ~7
- `tools/nika-lsp/Cargo.toml:42` has `nika-engine` dep → all types come from nika-core
- `tools/nika-lsp/src/ast_integration.rs:9` imports `nika_engine::` → should be `nika_core::`
- DAG scaffold complete: `tools/nika-vscode/` has esbuild.mjs, dagPanel.ts, dag.ts (641 lines), dag.css
- VS Code MCP uses `"servers"` key, NOT `"mcpServers"` (Cursor/Windsurf use mcpServers)
- `extensionKind: ["workspace"]` is MISSING → extension fails in Remote/WSL
- ELK.js must use `elk.bundled.js` (no Worker) — alias in esbuild.mjs
- `retainContextWhenHidden: true` mandatory on DAG WebviewPanel (zoom/pan state)
- NikaVault: use `~/.nika/secrets/` (engine path), NOT `~/.nika/daemon/secrets/`
- `current_exe()` returns `nika-lsp` not `nika` → pass explicit path to JobService

---

## V0 PHILOSOPHY

- Zero backward compat — v0.x = nuke and rebuild freely
- Zero config — install extension, open .nika.yaml, everything works
- `Co-Authored-By: Nika 🦋 <nika@supernovae.studio>` on every commit
- No Keychain popups — always `cargo test --workspace --lib`
- 1 fix = 1 commit

---

## VERIFICATION

```bash
# Rust
cd tools && cargo test --workspace --lib
cargo tree -p nika-lsp --no-dedupe | grep -c "nika-engine"  # MUST BE 0 after Phase 6

# TypeScript
cd editors/vscode && npm run compile

# VSIX (after Phase 1)
npx vsce package --no-git-tag-version --target darwin-arm64
ls -la *.vsix  # ~18MB
```

---

## START

1. Read ETAPE 1 files above
2. Read `docs/plans/2026-04-06-nika-ide-plan.md` for full details
3. Phase 0: fix 8 bugs → commit each → push
4. Phase 6 → 1 → 3 → 2 → 4 → 5 → 7
5. Each phase: TDD, verify, commit, push
