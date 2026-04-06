# Handoff A: LSP & Crate Decoupling

> **Copy-paste this into a new Claude Code session to execute.**
> Estimated time: 1-2 days | Tests baseline: 10,315

## Mission

Drop nika-engine dependency from nika-lsp. Compile time 90s → 15s.
Activate 6 hidden LSP features in the VS Code extension.

## Context

nika-lsp currently depends on nika-engine (157K LOC, reqwest, rig-core, petgraph).
But it ONLY uses re-exports from nika-core (30K LOC, pure types, zero I/O).
Removing this dependency slashes compile time by 6x.

Recent model/provider refactor SIMPLIFIED the AST:
- base_url removed from RawWorkflow/RawTask/AnalyzedWorkflow/AnalyzedTask
- Model/provider mismatch warning removed from analyzer
- All core types stable and in nika-core

## Pre-Flight

```bash
cd /Users/thibaut/dev/supernovae/nika
git status  # Should be clean
cd tools && cargo test --workspace --lib  # 10,315+ tests
```

## Mandatory Skills

Load these skills BEFORE starting:
- `test-driven-development`
- `verification-before-completion`
- `spn-rust:rust-core`

## Plan

Read the full plan first: `docs/plans/2026-04-06-plan-2-lsp-crate-decoupling.md`

### Phase 1: Audit (30 min)

**Task 1.1**: Map all nika_engine imports in nika-lsp:
```bash
cd tools && grep -rn "nika_engine::" nika-lsp/src/ | sort
```

**Task 1.2**: For each import, verify the type exists in nika-core:
```bash
cd tools && grep -rn "pub mod raw" nika-core/src/ast/
cd tools && grep -rn "pub mod analyzer" nika-core/src/ast/
cd tools && grep -rn "pub mod source" nika-core/src/
```

Also check nika-lsp-core:
```bash
cd tools && grep -rn "nika_engine::" nika-lsp-core/src/ | sort
# If nika-lsp-core depends on engine, fix it FIRST
```

### Phase 2: Rewire (2-4 hours)

**Task 2.1**: Replace ALL `nika_engine::` with `nika_core::` in nika-lsp.
Do NOT use bulk sed. Review each import individually.

Key files:
- `tools/nika-lsp/src/backend.rs`
- `tools/nika-lsp/src/diagnostics.rs`
- `tools/nika-lsp/src/ast_integration.rs`

**Task 2.2**: Remove nika-engine from `tools/nika-lsp/Cargo.toml`:
```toml
# DELETE this line:
nika-engine = { workspace = true }
```

**Task 2.3**: Compile check:
```bash
cd tools && cargo check -p nika-lsp
```

If it fails: some type is only in nika-engine. Options:
1. Move it to nika-core (preferred)
2. Add thin re-export in nika-lsp-core
3. If it's a runtime type the LSP shouldn't use, find the right abstraction

**Task 2.4**: Full test suite:
```bash
cd tools && cargo test --workspace --lib
cd tools && cargo test -p nika-lsp --test e2e_harness -- --ignored
```

Commit:
```
refactor(lsp): drop nika-engine dependency — imports rewired to nika-core

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

### Phase 3: Verify Boundary (30 min)

**Task 3.1**: Zero engine transitive deps:
```bash
cd tools && cargo tree -p nika-lsp --no-dedupe | grep -c "nika-engine"  # MUST BE 0
cd tools && cargo tree -p nika-lsp-core --no-dedupe | grep -c "nika-engine"  # MUST BE 0
```

**Task 3.2**: Compile time:
```bash
cd tools && cargo clean -p nika-lsp
time cargo build -p nika-lsp  # Target: ~15s
```

**Task 3.3**: Binary size (should be much smaller):
```bash
ls -lh target/debug/nika-lsp
```

### Phase 4: Activate Hidden Features (1-2 hours)

**Task 4.1**: Edit `editors/vscode/package.json`. Add under `contributes`:

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
]
```

Add to configuration properties:
```json
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
```

**Task 4.2**: Add to root of package.json:
```json
"extensionKind": ["workspace"]
```

**Task 4.3**: Add keybindings:
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

Commit:
```
feat(vscode): activate code lens, inlay hints, semantic tokens, keybindings

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

## Final Verification

```bash
# Rust
cd tools && cargo test --workspace --lib  # 10,315+ tests
cargo tree -p nika-lsp --no-dedupe | grep -c "nika-engine"  # 0
cargo build -p nika-lsp
cargo test -p nika-lsp --test e2e_harness -- --ignored

# TypeScript
cd editors/vscode && npm ci && npm run compile
```

## Gotchas

1. **nika-lsp-core might also depend on nika-engine** — check and fix FIRST
2. **Some AST types gated behind features** — add `features = ["analysis"]` if needed
3. **Source span types** — verify full API surface available in nika-core
4. **Always --lib** — avoid keychain popups on macOS

## Success Criteria

- [ ] `cargo tree -p nika-lsp | grep nika-engine` returns 0
- [ ] `cargo build -p nika-lsp` completes in < 20s
- [ ] All 10,315+ tests pass
- [ ] `cd editors/vscode && npm run compile` succeeds
- [ ] 6 features visible in VS Code (code lens, inlay hints, semantic tokens, folding, doc links, rename)
