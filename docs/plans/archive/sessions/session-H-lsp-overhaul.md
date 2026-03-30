# Session H: LSP Overhaul (~6-8h, split across 2-3 sittings)

## Context
Nika workflow engine. Workspace: `tools/` (12 Rust crates). Main branch, 8600+ tests.
Source plan: `docs/plans/2026-03-27-lsp-overhaul.md` -- READ IT FIRST.
Dev reference: `tools/nika/CLAUDE.md` for crate layout.

LSP crates:
- `nika-lsp` (binary, `tools/nika-lsp/src/backend.rs` 1035 LOC)
- `nika-lsp-core` (handlers, `tools/nika-lsp-core/src/handlers/` 6372 LOC across 13 files)
- `nika-core` (parser + analyzer, shared with engine)

## Mission: From broken editor experience to best-in-class YAML workflow IDE

The LSP has 14 features implemented (6372 LOC, 422+ tests) but the user experience is broken:
VS Code extension stale at v0.42 vs binary v0.50, NIKA-163 (UnknownField) defined but never used,
task-level unknown keys silently ignored when verb IS present, `template_validation.rs` crashes on
malformed YAML, `nika check` runs 7 validation phases but LSP runs only 3.

### Methodology
For EVERY fix: read code -> write failing test -> fix -> verify -> commit.
`cargo test --workspace --lib` (always --lib to avoid keychain popups).
Extension changes: `cd editors/vscode && npm run compile`.
1 fix = 1 commit. Conventional commits with co-authors.

---

## PART 1: Extension Emergency Fix (Layer 0)

### Bug 1: VS Code extension stale at v0.42
**File**: `editors/vscode/package.json:6`
**Problem**: `"version": "0.42.0"` while binary is v0.50.0. VSCE_PAT likely expired in CI.
**Fix**:
1. Update `editors/vscode/package.json` version to `"0.50.0"`
2. Renew VSCE_PAT in Azure DevOps (manual, portal step)
3. Update GitHub Actions secret `VSCE_PAT`
4. Also verify/renew `OVSX_PAT` for Cursor/VSCodium
**Test**: `cd editors/vscode && npm install && npm run compile && npx vsce package` -- produces `.vsix`
**Commit**: `fix(lsp): sync VS Code extension version to 0.50.0`

### Bug 2: Extension activation robustness
**File**: `editors/vscode/src/extension.ts`
**Problem**: Commands register AFTER async binary discovery. If `globalStorageUri.fsPath` crashes,
commands never register. User sees "command not found" on Cmd+Shift+P.
**Fix**:
1. Move ALL `registerCommand()` calls to the TOP of `activate()`, before async binary discovery
2. Wrap `globalStorageUri.fsPath` in try/catch with fallback to temp dir
3. Add output channel logging for activation errors
**Test**: Open `.nika.yaml` -> Cmd+Shift+P -> "Nika" -> all 5 commands visible even if binary missing
**Commit**: `fix(lsp): register commands before async binary discovery`

### Bug 3: files.associations + configurationDefaults
**File**: `editors/vscode/package.json`
**Problem**: VS Code chicken-and-egg: `onLanguage:nika` needs language recognized first.
**Fix**: Add `configurationDefaults` with `files.associations` for `*.nika.yaml` -> `nika`
**Commit**: `fix(lsp): add files.associations for .nika.yaml auto-detection`

### Bug 4: template_validation.rs crash on malformed YAML
**File**: `tools/nika-lsp-core/src/` (or `tools/nika-lsp/src/template_validation.rs`)
**Problem**: `.unwrap()` on `raw::parse()` -- crashes entire LSP process on malformed YAML
**Fix**: Replace `.unwrap()` with `match` + `return vec![]` on parse error. Add `tracing::warn`.
**Test**: Parse malformed YAML -> returns empty diagnostics, no panic
**Commit**: `fix(lsp): handle malformed YAML in template validation without crash`

---

## PART 2: Parser Validation -- NIKA-163 (Layer 1)

### Bug 5: Unknown workflow-level key detection
**File**: `tools/nika-core/src/ast/raw/parser.rs` (3372 LOC)
**Problem**: `tsks:`, `workfow:`, `proovider:` are silently ignored. Parser uses pull-based lookup
(`get_node("tasks")`) instead of whitelist validation. `ParseErrorKind::UnknownField` (NIKA-163) exists
but is NEVER used in the codebase.
**Fix**: After parsing all workflow fields (before `Ok(workflow)` return), iterate the YAML mapping
keys and validate each against a known list. Add Levenshtein "did you mean" suggestions.
**Known workflow keys**: `schema`, `workflow`, `description`, `provider`, `model`, `base_url`,
`mcp`, `pkg`, `context`, `imports`, `inputs`, `artifacts`, `log`, `agents`, `skills`, `tasks`
**Tests**:
- `tsks:` -> NIKA-163 with "did you mean 'tasks'?"
- `proovider:` -> NIKA-163 with "did you mean 'provider'?"
- Valid workflow -> zero errors (regression)
- Unknown key not similar to anything -> lists all known keys
**Commit**: `feat(parser): detect unknown workflow-level keys (NIKA-163)`

### Bug 6: Task-level unknown key detection (BUG)
**File**: `tools/nika-core/src/ast/raw/parser.rs` (`parse_action` function area)
**Problem**: The unrecognized-key check only runs in the "no verb found" branch. If a task has
`exec:` + `foobar:`, the `foobar:` key is silently ignored because verb parsing returns early.
**Fix**: Extract known task keys validation into a separate function `validate_task_keys()`. Call it
from `parse_task()` AFTER action parsing succeeds, regardless of verb.
**Known task keys**: id, description, provider, model, base_url, with, depends_on, output,
for_each, as, retry, decompose, structured, artifact, log, concurrency, fail_fast, timeout,
+ the 5 verb keys (infer, exec, fetch, invoke, agent)
**Tests**:
- `exec: "echo" + dependson: [a]` -> NIKA-163 "did you mean 'depends_on'?"
- `exec: "echo" + foobar: "baz"` -> NIKA-163 with unknown key error
- Valid task with all known keys -> zero errors (regression)
**Commit**: `fix(parser): detect unknown task keys even when verb is present`

### Bug 7: Empty tasks array warning
**File**: `tools/nika-core/src/ast/analyzer/analyze.rs`
**Problem**: Workflow with zero tasks parses successfully but is useless.
**Fix**: After task analysis loop, add warning for empty tasks. Add warning for LLM verbs
without `provider:` or `model:`.
**Test**: Empty tasks -> warning diagnostic
**Commit**: `feat(parser): warn on empty tasks array`

### Bug 8: Comprehensive parser validation tests
8 tests minimum covering all NIKA-163 paths, misspellings, regressions.
**Commit**: `test(parser): comprehensive NIKA-163 unknown field validation`

---

## PART 3: Validation Parity (Layer 2)

### Bug 9: LSP missing JSON Schema validation (nika check Phase 1)
**File**: `tools/nika-lsp/src/diagnostics.rs` (or wherever `validate_document` lives)
**Problem**: `nika check` runs JSON Schema validation first. LSP skips it entirely.
**Fix**: Add schema validation to `validate_document()` BEFORE raw parse.
**Test**: Workflow with structural schema violation -> LSP diagnostic with correct span
**Commit**: `feat(lsp): add JSON Schema validation parity with nika check`

### Bug 10: LSP missing imports/include path validation
**File**: `tools/nika-lsp/src/diagnostics.rs`
**Problem**: `imports: [./nonexistent.nika.yaml]` produces no LSP error.
**Fix**: After parse, check if `imports:` paths exist on disk.
**Test**: Non-existent import path -> warning diagnostic
**Commit**: `feat(lsp): validate import file paths in LSP`

### Bug 11: LSP missing provider key warnings
**File**: `tools/nika-lsp/src/diagnostics.rs`
**Problem**: `nika check` warns when provider API keys are missing. LSP does not.
**Fix**: Check `provider:` field value against env vars (`ANTHROPIC_API_KEY` etc.).
If missing -> Warning diagnostic (not Error, key might be in daemon keychain).
**Test**: `provider: anthropic` without `ANTHROPIC_API_KEY` -> warning
**Commit**: `feat(lsp): warn on missing provider API keys`

### Bug 12: Diagnostic tests for all error kinds (13+ tests)
Each of 9 `AnalyzeErrorKind` + 4 `ParseErrorKind` variants -> verify correct NIKA-XXX code + severity.
**Commit**: `test(lsp): diagnostic tests for all error kinds`

### Bug 13: Template validation tests (5 tests)
`{{with.undefined}}` -> diagnostic. Valid templates -> no diagnostic. etc.
**Commit**: `test(lsp): template validation edge cases`

---

## PART 4: E2E Test Infrastructure (Layer 3)

### Task 14: Create stdio JSON-RPC test harness
**File**: `tools/nika-lsp/tests/harness.rs` (NEW)
**Design**: `LspTestClient` struct wrapping spawned `nika lsp` child process.
`send_request(method, params)` -> JSON-RPC over stdin.
`read_notification()` -> parse from stdout.
`expect_diagnostics(uri)` -> wait for `publishDiagnostics`.
**Commit**: `test(lsp): create stdio JSON-RPC test harness`

### Task 15: E2E protocol tests (6 tests)
1. `test_initialize_handshake` -- capabilities response has all 14 features
2. `test_didopen_publishes_diagnostics` -- open file with error -> diagnostic notification
3. `test_completion_returns_items` -- request completion -> verb items
4. `test_hover_returns_docs` -- hover over verb -> markdown
5. `test_unknown_key_diagnostic` -- `tsks:` -> NIKA-163 diagnostic
6. `test_goto_definition` -- `$task_ref` -> location response
**Commit**: `test(lsp): add E2E protocol tests for 6 key scenarios`

### Task 16: Fix failing nika-lsp-core test
**File**: `tools/nika-lsp-core/tests/completion_e2e.rs`
**Problem**: `transform_chain_completions` test failing -- likely expects transform that does not exist.
**Fix**: Align test expectations with actual 31-transform catalog.
**Commit**: `fix(lsp): fix transform_chain_completions test`

---

## E2E Verification

### test-lsp-unknown-keys.nika.yaml
```yaml
schema: "nika/workflow@0.12"
workflow: test-lsp-unknown-keys
provider: mock

tsks:
  - id: test
    exec: "echo hello"
# Expected: NIKA-163 on "tsks:" with "did you mean 'tasks'?"
```

### test-lsp-task-unknown-key.nika.yaml
```yaml
schema: "nika/workflow@0.12"
workflow: test-lsp-task-key
provider: mock

tasks:
  - id: test
    exec: "echo hello"
    dependson: [nonexistent]
# Expected: NIKA-163 on "dependson:" with "did you mean 'depends_on'?"
```

### Manual verification:
1. Open `.nika.yaml` with typos in VS Code -> red squiggles with "did you mean" suggestions
2. Hover over `infer:` -> rich documentation popup
3. Ctrl+Space after `- ` -> verb completions
4. Type `$nonexistent` in `with:` -> red squiggle
5. Status bar shows `Nika Workflow`
6. Cmd+Shift+P -> all Nika commands present

---

## After All Fixes

```bash
cargo test -p nika-core --lib          # Parser tests
cargo test -p nika-lsp-core --lib      # Handler tests (374+)
cargo test -p nika-lsp --lib           # Integration tests
cargo test --workspace --lib           # Full suite
cd editors/vscode && npm run compile   # Extension builds
cargo clippy --workspace -- -D warnings  # Zero warnings
```

---

## Commit Strategy (16 commits)

```
# Layer 0: Extension fix
fix(lsp): sync VS Code extension version to 0.50.0
fix(lsp): register commands before async binary discovery
fix(lsp): add files.associations for .nika.yaml auto-detection
fix(lsp): handle malformed YAML in template validation without crash

# Layer 1: Parser validation
feat(parser): detect unknown workflow-level keys (NIKA-163)
fix(parser): detect unknown task keys even when verb is present
feat(parser): warn on empty tasks array
test(parser): comprehensive NIKA-163 unknown field validation

# Layer 2: Validation parity
feat(lsp): add JSON Schema validation parity with nika check
feat(lsp): validate import file paths in LSP
feat(lsp): warn on missing provider API keys
test(lsp): diagnostic tests for all error kinds
test(lsp): template validation edge cases

# Layer 3: E2E test infra
test(lsp): create stdio JSON-RPC test harness
test(lsp): add E2E protocol tests for 6 key scenarios
fix(lsp): fix transform_chain_completions test
```
