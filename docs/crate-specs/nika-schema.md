# Crate spec — `nika-schema`

| | |
|---|---|
| Status | Phase 1 — Step 4 of `nika-core` split |
| Layer | L0 (PURE, zero I/O, zero async) |
| Design | **Monolithic** — AST + parser + analyzer + validator + DAG in 1 crate (split rejected: circular deps) |
| LOC budget | ≤15,000 src (target ~13,000, alarm at 14,000, hard cap 15,000) |
| File cap | ≤1,500 LOC each |
| Function cap | ≤100 lines each |
| Source on `main` (reference) | `tools/nika-core/src/ast/` (22,544 LOC), `tools/nika-core/src/schema/` (400 LOC), `tools/nika-core/src/source/` (724 LOC), `tools/nika-core/src/trust.rs` (560 LOC), `tools/nika-core/src/binding/mention.rs` (851 LOC), `tools/nika-core/src/binding/validate.rs` (355 LOC) |
| Legacy total (reference only) | ~25,434 LOC (including ~8,000 LOC inline tests) — rewrite targets ~13,000 src + ~5,000 tests |
| Crate version | tracks workspace (bumped to `0.90.0-alpha.1` at Phase 1 close) |
| License | `AGPL-3.0-or-later` |
| Edition | 2024 |

---

## 1. Purpose

`nika-schema` is the **schema layer** for Nika: it defines the workflow AST
types, parses YAML into those types, analyzes them for correctness, and
validates the resulting DAG. This is the largest L0 crate and the core
data model of the entire engine.

### Three-phase pipeline

```
YAML string
    |
    v
[Parser] ─── YAML → RawWorkflow (spans, no validation)
    |
    v
[Analyzer] ── Raw → AnalyzedWorkflow (taint, guardrails, verb checks, DAG)
    |
    v
[Validator] ─ cycle detection, topological sort, schema validation
    |
    v
AnalyzedWorkflow (ready for lowering in nika-runtime)
```

The lowering step (AnalyzedWorkflow to runtime types) is NOT in this crate.
It lives in `nika-runtime` (L3) because it depends on runtime capabilities.

### Why 1 crate, not split

Decision locked (POST_AUDIT_REVISIONS.md, D1 + brainstorm correction):
splitting into `nika-schema-ast` + `nika-schema-analyze` creates circular
dependencies. The analyzer constructs analyzed types while reading raw types,
and both layers share span/source infrastructure. Module boundaries inside
1 crate achieve the same separation without the circular-dep problem.

### Structured output pure logic

Per brainstorm D8, the pure logic for structured output lives here:
- `extract_json()` — string-aware bracket-matching state machine
- `json_to_schema()` / `json_to_schema_strict()` — example-to-schema derivation
- `ValidatorCache` — LRU cache for compiled JSON Schema validators

The orchestration (retry loop, repair prompts) lives in `nika-verb-infer` (L2).

---

## 2. Layer + LOC budget + cap strategy

**Layer:** L0 — zero I/O, zero async, zero tokio. Pure data transformations.

**LOC budget:** 15,000 src (hard cap). Target ~13,000 src.

**Cap strategy — what if we approach 15k?**

The rewrite is expected to be significantly smaller than legacy (~17,400 prod
LOC across the reference files) because:

1. **Elimination of unwraps** replaces verbose match arms with `?` chains (net reduction)
2. **Deletion of dead code** — legacy has `#[allow(dead_code)]` patterns
3. **Shared types consolidate** — legacy duplicates span/source infrastructure
4. **Test code moves out** — legacy has ~8,000 LOC inline tests mixed in; diamond uses `#[cfg(test)] mod tests` blocks that are compact

If approaching 14,000 LOC despite rewrite savings:

| Priority | Action | Estimated savings |
|---|---|---|
| 1 | Move `mention.rs` back to nika-binding if it proves binding-coupled | ~500 LOC |
| 2 | Move `completion.rs` to nika-verb-agent (sole consumer) | ~600 LOC |
| 3 | Move `routing.rs` to nika-verb-agent (sole consumer) | ~40 LOC |
| 4 | Compress guardrails types into smaller representation | ~300 LOC |

These are escape hatches only. The rewrite should naturally land under 13k.

---

## 3. Public API surface (grouped by module)

### 3.1 Source tracking (`source::`)

```rust
// ── Source positions for miette diagnostics ──────────────────
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Default)]
pub struct FileId(pub u32);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Default)]
pub struct ByteOffset(pub u32);

#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub struct Span { pub file: FileId, pub start: ByteOffset, pub end: ByteOffset }

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct LineCol { pub line: u32, pub col: u32 }

pub struct Spanned<T> { pub value: T, pub span: Span }

pub struct SourceFile { pub id: FileId, pub path: PathBuf, pub content: Arc<String> }
pub struct SourceRegistry { /* intern files, convert offsets to line:col */ }

impl SourceRegistry {
    pub fn new() -> Self;
    pub fn add(&mut self, path: PathBuf, content: String) -> FileId;
    pub fn get(&self, id: FileId) -> Option<&SourceFile>;
    pub fn line_col(&self, file: FileId, offset: ByteOffset) -> Option<LineCol>;
}
```

### 3.2 Trust types (`trust::`)

```rust
// ── Taint propagation primitives ─────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum TrustLevel { Untrusted = 0, ModelTainted = 1, ModelGenerated = 2, Trusted = 3 }

impl TrustLevel {
    pub fn merge(self, other: Self) -> Self;      // min()
    pub fn is_untrusted(self) -> bool;             // Untrusted | ModelTainted
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvocationSource { Workflow, Agent, Chat, Api }

pub fn builtin_output_trust(name: &str) -> TrustLevel;
```

### 3.3 Raw AST types (`raw::`)

```rust
// ── What the YAML parser produces ────────────────────────────
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RawWorkflow {
    pub name: Option<Spanned<String>>,
    pub schema: Option<SchemaVersion>,
    pub tasks: Vec<RawTask>,
    pub context: Option<ContextConfig>,
    pub include: Vec<IncludeSpec>,
    pub mcp: Option<RawMcpConfig>,
    pub logging: Option<LogConfig>,
    // ...
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RawTask { pub name: Spanned<String>, pub action: RawAction, pub with: Option<WithSpec>, /* ... */ }

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum RawAction {
    Infer(RawInferAction),
    Exec(RawExecAction),
    Fetch(RawFetchAction),
    Invoke(RawInvokeAction),
    Agent(RawAgentAction),
}

// Sub-types: RawInferAction, RawExecAction, RawFetchAction, RawInvokeAction, RawAgentAction
// MCP config: RawMcpConfig, RawMcpServer
```

### 3.4 Parser (`parser::`)

```rust
// ── YAML → RawWorkflow ───────────────────────────────────────
pub fn parse(yaml: &str, file_id: FileId) -> Result<RawWorkflow, ParseError>;

// ParseError with span information for miette diagnostics
#[derive(Debug)]
pub struct ParseError {
    pub message: String,
    pub kind: ParseErrorKind,
    pub span: Option<Span>,
}
```

### 3.5 Analyzed AST types (`analyzed::`)

```rust
// ── Post-analysis validated types ────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TaskId(pub u32);  // interned index

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct AnalyzedWorkflow {
    pub name: Option<String>,
    pub tasks: Vec<AnalyzedTask>,
    pub task_order: Vec<TaskId>,  // topological
    pub context: Option<ContextConfig>,
    // ...
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct AnalyzedTask {
    pub id: TaskId,
    pub name: String,
    pub action: AnalyzedTaskAction,
    pub depends_on: Vec<TaskId>,
    pub trust_level: TrustLevel,
    // ...
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum AnalyzedTaskAction {
    Infer { /* ... */ },
    Exec { /* ... */ },
    Fetch { /* ... */ },
    Invoke { /* ... */ },
    Agent { /* ... */ },
}
```

### 3.6 Analyzer (`analyzer::`)

```rust
// ── Raw → Analyzed transformation ────────────────────────────
pub fn analyze(raw: RawWorkflow) -> AnalysisResult;

pub struct AnalysisResult {
    pub workflow: Option<AnalyzedWorkflow>,
    pub errors: Vec<AnalysisError>,
    pub warnings: Vec<AnalysisWarning>,
}

impl AnalysisResult {
    pub fn into_result(self) -> Result<AnalyzedWorkflow, Vec<AnalysisError>>;
    pub fn has_errors(&self) -> bool;
}

#[derive(Debug)]
pub struct AnalysisError { pub kind: AnalysisErrorKind, pub span: Option<Span>, pub message: String }
#[derive(Debug)]
pub struct AnalysisWarning { pub kind: AnalysisWarningKind, pub span: Option<Span>, pub message: String }
```

### 3.7 Taint analyzer (`taint::`)

```rust
// ── Compile-time taint propagation ───────────────────────────
pub struct TaintAnalyzer;

impl TaintAnalyzer {
    pub fn analyze(workflow: &AnalyzedWorkflow) -> TaintReport;
}

pub struct TaintReport {
    pub task_trust: HashMap<TaskId, TrustLevel>,
    pub warnings: Vec<TaintWarning>,
}

pub struct TaintWarning { pub task: TaskId, pub message: String, pub severity: TaintSeverity }
pub enum TaintSeverity { Info, Warning, Critical }
```

### 3.8 DAG validation (`dag::`)

```rust
// ── Cycle detection + topological sort ───────────────────────
pub fn detect_cycles(tasks: &[AnalyzedTask]) -> Result<(), CycleError>;
pub fn topological_sort(tasks: &[AnalyzedTask]) -> Result<Vec<TaskId>, CycleError>;

pub struct CycleError { pub cycle: Vec<TaskId> }
```

### 3.9 Guardrails (`guardrails::`)

```rust
// ── Agent output validation config ───────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Guardrail {
    Length(LengthGuardrail),
    Schema(SchemaGuardrail),
    Regex(RegexGuardrail),
    Llm(LlmGuardrail),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum EscalationAction { Retry, Escalate, Fail }

// Sub-types: LengthGuardrail, SchemaGuardrail, RegexGuardrail, LlmGuardrail
```

### 3.10 Structured output (`structured::`)

```rust
// ── Schema enforcement config ────────────────────────────────
#[derive(Debug, Clone, Serialize)]
#[non_exhaustive]
pub struct StructuredOutputSpec {
    pub schema: Option<SchemaRef>,
    pub from_example: Option<serde_json::Value>,
    pub max_retries: u32,
    pub enable_repair: bool,
    pub repair_model: Option<String>,
    pub strict: bool,
}
```

### 3.11 JSON schema utilities (`json_schema::`)

```rust
// ── Pure schema logic (D8 structured output phases 1-2) ──────
pub fn json_to_schema(value: &serde_json::Value) -> serde_json::Value;
pub fn json_to_schema_strict(value: &serde_json::Value) -> serde_json::Value;
pub fn extract_json(text: &str) -> Option<&str>;

pub struct ValidatorCache { /* LRU<blake3::Hash, CompiledSchema> */ }
impl ValidatorCache {
    pub fn new(capacity: usize) -> Self;
    pub fn validate(&self, json: &serde_json::Value, schema: &serde_json::Value) -> Result<(), Vec<ValidationError>>;
}
```

### 3.12 Completions / suggestions (`completion::`)

```rust
// ── LSP + CLI suggestions ────────────────────────────────────
pub fn complete_task_ids(workflow: &AnalyzedWorkflow, prefix: &str) -> Vec<String>;
pub fn complete_provider_names(prefix: &str) -> Vec<String>;   // delegates to nika-catalog
pub fn complete_model_names(provider: &str, prefix: &str) -> Vec<String>;
pub fn complete_transform_names(prefix: &str) -> Vec<String>;
pub fn suggest_did_you_mean(input: &str, candidates: &[&str]) -> Option<String>;
```

### 3.13 Mention parsing (`mention::`) — moved from binding

```rust
// ── @reference parsing for Chat-as-DAG ───────────────────────
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mention { Number(u32), Last, All, Range(u32, u32) }

pub fn parse_mentions(text: &str) -> Vec<Mention>;
pub fn has_parallel_marker(text: &str) -> bool;
pub fn strip_parallel_marker(text: &str) -> &str;
```

### 3.14 Task ID validation (`validate::`) — moved from binding

```rust
// ── Snake_case task ID validation ────────────────────────────
pub fn validate_task_id(id: &str) -> Result<(), SchemaError>;
```

### 3.15 Convenience pipeline

```rust
// ── Full pipeline: YAML → AnalyzedWorkflow ───────────────────
pub fn parse_analyzed(yaml: &str) -> Result<AnalyzedWorkflow, NikaError>;
```

### 3.16 AST type modules (re-exported from root)

```rust
// ── Workflow configuration types ─────────────────────────────
pub use types::{
    AgentDef, ArtifactsConfig, ArtifactSpec,
    Budget, BudgetConfig,
    CompletionConfig, CompletionMode,
    ContentPart,
    ContextConfig,
    DecomposeSpec, DecomposeStrategy,
    ExtractMode, ResponseMode,
    GuardrailConfig,
    IncludeSpec,
    LimitsConfig, LimitType, LimitAction, LimitStatus,
    LogConfig, LogFormat, LogLevel,
    OrchestrateConfig,
    OutputFormat, OutputPolicy, SchemaRef,
    RecordSpec,
    RoutingConfig,
    ScheduleConfig,
    SchemaVersion,
    StructuredOutputSpec,
    Templatable,
};
```

---

## 4. Module structure with LOC estimates

Every file stays under 1,500 LOC. The legacy `parser.rs` (4,377 LOC) and
`guardrails.rs` (2,166 LOC) are split into modules.

```
crates/nika-schema/
  Cargo.toml
  src/
    lib.rs                          (~80 LOC — pub mod + re-exports + parse_analyzed)

    error.rs                        (~120 LOC — SchemaError enum + NikaErrorCode impl)

    source/
      mod.rs                        (~20 LOC — re-exports)
      span.rs                       (~200 LOC — FileId, ByteOffset, Span, Spanned, LineCol)
      registry.rs                   (~250 LOC — SourceRegistry, SourceFile)

    trust.rs                        (~350 LOC — TrustLevel, InvocationSource, builtin_output_trust)

    types/
      mod.rs                        (~60 LOC — re-exports)
      schema_version.rs             (~80 LOC — SchemaVersion enum)
      budget.rs                     (~250 LOC — Budget, BudgetConfig, from_str_with_budget)
      content.rs                    (~200 LOC — ContentPart types)
      context.rs                    (~60 LOC — ContextConfig)
      decompose.rs                  (~70 LOC — DecomposeSpec, DecomposeStrategy)
      extract.rs                    (~150 LOC — ExtractMode, ResponseMode)
      include.rs                    (~80 LOC — IncludeSpec)
      limits.rs                     (~300 LOC — LimitsConfig, LimitType, LimitAction)
      logging.rs                    (~150 LOC — LogConfig, LogFormat, LogLevel)
      orchestrate.rs                (~60 LOC — OrchestrateConfig)
      output.rs                     (~200 LOC — OutputFormat, OutputPolicy, SchemaRef)
      record.rs                     (~80 LOC — RecordSpec)
      routing.rs                    (~30 LOC — RoutingConfig)
      schedule.rs                   (~300 LOC — ScheduleConfig, CronExpr)
      structured.rs                 (~300 LOC — StructuredOutputSpec + custom Deserialize)
      templatable.rs                (~120 LOC — Templatable<T>)
      agent_def.rs                  (~250 LOC — AgentDef)
      artifact.rs                   (~250 LOC — ArtifactsConfig, ArtifactSpec)
      completion.rs                 (~600 LOC — CompletionConfig, CompletionMode, signals)

    guardrails/
      mod.rs                        (~40 LOC — re-exports)
      types.rs                      (~300 LOC — Guardrail enum, EscalationAction, sub-types)
      validation.rs                 (~400 LOC — guardrail validation logic)
      serde_impl.rs                 (~400 LOC — custom Deserialize for guardrail YAML)

    raw/
      mod.rs                        (~50 LOC — re-exports + pub fn parse)
      workflow.rs                   (~150 LOC — RawWorkflow)
      task.rs                       (~150 LOC — RawTask)
      action.rs                     (~200 LOC — RawAction, verb sub-types)
      mcp.rs                        (~120 LOC — RawMcpConfig, RawMcpServer)

    parser/
      mod.rs                        (~100 LOC — parse entry point + YAML utilities)
      actions.rs                    (~1,200 LOC — infer/exec/fetch/invoke/agent action parsing)
      config.rs                     (~1,200 LOC — task/workflow/mcp/context/include/retry parsing)
      structured_parse.rs           (~400 LOC — structured output YAML parsing)

    analyzed/
      mod.rs                        (~50 LOC — re-exports)
      ids.rs                        (~120 LOC — TaskId + interning)
      task.rs                       (~450 LOC — AnalyzedTask, AnalyzedTaskAction)
      workflow.rs                   (~250 LOC — AnalyzedWorkflow)

    analyzer/
      mod.rs                        (~60 LOC — pub fn analyze + AnalysisResult)
      errors.rs                     (~400 LOC — AnalysisError, AnalysisErrorKind, AnalysisWarning)
      transform.rs                  (~700 LOC — Raw → Analyzed transformation core)
      task_table.rs                 (~120 LOC — name→TaskId interning table)
      verb_analysis.rs              (~300 LOC — per-verb semantic checks)
      validation.rs                 (~300 LOC — structural validation rules)

    taint.rs                        (~500 LOC — TaintAnalyzer, TaintReport, TaintWarning)

    dag/
      mod.rs                        (~30 LOC — re-exports)
      cycle_detection.rs            (~180 LOC — Tarjan or DFS cycle detection)
      topological_sort.rs           (~120 LOC — Kahn's algorithm)

    json_schema/
      mod.rs                        (~30 LOC — re-exports)
      derive.rs                     (~250 LOC — json_to_schema, json_to_schema_strict)
      extract.rs                    (~200 LOC — extract_json state machine)
      cache.rs                      (~150 LOC — ValidatorCache LRU)

    completion.rs                   (~600 LOC — complete_* functions, suggest_did_you_mean)

    mention.rs                      (~550 LOC — Mention, parse_mentions, Chat-as-DAG refs)

    validate.rs                     (~200 LOC — validate_task_id)
```

### LOC summary by area

| Area | Estimated LOC | Files |
|---|---|---|
| lib.rs + error.rs | ~200 | 2 |
| source/ | ~470 | 3 |
| trust.rs | ~350 | 1 |
| types/ | ~3,590 | 19 |
| guardrails/ | ~1,140 | 3 |
| raw/ | ~670 | 4 |
| parser/ | ~2,900 | 4 |
| analyzed/ | ~870 | 4 |
| analyzer/ | ~1,880 | 6 |
| taint.rs | ~500 | 1 |
| dag/ | ~330 | 3 |
| json_schema/ | ~630 | 4 |
| completion.rs | ~600 | 1 |
| mention.rs | ~550 | 1 |
| validate.rs | ~200 | 1 |
| **Total src** | **~14,880** | **57** |

This is tight but under cap. The rewrite savings (elimination of unwraps,
dead code, verbose match arms, duplicated logic) should bring the actual
number closer to ~13,000. If not, the cap strategy in section 2 applies.

---

## 5. Dependencies

```toml
[dependencies]
nika-error   = { path = "../nika-error" }
nika-catalog = { path = "../nika-catalog" }

serde        = { workspace = true, features = ["derive"] }
serde_yaml   = { workspace = true }
serde_json   = { workspace = true }
miette       = { workspace = true }
regex        = { workspace = true }
blake3       = { workspace = true }
lru          = { workspace = true }

[features]
default = ["serde"]
serde = []  # types already derive Serialize/Deserialize unconditionally

[dev-dependencies]
insta      = { workspace = true }
proptest   = { workspace = true }
rstest     = { workspace = true }
serde_json = { workspace = true }
```

### Dependency justification

| Dep | Why | Alternative considered |
|---|---|---|
| serde + serde_yaml | YAML parsing is this crate's primary job | None — fundamental |
| serde_json | JSON Schema derivation + structured output validation | None — fundamental |
| miette | Error spans for diagnostics (used by AnalysisError) | None — workspace standard |
| regex | Mention parsing (@N, @last, @all, @N..M patterns) | Manual parser — regex is simpler and already workspace dep |
| blake3 | ValidatorCache key hashing (fast, collision-resistant) | sha2 — blake3 is faster |
| lru | ValidatorCache eviction | hashlink — lru is simpler |

### L0 constraint verification

- Zero I/O: no std::fs, no std::net, no tokio
- Zero async: no async fn, no Future, no tokio
- All deps are pure computation libraries

---

## 6. File structure

```
crates/nika-schema/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── error.rs
│   ├── trust.rs
│   ├── taint.rs
│   ├── completion.rs
│   ├── mention.rs
│   ├── validate.rs
│   ├── source/
│   │   ├── mod.rs
│   │   ├── span.rs
│   │   └── registry.rs
│   ├── types/
│   │   ├── mod.rs
│   │   ├── schema_version.rs
│   │   ├── budget.rs
│   │   ├── content.rs
│   │   ├── context.rs
│   │   ├── decompose.rs
│   │   ├── extract.rs
│   │   ├── include.rs
│   │   ├── limits.rs
│   │   ├── logging.rs
│   │   ├── orchestrate.rs
│   │   ├── output.rs
│   │   ├── record.rs
│   │   ├── routing.rs
│   │   ├── schedule.rs
│   │   ├── structured.rs
│   │   ├── templatable.rs
│   │   ├── agent_def.rs
│   │   ├── artifact.rs
│   │   └── completion.rs
│   ├── guardrails/
│   │   ├── mod.rs
│   │   ├── types.rs
│   │   ├── validation.rs
│   │   └── serde_impl.rs
│   ├── raw/
│   │   ├── mod.rs
│   │   ├── workflow.rs
│   │   ├── task.rs
│   │   ├── action.rs
│   │   └── mcp.rs
│   ├── parser/
│   │   ├── mod.rs
│   │   ├── actions.rs
│   │   ├── config.rs
│   │   └── structured_parse.rs
│   ├── analyzed/
│   │   ├── mod.rs
│   │   ├── ids.rs
│   │   ├── task.rs
│   │   └── workflow.rs
│   ├── analyzer/
│   │   ├── mod.rs
│   │   ├── errors.rs
│   │   ├── transform.rs
│   │   ├── task_table.rs
│   │   ├── verb_analysis.rs
│   │   └── validation.rs
│   ├── dag/
│   │   ├── mod.rs
│   │   ├── cycle_detection.rs
│   │   └── topological_sort.rs
│   └── json_schema/
│       ├── mod.rs
│       ├── derive.rs
│       ├── extract.rs
│       └── cache.rs
└── (no tests/ directory — all tests inline #[cfg(test)] mod tests)
```

---

## 7. Testing strategy

### 7.1 Test locations

| Test location | Scope | Count |
|---|---|---|
| `src/source/span.rs` #[cfg(test)] | Span arithmetic, FileId, ByteOffset | ~5 |
| `src/source/registry.rs` #[cfg(test)] | SourceRegistry add/get, line_col conversion | ~6 |
| `src/trust.rs` #[cfg(test)] | TrustLevel merge, ordering, Display, serde | ~8 |
| `src/types/*.rs` #[cfg(test)] | Each type module: serde roundtrip, defaults | ~25 |
| `src/guardrails/*.rs` #[cfg(test)] | Guardrail parsing, validation, escalation | ~12 |
| `src/raw/*.rs` #[cfg(test)] | Raw AST construction, serde | ~8 |
| `src/parser/*.rs` #[cfg(test)] | YAML parsing: valid workflows, error cases, edge cases | ~35 |
| `src/analyzed/*.rs` #[cfg(test)] | TaskId interning, AnalyzedWorkflow construction | ~8 |
| `src/analyzer/*.rs` #[cfg(test)] | Raw→Analyzed transformation, error detection | ~20 |
| `src/taint.rs` #[cfg(test)] | Taint propagation through DAG, warning generation | ~12 |
| `src/dag/*.rs` #[cfg(test)] | Cycle detection, topological sort, edge cases | ~10 |
| `src/json_schema/*.rs` #[cfg(test)] | json_to_schema, extract_json, cache | ~15 |
| `src/completion.rs` #[cfg(test)] | Completions, did-you-mean, Levenshtein | ~8 |
| `src/mention.rs` #[cfg(test)] | @N, @last, @all, @N..M, parallel markers | ~10 |
| `src/validate.rs` #[cfg(test)] | Task ID validation: valid, invalid, edge cases | ~10 |
| `src/error.rs` #[cfg(test)] | SchemaError Display, NikaErrorCode impl | ~5 |
| **Total** | | **~197** |

### 7.2 Property testing (proptest) — MANDATORY (Gate 6)

This crate is a **parser** and handles **structured output extraction** — two
security-sensitive areas. Proptest is required, not optional.

| Property test | Strategy | Cases |
|---|---|---|
| Parser roundtrip: arbitrary YAML strings never panic | `proptest::string::arbitrary()` | 5,000 |
| extract_json: arbitrary strings → None or valid JSON | `proptest::string::arbitrary()` | 5,000 |
| json_to_schema: arbitrary JSON → valid JSON Schema | `proptest::arbitrary::<Value>()` | 2,000 |
| TrustLevel::merge is commutative and associative | enum strategy | 1,000 |
| validate_task_id: valid IDs pass, invalid reject | `"[a-z_][a-z0-9_]*"` + adversarial | 2,000 |
| Mention parsing: well-formed @refs parse correctly | custom strategy | 1,000 |
| Cycle detection: acyclic graphs → Ok, cyclic → Err | graph strategy | 1,000 |
| Topological sort: result respects all edges | graph strategy | 1,000 |
| Budget: YAML bomb inputs stay within limits | size-bounded YAML strategy | 500 |

### 7.3 Snapshot testing (insta)

- Parser golden tests: ~20 reference `.nika.yaml` files parsed to snapshot
- Analyzer golden tests: ~10 workflows analyzed to snapshot
- Error message goldens: every `SchemaError` variant has Display snapshot
- Taint report goldens: ~5 workflows with known trust propagation

### 7.4 Parity with legacy (Gate 10)

Golden tests comparing diamond output against legacy engine:

```rust
#[test]
fn parity_simple_infer_workflow() {
    // Parse with diamond, compare AnalyzedWorkflow structure
    // against known legacy output for same YAML input
}
```

Minimum 5 parity tests covering:
1. Simple single-task infer workflow
2. Multi-task DAG with dependencies
3. Agent workflow with guardrails
4. Workflow with structured output spec
5. Workflow with for_each and include

---

## 8. Relationship to nika-catalog

`nika-schema` depends on `nika-catalog` for **validation during analysis**:

| Usage | nika-catalog API called | Where in nika-schema |
|---|---|---|
| Validate provider name in infer action | `find_provider(name)` | `analyzer/verb_analysis.rs` |
| Validate builtin tool name in invoke action | `is_known_builtin(name)` | `analyzer/verb_analysis.rs` |
| Validate transform name in pipe expressions | `is_known_transform(name)` | `analyzer/validation.rs` |
| LSP completion: provider names | `all_providers()` | `completion.rs` |
| LSP completion: model names | `model_capabilities()` | `completion.rs` |
| LSP completion: transform names | `all_transforms()` | `completion.rs` |
| Did-you-mean for unknown provider | `all_providers()` | `analyzer/errors.rs` (suggestion) |

The dependency is **read-only lookups**. `nika-schema` never mutates catalog
data. Catalog returns `Option`, and the analyzer decides whether `None` is an
error or a warning (unknown providers are warnings, not errors — user may have
custom providers).

---

## 9. What was moved (boundary decisions)

### Moved INTO nika-schema (from binding on main)

| File on main | Why it moves | Diamond location |
|---|---|---|
| `binding/mention.rs` (851 LOC) | Mention parsing is schema-level concern (Chat-as-DAG references create task dependencies). Binding layer should not own DAG topology. | `src/mention.rs` (~550 LOC after rewrite) |
| `binding/validate.rs` (355 LOC) | `validate_task_id()` validates task names during schema analysis. It is called by the analyzer, not by binding resolution. | `src/validate.rs` (~200 LOC after rewrite) |

### Moved OUT of nika-schema (vs legacy ast/)

| Legacy location | Where it goes in diamond | Why |
|---|---|---|
| `ast/analyzer/analyze/tests.rs` (3,099 LOC) | Inline `#[cfg(test)] mod tests` in each module | Legacy dedicated test file violates diamond convention |
| `ast/mod.rs` re-exports (120 LOC) | `src/lib.rs` (~80 LOC) | Flattened — mod.rs was mostly re-exports |

### NOT in nika-schema (clarifications)

| Item | Where it lives | Why not nika-schema |
|---|---|---|
| Template engine (`{{with.x \| y}}`) | `nika-binding` (L0) | Template resolution is binding concern, not schema |
| 63 transforms (apply, dispatch) | `nika-binding` (L0) | Transforms are binding operations on resolved values |
| BindingStore trait | `nika-kernel` (L0.5) | Kernel trait, not schema type |
| Lowering (Analyzed → runtime) | `nika-runtime` (L3) | Depends on runtime capabilities |

---

## 10. Gate exemptions

- **Gate 7 (Benchmarks)**: **NOT exempt.** The parser is a hot path — every
  workflow execution starts with parsing. `benches/parser_bench.rs` is
  required with criterion, measuring `parse()` and `parse_analyzed()` on
  5 reference workflows of increasing complexity. Target: <1ms for a
  10-task workflow.

- **Gate 9 (Canary E2E)**: Exempt. No runtime exists yet in nika-diamond.
  Canary test lands when `nika-runtime` is admitted (Phase 4). Exemption
  documented: nika-schema is exercised indirectly via parser+analyzer
  golden tests which serve as functional E2E.

---

## 11. Risks and mitigations

| Risk | Likelihood | Mitigation |
|---|---|---|
| LOC exceeds 15k after rewrite | Medium | Cap strategy in section 2 — move completion/mention/routing out |
| Parser module files approach 1,500 LOC | Medium | Further split `parser/actions.rs` into `parser/actions_infer.rs` + `parser/actions_agent.rs` |
| Guardrails serde complexity | Low | Custom Deserialize already isolated in `guardrails/serde_impl.rs` |
| Circular imports between analyzer and types | Low | Solved by 1-crate design — modules, not crates |
| extract_json security (adversarial input) | Medium | Proptest 5,000 cases + fuzzing target in `fuzz/` (Phase 5) |

---

## 12. Audit trail

| Date | Author | Change |
|---|---|---|
| 2026-04-13 | Phase 1 S4 | Initial spec. 1-crate design locked (no ast/analyze split). |

