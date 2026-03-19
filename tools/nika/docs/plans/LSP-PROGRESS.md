# LSP v2.0 — Progress Tracker

> Updated by Claude Code after each session.

## Status: BATCH 2 DONE — foundation complete

| Batch | PR | Status | Tests Before | Tests After | Notes |
|-------|-----|--------|:------------:|:-----------:|-------|
| **0** | Pre-flight | **DONE** | 6,093 | 6,093 | Docs, clippy, nika-lsp v0.34.0 sync |
| **1** | PR:extract-ast | **DONE** | 6,093 | 6,637 | nika-core: 597 tests, nika: 6,040 |
| **1** | PR:tower-lsp-upgrade | **DONE** | 6,255 | 6,255 | tower-lsp-server 0.23, ls-types, Uri, RPITIT |
| **1.5** | PR4 verification | **DONE** | - | - | All 8 PR4 checks ✅, all Socratic ✅ |
| **2** | PR:foundation | **DONE** | 6,255 | 7,019 | nika-lsp-core: 94 tests, WorldDatabase+LineIndex+PositionIndex |
| **3** | PR:error-recovery | **NEXT** | - | - | tree-sitter bridge |
| **4** | PR:wire-* (6 sub-PRs) | - | - | - | Handler migration |
| **5** | PR:cleanup + errors | - | - | - | Delete old + error coverage |
| **6** | PR:standard-features | - | - | - | 14 new handlers |

## nika-core Crate Structure

```
nika-core v0.34.0 (15 deps, zero heavy deps)
├── source/        — FileId, Span, Spanned<T>, SourceRegistry (12 tests)
├── catalogs/      — providers (19), models (15), mcp_aliases (48) (41 tests)
├── binding/       — types, entry (WithSpec), transform (27 ops) (239 tests)
├── error.rs       — CoreError (InvalidPath, InvalidDefault, ValidationError)
└── ast/           — Full 3-phase pipeline (305 tests)
    ├── raw/       — parser, task, workflow, action, mcp
    ├── analyzed/  — workflow, task, ids
    ├── analyzer/  — analyze, errors, suggestions
    └── schema, content, budget, output, decompose, context,
        logging, limits, include, structured, agent_def, artifact
```

## Blockers

None.

## Session Log

| Date | Session | PR | Commits | Duration | Notes |
|------|---------|-----|---------|----------|-------|
| 2026-03-19 | S0.0 | Pre-flight | 4 | ~45min | Docs, cargo fmt + 49 clippy fixes, nika-lsp sync |
| 2026-03-19 | S1.1 | extract-ast | 5 | ~3h | nika-core skeleton, source/, catalogs/, binding/, AST |
| 2026-03-19 | S1.2 | tower-lsp | 1 | ~20min | tower-lsp-server 0.23, 23 files migrated |
| 2026-03-19 | S1.5 | PR4 verify | 0 | ~10min | All 8 PR4 checks + Socratic verification |
| 2026-03-19 | S2.1 | foundation | 1 | ~10min | nika-lsp-core: db, position, document (94 tests) |
