# MASTER INSTRUCTION — Nika v0.78.0 → Launch

> **Copy-paste this ENTIRE document into a new Claude Code session.**
> It contains EVERYTHING: what to do, how to do it, what skills to use,
> what documents to read, and how to verify.

---

## WHO YOU ARE

Rust + TypeScript senior on **Nika** — semantic YAML workflow engine for AI ("Inference as Code").
v0.78.0, 17 crates, ~409K LOC, 10,498 tests. Launch: May 5, 2026.
Franglais conversations, EN code/commits. Paris timezone. Solo founder.

**Project root**: `/Users/thibaut/dev/supernovae/nika`
**Workspace**: `/Users/thibaut/dev/supernovae/nika/tools` (Cargo workspace)
**Editors**: `/Users/thibaut/dev/supernovae/nika/editors/` (vscode, zed, neovim, helix)

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

# 4. TypeScript (if touching editors/vscode/)
cd /Users/thibaut/dev/supernovae/nika/editors/vscode && npm run compile

# 5. Editor keyword sync (if touching transforms/builtins/task keys)
./editors/sync-editors.sh --check
```

**Expected baseline**: 10,498+ tests GREEN, 0 clippy warnings, fmt clean.

---

## WHAT TO DO — 3 TRACKS

### Track 1: Pre-Launch Stabilization

**Document**: `docs/sprints/SESSION-MEGA-FINAL.md`

⚠ Many items from the original list are now DONE (npm sync done at 0.77, clippy fixed, etc.). **Verify each item before fixing** — the codebase moved fast.

**Still likely needed** (verify first):
| # | Fix | File | Effort |
|---|-----|------|--------|
| B2 | TreeDataProvider disposable leak | `editors/vscode/src/extension.ts` | 5min |
| B3 | Duplicate daemon reconnect | `tools/nika-lsp/src/backend.rs` | 10min |
| B4 | Blocking fs in async reconciler | `tools/nika-serve/src/lib.rs` | 30min |
| B5 | WWW-Authenticate header on 401 | `tools/nika-serve/src/auth.rs` | 10min |
| C2 | Artifact path collision detection | `tools/nika-engine/src/runtime/artifact_processor.rs` | 1h |
| D1 | Auth middleware integration tests | `tools/nika-serve/src/auth.rs` | 30min |

**New items from recent sessions** (check handoffs):
| # | Fix | Source |
|---|-----|--------|
| T1 | Templatable perf: has_any_template() fast-path | `SESSION-TEMPLATABLE-HANDOFF.md` S1 |
| T2 | Templatable: template in on_error fallback task | `SESSION-TEMPLATABLE-HANDOFF.md` S2 |
| T3 | Templatable: template in guardrails threshold | `SESSION-TEMPLATABLE-HANDOFF.md` S3 |
| M1 | Channel matrix: publish to Open VSX, Homebrew, npm | `SESSION-MEGA-HANDOFF-2026-04-07.md` |
| E1 | Zed: publish to extensions.zed.dev | `SESSION-HANDOFF-EDITORS-ARCH-2026-04-07.md` |

**How**: Read each handoff doc, verify what's done vs pending, fix remaining items in TDD.

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
1. `CLAUDE.md` (project root) — crate graph, commands, conventions, editor arch
2. `docs/plans/2026-04-07-architecture-roadmap.md` — **6-phase refactoring roadmap** (God Crate split, NikaError domains, runner decomp, provider macro, serve/daemon, AST split)
3. `CHANGELOG.md` — full history through v0.75.0

### Latest Handoffs (most recent first)
4. `docs/sprints/SESSION-MEGA-HANDOFF-2026-04-07.md` — **mega handoff**: podcast, 5 editors, distribution audit, channel matrix
5. `docs/sprints/SESSION-TEMPLATABLE-HANDOFF.md` — Templatable<T> feature (65 typed fields), deferred items S1-S4
6. `docs/sprints/SESSION-MODEL-RESILIENCE-HANDOFF.md` — ModelCapabilities catalog, stale defaults fix
7. `docs/sprints/SESSION-HANDOFF-EDITORS-ARCH-2026-04-07.md` — 5 editors architecture, Zed MCP Context Server
8. `docs/sprints/SESSION-MEGA-FINAL.md` — 15 stabilization fixes (some done, verify first)

### Architecture Plans
9. `docs/plans/2026-04-07-ai-rules-architecture.md` — 4-layer progressive AI context (L0→L3)
10. `docs/plans/2026-04-07-zed-deep-integration-plan.md` — Zed WASM extension details
11. `docs/plans/2026-04-07-deferred-provider-refactor.md` — provider type zoo cleanup plan
12. `docs/plans/2026-04-07-model-resilience.md` — model capabilities design

### Scheduling & IDE (DONE — reference only)
13. `docs/sprints/SESSION-SCHEDULING-HANDOFF.md` — scheduling implementation (DONE)
14. `docs/sprints/SESSION-IDE-HANDOFF.md` — IDE/VSCode extension (DONE)
15. `docs/plans/2026-04-05-scheduling-ux-bible.md` — scheduling UX patterns

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

### Editors (5 total)
- **VS Code/Cursor**: `cd editors/vscode && npm run compile` (tsc + esbuild)
  - `extensionKind: ["workspace"]` required for Remote/WSL
  - Cursor MCP uses `"mcpServers"`, VS Code uses `"servers"` (DIFFERENT keys!)
  - ELK.js must use bundled version (no Web Worker in webview sandbox)
- **Zed**: `cd editors/zed/` — Rust WASM extension, MCP Context Server is killer feature
- **Neovim**: `cd editors/neovim/` — Lua plugin, `require("nika").setup()`
- **Helix**: `cd editors/helix/` — TOML config + Tree-sitter queries
- **Shared**: `editors/shared/nika-keywords.json` — generated from Rust source
- **Sync**: `./editors/sync-editors.sh --fix` propagates keyword changes to all editors
- **When adding transforms/builtins**: update Rust source → run sync script → commit both

### Git
- 1 fix = 1 commit, granulaire
- Push after each logical chunk
- Pre-commit hooks: fmt + clippy (automatic)

---

## CURRENT STATS

```
Version:     0.78.0
Tests:       10,498 passed, 0 failed, 1 ignored
Clippy:      clean
Crates:      17 workspace members
LOC:         ~409K Rust + ~2K TypeScript
Storage:     Schema V6 (schedules + serve_tokens)
Providers:   9 (7 cloud + native + mock) + OpenAI-compat table (7 more)
Transforms:  64 (incl. parse_yaml)
Builtins:    63 (incl. json_diff)
LSP:         13 capabilities, decoupled from nika-engine
MCP:         7 tools, 3 prompts, enable_prompts()
Editors:     5 (VS Code, Zed, Neovim, Helix, shared keywords)
Extension:   DAG webview, sidebar tree, Cursor/Windsurf auto-config
Features:    Templatable<T> (65 typed fields), ModelCapabilities catalog,
             slash syntax (model: groq/llama), endpoints in nika.toml,
             S10 multi-tenant auth (BLAKE3), scheduling full stack
```

### Recent Major Changes (v0.75→v0.78)
- **Templatable<T>**: `{{inputs.temperature}}` works in ALL 65 typed fields (number/bool/integer)
- **ModelCapabilities catalog**: single source of truth for model-specific API behavior
- **Provider split**: rig/mod.rs god file → 4 modules (inference, streaming, construction, compat)
- **5 editors**: VS Code, Zed (WASM + MCP Context Server), Neovim (Lua), Helix (TOML), shared keywords
- **AI rules architecture**: 4-layer progressive discovery (L0 identity → L3 live MCP)
- **Distribution fixes**: Open VSX publishing fixed, VSIX icon, marketplace metadata
- **max_completion_tokens**: fixed for OpenAI reasoning models (o-series, gpt-5.x)

---

## START

1. Run verification baseline:
   ```bash
   cd /Users/thibaut/dev/supernovae/nika/tools
   cargo test --workspace --lib --exclude nika-py
   # Expected: 10,498+ tests, 0 failures
   ```

2. Check what's new since your last session:
   ```bash
   git log --oneline -20
   cat docs/sprints/SESSION-MEGA-HANDOFF-2026-04-07.md | head -60
   ```

3. Pick your track:
   - **Stabilization** → Read latest handoffs (items 4-8 in KEY DOCUMENTS), verify + fix remaining
   - **Architecture** → Read `docs/plans/2026-04-07-architecture-roadmap.md`, start at Phase 3
   - **Distribution** → Read channel matrix in `SESSION-MEGA-HANDOFF-2026-04-07.md`, publish to channels
   - **Templatable perf** → Read `SESSION-TEMPLATABLE-HANDOFF.md` items S1-S4
   - **Quick wins** → Just fix them inline

4. For every fix: **test first** (RED), then **fix** (GREEN), then **refactor** if needed.

5. Commit granulaire, push after each chunk.

6. When done, run full verification suite and push.
