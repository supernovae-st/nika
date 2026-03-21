# Nika LSP — Consolidated Gap Analysis

> **Date:** 2026-03-21
> **Source plans:** definitive-plan (Mar 18), v2-master-plan (Mar 19), v2-implementation-reference (Mar 19), LSP-PROGRESS.md, EXECUTE-LSP.md
> **Baseline verified:** 273 tests in nika-lsp-core, 7,050 tests in nika (1 failing), tower-lsp-server 0.23 confirmed

---

## 1. Crate Architecture Status

The 3-crate design is the central architectural decision. Here is where each crate stands.

| Crate | Planned | Exists | Status | Notes |
|-------|---------|:------:|--------|-------|
| **nika-core** | Lightweight AST + analysis (zero heavy deps) | Yes | Done | 40+ source files, binding/, ast/, catalogs/, source/ |
| **nika-lsp-core** | Protocol-agnostic handlers, WorldDatabase, error recovery | Yes | Partial | db.rs, position.rs, document.rs, parse/, analysis/context.rs, 6 handler stubs |
| **nika** (embedded LSP) | Thin tower-lsp shim delegating to nika-lsp-core | Yes | Not migrated | Still uses old handlers directly (10,186 LOC) |
| **nika-lsp** (standalone) | ~30 lines, thin wiring | Yes | Not migrated | Still has own handlers (4,009 LOC), not yet delegating to core |

### What exists in nika-lsp-core today (273 tests)

| Module | LOC | Tests | Content |
|--------|-----|:-----:|---------|
| db.rs (WorldDatabase, FileKey, FileSnapshot, Versioned) | 619 | 30 | Full — production-ready |
| position.rs (LineIndex, PositionIndex, SpanEntry, SpanKind) | ~600 | ~80 | Full — O(log n), proptest |
| document.rs (Rope-based Document) | ~200 | ~20 | Full — incremental edits |
| parse/mod.rs (TextRange, PartialWorkflow, PartialTask, PartialField) | 211 | 8 | Types done |
| parse/recovery.rs (RecoveryParser, tree-sitter-yaml) | 247 | ~20 | Full — incremental parsing, timeout protection |
| parse/bridge.rs (CST to PartialWorkflow extraction) | 1,370 | ~50 | Full — structure-only extraction |
| analysis/context.rs (CursorContext, 16 variants) | 1,320 | ~40 | Full — 3-strategy detection |
| handlers/completion.rs | 1,194 | ~20 | Full — ported from embedded, extended |
| handlers/hover.rs | 86 | 3 | Minimal — basic verb/field/root hover only |
| handlers/definition.rs | 58 | 4 | Minimal — text search, no AST-based |
| handlers/code_action.rs | 44 | 3 | Minimal — 2 actions only (schema, expand infer) |
| handlers/semantic_tokens.rs | 82 | 0 | Stub — types only, no implementation |
| handlers/symbols.rs | 109 | 0 | Stub — types only, no implementation |

### Key finding: nika-lsp-core handlers are NOT wired to either binary

Both `nika` (embedded) and `nika-lsp` (standalone) still use their own handler code. The nika-lsp-core handlers exist but are disconnected. The delegation layer (PR:handler-migration-wire) has not been built.

---

## 2. Feature Matrix: Planned vs Implemented

### Standard LSP Features (24 total per plan)

| # | Feature | Embedded | Standalone | nika-lsp-core | Plan Target | Gap |
|---|---------|:--------:|:----------:|:-------------:|-------------|-----|
| 1 | Completion | Yes (1,192 LOC) | Yes (725 LOC) | Yes (1,194 LOC) | Track A | Wiring missing |
| 2 | Hover | Yes (1,008 LOC) | Yes (480 LOC) | Minimal (86 LOC) | Track A | Needs full port + model_intel |
| 3 | Go-to-Definition | Yes (946 LOC) | Partial | Minimal (58 LOC) | Track A | Needs full port |
| 4 | Code Action | Yes (1,135 LOC) | No | Minimal (44 LOC) | Track A | 7 existing -> 2 in core |
| 5 | Semantic Tokens | Yes (1,181 LOC) | No | Stub (82 LOC) | Track A | Needs full port |
| 6 | Document Symbols | Yes (825 LOC) | No | Stub (109 LOC) | Track A | Needs full port |
| 7 | Diagnostics | Yes | Yes (213 LOC) | No | Track A | Not in core yet |
| 8 | **Inlay Hints** | No | No | No | Track A (PR:standard-features) | Not started |
| 9 | **Code Lens** | No | No | No | Track A (PR:standard-features) | Not started |
| 10 | **Rename** | No | No | No | Track A (PR:standard-features) | Not started |
| 11 | **References** | No | No | No | Track A (PR:standard-features) | Not started |
| 12 | **Folding** | No | No | No | Track A (PR:standard-features) | Not started |
| 13 | **Document Link** | No | No | No | Track A (PR:standard-features) | Not started |
| 14 | **Highlight** | No | No | No | Track A (PR:standard-features) | Not started |
| 15 | **Selection Range** | No | No | No | Track A (PR:standard-features) | Not started |
| 16 | **Workspace Symbol** | No | No | No | Track A (PR:standard-features) | Not started |
| 17 | **Linked Editing** | No | No | No | Track A (PR:standard-features) | Not started |
| 18 | **On-Type Formatting** | No | No | No | Track A (PR:standard-features) | Not started |
| 19 | **Document Color** | No | No | No | Track A (PR:standard-features) | Not started |
| 20 | **Signature Help** | No | No | No | Track A (PR:standard-features) | Not started |
| 21 | **Call Hierarchy** | No | No | No | Track A (PR:standard-features) | Not started |
| 22 | **Formatting** | No | No | No | Track E (PR:formatting) | Not started |
| 23 | **Selection Range** | No | No | No | Track A | Not started |
| 24 | **Workspace Diagnostics** | No | No | No | Track C (PR:cross-file) | Not started |

**Score: 7/24 standard features implemented (in embedded), 0/14 new features from plan started.**

### Novel Features (30 planned)

| # | Feature | Status | Track | Blocking Issue |
|---|---------|--------|-------|----------------|
| 1 | Model Intelligence (model_intel.rs) | **Done** (1,418 LOC, 65 tests) | C:cost-radar | In embedded LSP only, not in core |
| 2 | Error Recovery (tree-sitter bridge) | **Done** in nika-lsp-core | A:error-recovery | Exists but not connected to any handler |
| 3 | CursorContext (16-variant enum) | **Done** in nika-lsp-core | A:foundation | In analysis/context.rs |
| 4-30 | All other novel features | Not started | C/D/E | Blocked by Track A completion |

### Track Progress Summary

| Track | PRs Planned | PRs Done | PRs In Progress | PRs Not Started |
|-------|:-----------:|:--------:|:---------------:|:---------------:|
| **A: Foundation** | 7 | 3 (extract-ast, tower-lsp-upgrade, foundation) | 0 | 4 (error-recovery*, wire, delete, standard-features) |
| **B: Quick Wins** | 4 | 0 | 0 | 4 |
| **C: Intelligence** | 10 | 0 | 0 | 10 |
| **D: Magic** | 6 | 0 | 0 | 6 |
| **E: Ecosystem** | 7 | 0 | 0 | 7 |

*error-recovery: the parse/ module code exists in nika-lsp-core (recovery.rs + bridge.rs = 1,617 LOC with tests), but LSP-PROGRESS.md still lists it as "NEXT". The types and parser are implemented but may need the 50+ broken YAML fixture corpus and insta snapshot verification pass.*

---

## 3. Code Duplication (the original sin)

The plan's top priority was eliminating 65% duplication between embedded and standalone LSP. Current state:

| Component | Embedded (nika) | Standalone (nika-lsp) | Core (nika-lsp-core) | Duplication |
|-----------|:-:|:-:|:-:|:-:|
| Completion | 1,192 LOC | 725 LOC | 1,194 LOC | **Triple** — core exists but both originals remain |
| Hover | 1,008 LOC | 480 LOC | 86 LOC | Double (embedded + standalone) |
| Definition | 946 LOC | in backend.rs | 58 LOC | Double |
| Code Action | 1,135 LOC | None | 44 LOC | Single (only embedded) |
| Semantic Tokens | 1,181 LOC | None | 82 LOC | Single |
| Symbols | 825 LOC | None | 109 LOC | Single |
| Position/Conversion | 881 LOC | 427 LOC | ~600 LOC | Triple |
| AST Integration | in ast_index.rs (539 LOC) | 359 LOC | via WorldDatabase | Double |
| Document Store | 329 LOC | 159 LOC | ~200 LOC | Triple |

**Total duplicated LOC: ~5,500 LOC across 3 codebases.** The plan promised 0% duplication after Track A.

---

## 4. VS Code Extension Status

| Feature | Planned | Implemented | Gap |
|---------|---------|:-----------:|-----|
| LSP client (thin) | Yes | Yes | Done |
| TextMate grammar | Yes (with fixes) | Basic | Missing: multiline strings, `edges:`, `skills:`, `content:` |
| Snippets (7 verb-specific) | Track B | No | Not started |
| Commands (4: run, check, restart, new) | Track B | No | Not started |
| Keybindings (Ctrl+Shift+R/V) | Track B | No | Not started |
| Status bar item | Track B | No | Not started |
| Icons (light/dark) | Track B | No | Referenced in package.json but files likely missing |
| Settings (7 new) | Track B | 3 exist | Only server.path, extraArgs, trace.server |
| React Flow DAG webview | Track D | No | Not started |
| Notebook API | Track D | No | Not started |

**VS Code extension score: 2/10** (unchanged from plan baseline).

---

## 5. Competing Tools: LSP Landscape

### dbt (the gold standard for domain-specific LSPs)

dbt has the most mature workflow-engine LSP, powered by the dbt Fusion engine:

| Feature | dbt LSP | Nika LSP (today) |
|---------|:-------:|:----------------:|
| Go-to-definition (refs, macros, models, columns) | Yes | Partial (tasks only) |
| IntelliSense / Autocomplete | Yes (SQL functions, models, macros) | Yes (verbs, fields, providers) |
| Hover docs | Yes (tables, columns, types) | Yes (verbs, fields) |
| Live error detection | Yes (dbt errors + SQL errors, no warehouse) | Partial (parse errors only) |
| Rename (project-wide) | Yes | No |
| Compiled code preview | Yes | No |
| Lineage (column-level) | Yes | No (DAG view planned, Track D) |
| CTE previews | Yes | No |

**Key insight:** dbt's LSP validates both the DSL layer AND the target language (SQL). Nika could do the same: validate both the workflow YAML AND the template expressions / MCP tool schemas.

### Temporal

Temporal's VS Code extension is focused on **debugging**, not authoring:
- Replay debugging (download Event History, set breakpoints, step through)
- Workflow inspection (sidebar: activities, timers, signals)
- No YAML/authoring LSP features (Temporal uses code SDKs, not YAML)
- No completion, hover, or diagnostics for workflow definitions

**Relevance to Nika:** Temporal's time-travel debugging concept maps to Nika's planned Track D: PR:runtime-feedback (trace integration, replay). But Temporal has no equivalent of Nika's authoring-time intelligence.

### Prefect

- **No LSP.** Prefect is pure Python with decorators. No custom language, no need for a language server.
- VS Code experience = standard Python LSP (Pylance/Pyright)
- No YAML workflow definitions

### Kestra

- VS Code extension exists on Marketplace
- Uses **RedHat's generic YAML Language Server** with JSON Schema for validation
- Features: autocompletion via downloaded schema, embedded documentation webview
- **No custom LSP** -- relies entirely on JSON Schema validation through the generic YAML LSP
- No semantic understanding of workflow structure, DAG, or data flow

### GitHub Actions

- Custom language services library (`actions/languageservices` on GitHub)
- **Has a real LSP** with workflow-parser library for syntax validation
- Features: schema-based validation, some completion
- No semantic DAG analysis, no data flow tracking

### Summary: competitive positioning

```
LSP Maturity Scale (workflow engines, 2026-03-21)

dbt Fusion    ██████████████████████████  10/10  (gold standard)
GitHub Actions████████████░░░░░░░░░░░░░░   5/10  (schema + LSP)
Nika (today)  ████████░░░░░░░░░░░░░░░░░░   3/10  (7 features, no wiring)
Kestra        ██████░░░░░░░░░░░░░░░░░░░░   2/10  (generic YAML LSP)
Temporal      ████░░░░░░░░░░░░░░░░░░░░░░   1/10  (debug only)
Prefect       ░░░░░░░░░░░░░░░░░░░░░░░░░░   0/10  (no LSP)
```

**Nika's advantage:** None of these tools have AI-specific intelligence (prompt linting, cost estimation, model switching, token budget tracking). Track C features would be genuinely novel.

---

## 6. Priority Items for Next Session

Based on the gap analysis, the critical path is clear. The error-recovery code EXISTS but is not wired; handler migration has not begun.

### P0 — Immediate (next 1-2 sessions)

| Item | Why | Files |
|------|-----|-------|
| **Verify error-recovery completeness** | parse/bridge.rs + recovery.rs exist (1,617 LOC) but LSP-PROGRESS says "NEXT". Need broken YAML fixture corpus + insta snapshots. | `nika-lsp-core/src/parse/bridge.rs`, `nika-lsp-core/src/parse/recovery.rs` |
| **Port remaining handlers to nika-lsp-core** | hover (86 LOC vs 1,008 needed), definition (58 vs 946), code_action (44 vs 1,135), semantic_tokens (stub), symbols (stub) | `nika-lsp-core/src/handlers/*.rs` |

### P1 — Critical path (next 3-5 sessions)

| Item | Why | Files |
|------|-----|-------|
| **Build delegation layer** (PR:handler-migration-wire) | Both binaries must delegate to nika-lsp-core. This is THE blocker for everything. | `nika/src/lsp/server.rs`, `nika-lsp/src/backend.rs` |
| **Move model_intel.rs to nika-lsp-core** | 1,418 LOC in embedded LSP, planned for core. Needed for hover, code actions, cost radar. | `nika/src/lsp/model_intel.rs` -> `nika-lsp-core/src/knowledge/model_intel.rs` |
| **Wire diagnostics pipeline** | 0% of ~40 surfaceable error codes are published as LSP diagnostics. The analyzer produces errors but they never reach the editor. | `nika-lsp-core/src/analysis/diagnostics.rs` (new) |

### P2 — High value, parallelizable

| Item | Why | Files |
|------|-----|-------|
| **VS Code extension polish** (Track B) | Can be done independently. Snippets, commands, grammar fixes. | `editors/vscode/*` |
| **Broken YAML fixture corpus** | 50+ files needed for error recovery validation. Only ~9 exist today. | `tests/fixtures/broken/*.nika.yaml` (new) |
| **Criterion benchmarks** | Infrastructure dormant. Need completion/hover/parse benchmarks. | `tests/benchmarks/micro_benchmarks.rs` |

### P3 — After Track A completes

| Item | Why | Effort |
|------|-----|--------|
| Delete old handler code (19 files, ~5,000 LOC) | Only after delegation proven | 1 session |
| 14 new standard features (inlay hints, code lens, rename...) | Depends on core handlers working | 3 sessions |
| Track C intelligence features (10 PRs) | Depends on Track A complete | 10 sessions |

---

## 7. Specific Files That Need Changes

### Must change (Track A critical path)

| File | Change | LOC estimate |
|------|--------|:------------:|
| `nika-lsp-core/src/handlers/hover.rs` | Port full hover from embedded (1,008 LOC) + model_intel integration | +800 |
| `nika-lsp-core/src/handlers/definition.rs` | Port full go-to-def from embedded (946 LOC) | +700 |
| `nika-lsp-core/src/handlers/code_action.rs` | Port 7 existing code actions from embedded (1,135 LOC) | +900 |
| `nika-lsp-core/src/handlers/semantic_tokens.rs` | Port from embedded (1,181 LOC) | +900 |
| `nika-lsp-core/src/handlers/symbols.rs` | Port from embedded (825 LOC) | +600 |
| `nika-lsp-core/src/handlers/diagnostics.rs` | New — wire AnalyzeError to LSP diagnostics | +300 |
| `nika-lsp-core/src/knowledge/model_intel.rs` | Move from nika/src/lsp/model_intel.rs | +1,418 (move) |
| `nika-lsp-core/src/knowledge/schema.rs` | Verb/field/transform knowledge catalog | +500 |
| `nika-lsp-core/src/knowledge/mcp_tools.rs` | Merge from nika-lsp/src/mcp_discovery.rs + 18 nika:* tools | +400 |
| `nika/src/lsp/server.rs` | Rewrite to delegate to nika-lsp-core | ~200 (rewrite) |
| `nika-lsp/src/backend.rs` | Rewrite to delegate to nika-lsp-core | ~100 (rewrite) |

### Must create (new files)

| File | Purpose |
|------|---------|
| `nika-lsp-core/src/knowledge/mod.rs` | Knowledge module (schema, model_intel, mcp_tools, providers, transforms) |
| `nika-lsp-core/src/analysis/diagnostics.rs` | NikaError -> LSP Diagnostic mapping |
| `nika-lsp-core/src/analysis/template.rs` | Template validation (from standalone) |
| `tests/fixtures/broken/*.nika.yaml` | 50+ broken YAML files for error recovery |

### Must delete (after migration proven)

| File | LOC | Reason |
|------|:---:|--------|
| `nika/src/lsp/handlers/completion.rs` | 1,192 | Replaced by core |
| `nika/src/lsp/handlers/hover.rs` | 1,008 | Replaced by core |
| `nika/src/lsp/handlers/definition.rs` | 946 | Replaced by core |
| `nika/src/lsp/handlers/code_action.rs` | 1,135 | Replaced by core |
| `nika/src/lsp/handlers/semantic_tokens.rs` | 1,181 | Replaced by core |
| `nika/src/lsp/handlers/symbols.rs` | 825 | Replaced by core |
| `nika/src/lsp/ast_index.rs` | 539 | Replaced by WorldDatabase |
| `nika/src/lsp/document_store.rs` | 329 | Replaced by WorldDatabase |
| `nika/src/lsp/conversion.rs` | 881 | Replaced by core position.rs |
| `nika/src/lsp/utils.rs` | 92 | Absorbed |
| `nika-lsp/src/completion.rs` | 725 | Replaced by core |
| `nika-lsp/src/hover.rs` | 480 | Replaced by core |
| `nika-lsp/src/diagnostics.rs` | 213 | Replaced by core |
| `nika-lsp/src/node_context.rs` | 586 | Replaced by core analysis/context.rs |
| `nika-lsp/src/mcp_discovery.rs` | 239 | Replaced by core knowledge |
| `nika-lsp/src/template_validation.rs` | 355 | Replaced by core analysis/template.rs |
| `nika-lsp/src/position.rs` | 426 | Replaced by core position.rs |
| `nika-lsp/src/document.rs` | 159 | Replaced by core document.rs |
| `nika-lsp/src/ast_integration.rs` | 359 | Replaced by WorldDatabase |
| **Total to delete** | **~10,670** | |

---

## 8. Risk Register

| Risk | Impact | Mitigation |
|------|--------|------------|
| Handler migration breaks existing LSP users | High | Dual-write: keep old handlers during transition, delete only after 1 week |
| tower-lsp-server 0.23 ls-types API instability (0.0.x) | Medium | Already committed to; pin exact version, wrap in own types if needed |
| Error recovery false positives on valid YAML | High | STRUCTURE-ONLY invariant in bridge.rs; never trust ts-yaml values |
| nika-lsp-core completion.rs diverges from embedded | Medium | Delete embedded ASAP after wiring proven |
| 1 failing test in main (display::tests::tokens_above_10k_are_integer_k) | Low | Fix before starting Track A work |

---

## 9. Metrics Dashboard

| Metric | Plan Target | Current | Gap |
|--------|:-----------:|:-------:|:---:|
| Standard LSP features | 21/24 (Track A) | 7/24 | -14 |
| Code duplication | 0% | ~65% | -65pp |
| Error code surfacing | 50% (Track B) | 0% | -50pp |
| Completion latency | <50ms | ~200ms | 4x slower |
| Broken YAML support | 100% | Code exists, not wired | Partial |
| Novel features | 16 (Track C) | 1 (model_intel) | -15 |
| nika-lsp-core tests | 400+ (Track A exit) | 273 | -127 |
| Total tests | 8,500+ (Track A exit) | 7,050 | -1,450 |
| VS Code extension score | 6/10 (Track A+B) | 2/10 | -4 |
| Broken YAML fixtures | 50+ | ~9 | -41 |
| Criterion benchmarks active | 5+ | 0 | -5 |

---

## 10. Recommended Session Sequence (updated)

```
Session N+1:  Fix failing test + verify error-recovery code completeness     ~1h
Session N+2:  Port hover + definition + code_action to nika-lsp-core          ~4h
Session N+3:  Port semantic_tokens + symbols + move model_intel to core       ~3h
Session N+4:  Build delegation layer (server.rs + backend.rs -> core)          ~4h  <- HUMAN REVIEW
Session N+5:  Wire diagnostics pipeline (AnalyzeError -> LSP diagnostics)     ~2h
Session N+6:  Delete old handlers (19 files, verify all tests pass)            ~1h
Session N+7:  VS Code extension polish (snippets, commands, grammar)           ~2h  (parallel)
Session N+8:  Standard features batch 1 (folding, highlight, selection)        ~3h
Session N+9:  Standard features batch 2 (inlay hints, code lens, rename)       ~4h
Session N+10: Broken YAML fixture corpus (50+ files) + benchmarks              ~2h  (parallel)
```

**Estimated total to Track A exit: ~10 sessions, ~26 hours.**

---

## Sources: Competitive Research

- [dbt LSP features](https://docs.getdbt.com/docs/dbt-extension-features)
- [dbt about LSP](https://docs.getdbt.com/docs/about-dbt-lsp)
- [Understanding LSP (dbt Labs blog)](https://www.getdbt.com/blog/language-server-protocol)
- [Temporal VS Code extension](https://temporal.io/blog/temporal-for-vs-code)
- [Temporal Replay 2025 announcements](https://temporal.io/blog/replay-2025-product-announcements)
- [Kestra VS Code extension](https://marketplace.visualstudio.com/items?itemName=kestra-io.kestra)
- [Kestra vscode-kestra GitHub](https://github.com/kestra-io/vscode-kestra)
- [GitHub Actions language services](https://github.com/actions/languageservices)
- [RedHat YAML Language Server](https://github.com/redhat-developer/yaml-language-server)
- [Prefect Open Source](https://www.prefect.io/prefect/open-source)
