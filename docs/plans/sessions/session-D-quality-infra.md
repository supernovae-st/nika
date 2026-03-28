# Session D: Quality Infrastructure (~2-3h)

## Context
Nika workflow engine. Workspace: `tools/` (12 Rust crates).
P0 crates already added to workspace Cargo.toml: tracing-error, nutype, proptest, serial_test, rstest.

## Mission: Wire up quality tools + find weak tests with mutation testing

---

### Part 1: cargo-mutants on critical files (~1h)

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

For EVERY surviving mutant: write a test that kills it.

### Part 2: proptest strategies (~45min)

Add proptest to `nika-core` and `nika-engine` dev-dependencies (already in workspace).

Write property tests for:

**1. Cost never negative/NaN/infinite:**
```rust
proptest! {
    #[test]
    fn cost_always_valid(input in 0u64..10_000_000, output in 0u64..10_000_000, cached in 0u64..10_000_000) {
        let cost = calculate_cost_with_cache(ProviderKind::Claude, "claude-sonnet-4-6", input, output, cached);
        prop_assert!(cost >= 0.0);
        prop_assert!(cost.is_finite());
    }
}
```

**2. Pipe transforms never panic on any Value:**
```rust
proptest! {
    #[test]
    fn transform_never_panics(value in arb_json_value(), op_name in arb_transform_name()) {
        let op = TransformOp::parse(&op_name);
        if let Ok(op) = op {
            let _ = op.apply(value); // Must not panic
        }
    }
}
```

**3. Template parsing roundtrip:**
```rust
proptest! {
    #[test]
    fn template_parse_no_panic(input in "\\PC{0,200}") {
        let _ = parse_template_expr(&input); // Must not panic
    }
}
```

### Part 3: Wire tracing-error (~30min)

Add `tracing-error` to `nika-engine` Cargo.toml:
```toml
tracing-error = { workspace = true }
```

In the main binary (`nika/src/main.rs`), add ErrorLayer to subscriber:
```rust
use tracing_error::ErrorLayer;
// In subscriber setup:
.with(ErrorLayer::default())
```

Add `SpanTrace` capture at key error boundaries (start with provider layer).

### Part 4: Fix env-var race conditions (~15min)

Add `serial_test` to dev-deps of crates that need it:
- `nika-engine` (for `wave2_chat_continue_missing_gemini_dispatch`)
- `nika-media` (for CAS store env tests)
- `nika-engine` binding resolve tests

Add `#[serial]` to all env-var-manipulating tests.

### Part 5: Fix RC6 — workspace deps (~15min)

Move locally-declared deps to `[workspace.dependencies]`:
- `image`, `scraper`, `htmd`, `dom_smoothie`, `keyring`, `crossterm`

### Part 6: Fix RC4 — merge pricing tables (~30min)

Delete per-provider HashMaps in `nika-engine/src/provider/cost.rs`.
Make `calculate_cost()` delegate to `nika-core::catalogs::cost::find_pricing()`.
One table, one truth.

---

## After All Fixes
1. `cargo test --workspace --lib` — ALL pass
2. `cargo mutants -p nika-engine -f src/provider/cost.rs -- --lib` — 0 surviving
3. `cargo clippy --workspace -- -D warnings` — 0 warnings
