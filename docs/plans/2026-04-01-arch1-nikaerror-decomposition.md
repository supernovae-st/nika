# ARCH-1: NikaError Domain Enum Decomposition

> Decompose the 75-variant NikaError god enum into ~10 domain enums.
> Post-launch refactor. Estimated: 7-10 days across 4 phases, 12 commits.

## Current State (audited 2026-04-01)

| Metric | Value |
|--------|-------|
| File | `tools/nika-engine/src/error.rs` — 2,797 lines |
| Domain file | `tools/nika-engine/src/error_domains.rs` — 251 lines |
| Named variants | **75** (not 103 — the "103" counts match arms across `code()` + `FixSuggestion` + `is_recoverable()`) |
| Existing domain enums | 4 (BindingError, ExecutionError, ProviderError, DagError) |
| Domain adoption | **6%** (78 of ~1,319 callsites) |
| `From<Domain> for NikaError` | Implemented for all 4 existing domains |
| `FixSuggestion` | Implemented on all 75 variants |
| Cross-crate users | nika-tui, nika-cli (both import `NikaError` directly) |

## Architecture Decision: Flat Mapping (No Wrapper Variants)

Follow the **existing pattern** from `error_domains.rs`:
- Domain enums define variants with `#[error("[NIKA-XXX] ...")]` strings
- `From<DomainEnum> for NikaError` maps each to the corresponding flat NikaError variant
- **NikaError keeps all 75 variants unchanged** — domain enums are an additive ergonomic layer
- `FixSuggestion` on NikaError delegates to domain enums via `From` chain

**Why not wrapper variants?** Wrapper variants (`NikaError::Binding(BindingError)`) would require changing `code()`, `FixSuggestion`, `is_recoverable()`, and all 13 cross-crate `From` impls. The flat pattern costs more lines but **zero behavioral change and zero risk**.

## Known Bugs to Fix During Migration

| Bug | Location | Fix |
|-----|----------|-----|
| `ProviderError::FallbackChainExhausted` field mismatch | `error_domains.rs` | `last_provider` vs `providers` semantic lie — normalize fields |
| `WorkflowTimeout` missing `[NIKA-038]` prefix | `error.rs` | Add `[NIKA-038]` to `#[error]` string |
| `runner.rs` hardcoded NIKA-XXX string literals | `runner.rs:750-770` | Use `NikaError::code()` or constants |
| `is_retryable()` string inspection | `runner.rs:943` | Move classification to domain enum level |
| 2 unused `ProviderError` variants | `error_domains.rs` | Remove `EndpointNotFound`, `EndpointConnectionFailed` if truly dead |

## Complete Domain Enum Taxonomy (10 enums)

### Existing Enums — Extend

#### 1. `DagError` (NIKA-020..083) — extend from 3 to 7 variants

| Variant | NIKA | Status |
|---------|------|--------|
| `CycleDetected { cycle }` | 020 | EXISTS |
| `MissingDependency { task_id, dep_id }` | 021 | EXISTS |
| `DuplicateTaskId { task_id }` | 022 | EXISTS |
| `DependencyChainFailed { count, blocked_tasks, root_failure }` | 026 | **NEW** |
| `TaskCancelled { task_id, reason }` | 027 | **NEW** |
| `SemaphoreClosed { task_id }` | 028 | **NEW** |
| `RuntimeDeadlock { details }` | 083 | **NEW** |

#### 2. `ProviderError` (NIKA-030..038) — extend from 7 to 8 variants

| Variant | NIKA | Status |
|---------|------|--------|
| (existing 7) | 030-037 | EXISTS (fix FallbackChainExhausted fields) |
| `WorkflowTimeout { duration_secs, running_tasks }` | 038 | **NEW** |

#### 3. `BindingError` (NIKA-040..082) — extend from 3 to 11 variants

| Variant | NIKA | Status |
|---------|------|--------|
| `TemplateError { template, reason }` | 041 | EXISTS |
| `NotFound { alias }` | 042 | EXISTS |
| `TypeMismatch { expected, actual, path }` | 043 | EXISTS |
| `UnknownAlias { alias, task_id }` | 071 | **NEW** |
| `NullValue { path, alias }` | 072 | **NEW** |
| `InvalidTraversal { segment, value_type, full_path }` | 073 | **NEW** |
| `TemplateParse { position, details }` | 074 | **NEW** |
| `VaultAccess { service, field, reason }` | 075 | **NEW** |
| `WithUnknownTask { alias, from_task, task_id }` | 080 | **NEW** |
| `WithNotUpstream { alias, from_task, task_id }` | 081 | **NEW** |
| `WithCircularDep { alias, from_task, task_id }` | 082 | **NEW** |

#### 4. `ExecutionError` (NIKA-044..098) — extend from 6 to 7 variants

| Variant | NIKA | Status |
|---------|------|--------|
| (existing 6: ExecFailed, FetchFailed, ExtractFailed, General, Cancelled, Panicked) | 044-098 | EXISTS |
| `InvokeParamError { reason }` | 047 | **NEW** |

### New Enums — Create

#### 5. `SecurityError` (NIKA-050..056) — 5 variants, ~30 callsites

```rust
pub enum SecurityError {
    InvalidPath { path: String },                     // NIKA-050
    PathNotFound { path: String },                    // NIKA-052
    BlockedCommand { command: String, reason: String }, // NIKA-053
    InvalidTaskId { id: String, reason: String },     // NIKA-055
    InvalidDefault { raw: String, reason: String },   // NIKA-056
}
```

#### 6. `OutputError` (NIKA-060..062) — 3 variants, ~20 callsites

```rust
pub enum OutputError {
    InvalidJson { details: String },      // NIKA-060
    SchemaFailed { details: String },     // NIKA-061
    SerializationError { details: String }, // NIKA-062
}
```

#### 7. `AgentError` (NIKA-112..116) — 5 variants, ~25 callsites

```rust
pub enum AgentError {
    GuardrailViolation { task_id: String, violations: Vec<String> }, // NIKA-112
    ValidationFailed { reason: String },                             // NIKA-113
    LimitExceeded { limit_type: String, current: f64, maximum: f64 }, // NIKA-114
    ExecutionFailed { task_id: String, reason: String },             // NIKA-115
    ThinkingCaptureFailed { reason: String },                        // NIKA-116
}
```

#### 8. `ToolError` (NIKA-200..213) — 4 variants, ~83 callsites

```rust
pub enum ToolError {
    FileToolError { code: String, message: String },  // NIKA-200
    BuiltinError { tool: String, reason: String },    // NIKA-210
    BuiltinInvalidParams { tool: String, reason: String }, // NIKA-212
    AssertionFailed { message: String, condition: String }, // NIKA-213
}
```

#### 9. `ArtifactError` (NIKA-280..285) — 4 variants, ~15 callsites

```rust
pub enum ArtifactError {
    PathError { path: String, reason: String },        // NIKA-280
    WriteFailed { path: String, reason: String },      // NIKA-281
    SizeExceeded { path: String, size: u64, max_size: u64 }, // NIKA-282
    MediaStoreLocked { reason: String },               // NIKA-285
}
```

#### 10. `StructuredOutputError` (NIKA-300..303) — 4 variants, ~40 callsites

```rust
pub enum StructuredOutputError {
    ExtractionFailed { task_id: String, layer: String, reason: String },           // NIKA-300
    ValidationFailed { task_id: String, layer: String, attempt: u32, errors: Vec<String> }, // NIKA-301
    RepairFailed { task_id: String, original_errors: Vec<String>, repair_errors: Vec<String> }, // NIKA-302
    AllLayersFailed { task_id: String, attempts: u32, final_errors: String },       // NIKA-303
}
```

### NOT Migrated (stay in NikaError directly)

| Variants | Reason |
|----------|--------|
| `IoError`, `JsonError`, `YamlParse` (3 `#[from]`) | Primitive cross-cutting |
| `MediaError` (`#[from]`) | Transparent from nika-media crate |
| `Mcp*` (10 variants) | Already handled by `nika_mcp::McpError` → `NikaError` bridge |
| `Course*` (5), `Record*` (5) | Isolated subsystems, low callsite count |
| `TuiError`, `ConfigError`, `Timeout`, `PolicyViolation`, `Boot/Startup`, `DecomposeTimeout` | Cross-cutting or single callsite |
| `ContextLoadError`, `SkillLoadError`, `InvalidPkgUri`, `PackageNotFound`, `JsonPathUnsupported` | Low callsite count |

**Total: 75 variants remain in NikaError. ~55 get domain enum constructors. ~20 stay NikaError-only.**

## FixSuggestion Strategy

**Implement `FixSuggestion` on each domain enum independently.**

```rust
impl FixSuggestion for AgentError {
    fn fix_suggestion(&self) -> Option<String> {
        match self {
            Self::GuardrailViolation { .. } => Some("Review guardrail config...".into()),
            // ...
        }
    }
}
```

Suggestions are **copied** from `NikaError::fix_suggestion()` match arms. Duplication is mechanical (5-15 lines per enum). No macro — explicit is better.

`NikaError::fix_suggestion()` stays unchanged — the `From` chain means both paths produce the same result.

## File Organization

**Keep everything in `error_domains.rs`**. At 10 enums it will be ~600-700 lines — acceptable for a taxonomy file. No split into `error/binding.rs` etc.

## Migration Pattern (per variant)

```rust
// BEFORE (callsite):
return Err(NikaError::ArtifactWriteError { path: "...", reason: "..." });

// AFTER (callsite):
return Err(ArtifactError::WriteFailed { path: "...", reason: "..." }.into());

// The From impl (in error_domains.rs):
impl From<ArtifactError> for NikaError {
    fn from(e: ArtifactError) -> Self {
        match e {
            ArtifactError::WriteFailed { path, reason } =>
                NikaError::ArtifactWriteError { path, reason },
            // ...
        }
    }
}
```

## Phased Execution Plan

### Phase 1 — Extend Existing Enums (2 days, 4 commits)

| Commit | Action | Files | Tests |
|--------|--------|-------|-------|
| 1a | Extend `DagError` +4 variants + From + FixSuggestion | error_domains.rs | 4 new tests |
| 1b | Extend `BindingError` +8 variants + From + FixSuggestion | error_domains.rs | 8 new tests |
| 1c | Fix `ProviderError::FallbackChainExhausted` fields + add `WorkflowTimeout` | error_domains.rs | 2 new tests |
| 1d | Extend `ExecutionError` +1 variant (`InvokeParamError`) | error_domains.rs | 1 new test |

**Verify after each**: `cargo test --workspace --lib` + `cargo clippy`

### Phase 2 — Create High-Callsite Domains (3 days, 3 commits)

| Commit | Action | New enum | Variants | Estimated callsites |
|--------|--------|----------|----------|---------------------|
| 2a | Create `StructuredOutputError` | NIKA-300..303 | 4 | ~40 |
| 2b | Create `AgentError` | NIKA-112..116 | 5 | ~25 |
| 2c | Create `ToolError` | NIKA-200..213 | 4 | ~83 |

Each commit: enum definition + `From` impl + `FixSuggestion` impl + 3 tests (to_string, From, send_sync) + 1 test (fix_suggestion).

### Phase 3 — Create Medium-Callsite Domains (2 days, 3 commits)

| Commit | Action | New enum | Variants |
|--------|--------|----------|----------|
| 3a | Create `ArtifactError` | NIKA-280..285 | 4 |
| 3b | Create `SecurityError` | NIKA-050..056 | 5 |
| 3c | Create `OutputError` | NIKA-060..062 | 3 |

### Phase 4 — Cleanup + Re-exports (1 day, 2 commits)

| Commit | Action |
|--------|--------|
| 4a | Add all new domain enums to `lib.rs` re-exports |
| 4b | Fill missing `fix_suggestion_for_code()` entries, fix `WorkflowTimeout` display |

### Phase 5 — Callsite Migration (optional, separate PRs)

Migrate callsites file-by-file in separate PRs:
- `runtime/structured_output.rs` → use `StructuredOutputError`
- `runtime/artifact_processor.rs` → use `ArtifactError`
- `runtime/security.rs` → use `SecurityError`
- `binding/resolve.rs` → use `BindingError` for NIKA-071..082
- `runtime/runner.rs` → use `DagError` for NIKA-026, 083

## Verification Checklist

- [ ] `cargo test --workspace --lib` after each commit
- [ ] `cargo clippy --workspace -- -D warnings` clean
- [ ] All NIKA-XXX codes preserved (grep existing docs)
- [ ] `FixSuggestion` returns identical strings for domain vs NikaError paths
- [ ] Error display strings unchanged (test: `to_string().contains("[NIKA-XXX]")`)
- [ ] No new `.unwrap()` introduced
- [ ] Domain enums are `pub` (not `pub(crate)`) for cross-crate access
- [ ] `fix_suggestion_for_code()` covers all new domain codes

## Socratic Answers (Resolved)

1. **Wrapper vs flat**: Flat mapping. Zero risk, matches existing pattern.
2. **FixSuggestion**: Impl on each domain enum + keep on NikaError (delegation).
3. **File org**: Single `error_domains.rs` — no split.
4. **McpError**: Skip — already handled by `nika_mcp::McpError` cross-crate bridge.
5. **Course/Record**: Skip — too isolated, not worth the churn.
6. **WorkflowError**: Create but only in Phase 4 (needs `SchemaError` field type from internal module).
