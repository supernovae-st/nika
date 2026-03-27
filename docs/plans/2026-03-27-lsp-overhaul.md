# LSP Overhaul Plan — From Broken to Best-in-Class

**Date**: 2026-03-27
**Status**: PLAN — Waiting for execution
**Priority**: CRITICAL — User-facing, blocks adoption
**Scope**: nika-core (parser), nika-lsp-core (handlers), nika-lsp (backend), editors/vscode (extension), nika-daemon (IPC bridge)

## Problem Statement

The Nika LSP has 14 features implemented (~7000 lines, 422 tests) but the user experience is broken:

1. **Extension commands fail**: `nika.checkWorkflow` → "command not found"
2. **Extension marketplace is 7 versions behind**: local 0.42.0 vs binary 0.49.0 — likely stale `VSCE_PAT` or failed CI publish
3. **Zero validation for typos**: `tsks:`, `workfow:`, `exc:` → no errors shown
4. **NIKA-163 (UnknownField)** defined but NEVER used in code
5. **Task key validation only fires when NO verb found** — if task HAS a verb + unknown key, silent
6. **`nika check` runs 7 validation phases, LSP only runs 3** — massive parity gap
7. **`template_validation.rs:200` has `.unwrap()` → LSP CRASH on malformed YAML**
8. **Daemon not connected to LSP** — provider availability, MCP tools, cost data not surfaced
9. **3 diagnostic tests out of 40+ needed**
10. **Zero e2e tests** for LSP protocol (stdio/JSON-RPC) and VS Code extension

## Architecture

```
Current:                              Target:

VS Code ◄──stdio──► nika lsp          VS Code ◄──stdio──► nika lsp
                    (3 phases)                              (7 phases)
                    no daemon                                  │ IPC
                    no schema                            ┌─────▼─────┐
                    no crash guard                       │  daemon   │
                                                         │ providers │
                                                         │ MCP tools │
                                                         │ cost data │
                                                         └───────────┘

nika check (7 phases):                LSP (3 phases):
 1. JSON Schema validation            ❌ missing
 2. Parse (raw AST)                   ✅ parity
 3. Include imports                   ❌ missing
 4. DAG cycle detection               ✅ parity (via analyzer)
 5. Binding validation                ✅ parity (via analyzer)
 6. Schema file existence             ❌ missing
 7. Provider API key check            ❌ missing
 8. MCP strict validation             ❌ missing (--strict only)
```

---

## Layer 0: Extension Fix (CRITICAL)

### Task 0.1: Verify and fix marketplace publishing

**Why**: Extension is v0.42.0 on marketplace while binary is v0.49.0. CI `vscode-publish` job has `if: secrets.VSCE_PAT != ''` guard — if token expired, publish silently skips.

**Steps**:
1. Check secret: go to GitHub repo → Settings → Secrets → verify `VSCE_PAT` exists and is valid
2. Check last release workflow: Actions → release.yml → last `vscode-publish` step — did it pass or skip?
3. If token expired: regenerate at https://dev.azure.com → Personal Access Tokens → scope: Marketplace (Manage)
4. Update `VSCE_PAT` secret in GitHub
5. Manually trigger release workflow for latest tag OR publish locally:
   ```bash
   cd editors/vscode
   npm version 0.49.0 --no-git-tag-version
   npm run compile
   npx vsce package
   npx vsce publish -p <NEW_PAT>
   npx ovsx publish -p <OVSX_PAT>  # Open VSX for Cursor
   ```
6. Verify: https://marketplace.visualstudio.com/items?itemName=supernovae.nika-lang shows v0.49.0

**Also verify `OVSX_PAT`** — Open VSX is used by Cursor/VSCodium. Same issue if expired.

**Files**: `.github/workflows/release.yml` lines 602-667, GitHub Secrets

### Task 0.2: Fix extension activation robustness

**Why**: Commands register AFTER async binary discovery. If `context.globalStorageUri.fsPath` crashes, commands never register.

**Steps**:
1. Read `editors/vscode/src/extension.ts` lines 456-549
2. Move ALL `registerCommand()` calls (lines 527-549) to the TOP of `activate()`, before async binary discovery
3. Wrap `globalStorageUri.fsPath` in try/catch with fallback to temp dir
4. Add output channel logging for activation errors:
   ```typescript
   const outputChannel = window.createOutputChannel('Nika');
   outputChannel.appendLine(`Nika extension activating, binary: ${configPath}`);
   ```
5. In the `client.start().catch()` handler, log to output channel (not just showErrorMessage)

**Verify**: Open `.nika.yaml` → Cmd+Shift+P → type "Nika" → must see all 5 commands even if binary is missing

**Files**: `editors/vscode/src/extension.ts`

### Task 0.3: Add files.associations + configurationDefaults

**Why**: VS Code chicken-and-egg: `onLanguage:nika` activation needs language to be recognized first.

**Steps**:
1. Add to `package.json` configurationDefaults:
   ```json
   "configurationDefaults": {
     "files.associations": { "*.nika.yaml": "nika" },
     "yaml.schemas": {
       "https://nika.sh/schemas/workflow.json": "*.nika.yaml"
     }
   }
   ```

**Files**: `editors/vscode/package.json`

### Task 0.4: Sync extension version in repo

**Why**: CI syncs dynamically via `npm version` but committed package.json stays at 0.42.0. Confusing for developers.

**Steps**:
1. Update `editors/vscode/package.json` version to match workspace version (0.49.0)
2. Update `optionalDependencies` versions in `packages/npm/package.json` if applicable
3. Add CI guard: fail release if extension version < binary version

**Files**: `editors/vscode/package.json`

### Task 0.5: Build, package, and test extension locally

**Steps**:
1. `cd editors/vscode && npm install && npm run compile` → must pass
2. `npx vsce package` → creates `.vsix`
3. `code --install-extension nika-lang-*.vsix` → install local build
4. Open `.nika.yaml` → verify:
   - Cmd+Shift+P → all 5 Nika commands present
   - Hover over `infer:` → rich markdown popup
   - Ctrl+Space after `- ` → completions (verbs, fields)
   - Type `$nonexistent` in `with:` → red squiggle
   - Status bar shows `Nika Workflow`

**Files**: `editors/vscode/`

---

## Layer 1: Parser Validation — NIKA-163 (CRITICAL)

### Task 1.1: Add unknown workflow-level key detection

**Why**: `tsks:`, `workfow:`, `proovider:` are silently ignored. Parser uses pull-based lookup (`get_node("tasks")`) instead of whitelist validation. `ParseErrorKind::UnknownField` (NIKA-163) exists but is NEVER used.

**Steps**:
1. Read `nika-core/src/ast/raw/parser.rs` lines 1240-1332 (parse function)
2. After line 1329 (parse_tasks), before `Ok(workflow)`, add:
   ```rust
   // Validate no unknown workflow keys (NIKA-163)
   let known_workflow_keys: &[&str] = &[
       "schema", "workflow", "description", "provider", "model", "base_url",
       "mcp", "pkg", "context", "imports", "inputs", "artifacts",
       "log", "agents", "skills", "tasks",
   ];
   for (key, _) in map.iter() {
       let key_str = key.as_str();
       if !known_workflow_keys.contains(&key_str) {
           let span = marked_span_to_span(file_id, key.span());
           let suggestion = known_workflow_keys.iter()
               .find(|k| is_likely_misspelling(key_str, k))
               .map(|k| format!("did you mean '{}'?", k));
           return Err(ParseError {
               kind: ParseErrorKind::UnknownField,
               span,
               message: match suggestion {
                   Some(ref s) => format!("unknown workflow field '{}'. {}", key_str, s),
                   None => format!(
                       "unknown workflow field '{}'. Known: {}",
                       key_str, known_workflow_keys.join(", ")
                   ),
               },
           });
       }
   }
   ```

**Test**: `echo "schema: nika/workflow@0.12\ntsks:\n  - id: a\n    exec: echo" | nika check -` → `NIKA-163: unknown workflow field 'tsks'. did you mean 'tasks'?`

**Files**: `nika-core/src/ast/raw/parser.rs`

### Task 1.2: Fix task-level unknown key detection (BUG)

**Why**: The unrecognized-key check (lines 460-519) only runs in the `// No verb found` branch. If a task has `exec:` + `foobar:`, the `foobar:` key is silently ignored because verb parsing returns early at line 437.

**Steps**:
1. Read lines 380-522 of parser.rs (parse_action function)
2. Extract the known task keys list OUTSIDE the no-verb branch
3. Add a new function `validate_task_keys()` that checks ALL task mapping keys against the known list
4. Call it from `parse_task()` (the caller of `parse_action`) AFTER action parsing succeeds
5. Use NIKA-163 with Levenshtein suggestions (reuse `is_likely_misspelling`)
6. Known task keys: id, description, provider, model, base_url, with, depends_on, output, for_each, as, retry, decompose, structured, artifact, log, concurrency, fail_fast, timeout + the 5 verb keys

**Test**: `exec: "echo" + dependson: [a]` → `NIKA-163: unknown task field 'dependson'. did you mean 'depends_on'?`

**Files**: `nika-core/src/ast/raw/parser.rs`

### Task 1.3: Warn on empty tasks array

**Why**: Workflow with zero tasks parses successfully but is useless. User with `tsks:` typo sees no error AND no warning.

**Steps**:
1. In analyzer `nika-core/src/ast/analyzer/analyze.rs`, after task analysis loop
2. Add warning for empty tasks
3. Add warning for workflow without `provider:` or `model:` when tasks use LLM verbs

**Files**: `nika-core/src/ast/analyzer/analyze.rs`

### Task 1.4: Fix template_validation.rs crash

**Why**: Line 200 has `raw::parse(content, FileId(0)).unwrap()` → CRASH if YAML is malformed during editing. This kills the entire LSP process.

**Steps**:
1. Read `nika-lsp/src/template_validation.rs` line 200
2. Replace `.unwrap()` with `match` + `return vec![]` on error
3. Add tracing::warn for skipped template validation

**Test**: Open a file, type `infer: "unclosed quote` → LSP must NOT crash

**Files**: `nika-lsp/src/template_validation.rs`

### Task 1.5: Comprehensive parser validation tests

**Steps** (8 tests minimum):
1. Unknown workflow key → NIKA-163 with suggestion
2. Misspelled workflow key → "did you mean"
3. Unknown task key WITH verb → NIKA-163
4. Unknown task key WITHOUT verb → error (regression)
5. Empty tasks array → warning
6. Valid workflow → zero errors (regression)
7. Multiple unknown keys → first one reported
8. Key that's NOT a misspelling of anything → lists all known keys

**Files**: `nika-core/src/ast/raw/parser.rs` (test module)

---

## Layer 2: Validation Parity with `nika check` (HIGH)

### Task 2.1: Add JSON Schema validation to LSP

**Why**: `nika check` runs JSON Schema validation as Phase 1 (catches structural issues the parser misses). The LSP skips this entirely.

**Steps**:
1. Find `WorkflowSchemaValidator` used by `nika check` (in `nika/src/main.rs` ~line 1741)
2. Add schema validation to `validate_document()` in `nika-lsp/src/diagnostics.rs`
3. Run it BEFORE raw parse — catches gross structural issues early
4. Convert schema violations to LSP diagnostics with spans

**Files**: `nika-lsp/src/diagnostics.rs`

### Task 2.2: Add include/imports validation to LSP

**Why**: `nika check` resolves `imports:` paths. LSP doesn't — user gets no error for `imports: [./nonexistent.nika.yaml]`.

**Steps**:
1. In `validate_document()`, after parse, check if `imports:` paths exist
2. Emit warning for missing import files
3. For now: just path existence check (not full expansion)

**Files**: `nika-lsp/src/diagnostics.rs`

### Task 2.3: Add provider key warnings to LSP

**Why**: `nika check` warns when provider API keys are missing. LSP doesn't — user discovers the error only at runtime.

**Steps**:
1. In `validate_document()`, check the `provider:` field value
2. Check if corresponding env var exists (`ANTHROPIC_API_KEY` etc.)
3. If missing → Warning diagnostic (not Error — key might be in daemon keychain)
4. Later (Layer 3): check daemon for keychain keys too

**Files**: `nika-lsp/src/diagnostics.rs`

### Task 2.4: Diagnostic tests for all error kinds

**Steps** (13 tests):
1. Each of 9 `AnalyzeErrorKind` variants → verify correct NIKA-XXX code + severity
2. Each of 4 `ParseErrorKind` variants → verify correct NIKA-16X code + severity
3. Verify diagnostic includes suggestion text when available
4. Verify diagnostic includes related_information (note) when available

**Files**: `nika-lsp/src/diagnostics.rs`

### Task 2.5: Template validation tests

**Steps** (5 tests):
1. `{{with.undefined}}` → diagnostic
2. `{{with.data | nonexistent_filter}}` → diagnostic
3. `{{invalid_syntax` → diagnostic
4. Valid templates → no diagnostic
5. Template in for_each with `{{with.item}}` → no false positive

**Files**: `nika-lsp/src/template_validation.rs`

---

## Layer 3: LSP E2E Test Infrastructure (HIGH)

### Task 3.1: Create stdio JSON-RPC test harness

**Why**: Zero tests verify the actual LSP protocol flow. Handlers are tested in isolation but the full cycle (initialize → didOpen → publishDiagnostics → completion) is untested.

**Steps**:
1. Create `tools/nika-lsp/tests/harness.rs` with:
   - `LspTestClient` struct wrapping spawned `nika lsp` child process
   - `send_request(method, params)` → JSON-RPC over stdin
   - `read_notification()` → read next server notification from stdout
   - `expect_diagnostics(uri)` → wait for publishDiagnostics
   - JSON-RPC framing (Content-Length header + JSON body)
2. Use `tokio::test` for async

**Files**: `nika-lsp/tests/harness.rs` (NEW)

### Task 3.2: E2E protocol tests

**Steps** (6 tests):
1. `test_initialize_handshake` — send initialize, verify capabilities response has all 14 features
2. `test_didopen_publishes_diagnostics` — open file with error, verify diagnostic notification
3. `test_completion_returns_items` — request completion, verify verb items
4. `test_hover_returns_docs` — hover over verb, verify markdown
5. `test_unknown_key_diagnostic` — open file with `tsks:`, verify NIKA-163 diagnostic
6. `test_goto_definition` — click `$task_ref`, verify location response

**Files**: `nika-lsp/tests/e2e_protocol.rs` (NEW)

### Task 3.3: VS Code extension test setup

**Why**: Zero extension tests. Can't verify activation, command registration, or LSP connection.

**Steps**:
1. Add `@vscode/test-electron` to `devDependencies`
2. Create `editors/vscode/src/test/runTest.ts` — test runner
3. Create `editors/vscode/src/test/suite/` with:
   - `activation.test.ts` — extension activates on .nika.yaml
   - `commands.test.ts` — all 5 commands registered
   - `lsp.test.ts` — LSP client connects and receives capabilities
4. Add `"test"` script to `package.json`: `node ./out/test/runTest.js`

**Files**: `editors/vscode/src/test/` (NEW), `editors/vscode/package.json`

### Task 3.4: Fix failing nika-lsp-core test

**Why**: `transform_chain_completions` test is failing — needs immediate fix before adding more tests.

**Steps**:
1. Find test in `nika-lsp-core/tests/completion_e2e.rs`
2. Identify the broken assertion (likely expects `json` transform that doesn't exist)
3. Fix the test to match actual transform catalog (31 transforms)

**Files**: `nika-lsp-core/tests/completion_e2e.rs`

---

## Layer 4: Daemon ↔ LSP Bridge (MEDIUM)

### Task 4.1: Extend daemon IPC protocol

**Why**: Daemon already has 22 request types via length-prefixed JSON over Unix socket. Adding LSP queries is trivial.

**Steps**:
1. Add to `DaemonRequest` enum in `nika-daemon/src/protocol.rs`:
   ```rust
   CheckProvider { provider: String },     // → has_key, source
   GetCostEstimate { model: String, tokens_in: u64, tokens_out: u64 }, // → usd
   ListConfiguredProviders,                // → Vec<(provider, has_key)>
   ```
2. Add corresponding `DaemonResponse` variants
3. No auth needed for these read-only queries
4. Add serialization tests

**Files**: `nika-daemon/src/protocol.rs`

### Task 4.2: Implement daemon handlers

**Steps**:
1. `handle_check_provider()` — query SecretService for provider key existence
2. `handle_get_cost_estimate()` — use cost catalog from nika-core
3. `handle_list_configured_providers()` — iterate 7 known providers
4. Wire into request dispatcher in `server.rs`

**Files**: `nika-daemon/src/server.rs`

### Task 4.3: Create LSP daemon bridge

**Why**: No circular deps — `nika-daemon` only depends on `nika-core`, safe to import.

**Steps**:
1. Add optional dep: `nika-daemon = { workspace = true, optional = true }` to nika-lsp Cargo.toml
2. Create `nika-lsp/src/daemon_bridge.rs`:
   - `DaemonBridge::connect()` → try Unix socket, return `Option<Self>`
   - `query_provider(name) → ProviderStatus`
   - `estimate_cost(model, tokens) → f64`
   - Cache responses with 60s TTL
3. In `NikaBackend::new()`, attempt connection (non-blocking)
4. Graceful degradation if daemon not running

**Files**: `nika-lsp/src/daemon_bridge.rs` (NEW), `nika-lsp/Cargo.toml`, `nika-lsp/src/backend.rs`

### Task 4.4: Wire daemon data into LSP features

**Steps**:
1. **Completions**: filter providers by key availability, show `[no key]` suffix on unconfigured providers
2. **Inlay hints**: show `// ~$0.03` cost estimate next to model lines
3. **Diagnostics**: Warning on `provider:` line if no API key detected
4. **Hover**: show provider status and last run cost in hover popup

**Files**: `nika-lsp-core/src/handlers/completion.rs`, `nika-lsp-core/src/handlers/inlay_hints.rs`, `nika-lsp/src/diagnostics.rs`, `nika-lsp-core/src/handlers/hover.rs`

### Task 4.5: Daemon bridge tests

**Steps** (6 tests):
1. Bridge connects to running daemon → query succeeds
2. Bridge fails gracefully when daemon not running → returns None
3. Provider query returns correct status for env var keys
4. Cost estimate matches catalog values
5. Cache TTL expires → re-queries daemon
6. Mock daemon for unit tests

**Files**: `nika-lsp/src/daemon_bridge.rs` (test module)

---

## Layer 5: UX Polish (LOW)

### Task 5.1: Better empty-file experience

**Steps**:
1. When `.nika.yaml` opened empty: show code action "Initialize Nika workflow" → inserts template
2. Diagnostic hint: "Add schema: nika/workflow@0.12 to start"
3. Code lens on first line: "▶ Create workflow from template"

**Files**: `nika-lsp-core/src/handlers/code_action.rs`, `nika-lsp/src/diagnostics.rs`

### Task 5.2: Rich snippet completions

**Steps**:
1. `task` → full task scaffold with tab stops: `- id: ${1:task_name}\n    ${2|infer,exec,fetch,invoke,agent|}: ${3:""}`
2. `foreach` → for_each block with items, as, concurrency
3. `retry` → retry block with max_attempts, delay_ms, backoff
4. `agent` → full agent block with tools, max_turns, completion

**Files**: `editors/vscode/snippets/nika.code-snippets`, `nika-lsp-core/src/handlers/completion.rs`

### Task 5.3: Status bar + output channel

**Steps**:
1. Extension shows status bar item: "🦋 Nika LSP ✓" or "🦋 Nika LSP ✗"
2. Click → opens "Nika Language Server" output channel
3. Show daemon connection status: "(daemon: ✓)" or "(daemon: ✗)"
4. Show error count in status bar: "🦋 Nika: 3 errors"

**Files**: `editors/vscode/src/extension.ts`

### Task 5.4: Document last-valid-AST caching

**Why**: When user is typing (broken YAML), hover/goto-def stop working. Cache last successful parse for fallback.

**Steps**:
1. In backend.rs, store `Option<Arc<AnalyzedWorkflow>>` per document
2. Update on successful parse only
3. Use cached AST for hover/goto-def when current parse fails
4. Clear on document close

**Files**: `nika-lsp/src/backend.rs`

---

## Layer 6: CI + Release Verification (HIGH)

### Task 6.1: Add extension version sync to CI

**Why**: `editors/vscode/package.json` is v0.42.0 in repo while releases publish v0.49.0. Confusing.

**Steps**:
1. In `release.yml` vscode-publish job, COMMIT the version bump back to repo (or accept the dynamic sync)
2. Add pre-release check: fail if extension version < workspace version
3. Add `ci.yml` step: compile extension TypeScript on every PR

**Files**: `.github/workflows/release.yml`, `.github/workflows/ci.yml`

### Task 6.2: Add extension smoke test to CI

**Steps**:
1. In CI, after build: `cd editors/vscode && npm run compile && npx vsce package`
2. Verify .vsix is created and non-empty
3. If extension tests exist (Task 3.3): run `npm test`

**Files**: `.github/workflows/ci.yml`

### Task 6.3: Add LSP e2e tests to CI

**Steps**:
1. Build `nika lsp` binary in CI
2. Run e2e protocol tests (Task 3.2) as part of `cargo test`
3. These tests spawn the LSP binary and test via JSON-RPC

**Files**: `.github/workflows/ci.yml`

### Task 6.4: Post-release marketplace verification

**Why**: CI has smoke test but may not catch all issues. Add explicit version check.

**Steps**:
1. In post-release-smoke-test job, add:
   ```bash
   # Verify marketplace version matches release
   MARKETPLACE_VERSION=$(curl -s "https://marketplace.visualstudio.com/items?itemName=supernovae.nika-lang" | grep -oP '"version":"[^"]*"' | head -1 | grep -oP '[\d.]+')
   if [ "$MARKETPLACE_VERSION" != "$VERSION" ]; then
     echo "::error::Marketplace version ($MARKETPLACE_VERSION) != release ($VERSION)"
   fi
   ```
2. Same for Open VSX (Cursor)
3. Same for npm: `npm view @supernovae/nika version`
4. Same for crates.io: `cargo search nika --limit 1`
5. Send Telegram alert if any mismatch

**Files**: `.github/workflows/release.yml` (post-release-smoke-test job)

### Task 6.5: Version bump and final release

**Steps**:
1. Bump workspace version to 0.50.0 (or 0.49.4)
2. Update CHANGELOG with all LSP fixes
3. Tag + push → CI runs full release pipeline
4. Verify ALL platforms publish:
   - GitHub releases (7 binaries)
   - VS Code marketplace (v0.50.0)
   - Open VSX (v0.50.0)
   - npm (@supernovae/nika v0.50.0)
   - crates.io (nika v0.50.0)
   - Docker (ghcr.io/supernovae-st/nika:0.50.0)
   - Homebrew (supernovae-st/tap/nika 0.50.0)
5. Manual verification:
   - Fresh VS Code → install extension → open .nika.yaml → LSP works
   - Fresh Cursor → install extension → open .nika.yaml → LSP works + AI understands syntax
   - `npx @supernovae/nika --version` → 0.50.0
   - `brew install supernovae-st/tap/nika && nika --version` → 0.50.0

**Files**: `tools/Cargo.toml`, `CHANGELOG.md`, git tags

---

## Execution Schedule

```
Session 1 — EMERGENCY FIX (2-3h)
  Task 0.1: Verify VSCE_PAT + marketplace status
  Task 0.2: Fix extension activation robustness
  Task 0.3: Add files.associations
  Task 0.4: Sync extension version
  Task 0.5: Local build + test
  Task 1.4: Fix template_validation.rs crash (.unwrap())

Session 2 — PARSER VALIDATION (2-3h)
  Task 1.1: Unknown workflow key detection (NIKA-163)
  Task 1.2: Fix task-level unknown key detection bug
  Task 1.3: Empty tasks array warning
  Task 1.5: Parser validation tests (8+)

Session 3 — VALIDATION PARITY (2-3h)
  Task 2.1: JSON Schema validation in LSP
  Task 2.2: Include/imports validation
  Task 2.3: Provider key warnings
  Task 2.4: Diagnostic tests for all error kinds (13)
  Task 2.5: Template validation tests (5)

Session 4 — E2E TEST INFRASTRUCTURE (2-3h)
  Task 3.1: Create stdio JSON-RPC test harness
  Task 3.2: E2E protocol tests (6)
  Task 3.3: VS Code extension test setup
  Task 3.4: Fix failing nika-lsp-core test

Session 5 — DAEMON BRIDGE (3-4h)
  Task 4.1: Extend daemon IPC protocol
  Task 4.2: Implement daemon handlers
  Task 4.3: Create LSP daemon bridge
  Task 4.4: Wire daemon data into LSP features
  Task 4.5: Daemon bridge tests

Session 6 — UX POLISH (2h)
  Task 5.1: Empty-file experience
  Task 5.2: Rich snippet completions
  Task 5.3: Status bar + output channel
  Task 5.4: Last-valid-AST caching

Session 7 — CI + RELEASE (2h)
  Task 6.1: Extension version sync in CI
  Task 6.2: Extension smoke test in CI
  Task 6.3: LSP e2e tests in CI
  Task 6.4: Post-release marketplace verification
  Task 6.5: Version bump + final release + manual verification
```

## Verification Checklist (per session)

After each session:
- [ ] `cargo test -p nika-core --lib` — parser tests pass
- [ ] `cargo test -p nika-lsp-core --lib` — handler tests pass (374+)
- [ ] `cargo test -p nika-lsp --lib` — integration tests pass
- [ ] `cargo test -p nika-engine --lib` — engine tests pass (3708+)
- [ ] `cd editors/vscode && npm run compile` — extension builds
- [ ] `cargo clippy --workspace -- -D warnings` — zero warnings
- [ ] Manual: open `.nika.yaml` with typos → red squiggles
- [ ] Manual: hover + completion + goto-def work

## Final Success Criteria

- [ ] `tsks:` → red squiggle with "did you mean tasks?" (NIKA-163)
- [ ] `exc:` → red squiggle with "did you mean exec?" (NIKA-163)
- [ ] `dependson:` in task WITH verb → red squiggle (fixed bug)
- [ ] Empty tasks array → warning
- [ ] Ctrl+Space → real completions for verbs, fields, providers
- [ ] Hover over `infer:` → documentation popup
- [ ] Cmd+Shift+P → "Nika: Validate" → works (command registered)
- [ ] Missing API key → warning on `provider:` line
- [ ] Status bar shows "🦋 Nika LSP ✓"
- [ ] Daemon provides live provider status to LSP
- [ ] 50+ diagnostic tests passing
- [ ] 6+ e2e protocol tests passing
- [ ] Extension tests passing in CI
- [ ] VS Code marketplace shows latest version
- [ ] Open VSX (Cursor) shows latest version
- [ ] All 7 distribution channels version-aligned
