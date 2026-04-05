# Nika Best-in-Class Crawler — Mega Plan v3

> 2026-04-03 | 16 research agents + 4 code exploration agents + production crawl findings
> Sources: v8 session (htmx.org 185 pages, qrcode-ai.com 1586 pages), 12 Perplexity/crate research agents
> Autonomous execution: ~16-20h TDD, 7 phases, 52 tasks

---

## Ground Rules

```
WORKFLOW:  Read → Test FIRST → Code → Verify → Commit
TDD:      RED → GREEN → REFACTOR (no exceptions)
COMMITS:  1 fix = 1 commit, type(scope): description
TESTS:    cargo test --workspace --lib --exclude nika-py
VERIFY:   cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings
```

### Co-author lines (every commit)
```
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

---

## Priority Map (from production crawl data)

```
TIER 0 — BLOCKING for 100% native workflows (zero python/jq):
  Phase 1   Null handling (BUG-034/035/038) — the #1 blocker
  Phase 2   Data tools (nika:map, nika:filter, nika:group_by, nika:aggregate)
  Phase 3   Transforms (starts_with, ends_with, contains, content_hash, unique_urls)

TIER 1 — HIGH VALUE for production crawling:
  Phase 4   Engine fixes (BUG-037 json_query null, BUG-040 PartialSuccess artifact,
            IMP-028 response:slim, IMP-030 sitemap consistent keys)
  Phase 5   Crawl ethics (robots.txt, per-domain rate limiting)

TIER 2 — COMPETITIVE ADVANTAGE:
  Phase 6   HTTP intelligence (cookies, ETag/304, session persistence)
  Phase 7   RAG builtins (nika:chunk, nika:token_count)

DEFERRED (separate sprint):
  Phase 8   Anti-detection (wreq TLS emulation) — heavy build deps, feature-gated
```

---

## PHASE 1: Null Handling — The #1 Blocker (3h, 7 tasks)

### Root Cause (from code exploration)

3 code paths handle binding resolution. Only 1 is lenient:

| Path | File | Behavior | Status |
|------|------|----------|--------|
| Template `{{with.obj.missing}}` | `template.rs:327` | STRICT — throws NIKA-052 before transforms | **BROKEN** |
| for_each `$task.field` | `runner.rs:2080` | STRICT — manual path nav, no fallback | **BROKEN** |
| WithSpec `with: { x: $task.field }` | `resolve.rs:1049` | LENIENT — `navigate_segments()` returns `Ok(None)` | **WORKS** |

The fix: make ALL paths use lenient navigation, letting `| default()` and `??` handle missing keys.

### 1.1 — BUG-034: for_each on null = 0 iterations

**File**: `nika-engine/src/runtime/runner.rs` lines 171-187

**Root cause**: `value_to_array()` returns `None` for `Value::Null`. Downstream code treats `None` as error.

**Fix**: Add null check at top of `value_to_array()`:
```rust
fn value_to_array(value: &Value) -> Option<Vec<Value>> {
    // BUG-034: Treat null as empty array (0 iterations, not error)
    if value.is_null() {
        return Some(vec![]);
    }
    // ... rest unchanged
}
```

**Tests** (3):
- `for_each_null_items_returns_empty_array` — null → 0 iterations, result = `[]`
- `for_each_missing_field_returns_empty_array` — `$task.nonexistent` → 0 iterations
- `for_each_empty_array_returns_empty_array` — `[]` → 0 iterations (regression)

**Commit**: `fix(runtime): for_each treats null as empty array (BUG-034)`

### 1.2 — BUG-035: Template path resolution too strict

**File**: `nika-engine/src/binding/template.rs` lines 312-329

**Root cause**: `resolve_alias_path()` throws NIKA-052 on missing key BEFORE transforms (including `| default()`) are applied.

**Fix**: Return `Value::Null` instead of error when path segment not found in template resolution. The calling code (`resolve_with()` at line 400+) will then apply transforms including `default()`.

```rust
// BEFORE (line 327):
return Err(NikaError::PathNotFound { path: ... });

// AFTER:
return Ok(Value::Null);  // Let | default() handle it
```

**IMPORTANT**: Only return null for INTERMEDIATE path segments in objects/arrays. Root alias missing (`with.nonexistent_alias`) should still throw NIKA-071 (UnknownAlias).

**Tests** (4):
- `template_missing_key_with_default_returns_fallback` — `{{with.obj.missing | default("x")}}` → `"x"`
- `template_missing_key_without_default_returns_null_string` — `{{with.obj.missing}}` → `"null"`
- `template_missing_root_alias_still_errors` — `{{with.nonexistent}}` → NIKA-071
- `template_deep_missing_with_default` — `{{with.obj.a.b.c | default("deep")}}` → `"deep"`

**Commit**: `fix(template): lenient path resolution for missing keys (BUG-035)`

### 1.3 — BUG-038: Binding `$task.field` strict for missing keys

**File**: `nika-engine/src/binding/resolve.rs` lines 607-634

**Root cause**: `resolve_entry()` throws NIKA-052 when path doesn't exist, even if `??` default is available.

**Fix**: Ensure that when `navigate_segments()` returns `Ok(None)`, the code falls through to check for `??` default and `| default()` transform before throwing.

**Tests** (3):
- `binding_missing_field_with_double_question_mark` — `$task.missing ?? "fallback"` → `"fallback"`
- `binding_missing_field_with_pipe_default` — `$task.missing | default("x")` → `"x"`
- `binding_missing_field_no_default_errors` — `$task.missing` → NIKA-052 (intentional)

**Commit**: `fix(binding): lenient path resolution for missing keys in bindings (BUG-038)`

### 1.4 — BUG-036: `??` in for_each binding

**File**: `nika-engine/src/runtime/runner.rs` lines 2037-2103

**Root cause**: Manual path navigation in for_each doesn't support `??` operator.

**Fix**: When for_each binding fails to resolve, check for `??` default in the parsed entry and use it.

**Tests** (2):
- `for_each_binding_with_double_question_mark_fallback` — `$task.field ?? []` → 0 iterations
- `for_each_binding_with_pipe_default` — `$task.field | default([])` → 0 iterations

**Commit**: `fix(runtime): support ?? operator in for_each binding resolution (BUG-036)`

### 1.5-1.7 — Integration tests + phase review

- 1.5: E2E test: sitemap workflow with optional `sitemaps` field → for_each → 0 iterations
- 1.6: E2E test: `with: { x: $task.optional | default("none") }` through full pipeline
- 1.7: Full test suite + code review

---

## PHASE 2: Data Tools — The Missing Builtins (4h, 8 tasks)

### Pattern (from code exploration)

All tools follow identical 3-part structure in `data_tools.rs`:
1. Struct + Params with `#[derive(Deserialize)]`
2. `impl BuiltinTool` with `name()`, `description()`, `parameters_schema()`, `call()`
3. Register in `router.rs`: `tools.insert("tool_name", Arc::new(ToolNameTool))`

No new dependencies needed — `serde_json_path` (already used by json_query) handles JSONPath selectors and filter expressions.

### 2.1 — IMP-020: `nika:map` (CRITICAL — the biggest blocker)

**File**: `nika-engine/src/runtime/builtin/data_tools.rs`

```rust
pub struct MapTool;

#[derive(Debug, Deserialize)]
struct MapParams {
    array: Vec<Value>,
    /// JSONPath selector applied to each element: "$.name", "$.url", "$"
    selector: String,
}
```

**Behavior**: For each element, apply JSONPath selector and collect results.
```yaml
invoke:
  tool: nika:map
  params:
    array: "{{with.urls}}"
    selector: "$.loc"
# [{"loc":"https://a.com"}, {"loc":"https://b.com"}] → ["https://a.com", "https://b.com"]
```

**Implementation**: Use `serde_json_path::JsonPath::parse()` per element. For simple field access (no wildcards/filters), optimize with direct `value.get("field")`.

**Tests** (5):
- `map_extract_field` — `[{name:"A"},{name:"B"}]` + `$.name` → `["A","B"]`
- `map_identity` — `[1,2,3]` + `$` → `[1,2,3]`
- `map_nested_field` — `$.address.city` on objects
- `map_missing_field_returns_null` — missing field → `null` in output
- `map_empty_array` — `[]` → `[]`

**Commit**: `feat(builtin): add nika:map tool for per-element field extraction`

### 2.2 — IMP-021: `nika:filter`

```rust
pub struct FilterTool;

#[derive(Debug, Deserialize)]
struct FilterParams {
    array: Vec<Value>,
    /// JSONPath filter predicate: "@.age > 18", "@.status == 'active'"
    predicate: String,
}
```

**Implementation**: Wrap each element as `{"item": element}`, apply JSONPath `$[?<predicate>]`, collect matches.

Actually simpler: iterate array, for each element evaluate predicate using `serde_json_path` filter syntax. The trick: construct a temporary single-element array `[element]`, query with `$[?<predicate>]`, if result is non-empty → keep.

**Tests** (5):
- `filter_numeric_comparison` — `@.age > 18`
- `filter_string_equality` — `@.status == 'active'`
- `filter_all_match` — predicate matches everything → same array
- `filter_none_match` — predicate matches nothing → `[]`
- `filter_empty_array` — `[]` → `[]`

**Commit**: `feat(builtin): add nika:filter tool for predicate-based array filtering`

### 2.3 — IMP-022: `nika:group_by`

```rust
pub struct GroupByTool;

#[derive(Debug, Deserialize)]
struct GroupByParams {
    array: Vec<Value>,
    /// Field name to group by: "locale", "status", "category"
    key: String,
}
```

**Implementation**: Iterate array, extract `key` field from each element, build `HashMap<String, Vec<Value>>`, serialize as JSON object.

**Tests** (4):
- `group_by_string_field` — group pages by locale
- `group_by_numeric_field` — group by status code
- `group_by_missing_key` — elements without key go to `"null"` group
- `group_by_empty_array` — `[]` → `{}`

**Commit**: `feat(builtin): add nika:group_by tool for array grouping by field`

### 2.4 — IMP-023: `nika:aggregate`

```rust
pub struct AggregateTool;

#[derive(Debug, Deserialize)]
struct AggregateParams {
    array: Vec<Value>,
    #[serde(default)]
    field: Option<String>,  // extract field before aggregating
    operation: String,      // count, sum, avg, min, max, all
}
```

**Output for `all`**:
```json
{"count": 100, "sum": 5000, "avg": 50.0, "min": 10, "max": 200}
```

**Tests** (6):
- `aggregate_count` — array length
- `aggregate_sum` — numeric sum
- `aggregate_avg` — numeric average
- `aggregate_min_max` — min/max values
- `aggregate_all` — all stats combined
- `aggregate_with_field` — extract field then aggregate

**Commit**: `feat(builtin): add nika:aggregate tool for array statistics`

### 2.5 — Register all 4 tools in router

**File**: `nika-engine/src/runtime/builtin/router.rs`

```rust
use super::data_tools::{MapTool, FilterTool, GroupByTool, AggregateTool};
tools.insert("map", Arc::new(MapTool));
tools.insert("filter", Arc::new(FilterTool));
tools.insert("group_by", Arc::new(GroupByTool));
tools.insert("aggregate", Arc::new(AggregateTool));
```

**Commit**: `feat(builtin): register map, filter, group_by, aggregate in router`

### 2.6-2.8 — Integration tests + phase review

---

## PHASE 3: New Transforms (2h, 6 tasks)

### 3.1 — IMP-025: `starts_with(prefix)`, `ends_with(suffix)`, `contains(text)`

**File**: `nika-core/src/binding/transform.rs`

New variants: `StartsWith(String)`, `EndsWith(String)`, `Contains(String)`

```rust
TransformOp::StartsWith(prefix) => match value {
    Value::Null => Err(TransformError::NullInput { op: "starts_with" }),
    Value::String(s) => Ok(Value::Bool(s.starts_with(prefix.as_str()))),
    _ => Err(type_mismatch("starts_with", "string", value)),
},
```

Parser: `"starts_with" => { let arg = strip_quotes(arg); Ok(TransformOp::StartsWith(arg.to_string())) }`

**Tests** (6):
- `starts_with_true` / `starts_with_false`
- `ends_with_true` / `ends_with_false`
- `contains_true` / `contains_false`

**Commit**: `feat(core): add starts_with, ends_with, contains transforms`

### 3.2 — `content_hash` transform

**File**: `nika-core/src/binding/transform.rs`

Uses `xxhash_rust::xxh3::xxh3_128` (already in workspace deps). Fast, non-cryptographic, perfect for content dedup.

```rust
TransformOp::ContentHash => match value {
    Value::Null => Err(TransformError::NullInput { op: "content_hash" }),
    Value::String(s) => {
        let hash = xxhash_rust::xxh3::xxh3_128(s.as_bytes());
        Ok(Value::String(format!("{:032x}", hash)))
    }
    _ => {
        let json = serde_json::to_string(value).expect("Value is serializable");
        let hash = xxhash_rust::xxh3::xxh3_128(json.as_bytes());
        Ok(Value::String(format!("{:032x}", hash)))
    }
},
```

**Tests** (3):
- `content_hash_deterministic` — same input → same hash
- `content_hash_different` — different input → different hash
- `content_hash_object` — hashes JSON representation

**Commit**: `feat(core): add content_hash transform for content deduplication`

### 3.3 — `unique_urls` transform

Applies `url_normalize` logic to each element before dedup.

```rust
TransformOp::UniqueUrls => match value {
    Value::Array(arr) => {
        let mut seen = HashSet::new();
        let unique: Vec<Value> = arr.iter().filter(|v| {
            let key = if let Value::String(s) = v {
                normalize_url(s)  // reuse url_normalize logic
            } else {
                v.to_string()
            };
            seen.insert(key)
        }).cloned().collect();
        Ok(Value::Array(unique))
    }
    ...
}
```

**Tests** (3):
- `unique_urls_strips_tracking` — dedup URLs that differ only by utm params
- `unique_urls_trailing_slash` — `/page/` and `/page` deduped
- `unique_urls_preserves_order` — first occurrence kept

**Commit**: `feat(core): add unique_urls transform for normalized URL dedup`

### 3.4-3.6 — Parser entries, Display impl, phase review

---

## PHASE 4: Engine Fixes (3h, 8 tasks)

### 4.1 — BUG-037: json_query returns `[]` not `null` for empty results

**File**: `nika-engine/src/binding/jsonpath.rs` line 73

```rust
// BEFORE:
0 => Ok(Value::Null),
// AFTER:
0 => Ok(Value::Array(vec![])),
```

**Tests** (2):
- `query_no_match_returns_empty_array`
- `query_single_match_returns_value` (regression — still unwrapped)

**Commit**: `fix(jsonpath): return empty array instead of null for no matches (BUG-037)`

### 4.2 — IMP-028: `response: slim`

**File**: `nika-core/src/ast/extract.rs` — add `Slim` variant to `ResponseMode`
**File**: `nika-engine/src/runtime/executor/fetch.rs` — construct slim JSON

Slim = everything from Full EXCEPT `body` and `headers`:
```json
{"status": 200, "url": "...", "elapsed_ms": 42, "redirects": [], "redirect_count": 0, "extracted": {...}}
```

**Tests** (2):
- `wiremock_fetch_response_slim_no_body` — slim omits body + headers
- `wiremock_fetch_response_slim_with_extract` — slim + extract:metadata

**Commit**: `feat(fetch): add response:slim mode — status + url + elapsed without body/headers (IMP-028)`

### 4.3 — IMP-030: sitemap consistent output keys

**File**: `nika-engine/src/runtime/executor/extract.rs` lines 644-650

Add `"sitemaps": []` to urlset output and `"urls": []` to sitemapindex output.

**Tests** (2):
- `sitemap_urlset_has_sitemaps_key` — urlset includes `sitemaps: []`
- `sitemap_index_has_urls_key` — sitemapindex includes `urls: []`

**Commit**: `fix(extract): consistent output keys for sitemap urlset/index (IMP-030)`

### 4.4 — BUG-040: Verify PartialSuccess artifact write

**Investigation**: The exploration agent found that `runner.rs:3186` already uses `is_usable()` which includes PartialSuccess. The bug may be fixed or the issue is elsewhere (event emission, artifact validation).

**Action**: Write a test that verifies PartialSuccess for_each writes its artifact. If it passes → BUG-040 is already fixed. If it fails → investigate further.

**Test** (1):
- `for_each_partial_success_writes_artifact` — some iterations fail, artifact still written

**Commit**: `test(runtime): verify PartialSuccess for_each artifact write (BUG-040)`

### 4.5-4.8 — AST updates for response:slim + phase review

---

## PHASE 5: Crawl Ethics (3h, 6 tasks)

### 5.1 — robots.txt compliance

**Crate**: `texting_robots` 0.2.2 (already in workspace Cargo.toml)
- Tested against 34M real-world robots.txt files
- Supports Crawl-Delay as f32, sitemap discovery
- API: `Robot::new(user_agent, bytes)` → `robot.allowed(url)` → bool

**New file**: `nika-engine/src/runtime/robots.rs`

```rust
pub struct RobotsCache {
    cache: RwLock<FxHashMap<String, CachedRobot>>,
    user_agent: String,
}

impl RobotsCache {
    pub async fn is_allowed(&self, url: &url::Url, client: &reqwest::Client) -> bool { ... }
    pub fn crawl_delay(&self, domain: &str) -> Option<Duration> { ... }
}
```

**Integration**: Check before HTTP request in fetch.rs. Controlled by `nika.toml` policy.

**Tests** (5):
- robots cache allows uncached URL
- robots cache blocks disallowed path
- robots cache reuses cached entry
- 404 robots.txt allows all
- Crawl-delay parsed

**Commit**: `feat(crawl): robots.txt compliance with per-domain caching`

### 5.2 — Per-domain rate limiting

**Crate**: `governor` 0.10 (already in workspace Cargo.toml)

**New file**: `nika-engine/src/runtime/rate_limit.rs`

```rust
pub struct DomainRateLimiter {
    limiter: RateLimiter<String, DashMapStateStore<String>, DefaultClock>,
}

impl DomainRateLimiter {
    pub fn new(requests_per_second: u32) -> Self { ... }
    pub async fn acquire(&self, domain: &str) { ... }
}
```

**Config** (`nika.toml`):
```toml
[fetch]
rate_limit_per_domain = 5
respect_robots_txt = true
```

**Tests** (3):
- basic rate limiting
- per-domain isolation
- Crawl-delay override

**Commit**: `feat(fetch): per-domain rate limiting via governor`

### 5.3-5.6 — Integration + config parsing + events + phase review

---

## PHASE 6: HTTP Intelligence (4h, 8 tasks)

### 6.1-6.2 — Cookie jar / session persistence

**Crates**: `cookie_store` 0.22 + `reqwest_cookie_store` 0.10 (add to workspace)

- `CookieStoreRwLock` implements `reqwest::cookie::CookieStore`
- Pass to `reqwest::Client::builder().cookie_provider()`
- Per-workflow scope, JSON serializable for persistence
- **CRITICAL**: every custom client (SSRF pinning, redirect tracking) must also receive cookie_provider

**New AST field**: `session: true` on fetch verb (opt-in)

### 6.3-6.5 — ETag / conditional requests

**Crate**: `http-cache-semantics` 3.0 (stable, already in workspace)

- NOT using `http-cache-reqwest` (alpha, replaces reqwest::Client)
- Direct integration: `CachePolicy::new()`, `before_request()` → Fresh/Stale
- Storage: DashMap (hot) + optional SQLite in `.nika/cache/fetch.db`

**New AST field**: `cache: true` on fetch verb (opt-in)

### 6.6-6.8 — Events + wire-up + phase review

---

## PHASE 7: RAG Builtins (3h, 6 tasks)

### 7.1-7.2 — `nika:chunk`

**Crate**: `text-splitter` 0.29.3 (already in workspace Cargo.toml)
- Superior to LangChain's RecursiveCharacterTextSplitter
- 9 Markdown semantic levels
- Built-in overlap with binary search

**Implementation**: As `BuiltinTool` in `data_tools.rs` (not MediaOp — no CAS/binary needed).

```yaml
invoke:
  tool: nika:chunk
  params:
    text: "{{with.article}}"
    chunk_size: 500
    mode: markdown   # text | markdown
    overlap: 50
```

**Output**: `{ "chunks": ["...", "..."], "count": 12 }`

### 7.3-7.4 — `nika:token_count`

**Crate**: `tiktoken-rs` 0.9.1 (already in workspace, optional behind feature)

```yaml
invoke:
  tool: nika:token_count
  params:
    text: "{{with.content}}"
    model: "cl100k_base"   # cl100k_base | o200k_base | heuristic
```

**Output**: `{ "tokens": 1234, "characters": 5678, "model": "cl100k_base" }`

Default mode `heuristic` = `chars / 4` (zero deps, good enough for most cases).

### 7.5-7.6 — Register + phase review

---

## Execution Order

```
Phase 1 (3h)  → Null handling         ← THE BLOCKER, do first
Phase 2 (4h)  → Data tools            ← enables 100% native workflows
Phase 3 (2h)  → Transforms            ← complements data tools
Phase 4 (3h)  → Engine fixes          ← quality of life
Phase 5 (3h)  → Crawl ethics          ← robots.txt, rate limiting
Phase 6 (4h)  → HTTP intelligence     ← cookies, ETag
Phase 7 (3h)  → RAG builtins          ← nika:chunk, nika:token_count
```

**Phases 1-3 = CRITICAL PATH** (enable zero-script workflows)
**Phases 4-5 = HIGH VALUE** (production quality)
**Phases 6-7 = COMPETITIVE** (match/beat Firecrawl)

---

## New Crate Dependencies (all validated)

| Crate | Version | Phase | Status | Binary Impact |
|-------|---------|-------|--------|---------------|
| `texting_robots` | 0.2.2 | 5 | In workspace | ~50KB |
| `http-cache-semantics` | 3.0 | 6 | In workspace | ~40KB |
| `fastbloom` | 0.17 | 3 | In workspace | ~30KB |
| `text-splitter` | 0.29.3 | 7 | In workspace | ~200KB |
| `tiktoken-rs` | 0.9.1 | 7 | In workspace (optional) | ~4MB |
| `cookie_store` | 0.22 | 6 | **ADD to workspace** | ~30KB |
| `reqwest_cookie_store` | 0.10 | 6 | **ADD to workspace** | ~15KB |

reqwest features already enabled: `hickory-dns`, `zstd`, `cookies` (Phase 1 commit `736874002`)

---

## New Builtins Summary

| Tool | Phase | Dependencies | Complexity |
|------|-------|-------------|------------|
| `nika:map` | 2 | serde_json_path (exists) | Low |
| `nika:filter` | 2 | serde_json_path (exists) | Low |
| `nika:group_by` | 2 | None (pure Rust) | Low |
| `nika:aggregate` | 2 | None (pure Rust) | Low |
| `nika:chunk` | 7 | text-splitter | Medium |
| `nika:token_count` | 7 | tiktoken-rs (optional) | Low |

## New Transforms Summary

| Transform | Type | Phase |
|-----------|------|-------|
| `starts_with(prefix)` | `string → bool` | 3 |
| `ends_with(suffix)` | `string → bool` | 3 |
| `contains(text)` | `string → bool` | 3 |
| `content_hash` | `any → string` (xxh3_128) | 3 |
| `unique_urls` | `array → array` | 3 |

## New AST Fields

| Verb | Field | Type | Default | Phase |
|------|-------|------|---------|-------|
| `fetch:` | `session` | bool | false | 6 |
| `fetch:` | `cache` | bool | false | 6 |
| `response:` | `slim` | enum variant | — | 4 |

## Bug Fix Summary

| Bug | Description | Phase | Root File |
|-----|-------------|-------|-----------|
| BUG-034 | for_each null → 0 iterations | 1 | runner.rs:171 |
| BUG-035 | Template missing key + default() | 1 | template.rs:327 |
| BUG-036 | ?? in for_each binding | 1 | runner.rs:2080 |
| BUG-037 | json_query [] not null | 4 | jsonpath.rs:73 |
| BUG-038 | Binding $task.field lenient | 1 | resolve.rs:633 |
| BUG-040 | PartialSuccess artifact (verify) | 4 | runner.rs:3186 |
| IMP-028 | response:slim | 4 | fetch.rs + extract.rs |
| IMP-030 | sitemap consistent keys | 4 | extract.rs:644 |

---

## Success Criteria

After all 7 phases:
- [ ] `cargo test --workspace --lib --exclude nika-py` = ALL GREEN
- [ ] `cargo fmt --all --check` = clean
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` = 0
- [ ] 80+ new tests added
- [ ] Null handling works: `$task.missing | default()` everywhere
- [ ] 4 new data tools: map, filter, group_by, aggregate
- [ ] 5 new transforms: starts_with, ends_with, contains, content_hash, unique_urls
- [ ] response:slim mode working
- [ ] json_query returns [] for empty
- [ ] robots.txt compliance
- [ ] Per-domain rate limiting
- [ ] Cookie/session persistence
- [ ] ETag/304 caching
- [ ] nika:chunk producing correct chunks
- [ ] **Sitemap crawler workflow: ZERO python, ZERO jq, 100% native**

---

## Research Reports (on disk)

| Report | Path |
|--------|------|
| Competitive Analysis | `docs/research/2026-04-03-web-scraping-competitive-analysis.md` |
| Synthesis | `docs/research/2026-04-03-best-in-class-crawler-synthesis.md` |
| Embedding/Vector | `docs/research/2026-04-03-embedding-vector-search.md` |
| Fetch Capabilities | `docs/research/2026-04-03-fetch-web-capabilities.md` |
| Firecrawl Deep | `docs/research/2026-04-03-firecrawl-deep-analysis.md` |
| Crawl Research | `docs/research/2026-04-03-advanced-web-crawling-research.md` |
| v8 Session Findings | (provided inline — 12 bugs, 11 improvements) |
