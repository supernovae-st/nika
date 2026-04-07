# MASTER INSTRUCTION — Nika v0.75.0 → Launch

> **Copy-paste this ENTIRE document into a new Claude Code session.**
> It contains EVERYTHING: what to do, how to do it, what skills to use,
> what documents to read, and how to verify.

---

## WHO YOU ARE

Rust + TypeScript senior on **Nika** — semantic YAML workflow engine for AI.
v0.75.0, 17 crates, ~409K LOC, 10,365 tests. Launch: May 5, 2026.
Franglais conversations, EN code/commits. Paris timezone.

**Project root**: `/Users/thibaut/dev/supernovae/nika`
**Workspace**: `/Users/thibaut/dev/supernovae/nika/tools` (Cargo workspace)
**Extension**: `/Users/thibaut/dev/supernovae/nika/editors/vscode`

---

## MANDATORY SKILLS (use the Skill tool for each)

```
test-driven-development           — RED → GREEN → REFACTOR, always
verification-before-completion    — cargo test + clippy + fmt BEFORE commit
systematic-debugging              — Root cause BEFORE fix
rust                              — Idiomatic Rust, proper error types
requesting-code-review            — After each major feature
```

---

## COMMIT RULES

```
type(scope): description

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

Types: `feat`, `fix`, `refactor`, `test`, `docs`, `chore`, `perf`, `style`
Scopes: `engine`, `ast`, `cli`, `tui`, `serve`, `daemon`, `lsp`, `mcp`, `vscode`, `ci`

**1 fix = 1 commit. Granulaire. Jamais batché.**

---

## VERIFICATION (before EVERY commit)

```bash
cd /Users/thibaut/dev/supernovae/nika/tools

# 1. Tests (ALWAYS --lib to avoid macOS Keychain popups)
cargo test --workspace --lib --exclude nika-py

# 2. Clippy
cargo clippy --workspace -- -D warnings

# 3. Format
cargo fmt --all --check

# 4. TypeScript (if touching editors/)
cd /Users/thibaut/dev/supernovae/nika/editors/vscode && npm run compile
```

**Expected baseline**: 10,365+ tests GREEN, 0 clippy warnings, fmt clean.

---

## WHAT TO DO — 3 TRACKS

### Track 1: Pre-Launch Stabilization (~4h)

**Document**: `docs/sprints/SESSION-MEGA-FINAL.md`

15 fixes ordered by priority. TDD for each. Summary:

| # | Fix | File | Effort |
|---|-----|------|--------|
| A1 | Stale counts 63→64/62→63 | `nika-core/catalogs/builtins.rs:108`, `nika/src/main.rs:945`, `nika-mcp/server.rs` | 15min |
| A2 | npm version sync 0.74→0.75 | `packages/*/package.json` (6 files) | 10min |
| A3 | Dockerfile version | `tools/nika/Dockerfile:56` | 2min |
| B1 | CI upload-artifact@v7→v8 | `.github/workflows/release.yml` (3 sites) | 5min |
| B2 | TreeDataProvider disposable | `editors/vscode/src/extension.ts:77` | 5min |
| B3 | Duplicate daemon reconnect | `tools/nika-lsp/src/backend.rs:96-101` | 10min |
| B4 | Blocking fs in async reconciler | `tools/nika-serve/src/lib.rs` | 30min |
| B5 | WWW-Authenticate header | `tools/nika-serve/src/auth.rs:47` | 10min |
| B7 | MCP schema version param doc | `tools/nika-mcp/src/server.rs` | 5min |
| C1 | cargo fmt | `tools/` | 2min |
| C2 | Artifact path collision | `tools/nika-engine/src/runtime/artifact_processor.rs` | 1h |
| C3 | Stale TODO comment | `tools/nika-engine/src/runtime/for_each.rs:28` | 2min |
| D1 | Auth middleware tests | `tools/nika-serve/src/auth.rs` | 30min |
| D2 | Named endpoint slash test | `tools/nika-engine/src/runtime/executor/infer.rs` | 10min |
| D4 | Schedule comment test | `tools/nika-serve/src/lib.rs` | 10min |

**How**: Read `docs/sprints/SESSION-MEGA-FINAL.md` for exact before/after code.

---

### Track 2: Post-Launch Architecture Refactoring (~10 weeks)

**Document**: `docs/plans/2026-04-07-architecture-roadmap.md`

6 phases with session prompts ready to copy-paste:

| Phase | What | Effort | ROI |
|-------|------|--------|-----|
| 3 | Runner decomposition (2,262→10 modules) | 1 week | Medium — navigability |
| 1 | God Crate extraction (nika-engine 158K→5 crates) | 2 weeks | **HIGH — compile time -30%** |
| 2 | NikaError domains (103→15 sub-enums) | 4-6 weeks | HIGH — dev velocity |
| 4 | Provider macro (20→3 touch points) | 1 week | Medium — extensibility |
| 6 | Parser/analyzer split (4K+5K→5+8 files) | 1 week | Low — readability |
| 5 | Serve/daemon unification | 2 weeks | Medium — correctness |

**How**: Each phase in the roadmap has a `### Session Prompt` section. Copy it into a new session.

---

### Track 3: Quick Wins (do anytime, in any session)

| Fix | Where | Effort |
|-----|-------|--------|
| Centralize VERB_KEYS constant | `parser.rs` + `validate.rs` | 15min |
| Merge Runner constructors | `runner/mod.rs` (with_event_log + with_policy share 90%) | 30min |
| Delete dead error_domains.rs | `nika-engine/src/error_domains.rs` | 5min |
| Comment `schedule: _` in lower.rs | `lower.rs:61` | 2min |
| provider_embedded.rs fmt | `nika-daemon/src/provider_embedded.rs` | 2min |

---

## KEY DOCUMENTS (read order for new sessions)

### Architecture & Current State
1. `CLAUDE.md` (project root) — crate graph, commands, conventions
2. `docs/plans/2026-04-07-architecture-roadmap.md` — **6-phase refactoring roadmap**
3. `docs/sprints/SESSION-FINAL-HANDOFF.md` — launch readiness inventory
4. `CHANGELOG.md` — full history through v0.74.0

### Stabilization
5. `docs/sprints/SESSION-MEGA-FINAL.md` — **15 fixes with TDD workflow**

### Feature-Specific (if continuing work on these)
6. `docs/sprints/SESSION-SCHEDULING-HANDOFF.md` — scheduling implementation
7. `docs/sprints/SESSION-IDE-HANDOFF.md` — IDE/VSCode extension
8. `docs/plans/2026-04-06-s10-multi-tenant-auth-blueprint.md` — auth (DONE)
9. `docs/plans/2026-04-06-nika-ide-plan.md` — IDE detailed plan (DONE)

### Design References
10. `docs/plans/2026-04-05-scheduling-ux-bible.md` — scheduling UX patterns
11. `docs/plans/2026-04-05-scheduling-design.md` — scheduling architecture
12. `docs/plans/2026-04-05-cli-ux-wow-patterns.md` — CLI interaction design

---

## MULTI-AGENT WORKFLOW

Pour les tâches complexes, lance plusieurs agents en parallèle :

### Pattern: Code Review
```
Lance 3 agents feature-dev:code-reviewer en parallèle:
1. Agent A: review fichier X pour bugs, security, quality
2. Agent B: review fichier Y pour les mêmes
3. Agent C: run tests + clippy + fmt
→ Consolide les résultats, fix les bugs trouvés
```

### Pattern: Architecture Analysis
```
Lance 6 agents spn-rust:rust-architect en parallèle:
1. Un par sous-système (runner, errors, crates, providers, serve, AST)
→ Chaque agent lit le code, propose des améliorations
→ Consolide en un document architecture
```

### Pattern: Bug Hunting
```
Lance 3-5 agents feature-dev:code-reviewer sur des fichiers différents:
→ Chaque agent cherche bugs, dead code, missing tests
→ Fusionne les résultats, priorise, fix en TDD
```

### Pattern: Feature Implementation
```
1. Agent feature-dev:code-explorer — analyse le code existant
2. Agent feature-dev:code-architect — propose l'architecture
3. Toi: implémentes en TDD avec verification-before-completion
4. Agent spn-powers:code-reviewer — review le résultat
```

---

## NIKA-SPECIFIC CONVENTIONS

### Rust
- `cargo test --workspace --lib` (ALWAYS --lib, never without, to avoid Keychain)
- `Co-Authored-By: Nika 🦋 <nika@supernovae.studio>` (ALWAYS, never Claude)
- Schema: `nika/workflow@0.12`
- 5 verbs: infer, exec, fetch, invoke, agent
- Error codes: NIKA-XXX (see `nika-core/src/error_codes.rs`)

### TypeScript (editors/vscode)
- `npm run compile` (tsc + esbuild)
- `extensionKind: ["workspace"]` required for Remote/WSL
- Cursor MCP uses `"mcpServers"`, VS Code uses `"servers"` (DIFFERENT keys!)
- ELK.js must use bundled version (no Web Worker in webview sandbox)

### Git
- 1 fix = 1 commit, granulaire
- Push after each logical chunk
- Pre-commit hooks: fmt + clippy (automatic)

---

## CURRENT STATS

```
Version:     0.75.0
Tests:       10,365 passed, 0 failed, 1 ignored
Clippy:      clean
Fmt:         1 file drift (provider_embedded.rs)
Crates:      17 workspace members
LOC:         ~409K Rust + ~2K TypeScript
Storage:     Schema V6 (schedules + serve_tokens)
Providers:   9 (7 cloud + native + mock)
Transforms:  64
Builtins:    63
LSP:         13 capabilities, 949 tests
MCP:         7 tools, 3 prompts
Extension:   DAG webview, sidebar tree, Cursor/Windsurf auto-config
```

---

## START

1. Run verification baseline:
   ```bash
   cd /Users/thibaut/dev/supernovae/nika/tools
   cargo test --workspace --lib --exclude nika-py 2>&1 | grep "passed"
   ```

2. Pick your track:
   - **Stabilization** → Read `docs/sprints/SESSION-MEGA-FINAL.md`, start at A1
   - **Architecture** → Read `docs/plans/2026-04-07-architecture-roadmap.md`, start at Phase 3
   - **Quick wins** → Just fix them inline

3. For every fix: **test first** (RED), then **fix** (GREEN), then **refactor** if needed.

4. Commit granulaire, push after each chunk.

5. When done, run full verification suite and push.
