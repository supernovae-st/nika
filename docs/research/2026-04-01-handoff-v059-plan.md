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

### Root Cause (confirmed by rust-pro deep analysis)
`fetch.rs:557` — Comment says "Success or non-retryable error status" but code makes NO distinction between 200 and 404. The decision tree:
- 5xx / 429 → retry → `Err(FetchError)` if max attempts reached
- **ALL other statuses (1xx, 2xx, 3xx, 4xx)** → fall through to response handling as "success"

**Mode-specific behavior on 404:**
- `response: binary` — ALREADY fails (line 597 `is_success()` check). Correct.
- `response: full` — Returns `{"status": 404, "body": "..."}`. Status visible. Arguably correct.
- Default (no response) — Returns 404 HTML as task output. **BUG.**
- `extract: metadata` on 404 → parses error page HTML for OG tags. Returns garbage.
- `extract: article` on 404 → runs Readability on error page. Returns error text as "article".
- `extract: jsonpath` on 404 → HTML body fails `serde_json::from_str`. Returns NIKA-046. Correct.

**No double retry risk**: runner.rs:1137-1138 explicitly skips task-level retry for fetch verbs.

**Existing test asserts buggy behavior**: `wiremock_fetch_404_returns_body` (line 639) — must be updated.

### File
`tools/nika-engine/src/runtime/executor/fetch.rs`

### Fix Plan (layered, per response mode)

**Step 1** — Add 4xx status check after the retry block (~line 557):

```rust
// After retry logic, before response mode handling:
if !response.status().is_success() && !response.status().is_redirection() {
    let status = response.status();
    // response: full explicitly wants all statuses — don't fail
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

**Step 2** — `response: full` is correct as-is. `response: binary` is correct as-is.

**Step 3** — Update test `wiremock_fetch_404_returns_body` to assert error instead of body.

### What would break
- `follow_redirects: false` with 3xx → would now fail. Add exception for 3xx when `follow_redirects: false`.
- APIs returning useful JSON on 4xx → broken. **Consider**: add `allow_error_status: true` flag later if needed.

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

### Root Cause (confirmed by rust-pro deep analysis — exact code flow)

**Full pipeline for `with: { key: $env.MISSING | default("fallback") }`**:

```
1. BindingPath::parse("$env.MISSING") → source: Env("MISSING")
2. TransformExpr::parse('default("fallback")') → ops: [Default("fallback")]
3. resolve_binding_path() → std::env::var("MISSING") → Ok(None)  ← var not found
4. Transform dispatch (resolve.rs:658-676):
     match (&raw_value, &entry.transform):
       (Some(v), Some(expr)) if !v.is_null() → apply transforms
       (Some(v), Some(expr)) if v.is_null() → try apply, fallback
       _ → raw_value  ← None + Some(transform) hits THIS ARM
   → Transforms SKIPPED because raw_value is None
5. Step 4 fallback: transformed is None, entry.default is None
   → Err(PathNotFound) = NIKA-052
```

**Key insight**: The `_` catch-all arm at line 675 matches `(None, Some(_))`. The `default()` transform never fires because `None` is not `Some(Value::Null)`.

**`??` works because** it's stored in `entry.default` (Step 5 fallback), NOT in the transform chain.

**Affects ALL binding sources**: $env, $vault, $task (missing output) — all return `Ok(None)`.

### Files
- `tools/nika-engine/src/binding/resolve.rs:658-676` — transform dispatch
- `tools/nika-core/src/binding/transform.rs:543` — default() transform impl
- `tools/nika-core/src/binding/entry.rs:134` — `??` operator parsing

### Fix Plan (targeted — from rust-pro recommendation)

**Better than changing resolve_binding_path semantics**: fix the transform dispatch to promote `None` → `Value::Null` when the chain contains `default()`.

**Step 1** — Add `has_default()` to `TransformExpr` (nika-core/src/binding/transform.rs):
```rust
impl TransformExpr {
    pub fn has_default(&self) -> bool {
        self.ops.iter().any(|op| matches!(op, TransformOp::Default(_)))
    }
}
```

**Step 2** — Fix the match in resolve.rs:658-676:
```rust
let transformed = match (&raw_value, &entry.transform) {
    (Some(v), Some(expr)) if !v.is_null() => { /* existing: apply */ }
    (Some(v), Some(expr)) if v.is_null() => { /* existing: try apply */ }
    // NEW: None + transform chain containing default() → promote to Null
    (None, Some(expr)) if expr.has_default() => {
        match expr.apply(&Value::Null) {
            Ok(result) => Some(result),
            Err(e) => {
                tracing::debug!(path = %path_str, error = %e,
                    "Transform failed on missing value");
                None // Fall through to Step 4
            }
        }
    }
    _ => raw_value,  // Unchanged: None without default() → NIKA-052
};
```

**What this preserves**:
- `$env.MISSING` (no transforms) → still NIKA-052 (correct)
- `$env.MISSING | upper` → still NIKA-052 (no default in chain)
- `$env.MISSING | default("x") | upper` → "X" (default fires, then upper)
- `$env.MISSING ?? "x"` → still works via entry.default (unchanged)
- `$env.SET | default("x")` → still returns SET value (unchanged)
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

### Root Cause (confirmed by rust-pro — exact variant count and migration status)
103 variants in `error.rs` (2,796 lines). `error_domains.rs` has 4 scaffolded sub-enums but **ZERO production callsites** use them — the `From` impls exist but no code constructs domain errors yet.

### Current migration status
| Domain Enum | Scaffolded Variants | Completeness |
|-------------|-------------------|--------------|
| `ProviderError` | 7 of 8 | 87% (missing WorkflowTimeout) |
| `DagError` | 3 of 6 | 50% (missing DependencyChainFailed, TaskCancelled) |
| `ExecutionError` | 6 of 7 | 86% (missing InvokeParamError) |
| `BindingError` | 3 of ~8 | 38% (missing 5 variants) |

### Callsite hotspots (top 5 hardest)
| Variant | Callsites | Notes |
|---------|-----------|-------|
| `ValidationError` | **75** | Used as catch-all across 20+ files. NEEDS AUDIT before migrating. |
| `BuiltinToolError` | 32 | Concentrated in builtin/ |
| `ToolError` | 30 | Concentrated in tools/ |
| `TemplateParse` | 28 | Concentrated in template.rs (38 total binding domain) |
| `InvalidPkgUri` | 26 | Self-contained in registry |

### Easiest wins (3 fewest callsites)
| Domain | Variants | Callsites | Notes |
|--------|----------|-----------|-------|
| Course (310-314) | 5 | ~0 | Completely self-contained |
| Record (320-324) | 5 | ~5 | Isolated in record_compress.rs |
| StructuredOutput (300-303) | 4 | ~15 | Isolated in structured_output.rs |

### Fix Plan (4 phases, reordered by effort/value)

**Phase 1 — Quick Wins (1-2 days, 4 domains)**:
1. `CourseError` (310-314) — 5 variants, ~0 cross-cutting callsites
2. `RecordError` (320-324) — 5 variants, isolated in 1 file
3. `StructuredOutputError` (300-303) — 4 variants, isolated
4. `ArtifactError` (280-285) — 4 variants, 53 callsites but all in artifact_processor.rs

**Phase 2 — Complete Existing Scaffolding (2-3 days)**:
5. `DagError` — add 3 missing variants, ~10 callsites
6. `ProviderError` — add WorkflowTimeout, ~20 callsites
7. `ExecutionError` — add InvokeParamError, ~30 callsites
8. `BindingError` — add 5 missing variants, ~50 callsites

**Phase 3 — New Domains (3-5 days)**:
9. `McpError` (100-110) — 11 variants
10. `AgentError` (110-119) — 5 variants
11. `OutputError` (060-069) — 3 variants

**Phase 4 — Hard Cases (5-7 days, LAST)**:
12. `ToolError` (200-213) — 83 callsites, sprawled across builtin system
13. `ValidationError` split — 75 callsites, **REQUIRES AUDIT to reclassify each site**

### miette::Diagnostic with #[error(transparent)]
Partial support only. Existing `MediaError` pattern shows working approach: sub-enum has own `code()` method, and `NikaError::code()` delegates explicitly. This works but is manual.

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

### Fix Plan (from rust-pro — 4 phases, dependency-ordered)

**The critical insight**: `resolve_for_each_items()` (S11f, 372 lines) MUST be extracted FIRST. It contains 4 duplicated binding resolution formats with ~70 lines of verbatim duplication. This is the single largest contributor to complexity.

**Dependency graph**:
```
Phase 1 (leaf extractions, any order):
  compute_dag_depths()  |  check_pre_flight()  |  aggregate_for_each_results()

Phase 2 (critical path — MUST happen before main loop simplification):
  resolve_for_each_items()  ← free function, 372 lines → ~50 lines

Phase 3 (main loop cleanup, depends on Phase 2):
  expand_decompose_items()  |  check_completion()  |  check_cancellation()

Phase 4 (bookends, depends on Phase 1+3):
  initialize_workflow_context()  |  finalize_workflow()  |  initialize_renderer()
```

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
