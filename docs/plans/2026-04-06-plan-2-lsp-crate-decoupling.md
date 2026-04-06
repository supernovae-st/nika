# Plan 2: LSP & Crate Decoupling — Execution Plan

> **Date**: 2026-04-06 | **Version**: v0.73.0 | **Target**: v0.75.0
> **Effort**: 2-3 days | **12 tasks** | **4 phases**
> **Prerequisites**: All 10,190+ tests GREEN, no uncommitted AST changes

## Executive Summary

Decouple `nika-lsp` from `nika-engine` to slash compile time from 90s to 15s.
Activate 6 hidden LSP features. Unblock IDE development (Plan 1 Phase 6).

```
BEFORE: nika-lsp -> nika-engine (157K LOC, reqwest, rig-core, petgraph) = 90s compile
AFTER:  nika-lsp -> nika-core (30K LOC, pure types, zero I/O) = 15s compile
```

## Current Architecture

```
                    nika-vault (leaf)
                        |
           +------------+
           |            |
       nika-core      nika-daemon
       (types, AST,   (services, IPC,
        catalogs)      EventBus)
           |              |
       nika-event      nika-mcp
       (58 EventKind)  (4 tools)
           |              |
       nika-display    nika-lsp-core (13 handlers, 745+ tests)
           |              |
       nika-engine <------+ (re-exports nika-core types)
       (runtime, DAG,     |
        providers,      nika-lsp  <-- DEPENDS ON ENGINE (WRONG!)
        157K LOC)
```

**The problem**: nika-lsp imports types like `nika_engine::ast::raw::*`,
`nika_engine::ast::analyzer::*`, `nika_engine::source::*`. But ALL of these
are re-exports from nika-core. The engine dependency pulls in the entire
runtime (reqwest, rig-core, petgraph, provider HTTP clients) — 157K LOC of
code that the LSP never uses.

## Target Architecture

```
       nika-core (30K LOC, pure types)
           |
    +------+-------+
    |              |
nika-lsp-core   nika-engine (157K LOC, runtime)
    |              |
nika-lsp         nika (binary)
(15s compile)    (60s compile)
```

**Key change**: nika-lsp depends ONLY on nika-core + nika-lsp-core.
No nika-engine. No reqwest. No rig-core. No providers.

---

## Phase 1: Audit Import Sources (30 min)

### Task 1.1: Map All nika-engine Imports in nika-lsp

Run:
```bash
cd tools && grep -rn "nika_engine::" nika-lsp/src/ | sort
```

Expected findings:
- `nika_engine::ast::raw::*` -> available in `nika_core::ast::raw::*`
- `nika_engine::ast::analyzer::*` -> available in `nika_core::ast::analyzer::*`
- `nika_engine::ast::analyzed::*` -> available in `nika_core::ast::analyzed::*`
- `nika_engine::source::*` -> available in `nika_core::source::*`

**Key files to check**:
- `tools/nika-lsp/src/backend.rs`
- `tools/nika-lsp/src/diagnostics.rs`
- `tools/nika-lsp/src/ast_integration.rs`

### Task 1.2: Verify All Types Exist in nika-core

For each import found, verify the type/module exists directly in nika-core:

```bash
# For each module path found in Task 1.1:
cd tools && grep -rn "pub mod raw" nika-core/src/ast/
cd tools && grep -rn "pub mod analyzer" nika-core/src/ast/
cd tools && grep -rn "pub mod analyzed" nika-core/src/ast/
cd tools && grep -rn "pub mod source" nika-core/src/
```

If any type is NOT in nika-core, it needs to be moved or the import rethought.

```
Commit: chore(lsp): audit nika-engine imports — all re-exports from nika-core
```

---

## Phase 2: Rewire Imports (2-4 hours)

### Task 2.1: Replace nika_engine:: with nika_core:: in nika-lsp

**Bulk replacement** (file by file, not sed — verify each):

```
tools/nika-lsp/src/backend.rs:
  nika_engine::ast::raw::  ->  nika_core::ast::raw::
  nika_engine::ast::analyzer::  ->  nika_core::ast::analyzer::
  nika_engine::ast::analyzed::  ->  nika_core::ast::analyzed::
  nika_engine::source::  ->  nika_core::source::

tools/nika-lsp/src/diagnostics.rs:
  (same pattern)

tools/nika-lsp/src/ast_integration.rs:
  (same pattern)
```

**Do NOT use bulk sed** — review each import to catch edge cases.

### Task 2.2: Update Cargo.toml

Remove nika-engine from `tools/nika-lsp/Cargo.toml`:

```toml
# BEFORE:
[dependencies]
nika-core = { path = "../nika-core" }
nika-engine = { path = "../nika" }  # DELETE THIS LINE
nika-lsp-core = { path = "../nika-lsp-core" }

# AFTER:
[dependencies]
nika-core = { path = "../nika-core" }
nika-lsp-core = { path = "../nika-lsp-core" }
```

### Task 2.3: Verify Compilation

```bash
cd tools && cargo check -p nika-lsp
```

If this fails, some type is NOT in nika-core. Options:
1. Move the type to nika-core (preferred)
2. Add a thin bridge in nika-lsp-core
3. Re-export from a lighter crate

### Task 2.4: Run Full Test Suite

```bash
cd tools && cargo test --workspace --lib
cd tools && cargo test -p nika-lsp --test e2e_harness -- --ignored
```

```
Commit: refactor(lsp): replace nika-engine imports with nika-core — drop engine dependency
```

---

## Phase 3: Verify Boundary (30 min)

### Task 3.1: Dependency Tree Verification

```bash
cd tools && cargo tree -p nika-lsp --no-dedupe | grep -c "nika-engine"
# MUST BE 0
```

If > 0, a transitive dependency still pulls in nika-engine.
Check: does nika-lsp-core depend on nika-engine?

```bash
cd tools && cargo tree -p nika-lsp-core --no-dedupe | grep -c "nika-engine"
# MUST ALSO BE 0
```

### Task 3.2: Compile Time Measurement

```bash
cd tools && cargo clean -p nika-lsp
cd tools && time cargo build -p nika-lsp --timings
# Target: ~15s (was ~90s)
```

Save the timing report for comparison.

### Task 3.3: Binary Size Check

```bash
ls -la target/debug/nika-lsp
# Should be significantly smaller without nika-engine
```

```
Commit: test(lsp): verify nika-engine boundary — zero transitive deps, 15s compile
```

---

## Phase 4: Activate Hidden LSP Features (1-2 hours)

These features are already CODED in nika-lsp-core but not declared in the
VS Code extension's package.json. This is also Task 0.8 in Plan 1.

### Task 4.1: Declare Missing Capabilities in package.json

**6 features coded but invisible**:

| LSP Capability | Implemented In | package.json Status |
|---|---|---|
| Code Lens (4 types) | backend.rs:235-237 | MISSING |
| Inlay Hints | backend.rs:239 | MISSING |
| Semantic Tokens (7 types) | backend.rs:210-229 | MISSING |
| Document Links | backend.rs:248-251 | MISSING |
| Folding Ranges | backend.rs:253 | MISSING |
| Rename (with prepare) | backend.rs:243-246 | No keybinding |

**Add to `editors/vscode/package.json`**:

```json
"semanticTokenScopes": [
  {
    "language": "nika",
    "scopes": {
      "keyword": ["keyword.control.nika"],
      "macro": ["entity.name.function.verb.nika"],
      "property": ["variable.other.property.nika"],
      "variable": ["variable.other.template.nika"],
      "type": ["entity.name.type.nika"],
      "comment": ["comment.nika"]
    }
  }
],
"configuration": {
  "properties": {
    "nika.inlayHints.enabled": {
      "type": "boolean",
      "default": true,
      "description": "Show inlay hints (cost estimates, durations, dependencies)"
    },
    "nika.codeLens.enabled": {
      "type": "boolean",
      "default": true,
      "description": "Show code lens (Run, Validate, task count)"
    }
  }
}
```

### Task 4.2: Add extensionKind for Remote Support

```json
"extensionKind": ["workspace"]
```

This enables Remote SSH, WSL, Codespaces, and dev containers.

### Task 4.3: Add Keybindings

```json
"keybindings": [
  {
    "command": "nika.runWorkflow",
    "key": "ctrl+shift+r",
    "mac": "cmd+shift+r",
    "when": "resourceLangId == 'nika'"
  },
  {
    "command": "nika.checkWorkflow",
    "key": "ctrl+shift+k",
    "mac": "cmd+shift+k",
    "when": "resourceLangId == 'nika'"
  }
]
```

```
Commit: feat(vscode): declare code lens, inlay hints, semantic tokens, keybindings
```

---

## LSP Feature Status (12/28 done, 43%)

### Done (12 features)

1. Diagnostics (push) — NIKA-SYNTAX from tree-sitter + schema errors
2. Completion — verbs, `with:` keys, providers, models
3. Hover — verb docs, template resolution, binding values
4. Go to Definition — 7 targets (alias, template, agent from:, MCP, skills)
5. Code Actions — 9 quick fixes (fuzzy "did you mean?", add task id, expand)
6. Semantic Tokens — 7 types (keyword, variable, string, function, property)
7. Document Symbols — hierarchical outline
8. Inlay Hints — `<- step output`, `2 deps`, `seconds` units
9. Code Lens — "Validate", "Run Workflow", "N tasks"
10. Document Links — clickable URLs
11. Folding Ranges — fold tasks, `with:` blocks
12. References — find usages of task ID in depends_on, $binding, {{with.alias}}

### Remaining (16 features, P1-P3)

| Feature | Priority | Effort | Notes |
|---------|----------|--------|-------|
| Rename | P1 | Medium | Prepare support exists |
| Formatting | P1 | Medium | YAML formatting |
| Diagnostic Related Info | P1 | Low | Link errors to causes |
| Inlay Hints (AST-driven) | P1 | Medium | Replace text-based |
| CodeLens (AST-driven) | P1 | Medium | Replace text-based |
| Completion Item Resolve | P2 | Low | Lazy docs loading |
| Signature Help | P2 | Medium | Parameter hints |
| Workspace Symbols | P2 | Medium | Cross-file search |
| File Watchers | P2 | Low | Auto-refresh |
| Selection Ranges | P2 | Low | Smart select |
| Cost Estimate Inlay | P2 | Medium | Daemon integration |
| Cross-file Definition | P2 | High | include: imports |
| On-type Formatting | P3 | Medium | - |
| Document Highlight | P3 | Low | - |
| Call Hierarchy | P3 | Medium | - |

---

## Gotchas

### Gotcha 1: nika-lsp-core May Also Depend on nika-engine
Check `tools/nika-lsp-core/Cargo.toml` — if it depends on nika-engine,
the decoupling won't help. Fix nika-lsp-core FIRST.

### Gotcha 2: AST Types May Have Diverged
nika had FULL COPIES of nika-core's AST modules. Some types might exist
only in nika-engine's extended AST, not in nika-core. Check each import.

### Gotcha 3: Source Span Types
`nika_engine::source::SourceSpan` might have extra methods not in nika-core.
Verify the full API surface.

### Gotcha 4: Feature Flags
Some AST types may be gated behind feature flags in nika-core.
Ensure nika-lsp enables the right features:
```toml
nika-core = { path = "../nika-core", features = ["analysis"] }
```

### Gotcha 5: Uncommitted AST Changes
There are currently uncommitted changes in:
- `tools/nika-core/src/ast/analyzer/analyze.rs`
- `tools/nika-core/src/ast/analyzed/workflow.rs`
- `tools/nika-core/src/ast/raw/parser.rs`

**Commit or stash these BEFORE starting this plan.**

---

## Verification Checklist

```bash
# 1. Zero nika-engine dependency
cargo tree -p nika-lsp --no-dedupe | grep -c "nika-engine"  # 0
cargo tree -p nika-lsp-core --no-dedupe | grep -c "nika-engine"  # 0

# 2. All tests pass
cd tools && cargo test --workspace --lib  # 10,190+ tests

# 3. E2E harness
cargo test -p nika-lsp --test e2e_harness -- --ignored

# 4. Compile time
cargo clean -p nika-lsp && time cargo build -p nika-lsp  # ~15s target

# 5. Extension compiles
cd editors/vscode && npm run compile

# 6. Manual: open .nika.yaml, verify all 12 features visible
```

---

## Summary

| Phase | Tasks | Time | Deliverable |
|-------|-------|------|-------------|
| 1 | 2 | 30 min | Import audit — confirm all re-exports |
| 2 | 4 | 2-4 hours | Rewire imports + drop nika-engine dep |
| 3 | 3 | 30 min | Verify boundary — 0 transitive deps, 15s |
| 4 | 3 | 1-2 hours | Activate 6 hidden features in package.json |
| **Total** | **12** | **1-2 days** | **LSP compiles in 15s, all features visible** |
