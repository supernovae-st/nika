# Handoff Plan — v0.59 Known Issues

> 7 functional issues + 4 security findings from v0.58 deep audit.
> Analyzed by 8 specialized agents (rust-pro, rust-architect, rust-security).
> Each with root cause, exact code locations, fix plan, and verification steps.
>
> **FIXED in this session**: path traversal in from_example/schema (HIGH security)

## Priority Order

| # | Issue | Severity | Effort | Type |
|---|-------|----------|--------|------|
| S1 | ~~from_example/schema path traversal~~ | ~~HIGH~~ | ~~S~~ | ~~FIXED~~ |
| S2 | $env.* unrestricted access | MEDIUM | M | Security |
| S3 | Trace secret redaction gaps | MEDIUM | M | Security |
| S4 | Shell blocklist bypass via quoting | LOW | S | Security |
| 1 | fetch 404 returns exit 0 | HIGH | S | Bug |
| 2 | fail_fast:false partial results | HIGH | M | Logic |
| 3 | $env.MISSING before default() | HIGH | M | Binding |
| 4 | workflow graph duplicate edges | LOW | S | Display |
| 5 | {{skills.NAME}} not resolved | MEDIUM | M | Feature Gap |
| 6 | NikaError god enum (103 variants) | LOW | L | Tech Debt |
| 7 | runner.rs run() 1580 lines | LOW | L | Refactor |

## Security Findings (from rust-security agent)

### S1: from_example/schema path traversal — FIXED

`from_example: "../../.env"` could read arbitrary files. Now blocked by `..` component validation in `infer.rs` and `structured_output.rs`.

### S2: $env.* unrestricted access (MEDIUM)

**File**: `resolve.rs:804-821`
`$env.*` reads ANY env var (ANTHROPIC_API_KEY, SSH_AUTH_SOCK, AWS_SECRET_ACCESS_KEY). Combined with `fetch:`, secrets can be exfiltrated to external servers. Mitigating factor: workflow author = user who runs it (only a risk for untrusted YAML execution).

**Recommendation**: Consider env var allowlist for `$env` access, or block known sensitive vars at binding resolution level.

### S3: Trace secret redaction gaps (MEDIUM)

**Files**: `resolve.rs:446-474`, `util/mod.rs:23+`
Pattern-based redaction misses custom API keys (ElevenLabs, xAI, webhook secrets). Env-sourced bindings are tracked but value-based redaction only works for known patterns.

**Recommendation**: Track env-sourced values and redact them wherever they appear in traces (value-based, not just pattern-based).

### S4: Shell blocklist bypass via quoting (LOW)

**File**: `security.rs:358-388`
`su""do rm -rf /` in `shell: true` could bypass the `sudo` pattern. NFKC normalization handles Unicode but not shell quoting.

**Recommendation**: Strip or normalize shell quoting before blocklist comparison.

### Positive Security Findings

- SSRF: defense-in-depth with DNS pinning (prevents rebinding) — EXCELLENT
- Template injection: 3-pass isolation with trusted path sets — EXCELLENT
- CRLF header injection: blocked
- Response size limits: streaming byte counting (50MB text / 100MB binary)
- Unicode NFKC normalization: prevents confusable bypass
- Shell hardcoded to `sh -c`, not user-configurable
- `cwd` traversal: properly blocked via canonicalization
- Library injection env vars: blocked
- `kill_on_drop(true)`: prevents orphaned processes
- `unsafe_code = "deny"` workspace-wide

---

## 1. fetch 404 returns exit 0 silently

### Root Cause
`fetch.rs:481-556` treats ALL non-retryable HTTP statuses (404, 403, 401) as valid responses. Only `response: binary` mode checks for non-success status (line 597). Regular text responses return the 404 HTML error page as "success output".

### File
`tools/nika-engine/src/runtime/executor/fetch.rs`

### Fix Plan

**Step 1** — Add 4xx status check after the retry block (~line 556):

```rust
// After the retry logic, before extraction:
if !response.status().is_success() && !response.status().is_redirection() {
    let status = response.status();
    // Only error if response: full is NOT set (full mode intentionally returns all statuses)
    if fetch.response != Some(ResponseMode::Full) {
        return Err(NikaError::FetchError {
            reason: format!(
                "HTTP {} {} for URL: {}",
                status.as_u16(),
                status.canonical_reason().unwrap_or("Unknown"),
                final_url
            ),
        });
    }
}
```

**Step 2** — `response: full` mode already returns status/headers/body — leave it as-is (users explicitly want the full response including errors).

**Step 3** — For `nika fetch` CLI command, the error propagates through handle_result → exit(1).

### Verification
```bash
nika fetch https://httpstat.us/404; echo "EXIT: $?"   # Should be EXIT: 1
nika fetch https://httpstat.us/200; echo "EXIT: $?"   # Should be EXIT: 0
nika fetch https://httpstat.us/404 --response full    # Should be EXIT: 0 (full mode)
```

### Tests to add
```rust
#[tokio::test]
async fn fetch_404_returns_error() {
    // Use mock server or httpbin
    let result = run_fetch_task("https://httpstat.us/404", None).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("404"));
}

#[tokio::test]
async fn fetch_404_full_mode_returns_ok() {
    let result = run_fetch_task("https://httpstat.us/404", Some(ResponseMode::Full)).await;
    assert!(result.is_ok());
    let output: Value = serde_json::from_str(&result.unwrap()).unwrap();
    assert_eq!(output["status"], 404);
}
```

### Edge cases
- `extract: metadata` on a 404 page — should error (no useful metadata)
- `extract: article` on a 404 page — should error (no article)
- Redirects (301→200) — should succeed (already handled by reqwest)
- `response: binary` on 404 — already errors (line 597)

---

## 2. fail_fast:false partial results not passed downstream

### Root Cause
`runner.rs:2786-2811` — When `fail_fast: false` and some iterations fail, the parent task's `TaskResult` is marked as `failed()` (line 2808). Even though partial outputs ARE stored in `result.output` (line 2809), downstream tasks see `is_success() == false` and get NIKA-026 blocked.

The issue is in `get_ready_tasks()` (runner.rs:457-499): line 473 checks `is_completed_successfully(dep)` which returns `false` for the parent → blocks all dependents.

### Files
- `tools/nika-engine/src/runtime/runner.rs` — aggregation (2786-2811) + ready check (457-499)
- `tools/nika-engine/src/store/run_context.rs` — TaskOutcome enum (22-37)

### Recommended Approach (from rust-architect analysis)

**Option C: `TaskOutcome::PartialSuccess` + `is_usable()` helper**

Instead of changing every `is_success()` caller, add a dual-purpose method:

```rust
pub enum TaskOutcome {
    Success,
    PartialSuccess { error_summary: String, succeeded: u32, failed: u32 },
    Failed(String),
    DependencyFailed { dependency: String },
    Skipped { reason: String },
}

impl TaskResult {
    /// Returns true for Success OR PartialSuccess.
    /// Use for dependency gating (should downstream tasks run?).
    pub fn is_usable(&self) -> bool {
        matches!(self.status, TaskOutcome::Success | TaskOutcome::PartialSuccess { .. })
    }
    /// Returns true ONLY for full Success.
    /// Use for strict checks (artifacts, records, workflow success).
    pub fn is_success(&self) -> bool {
        matches!(self.status, TaskOutcome::Success)
    }
}
```

**Surgical changes needed** (~5-6 lines):
1. `run_context.rs`: Add `PartialSuccess` variant + `is_usable()` method
2. `runner.rs:470`: Change `is_completed_successfully` to call `is_usable()`
3. `runner.rs:2786-2811`: Use `PartialSuccess` when `!fail_fast && any_success`

**Key detail**: `result.output` is ALREADY set with partial data at line 2809. Downstream tasks CAN access `$parent.items[0]` once the dependency gate opens.

**Output array for failed iterations**: `Value::Null` (preserves index alignment).

### Fix Plan

**Step 1** — Add `PartialSuccess` variant to `TaskOutcome`:

```rust
// run_context.rs
pub enum TaskOutcome {
    Success,
    PartialSuccess { failed_count: usize, total_count: usize },
    Failed(String),
    DependencyFailed { dependency: String },
    Skipped { reason: String },
}
```

**Step 2** — In runner.rs aggregation (line 2786), use `PartialSuccess` when some iterations fail and `fail_fast: false`:

```rust
let aggregated_result = if all_success {
    TaskResult::success(Value::Array(outputs), total_duration)
} else if !fail_fast_enabled {
    // Partial success: some failed, but fail_fast is off
    TaskResult::partial_success(
        Value::Array(outputs),
        total_duration,
        failed_count,
        total_count,
    )
} else {
    TaskResult::failed(error_msg, total_duration)
};
```

**Step 3** — In `is_completed_successfully()` (run_context.rs), treat `PartialSuccess` as success for dependency resolution:

```rust
pub fn is_completed_successfully(&self) -> Option<bool> {
    match &self.outcome {
        TaskOutcome::Success | TaskOutcome::PartialSuccess { .. } => Some(true),
        TaskOutcome::Failed(_) | TaskOutcome::DependencyFailed { .. } => Some(false),
        TaskOutcome::Skipped { .. } => Some(false),
    }
}
```

**Step 4** — Emit a warning event when downstream uses partial results:

```rust
EventKind::TaskPartialResults {
    task_id: parent_id,
    succeeded: success_count,
    failed: failed_count,
    total: total_count,
}
```

### Verification
```yaml
# Test workflow
- id: process
  for_each: ["a", "b", "FAIL_ME"]
  as: item
  fail_fast: false
  concurrency: 1
  infer: "Echo: {{with.item}}"

- id: consume
  depends_on: [process]
  with: { results: $process }
  infer: "Got {{with.results | length}} results"
```

Expected: `consume` runs with 3 results (2 valid + 1 null/error placeholder).

### Edge cases
- What goes in the output array for failed iterations? Options: `null`, `{"error": "..."}`, or skip. Recommend `null` to preserve index alignment.
- for_each with `fail_fast: true` (default) — behavior unchanged, still fails immediately.
- Nested for_each — each level independently determines partial success.

---

## 3. $env.MISSING fails before default() can apply

### Root Cause
`resolve.rs:804-821` — `resolve_binding_path()` returns `Ok(None)` for missing env vars. But the error happens in `resolve_with_entry_traced()` around line 900-930 where `None` bindings trigger NIKA-052 BEFORE template transforms (including `default()`) are applied.

The resolution pipeline is:
1. Parse `with:` block → extract `$env.VAR_NAME`
2. Resolve binding → `Ok(None)` for missing var
3. **FAIL: NIKA-052 "binding resolved to null"** ← HERE
4. Never reaches template `{{with.val | default('fallback')}}`

The `??` operator in bindings (`$env.VAR ?? "fallback"`) works because it's evaluated at step 2, not step 4.

### Files
- `tools/nika-engine/src/binding/resolve.rs` — resolution pipeline
- `tools/nika-core/src/binding/types.rs` — BindingSource enum

### Fix Plan

**Step 1** — In `resolve_with_entry_traced()`, when a binding resolves to `None`, DON'T error immediately. Instead, store `Value::Null` in the binding map:

```rust
// Around line 900-930 in resolve.rs
match resolved {
    Some(value) => bindings.insert(alias, value),
    None => {
        // Instead of NIKA-052 immediately, store null and let template transforms handle it
        bindings.insert(alias, Value::Null);
        debug!(alias = %alias, "Binding resolved to null — template default() may apply");
    }
}
```

**Step 2** — Keep NIKA-052 as a warning in strict mode or when no `default()` transform is present in any template referencing this alias.

**Step 3** — Add a post-resolution check: if any `Value::Null` binding is used in a template WITHOUT `default()`, THEN raise NIKA-052.

### Verification
```yaml
# Should work:
with: { val: $env.NONEXISTENT }
infer: "Result: {{with.val | default('fallback')}}"
# Expected output contains "fallback"

# Should still fail:
with: { val: $env.NONEXISTENT }
infer: "Result: {{with.val}}"
# Expected: NIKA-052 (no default() to rescue)
```

### Risks
- Changing null propagation could silently hide real errors where users forgot to set env vars
- Solution: only suppress NIKA-052 when the alias is used with `default()` somewhere in the task's templates

---

## 4. workflow graph duplicate edges

### Root Cause
`nika-engine/src/ast/workflow.rs:128-138` — `Workflow::edges()` collects edges from `depends_on` only. But during DAG construction in `flow.rs:492-573`, implicit edges from `with:` bindings are also created. The CLI graph command uses a simple edge collection that includes BOTH sources without deduplication.

### Files
- `tools/nika-cli/src/workflow.rs:540-545` — graph command
- `tools/nika-engine/src/ast/workflow.rs:128-138` — edges() method

### Fix Plan

**Step 1** — In the `edges()` method, deduplicate using a HashSet:

```rust
pub fn edges(&self) -> Vec<(&str, &str)> {
    let mut seen = HashSet::new();
    let mut edges = Vec::new();
    for task in &self.tasks {
        if let Some(ref deps) = task.depends_on {
            for dep in deps {
                if seen.insert((dep.as_str(), task.id.as_str())) {
                    edges.push((dep.as_str(), task.id.as_str()));
                }
            }
        }
        // Also collect implicit edges from with: bindings
        // ... but deduplicate against depends_on edges
    }
    edges
}
```

**Step 2** — Alternatively, use the DAG's `flow.rs` which already deduplicates (line 492-493 `seen_edges: FxHashSet`). The graph command should use the analyzed DAG, not the raw workflow.

### Verification
```bash
nika workflow graph multi-step.nika.yaml | grep -c "→"
# Should match the unique edge count, not 2x
```

---

## 5. {{skills.NAME}} not resolved in templates

### Root Cause
`BindingSource` enum (nika-core/src/binding/types.rs:58-78) has no `Skills` variant. Skills are only injected into agent system prompts (executor/agent.rs), not available as template variables.

### Files
- `tools/nika-core/src/binding/types.rs` — BindingSource enum
- `tools/nika-engine/src/binding/resolve.rs` — resolution logic
- `tools/nika-engine/src/runtime/runner.rs:1507-1516` — skills loading

### Fix Plan

**Recommended: Option C — Thin Skills Resolution via Template Only** (from rust-architect)

`{{skills.NAME}}` should resolve from a dedicated skills store, reusing the `LoadedContext` infrastructure. No new `BindingSource` variant needed — skills in `with:` blocks (`$skills.pirate`) makes no semantic sense.

**Why NOT Option A (BindingSource::Skills)**: Adds a variant that propagates through every match on BindingSource across 6+ files. Skills in `with:` blocks is not a valid use case — skills are prompt text, not data.

**Changes needed (~60 lines across 3 files)**:

**Step 1** — `run_context.rs`: Add `skills: FxHashMap<String, Value>` field + `resolve_skills_path()`:
```rust
pub fn resolve_skills_path(&self, skill_name: &str) -> Option<Value> {
    self.skills.get(skill_name).cloned()
}
```

**Step 2** — `template.rs`: Add `Skills { path, transforms }` to `TemplateExpr` + `strip_prefix("skills.")` in parser + resolution pass:
```rust
// In parse_template_expr():
if let Some(rest) = expr.strip_prefix("skills.") {
    return TemplateExpr::Skills { path: rest.to_string(), transforms };
}

// In resolve_with() — add pass 4:
TemplateExpr::Skills { path, transforms } => {
    if let Some(val) = ctx.resolve_skills_path(&path) {
        apply_transforms(val, transforms)
    } else {
        warn!("Unknown skill: {path}");
        String::new()
    }
}
```

**Step 3** — `runner.rs`: Load skills into RunContext at workflow start (reuse SkillInjector):
```rust
if !self.workflow.skills_map.is_empty() {
    for (alias, path) in &self.workflow.skills_map {
        let content = tokio::fs::read_to_string(base_path.join(path)).await?;
        self.datastore.set_skill(alias, Value::String(content));
    }
}
```

**NOT changed**: resolve.rs, types.rs, agent.rs (agent injection still works via existing path)

**Step 2** — In resolve.rs `resolve_binding_path()`, add the Skills case:
```rust
BindingSource::Skills(skill_name) => {
    match datastore.get_skill(skill_name.as_ref()) {
        Some(content) => Ok(Some(Value::String(content.to_string()))),
        None => Ok(None),
    }
}
```

**Step 3** — In template parser, recognize `{{skills.NAME}}` prefix (alongside `{{context.NAME}}`, `{{inputs.NAME}}`).

**Step 4** — Load skill files at workflow startup (already done in runner.rs:1507-1516) and store in datastore.

**Option B — Document as system-prompt only** (cheaper):
- Update docs to clarify that `skills:` only works with `agent:` verb
- Add a `nika check` warning when `{{skills.X}}` is used in non-agent tasks

### Verification
```yaml
skills:
  pirate: ./pirate.md

tasks:
  - id: test
    infer: "{{skills.pirate}} Now talk like a pirate about AI"
```

---

## 6. NikaError god enum (103 variants)

### Root Cause
All errors from all subsystems are in one enum in `error.rs` (2,796 lines, 103 variants). `error_domains.rs` has a partial migration (~25%): only `ProviderError` and `DagError` have sub-enums.

### Files
- `tools/nika-engine/src/error.rs` — main enum
- `tools/nika-engine/src/error_domains.rs` — domain sub-enums

### Fix Plan (incremental, 6 phases)

Each phase is one domain, one PR, independently shippable:

**Phase 1 — ExecutionError** (highest ROI):
- Variants: TaskCancelled, ContextError, RuntimeDeadlock, DependencyChainFailed, OrchestratorError
- ~8 variants → `ExecutionError` sub-enum
- `From<ExecutionError> for NikaError` impl

**Phase 2 — BindingError**:
- Variants: TemplateError, BindingNotFound, BindingTypeMismatch, NullBindingValue, JsonPathError
- ~10 variants → `BindingError` sub-enum

**Phase 3 — StructuredOutputError**:
- Variants: SchemaFailed, StructuredOutputFailed, StructuredOutputTimeout, StructuredOutputValidationFailed
- ~6 variants → `StructuredOutputError`

**Phase 4 — FetchError**:
- Variants: FetchError, FetchTimeout, SsrfBlocked, ExtractError
- ~5 variants → `FetchError`

**Phase 5 — MediaError**:
- Variants: MediaToolError, MediaFormatError, MediaDependencyMissing, ArtifactWriteFailed, etc.
- ~12 variants → `MediaError`

**Phase 6 — ToolError** (file tools + builtins):
- Variants: FileNotFound, FileAlreadyExists, BuiltinToolError, ExecError
- ~8 variants → `ToolError`

**Remaining** ~50 variants stay in NikaError (workflow-level, config, MCP, agent, course, etc.).

### Migration pattern per phase:
```rust
// error_domains.rs
#[derive(Debug, Error, Diagnostic)]
pub enum ExecutionError {
    #[error("[NIKA-026] ...")]
    DependencyChainFailed { ... },
    // ...
}

impl From<ExecutionError> for NikaError {
    fn from(e: ExecutionError) -> Self {
        NikaError::Execution(e)
    }
}

// error.rs — replace individual variants with:
#[error(transparent)]
Execution(#[from] ExecutionError),
```

### Verification
- `cargo test --workspace --lib` after each phase
- `error_code()` method must still return correct NIKA-XXX
- `FixSuggestion` must still work for all migrated variants

---

## 7. runner.rs run() method 1580 lines

### Root Cause
The `run()` method (runner.rs:1421-3012) handles everything: initialization, DAG rendering, main loop, for_each expansion, result collection, aggregation, output, artifacts, records, traces, and summary.

### File
`tools/nika-engine/src/runtime/runner.rs`

### Fix Plan (6 extractions)

Extract in this order (each is a standalone PR):

**Extraction 1 — `initialize_workflow_context()`** (lines 1425-1516):
```rust
async fn initialize_workflow_context(&mut self) -> Result<(), NikaError> {
    // Cancel check, orchestrator init, context files, input files,
    // agent resolution, skill resolution
}
```

**Extraction 2 — `setup_dag_and_renderer()`** (lines 1518-1640):
```rust
fn setup_dag_and_renderer(&mut self) -> (Vec<usize>, Box<dyn Renderer>) {
    // Layer computation, pending indices, CLI renderer init,
    // DAG display, warning/tip messages
}
```

**Extraction 3 — `aggregate_for_each_results()`** (lines 2764-2828):
```rust
fn aggregate_for_each_results(
    &self,
    results: IndexMap<Arc<str>, Vec<(usize, TaskResult)>>,
) -> Vec<(Arc<str>, TaskResult)> {
    // Sort by index, merge media, compute all_success,
    // build aggregated TaskResult
}
```

**Extraction 4 — `finalize_workflow()`** (lines 2842-2933):
```rust
async fn finalize_workflow(
    &self,
    output: Option<String>,
    base_path: &Path,
) -> Result<Option<String>, NikaError> {
    // Artifact manifest, media GC, record persistence
}
```

**Extraction 5 — `render_summary()`** (lines 2938-3011):
```rust
fn render_summary(
    &self,
    renderer: &mut dyn Renderer,
    output: Option<&str>,
) {
    // Final events, MCP shutdown, summary display
}
```

**Extraction 6 — `expand_for_each_items()`** (~400 lines inside the main loop):
```rust
async fn expand_for_each_items(
    &self,
    task: &AnalyzedTask,
    datastore: &RunContext,
    bindings: &Bindings,
) -> Result<Vec<Value>, NikaError> {
    // 4 binding format resolutions, JSON string auto-parse,
    // path segment traversal
}
```

### Verification
After each extraction:
- `cargo test --workspace --lib` (all 9232 tests)
- Run a complex workflow with for_each + artifacts + structured output
- Verify no behavior change

---

## Execution Order

```
Week 1: Issues 1 + 4 (quick fixes, HIGH + LOW)
         fetch 404 check + graph dedup
         Est: 2 commits, ~30 min

Week 2: Issue 2 (binding resolution)
         $env null propagation + default() chain
         Est: 3 commits, ~2 hours

Week 3: Issue 3 (fail_fast partial results)
         PartialSuccess variant + aggregation
         Est: 4 commits, ~3 hours

Week 4: Issue 5 (skills template)
         BindingSource::Skills + template parser
         Est: 3 commits, ~2 hours

Post-launch: Issues 6 + 7 (tech debt)
         NikaError migration (6 phases)
         runner.rs extraction (6 PRs)
         Est: 12+ commits, ~2 days
```

## Socratic Questions to Ask Before Each Fix

1. **fetch 404**: Should `extract: metadata` on a soft-404 page (returns 200 but "not found" content) also fail? Or is HTTP status the only signal?
2. **fail_fast partial**: What should the output array contain for failed iterations — `null`, `{"error":"..."}`, or should failed indices be skipped entirely?
3. **$env default**: Should ALL null bindings be allowed through, or only `$env.` sources? What about `$task_that_failed`?
4. **skills**: Is `{{skills.NAME}}` the right syntax, or should it be `{{context.skills.NAME}}` to reuse existing context resolution?
5. **NikaError**: Should domain errors keep NIKA-XXX codes or get domain-specific codes (e.g., PROV-030)?
6. **runner.rs**: Should extracted functions be `pub` (testable from integration tests) or `pub(crate)` (implementation detail)?
7. **graph**: Should implicit `with:` edges be visually distinct from explicit `depends_on` edges (e.g., dashed lines)?
