# Nika Backlog — Detailed Handoff Prompts

> Generated 2026-04-03 after Sprint 1-4 completion (9 commits).
> Each section is a self-contained prompt for a future Claude Code session.

---

## Table of Contents

1. [IMP-002: extract:sitemap](#imp-002-extractsitemap)
2. [BUG-017: Cookie persistence](#bug-017-cookie-persistence)
3. [IMP-004: Agent web extraction scope](#imp-004-agent-web-extraction-scope)
4. [IMP-005: iterate construct](#imp-005-iterate-construct)
5. [IMP-009: for_each output coercion](#imp-009-for_each-output-coercion)
6. [IMP-011: File-based data flow](#imp-011-file-based-data-flow)
7. [DOC-001 through DOC-007](#doc-001-through-doc-007)

---

## IMP-002: extract:sitemap

**Effort:** Medium | **Dep:** quick-xml crate | **Priority:** Medium

### What

Add a 10th extract mode `extract: sitemap` for native XML sitemap parsing. Returns `{ urls: [{loc, lastmod, changefreq, priority}], count, is_index: false }` for urlset or `{ sitemaps: [{loc, lastmod}], count, is_index: true }` for sitemapindex.

### Implementation Status

**90% implemented in working tree** (but NOT committed). The code exists across 4 files:

- `nika-core/src/ast/extract.rs` — `Sitemap` variant added to `ExtractMode`, ALL_NAMES, parse(), as_str(), required_feature() all done
- `nika-engine/Cargo.toml` — `fetch-sitemap = ["dep:quick-xml"]` feature gate added, in default features
- `nika-engine/src/runtime/executor/extract.rs` — `extract_sitemap_xml()` function complete (event-driven quick_xml::Reader, handles urlset + sitemapindex + xhtml:link hreflang)
- Workspace `Cargo.toml` — `quick-xml = "0.37"` added

### Remaining Tasks

1. **Fix LSP completion description**: `nika-lsp-core/src/handlers/completion.rs` line 350 — change `"9 modes"` to `"10 modes"`
2. **Add missing test assertion**: `nika-core/src/ast/extract.rs` test `extract_mode_required_features` — add `assert_eq!(ExtractMode::Sitemap.required_feature(), Some("fetch-sitemap"))`
3. **Add wiremock E2E test**: `nika-engine/src/runtime/executor/tests_wiremock.rs` — mock server returns XML sitemap, verify parsing
4. **Verify Cargo.lock**: `cargo tree -i quick-xml` — confirm nika-engine uses 0.37.5

### Critical Notes

- Uses `quick_xml::reader::Reader` (event-driven), NOT serde-based deserialization — intentional due to namespace prefix handling
- `read_event_into(&mut buf)` requires `buf.clear()` before each call
- `local_name()` strips namespace prefix so `xhtml:link` matches as `"link"`
- Tests gated with `#[cfg(feature = "fetch-sitemap")]`

---

## BUG-017: Cookie Persistence

**Effort:** High (~250 LoC) | **Dep:** reqwest `cookies` feature | **Priority:** Medium

### What

Add `session: "name"` field to `fetch:` tasks. Tasks sharing the same session name share one persistent `reqwest::Client` with `cookie_store(true)`. Cookies from Set-Cookie headers persist across tasks within the same session.

### Architecture

Named sessions via `DashMap<String, Arc<reqwest::Client>>` on `TaskExecutor`. Not a single global jar — isolated per session name, per workflow run.

### 10-Phase Implementation

1. **Cargo.toml** — Add `"cookies"` to reqwest features in `tools/Cargo.toml` line 89
2. **RawFetchAction** — Add `session: Option<Spanned<String>>` in `nika-core/src/ast/raw/action.rs` line 140
3. **Parser** — Add `get_string_field(file, m, "session")?` in `nika-core/src/ast/raw/parser.rs` line 836
4. **AnalyzedFetchAction** — Add `session: Option<String>` in `nika-core/src/ast/analyzed/task.rs` line 230
5. **Analyzer** — Add `session: raw.session.as_ref().map(|s| s.value.clone())` in `nika-core/src/ast/analyzer/analyze.rs` line 1128
6. **FetchParams** — Add `session: Option<String>` in `nika-engine/src/ast/action.rs` line 392
7. **Lower** — Add `session: fetch.session` in `nika-engine/src/ast/lower.rs` line 272
8. **TaskExecutor** — Add `session_clients: Arc<DashMap<String, Arc<reqwest::Client>>>` in `nika-engine/src/runtime/executor/mod.rs` line 60
9. **Fetch execution** — In `fetch.rs` client selection block (lines 196-254), add session client logic: get-or-insert from DashMap, build with `cookie_store(true)` + same SSRF redirect policy
10. **JSON Schema** — Add `session` property to `nika-engine/schemas/nika-workflow.schema.json`

### Security

- Cookies must NOT appear in event logs or traces — verified: HttpRequest/HttpResponse events don't include headers
- Session clients get same SSRF redirect policy as main client
- Log session name at debug level only

### Tests (7 total)

1. `wiremock_fetch_session_cookie_persists` — Set-Cookie on task A, Cookie on task B (same session)
2. `wiremock_fetch_session_isolation` — Different session names don't share cookies
3. `wiremock_fetch_no_session_no_cookies` — No session = no cookie persistence
4. `wiremock_fetch_session_same_client_reused` — Arc::ptr_eq for same session name
5. Unit test for AnalyzedFetchAction session field passthrough
6. Parser round-trip test
7. Schema validation test

---

## IMP-004: Agent Web Extraction Scope

**Effort:** Low (~80 LoC) | **Dep:** None (all tools exist) | **Priority:** Medium

### What

Add 5 web extraction tools to agent scopes: `nika:extract_links`, `nika:extract_metadata`, `nika:html_to_md`, `nika:css_select`, `nika:readability`. Add a `"web"` scope and support explicit tool opt-in.

### Tool Registration Pattern

```
MediaOp (nika-media) → MediaToolAdapter (BuiltinTool) → NikaBuiltinToolAdapter (ToolDyn)
```

### Key Changes

1. **Thread MediaToolContext** — `RigAgentLoop::new()` gains `media_ctx: Option<Arc<MediaToolContext>>` parameter
2. **Store media_ctx on TaskExecutor** — Add field to `nika-engine/src/runtime/executor/mod.rs`
3. **Add web tools block** — In `rig_agent_loop/mod.rs` after file tools (line 489), add conditional registration for 5 web tools with `#[cfg(feature)]` gates
4. **Update callers** — `executor/agent.rs` passes `Some(Arc::clone(&self.media_ctx))`, `spawn.rs` and TUI pass `None`
5. **Update all tests** — Add `None` as last arg to all `RigAgentLoop::new()` calls

### Scope Behavior

| Scope | Core | File | Web | Introspection |
|-------|------|------|-----|---------------|
| `minimal` | complete + log | - | - | - |
| `full` (default) | all 7 | all 5 | - | - |
| `debug` | all 7 | all 5 | - | all 4 |
| `web` | all 7 | all 5 | all 5 | - |

Explicit `tools: [nika:html_to_md]` always overrides scope.

---

## IMP-005: iterate Construct

**Effort:** HIGH (RFC) | **Dep:** None | **Priority:** Low (post-v1.0)

### What

First-class feedback loop for dynamic crawling. `for_each` needs a static array — `iterate:` runs sequential waves where each wave's output seeds the next.

### Proposed Syntax

```yaml
- id: crawl
  iterate:
    seed: $initial_urls
    expand: $crawl.new_urls
    until: "{{with.new_urls | length}} == 0"
    max_iterations: 10
    concurrency: 5
    dedup: true
    accumulate: true
  fetch:
    url: "{{with.item}}"
    extract: links
```

### Execution Model

- Wave 0: items = seed array
- Wave N: items = expansion of wave N-1 outputs, minus visited (dedup)
- Stop: until condition met, max_iterations reached, or expansion is empty
- Output: accumulated array of all per-item outputs across all waves
- max_iterations exceeded → `PartialSuccess` (not failure)

### Implementation Phases

1. **AST only** — `IterateSpec` struct, parser, validator, mutually exclusive with for_each
2. **Execution** — Extract `execute_for_each_wave()`, implement `execute_iterate()` on Runner
3. **Until condition** — Parse numeric comparisons (`== 0`, `> N`, `< N`)
4. **Dedup + accumulation** — Visited set, cycle termination
5. **CLI/TUI display** — Wave progress

---

## IMP-009: for_each Output Coercion

**Effort:** Low | **Dep:** None | **Priority:** Medium

### Root Cause

The aggregation block at `runner.rs` lines 3080-3093 coerces `Value::String` to native types IF the string starts with `{` or `[`. This handles most cases but misses:
- LLM responses wrapped in markdown fences (` ```json ... ``` `)
- JSON with leading whitespace

### Fix

Replace the `starts_with` guard with `extract_json()` from `output.rs` which handles markdown fences and bracket-finding:

```rust
// In runner.rs aggregation block:
let outputs: Vec<Value> = results.iter().map(|(_, r)| {
    let val = (*r.output).clone();
    if let Value::String(ref s) = val {
        if let Ok(parsed) = extract_json(s) {
            return parsed;
        }
    }
    val
}).collect();
```

Make `extract_json` pub(crate) in `output.rs` if not already.

### Tests

1. structured output in for_each → `$task[0].field` works
2. exec with JSON output → parsed to native
3. plain string output → unchanged
4. markdown-fenced JSON → parsed correctly

---

## IMP-011: File-Based Data Flow

**Effort:** HIGH (architectural) | **Priority:** Low

### Problem

All inter-task data flows through template interpolation. For large datasets: memory pressure, NIKA-053 false positives on large data in exec commands, potential `{{` injection from HTML data.

### Recommended Approach: Two Phases

**Phase 1 (immediate):** Size limit on for_each aggregated outputs:
- `MAX_FOR_EACH_AGGREGATE_SIZE = 10MB`
- Fail with clear NIKA-281 message directing to `artifact:`

**Phase 2 (sprint):** CAS-backed DataRef:
- `DataRef` type: inline `Value` OR CAS hash
- Spill threshold configurable in nika.toml
- Lazy file read on template resolution with LRU cache
- Transparent to workflow authors

### Files to Create/Modify (Phase 2)

- Create: `nika-core/src/binding/dataref.rs`
- Modify: `nika-engine/src/store/run_context.rs` — swap Arc<Value> for DataRef
- Modify: `nika-engine/src/binding/resolve.rs` — dereference DataRef
- Modify: `nika-engine/src/runtime/runner.rs` — spill decision after aggregation

---

## DOC-001 through DOC-007

**Effort:** Low | **Target:** `~/.claude/rules/nika-bugs-and-patterns.md`

Seven documentation additions covering behaviors verified from source code:

| Doc | Topic | Summary |
|-----|-------|---------|
| DOC-001 | for_each + artifact: | Now writes aggregated array post-completion (BUG-018 fix) |
| DOC-002 | PartialSuccess deps | is_usable() returns true for PartialSuccess, downstream tasks run |
| DOC-003 | `\| to_json \| shell` | Safe JSON data injection pattern for exec commands |
| DOC-004 | nika:write vs artifact: | Permission model differences (BUG-021 fix: AcceptEdits allows Write) |
| DOC-005 | for_each output types | Coercion rules: starts with `{`/`[` → native, else stays String |
| DOC-006 | extract:links vs nika:extract_links | Different schemas, when to use which |
| DOC-007 | Template injection safety | 3-pass resolver prevents injection, exec needs `\| shell` |

Each doc entry includes YAML examples showing correct and incorrect patterns.
