# Nika Crawler Hardening — Execution Prompt

> Give this ENTIRE file as the prompt to an autonomous agent.
> It contains ALL context, ALL fixes, ALL caveats from 20+ research/review agents.

---

## Mission

Implement 7 phases of improvements to Nika's workflow engine, focusing on:
1. Null handling fixes (BUG-034/035/036/038) — the #1 blocker
2. Data tools (nika:map, nika:filter, nika:group_by, nika:aggregate)
3. New transforms (starts_with, ends_with, contains, content_hash, unique_urls)
4. Engine fixes (BUG-037, IMP-028 response:slim, IMP-030 sitemap keys)
5. Crawl ethics (robots.txt, per-domain rate limiting)
6. HTTP intelligence (cookies, ETag/304 caching)
7. RAG builtins (nika:chunk, nika:token_count)

---

## Workspace

```
~/dev/supernovae/nika/tools/          ← Cargo workspace root
├── nika-core/src/
│   ├── ast/extract.rs                ← ExtractMode, ResponseMode enums
│   ├── ast/analyzed/task.rs          ← AnalyzedForEach struct
│   └── binding/transform.rs          ← 36 transforms (url_normalize, slice, etc.)
├── nika-engine/src/
│   ├── binding/
│   │   ├── template.rs               ← Template resolution ({{with.x}}) — BUG-035
│   │   ├── resolve.rs                ← Binding resolution ($task.field) — BUG-038
│   │   └── jsonpath.rs               ← JSONPath query — BUG-037
│   ├── runtime/
│   │   ├── runner.rs                 ← for_each loop, artifact write — BUG-034/036/040
│   │   ├── executor/
│   │   │   ├── fetch.rs              ← HTTP fetch, response modes — IMP-028
│   │   │   └── extract.rs            ← 10 extract modes, sitemap — IMP-030
│   │   ├── robots.rs                 ← NEW FILE (Phase 5)
│   │   ├── rate_limit.rs             ← NEW FILE (Phase 5)
│   │   └── fetch_cache.rs            ← NEW FILE (Phase 6)
│   │   └── builtin/
│   │       ├── data_tools.rs         ← json_merge, set_diff, zip, json_query + NEW tools
│   │       ├── router.rs             ← Tool registration
│   │       ├── trait.rs              ← BuiltinTool trait
│   │       └── mod.rs                ← Module exports
│   ├── error.rs                      ← NikaError enum (NIKA-052, NIKA-072, etc.)
│   └── store/run_context.rs          ← TaskOutcome, is_usable()
├── nika-event/src/log.rs             ← EventLog
└── Cargo.toml                        ← Workspace deps (already has texting_robots, text-splitter, etc.)
```

## Commands

```bash
cd ~/dev/supernovae/nika/tools
cargo test --workspace --lib --exclude nika-py    # ALWAYS --lib, ALWAYS --exclude nika-py
cargo test -p nika-engine --lib -- test_name      # Targeted
cargo test -p nika-core --lib -- test_name        # Targeted
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

## TDD Workflow (MANDATORY)

```
For EACH fix:
1. Read the EXACT source code (never code blind)
2. Write test FIRST (RED) — verify it FAILS
3. Implement minimal fix (GREEN)
4. cargo test — verify GREEN
5. cargo fmt && cargo clippy — 0 warnings
6. Commit: type(scope): description
```

## Commit format
```
type(scope): description

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

---

# PHASE 1: Null Handling (3h, 7 tasks)

## CRITICAL CAVEATS (from code review)

### Template null behavior:
- `value_to_display()` at `template.rs:258` converts `Value::Null` → empty string `""`
- `value_to_string()` at `template.rs:1802` converts `Value::Null` → NIKA-072 error
- `resolve_with()` uses `value_to_display()` (lenient path)
- `resolve()` uses `value_to_string()` (strict path)

### Transform null behavior:
- `| default("x")` on null → returns `"x"` (SAFE)
- `| trim` on null → NIKA-153 NullInput error (CRASH)
- `| upper` on null → NIKA-153 NullInput error (CRASH)

### Implication: returning `Value::Null` for missing keys is ONLY safe if:
- The transform chain starts with `| default()`, OR
- No transforms are applied (value_to_display converts to `""`)

---

## 1.1 — BUG-034: for_each on null = 0 iterations

**File**: `nika-engine/src/runtime/runner.rs` — function `value_to_array()` around line 171

**Read first**: lines 165-190 of runner.rs

**Current code**:
```rust
fn value_to_array(value: &Value) -> Option<Vec<Value>> {
    if let Some(arr) = value.as_array() {
        return Some(arr.clone());
    }
    if let Some(s) = value.as_str() {
        if let Ok(extracted) = extract_json(s) {
            if let Some(arr) = extracted.as_array() {
                return Some(arr.clone());
            }
        }
    }
    None
}
```

**Fix**: Add null check at top:
```rust
fn value_to_array(value: &Value) -> Option<Vec<Value>> {
    // BUG-034: Treat null as empty array — 0 iterations, not error.
    // Every workflow engine (Temporal, Airflow, Prefect) does this.
    if value.is_null() {
        return Some(vec![]);
    }
    // ... rest unchanged
}
```

**Tests** (2):
```rust
#[test]
fn value_to_array_null_returns_empty() {
    assert_eq!(value_to_array(&Value::Null), Some(vec![]));
}

#[test]
fn value_to_array_empty_array_returns_empty() {
    assert_eq!(value_to_array(&json!([])), Some(vec![]));
}
```

**Commit**: `fix(runtime): for_each treats null as empty array (BUG-034)`

---

## 1.2 — BUG-038: Binding path resolution lenient for missing keys

**File**: `nika-engine/src/binding/resolve.rs` — function `resolve_entry()` around line 607

**Read first**: lines 540-640 of resolve.rs

**Root cause**: When path navigation returns None (missing key), the code throws NIKA-052 without checking if `??` default or `| default()` transform exists.

**Fix**: In the `None` case for path resolution, check for default/transform before throwing:
```rust
None => {
    // Check ?? default first
    if let Some(ref default_val) = entry.default {
        return Ok(default_val.clone());
    }
    // Check | default() transform
    if let Some(ref expr) = entry.transform {
        if expr.has_default() {
            return match expr.apply(&Value::Null) {
                Ok(v) => Ok(v),
                Err(_) => Err(NikaError::PathNotFound { path: path.clone() }),
            };
        }
    }
    Err(NikaError::PathNotFound { path: path.clone() })
}
```

**Tests** (3):
- `binding_missing_field_with_double_question_mark` → returns fallback
- `binding_missing_field_with_pipe_default` → returns default
- `binding_missing_field_no_default_errors` → still throws NIKA-052

**Commit**: `fix(binding): lenient path resolution with ?? and | default() support (BUG-038)`

---

## 1.3 — BUG-035: Template path `| default()` — CAREFUL implementation

**File**: `nika-engine/src/binding/template.rs` — function `resolve_alias_path()` around line 327

**Read first**: lines 268-350 AND lines 409-491 of template.ts (the calling code `resolve_with()`)

**DANGER**: Simply returning `Ok(Value::Null)` has these consequences:
- `{{with.obj.missing}}` → empty string (via value_to_display) — SILENT FAILURE
- `{{with.obj.missing | default("x")}}` → "x" — CORRECT
- `{{with.obj.missing | trim}}` → NIKA-153 crash — BAD

**SAFE FIX**: The fix must be in `resolve_with()` (the caller), NOT in `resolve_alias_path()` itself.

In `resolve_with()` around line 409, catch the NIKA-052 error and check if transforms include `default()`:

```rust
match resolve_alias_path(path, with_values) {
    Ok(value) => {
        // ... existing transform + display logic
    }
    Err(e) => {
        // BUG-035: If transforms include default(), recover with null
        if !transforms.is_empty() {
            let transform_str = transforms.join(" | ");
            if let Ok(expr) = TransformExpr::parse(&transform_str) {
                if expr.has_default() {
                    // Apply transform chain to null — default() will catch it
                    match expr.apply(&Value::Null) {
                        Ok(transformed) => {
                            result.push_str(&value_to_display(&transformed));
                            continue; // skip error collection
                        }
                        Err(_) => {} // fall through to error
                    }
                }
            }
        }
        // Original error handling
        let msg = format!("{}", e);
        if msg.contains("exceeds maximum") || msg.contains("Empty alias path") {
            return Err(e);
        }
        errors.push(path.clone());
    }
}
```

**KEY**: `resolve_alias_path()` STAYS STRICT. Only the caller catches the error when `| default()` is in the transform chain.

**Tests** (3):
- `template_missing_key_with_default` → `{{with.obj.missing | default("N/A")}}` → `"N/A"`
- `template_missing_key_no_default_still_errors` → `{{with.obj.missing}}` → NIKA-052
- `template_missing_key_with_non_default_transform_errors` → `{{with.obj.missing | trim}}` → NIKA-052 (NOT NIKA-153)

**Commit**: `fix(template): allow | default() on missing paths without silent null (BUG-035)`

---

## 1.4 — BUG-036: `??` in for_each binding (simple path only)

**File**: `nika-engine/src/runtime/runner.rs` — for_each resolution

**Read first**: lines 2037-2130 of runner.rs

**CAVEAT**: The `??` operator ALREADY WORKS for the pipe-expression path at runner.rs:2107. The bug is ONLY in the simple `$task.field` path (no `|` or `??` markers).

**Fix**: In the simple path resolution branch, when path navigation fails, check if a `??` default was parsed and use it. If items_str does not contain `|` or `??`, the code takes the simple branch — add null-to-empty-array fallback there.

**Tests** (2):
- `for_each_null_binding_zero_iterations` — binding resolves to null → `[]`
- `for_each_empty_array_literal_zero_iterations` — regression test

**Commit**: `fix(runtime): for_each null binding yields 0 iterations (BUG-036)`

---

## 1.5-1.7 — Integration tests + phase review

Run full test suite after all Phase 1 fixes. Verify no regressions.

---

# PHASE 2: Data Tools (4h, 8 tasks)

## Pattern to follow

Read `nika-engine/src/runtime/builtin/data_tools.rs` FULLY before writing any tool. Every tool follows:

```rust
pub struct ToolNameTool;

#[derive(Debug, Deserialize)]
struct ToolNameParams {
    // Use Value (not Vec<Value>) for array params — handles string-encoded JSON
    array: Value,
    // ...
}

impl BuiltinTool for ToolNameTool {
    fn name(&self) -> &'static str { "tool_name" }
    fn description(&self) -> &'static str { "..." }
    fn parameters_schema(&self) -> serde_json::Value { json!({...}) }
    fn call<'a>(&'a self, args: String) -> Pin<Box<dyn Future<Output = Result<String, NikaError>> + Send + 'a>> {
        Box::pin(async move {
            let params: ToolNameParams = serde_json::from_str(&args).map_err(|e| NikaError::BuiltinToolError {
                tool: "nika:tool_name".into(),
                reason: format!("Invalid parameters: {e}"),
            })?;
            // Convert Value to array
            let array = params.array.as_array().ok_or_else(|| NikaError::BuiltinToolError {
                tool: "nika:tool_name".into(),
                reason: "Expected array for 'array' parameter".into(),
            })?;
            // ... logic ...
            serde_json::to_string(&result).map_err(|e| NikaError::BuiltinToolError {
                tool: "nika:tool_name".into(),
                reason: format!("Serialization failed: {e}"),
            })
        })
    }
}
```

Register in `router.rs`:
```rust
tools.insert("tool_name", Arc::new(ToolNameTool));
```

Export in `mod.rs`:
```rust
pub use data_tools::ToolNameTool;
```

## IMPORTANT: Use `Value` not `Vec<Value>` for array params
Upstream task output may be a JSON string, not a raw array. `Vec<Value>` deserialization will fail on string-encoded JSON. Use `Value` and manually convert with `.as_array()`.

---

## 2.1 — nika:map

**Params**: `{ array: Value, selector: String }`

**selector** is a simple dot-path field name (NOT full JSONPath) for performance. Examples: `"loc"`, `"name"`, `"address.city"`.

**Implementation**: For each element in array, navigate dot-path and collect result. If field missing → `Value::Null` in output.

```rust
fn extract_field(value: &Value, path: &str) -> Value {
    let mut current = value;
    for segment in path.split('.') {
        match current.get(segment) {
            Some(v) => current = v,
            None => return Value::Null,
        }
    }
    current.clone()
}
```

**Tests** (5): extract field, identity (selector "."), nested field, missing field → null, empty array → []

**Commit**: `feat(builtin): add nika:map tool for per-element field extraction`

---

## 2.2 — nika:filter

**Params**: `{ array: Value, field: String, op: String, value: Value }`

**NOT using JSONPath filter syntax** (too risky — serde_json_path string literal support unverified). Instead, use explicit field + operator + value:

```yaml
invoke:
  tool: nika:filter
  params:
    array: "{{with.pages}}"
    field: "status"
    op: "eq"          # eq, ne, gt, lt, gte, lte, contains, starts_with
    value: 200
```

**Implementation**: Extract field from each element, compare with operator.

```rust
fn matches(element: &Value, field: &str, op: &str, compare: &Value) -> bool {
    let val = extract_field(element, field);  // reuse from nika:map
    match op {
        "eq" => &val == compare,
        "ne" => &val != compare,
        "gt" => val.as_f64().zip(compare.as_f64()).map(|(a, b)| a > b).unwrap_or(false),
        "lt" => val.as_f64().zip(compare.as_f64()).map(|(a, b)| a < b).unwrap_or(false),
        "gte" => val.as_f64().zip(compare.as_f64()).map(|(a, b)| a >= b).unwrap_or(false),
        "lte" => val.as_f64().zip(compare.as_f64()).map(|(a, b)| a <= b).unwrap_or(false),
        "contains" => val.as_str().zip(compare.as_str()).map(|(a, b)| a.contains(b)).unwrap_or(false),
        "starts_with" => val.as_str().zip(compare.as_str()).map(|(a, b)| a.starts_with(b)).unwrap_or(false),
        "ends_with" => val.as_str().zip(compare.as_str()).map(|(a, b)| a.ends_with(b)).unwrap_or(false),
        _ => false,
    }
}
```

**Tests** (6): eq numeric, eq string, gt, contains, starts_with, empty result

**Commit**: `feat(builtin): add nika:filter tool for predicate-based array filtering`

---

## 2.3 — nika:group_by

**Params**: `{ array: Value, key: String }`

**Implementation**: Extract key field from each element, group into `HashMap<String, Vec<Value>>`.

**Tests** (4): group by string, group by number, missing key → "null" group, empty array

**Commit**: `feat(builtin): add nika:group_by tool for array grouping`

---

## 2.4 — nika:aggregate

**Params**: `{ array: Value, field: Option<String>, operation: String }`

**Operations**: count, sum, avg, min, max, all

**Tests** (6): count, sum, avg, min, max, all combined

**Commit**: `feat(builtin): add nika:aggregate tool for array statistics`

---

## 2.5 — Register all 4 tools + phase review

---

# PHASE 3: New Transforms (2h, 6 tasks)

## 3.1 — starts_with(prefix), ends_with(suffix), contains(text)

**File**: `nika-core/src/binding/transform.rs`

Add 3 parametric variants: `StartsWith(String)`, `EndsWith(String)`, `Contains(String)`

**Parser**: In `parse_single_op()`, add to the parametric match block:
```rust
"starts_with" => Ok(TransformOp::StartsWith(strip_quotes(arg).to_string())),
"ends_with" => Ok(TransformOp::EndsWith(strip_quotes(arg).to_string())),
"contains" => Ok(TransformOp::Contains(strip_quotes(arg).to_string())),
```

**Apply**: Return `Value::Bool`. Null input → NullInput error.

**Display**: `starts_with('prefix')`, etc.

**Tests** (6): true/false for each

**Commit**: `feat(core): add starts_with, ends_with, contains transforms`

---

## 3.2 — content_hash

**File**: `nika-core/src/binding/transform.rs`

Use `xxhash_rust::xxh3::xxh3_64` (NOT xxh3_128) for consistency with rest of codebase.

```rust
TransformOp::ContentHash => match value {
    Value::Null => Err(TransformError::NullInput { op: "content_hash" }),
    Value::String(s) => {
        let hash = xxhash_rust::xxh3::xxh3_64(s.as_bytes());
        Ok(Value::String(format!("{:016x}", hash)))
    }
    _ => {
        let json = serde_json::to_string(value).expect("Value is serializable");
        let hash = xxhash_rust::xxh3::xxh3_64(json.as_bytes());
        Ok(Value::String(format!("{:016x}", hash)))
    }
},
```

**Tests** (3): deterministic, different input → different hash, object hashing

**Commit**: `feat(core): add content_hash transform (xxh3_64) for content dedup`

---

## 3.3 — unique_urls

**File**: `nika-core/src/binding/transform.rs`

**CAVEAT**: There is no standalone `normalize_url()` function. The logic is inside `TransformOp::UrlNormalize.apply()`. Two options:
1. Extract normalization into a free function `pub fn normalize_url_string(s: &str) -> String` and call from both `UrlNormalize` and `UniqueUrls`
2. Call `TransformOp::UrlNormalize.apply(&Value::String(s))` from within `UniqueUrls`

**Option 2 is simpler** (no refactor needed):

```rust
TransformOp::UniqueUrls => match value {
    Value::Null => Err(TransformError::NullInput { op: "unique_urls" }),
    Value::Array(arr) => {
        let mut seen = std::collections::HashSet::new();
        let unique: Vec<Value> = arr.iter().filter(|v| {
            let key = match TransformOp::UrlNormalize.apply(v) {
                Ok(Value::String(normalized)) => normalized,
                _ => v.to_string(),
            };
            seen.insert(key)
        }).cloned().collect();
        Ok(Value::Array(unique))
    }
    _ => Err(type_mismatch("unique_urls", "array", value)),
},
```

**Tests** (3): dedup tracking params, trailing slash, preserves order

**Commit**: `feat(core): add unique_urls transform for normalized URL dedup`

---

## 3.4-3.6 — Parser entries, Display impl, phase review

---

# PHASE 4: Engine Fixes (3h, 8 tasks)

## 4.1 — BUG-037: json_query returns [] for empty results

**CRITICAL**: Fix in `data_tools.rs::JsonQueryTool::call()` ONLY, NOT in `jsonpath.rs::query()`.

The shared `jsonpath::query()` function's null-return contract is used by the binding system. Changing it would break `$task.json_query_result ?? "fallback"`.

**Fix in data_tools.rs**:
```rust
// In JsonQueryTool::call():
let results = crate::binding::jsonpath::query(&params.data, &params.query)?;
// BUG-037: Convert null (no matches) to empty array for tool output
let output = if results.is_null() {
    Value::Array(vec![])
} else {
    results
};
serde_json::to_string(&output)...
```

**Tests** (2):
- `json_query_tool_no_match_returns_empty_array` — NOT null
- `json_query_tool_single_match_returns_value` — regression, still unwrapped

**Commit**: `fix(builtin): json_query returns [] not null for empty results (BUG-037)`

---

## 4.2 — IMP-028: response:slim

**File 1**: `nika-core/src/ast/extract.rs` — add `Slim` to ResponseMode enum
**File 2**: `nika-engine/src/runtime/executor/fetch.rs` — construct slim JSON

Read ResponseMode enum at `extract.rs` first. Add:
```rust
pub enum ResponseMode {
    Full,
    Binary,
    Slim,  // NEW: status + url + elapsed_ms + redirects + extracted, NO body/headers
}
```

Update `ALL_NAMES`, `parse()`, `as_str()`.

In `fetch.rs`, handle Slim before Full (they share the redirect chain + extract logic):
```json
// Slim output (no body, no headers):
{"status": 200, "url": "...", "elapsed_ms": 42, "redirects": [], "redirect_count": 0}
// With extract:
{"status": 200, "url": "...", "elapsed_ms": 42, "redirects": [], "redirect_count": 0, "extracted": {...}}
```

**Tests** (2): wiremock slim no body, wiremock slim + extract:metadata

**Commit**: `feat(fetch): add response:slim mode — metadata without body/headers (IMP-028)`

---

## 4.3 — IMP-030: sitemap consistent output keys

**File**: `nika-engine/src/runtime/executor/extract.rs` — sitemap output construction (search for `is_index`)

Add `"sitemaps": []` to urlset output and `"urls": []` to sitemapindex output.

**Tests** (2): urlset has sitemaps key, sitemapindex has urls key

**Commit**: `fix(extract): consistent output keys for sitemap urlset/index (IMP-030)`

---

## 4.4 — BUG-040: Verify PartialSuccess artifact write

**Investigation**: Code at `runner.rs:3186` uses `is_usable()` which INCLUDES PartialSuccess. The bug may already be fixed.

**Action**: Write a test. If it passes → document as already fixed. If it fails → investigate.

**Commit**: `test(runtime): verify PartialSuccess for_each writes artifact (BUG-040)`

---

## 4.5-4.8 — AST updates for response:slim + phase review

---

# PHASE 5: Crawl Ethics (3h, 6 tasks)

## 5.1-5.2 — robots.txt with texting_robots

**New file**: `nika-engine/src/runtime/robots.rs`

**Crate**: `texting_robots` 0.2.2 (already in workspace Cargo.toml)
- API: `Robot::new("nika/0.63", bytes)` → `robot.allowed(url)` → bool
- Does NOT fetch robots.txt — we control fetching + caching
- Supports Crawl-Delay as f32

```rust
use texting_robots::Robot;
use parking_lot::RwLock;
use rustc_hash::FxHashMap;

pub struct RobotsCache {
    cache: RwLock<FxHashMap<String, Option<Robot>>>,
    user_agent: String,
}

impl RobotsCache {
    pub fn new(user_agent: &str) -> Self { ... }

    pub async fn is_allowed(&self, url: &url::Url, client: &reqwest::Client) -> bool {
        let domain = url.host_str().unwrap_or_default().to_string();
        // Check cache
        if let Some(entry) = self.cache.read().get(&domain) {
            return match entry {
                Some(robot) => robot.allowed(url.as_str()),
                None => true, // 404 robots.txt = allow all
            };
        }
        // Fetch and cache
        let robots_url = format!("{}://{}/robots.txt", url.scheme(), domain);
        let robot = match client.get(&robots_url).timeout(Duration::from_secs(5)).send().await {
            Ok(resp) if resp.status().is_success() => {
                let body = resp.bytes().await.unwrap_or_default();
                Robot::new(&self.user_agent, &body).ok()
            }
            _ => None, // 404 or error = allow all
        };
        let allowed = robot.as_ref().map(|r| r.allowed(url.as_str())).unwrap_or(true);
        self.cache.write().insert(domain, robot);
        allowed
    }
}
```

**Integration**: In fetch.rs, before HTTP request, check robots.txt if enabled.

**Tests** (5): allows, blocks, caches, 404 allows all, crawl-delay

**Commit**: `feat(crawl): robots.txt compliance with texting_robots (RFC 9309)`

---

## 5.3-5.4 — Per-domain rate limiting with governor

**New file**: `nika-engine/src/runtime/rate_limit.rs`

**Crate**: `governor` 0.10 (already in workspace Cargo.toml)

```rust
use governor::{Quota, RateLimiter};
use governor::clock::DefaultClock;
use governor::state::keyed::DashMapStateStore;
use std::num::NonZeroU32;

pub struct DomainRateLimiter {
    limiter: RateLimiter<String, DashMapStateStore<String>, DefaultClock>,
}

impl DomainRateLimiter {
    pub fn new(requests_per_second: u32) -> Self {
        let quota = Quota::per_second(NonZeroU32::new(requests_per_second).unwrap_or(NonZeroU32::MIN));
        Self { limiter: RateLimiter::dashmap(quota) }
    }

    pub async fn acquire(&self, domain: &str) {
        self.limiter.until_key_ready(&domain.to_string()).await;
    }
}
```

**NOTE**: Do NOT specify explicit type params on the struct field. Use `RateLimiter::dashmap()` constructor which infers everything.

**Tests** (3): basic limiting, per-domain isolation, different rates

**Commit**: `feat(fetch): per-domain rate limiting via governor`

---

## 5.5-5.6 — Integration + phase review

---

# PHASE 6: HTTP Intelligence (4h, 8 tasks)

## 6.0 — Add cookie_store + reqwest_cookie_store to workspace Cargo.toml

```toml
# In [workspace.dependencies]:
cookie_store = "0.22"
reqwest_cookie_store = "0.10"
```

**Test**: `cargo check --workspace`

**Commit**: `chore(deps): add cookie_store and reqwest_cookie_store`

---

## 6.1-6.2 — Cookie jar / session persistence

**Crate**: `cookie_store` 0.22 + `reqwest_cookie_store` 0.10

```rust
use cookie_store::CookieStore;
use reqwest_cookie_store::CookieStoreRwLock;

// Per-workflow cookie jar:
let cookie_jar = Arc::new(CookieStoreRwLock::new(CookieStore::default()));

// Pass to EVERY reqwest Client built (including SSRF-pinned custom clients):
let client = reqwest::Client::builder()
    .cookie_provider(Arc::clone(&cookie_jar))
    .build()?;
```

**CRITICAL**: Every custom client (SSRF pinning, redirect tracking, emulation) MUST receive the cookie_provider. Search for `Client::builder()` in fetch.rs and ensure ALL paths include it.

**New AST field**: `session: true` on fetch verb (opt-in, default false)

**Tests** (3): cookies persist across tasks, isolated per workflow, domain-scoped

**Commit**: `feat(fetch): shared cookie jar per workflow for session persistence`

---

## 6.3-6.5 — ETag / conditional requests

**Crate**: `http-cache-semantics` 3.0 (already in workspace)

**New file**: `nika-engine/src/runtime/fetch_cache.rs`

Use DashMap for hot cache + optional SQLite persistence in `.nika/cache/fetch.db`.

**New AST field**: `cache: true` on fetch verb (opt-in, default false)

**Tests** (4): 304 returns cached, 200 updates cache, sends If-None-Match, sends If-Modified-Since

**Commit**: `feat(fetch): conditional requests with ETag/If-Modified-Since caching`

---

## 6.6-6.8 — AST updates + events + phase review

---

# PHASE 7: RAG Builtins (3h, 6 tasks)

## 7.1-7.2 — nika:chunk

**Crate**: `text-splitter` 0.29.3 (already in workspace Cargo.toml)

Add to `nika-engine/Cargo.toml`:
```toml
text-splitter = { workspace = true }
```

**Implementation in data_tools.rs** (not MediaOp — no CAS/binary needed):

```rust
pub struct ChunkTool;

#[derive(Debug, Deserialize)]
struct ChunkParams {
    text: String,
    #[serde(default = "default_chunk_size")]
    chunk_size: usize,
    #[serde(default)]
    overlap: usize,
    #[serde(default = "default_mode")]
    mode: String,  // "text" | "markdown"
}

fn default_chunk_size() -> usize { 1000 }
fn default_mode() -> String { "text".into() }
```

**API** (text-splitter 0.29):
```rust
use text_splitter::{TextSplitter, MarkdownSplitter, ChunkConfig};

let config = ChunkConfig::new(params.chunk_size).with_overlap(params.overlap);
let chunks: Vec<&str> = match params.mode.as_str() {
    "markdown" => MarkdownSplitter::new(config).chunks(&params.text).collect(),
    _ => TextSplitter::new(config).chunks(&params.text).collect(),
};
```

**Output**:
```json
{"chunks": ["chunk1...", "chunk2..."], "count": 5, "mode": "markdown", "chunk_size": 1000, "overlap": 50}
```

**Tests** (6): markdown headings, respects max size, overlap, small text single chunk, empty text, character mode

**Commit**: `feat(builtin): add nika:chunk tool for RAG text chunking`

---

## 7.3-7.4 — nika:token_count

**Implementation in data_tools.rs**:

```rust
pub struct TokenCountTool;

#[derive(Debug, Deserialize)]
struct TokenCountParams {
    text: String,
    #[serde(default = "default_tokenizer")]
    model: String,  // "heuristic" (default) | "cl100k_base" | "o200k_base"
}

fn default_tokenizer() -> String { "heuristic".into() }
```

**Heuristic mode** (zero deps): `text.len() / 4` — OpenAI's own approximation.

**Tiktoken mode** (behind feature flag — 4MB binary impact):
```rust
#[cfg(feature = "rag-tiktoken")]
{
    use tiktoken_rs::cl100k_base;
    let bpe = cl100k_base().unwrap();
    bpe.encode_ordinary(&params.text).len()
}
```

**Tests** (4): heuristic count, empty text, unicode, model parameter

**Commit**: `feat(builtin): add nika:token_count tool for token counting`

---

## 7.5-7.6 — Register + phase review

---

# Verification Checklist (after ALL phases)

```bash
# Must all pass:
cargo test --workspace --lib --exclude nika-py
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

Expected outcomes:
- 80+ new tests
- 0 regressions
- 6 new builtins: map, filter, group_by, aggregate, chunk, token_count
- 5 new transforms: starts_with, ends_with, contains, content_hash, unique_urls
- 1 new response mode: slim
- 4 critical bugs fixed: BUG-034/035/036/038
- 2 important bugs fixed: BUG-037, IMP-030
- robots.txt + rate limiting operational
- Cookie/ETag support operational
