# LSP Overhaul Plan — From Broken to Best-in-Class

**Date**: 2026-03-27
**Status**: PLAN — Waiting for execution
**Priority**: CRITICAL — User-facing, blocks adoption
**Scope**: nika-core (parser), nika-lsp-core (handlers), nika-lsp (backend), editors/vscode (extension), nika-daemon (IPC bridge)

## Problem Statement

The Nika LSP has 14 features implemented (~7000 lines, 422 tests) but the user experience is broken:

1. **Extension commands fail**: `nika.checkWorkflow` → "command not found" (extension activation crash or stale marketplace build)
2. **Zero validation for typos**: `tsks:`, `workfow:`, `exc:` → no errors shown
3. **NIKA-163 (UnknownField)** defined but never used in code
4. **Task key validation only fires when NO verb found** — if task HAS a verb + unknown key, it's silent
5. **Daemon not connected to LSP** — provider availability, MCP tools, cost data not surfaced
6. **3 diagnostic tests out of 40+ needed**

## Architecture

```
Current:                              Target:
┌───────────┐                         ┌───────────┐
│  VS Code  │◄──stdio──►│  nika lsp │ │  VS Code  │◄──stdio──►│  nika lsp │
└───────────┘            └───────────┘ └───────────┘            └─────┬─────┘
                                                                      │ IPC
                                                                ┌─────▼─────┐
                                                                │  daemon   │
                                                                │ providers │
                                                                │ MCP tools │
                                                                │ cost data │
                                                                └───────────┘
```

---

## Layer 0: Extension Fix (CRITICAL — do first)

### Task 0.1: Diagnose extension activation failure

**Why**: `nika.checkWorkflow` not found means extension crashed during activate() or marketplace version is stale.

1. Check published extension version: `code --list-extensions --show-versions | grep nika`
2. Compare with local `editors/vscode/package.json` version (currently 0.42.0)
3. If mismatch → republish extension via `cd editors/vscode && vsce package && vsce publish`
4. If versions match → extension crashes during activation

**Files**: `editors/vscode/package.json`, `editors/vscode/src/extension.ts`

### Task 0.2: Fix extension activation robustness

**Why**: `context.globalStorageUri.fsPath` at line 460 may crash if undefined. All subsequent code (including command registration) is skipped.

1. Read `editors/vscode/src/extension.ts` fully
2. Wrap `globalStorageUri.fsPath` access in try/catch
3. Move command registration (lines 527-549) BEFORE the async binary discovery block (lines 456-525) so commands always register even if binary discovery fails
4. Add error logging to VS Code output channel for activation failures

**Verify**: Open `.nika.yaml` → Cmd+Shift+P → type "Nika" → should see Run Workflow, Validate, New Workflow commands

**Files**: `editors/vscode/src/extension.ts`

### Task 0.3: Add files.associations default for .nika.yaml

**Why**: VS Code may not auto-detect `.nika.yaml` as language ID "nika" on first open (before extension activates).

1. Add to `package.json` configurationDefaults:
   ```json
   "files.associations": { "*.nika.yaml": "nika" }
   ```
2. This ensures the language is recognized even before the extension's `activate()` fires

**Files**: `editors/vscode/package.json`

### Task 0.4: Verify extension builds and commands work

1. `cd editors/vscode && npm run compile` → must succeed
2. Open VS Code → install local extension: `code --install-extension nika-lang-0.42.0.vsix`
3. Open any `.nika.yaml` file
4. Verify: Cmd+Shift+P → "Nika: Validate" → should open terminal
5. Verify: hover over `infer:` → should show docs
6. Verify: Ctrl+Space after `- ` in tasks → should show completions

**Files**: `editors/vscode/`

---

## Layer 1: Parser Validation — NIKA-163 (HIGH — core fix)

### Task 1.1: Add unknown workflow-level key detection

**Why**: `tsks:` instead of `tasks:` is silently ignored. Parser only looks up keys by name, never checks for extras.

1. Read `nika-core/src/ast/raw/parser.rs` lines 1240-1332 (parse function)
2. After line 1329 (parse_tasks), add workflow-level key validation:
   ```rust
   // Validate no unknown workflow keys
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
               message: if let Some(ref s) = suggestion {
                   format!("unknown workflow field '{}'. {}", key_str, s)
               } else {
                   format!("unknown workflow field '{}'. Known fields: {}", key_str,
                       known_workflow_keys.join(", "))
               },
           });
       }
   }
   ```
3. This activates NIKA-163 for the first time

**Test**: `nika check` on a file with `tsks:` must show `NIKA-163: unknown workflow field 'tsks'. did you mean 'tasks'?`

**Files**: `nika-core/src/ast/raw/parser.rs`

### Task 1.2: Fix task-level unknown key detection (BUG)

**Why**: Task unknown-key check at line 460 only runs when NO verb is found. If task has `exec:` + `foobar:`, `foobar:` is silently ignored.

1. Read lines 380-522 of parser.rs (parse_action function)
2. The unknown-key check (lines 460-519) is inside the `// No verb found` block
3. Move the unrecognized-key check BEFORE the verb parsing (lines 383-458), or add a SECOND check after the verb is parsed:
   ```rust
   // After verb parsing succeeds (before returning Ok(Some(action))):
   // Check for unknown keys that aren't the verb or known task fields
   let all_known: Vec<&str> = known_non_verb_keys.iter().copied()
       .chain(verb_keys.iter().copied())
       .chain(["base_url", "as"].iter().copied())  // for_each extras
       .collect();
   for (key, _) in map.iter() {
       if !all_known.contains(&key.as_str()) {
           // Emit warning or error for unrecognized task key
       }
   }
   ```
4. Use ParseErrorKind::UnknownField (NIKA-163) with misspelling suggestions

**Test**: Task with `exec: "echo hi"` + `dependson: [foo]` must flag `dependson` as unknown (did you mean `depends_on`?)

**Files**: `nika-core/src/ast/raw/parser.rs`

### Task 1.3: Warn on empty tasks array

**Why**: A workflow with zero tasks is valid YAML but useless. Currently silently accepted.

1. In the analyzer (`nika-core/src/ast/analyzer/analyze.rs`), after task collection
2. If tasks array is empty, emit a warning:
   ```rust
   if workflow.tasks.is_empty() {
       result.warnings.push(AnalyzeError::new(
           AnalyzeErrorKind::MissingField,
           workflow.span,
           "workflow has no tasks",
       ).with_suggestion("add at least one task under 'tasks:'"));
   }
   ```

**Files**: `nika-core/src/ast/analyzer/analyze.rs`

### Task 1.4: Add tests for unknown key detection

1. Test: unknown workflow key → NIKA-163 error with suggestion
2. Test: misspelled workflow key (`tsks` → `tasks`) → "did you mean"
3. Test: unknown task key WITH verb present → error
4. Test: unknown task key WITHOUT verb → error (existing behavior, add test)
5. Test: empty tasks array → warning
6. Test: valid workflow → no errors (regression test)

**Files**: `nika-core/src/ast/raw/parser.rs` (test module), `nika-core/src/ast/analyzer/analyze.rs` (test module)

---

## Layer 2: Diagnostic Depth (HIGH — LSP quality)

### Task 2.1: Add diagnostic tests for all 9 AnalyzeErrorKind variants

**Why**: Only 3/9 error kinds are tested in diagnostics. No coverage for DuplicateTask, CyclicDependency, MissingField, MissingModel, InvalidBinding, InvalidValue, UnsupportedFeature.

1. Read `nika-lsp/src/diagnostics.rs` test module
2. Add one test per error kind:
   - `test_validate_duplicate_task` (two tasks with same id)
   - `test_validate_cyclic_dependency` (A depends on B, B depends on A)
   - `test_validate_missing_field` (task without id)
   - `test_validate_missing_model` (infer verb without model or workflow-level model)
   - `test_validate_invalid_binding` ($nonexistent in with block)
   - `test_validate_invalid_value` (invalid for_each value)
   - `test_validate_unsupported_feature` (if applicable)
3. Each test verifies: diagnostic count > 0, severity is ERROR, code matches NIKA-XXX

**Files**: `nika-lsp/src/diagnostics.rs`

### Task 2.2: Add diagnostic tests for ParseError variants

**Why**: Zero tests for parse errors reaching the diagnostic pipeline.

1. Test: malformed YAML (syntax error) → NIKA-160
2. Test: missing `schema:` field → NIKA-161
3. Test: invalid schema version → NIKA-164
4. Test: unknown workflow field → NIKA-163 (after Task 1.1)
5. Verify: diagnostic severity, code, message all correct

**Files**: `nika-lsp/src/diagnostics.rs`

### Task 2.3: Add template validation tests

**Why**: Template validation exists (template_validation.rs) but may not surface errors.

1. Test: `{{with.undefined_alias}}` → diagnostic about undefined alias
2. Test: `{{with.data | nonexistent_transform}}` → diagnostic
3. Test: `{{invalid_syntax` → diagnostic

**Files**: `nika-lsp/src/template_validation.rs`

---

## Layer 3: Daemon ↔ LSP Bridge (MEDIUM — differentiation)

### Task 3.1: Design IPC protocol

**Why**: Daemon has provider keys, MCP server registry, cost data. LSP needs this for smart completions and diagnostics.

1. Define message types:
   ```rust
   enum LspQuery {
       GetProviderStatus,           // → which providers have valid keys
       GetMcpTools(String),         // → tools for a specific MCP server
       GetModelCost(String),        // → pricing for a model ID
       GetRecentRunCost(String),    // → last run cost for a workflow
   }
   ```
2. Use existing daemon Unix socket (`~/.nika/daemon/nika.sock`)
3. Add `LspQuery` handler to daemon server

**Files**: `nika-daemon/src/protocol.rs`, `nika-daemon/src/server.rs`

### Task 3.2: LSP connects to daemon on startup

**Why**: LSP currently has zero external intelligence.

1. In `NikaBackend::new()`, attempt daemon connection
2. Store as `Option<DaemonClient>` — LSP works without daemon (graceful degradation)
3. Query provider status on connect → cache locally
4. Refresh on `did_save` events

**Files**: `nika-lsp/src/backend.rs`

### Task 3.3: Smart completions from daemon data

**Why**: Completion currently suggests all providers/models regardless of which have keys.

1. Provider completion → only suggest providers with valid API keys (from daemon)
2. Model completion → only suggest models for configured providers
3. MCP tool completion → suggest tools from running MCP servers (not just hardcoded nika:* builtins)
4. Inlay hints → show actual cost from last run ("$0.03 last run" next to model line)

**Files**: `nika-lsp-core/src/handlers/completion.rs`, `nika-lsp-core/src/handlers/inlay_hints.rs`

### Task 3.4: Live provider diagnostics

**Why**: "This API key expired 3 days ago" should be a warning in the editor.

1. On daemon connect, get provider status
2. If provider used in workflow has no key → Warning diagnostic on `provider:` line
3. If provider has invalid key → Error diagnostic
4. If MCP server referenced but not running → Warning on `mcp:` line

**Files**: `nika-lsp/src/diagnostics.rs`, `nika-lsp/src/backend.rs`

---

## Layer 4: UX Polish (LOW — nice to have)

### Task 4.1: Better empty-file experience

1. When `.nika.yaml` is opened with no content or minimal content:
   - Show code action: "Initialize Nika workflow" → inserts template
   - Show diagnostic hint: "Add schema: nika/workflow@0.12 to start"

### Task 4.2: Snippet completions for common patterns

1. Type `task` → full task scaffold with id, verb, with block
2. Type `foreach` → for_each scaffold
3. Type `retry` → retry block scaffold
4. Type `agent` → agent block scaffold with tools, max_turns, completion

### Task 4.3: Status bar integration

1. Show LSP status in status bar: "Nika LSP ✓" or "Nika LSP ✗"
2. Show daemon connection status: "Daemon ✓" or "Daemon ✗"
3. Click opens output channel with diagnostics

---

## Execution Order

```
Session 1 (CRITICAL — 2h):
  Layer 0: Tasks 0.1-0.4 (fix extension, republish)
  Layer 1: Tasks 1.1-1.2 (parser validation, NIKA-163)

Session 2 (HIGH — 2h):
  Layer 1: Tasks 1.3-1.4 (empty tasks, tests)
  Layer 2: Tasks 2.1-2.3 (diagnostic test coverage)

Session 3 (MEDIUM — 3h):
  Layer 3: Tasks 3.1-3.4 (daemon ↔ LSP bridge)

Session 4 (LOW — 1h):
  Layer 4: Tasks 4.1-4.3 (UX polish)
```

## Verification

After each layer:
- `cargo test -p nika-core --lib` — parser tests
- `cargo test -p nika-lsp-core --lib` — handler tests
- `cargo test -p nika-lsp --lib` — integration tests
- `cd editors/vscode && npm run compile` — extension builds
- Manual: open `.nika.yaml` with typos → must show red squiggles

## Success Criteria

- [ ] `tsks:` → red squiggle with "did you mean tasks?"
- [ ] `exc:` → red squiggle with "did you mean exec?"
- [ ] Ctrl+Space → real completions for verbs, fields, providers
- [ ] Hover over `infer:` → documentation popup
- [ ] Cmd+Shift+P → "Nika: Validate" → works
- [ ] Daemon provides live provider status to LSP
- [ ] 40+ diagnostic tests passing
