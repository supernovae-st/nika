# MASTER INSTRUCTION — Nika v0.79.0

> **Copy-paste this ENTIRE document into a new Claude Code session.**

---

## WHO YOU ARE

Rust + TypeScript senior on **Nika** — semantic YAML workflow engine for AI ("Inference as Code").
v0.79.0, 17 crates, ~409K LOC, 10,666 tests. Launch: **May 5, 2026**. Solo founder, Paris.

```
Project root:  /Users/thibaut/dev/supernovae/nika
Workspace:     /Users/thibaut/dev/supernovae/nika/tools    (Cargo)
Editors:       /Users/thibaut/dev/supernovae/nika/editors/  (vscode, zed, neovim, helix, shared)
```

---

## SKILLS (mandatory — use Skill tool)

```
test-driven-development           RED → GREEN → REFACTOR
verification-before-completion    cargo test + clippy + fmt BEFORE commit
systematic-debugging              Root cause BEFORE fix
rust                              Idiomatic Rust, proper error types
requesting-code-review            After each major feature
```

## COMMITS

```
type(scope): description

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```
1 fix = 1 commit. Push after each logical chunk.

## VERIFICATION (before EVERY commit)

```bash
cd /Users/thibaut/dev/supernovae/nika/tools
cargo test --workspace --lib --exclude nika-py    # 10,498+ tests, 0 fail
cargo clippy --workspace -- -D warnings           # 0 warnings
cargo fmt --all --check                           # clean
# If touching editors/vscode:
cd ../editors/vscode && npm run compile
# If touching transforms/builtins/keywords:
./editors/sync-editors.sh --check
```

---

## WHAT EXISTS (don't redo)

All of these are SHIPPED and TESTED:

| Feature | Version | Tests |
|---------|---------|-------|
| 5 verbs (infer/exec/fetch/invoke/agent) | @0.12 | 4,532 |
| 64 transforms + 63 builtins | v0.74 | catalog guards |
| Scheduling (cron, overlap, timeline, wizard, reconciler) | v0.72-74 | 40+ |
| IDE (DAG webview, sidebar, Cursor MCP, binary bundling) | v0.74 | 12 vitest |
| S10 Multi-tenant auth (BLAKE3, moka, AuthMode::MultiKey) | v0.74 | V6 schema |
| LSP decoupled (nika-core only, 13 capabilities) | v0.74 | 949 |
| MCP (7 tools, 3 prompts, enable_prompts) | v0.74 | 388 |
| Templatable<T> (65 typed fields, runtime resolution) | v0.76 | +40 |
| ModelCapabilities catalog (token limits, vision, thinking) | v0.76 | 23 |
| Provider split (rig/mod.rs → 4 modules) | v0.77 | existing |
| 5 editors (VS Code, Zed, Neovim, Helix, shared keywords) | v0.77 | sync script |
| AI rules (shared modules, per-tool assemblers, L0-L3) | v0.78 | doctor checks |
| Slash syntax (model: groq/llama), endpoints nika.toml | v0.74 | existing |
| on_error routing, native-inference default | v0.69-73 | existing |
| runner.rs test split + task_dispatch.rs extracted | v0.75 | existing |

---

## WHAT TO DO

### Track A: Pre-Launch Fixes (~3h)

⚠ **Verify each item first** — codebase moves fast, some may be done.

| # | Fix | Check | Effort |
|---|-----|-------|--------|
| A1 | TreeDataProvider disposable leak | `grep "registerTreeDataProvider" editors/vscode/src/extension.ts` — is return value pushed to subscriptions? | 5min |
| A2 | Blocking std::fs in async reconciler | `grep "std::fs::read_to_string" tools/nika-serve/src/lib.rs` — in async context? | 30min |
| A3 | WWW-Authenticate header on 401 | `grep "UNAUTHORIZED" tools/nika-serve/src/auth.rs` — has WWW-Authenticate? | 10min |
| A4 | Artifact path collision detection | `grep "HashSet" tools/nika-engine/src/runtime/artifact_processor.rs` — exists? | 1h |
| A5 | Auth middleware integration tests | `grep "#\[tokio::test\]" tools/nika-serve/src/auth.rs` — how many? | 30min |
| A6 | Templatable perf fast-path | Read `docs/sprints/SESSION-TEMPLATABLE-HANDOFF.md` item S1 | 2h |

### Track B: Distribution / Publish (~2h)

From `docs/sprints/SESSION-MEGA-HANDOFF-2026-04-07.md` channel matrix:

| Channel | Status | Action |
|---------|--------|--------|
| VS Code Marketplace | v0.74 published | Push + tag v0.78 |
| Open VSX (Cursor) | NEVER published | Need OVSX_PAT in GitHub secrets |
| Homebrew | v0.72 | Need HOMEBREW_TAP_TOKEN refresh |
| npm | v0.71 | Fix Windows build cascade |
| crates.io | v0.47 | Decide strategy |
| Zed extensions | Not published | Submit to extensions.zed.dev |

### Track C: Architecture Refactoring (post-launch, ~10 weeks)

**Document**: `docs/plans/2026-04-07-architecture-roadmap.md`

Each phase has a **copy-paste session prompt** in the document:

| Phase | What | Effort | Impact |
|-------|------|--------|--------|
| 3 | Runner decomposition (2,262 LOC → 10 modules) | 1 week | Medium |
| 1 | **God Crate split** (nika-engine 158K → 5 crates) | 2 weeks | **HIGH — compile -30%** |
| 2 | NikaError domains (103 → 15 sub-enums) | 4-6 weeks | HIGH |
| 4 | Provider macro (20 → 3 touch points) | 1 week | Medium |
| 6 | Parser/analyzer split (4K+5K → 5+8 files) | 1 week | Low |
| 5 | Serve/daemon unification | 2 weeks | Medium |

### Track D: Templatable Deferred Items

From `docs/sprints/SESSION-TEMPLATABLE-HANDOFF.md`:

| # | Item | Effort |
|---|------|--------|
| S1 | `has_any_template()` fast-path (skip clone when no templates) | 2h |
| S2 | Template in `on_error` fallback task | 1h |
| S3 | Template in guardrails `threshold` field | 1h |
| S4 | Negative test: `temperature: "{{inputs.temp}}"` with string that can't parse | 30min |

### Quick Wins (anytime)

| Fix | Where | Effort |
|-----|-------|--------|
| Delete dead `error_domains.rs` | `nika-engine/src/error_domains.rs` | 5min |
| Centralize VERB_KEYS constant | `parser.rs` + `validate.rs` | 15min |
| Merge Runner constructors | `runner/mod.rs` (90% duplicate) | 30min |

---

## KEY DOCUMENTS (read order)

### Start Here
1. **`CLAUDE.md`** — crate graph, commands, conventions, editor arch, AI rules arch
2. **`docs/plans/2026-04-07-architecture-roadmap.md`** — 6-phase refactoring with session prompts

### Latest Handoffs (most recent first)
3. `docs/sprints/SESSION-MEGA-HANDOFF-2026-04-07.md` — podcast, 5 editors, distribution, channel matrix
4. `docs/sprints/SESSION-TEMPLATABLE-HANDOFF.md` — Templatable<T>, deferred S1-S4
5. `docs/sprints/SESSION-MODEL-RESILIENCE-HANDOFF.md` — ModelCapabilities catalog
6. `docs/sprints/SESSION-HANDOFF-EDITORS-ARCH-2026-04-07.md` — 5 editors, Zed MCP

### Architecture Plans
7. `docs/plans/2026-04-07-ai-rules-architecture.md` — 4-layer progressive AI context
8. `docs/plans/2026-04-07-deferred-provider-refactor.md` — provider type zoo cleanup
9. `docs/plans/2026-04-07-zed-deep-integration-plan.md` — Zed WASM extension

### Reference (DONE — only if needed)
10. `docs/sprints/SESSION-SCHEDULING-HANDOFF.md` — scheduling (DONE)
11. `docs/sprints/SESSION-IDE-HANDOFF.md` — IDE/VSCode (DONE)
12. `docs/plans/2026-04-05-scheduling-ux-bible.md` — UX patterns

---

## MULTI-AGENT PATTERNS

### Code Review (after implementing)
```
Lance 3 agents feature-dev:code-reviewer en parallèle sur les fichiers touchés.
Consolide → fix les bugs trouvés → re-test.
```

### Architecture Analysis (before refactoring)
```
Lance un agent spn-rust:rust-architect par sous-système.
Chaque agent lit le code, propose améliorations avec file:line.
Consolide en plan d'exécution.
```

### Bug Hunting (periodic)
```
Lance 3-5 agents feature-dev:code-reviewer sur des fichiers différents.
Cherche: bugs, dead code, missing tests, security issues.
Priorise et fix en TDD.
```

### Feature Implementation
```
1. Agent feature-dev:code-explorer — analyse le code existant
2. Agent feature-dev:code-architect — propose l'architecture
3. Implémente en TDD
4. Agent spn-powers:code-reviewer — review le résultat
```

---

## CONVENTIONS

### Rust
- `cargo test --workspace --lib` (ALWAYS --lib — Keychain popups)
- Error codes: NIKA-XXX (see `nika-core/src/error_codes.rs`)
- Schema: `nika/workflow@0.12`, 5 verbs, 64 transforms, 63 builtins

### Editors
- 1 LSP binary (`nika lsp --stdio`), N thin wrappers
- `./editors/sync-editors.sh --fix` after adding transforms/builtins/keywords
- VS Code: `npm run compile` (tsc + esbuild), `extensionKind: ["workspace"]`
- Zed: Rust WASM, MCP Context Server = killer feature
- Cursor MCP key: `"mcpServers"` — VS Code MCP key: `"servers"` (DIFFERENT!)

### AI Rules (new v0.78)
- Content in `tools/nika-cli/rules/shared/` (7 modules)
- Per-tool assemblers in `tools/nika-cli/src/rules.rs`
- 4 layers: L0 identity → L1 syntax → L2 reference → L3 live (MCP)
- `nika init` writes project files, `nika setup` writes home dir rules
- `nika doctor` checks MCP config + rule file health

---

## STATS

```
Version:     0.79.0
Tests:       10,666 passed, 0 failed, 1 ignored
Crates:      17 workspace members
Providers:   9 + 7 OpenAI-compat = 16 total
Transforms:  65 (incl. html_escape, md_escape, sanitize)
Builtins:    63 (incl. json_diff)
Editors:     5 (VS Code, Zed, Neovim, Helix, shared)
LSP:         13 capabilities, decoupled
MCP:         7 tools, 3 prompts
Auth:        Multi-tenant BLAKE3 + legacy Bearer
```

---

## START

```bash
# 1. Baseline
cd /Users/thibaut/dev/supernovae/nika/tools
cargo test --workspace --lib --exclude nika-py

# 2. What's new
git log --oneline -15

# 3. Pick track: A (fixes) / B (publish) / C (architecture) / D (Templatable)

# 4. For every fix: test FIRST (RED) → fix (GREEN) → refactor if needed

# 5. Commit granulaire, push after each chunk
```
