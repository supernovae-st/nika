# MEGA SESSION — Stabilization, Bug Fixes, Quality Polish

> **Copy-paste into a new Claude Code session.**
> **Mode**: TDD, granular commits, push after each fix.
> **Version**: v0.75.0 | **Tests**: 10,339+ | **Launch**: May 5, 2026

---

## WHO YOU ARE

Rust + TypeScript senior on **Nika** — AI workflow engine. This is the FINAL stabilization pass before launch. Every fix must have a test. Franglais ok, EN code/commits.

---

## RULES

```
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```
- 1 fix = 1 commit, TDD (test first, then fix)
- `cargo test --workspace --lib` before EVERY commit
- `cargo clippy --workspace -- -D warnings` must be clean
- `cd editors/vscode && npm run compile` must build

---

## ETAPE 1 — Verify Baseline

```bash
cd /Users/thibaut/dev/supernovae/nika/tools
cargo test --workspace --lib --exclude nika-py 2>&1 | grep "test result:"
cargo clippy --workspace -- -D warnings 2>&1 | tail -5
cargo fmt --all --check 2>&1 | head -10
cd ../editors/vscode && npm run compile 2>&1 | tail -5
```

Expected: 10,339+ tests green, clippy clean, fmt may have 6 files drift.

---

## PART A — MUST FIX (blocks launch)

### A1. Stale counts: "63 transforms, 62 tools" → "64 transforms, 63 tools"

**Test first:**
```bash
# This test should FAIL before fix:
cargo test -p nika-core --lib -- catalogs::builtins::tests::test_known_builtins_count
```

**Files to fix:**
1. `tools/nika-core/src/catalogs/builtins.rs:108` — `assert_eq!(KNOWN_BUILTIN_TOOLS.len(), 62)` → `63`
2. `tools/nika-core/src/catalogs/builtins.rs` — update comment arithmetic: "7 + 5 + **13** + 6 + 4 + 2 + 5 + 3 + 17 = **63**"
3. `tools/nika/src/main.rs:945` — `"63 transforms, 62 tools"` → `"64 transforms, 63 tools"`
4. `tools/nika-mcp/src/server.rs` — SCHEMA_REF constant: "63 Transforms" → "64 Transforms", "62 Builtin Tools" → "63 Builtin Tools"

**Commit:** `fix: update transform/tool counts (64 transforms, 63 builtins)`

### A2. npm package version sync (0.74.0 → 0.75.0)

```bash
# Check current state:
grep '"version"' packages/npm/package.json packages/nika-*/package.json
```

**Files:** ALL 6 package.json files in `packages/`:
- `packages/npm/package.json` — version + optionalDependencies versions
- `packages/nika-darwin-arm64/package.json`
- `packages/nika-darwin-x64/package.json`
- `packages/nika-linux-x64/package.json`
- `packages/nika-linux-arm64/package.json`
- `packages/nika-win32-x64/package.json`

Change all `"0.74.0"` → `"0.75.0"`.

**Commit:** `chore: sync npm package versions to 0.75.0`

### A3. Dockerfile version stale

**File:** `tools/nika/Dockerfile:56`
```dockerfile
# BEFORE:
ARG VERSION=0.74.0
# AFTER:
ARG VERSION=0.75.0
```

**Commit:** `chore: update Dockerfile default VERSION to 0.75.0`

---

## PART B — BUGS (from 6-agent code review)

### B1. CI release.yml: upload-artifact@v7 vs download-artifact@v8

**File:** `.github/workflows/release.yml`
Find all `upload-artifact@v7` (should be 3 sites around lines 306, 1156, 1249).
Change each to `upload-artifact@v8`.

**Verify:** `grep -n "upload-artifact@v" .github/workflows/release.yml`

**Commit:** `fix(ci): upgrade upload-artifact v7 to v8 for compatibility with download-artifact v8`

### B2. Extension: registerTreeDataProvider disposable leak

**File:** `editors/vscode/src/extension.ts:77`
```typescript
// BEFORE:
window.registerTreeDataProvider('nikaWorkflows', workflowTree);

// AFTER:
context.subscriptions.push(window.registerTreeDataProvider('nikaWorkflows', workflowTree));
```

**Commit:** `fix(vscode): push TreeDataProvider disposable to subscriptions`

### B3. LSP: duplicate reconnect at startup

**File:** `tools/nika-lsp/src/backend.rs:96-101`
The immediate `b.reconnect().await` spawned task AND `DaemonBridge::spawn_reconnect_loop` both attempt connection simultaneously.

**Fix:** Remove the immediate spawn, let `spawn_reconnect_loop` handle it (it already does an immediate attempt on first iteration).

**Commit:** `fix(lsp): remove duplicate daemon reconnect at startup`

### B4. Serve: blocking std::fs in async reconciler

**File:** `tools/nika-serve/src/lib.rs` — `reconcile_yaml_schedules` and `scan_scheduled_workflows`
Both call `std::fs::read_to_string` synchronously inside async context.

**Fix:** Wrap the file scan in `tokio::task::spawn_blocking`:
```rust
let paths = tokio::task::spawn_blocking(move || collect_workflow_paths(&dir)).await.unwrap_or_default();
```

Or convert to `tokio::fs::read_to_string` for individual file reads.

**Commit:** `fix(serve): use spawn_blocking for filesystem ops in async reconciler`

### B5. Serve auth: missing WWW-Authenticate header

**File:** `tools/nika-serve/src/auth.rs:47`
```rust
// BEFORE:
StatusCode::UNAUTHORIZED

// AFTER:
(
    StatusCode::UNAUTHORIZED,
    [(header::WWW_AUTHENTICATE, "Bearer realm=\"nika\"")],
)
```

**Commit:** `fix(serve): add WWW-Authenticate header on 401 per RFC 7235`

### B6. MCP: SCHEMA_REF stale count

Already covered in A1 above.

### B7. MCP: nika_schema ignores version parameter

**File:** `tools/nika-mcp/src/server.rs` — `schema()` handler
Either remove the `version` field from `SchemaParams` or add a doc comment: "Version parameter is reserved for future schema versioning."

**Commit:** `fix(mcp): document that nika_schema version param is reserved`

---

## PART C — QUALITY IMPROVEMENTS

### C1. cargo fmt --all

```bash
cd tools && cargo fmt --all
```

**Commit:** `style: cargo fmt`

### C2. Artifact path collision detection (RUNNER-5)

**File:** `tools/nika-engine/src/runtime/artifact_processor.rs`

**TDD:**
```rust
#[test]
fn artifact_collision_detected() {
    // Two tasks writing to same path should produce NIKA-281 error
}
```

**Fix:** Add `HashSet<String>` tracking resolved artifact paths. On collision, emit `NIKA-281`.

**Commit:** `fix(engine): detect artifact path collisions between tasks (NIKA-281)`

### C3. for_each.rs stale TODO comment

**File:** `tools/nika-engine/src/runtime/for_each.rs:28`
The function IS wired but the comment says it isn't. Update docstring.

**Commit:** `docs: update for_each.rs docstring — function is wired`

### C4. Extension: `as any` cast on updateTaskStatus

**File:** `editors/vscode/src/lspClient.ts:164`
Define a proper type union for the status parameter instead of `as any`.

**Commit:** `fix(vscode): type updateTaskStatus status parameter properly`

### C5. Extension: resolvedServerPath set to literal 'nika'

**File:** `editors/vscode/src/extension.ts:263`
When PATH binary is found, `resolvedServerPath` is set to the literal string `'nika'`.
Should resolve to absolute path via `which nika` or keep as-is with documentation.

**Commit:** `fix(vscode): resolve absolute PATH for nika binary`

---

## PART D — TESTS TO ADD

### D1. Auth middleware integration test

**File:** `tools/nika-serve/src/auth.rs` (add #[cfg(test)] module)

```rust
#[tokio::test]
async fn auth_middleware_rejects_without_token() {
    // Build Router with require_auth middleware
    // Send request without Authorization header
    // Assert 401 + WWW-Authenticate header
}

#[tokio::test]
async fn auth_middleware_accepts_valid_token() {
    // Build Router with require_auth, set token
    // Send request with valid Bearer token
    // Assert 200
}
```

**Commit:** `test(serve): add auth middleware integration tests`

### D2. Named endpoint slash model test

**File:** `tools/nika-engine/src/runtime/executor/infer.rs`

```rust
#[test]
fn test_parse_model_slash_named_endpoint() {
    // model: "h100/Qwen/Qwen3-8B" → ("h100", "Qwen/Qwen3-8B")
    let result = parse_model_slash("h100/Qwen/Qwen3-8B");
    assert_eq!(result, Some(("h100", "Qwen/Qwen3-8B")));
}
```

**Commit:** `test(engine): add named endpoint slash model test`

### D3. MCP dag_visualization block-style depends_on

**File:** `tools/nika-mcp/src/server.rs`

Currently only parses `depends_on: [a, b]` inline. Add test for:
```yaml
depends_on:
  - task_a
  - task_b
```

**Commit:** `test(mcp): document block-style depends_on limitation in dag_visualization`

### D4. extract_schedule_value with inline comments

**File:** `tools/nika-serve/src/lib.rs`

```rust
#[test]
fn extract_schedule_with_inline_comment() {
    let header = "schedule: \"@daily\" # runs every day\ntasks:";
    let val = extract_schedule_value(header);
    assert_eq!(val.unwrap(), serde_json::json!("@daily"));
}
```

**Commit:** `test(serve): verify extract_schedule_value handles inline comments`

---

## PART E — POST-LAUNCH (document, don't implement)

These are tracked but NOT blocking launch:

1. **LSP executionEvent wiring** — live DAG updates require in-process engine
2. **Native model discovery** — TUI stub at loader.rs:147
3. **EventLog drain O(n)** — switch Vec to VecDeque for buffer eviction
4. **DaemonBridge full feature set** — provider status, cost, history in LSP
5. **ast_integration.rs wiring** — AST-based extraction for go-to-definition
6. **nika-napi/nika-py cleanup** — remove deprecated crate dirs
7. **runner.rs further decomposition** — test split done, prod split next
8. **nikaProviders tree view** — declared in package.json, no TreeDataProvider

---

## EXECUTION ORDER

```
1. cargo fmt --all                              (C1, 2 min)
2. Fix counts: 63→64 transforms, 62→63 tools   (A1, 15 min)
3. npm version sync                             (A2, 10 min)
4. Dockerfile version                           (A3, 2 min)
5. CI upload-artifact version                   (B1, 5 min)
6. TreeDataProvider disposable                  (B2, 5 min)
7. Duplicate reconnect                          (B3, 10 min)
8. spawn_blocking in reconciler                 (B4, 30 min)
9. WWW-Authenticate header                      (B5, 10 min)
10. MCP schema version doc                      (B7, 5 min)
11. for_each docstring                          (C3, 2 min)
12. as any cast fix                             (C4, 10 min)
13. Auth middleware tests                       (D1, 30 min)
14. Named endpoint test                         (D2, 10 min)
15. Artifact collision detection                (C2, 1h)
```

Total: ~4h of focused work.

---

## VERIFICATION (after all fixes)

```bash
cd /Users/thibaut/dev/supernovae/nika/tools
cargo test --workspace --lib --exclude nika-py   # ALL pass
cargo clippy --workspace -- -D warnings          # clean
cargo fmt --all --check                          # clean
cd ../editors/vscode && npm run compile          # builds
git push                                         # ship it
```
