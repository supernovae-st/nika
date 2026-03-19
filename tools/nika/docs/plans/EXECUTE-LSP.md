# EXECUTE-LSP — Autonomous Execution Playbook

> **For:** Claude Code autonomous sessions
> **Plan:** [v2.0 Master Plan](./2026-03-19-nika-lsp-v2-master-plan.md)
> **Reference:** [Implementation Reference](./2026-03-19-nika-lsp-v2-implementation-reference.md)
> **Progress:** Track in [LSP-PROGRESS.md](./LSP-PROGRESS.md)

---

## Execution Rules

1. **1 session = 1 PR (or 1 sub-PR)**. Never cross PR boundaries.
2. **Checkpoint after every session.** Run verification commands. If ANY test fails, STOP.
3. **1 fix = 1 commit.** Conventional commits with co-author lines.
4. **Tests before commit:** `cargo test --lib` (safe, no keychain). `cargo clippy -- -D warnings`.
5. **Human review required** after: PR:extract-ast, first handler migration, PR:error-recovery.
6. **Never delete code** until the replacement is proven for 1+ week.

---

## PR Dependency Graph

```
                        PR:extract-ast ←── HUMAN REVIEW
                             |
                    +--------+--------+
                    |                 |
             PR:foundation      PR:tower-lsp-upgrade
                    |                 |
                    +--------+--------+
                             |
                      PR:error-recovery ←── HUMAN REVIEW
                             |
              PR:handler-migration-wire (6 sub-PRs) ←── HUMAN REVIEW after first
                             |
                      PR:handler-migration-delete
                             |
                      PR:standard-features (3 sub-PRs)

PARALLEL (start anytime):
  PR:vscode-polish
  PR:benchmarks
  PR:context-inputs

BLOCKED by handler-migration-wire:
  PR:error-coverage (was Track B, actually Track A dependency)
  All Track C PRs (10 PRs, parallelizable among themselves)
```

---

## Branch Strategy

| Batch | Branch name | Base |
|-------|-------------|------|
| 0 | `feat/lsp-v2-preflight` | main |
| 1 | `feat/lsp-v2-extract-upgrade` | main (after batch 0 merge) |
| 2 | `feat/lsp-v2-foundation` | main (after batch 1 merge) |
| QW | `feat/lsp-v2-quick-wins` | main (after batch 1 merge, parallel) |
| 3 | `feat/lsp-v2-error-recovery` | main (after batch 2 merge) |
| 4 | `feat/lsp-v2-handler-migration` | main (after batch 3 merge) |
| 5 | `feat/lsp-v2-cleanup` | main (after batch 4 merge) |
| 6 | `feat/lsp-v2-standard-features` | main (after batch 5 merge) |
| C | `feat/lsp-v2-intelligence-*` | main (after batch 6 merge) |

---

## Session-by-Session Execution Order

### Batch 0 — Pre-flight (BEFORE any LSP work)

| Session | What | Commits | Checkpoint |
|---------|------|---------|-----------|
| S0.0 | Merge `feat/pr4-vision-media` to main | 1 | `cargo test --lib` on main passes |
| S0.0 | Fix `nika-lsp` version dep (0.30.3 → 0.34) | 1 | `cargo check -p nika-lsp` compiles |

### Batch 0b — Quick Wins (start NOW, parallel to everything)

| Session | PR | Commits | Checkpoint |
|---------|-----|---------|-----------|
| S0.1 | PR:vscode-polish | 5-7 | Extension builds, grammar tests pass |
| S0.2 | PR:benchmarks | 3-4 | `cargo bench --bench micro_benchmarks` runs |
| S0.3 | PR:context-inputs | 4-6 | `cargo test --lib --features lsp` passes |

### Batch 1 — Extract AST (CRITICAL PATH)

| Session | PR | What | Commits | Checkpoint |
|---------|-----|------|---------|-----------|
| S1.1 | PR:extract-ast | Move `source/` + `error.rs` to nika-core | 3-4 | `cargo check -p nika-core` + `cargo test --lib` no regression |
| S1.2 | PR:extract-ast | Move `ast/raw/` + `ast/analyzed/` + `ast/analyzer/` | 4-5 | `cargo test -p nika-core --lib` runs AST tests |
| S1.3 | PR:extract-ast | Move `binding/{types,entry,transform}.rs` + `core/{providers,models,mcp_aliases}` | 4-5 | `cargo test --lib` 6093+ passed |

**STOP: Human review of crate boundary before continuing.**

Verification:
```bash
cargo check -p nika-core
cargo check -p nika
cargo check -p nika-lsp
cargo test -p nika-core --lib 2>&1 | tail -3   # >= 700 tests
cargo test --lib 2>&1 | tail -3                  # 6093+ tests, 0 failed
cargo clippy -p nika-core -- -D warnings
# Verify no heavy deps in nika-core:
cargo tree -p nika-core --depth 1 | grep -E "tokio|reqwest|rmcp|rig-core|image"
# Expected: empty (no heavy deps)
```

### Batch 2 — Foundation (parallel pair)

| Session | PR | What | Commits | Checkpoint |
|---------|-----|------|---------|-----------|
| S2.1 | PR:tower-lsp-upgrade | tower-lsp 0.20 → tower-lsp-server 0.23 + ls-types | 2-3 | Both LSP entry points start and respond |
| S2.2 | PR:foundation (part 1) | WorldDatabase + LineIndex + PositionIndex | 4-5 | `cargo test -p nika-lsp-core --lib` 40+ tests |
| S2.3 | PR:foundation (part 2) | Workspace, document (Rope), protocol types | 3-4 | `cargo test -p nika-lsp-core --lib` 80+ tests |

Verification:
```bash
cargo check -p nika-lsp-core
cargo test -p nika-lsp-core --lib 2>&1 | tail -3   # 80+ tests
cargo test -p nika-lsp-core --lib -- position       # Position tests
cargo test -p nika-lsp-core --lib -- db              # WorldDatabase tests
cargo clippy -p nika-lsp-core -- -D warnings
```

### Batch 3 — Error Recovery

| Session | PR | What | Commits | Checkpoint |
|---------|-----|------|---------|-----------|
| S3.1 | PR:error-recovery (part 1) | tree-sitter bridge + 10 broken YAML fixtures | 4-5 | All fixtures produce PartialWorkflow (never panic) |
| S3.2 | PR:error-recovery (part 2) | 40+ more fixtures + insta snapshots | 3-4 | `cargo insta test -p nika-lsp-core` |

**STOP: Human review of tree-sitter integration.**

Verification:
```bash
cargo test -p nika-lsp-core --lib -- recovery
cargo test -p nika-lsp-core --lib -- bridge
cargo test -p nika-lsp-core --lib -- broken
# Zero panics on ANY broken fixture:
cargo test -p nika-lsp-core --lib -- broken 2>&1 | grep -c "FAILED"  # Expect: 0
```

### Batch 4 — Handler Migration (6 sub-PRs, 1 per handler)

| Session | Sub-PR | Handler | Commits | Checkpoint |
|---------|--------|---------|---------|-----------|
| S4.1 | wire-completion | completion.rs (biggest, ~1,200 LOC) | 5-7 | Completions work in VS Code |
| S4.2 | wire-hover | hover.rs + model_intel integration | 4-5 | Hover works |
| S4.3 | wire-definition | definition.rs + cross-file stub | 3-4 | Go-to-def works |
| S4.4 | wire-code-action | code_action.rs (7 existing fixes) | 3-4 | Quick fixes work |
| S4.5 | wire-semantic-tokens | semantic_tokens.rs | 3-4 | Syntax colors work |
| S4.6 | wire-symbols | symbols.rs | 2-3 | Outline works |

**STOP after S4.1: Human review of the handler migration pattern before replicating 5 more times.**

Verification (after each sub-PR):
```bash
cargo test --lib --features lsp 2>&1 | tail -3   # No regression
cargo test -p nika-lsp-core --lib 2>&1 | tail -3  # Growing test count
cargo test -p nika-lsp --lib 2>&1 | tail -3        # Standalone works
```

### Batch 5 — Cleanup + Error Coverage

| Session | PR | What | Commits | Checkpoint |
|---------|-----|------|---------|-----------|
| S5.1 | PR:handler-migration-delete | Delete 19 old files | 2-3 | `git diff --stat` shows net LOC decrease |
| S5.2 | PR:error-coverage (part 1) | Wire NIKA-001-029, NIKA-050-059 with quick fixes | 5-7 | 20+ codes surfaced |
| S5.3 | PR:error-coverage (part 2) | Wire NIKA-030-049, NIKA-070-089 with quick fixes | 4-5 | 35+ codes surfaced |

### Batch 6 — Standard Features (3 sub-PRs)

| Session | Sub-PR | Handlers | Commits |
|---------|--------|----------|---------|
| S6.1 | standard-trivial | folding, highlight, selection_range, document_link, linked_editing, document_color | 6-8 |
| S6.2 | standard-medium | workspace_symbol, on_type_formatting, signature_help, references | 5-7 |
| S6.3 | standard-hard | inlay_hints, code_lens, rename, call_hierarchy | 6-8 |

---

## Track C Sessions (post Track A, parallelizable)

| Session | PR | Commits | Est. |
|---------|-----|---------|------|
| SC.1 | PR:vision-lsp | 4-5 | 1 session |
| SC.2 | PR:media-tools-lsp | 5-6 | 1 session |
| SC.3 | PR:template-intelligence | 6-8 | 2 sessions |
| SC.4 | PR:prompt-linter | 4-5 | 1 session |
| SC.5 | PR:cost-radar | 4-5 | 1 session |
| SC.6 | PR:cross-file | 6-8 | 2 sessions |
| SC.7 | PR:structured-output-lsp | 4-5 | 1 session |
| SC.8 | PR:foreach-agent-retry-lsp | 5-6 | 1 session |
| SC.9 | PR:guardrails-lsp | 4-5 | 1 session |
| SC.10 | PR:security-diagnostics | 2-3 | 1 session |

---

## Session Protocol

### Starting a session

```bash
# 1. Check current state
git status
cargo test --lib 2>&1 | tail -3  # Verify green baseline

# 2. Create branch (if new PR)
git checkout -b lsp/PR-NAME

# 3. Check LSP-PROGRESS.md for what's done
cat docs/plans/LSP-PROGRESS.md
```

### During a session

```bash
# After each logical change:
cargo test --lib 2>&1 | tail -3          # Must pass
cargo clippy -- -D warnings 2>&1 | tail -3  # Must be clean
git add <specific files>
git commit -m "type(scope): description

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <nika@supernovae.studio>"
```

### Ending a session

```bash
# 1. Full verification
cargo test --lib --features lsp 2>&1 | tail -3
cargo clippy -- -D warnings

# 2. Update progress
# Edit docs/plans/LSP-PROGRESS.md with:
# - PR status (done/in-progress/blocked)
# - Test count before/after
# - Blockers found
# - Next session's starting point

# 3. Push
git push -u origin lsp/PR-NAME
```

### When things go wrong

1. **Test fails after a commit** → `git stash`, investigate, fix, `git stash pop`
2. **Compile error in another crate** → Check if you broke a re-export. `cargo check -p nika` to verify.
3. **Test count decreased** → Something was deleted. `git diff --stat` to find what.
4. **Clippy warning** → Fix immediately. Never suppress with `#[allow]`.
5. **Unclear next step** → Read the master plan section for the current PR. If still unclear, END SESSION and request human guidance.

---

## Key Decisions (Already Made)

| Decision | Choice | Rationale |
|----------|--------|-----------|
| tower-lsp-server version | **0.23** (ls-types, not lsp-types) | Latest, actively maintained |
| Translation layer | **Option B: depend on ls-types** in nika-lsp-core | 0 LOC boilerplate, ls-types is lightweight |
| handler-migration-wire | **6 sub-PRs** (1 per handler) | Never merge 6 handlers at once |
| standard-features | **3 sub-PRs** (trivial/medium/hard) | Risk-ordered |
| nika-core extraction | **source + error + ast/raw + ast/analyzed + ast/analyzer + binding/{types,entry,transform} + core/{providers,models,mcp_aliases}** | Verified clean deps |
| Broken YAML fixtures | **Start with 10, grow to 50+** | Incremental, not big-bang |
| PR:error-coverage | **Moved to after handler-migration-wire** | Needs diagnostics pipeline |

---

## Estimated Total

| Track | Sessions | Wall-clock (4 sessions/week) |
|-------|----------|------------------------------|
| Batch 0 (Quick Wins) | 3 | Week 1 (parallel) |
| Batch 1 (extract-ast) | 3 + review | Weeks 1-2 |
| Batch 2 (foundation) | 3 | Weeks 3-4 |
| Batch 3 (error-recovery) | 2 + review | Week 5 |
| Batch 4 (handler migration) | 6 + review | Weeks 6-8 |
| Batch 5 (cleanup + errors) | 3 | Week 9 |
| Batch 6 (standard features) | 3 | Weeks 10-11 |
| Track C (intelligence) | 12 | Weeks 12-15 |
| **Total** | **~35 sessions** | **~15-18 weeks** |

Realistic with setbacks: **20-26 weeks** for Tracks A+B+C.

---

## Optimal Session Interleaving

Alternate heavy Track A sessions with lighter Track B "breathers":

```
Session 1:  BATCH 0  (Pre-flight: merge PR4, fix nika-lsp)       ~30 min
Session 2:  BATCH 1  (Extract AST + tower-lsp upgrade)            ~4 hours
Session 3:  Quick Wins (vscode-polish + benchmarks)               ~2 hours  ← BREATHER
Session 4:  BATCH 2  (Foundation: WorldDatabase)                   ~4 hours
Session 5:  Quick Wins (context-inputs)                            ~2 hours  ← BREATHER
Session 6:  BATCH 3  (Error recovery: tree-sitter)                 ~4 hours
            ─── HUMAN REVIEW GATE (1+ day) ───
Session 7:  BATCH 4a (Handler migration: completion)               ~4 hours
            ─── HUMAN REVIEW GATE (review pattern) ───
Session 8:  BATCH 4b (Handler migration: hover + definition)       ~4 hours
Session 9:  BATCH 4c (Handler migration: code_action + tokens + symbols) ~4 hours
Session 10: BATCH 5  (Delete old code + error coverage)            ~3 hours
Session 11: BATCH 6a (Standard features: trivial 6)                ~3 hours
Session 12: BATCH 6b (Standard features: medium + hard 8)          ~4 hours
Session 13: Track C  (Intelligence alpha: vision + media + template) ~4 hours
Session 14: Track C  (Intelligence beta: linter + cost + guardrails) ~4 hours
Session 15: Track C  (Intelligence gamma: cross-file + structured + loops) ~4 hours

Total: ~14-15 sessions, ~45-55 hours Claude Code time
```

---

## nika-core Extraction Notes (Technical)

**What moves to nika-core** (verified clean deps):
- `source/` (span.rs, registry.rs) — 660 LOC
- `error.rs` — BUT only `AnalysisError` variant subset, NOT full NikaError
- `ast/raw/` — parser + raw types, ~3,000 LOC
- `ast/analyzed/` — validated types, ~2,000 LOC
- `ast/analyzer/` — analysis pass, ~2,000 LOC
- `ast/{schema,content,budget,output,decompose,context,logging,limits,include}.rs`
- `binding/{types,entry,transform}.rs` — types + parsing only, NOT resolve/template
- `core/{providers,models,mcp_aliases}.rs` — static catalogs only

**What STAYS in nika** (has I/O or runtime deps):
- `ast/lower.rs` (produces runtime types)
- `ast/{workflow,action,agent,invoke}.rs` (runtime type defs)
- `ast/{loader,include_loader,import_loader}.rs` (file I/O)
- `ast/{guardrails,skill_def,pkg_resolver,schema_validator}.rs` (heavy deps)
- `binding/{resolve,template}.rs` (depends on store::RunContext)
- `dag/` (depends on runtime types — could split later)
- All runtime, MCP, provider, media, event modules

**The binding split is the hardest part.** binding/mod.rs currently re-exports everything. After the split:
- nika-core has `binding/{types,entry,transform}` → re-exported
- nika has `binding/{resolve,template,validate,jsonpath,mention}` → adds to the re-exports
