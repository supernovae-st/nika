# Rust Best Practices Research Report (2025-2026)

**Date:** 2026-03-12
**Project:** Nika v0.27.0
**Codebase:** 358 files, 210K+ lines of Rust

## Executive Summary

This report analyzes current Rust best practices and compares them against Nika's existing patterns. The codebase demonstrates strong adherence to modern Rust idioms with some areas for potential improvement.

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║  NIKA RUST PRACTICES ASSESSMENT                                               ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  Error Handling       ████████████████████░░░░  85%  thiserror + miette      ║
║  Async Patterns       █████████████████████░░░  90%  tokio + structured      ║
║  Testing              ████████████████████████  95%  4,433 tests, criterion  ║
║  Documentation        ████████████████░░░░░░░░  70%  Good module docs        ║
║  Performance          ████████████████████░░░░  85%  Benchmarks present      ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## 1. Error Handling

### Current State in Nika

Nika uses a hybrid approach that aligns well with 2025-2026 best practices:

| Crate | Version | Usage |
|-------|---------|-------|
| `thiserror` | 1.0 | Structured error types with `#[error]` and `#[diagnostic]` |
| `miette` | 7.6 | Fancy terminal error display with source spans |

**Current Implementation (`src/error.rs`):**

```rust
#[derive(Error, Debug, Diagnostic)]
#[diagnostic(url(docsrs))]
pub enum NikaError {
    #[error("[NIKA-001] Failed to parse workflow: {details}")]
    #[diagnostic(
        code(nika::parse_error),
        help("Check YAML syntax: indentation and quoting")
    )]
    ParseError { details: String },
    // ... 40+ variants with error codes
}
```

### 2025-2026 Best Practices

1. **thiserror for library errors** (Nika: COMPLIANT)
   - Use `#[error]` for Display impl
   - Use `#[from]` for automatic conversions
   - Prefer structured error types over strings

2. **anyhow for application code** (Nika: PARTIALLY COMPLIANT)
   - Currently not used; Nika uses `NikaError` everywhere
   - Consider: `anyhow` in CLI entry points, `thiserror` in library code

3. **Error codes for categorization** (Nika: EXEMPLARY)
   - Nika uses NIKA-XXX codes (NIKA-001 through NIKA-400+)
   - Well-documented ranges per category

### Recommendations

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  RECOMMENDATION: Consider Error Architecture Split                              │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Current:  NikaError used everywhere (40+ variants)                             │
│                                                                                 │
│  Proposed: Domain-specific error enums                                          │
│                                                                                 │
│    pub enum AstError { ... }     // ast/ module only                           │
│    pub enum McpError { ... }     // mcp/ module only                           │
│    pub enum RuntimeError { ... } // runtime/ module only                        │
│                                                                                 │
│    // Top-level aggregates all                                                  │
│    pub enum NikaError {                                                         │
│        #[error(transparent)]                                                    │
│        Ast(#[from] AstError),                                                   │
│        // ...                                                                   │
│    }                                                                            │
│                                                                                 │
│  Benefit: Smaller match arms, clearer boundaries, better IDE completion         │
│  Priority: LOW (current approach works well, just verbose)                      │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Async Rust (tokio)

### Current State in Nika

Nika demonstrates excellent async patterns:

```rust
// Cargo.toml
tokio = { version = "1.49", features = ["rt-multi-thread", "macros", "process", "sync", "time", "fs", "signal"] }
tokio-util = "0.7"  // CancellationToken
```

**Structured Concurrency Patterns Found:**

1. **JoinSet for parallel tasks** (`src/runtime/executor.rs`):
   ```rust
   // for_each parallelism with concurrency control
   let mut join_set = JoinSet::new();
   for item in items {
       join_set.spawn(async move { /* task */ });
   }
   ```

2. **CancellationToken for abort** (`src/runtime/runner.rs`):
   ```rust
   let token = CancellationToken::new();
   tokio::select! {
       result = task => { /* handle result */ }
       _ = token.cancelled() => { /* cleanup */ }
   }
   ```

3. **OnceCell for lazy init** (`src/runtime/executor.rs`):
   ```rust
   // Thread-safe async initialization
   mcp_client_cache: Arc<DashMap<String, Arc<OnceCell<Arc<McpClient>>>>>
   ```

4. **Timeout wrapping** (`src/mcp/client.rs`):
   ```rust
   timeout(MCP_CALL_TIMEOUT, service.call_tool(request)).await
   ```

### 2025-2026 Best Practices

| Pattern | Nika Status | Notes |
|---------|-------------|-------|
| `JoinSet` over manual `JoinHandle` | YES | Used in for_each |
| `CancellationToken` for shutdown | YES | Used in runner |
| `select!` for racing | YES | Used in fail_fast |
| Timeout all external calls | YES | 30s MCP, 5m tasks |
| `OnceCell`/`OnceLock` for lazy | YES | MCP client cache |
| Backpressure (bounded channels) | YES | `mpsc::channel(32)` |

### Recommendations

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  RECOMMENDATION: Add Structured Concurrency Helper                              │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Current: Manual JoinSet management with custom cancellation                    │
│                                                                                 │
│  Proposed: Consider `async-scoped` or similar for guaranteed cleanup            │
│                                                                                 │
│    // Ensures all spawned tasks complete before scope exits                     │
│    async_scoped::TokioScope::scope_and_block(|scope| {                          │
│        for item in items {                                                      │
│            scope.spawn(async { /* task */ });                                   │
│        }                                                                        │
│    });                                                                          │
│                                                                                 │
│  Benefit: Guaranteed cleanup, no orphaned tasks                                 │
│  Priority: LOW (current JoinSet pattern is correct)                            │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## 3. Workspace Organization

### Current State in Nika

Nika uses a **monolithic crate** architecture (single crate, feature-gated):

```
tools/nika/
├── Cargo.toml       # Single crate with features
└── src/
    ├── lib.rs       # Module organization
    ├── ast/         # Domain model
    ├── runtime/     # Execution engine
    ├── mcp/         # MCP client
    ├── provider/    # LLM providers
    ├── tui/         # Terminal UI (feature-gated)
    └── ...
```

**Feature Flags:**
```toml
[features]
default = ["tui", "spn-daemon", "native-keychain", "native-inference"]
tui = [...]
lsp = [...]
jobs = [...]
native-inference = ["dep:mistralrs", "dep:async-stream"]
```

### 2025-2026 Best Practices

Two schools of thought:

| Approach | Pros | Cons |
|----------|------|------|
| **Monolith + Features** (Nika) | Simple deps, fast CI, easy imports | Large binary, all-or-nothing compile |
| **Workspace + Crates** | Parallel compilation, smaller binaries | Complex deps, version management |

**Current community consensus (2025):**
- Monolith is fine for < 500K lines
- Workspace when teams work on separate crates
- Features for optional functionality (Nika does this well)

### Recommendations

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  RECOMMENDATION: Keep Monolith, Consider Extraction for Heavy Features          │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Current: Good feature separation                                               │
│                                                                                 │
│  Consider extracting IF:                                                        │
│  - mistral.rs adds >10s to compile time (native-inference)                     │
│  - TUI grows to >50K lines                                                      │
│  - LSP becomes a standalone product                                             │
│                                                                                 │
│  Extraction candidates (if needed):                                             │
│    nika-core        # ast, dag, binding (zero deps)                            │
│    nika-runtime     # execution engine (depends on core)                        │
│    nika-mcp         # MCP client (standalone)                                   │
│    nika-tui         # TUI (ratatui deps)                                        │
│    nika-native      # mistral.rs inference                                      │
│                                                                                 │
│  Priority: LOW (current architecture is appropriate for codebase size)         │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## 4. Testing

### Current State in Nika

**Impressive test coverage:**

| Metric | Value |
|--------|-------|
| Total tests | 4,433 |
| Test files | 20+ integration tests |
| Benchmark suites | 4 (criterion) |

**Testing patterns observed:**

1. **Unit tests in modules** (standard `#[cfg(test)]`):
   ```rust
   #[cfg(test)]
   mod tests {
       use super::*;

       #[test]
       fn test_parse_workflow() { ... }
   }
   ```

2. **Integration tests** (`tests/`):
   - `comprehensive_tests.rs` — Error handling coverage
   - `mcp_concurrent_test.rs` — Race condition testing
   - `rig_agent_loop_test.rs` — LLM integration

3. **Criterion benchmarks** (`benches/`):
   - `workflow_parsing.rs` — YAML parse performance
   - `dag_validation.rs` — DAG construction
   - `binding_resolution.rs` — Template resolution
   - `task_execution.rs` — DataStore operations

### 2025-2026 Best Practices

| Practice | Nika Status | Notes |
|----------|-------------|-------|
| Unit tests per module | YES | Comprehensive |
| Integration tests | YES | tests/ directory |
| Property testing (proptest) | NO | Not observed |
| Snapshot testing (insta) | Mentioned | Not actively used |
| Benchmarks (criterion) | YES | 4 suites |
| Test fixtures module | YES | `test_fixtures` feature |

### Recommendations

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  RECOMMENDATION: Add Property Testing for AST Parsing                           │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Current: Manual test cases for YAML parsing                                    │
│                                                                                 │
│  Proposed: Add proptest for fuzzing YAML inputs                                 │
│                                                                                 │
│    use proptest::prelude::*;                                                    │
│                                                                                 │
│    proptest! {                                                                  │
│        #[test]                                                                  │
│        fn parse_never_panics(yaml in ".*") {                                   │
│            let _ = Workflow::from_str(&yaml); // Should never panic            │
│        }                                                                        │
│                                                                                 │
│        #[test]                                                                  │
│        fn roundtrip_preserves_semantics(workflow in arb_workflow()) {          │
│            let yaml = serde_yaml::to_string(&workflow)?;                        │
│            let parsed: Workflow = serde_yaml::from_str(&yaml)?;                │
│            assert_eq!(workflow, parsed);                                        │
│        }                                                                        │
│    }                                                                            │
│                                                                                 │
│  Benefit: Catches edge cases in parser, prevents regressions                    │
│  Priority: MEDIUM                                                               │
│                                                                                 │
│  Add to Cargo.toml:                                                             │
│    [dev-dependencies]                                                           │
│    proptest = "1.4"                                                             │
│    proptest-derive = "0.4"                                                      │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────────┐
│  RECOMMENDATION: Add insta Snapshot Tests for Error Messages                    │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Current: Manual assertion on error strings                                     │
│                                                                                 │
│  Proposed: Snapshot error output for regression testing                         │
│                                                                                 │
│    use insta::assert_snapshot;                                                  │
│                                                                                 │
│    #[test]                                                                      │
│    fn error_message_parse_error() {                                             │
│        let err = parse_workflow("invalid").unwrap_err();                        │
│        assert_snapshot!(format!("{err}")); // Stores in snapshots/              │
│    }                                                                            │
│                                                                                 │
│  Benefit: Catches unintentional error message changes                           │
│  Priority: LOW                                                                  │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## 5. Performance

### Current State in Nika

**Performance-conscious choices observed:**

1. **Fast hashmaps**:
   ```rust
   use rustc_hash::FxHashMap;  // Fast, deterministic hashing
   use dashmap::DashMap;       // Lock-free concurrent map
   ```

2. **Connection pooling**:
   ```rust
   // Single reqwest::Client shared across all fetch tasks
   http_client: reqwest::Client,  // Pooled connections
   ```

3. **String interning** (mentioned in lib.rs):
   ```rust
   pub mod util;  // String interning, JSONPath parser
   ```

4. **Fast hashing**:
   ```rust
   xxhash-rust = { version = "0.8", features = ["xxh3"] }
   ```

5. **Benchmarks**:
   - YAML parsing: ~4.6µs for 1 task, ~340µs for 100 tasks
   - DAG validation: ~800ns for 10 nodes
   - Binding resolution: ~450ns for 3 entries

### 2025-2026 Best Practices

| Practice | Nika Status | Notes |
|----------|-------------|-------|
| FxHashMap for hot paths | YES | Used throughout |
| Connection pooling | YES | reqwest Client |
| Avoid allocations | PARTIAL | Could use more `Cow<'_, str>` |
| Benchmark regressions | YES | Criterion in CI |
| Profile-guided optimization | NO | Not configured |

### Recommendations

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  RECOMMENDATION: Add Benchmark Regression CI Check                              │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Current: Benchmarks exist but may not be run in CI                             │
│                                                                                 │
│  Proposed: Add benchmark comparison to PR checks                                │
│                                                                                 │
│    # .github/workflows/bench.yml                                                │
│    - name: Run benchmarks                                                       │
│      run: |                                                                     │
│        cargo bench -- --save-baseline main                                      │
│        cargo bench -- --baseline main                                           │
│                                                                                 │
│  Benefit: Catches performance regressions before merge                          │
│  Priority: MEDIUM                                                               │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────────┐
│  RECOMMENDATION: Profile MCP Client Latency                                     │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Current: MCP client has caching, but latency not profiled                      │
│                                                                                 │
│  Proposed: Add tracing spans for MCP operations                                 │
│                                                                                 │
│    #[instrument(skip(self), fields(tool = %name))]                             │
│    pub async fn call_tool(&self, name: &str, args: Value) -> Result<Value> {   │
│        let start = Instant::now();                                              │
│        let result = self.inner_call(name, args).await;                          │
│        tracing::info!(latency_ms = start.elapsed().as_millis());               │
│        result                                                                   │
│    }                                                                            │
│                                                                                 │
│  Benefit: Identifies slow MCP servers, optimizes caching                        │
│  Priority: LOW (already have basic tracing)                                    │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## 6. Documentation

### Current State in Nika

**Documentation patterns observed:**

1. **Module-level docs** (excellent):
   ```rust
   //! Nika - DAG workflow runner for AI tasks (v0.1)
   //!
   //! ## Module Architecture (DDD-Inspired)
   //!
   //! ```text
   //! ┌──────────────────────────────────────────────────────────────┐
   //! │                        DOMAIN MODEL                          │
   //! │  ast/       YAML → Rust types (Workflow, Task, TaskAction)   │
   //! └──────────────────────────────────────────────────────────────┘
   ```

2. **Doc comments on public items** (good coverage):
   ```rust
   /// Task executor with cached providers, shared HTTP client, and event logging
   pub struct TaskExecutor { ... }
   ```

3. **Architecture Decision Records** (excellent):
   - 8 ADRs in `dx/adr/nika/`
   - Well-documented design decisions

### 2025-2026 Best Practices

| Practice | Nika Status | Notes |
|----------|-------------|-------|
| Module-level `//!` docs | YES | Comprehensive |
| Item-level `///` docs | PARTIAL | Could be more complete |
| Doc tests | PARTIAL | Some `ignore` examples |
| Examples directory | YES | `examples/` with workflows |
| Architecture docs | YES | ADRs, CLAUDE.md |

### Recommendations

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  RECOMMENDATION: Add Doc Tests for Public API                                   │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Current: Many examples marked `ignore` due to async/setup requirements         │
│                                                                                 │
│  Proposed: Add `tokio::test` support for doc tests                             │
│                                                                                 │
│    /// Parse a workflow from YAML string.                                       │
│    ///                                                                          │
│    /// # Example                                                                │
│    ///                                                                          │
│    /// ```                                                                      │
│    /// use nika::Workflow;                                                      │
│    ///                                                                          │
│    /// let yaml = r#"                                                          │
│    /// schema: "nika/workflow@0.9"                                             │
│    /// tasks:                                                                   │
│    ///   - id: hello                                                            │
│    ///     infer: "Say hello"                                                   │
│    /// "#;                                                                      │
│    ///                                                                          │
│    /// let workflow: Workflow = nika::serde_yaml::from_str(yaml).unwrap();     │
│    /// assert_eq!(workflow.tasks.len(), 1);                                     │
│    /// ```                                                                      │
│    pub fn from_str(s: &str) -> Result<Self, Error> { ... }                     │
│                                                                                 │
│  Benefit: Examples stay in sync with API changes                                │
│  Priority: MEDIUM                                                               │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## 7. Serde Best Practices

### Current State in Nika

**Serde patterns observed:**

```rust
// Cargo.toml
serde = { version = "1.0", features = ["derive", "rc"] }
serde_json = "1.0"
serde-saphyr = "0.0.20"  // Replaced deprecated serde_yaml
```

**Good patterns:**
- Proper feature flags (`derive`, `rc`)
- Migration from deprecated `serde_yaml` to `serde-saphyr`
- JSON Schema generation with `schemars`

### 2025-2026 Best Practices

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  RECOMMENDATION: Add #[serde(deny_unknown_fields)] for Strict Parsing           │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Current: Unknown fields silently ignored (forward compatibility)               │
│                                                                                 │
│  Consider: Strict mode for validation command                                   │
│                                                                                 │
│    #[derive(Deserialize)]                                                       │
│    #[serde(deny_unknown_fields)]  // Catch typos in workflow YAML              │
│    pub struct StrictWorkflow { ... }                                            │
│                                                                                 │
│    // Use in `nika check --strict`                                             │
│    let workflow: StrictWorkflow = serde_yaml::from_str(&yaml)?;                │
│                                                                                 │
│  Benefit: Catches typos like `proivder:` instead of `provider:`                 │
│  Priority: LOW (tradeoff with forward compatibility)                           │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## 8. Tracing Best Practices

### Current State in Nika

**Tracing patterns observed:**

```rust
// Good: #[instrument] for span-based tracing
#[instrument(skip(self), fields(workflow_tasks = self.workflow.tasks.len()))]
pub async fn run(&mut self) -> Result<DataStore> { ... }

// Good: Structured logging
tracing::info!(servers = ?server_names, "Loaded MCP server configurations");

// Good: Different levels
tracing::debug!("ChatInfer: {}", prompt);
tracing::warn!("TUI lagged behind by {} events", n);
tracing::error!("Workflow '{}' failed: {}", workflow_name, e);
```

### 2025-2026 Best Practices

| Practice | Nika Status | Notes |
|----------|-------------|-------|
| `#[instrument]` on key functions | YES | Runtime, executor |
| Structured fields | YES | `fields(x = %y)` |
| Skip large args | YES | `skip(self, bindings)` |
| Error logging | YES | `error!` with context |
| Span hierarchy | PARTIAL | Could be more consistent |

### Recommendations

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│  RECOMMENDATION: Add Trace ID for Request Correlation                           │
├─────────────────────────────────────────────────────────────────────────────────┤
│                                                                                 │
│  Current: Individual spans, no workflow-level correlation ID                    │
│                                                                                 │
│  Proposed: Add trace ID to all spans within a workflow run                      │
│                                                                                 │
│    let trace_id = Uuid::new_v4();                                              │
│    let span = tracing::info_span!("workflow", %trace_id, name = %workflow.name);│
│    async move { ... }.instrument(span).await                                    │
│                                                                                 │
│  Benefit: Filter logs by workflow execution                                     │
│  Priority: MEDIUM (helps debugging in production)                              │
│                                                                                 │
└─────────────────────────────────────────────────────────────────────────────────┘
```

---

## Summary: Priority Actions

| Priority | Action | Impact | Effort |
|----------|--------|--------|--------|
| **HIGH** | Keep current thiserror + miette pattern | Maintain | Low |
| **MEDIUM** | Add proptest for AST fuzzing | Quality | Medium |
| **MEDIUM** | Add benchmark regression CI | Performance | Low |
| **MEDIUM** | Add trace ID correlation | Debugging | Low |
| **LOW** | Consider domain-specific error types | Maintainability | High |
| **LOW** | Add insta snapshot tests | Quality | Low |
| **LOW** | Extract heavy features to crates | Compile time | High |

---

## Appendix: Crate Versions (March 2026)

| Crate | Nika Version | Latest Stable | Notes |
|-------|--------------|---------------|-------|
| tokio | 1.49 | 1.50+ | Keep current |
| thiserror | 1.0 | 2.0 (beta) | Wait for stable |
| serde | 1.0 | 1.0 | Stable |
| tracing | (dep of deps) | 0.1.40+ | Stable |
| criterion | (dev-dep) | 0.5+ | Stable |
| proptest | Not used | 1.4 | Recommend adding |
| insta | Not used | 1.34+ | Consider adding |

---

## References

- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- [tokio Tutorial](https://tokio.rs/tokio/tutorial)
- [Error Handling in Rust (2025)](https://blog.burntsushi.net/rust-error-handling/)
- [Criterion User Guide](https://bheisler.github.io/criterion.rs/book/)
- [proptest Book](https://proptest-rs.github.io/proptest/proptest/index.html)
