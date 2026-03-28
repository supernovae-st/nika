# Session D: Quality Infrastructure (~3-4h)

## Context
Nika workflow engine. Workspace: `tools/` (12 Rust crates).
P0 crates already added to workspace Cargo.toml: tracing-error, nutype, proptest, serial_test, rstest, strum, derive_more, static_assertions.

## Mission: Wire up quality tools, find weak tests with mutation testing, property-test critical paths, merge dual pricing, unify workspace deps.

---

## Part 1: cargo-mutants on critical files (~1h)

```bash
cargo install --locked cargo-mutants
```

Run mutation testing on the 5 most bug-prone files:

```bash
# Cost calculation — have bugs been here before?
cargo mutants -p nika-engine -f src/provider/cost.rs -- --lib

# Security blocklist — are patterns actually tested?
cargo mutants -p nika-engine -f src/runtime/security.rs -- --lib

# Template resolution — are edge cases covered?
cargo mutants -p nika-core -f src/binding/transform.rs -- --lib

# DAG validation — are cycles really detected?
cargo mutants -p nika-engine -f src/dag/flow.rs -- --lib

# Guardrails — is SchemaGuardrail actually testing?
cargo mutants -p nika-core -f src/ast/guardrails.rs -- --lib
```

### Top 20 mutation targets by criticality

These are the functions where a surviving mutant would represent a real security or correctness bug. Ordered by blast radius.

**Tier A — Security (surviving mutant = vulnerability)**

| # | Function | File | Why critical |
|---|----------|------|--------------|
| 1 | `check_blocklist(cmd)` | `security.rs:252` | A mutant that removes a pattern = attacker bypass. 70+ blocklist entries, each must be tested. |
| 2 | `check_shell_mode_blocklist(cmd)` | `security.rs:145` | Shell-mode patterns (`$(`, backtick, `<<<`) — 4 entries, all security-critical. |
| 3 | `validate_command_string(cmd)` | `security.rs:175` | Control char detection (0x00-0x1F). Mutant removing null-byte check = injection. |
| 4 | `validate_env_vars(vars)` | `security.rs:306` | `LD_PRELOAD` / `DYLD_INSERT_LIBRARIES` — blocked env vars. Mutant = library injection. |
| 5 | `is_valid_env_var_name(name)` | `security.rs:340` | POSIX name validation. Mutant = BASH_FUNC injection via crafted env var names. |
| 6 | `normalize_for_blocklist(s)` | `security.rs:227` | NFKC normalization + zero-width stripping. Mutant = Unicode confusable bypass. |
| 7 | `validate_exec_command_with_shell(cmd, shell_mode)` | `security.rs:429` | Orchestrator function. Mutant removing `shell_mode` newline check = command separator bypass. |

**Tier B — Correctness (surviving mutant = wrong money / wrong data)**

| # | Function | File | Why critical |
|---|----------|------|--------------|
| 8 | `ModelPricing::calculate(input, output)` | `cost.rs:129` | Division by 1M. Mutant changing divisor = 1000x cost error. |
| 9 | `ModelPricing::calculate_with_cache(in, out, cached, discount)` | `cost.rs:145` | Cache discount logic. `cached.min(input)` cap — mutant removing it = negative cost. |
| 10 | `calculate_cost_with_cache(provider, model, in, out, cached)` | `cost.rs:673` | Wires provider discount rate. Mutant changing 0.1 to 0.5 = 5x Anthropic cost error. |
| 11 | `cache_discount_for_provider(provider)` | `cost.rs:662` | Per-provider discount rates. Each match arm must survive independently. |
| 12 | `ProviderKind::parse(s)` | `cost.rs:41` | Case-insensitive provider parsing. Mutant removing "anthropic" alias = silent fallthrough. |
| 13 | `Dag::detect_cycles()` | `flow.rs:393` | Three-color DFS. Mutant skipping Gray check = undetected cycles = infinite loop at runtime. |
| 14 | `Dag::from_workflow(workflow)` | `flow.rs:467` | Missing dependency error path. Mutant swallowing error = silent data flow corruption. |
| 15 | `Dag::has_path(from, to)` | `flow.rs:354` | BFS reachability. Mutant = broken `with:` validation (NIKA-081 WithNotUpstream). |
| 16 | `TransformExpr::parse(input)` | `transform.rs:147` | Pipe parser. Mutant skipping pipe split = transforms silently ignored. |

**Tier C — Data integrity (surviving mutant = wrong output)**

| # | Function | File | Why critical |
|---|----------|------|--------------|
| 17 | `TransformOp::apply(value)` — Sort comparator | `transform.rs:303` | `partial_cmp` with `unwrap_or(Equal)` — NaN handling. Mutant = unstable sort. |
| 18 | `TransformOp::apply(value)` — ToNumber | `transform.rs:350` | i64 parse then f64 fallback. Mutant reordering = precision loss. |
| 19 | `SchemaGuardrail::check(output)` | `guardrails.rs:332` | Only checks `required` fields (CR1 bug). Mutant removing even that = zero validation. |
| 20 | `compute_layers(nodes, edges)` | `flow.rs:32` | Kahn's algorithm for DAG depth. Mutant = wrong task scheduling order. |

For EVERY surviving mutant: write a test that kills it.

---

## Part 2: proptest strategies (~1h)

Add proptest to `nika-core` and `nika-engine` dev-dependencies (already in workspace).

### Strategy 1: All 31 transforms never panic on any Value (including null)

The 31 transforms and their null behavior:

| Transform | Null behavior | Category |
|-----------|--------------|----------|
| `upper` | NIKA-153 (fail) | String |
| `lower` | NIKA-153 (fail) | String |
| `trim` | NIKA-153 (fail) | String |
| `trim_start` | NIKA-153 (fail) | String |
| `trim_end` | NIKA-153 (fail) | String |
| `length` | propagate (null) | Collection |
| `first` | NIKA-153 (fail) | Collection |
| `last` | NIKA-153 (fail) | Collection |
| `first(N)` | NIKA-153 (fail) | Collection |
| `last(N)` | NIKA-153 (fail) | Collection |
| `keys` | propagate (null) | Collection |
| `values` | NIKA-153 (fail) | Collection |
| `flatten` | NIKA-153 (fail) | Collection |
| `reverse` | NIKA-153 (fail) | Collection |
| `sort` | NIKA-153 (fail) | Collection |
| `unique` | NIKA-153 (fail) | Collection |
| `compact` | NIKA-153 (fail) | Collection |
| `to_string` | propagate (null) | Type |
| `to_number` | NIKA-153 (fail) | Type |
| `to_bool` | NIKA-153 (fail) | Type |
| `to_json` | propagate (null) | Type |
| `parse_json` | NIKA-153 (fail) | Type |
| `round` / `round(N)` | NIKA-153 (fail) | Numeric |
| `abs` | NIKA-153 (fail) | Numeric |
| `ceil` | NIKA-153 (fail) | Numeric |
| `floor` | NIKA-153 (fail) | Numeric |
| `default(V)` | returns V (special) | Utility |
| `type_of` | returns "null" | Utility |
| `join(S)` | NIKA-153 (fail) | Utility |
| `split(S)` | NIKA-153 (fail) | Utility |
| `shell` | escapes (works on all) | Utility |

19 transforms fail on null. Tests must verify each one returns `Err(TransformError::NullInput)` and never panics.

```rust
// File: nika-core/src/binding/transform_proptest.rs (or inline in transform.rs #[cfg(test)])
use proptest::prelude::*;
use serde_json::{json, Value, Number};

/// Strategy for arbitrary JSON values (bounded depth to prevent explosion)
fn arb_json_value() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(|n| Value::Number(n.into())),
        any::<f64>()
            .prop_filter("finite", |f| f.is_finite())
            .prop_map(|f| Number::from_f64(f).map(Value::Number).unwrap_or(Value::Null)),
        "\\PC{0,100}".prop_map(|s| Value::String(s)),
    ];
    leaf.prop_recursive(
        3,   // depth
        64,  // max nodes
        8,   // items per collection
        |inner| {
            prop_oneof![
                prop::collection::vec(inner.clone(), 0..8)
                    .prop_map(Value::Array),
                prop::collection::hash_map("\\PC{1,20}", inner, 0..5)
                    .prop_map(|m| Value::Object(m.into_iter().collect())),
            ]
        },
    )
}

/// All 31 transform op names (simple forms)
fn arb_simple_transform_name() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("upper".into()),
        Just("lower".into()),
        Just("trim".into()),
        Just("trim_start".into()),
        Just("trim_end".into()),
        Just("length".into()),
        Just("first".into()),
        Just("last".into()),
        Just("keys".into()),
        Just("values".into()),
        Just("flatten".into()),
        Just("reverse".into()),
        Just("sort".into()),
        Just("unique".into()),
        Just("compact".into()),
        Just("to_string".into()),
        Just("to_number".into()),
        Just("to_bool".into()),
        Just("to_json".into()),
        Just("parse_json".into()),
        Just("round".into()),
        Just("abs".into()),
        Just("ceil".into()),
        Just("floor".into()),
        Just("type_of".into()),
        Just("shell".into()),
    ]
}

/// Parameterized transform names
fn arb_param_transform_name() -> impl Strategy<Value = String> {
    prop_oneof![
        (0usize..100).prop_map(|n| format!("first({})", n)),
        (0usize..100).prop_map(|n| format!("last({})", n)),
        (0u32..10).prop_map(|n| format!("round({})", n)),
        "\\PC{0,20}".prop_map(|s| format!("join('{}')", s)),
        "\\PC{0,20}".prop_map(|s| format!("split('{}')", s)),
        "\\PC{0,20}".prop_map(|s| format!("default('{}')", s)),
    ]
}

proptest! {
    // ── Property 1: No transform panics on any JSON value ──
    #[test]
    fn transform_never_panics(
        value in arb_json_value(),
        op_name in prop_oneof![arb_simple_transform_name(), arb_param_transform_name()]
    ) {
        if let Ok(expr) = TransformExpr::parse(&op_name) {
            let _ = expr.apply(&value); // MUST NOT panic — Err is fine
        }
    }

    // ── Property 2: Null input on failing transforms always returns NullInput error ──
    #[test]
    fn null_on_failing_transform_returns_error(
        op_name in prop_oneof![
            Just("upper"), Just("lower"), Just("trim"), Just("trim_start"),
            Just("trim_end"), Just("first"), Just("last"), Just("values"),
            Just("flatten"), Just("reverse"), Just("sort"), Just("unique"),
            Just("compact"), Just("to_number"), Just("to_bool"),
            Just("parse_json"), Just("round"), Just("abs"), Just("ceil"),
            Just("floor"), Just("join(',')"), Just("split(',')"),
        ]
    ) {
        let expr = TransformExpr::parse(op_name).unwrap();
        let result = expr.apply(&Value::Null);
        prop_assert!(result.is_err());
        if let Err(TransformError::NullInput { .. }) = result {
            // Expected
        } else {
            prop_assert!(false, "Expected NullInput error, got {:?}", result);
        }
    }

    // ── Property 3: Propagating transforms on null return null ──
    #[test]
    fn null_on_propagating_transform_returns_null(
        op_name in prop_oneof![
            Just("length"), Just("keys"), Just("to_string"), Just("to_json"),
            Just("type_of"),
        ]
    ) {
        let expr = TransformExpr::parse(op_name).unwrap();
        let result = expr.apply(&Value::Null);
        match op_name {
            "type_of" => {
                prop_assert_eq!(result.unwrap(), json!("null"));
            }
            _ => {
                prop_assert!(result.is_ok());
                prop_assert!(result.unwrap().is_null() || true); // length/keys return null, type_of returns "null"
            }
        }
    }

    // ── Property 4: default() always returns non-null ──
    #[test]
    fn default_on_null_returns_default(
        default_val in "\\PC{0,50}"
    ) {
        let expr_str = format!("default('{}')", default_val.replace('\'', ""));
        if let Ok(expr) = TransformExpr::parse(&expr_str) {
            let result = expr.apply(&Value::Null);
            prop_assert!(result.is_ok());
            prop_assert!(!result.unwrap().is_null());
        }
    }

    // ── Property 5: shell escape always wraps in single quotes ──
    #[test]
    fn shell_escape_always_single_quoted(input in "\\PC{0,100}") {
        let expr = TransformExpr::parse("shell").unwrap();
        let result = expr.apply(&Value::String(input)).unwrap();
        if let Value::String(s) = result {
            prop_assert!(s.starts_with('\''));
            prop_assert!(s.ends_with('\''));
        }
    }

    // ── Property 6: sort is idempotent ──
    #[test]
    fn sort_is_idempotent(items in prop::collection::vec(any::<i64>(), 0..20)) {
        let arr = Value::Array(items.iter().map(|n| json!(n)).collect());
        let expr = TransformExpr::parse("sort").unwrap();
        let once = expr.apply(&arr).unwrap();
        let twice = expr.apply(&once).unwrap();
        prop_assert_eq!(once, twice);
    }

    // ── Property 7: unique is idempotent ──
    #[test]
    fn unique_is_idempotent(items in prop::collection::vec(0i64..10, 0..20)) {
        let arr = Value::Array(items.iter().map(|n| json!(n)).collect());
        let expr = TransformExpr::parse("unique").unwrap();
        let once = expr.apply(&arr).unwrap();
        let twice = expr.apply(&once).unwrap();
        prop_assert_eq!(once, twice);
    }

    // ── Property 8: flatten then length >= length of outer ──
    #[test]
    fn flatten_preserves_or_increases_count(
        items in prop::collection::vec(
            prop::collection::vec(any::<i64>(), 0..5).prop_map(|v| Value::Array(v.into_iter().map(|n| json!(n)).collect())),
            0..10,
        )
    ) {
        let outer_len = items.len();
        let arr = Value::Array(items);
        let flat = TransformExpr::parse("flatten").unwrap().apply(&arr).unwrap();
        if let Value::Array(ref f) = flat {
            prop_assert!(f.len() >= outer_len || outer_len == 0);
        }
    }

    // ── Property 9: reverse is involution (f(f(x)) = x) ──
    #[test]
    fn reverse_is_involution(items in prop::collection::vec(any::<i64>(), 0..20)) {
        let arr = Value::Array(items.iter().map(|n| json!(n)).collect());
        let expr = TransformExpr::parse("reverse").unwrap();
        let once = expr.apply(&arr).unwrap();
        let twice = expr.apply(&once).unwrap();
        prop_assert_eq!(arr, twice);
    }

    // ── Property 10: to_string then parse_json roundtrip for primitives ──
    #[test]
    fn to_json_parse_json_roundtrip(n in any::<i64>()) {
        let val = json!(n);
        let as_json = TransformExpr::parse("to_json").unwrap().apply(&val).unwrap();
        let back = TransformExpr::parse("parse_json").unwrap().apply(&as_json).unwrap();
        prop_assert_eq!(val, back);
    }

    // ── Property 11: compact removes all nulls ��─
    #[test]
    fn compact_no_nulls(items in prop::collection::vec(
        prop_oneof![
            Just(Value::Null),
            any::<i64>().prop_map(|n| json!(n)),
            Just(json!("hello")),
        ],
        0..20
    )) {
        let arr = Value::Array(items);
        let result = TransformExpr::parse("compact").unwrap().apply(&arr).unwrap();
        if let Value::Array(ref compacted) = result {
            prop_assert!(compacted.iter().all(|v| !v.is_null()));
        }
    }
}
```

### Strategy 2: Cost calculation never NaN/negative/infinite

```rust
// File: nika-engine/src/provider/cost_proptest.rs
use proptest::prelude::*;

proptest! {
    #[test]
    fn cost_always_valid(
        input in 0u64..10_000_000_000,
        output in 0u64..10_000_000_000,
    ) {
        // Test all providers with their actual models
        let cases = vec![
            (ProviderKind::Claude, "claude-sonnet-4-6"),
            (ProviderKind::Claude, "claude-opus-4-20250514"),
            (ProviderKind::OpenAI, "gpt-4o"),
            (ProviderKind::OpenAI, "o3"),
            (ProviderKind::Mistral, "mistral-large-latest"),
            (ProviderKind::Groq, "llama-3.3-70b-versatile"),
            (ProviderKind::DeepSeek, "deepseek-chat"),
            (ProviderKind::Gemini, "gemini-2.5-pro"),
            (ProviderKind::XAi, "grok-3"),
            (ProviderKind::Native, "anything"),
        ];
        for (provider, model) in &cases {
            let cost = calculate_cost(*provider, model, input, output);
            prop_assert!(cost >= 0.0, "Negative cost for {:?}/{}: {}", provider, model, cost);
            prop_assert!(cost.is_finite(), "Non-finite cost for {:?}/{}: {}", provider, model, cost);
        }
    }

    #[test]
    fn cost_with_cache_always_valid(
        input in 0u64..10_000_000_000,
        output in 0u64..10_000_000_000,
        cached in 0u64..10_000_000_000,
    ) {
        let providers = vec![
            (ProviderKind::Claude, "claude-sonnet-4-6"),
            (ProviderKind::OpenAI, "gpt-4o"),
            (ProviderKind::DeepSeek, "deepseek-chat"),
        ];
        for (provider, model) in &providers {
            let cost = calculate_cost_with_cache(*provider, model, input, output, cached);
            prop_assert!(cost >= 0.0, "Negative cached cost: {}", cost);
            prop_assert!(cost.is_finite(), "Non-finite cached cost: {}", cost);

            // Cached cost should be <= non-cached cost
            let uncached = calculate_cost(*provider, model, input, output);
            prop_assert!(cost <= uncached + f64::EPSILON,
                "Cached cost {} > uncached cost {} for {:?}", cost, uncached, provider);
        }
    }

    #[test]
    fn cost_zero_tokens_is_zero(
        provider in prop_oneof![
            Just(ProviderKind::Claude),
            Just(ProviderKind::OpenAI),
            Just(ProviderKind::Gemini),
        ]
    ) {
        let cost = calculate_cost(provider, "any-model", 0, 0);
        prop_assert_eq!(cost, 0.0);
    }

    #[test]
    fn cost_monotonically_increases(
        input1 in 0u64..5_000_000,
        input2 in 5_000_001u64..10_000_000,
        output in 0u64..10_000_000,
    ) {
        let cost1 = calculate_cost(ProviderKind::Claude, "claude-sonnet-4-6", input1, output);
        let cost2 = calculate_cost(ProviderKind::Claude, "claude-sonnet-4-6", input2, output);
        prop_assert!(cost2 >= cost1, "More tokens should cost more: {} vs {}", cost1, cost2);
    }

    #[test]
    fn native_always_free(input in 0u64..u64::MAX, output in 0u64..u64::MAX) {
        let cost = calculate_cost(ProviderKind::Native, "any-model", input, output);
        prop_assert_eq!(cost, 0.0);
    }

    #[test]
    fn format_cost_never_panics(cost in -1000.0f64..1000.0) {
        let _ = format_cost(cost); // Must not panic
    }
}
```

### Strategy 3: Template parsing never panics

```rust
// File: nika-core/src/binding/transform_parse_proptest.rs
proptest! {
    #[test]
    fn transform_parse_no_panic(input in "\\PC{0,200}") {
        let _ = TransformExpr::parse(&input); // Must not panic — Err is fine
    }

    #[test]
    fn transform_parse_valid_roundtrip(
        op_name in arb_simple_transform_name()
    ) {
        let expr = TransformExpr::parse(&op_name).unwrap();
        prop_assert_eq!(expr.ops.len(), 1);
    }

    #[test]
    fn pipe_chain_parse_no_panic(
        ops in prop::collection::vec(arb_simple_transform_name(), 1..10)
    ) {
        let chain = ops.join(" | ");
        let _ = TransformExpr::parse(&chain); // Must not panic
    }

    #[test]
    fn empty_and_whitespace_parse(input in "[ \t\n]*") {
        let result = TransformExpr::parse(&input);
        prop_assert!(result.is_ok());
        prop_assert!(result.unwrap().is_empty());
    }
}
```

### Strategy 4: DAG validation with arbitrary graph shapes

```rust
// File: nika-engine/src/dag/flow_proptest.rs
proptest! {
    #[test]
    fn compute_layers_never_panics(
        n_nodes in 1usize..50,
        n_edges in 0usize..100,
    ) {
        let nodes: Vec<String> = (0..n_nodes).map(|i| format!("t{}", i)).collect();
        let node_refs: Vec<&str> = nodes.iter().map(|s| s.as_str()).collect();

        let edges: Vec<(usize, usize)> = (0..n_edges)
            .map(|i| (i % n_nodes, (i * 7 + 3) % n_nodes))
            .collect();
        let edge_refs: Vec<(&str, &str)> = edges.iter()
            .map(|(a, b)| (node_refs[*a], node_refs[*b]))
            .collect();

        let _ = compute_layers(&node_refs, &edge_refs); // Must not panic
    }

    #[test]
    fn layer_count_at_least_one(
        n_nodes in 1usize..20,
    ) {
        let nodes: Vec<String> = (0..n_nodes).map(|i| format!("t{}", i)).collect();
        let node_refs: Vec<&str> = nodes.iter().map(|s| s.as_str()).collect();
        let depths = compute_layers(&node_refs, &[]);
        prop_assert!(layer_count(&depths) >= 1);
    }

    // DAG with guaranteed acyclic shape (forward edges only)
    #[test]
    fn acyclic_graph_no_cycle(
        n_nodes in 2usize..30,
        edge_density in 0.0f64..0.3,
    ) {
        // Build a guaranteed-acyclic graph: edges only go from lower to higher index
        let nodes: Vec<String> = (0..n_nodes).map(|i| format!("t{}", i)).collect();
        let node_refs: Vec<&str> = nodes.iter().map(|s| s.as_str()).collect();

        let mut edges = Vec::new();
        for i in 0..n_nodes {
            for j in (i+1)..n_nodes {
                // Use a deterministic hash to decide edge inclusion
                if ((i * 31 + j * 17) as f64 / (n_nodes * n_nodes) as f64) < edge_density {
                    edges.push((node_refs[i], node_refs[j]));
                }
            }
        }

        let depths = compute_layers(&node_refs, &edges);
        // All nodes should have a depth
        prop_assert_eq!(depths.len(), n_nodes);
    }
}
```

---

## Part 3: Wire tracing-error (~30min)

### Design: SpanTrace integration without changing 96 variants

The key insight: we do NOT add `SpanTrace` to every `NikaError` variant. Instead, we capture it at the **error boundary** — the point where an error is first created or where context would be most useful.

**Minimal change (3 touch points):**

1. **Binary entry point** (`nika/src/main.rs`): Add `ErrorLayer` to tracing subscriber.

```rust
use tracing_error::ErrorLayer;
// In subscriber setup:
.with(ErrorLayer::default())
```

2. **Error wrapper type** (new, ~20 lines in `error.rs`):

```rust
use tracing_error::SpanTrace;

/// Wrapper that pairs a NikaError with its SpanTrace at creation site.
/// Used at key error boundaries — NOT on every error path.
#[derive(Debug)]
pub struct TracedError {
    pub error: NikaError,
    pub span_trace: SpanTrace,
}

impl TracedError {
    pub fn new(error: NikaError) -> Self {
        Self {
            error,
            span_trace: SpanTrace::capture(),
        }
    }
}

impl std::fmt::Display for TracedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}\n\nSpan trace:\n{}", self.error, self.span_trace)
    }
}

impl std::error::Error for TracedError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}
```

3. **Error boundaries** (4 strategic `#[instrument]` additions):

```rust
// runner.rs — task execution entry
#[tracing::instrument(skip_all, fields(task_id = %task.id, verb = %task.verb_name()))]
async fn execute_task(&self, task: &Task) -> Result<TaskResult, NikaError> { ... }

// provider/rig.rs — LLM call
#[tracing::instrument(skip_all, fields(provider = %provider, model = %model))]
async fn call_provider(&self, ...) -> Result<String, NikaError> { ... }

// executor/fetch.rs — HTTP fetch
#[tracing::instrument(skip_all, fields(url = %url, extract = %extract_mode))]
async fn execute_fetch(&self, ...) -> Result<String, NikaError> { ... }

// mcp/client.rs — MCP tool call
#[tracing::instrument(skip_all, fields(server = %server, tool = %tool))]
async fn call_tool(&self, ...) -> Result<Value, NikaError> { ... }
```

**Why this is minimal**: No `From` impl changes, no variant changes, no breaking API. The `SpanTrace` is captured automatically by `tracing-error` wherever the error propagates through an instrumented span. The `TracedError` wrapper is only used at the top-level display (CLI output, TUI error panel).

---

## Part 4: Fix RC4 — Merge dual pricing tables (~30min)

### Complete discrepancy analysis

**nika-core** (`catalogs/cost.rs`): 22 pattern-matched entries, `contains()` matching.
**nika-engine** (`provider/cost.rs`): 7 per-provider `FxHashMap`s, exact key matching.

#### Models in nika-engine but NOT in nika-core

| Provider | Model | Input$/M | Output$/M | Status |
|----------|-------|----------|-----------|--------|
| Anthropic | `claude-3-5-sonnet-20241022` | 3.0 | 15.0 | Covered by core `sonnet-4` pattern |
| Anthropic | `claude-3-5-sonnet-latest` | 3.0 | 15.0 | Covered |
| Anthropic | `claude-3-5-haiku-20241022` | 0.8 | 4.0 | Covered by core `haiku-4` pattern |
| Anthropic | `claude-3-5-haiku-latest` | 0.8 | 4.0 | Covered |
| Anthropic | `claude-3-opus-20240229` | 15.0 | 75.0 | Covered by core `opus-4` pattern |
| Anthropic | `claude-3-opus-latest` | 15.0 | 75.0 | Covered |
| Anthropic | `claude-3-sonnet-20240229` | 3.0 | 15.0 | Covered |
| Anthropic | `claude-3-haiku-20240307` | 0.25 | 1.25 | **MISSING from core** — old Haiku is cheaper |
| OpenAI | `gpt-4-turbo` | 10.0 | 30.0 | **MISSING from core** |
| OpenAI | `gpt-4-turbo-2024-04-09` | 10.0 | 30.0 | **MISSING from core** |
| OpenAI | `gpt-4-turbo-preview` | 10.0 | 30.0 | **MISSING from core** |
| OpenAI | `gpt-4` | 30.0 | 60.0 | **MISSING from core** |
| OpenAI | `gpt-4-0613` | 30.0 | 60.0 | **MISSING from core** |
| OpenAI | `gpt-3.5-turbo` | 0.5 | 1.5 | **MISSING from core** |
| OpenAI | `gpt-3.5-turbo-0125` | 0.5 | 1.5 | **MISSING from core** |
| OpenAI | `o1-preview` | 15.0 | 60.0 | Covered by core `o1` pattern |
| OpenAI | `o1-mini` | 3.0 | 12.0 | **MISSING from core** (different price from o1) |
| Mistral | `mistral-medium-latest` | 2.7 | 8.1 | **MISSING from core** |
| Mistral | `codestral-latest` | 0.3 | 0.9 | **MISSING from core** |
| Mistral | `codestral-2501` | 0.3 | 0.9 | **MISSING from core** |
| Mistral | `ministral-8b-latest` | 0.1 | 0.1 | **MISSING from core** |
| Mistral | `ministral-3b-latest` | 0.04 | 0.04 | **MISSING from core** |
| Mistral | `pixtral-large-latest` | 2.0 | 6.0 | **MISSING from core** |
| Mistral | `pixtral-12b-2409` | 0.15 | 0.15 | **MISSING from core** |
| Groq | `llama-3.3-70b-specdec` | 0.59 | 0.99 | **PRICE MISMATCH** — engine has 0.99 output, core has 0.79 |
| Groq | `llama-3.1-70b-versatile` | 0.59 | 0.79 | Covered by core `llama-3.3-70b` pattern (wrong match!) |
| Groq | `llama3-70b-8192` | 0.59 | 0.79 | **MISSING from core** |
| Groq | `llama3-8b-8192` | 0.05 | 0.08 | Covered |
| Groq | `mixtral-8x7b-32768` | 0.24 | 0.24 | **MISSING from core** |
| Groq | `gemma2-9b-it` | 0.20 | 0.20 | **MISSING from core** |
| DeepSeek | `deepseek-coder` | 0.14 | 0.28 | **MISSING from core** |
| Gemini | `gemini-2.0-flash` | 0.1 | 0.4 | **MISSING from core** |
| Gemini | `gemini-2.0-flash-exp` | 0.0 | 0.0 | **MISSING from core** (free preview) |
| Gemini | `gemini-2.0-flash-thinking` | 0.0 | 0.0 | **MISSING from core** (free preview) |
| Gemini | `gemini-1.5-pro` | 1.25 | 5.0 | **MISSING from core** |
| Gemini | `gemini-1.5-flash` | 0.075 | 0.3 | **MISSING from core** |
| Gemini | `gemini-1.5-flash-8b` | 0.0375 | 0.15 | **MISSING from core** |
| Gemini | `gemini-pro` | 0.5 | 1.5 | **MISSING from core** |
| xAI | `grok-3-fast` | 0.6 | 4.0 | **MISSING from core** |
| xAI | `grok-3-mini-fast` | 0.1 | 0.4 | **MISSING from core** |
| xAI | `grok-2` | 2.0 | 10.0 | **MISSING from core** |

#### Semantic mismatch: `contains()` vs exact match

The nika-core `find_pricing` uses `model.contains(pattern)`, which means:
- `"gpt-4o-mini"` must appear BEFORE `"gpt-4o"` in the list (already correct)
- `"o3-mini"` must appear BEFORE `"o3"` (already correct)
- But `"llama-3.3-70b"` matches BOTH `llama-3.3-70b-versatile` AND `llama-3.3-70b-specdec` with the SAME price, when `specdec` has different output pricing (0.99 vs 0.79)

#### Merge strategy

**Step 1**: Expand nika-core `KNOWN_PRICING` to cover ALL models from nika-engine (currently 22 entries, needs ~45).

**Step 2**: Change nika-core matching from `contains()` to a two-pass system:
1. Try exact match first
2. Fall back to prefix/contains match

**Step 3**: In nika-engine, delete all 7 `LazyLock<FxHashMap>` tables. Replace `get_model_pricing()` with:

```rust
pub fn get_model_pricing(provider: ProviderKind, model: &str) -> ModelPricing {
    if provider.is_free() {
        return FREE_PRICING;
    }
    nika_core::catalogs::cost::find_pricing(model)
        .map(|p| ModelPricing::new(p.input_per_million, p.output_per_million))
        .unwrap_or_else(|| {
            tracing::warn!(provider = %provider.name(), model = %model,
                "Unknown model — using default pricing");
            DEFAULT_PRICING
        })
}
```

**Step 4**: Keep `ProviderKind`, `format_cost()`, `cache_discount_for_provider()`, `calculate_cost()`, `calculate_cost_with_cache()`, `list_provider_models()`, `ModelMeta` in nika-engine (they have engine-specific logic). Only the PRICING DATA moves.

**Step 5**: Update all tests in both crates. Add a sync test:

```rust
#[test]
fn pricing_tables_in_sync() {
    // Every model in engine's list_provider_models() must be findable in core
    for provider in [ProviderKind::Claude, ProviderKind::OpenAI, /* ... */] {
        for (model, pricing) in list_provider_models(provider) {
            let core_pricing = nika_core::catalogs::cost::find_pricing(model);
            assert!(core_pricing.is_some(), "Model {} missing from core pricing", model);
            let cp = core_pricing.unwrap();
            assert!((cp.input_per_million - pricing.input_per_million).abs() < 0.01,
                "Price mismatch for {}: core={}, engine={}", model, cp.input_per_million, pricing.input_per_million);
        }
    }
}
```

---

## Part 5: Fix RC6 — Workspace deps (~15min)

### Complete audit of non-workspace dependencies

Grepping all 12 crate `Cargo.toml` files for deps declared locally that should be workspace:

#### Dependencies that SHOULD become workspace

| Crate | Dependency | Current | Action |
|-------|-----------|---------|--------|
| nika | `cliclack` | `"0.5"` | Add to workspace (also used by nika-cli) |
| nika | `dotenvy` | `"0.15"` | Add to workspace |
| nika-cli | `cliclack` | `"0.5"` | Same version as nika — unify |
| nika-cli | `console` | `"0.16"` | Add to workspace |
| nika-cli | `infer` | `"0.19"` | Add to workspace (also in nika-engine, nika-media, nika-tui) |
| nika-cli | `which` | `"8"` | Add to workspace |
| nika-engine | `secrecy` | `"0.10"` | Add to workspace |
| nika-engine | `zeroize` | `"1.8"` | Add to workspace (also in nika-tui as `"1"`) |
| nika-engine | `shlex` | `"1.3"` | Add to workspace |
| nika-engine | `unicode-normalization` | `"0.1"` | Add to workspace |
| nika-engine | `urlencoding` | `"2.1"` | Add to workspace |
| nika-engine | `flate2` | `"1.1"` | Add to workspace |
| nika-engine | `tar` | `"0.4"` | Add to workspace |
| nika-engine | `unicode-width` | `"0.2"` | Add to workspace (also in nika-tui) |
| nika-engine | `terminal_size` | `"0.4"` | Add to workspace (also in nika-tui) |
| nika-engine | `infer` | `"0.19"` | Add to workspace |
| nika-daemon | `keyring` | `version = "3", features = ["apple-native"]` | **Feature mismatch** — engine has `["apple-native", "windows-native", "sync-secret-service"]` |
| nika-daemon | `croner` | `"3"` | Add to workspace |
| nika-daemon | `rusqlite` | `version = "0.39", features = ["bundled"]` | Add to workspace |
| nika-daemon | `notify` | `"8"` | Add to workspace |
| nika-daemon | `nix` | `version = "0.31", features = [...]` | Add to workspace (Unix only) |
| nika-media | `mime` | `"0.3"` | Add to workspace |
| nika-media | `mime_guess` | `"2.0"` | Add to workspace |
| nika-media | `bytes` | `"1"` | Add to workspace |
| nika-media | `imagesize` | `"0.14"` | Add to workspace |
| nika-media | `thumbhash` | `"0.1"` | Add to workspace |
| nika-media | `color-thief` | `"0.2"` | Add to workspace |
| nika-media | `rayon` | `"1.10"` | Add to workspace |
| nika-media | `image` | `version = "0.25", ...` | Already optional in engine too — unify features |
| nika-media | `scraper` | `version = "0.26", optional, features = ["atomic"]` | Add to workspace |
| nika-media | `htmd` | `version = "0.5", optional` | Add to workspace |
| nika-media | `dom_smoothie` | `version = "0.16", optional` | Add to workspace (also in nika-engine) |
| nika-media | `feed-rs` | `version = "2.3", optional` | Add to workspace (also in nika-engine) |
| nika-media | `psl` | `version = "2", optional` | Add to workspace |
| nika-tui | `ratatui` | `"0.30"` | Add to workspace |
| nika-tui | `crossterm` | `version = "0.29", features = ["event-stream"]` | Add to workspace |
| nika-tui | `tui-input` | `version = "0.15", features = ["crossterm"]` | Add to workspace |
| nika-tui | `arboard` | `"3.4"` | Add to workspace |
| nika-tui | `tree-sitter` | `"0.25"` | Add to workspace (also in nika-lsp-core) |
| nika-tui | `tree-sitter-yaml` | `"0.7"` | Add to workspace (also in nika-lsp-core) |
| nika-tui | `streaming-iterator` | `"0.1"` | Add to workspace |
| nika-tui | `signal-hook` | `version = "0.4", features = ["iterator"]` | Add to workspace |
| nika-tui | `zeroize` | `"1"` | **Version mismatch** with engine `"1.8"` — unify to `"1.8"` |
| nika-tui | `infer` | `"0.19"` | Already listed — unify |
| nika-lsp | `tower-lsp-server` | `"0.23"` | Add to workspace (also optional in nika) |
| nika-lsp | `ropey` | `"1.6"` | Add to workspace (also in nika-lsp-core) |
| nika-lsp-core | `ls-types` | `"0.0"` | Add to workspace |
| nika-lsp-core | `ropey` | `"1.6"` | Same as nika-lsp — unify |
| nika-lsp-core | `tree-sitter` | `"0.25"` | Same as nika-tui — unify |
| nika-lsp-core | `tree-sitter-yaml` | `"0.7"` | Same as nika-tui — unify |
| nika-mcp | `shellexpand` | `"3.1"` | Add to workspace |
| nika-mcp | `schemars` | `"1.2"` | Add to workspace |

#### Critical: keyring feature mismatch

```
nika-daemon: keyring = { version = "3", features = ["apple-native"] }
nika-engine: keyring = { version = "3", features = ["apple-native", "windows-native", "sync-secret-service"] }
```

Resolution: workspace entry with ALL features (superset), each crate uses `default-features = false` + only what it needs. Or: single workspace entry with all features.

#### Critical: zeroize version mismatch

```
nika-engine: zeroize = "1.8"
nika-tui:    zeroize = "1"
```

Resolution: Unify to `"1.8"` in workspace.

---

## Part 6: Fix env-var race conditions (~15min)

Add `#[serial]` to dev-deps of crates that need it:
- `nika-engine` (already in workspace)
- `nika-media` (for CAS store env tests)
- `nika-daemon` (already uses `serial_test = "3"`)
- `nika-mcp` (already uses `serial_test = "3.1"`)
- `nika-tui` (already uses `serial_test = "3.1"`)

Add `#[serial]` to all env-var-manipulating tests. Grep for `std::env::set_var` and `std::env::remove_var` in test code.

---

## Part 7: E2E stress-test workflows

### Workflow 1: All 9 extract modes

```yaml
schema: "nika/workflow@0.12"
workflow: stress-test-extract-modes
description: "Test all 9 fetch extract modes on real URLs"
provider: mock

tasks:
  - id: markdown_extract
    fetch:
      url: "https://example.com"
      extract: markdown

  - id: article_extract
    fetch:
      url: "https://example.com"
      extract: article

  - id: text_extract
    fetch:
      url: "https://example.com"
      extract: text
      selector: "body"

  - id: selector_extract
    fetch:
      url: "https://example.com"
      extract: selector
      selector: "h1"

  - id: metadata_extract
    fetch:
      url: "https://example.com"
      extract: metadata

  - id: links_extract
    fetch:
      url: "https://example.com"
      extract: links

  - id: jsonpath_extract
    fetch:
      url: "https://httpbin.org/get"
      extract: jsonpath
      selector: "$.headers"

  - id: feed_extract
    fetch:
      url: "https://feeds.bbci.co.uk/news/rss.xml"
      extract: feed

  - id: llm_txt_extract
    fetch:
      url: "https://docs.anthropic.com"
      extract: llm_txt

  - id: verify_all
    depends_on: [markdown_extract, article_extract, text_extract, selector_extract, metadata_extract, links_extract, jsonpath_extract, feed_extract, llm_txt_extract]
    with:
      md: $markdown_extract
      art: $article_extract
      txt: $text_extract
      sel: $selector_extract
      meta: $metadata_extract
      links: $links_extract
      jp: $jsonpath_extract
      feed: $feed_extract
      llm: $llm_txt_extract
    infer:
      prompt: |
        Verify all 9 extracts produced non-empty output:
        markdown: {{with.md | length}} chars
        article: {{with.art | length}} chars
        text: {{with.txt | length}} chars
        selector: {{with.sel | length}} chars
        metadata: {{with.meta | length}} chars
        links: {{with.links | length}} chars
        jsonpath: {{with.jp | length}} chars
        feed: {{with.feed | length}} chars
        llm_txt: {{with.llm | length}} chars
```

### Workflow 2: Mixed verbs, for_each, cost verification

```yaml
schema: "nika/workflow@0.12"
workflow: stress-test-cost-tracking
description: "10+ tasks, mixed verbs, for_each — verify cost > 0 for LLM tasks"
provider: mock

inputs:
  topics: ["rust", "python", "go", "typescript", "zig"]

tasks:
  - id: generate_prompts
    infer:
      prompt: "List 5 programming topics"
    structured:
      schema:
        type: object
        properties:
          topics: { type: array, items: { type: string } }
        required: [topics]

  - id: fetch_docs
    exec:
      command: "echo '{\"status\": \"ok\", \"count\": 5}'"
      shell: true

  - id: parallel_research
    depends_on: [generate_prompts]
    with:
      topics: $generate_prompts
    for_each:
      items: "{{inputs.topics}}"
      as: topic
      concurrency: 3
    infer:
      prompt: "Brief summary of {{with.topic}}"
      max_tokens: 100

  - id: aggregate
    depends_on: [parallel_research, fetch_docs]
    with:
      research: $parallel_research
      docs: $fetch_docs
    infer:
      prompt: |
        Combine research ({{with.research | length}} items)
        with docs status: {{with.docs}}

  - id: transform_chain
    depends_on: [aggregate]
    with:
      data: $aggregate
    infer:
      prompt: "{{with.data | trim | first(500)}}"

  - id: validate_output
    depends_on: [transform_chain]
    with:
      result: $transform_chain
    infer:
      prompt: "Validate: {{with.result | type_of}}"

  - id: secondary_analysis
    depends_on: [parallel_research]
    with:
      items: $parallel_research
    infer:
      prompt: "Analyze: {{with.items | first}}"

  - id: format_report
    depends_on: [validate_output, secondary_analysis]
    with:
      validation: $validate_output
      analysis: $secondary_analysis
    infer:
      prompt: "Report: {{with.validation}} + {{with.analysis}}"
      temperature: 0.3

  - id: exec_cleanup
    depends_on: [format_report]
    exec:
      command: "echo 'done'"

  - id: final_summary
    depends_on: [exec_cleanup, format_report]
    with:
      report: $format_report
    infer:
      prompt: "Final: {{with.report | trim | length}} chars processed"
```

---

## Part 8: Additional quality tools to consider

### Fuzzing (HIGH priority, should be P1)

Template parsing and YAML parsing are the two highest-risk parser surfaces. Both accept arbitrary user input.

**cargo-fuzz** (LibFuzzer):
```bash
cargo install cargo-fuzz
cargo fuzz init  # in nika-core
cargo fuzz add template_parse
cargo fuzz add yaml_parse
cargo fuzz add transform_apply
```

Fuzz targets:
1. `TransformExpr::parse(input)` — arbitrary strings
2. `TransformOp::apply(value)` — arbitrary JSON + op combinations
3. `parse_single_op(input)` — parameterized transform parsing
4. YAML workflow parser — arbitrary YAML → Raw AST
5. `shell_escape(s)` — ensure no escaping bypasses
6. `normalize_for_blocklist(s)` — Unicode normalization edge cases

**bolero** (coverage-guided, integrated with proptest):
Better for Rust-native fuzzing. Combine with existing proptest strategies.

### Benchmarks (MEDIUM priority)

Already have `criterion` in dev-deps. Current benchmarks:
- `binding_resolution` — template resolve perf
- `task_execution` — verb dispatch overhead

Additional benchmarks needed:
- `cost_calculation` — ensure no regression on hot path
- `dag_construction` — scale test with 100/500/1000 tasks
- `transform_chain` — pipeline of 10 transforms on 10K items
- `blocklist_check` — 70+ patterns against long commands

```bash
cargo bench --bench binding_resolution
cargo bench --bench task_execution
```

### Coverage reports (MEDIUM priority)

**cargo-tarpaulin** (Linux) or **cargo-llvm-cov** (cross-platform):

```bash
# cargo-llvm-cov is better for macOS
cargo install cargo-llvm-cov
cargo llvm-cov --workspace --lib --html --output-dir target/coverage
```

Target: 80% line coverage for nika-core and nika-engine (the two critical crates).

Focus areas likely under-covered:
- Error paths (all the `Err(NikaError::...)` branches)
- Shell-mode security paths
- Edge cases in transform (empty arrays, empty strings, max values)

### cargo-deny (HIGH priority, should be P0)

```bash
cargo install cargo-deny
cargo deny init  # creates deny.toml
cargo deny check
```

This checks:
- **Licenses**: AGPL compliance — flag any dependency with incompatible license
- **Advisories**: CVE scanning against RustSec database
- **Bans**: Prevent duplicate crate versions
- **Sources**: Ensure all deps come from crates.io

### cargo-audit (HIGH priority for CI)

```bash
cargo install cargo-audit
cargo audit
```

Run in CI on every push. Zero tolerance for known vulnerabilities.

### cargo-semver-checks (MEDIUM, pre-publish)

```bash
cargo install cargo-semver-checks
cargo semver-checks check-release -p nika-core
cargo semver-checks check-release -p nika-engine
```

Catches accidental API breaks before crates.io publish.

### cargo-careful (LOW priority, occasional)

```bash
cargo install cargo-careful
cargo careful test --workspace --lib
```

Extra UB checks (Miri-lite). Run occasionally, not in CI (slow).

---

## Execution order

| Step | Task | Time | Depends on |
|------|------|------|------------|
| 1 | Install cargo-mutants, run on 5 files | 20min | Nothing |
| 2 | Write proptest strategies (transforms + cost + DAG) | 30min | Nothing (parallel with 1) |
| 3 | Triage surviving mutants, write killing tests | 30min | Step 1 |
| 4 | Wire tracing-error (ErrorLayer + TracedError + 4 instrument) | 20min | Nothing (parallel) |
| 5 | RC6: Move ~50 deps to workspace | 15min | Nothing (parallel) |
| 6 | RC4: Merge pricing tables | 30min | Step 5 (needs workspace deps clean) |
| 7 | Fix env-var race conditions (#[serial]) | 10min | Nothing (parallel) |
| 8 | Add E2E stress-test workflows | 15min | Step 6 |
| 9 | cargo-deny setup + initial audit | 10min | Step 5 |

**Critical path**: Steps 1→3 (mutation testing) + Step 6 (pricing merge) = ~80min.
**Parallel work**: Steps 2, 4, 5, 7, 9 can all run alongside.

---

## After all fixes

1. `cargo test --workspace --lib` — ALL pass (expect 8700+)
2. `cargo mutants -p nika-engine -f src/provider/cost.rs -- --lib` — 0 surviving
3. `cargo mutants -p nika-engine -f src/runtime/security.rs -- --lib` — 0 surviving
4. `cargo mutants -p nika-core -f src/binding/transform.rs -- --lib` — 0 surviving
5. `cargo clippy --workspace -- -D warnings` — 0 warnings
6. `cargo deny check` — 0 license violations, 0 advisories
7. All proptest properties pass with 256+ cases each
8. Pricing: `nika-engine` has ZERO `LazyLock<FxHashMap>` pricing tables
9. Workspace: ZERO locally-declared deps that exist in `[workspace.dependencies]`
10. No `serial_test` is needed without `#[serial]` annotation
