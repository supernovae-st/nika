# Nika Revised Implementation Plan — 2026-03-13

**Status**: ACTIVE
**Author**: Claude Opus 4 + 4-Agent Research Swarm
**Base**: Revision of 2026-03-12 master plan after deep agent audit
**Current Version**: v0.27.0 | 6,157 tests | Zero clippy warnings

---

## Executive Summary

```
+===============================================================================+
|  REVISED PLAN — 3 PHASES (trimmed from 6)                                      |
+===============================================================================+
|                                                                                |
|  Phase 1: Bug Fixes (ONLY BUG-005 remains)                                    |
|  ├── BUG-003: ALREADY FIXED (implicit depends_on via analyze.rs:399-405)      |
|  ├── BUG-004: ALREADY FIXED (v0.22.4 compute_depths + deepest_final_task)     |
|  └── BUG-005: for_each Pattern 2 ($items) missing JSON parse  [ACTIONABLE]    |
|                                                                                |
|  Phase 3: MCP Hardening (3 actionable, 2 deferred)                             |
|  ├── NIKA-103: Wire existing retry.rs into call_tool paths   [ACTIONABLE]     |
|  ├── NIKA-104: Cache invalidation on disconnect/reconnect    [ACTIONABLE]     |
|  ├── NIKA-105: Workflow timeout propagation to MCP calls     [ACTIONABLE]     |
|  ├── NIKA-101: OnceCell mitigates spawn race (low priority)  [DEFERRED]       |
|  └── NIKA-102: rmcp handles transport internally             [DEFERRED]       |
|                                                                                |
|  Phase 5: v0.28 Workspace Restructure (7 crates, incremental)                  |
|  ├── Step 0: Pre-migration audit + TemplateContext trait      [PLANNING]       |
|  ├── Step 1: Extract nika-core (error, types, security)                        |
|  ├── Step 2: Extract nika-ast (raw, analyzed, analyzer)                        |
|  ├── Step 3: Extract nika-mcp (client, pool, retry, adapter)                   |
|  ├── Step 4: Extract nika-provider (rig, native)                               |
|  ├── Step 5: Extract nika-runtime (runner, executor, bindings)                 |
|  ├── Step 6: Extract nika-tui (feature-gated, largest)                         |
|  └── Step 7: nika-cli becomes thin entry point                                 |
|                                                                                |
+===============================================================================+
```

---

## Research Methodology

4 exploration agents + Perplexity + Context7 + sequential thinking:

| Agent | Scope | Key Finding |
|-------|-------|-------------|
| Agent 1 | BUG-003 analysis | ALREADY FIXED in both analyzer and runtime paths |
| Agent 2 | BUG-004 + BUG-005 | BUG-004 FIXED, BUG-005 Pattern 2 incomplete |
| Agent 3 | NIKA-101 to NIKA-105 | retry.rs exists but not wired; 2 real bugs |
| Agent 4 | Monolith structure | 218K lines, 371 files, 1 circular dep identified |
| Perplexity | Workspace patterns | Incremental extraction, `resolver = "2"`, dep inheritance |

---

## Phase 1: Bug Fixes

### BUG-003: use: implicit depends_on — ALREADY FIXED

**Evidence**:
- `analyze.rs:399-405` — Extracts implicit deps from `with:` bindings into `AnalyzedTask.implicit_deps`
- `flow.rs:124-148` — Processes implicit deps when building DAG from analyzed workflow
- `flow.rs:466-519` — Runtime path also handles implicit deps
- Tests pass for both paths

**Action**: None. Remove from roadmap.

### BUG-004: Wrong terminal task selection — ALREADY FIXED

**Evidence**:
- `flow.rs:196-275` — `compute_depths()` + `get_deepest_final_task()` implemented in v0.22.4
- Multiple test scenarios covering various DAG shapes
- Terminal task correctly identified via depth-first selection

**Action**: None. Remove from roadmap.

### BUG-005: for_each $items resolution — NEEDS FIX

**Root Cause**: Three patterns exist for for_each resolution, but Pattern 2 is inconsistent:

```
Pattern 1: $inputs.xxx    → Resolved via workflow inputs → WORKS
Pattern 2: $items          → Task binding ref            → INCOMPLETE
Pattern 3: {{use.items}}   → Template resolution         → WORKS
```

**Pattern 2 Issues** (identified in `runner.rs`):
1. When `$items` resolves to a JSON string (e.g., `"[\"a\",\"b\"]"`), no JSON parsing occurs
2. Nested path support missing — `$task.field.subfield` doesn't drill into JSON
3. Silent failure mode — falls back to single-element instead of erroring

**Fix**:
```rust
// In resolve_for_each_items():
// After resolving $items binding to a string value:
if let Ok(parsed) = serde_json::from_str::<Vec<serde_json::Value>>(&value) {
    return Ok(parsed.into_iter().map(|v| v.to_string()).collect());
}
// If it's a JSON object, extract by path
if path.contains('.') {
    // Drill into nested JSON with serde_json pointer
}
```

**Tests to add**:
- `for_each_dollar_binding_json_string` — `$step1` output is `"[\"a\",\"b\",\"c\"]"`, should expand
- `for_each_dollar_binding_nested_path` — `$step1.items` drills into JSON response
- `for_each_dollar_binding_non_array_errors` — `$step1` is plain string, should error not silently fallback

---

## Phase 3: MCP Hardening

### NIKA-101: Server spawn race — DEFERRED

**Current state**: `OnceCell` in `pool.rs` already serializes per-server initialization. The race condition described in the master plan is mitigated by the async `OnceCell` pattern — only one task spawns the server, others await.

**Remaining concern**: No explicit readiness probe after spawn. The tool cache population at lines 256-261 is conditional but doesn't verify the server actually responds.

**Decision**: Defer. The `OnceCell` pattern prevents the catastrophic race. A readiness probe would be nice-to-have but not a crash risk.

### NIKA-102: Stdin/stdout mutex — DEFERRED

**Current state**: rmcp SDK handles transport layer internally. The concurrent peer clones at `client.rs:892-953` use rmcp's own concurrency model.

**Decision**: Defer. This is an rmcp SDK responsibility, not a Nika bug.

### NIKA-103: Wire retry.rs into call_tool paths — ACTIONABLE

**Current state**: `retry.rs` already exists with:
- `McpRetryConfig` (max_retries=3, initial_delay=100ms)
- `retry_mcp_call()` using `backon` exponential backoff with jitter
- `is_retryable_mcp_error()` for error classification
- 10 unit tests passing

**What's missing**: `retry_mcp_call()` is NOT called from `client.rs::call_tool()` or pool operations. The retry infrastructure exists but is dead code.

**Fix**:
```rust
// In client.rs::call_tool():
// Before:
let result = self.inner_call_tool(tool, params).await?;

// After:
let result = retry_mcp_call(
    McpRetryConfig::default(),
    || self.inner_call_tool(tool, params.clone())
).await?;
```

**Tests to add**:
- `call_tool_retries_on_timeout` — Simulate timeout, verify retry occurs
- `call_tool_no_retry_on_validation_error` — Non-retryable error, no retry

### NIKA-104: Tool cache stale after reconnect — ACTIONABLE

**Root cause** in `rmcp_adapter.rs`:
- `disconnect()` (lines 269-277) doesn't invalidate the tool cache
- TTL-based cache (lines 556-586) never resets timestamp on reconnection
- After reconnect, stale tool list may reference tools the server no longer exposes

**Fix**:
```rust
// In disconnect():
pub async fn disconnect(&self) -> Result<(), NikaError> {
    // ... existing shutdown logic ...
    self.invalidate_tool_cache();  // ADD: Clear stale cache
    Ok(())
}

// In reconnect or re-initialize:
// After successful re-connection:
self.invalidate_tool_cache();  // Force re-fetch on next call
```

**Tests to add**:
- `tool_cache_cleared_on_disconnect` — After disconnect, cache is empty
- `tool_cache_refreshed_after_reconnect` — Reconnect triggers fresh list_tools

### NIKA-105: Timeout not propagating from workflow — ACTIONABLE

**Root cause**: All MCP call sites use hardcoded `MCP_CALL_TIMEOUT` constant:
- `rmcp_adapter.rs:244` — call_tool
- `rmcp_adapter.rs:347` — list_tools
- `rmcp_adapter.rs:448` — read_resource
- `rmcp_adapter.rs:517` — list_resources

The workflow-level `timeout` field on tasks is ignored for MCP operations.

**Fix**:
```rust
// Add optional timeout parameter to call_tool:
pub async fn call_tool(
    &self,
    tool: &str,
    params: Value,
    timeout: Option<Duration>,  // ADD
) -> Result<Value, NikaError> {
    let deadline = timeout.unwrap_or(MCP_CALL_TIMEOUT);
    tokio::time::timeout(deadline, self.inner_call_tool(tool, params))
        .await
        .map_err(|_| NikaError::Timeout { ... })??
}
```

**Propagation chain**:
```
workflow.yaml timeout: → executor.rs → runner.rs → client.rs → rmcp_adapter.rs
```

**Tests to add**:
- `call_tool_uses_custom_timeout` — Pass 1s timeout, verify it's respected
- `call_tool_defaults_to_global_timeout` — No timeout passed, uses MCP_CALL_TIMEOUT

---

## Phase 5: v0.28 Workspace Restructure

### Architecture

```
nika-dev/
├── Cargo.toml                 # [workspace] root
├── crates/
│   ├── nika-core/             # Error types, shared types, security
│   │   └── src/
│   │       ├── error.rs       # NikaError (40+ variants)
│   │       ├── types.rs       # Shared types (TaskId, etc.)
│   │       ├── security.rs    # Command blocklist, path validation
│   │       └── template.rs    # TemplateContext trait (breaks circular dep)
│   │
│   ├── nika-ast/              # YAML parsing pipeline
│   │   └── src/
│   │       ├── raw/           # Phase 1: marked_yaml → RawWorkflow
│   │       ├── analyzed/      # Phase 2: validated AnalyzedWorkflow
│   │       ├── analyzer/      # Transformation + validation
│   │       ├── context.rs     # ContextSpec
│   │       ├── include.rs     # IncludeSpec + DAG fusion
│   │       └── output.rs      # OutputSpec
│   │
│   ├── nika-mcp/              # MCP client layer
│   │   └── src/
│   │       ├── client.rs      # McpClient
│   │       ├── pool.rs        # McpClientPool (DashMap + OnceCell)
│   │       ├── retry.rs       # backon retry logic
│   │       └── rmcp_adapter.rs # rmcp SDK wrapper
│   │
│   ├── nika-provider/         # LLM providers
│   │   └── src/
│   │       ├── rig.rs         # RigProvider + NikaMcpTool
│   │       └── native/        # mistral.rs NativeRuntime
│   │
│   ├── nika-runtime/          # Execution engine
│   │   └── src/
│   │       ├── runner.rs      # Workflow orchestration
│   │       ├── executor/      # Task dispatch
│   │       ├── rig_agent_loop/ # Agent execution
│   │       ├── binding/       # Data flow ({{use.alias}})
│   │       ├── dag/           # DAG construction + validation
│   │       └── event/         # EventLog + trace
│   │
│   ├── nika-tui/              # Terminal UI (feature-gated)
│   │   └── src/               # ratatui views, widgets, panels
│   │
│   └── nika-cli/              # Thin CLI entry point
│       └── src/
│           └── main.rs        # Clap commands → crate dispatch
│
└── tools/nika/                # Legacy location (redirect to crates/)
```

### Dependency Graph

```
nika-cli
├── nika-tui (optional, feature = "tui")
│   ├── nika-runtime
│   │   ├── nika-provider
│   │   │   ├── nika-mcp
│   │   │   │   └── nika-core
│   │   │   └── nika-core
│   │   ├── nika-ast
│   │   │   └── nika-core
│   │   ├── nika-mcp
│   │   └── nika-core
│   └── nika-core
└── nika-core
```

### Circular Dependency Solution

The `binding` module needs runtime context for template resolution, but `runtime` depends on `binding`. Solution: trait boundary in `nika-core`.

```rust
// In nika-core/src/template.rs
pub trait TemplateContext: Send + Sync {
    fn resolve(&self, key: &str) -> Option<String>;
    fn has_key(&self, key: &str) -> bool;
}
```

`nika-runtime` implements `TemplateContext`, while `binding` code only depends on the trait from `nika-core`.

### Extraction Order

Extract in dependency order (leaves first):

1. **nika-core** — Zero deps, foundation for everything
2. **nika-ast** — Depends only on nika-core
3. **nika-mcp** — Depends only on nika-core
4. **nika-provider** — Depends on nika-core + nika-mcp
5. **nika-runtime** — Depends on core + ast + mcp + provider
6. **nika-tui** — Depends on everything (feature-gated)
7. **nika-cli** — Thin wrapper, final step

### Migration Strategy (Perplexity-informed)

**Incremental, not big-bang**:

1. Create workspace root `Cargo.toml` with `members = []`
2. Keep existing `tools/nika/` as the first (only) member
3. Extract one crate at a time:
   - Create `crates/nika-X/Cargo.toml`
   - Move files with `git mv`
   - Update imports in remaining code
   - Run tests, fix, commit
   - Add `nika-X` as dependency where needed
4. Each extraction is ONE commit
5. Tests must pass at every step

**Workspace dependency inheritance**:
```toml
# Root Cargo.toml
[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
tokio = { version = "1", features = ["full"] }
thiserror = "2"

# Crate Cargo.toml
[dependencies]
serde = { workspace = true }
tokio = { workspace = true }
```

---

## Priority & Ordering

```
+===============================================================================+
|  EXECUTION ORDER                                                                |
+===============================================================================+
|                                                                                |
|  1. BUG-005 fix (for_each Pattern 2)              ~30 min   [SMALL, HIGH ROI] |
|  2. NIKA-103 wire retry.rs                         ~30 min   [SMALL, HIGH ROI] |
|  3. NIKA-104 cache invalidation                    ~30 min   [SMALL, HIGH ROI] |
|  4. NIKA-105 timeout propagation                   ~1 hour   [MEDIUM]          |
|  5. v0.28 workspace setup (core + ast)             ~multi-session   [LARGE]    |
|                                                                                |
+===============================================================================+
```

Items 1-4 are actionable NOW. Item 5 is a multi-session project that should be brainstormed separately with proper worktree isolation.

---

## Quality Gates

Every change must pass:
- [ ] `cargo check` clean
- [ ] `cargo clippy -- -D warnings` zero warnings
- [ ] `cargo test` all tests pass
- [ ] New tests for each fix (TDD preferred)
- [ ] Granular commits (1 fix = 1 commit)
