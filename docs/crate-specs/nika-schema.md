# Crate spec — `nika-schema`

| | |
|---|---|
| Status | Phase 1 — Step 4 of `nika-core` split |
| Layer | L0 (PURE, zero I/O, zero async) |
| Design | **Monolithic** — AST + parser + analyzer + validator + DAG in 1 crate (split rejected: circular deps) |
| LOC budget | ≤15,000 src (target ~13,000, alarm at 14,000, hard cap 15,000) |
| File cap | ≤1,500 LOC each |
| Function cap | ≤100 lines each |
| Source on `brouillon` (legacy reference · read via `git show brouillon:…`) | `tools/nika-core/src/ast/` (22,544 LOC), `tools/nika-core/src/schema/` (400 LOC), `tools/nika-core/src/source/` (724 LOC), `tools/nika-core/src/trust.rs` (560 LOC), `tools/nika-core/src/binding/mention.rs` (851 LOC), `tools/nika-core/src/binding/validate.rs` (355 LOC) |
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
      actions.rs                    (~1,200 LOC — infer/exec/invoke/agent action parsing · fetch is the nika:fetch builtin under invoke)
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

> **Superseded 2026-06-16** · per ADR-001 (CRAFT · orphan rebuild) there is NO
> legacy parser to compare against. Gate 10 is satisfied by **spec-conformance
> parity** instead: `conformance_{core,deep}.rs` + `research_conformance.rs` pin
> the parser+analyzer output against the `nika-spec` contract. The
> legacy-comparison sketch below is retained as historical intent only.

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

- **Gate 7 (Benchmarks)**: **NOT exempt** — the parser is a hot path (every
  workflow execution starts by parsing). ✅ **SHIPPED 2026-06-16**:
  `benches/parse_bench.rs` (criterion) measures `parse()` on a small 2-task
  workflow + a generated 200-task DAG, and `analyze()` (the super-linear
  passes) on the 200-task DAG. Measured (CPU): parse/small **13.4µs** ·
  parse/large(200) **550µs** · analyze/large(200) **222µs** — the 200-task DAG
  is 0.55ms, well under the <1ms-for-a-10-task-workflow target.

- **Gate 9 (Canary E2E)**: Exempt. No runtime exists yet in the Diamond workspace.
  Canary test lands when `nika-runtime` is admitted (Phase 4). Exemption
  documented: nika-schema is exercised indirectly via parser+analyzer
  golden tests which serve as functional E2E.

- **Gate 5 (Mutation ≥90%)**: **BUDGET mode** — ADR-003 Rule-2 *documented
  exemption*, not a skip. The whole-crate floor (1601 mutants · `--jobs 1
  --timeout 20` · the 8 `check/analysis.rs` graph-fixpoint divergers excluded
  via `.cargo/mutants.toml` so they cannot OOM-reboot the host) leaves only
  survivors that are each a diverge-by-design **timeout** or a proven
  **equivalent** mutant. `scripts/ci/check-mutation-floor.sh` enforces
  `survivors ≤ N`:

  <!-- GATE5-EXEMPT: 300 -->

  - **269 timeout-divergers** — a mutation that turns a bounded loop/stride
    into a non-terminating one. E.g. `expression/template.rs`'s byte-scanner
    `+= → -=` / `*=` reverse or explode the index; `analyzer/dag.rs` and
    `check/certificate.rs` accumulators `+= → *=`. The 20 s timeout IS the
    kill signal (detected-by-divergence). Measured by the floor-v2 run
    (2026-06-18); budget is set to 300 (not 290) to absorb the wall-clock
    variance inherent in timing out non-terminating mutants.
  - **21 equivalent mutants** (enumerated · each re-verified by scoped
    `cargo mutants`):
    - `check/reach.rs` ×14 — disjoint-bit `| ≡ ^` on the status flags
      (`S_SUCCESS|S_FAILURE|S_SKIPPED|S_CANCELLED` = bits 1/2/4/8, so XOR and
      OR coincide), the unreachable `parse_gate → None` arm (the analyzer
      rejects bad `${{…}}` islands upstream — `check_single_island`), and the
      default-gate runnable masks neutralised by the always-set `S_CANCELLED`
      bit + the runnable-always-true induction (roots are vacuously runnable;
      every task therefore carries success or skipped).
    - `expression/parser.rs` ×5 — the `peek()` defensive fallback
      (unreachable: `advance()` clamps `pos < len`, so `get(pos)` is always
      `Some`) and the `advance` bound (an over-advanced `pos` is clamped back
      to the `Eof` token by `peek()`, unobservable).
    - `expression/template.rs` ×1 — the `scan_templates` loop bound `<` → `<=`
      (one extra `bytes[len..]` empty-slice iteration, no effect).
    - `check/schema_lint.rs` ×1 — `json_matches_type` `|| → &&` (the
      `f64`-is-whole disjunct already subsumes `is_i64() || is_u64()`).

  **Killable survivors were CLOSED, not budgeted.** Rounds 1-7 added ~190
  tests across the analyzer/check collection + lint logic, the `read_dag`
  cap/pinch boundaries, the default-gate runnable path, and the
  expression-parser offset/depth/byte-scanner. Two mutants the earlier §12
  budget had mislabelled "equivalent-by-invariant" (`reach.rs` runnable mask
  `| → &` and `!= → ==`) were proven *killable* and pinned by
  `default_gate_with_deps_keeps_downstream_success_reachable`.

---

## 11. Risks and mitigations

| Risk | Likelihood | Mitigation |
|---|---|---|
| LOC exceeds 15k after rewrite | Medium | Cap strategy in section 2 — move completion/mention/routing out |
| Parser module files approach 1,500 LOC | Medium | Further split `parser/actions.rs` into `parser/actions_infer.rs` + `parser/actions_agent.rs` |
| Guardrails serde complexity | Low | Custom Deserialize already isolated in `guardrails/serde_impl.rs` |
| Circular imports between analyzer and types | Low | Solved by 1-crate design — modules, not crates |
| extract_json security (adversarial input) | Medium | Proptest 5,000 cases + fuzzing target in `fuzz/` (Phase 5) |
| **Untrusted-input DoS · oversized workflow** | **High** (once `nika serve` is wired) | The `CharToByte::new` guard (`parser/mod.rs`) rejects > `u32::MAX` bytes, but that is a SPAN-CORRECTNESS bound (4 GB), not a DoS bound — `marked-yaml` allocates the entire node tree up-front. **Pre-admission gate**: lower to a workflow-realistic byte cap (≈16 MB) before the parser sees untrusted input. |
| **Untrusted-input DoS · deep YAML nesting** | **High** (`nika serve`) | `value::node_to_json` recurses on `Sequence`/`Mapping` with no depth bound → a deeply-nested value overflows the stack. **Pre-admission gate**: enforce a nesting-depth cap (the parse-time analog of the spec's run-recursion depth cap in `08-out-of-scope.md` §Depth cap, which is a distinct *runtime* guard). |
| **Untrusted-input DoS · unbounded task count** | Medium (`nika serve`) | `parser::tasks::parse_tasks` iterates the `tasks:` sequence with no count cap (output `Vec` + downstream DAG scale with it). **Pre-admission gate**: add a `MAX_TASKS` bound. |
| Billion-laughs (YAML anchor/alias expansion) | — | **Already mitigated** by the lib choice: `marked-yaml` 0.8 does not expand anchors/aliases (config-subset YAML). No action needed; documented so the mitigation isn't lost on a future YAML-lib swap. |
| **Untrusted-input DoS · `when:`-gate literal scan** | **Fixed 2026-06-18** | A `when:` gate `in [...]` list of distinct non-status strings drove `check/reach.rs::collect_bad_literals` into O(n²) `Vec::contains` dedup — 40k literals ≈ 3 s CPU on a 2-task workflow, bypassing every cap (the list is depth-1 so `MAX_DEPTH` never fires; the gate lives on one task so `MAX_TASKS`/`MAX_GATE_REFS` do not bound it). **Fixed** (`7312519f0`): `BTreeSet` seen-guard → O(n log n), same findings; regression test `huge_in_list_of_distinct_literals_is_not_quadratic`. Found by an adversarial review refuter + reproduced empirically. |

> **Pre-admission security gate (untrusted-input resource bounds).** Before
> `nika-schema` is wired behind `nika serve` (untrusted workflow input), the
> three caps above (byte-size · nesting-depth · task-count) MUST be enforced at
> the parse boundary, each with a regression test. They are NOT Gate-12 blockers
> for L2/L3 internal-only use, but they ARE blockers for the public-serve
> surface. Tracked here so the requirement is explicit at admission (the limit
> *values* are a design decision deferred to that point). The inline
> `SECURITY NOTE` in `parser/mod.rs` `CharToByte::new` cross-links this row.

> **Gate-11 admission security pass · 2026-06-16 (rust-security swarm).** The
> three pre-parse caps above are **SHIPPED** (not deferred): `MAX_SOURCE_BYTES`
> 4 MiB + `MAX_INDENT_BYTES` + `value::MAX_VALUE_DEPTH` 128 + `tasks::MAX_TASKS`
> 10k, all pre-`marked-yaml`, all loud — empirically re-confirmed by the swarm
> (`[`×256 / `{a:`×256 → loud `Err`, no overflow; the expression
> postfix/index/wide-chain depth caps hold). The swarm found **one P0** the
> pre-parse caps did NOT cover, now **FIXED**:
>
> - **P0 (FIXED) · `check/flow.rs` IFC taint-trace O(n²).** A per-task
>   `TaintTrace` carried its hop chain as a `Vec<String>` cloned 2-3× per task →
>   O(n²) time + memory. A 0.89 MiB *valid* workflow (one secret + an ordinary
>   output-reference chain) → 9.4 s + **5.2 GB RSS** (instant OOM), reachable via
>   `nika check` AND the agent-facing MCP `nika_check` builtin. **Fix** · the hop
>   chain is now an `Arc` cons-list (`Hop`), so `via()`/`clone()` are O(1) and
>   `analyze_flow` is O(n) — the reachable hops + leak semantics unchanged (596
>   prior tests green). Regression-guarded by
>   `flow::tests::long_secret_chain_stays_linear_not_quadratic` (3000-task chain).
>
> **Fast-follow ratchets — FIXED same arc 2026-06-16 (the swarm classified both ship-after):**
> - **F2 (P1) · `check/reach.rs` gate enumeration → FIXED.** `MAX_GATE_REFS`=6
>   bounded the 4096 enumerations' COUNT but not the SIZE of a `when:` gate's
>   `in [...]` list re-scanned each pass → O(4096×list) (~0.88 s for one 3.6 MiB
>   gate). Fix · a `max_list_len` guard widens any gate past `MAX_GATE_LIST_ITEMS`
>   (256) to satisfiable∧falsifiable — the same sound back-off as `MAX_GATE_REFS`
>   (a status list has ≤4 meaningful values; a larger one is adversarial padding).
>   Regression `reach::tests::oversized_gate_in_list_is_bounded_not_quadratic`.
> - **P2 · `flow.rs` secrets membership → FIXED.** The per-task `declared.contains`
>   linear scan (O(n·S)) is now a `BTreeSet` lookup (O(n·log S) · `BTreeSet` not
>   `HashSet` — the latter is a disallowed type here, determinism-pinned). Bounds
>   the lower-constant O(n²)-class scan an adversarial many-secrets workflow could hit.

---

## 11bis. `nika check` — the static pre-flight (shipped 2026-06-11)

The `check` module composes `analyze()` with the static reports that make
« audit before it runs » concrete (spec `07-conformance.md` §nika check) ·
the wave **plan** · the **cost ceiling** · the **secret-leak + egress** IFC
scan · the **capability-escape** scan against a declared `permits:` block ·
**capability inference** (`infer_permits()` — synthesize the tightest
`permits:` block, ADR-092 #2). Runnable today via
`cargo run -p nika-schema --example check -- [--infer-permits] wf.yaml`;
the polished CLI ships with `nika-cli` (step 19).

Three hardening passes (2026-06-11) · a 3-angle adversarial review (net/fs
literal escapes · secret island-scoping via `expr_refs` · provider-scoped +
for_each-fan-out cost), then the ADR-092 program-analysis ladder · the
`flow.rs` IFC engine (Denning-lattice taint, one topological least-fixpoint
pass over the DAG = the `FlowFacts` IR) replaced the heuristic secret scan —
the `with:`-aliased false negative is FIXED (transitive taint with a full
`secrets.x → with.t → tasks.a.output` trace), and secrets reaching
`outputs:` are reported as **egress** — then a second 3-lens swarm on the
inference work itself · **`on_finally` cleanup verbs are now first-class**
across all three walkers (IFC sinks · escape checks · inference — a cleanup
ALWAYS runs, so a blind spot there broke every run) · builtin effect
classification is the shared `BuiltinEffect` table covering
`read`/`write`/`edit` (read+write)/`grep` (recursive → `<path>/**`)/`fetch`/
webhook-`notify` (`nika:glob` deliberately excluded — its arg is itself a
glob, inclusion is undecidable statically) · IPv6 bracket hosts · YAML
escaping in the rendered block · strict example flags (a typo'd `--flag`
exits 2, never a silent plain check). `infer_permits` shares ALL extractors
(`builtin_effect` · `static_program` · `literal_arg` · `url_host`) with the
escape checker, so inference and verification cannot drift; the inferred
block round-trips through the parser and re-checks with zero escapes —
tested for `exec: false`/`true`/programs, agent tool globs, on_finally
effects, and quote-bearing paths.

### Known limitations (honest · no silent gaps)

| Gap | Why deferred | Where it's still caught |
|---|---|---|
| **fs/net escape via a shell `exec` string** (`curl https://evil` inside `command:`) | the shell command is the runner blocklist's domain, not a structured arg; fine-grained net/fs inside `/bin/sh -c` is inherently runtime | `exec` is gated by `permits.exec` + the s7 runner blocklist; the structured builtins (`nika:fetch`/`read`/`write`) ARE checked |
| **agent tool-call args** | the model picks tool args at runtime — no static surface | runtime `NIKA-SEC-004` when the agent dispatches; agent `tools:` globs ARE checked vs `permits.tools` |
| **cost input-token term** | input cost is prompt-dependent (interpolates task outputs) → statically unbounded | the figure is documented as an OUTPUT-token ceiling; `max_tokens` bounds output only |
| **dynamic effects in inference** (`${{ }}`-built path/host/program) | not statically pinnable — sound-by-honesty: the category widens (e.g. `exec: true`) and a review note is emitted, never a silently under-permissive block | runtime `NIKA-SEC-004` enforces the declared boundary on resolved values |
| **`infer`/`agent` outputs are NOT tainted** | trust-model carve-out (ADR-092): the provider is operator-chosen; a secret in a prompt is provider-bound by design, and a model response is not a verbatim echo | prompts are still masked in the engine's own logs/traces |
| **`$ref` siblings are NOT linted** (a `required`/`type` defect beside a `$ref` in the same schema node) | draft 2020-12 evaluates `$ref` siblings, but the static linter has no `$ref` resolver — descending siblings without resolving the ref could emit FALSE claims, so the whole node goes opaque (sound-by-honesty) | the runtime validator (nika-verb-infer compiles the full schema) evaluates siblings per the draft and fails the task at dispatch |

### Next — completing the `nika check` story

1. **Runtime `NIKA-SEC-004` enforcement** (the dynamic half) — the engine/
   runner enforces `permits:` DURING execution for the cases the static scan
   marks dynamic (a `${{ }}`-built host/path, an agent tool dispatch). This
   is the L3 runtime's job, sequenced with the engine crate.
2. **`nika-cli check` subcommand** — the polished CLI surface (colour, exit
   codes, `--providers` parity flag, `--infer-permits`) over this module,
   at step 19.
3. **Ladder #5-#9** — symbolic cost intervals · SMT reachability ·
   termination certificate · incremental IR · differential conformance,
   per the ADR-092 sequence (#4 dataflow schema typing SHIPPED — deep
   `tasks.X.output.<path>` references resolve against X's `schema:`
   [properties/items/anyOf descent · explicit `additionalProperties:
   true` = opaque] or `output:` binding names, across prompts · commands
   · args · `when:` · `with:` · `for_each` · envelope `outputs:` ·
   `on_finally` — a typo'd field is a finding BEFORE any token is spent).

## 11ter. Admission · 12-gate ledger (ADR-003)

> **Status 2026-06-16** · **11/12 gates green** — the Gate-11 admission swarm
> completed (found + **FIXED** a P0 IFC-taint O(n²) DoS · see Gate 11). The one
> remaining gate is **Gate 5 (mutation)** — a quiet-window FLOOR run (see
> strategy below). The crate admits with a single `wip`-array edit once Gate 5
> lands. `nika-schema` is the **last L0 crate** — its admission closes the L0
> foundation (the 1.0 launch floor).

| # Gate | Status |
|---|---|
| 1 SPEC | ✅ this file (§1 purpose · §2 layer/LOC budget · §3 public API surface · §4 module map). |
| 2 TDD | ✅ **596 lib tests** (0 failed · 2026-06-16) RED→GREEN + the `tests/` integration suites (`examples_valid` · `conformance_{core,deep}` · `research_conformance` · `static_binding_paths` · `lints_one_obvious_way`). |
| 3 IMPL | ✅ parser (`marked-yaml` tree → typed lowering) + analyzer (DAG order · dataflow schema-typing · IFC taint via a Denning lattice) + `check` (plan/cost/secrets/permits · `--infer-permits` · CEL cel-subset/0.1). |
| 4 CLIPPY 0 | ✅ `cargo clippy --all-targets -- -D warnings` clean (2026-06-16). |
| 5 MUTATION ≥90% | ⏳ **PENDING the quiet-window FLOOR run** — `nika-schema` lists **1711 mutants** (the largest Diamond crate; the full parser+analyzer+check surface). The kill engine is the 596 lib tests + the proptest/metamorphic batteries (Gate 6). **Strategy** · FLOOR mode (`caught/viable ≥ 90`, no exemption claimed) via `bash scripts/ci/check-mutation-floor.sh nika-schema`. **Why deferred** · cargo-mutants is contention-sensitive — concurrent cargo load inflates timeouts into false survivors (cf the nika-builtin 86.8%→91.3% lesson), and a ~1711-mutant run (multi-hour) is only valid run ALONE (no concurrent loops). **Fallback** · if the long tail carries hard-to-kill *equivalent* mutants (span-offset arithmetic · the `marked-yaml` recursion guards reachable only by the DoS probes), a documented `GATE5-EXEMPT` budget per the nika-builtin/nika-screen precedent — but FLOOR is the default. |
| 6 PROPERTY | ✅ proptest invariants — `check/metamorphic.rs` (metamorphic relations) · `check/infer_permits.rs` (round-trip-clean: an emitted `permits:` block re-parses to the same grants for arbitrary literal paths) · `suggest.rs` (damerau-levenshtein pseudometric · did-you-mean never-invents/never-returns-exact) · `expression/parser.rs` (binding-path soundness) · IFC taint transitivity + fanout-extends-work-never-span. |
| 7 BENCH | ✅ `benches/parse_bench.rs` (criterion · 2026-06-16) · **parse/small_2_tasks 13.4µs** · **parse/large_200_tasks 550µs** · **analyze/large_200_tasks 222µs** — all well under the §10 target (<1ms for a 10-task workflow; the 200-task DAG is 0.55ms). |
| 8 DOCS | ✅ `cargo doc --no-deps` 0 warnings (2026-06-16). |
| 9 CANARY | ✅ **EXEMPT** — the parser's functional E2E IS its corpus: `examples_valid.rs` parses EVERY `nika-spec` example + the `conformance_{core,deep}` + `research_conformance` suites. Parsing is itself the parser's end-to-end; a runtime-level canary (parse → run) belongs to `nika-runtime`'s admission, not the parser's. (The §10 "no runtime yet" wording is superseded — the runtime now exists — but the golden-corpus exemption stands and is stronger.) |
| 10 PARITY | ✅ **spec-conformance parity** — `conformance_{core,deep}.rs` + `research_conformance.rs` pin the parser+analyzer output against the `nika-spec` contract. NOT legacy parity: per ADR-001 (CRAFT, orphan rebuild) there is no legacy parser to round-trip against — the §7.4 "compare against legacy engine" framing is superseded by spec-conformance. |
| 11 REVIEW | ✅ **final admission swarm 2026-06-16** (rust-security · untrusted-input/DoS/panic-freedom) atop the 2026-06-11 inference swarm + ADR-092 ladder reviews. Every probed surface SOUND (zero-unwrap · YAML nesting caps · billion-laughs · pre-parse byte/indent/task/depth guards · expression depth caps · the IFC declassification correctness · the bounded read_dag/certify/cost passes) EXCEPT **one P0 found + FIXED**: `check/flow.rs` carried per-task `TaintTrace` hops as a cloned `Vec<String>` → O(n²) (a 0.89 MiB valid workflow OOM'd at 5.2 GB · reachable via `nika check` + the agent MCP `nika_check`). Fix · `Arc` cons-list → `via()`/`clone()` O(1) → `analyze_flow` O(n); pinned by `long_secret_chain_stays_linear_not_quadratic`. The 2 non-blocker fast-follows are also FIXED same arc (§11): F2 P1 gate `in`-list back-off, P2 secrets `BTreeSet`. |
| 12 ATOMIC | 1 admission = 1 commit (the single edit removing `nika-schema` from `workspace.metadata.diamond.wip`). |

---

## 12. Audit trail

| Date | Author | Change |
|---|---|---|
| 2026-04-13 | Phase 1 S4 | Initial spec. 1-crate design locked (no ast/analyze split). |
| 2026-06-11 | nika check arc | `permits:` parser + `check` module (plan/cost/secrets/permits) + CEL cel-subset/0.1 (ternary·has·string tests) + runnable example. 3-angle review hardened (net/fs literal escapes · secret island-scoping · provider-scoped + for_each cost). §11bis added. |
| 2026-06-11 | ADR-092 ladder #1-#3 | `flow.rs` IFC taint engine (Denning lattice · topological least fixpoint · `FlowFacts` IR · full taint traces) replaces the heuristic secret scan — `with:`-aliased false negative FIXED + `outputs:` egress sink added. `infer_permits.rs` capability inference (`--infer-permits` · sound-by-honesty notes · round-trip-clean property). `static_program` shared extractor (dynamic-argv[0] false positive fixed both sides). §11bis rewritten. |
| 2026-06-11 | inference review swarm | 3-lens swarm (11 findings · 5 PROVEN by probe): `on_finally` cleanup verbs now walked by ALL THREE walkers (flow sinks · escapes · inference — were invisible, breaking every run of a pasted block). Shared `BuiltinEffect` table extends coverage to `edit`/`grep`/webhook-`notify` (glob excluded · undecidable). IPv6 bracket hosts. YAML escaping (`yaml_quote`). Empty block renders `permits: {}`. Strict example flags (typo → exit 2). Round-trip property tested across exec variants + agent globs + on_finally + quoted paths. |
| 2026-06-11 | ADR-092 ladder #4 | `schema_typing.rs` dataflow schema typing — deep `tasks.X.output.<path>` refs resolved against X's `schema:` (JSON Schema descent: properties · transparent array `items` · anyOf/oneOf/allOf any-branch-admits · `$ref` opaque · explicit `additionalProperties: true` opaque) or `output:` jq binding names (rebind precedence). Surface = prompts/system · command fragments/env/stdin · invoke args · `when:` · `with:` · `for_each` · envelope `outputs:` · `on_finally`. `expression::refs` refactored to ONE `walk_chains` core feeding `expr_refs` + new `task_output_paths` (no extractor drift). `CheckReport.schema_findings` + example TYPES section. |
| 2026-06-11 | ADR-092 ladder #5 | `cost.rs` structural cost interval — `retry.max_attempts` worst-path multiplier (ignoring it silently UNDERCOUNTED the ceiling), `when:`-gated tasks zero the cheapest path, `min_path_total_usd` alongside the worst-path Σ. The envelope is [cheapest structural path, worst structural path] at the DECLARED per-call budget — emission variance below `max_tokens` + the input-token side stay out per the documented output-ceiling convention. Example renders the range + ×N retries/(when:-gated) annotations. |
| 2026-06-11 | agent intelligence layer | `suggest.rs` deterministic did-you-mean (Damerau-Levenshtein · rustc threshold `max(len/3,1)` · lexicographic tie-break · silence over wrong guesses). `tools.rs` — unknown `nika:` builtins caught statically against the SAME `all_builtins()` the codegen enum reads (`nika:raed` → `nika:read`). `schema_lint.rs` — authored `schema:` defects (required∉properties = every-output-unsatisfiable · bad `type` name · empty `enum` · composite descent · `$ref` opaque). Suggestions threaded into schema-typing + binding details; `CapabilityEscape.fix` carries the machine-applicable grant (withheld for phantom builtins — the rename owns the repair). Serialize on the whole report family + `--json` example mode = the agent repair-loop surface. E2E: 6-finding workflow converges to CLEAN in 2 rounds applying emitted fixes verbatim. |
| 2026-06-11 | review fold + hints | 2-lens swarm findings folded: fix idiom UNIFIED (one `add "X" to permits.<path>` shape everywhere — the exec-denied prose directive broke the verbatim-apply contract), `repair_loop_converges_to_clean` automated test (the convergence claim is now CI-pinned), `$ref`-siblings gap documented in Known Limitations, OSA distance variant pinned ("ca"→"abc"=3). `hints.rs` — the deterministic « ameliorateur » (advisory · never fail `is_clean`): cost (no token bound → hard ceiling unlock) · dead-spend (unconsumed pure infer) · typing (deeply-referenced un-schema'd output → declare `schema:` unlock) · permits (effectful + no boundary → `--infer-permits` pointer). 412 lib tests. |
| 2026-06-11 | socratic round: corrector + maximal report | `suggest` promoted to crate level; **analyzer did-you-mean** — `UnresolvedNamespaceRef` + `UnknownDependency` carry `suggestion` (each namespace suggests within ITS OWN declared set · `vars.topci → vars.topic` · typo'd ROOTS suggested too `vrs → vars` · the #1 agent error class now self-repairs). **`check()` is INFALLIBLE** (rustc model: maximal info per run) — conformance violations land IN the report (`conformance: Vec<ConformanceViolation>` with spec codes); every DAG-independent analysis (cost/tools/schema-lint/escapes/typing/hints) still runs; only `waves` + IFC need the topo order. ONE round-trip repairs conformance AND findings. `report_version: 1` JSON contract field; determinism CI-pinned (two runs → byte-identical JSON, catches stray HashMaps). 417 lib tests. |
| 2026-06-11 | strictness hint (hint #5) | structured-output DETERMINISM: an object schema declaring `properties` without `additionalProperties: false` admits undeclared keys — the validated shape varies across providers/runs. New `strictness` hint class names the close-it fix (the recipe provider-native strict modes require); one hint per task, full composite descent (`items`/`anyOf`/…), `$ref` opaque. 419 lib tests. |
| 2026-06-11 | DoS hardening (stack-safety) | PROVEN crash class closed: unbounded block nesting overflowed the stack (~3000 levels · marked-yaml block parse recursion · the exact gap the parser/mod.rs security note had pre-flagged). TWO loud deterministic layers: (1) pre-parse indent guard (`MAX_INDENT_BYTES` 1024 spaces ≈ 512 levels · O(n) early-exit scan · protects marked-yaml itself — a 450 MB / 15k-level probe now rejects instantly) · (2) `value::MAX_VALUE_DEPTH` 128 at the YAML→`serde_json::Value` conversion seam (`json_value(cx, node)` fallible, span-carrying — ONE seam bounds every downstream walker: schema lint · dataflow typing · strictness · runtime `compile_schema` · recursive `Drop`). Silently truncating was rejected — the runtime must never receive a DIFFERENT value than authored. 421 lib tests. |
| 2026-06-11 | resource-bound trio complete + proptests | The untrusted-input guard SET is now closed (all pre-marked-yaml, all loud): `MAX_SOURCE_BYTES` 4 MiB (memory · tree allocated up-front) + `tasks::MAX_TASKS` 10k (analyzer DAG passes super-linear) join the indent + value-depth caps. Security note rewritten — what remains pre-`nika serve` is POLICY (per-tenant quota · wall-clock timeout), not safety. Proptests bank Gate-6: `damerau_levenshtein` proven a pseudometric (identity · symmetry · triangle · length-bound over arbitrary unicode) · `did_you_mean` proven to never invent/never-return-exact · the `infer_permits` YAML-escaping round-trip proven for ARBITRARY literal paths (quotes/backslashes/tabs · the security-critical « pasteable block that admits the path » property, now property-tested not just example-tested). 441 lib tests. |
| 2026-06-11 | check example polish (the visual seam) | The example graduates to a multi-file binary (`examples/check/{main,theme,render}.rs`) per the CLI presentation canon + the nika-cli display contract: ONE colour seam (`theme.rs` · semantic-only green/red/yellow/cyan/dim · auto-resetting · pure `resolve_colour` · precedence `--color` → `NO_COLOR` → `CLICOLOR_FORCE` → TTY · zero raw ANSI elsewhere) · the contract glyph grammar (`○` will-run · `⊘` when:-gated · `✔/✖` verdicts · `↻×N` retries · `↳` machine fix · `◆` banner · glyphs survive colour loss, meaning never in colour alone) · the PLAN renders as a DAG in wave lanes (per-task verb · deps `←` · retry/for_each/when: annotations · max-parallelism header). NO spinner — the static check is instant; animation belongs to the run surface (semantic-not-decorative). `--json` stays byte-identical and never coloured. Section keywords grep-stable. |
| 2026-06-11 | canonical theme · verb-colour logic | `theme.rs` becomes the SINGLE SOURCE OF TRUTH: a closed `Role` enum (status axis + verb axis) → ONE `Role::sgr` table (pure, total, pinned by `sgr_table_is_canonical` — the palette cannot drift). The verb axis carries ARCHITECTURAL logic — colour FAMILY = the governing gate: **magenta = COST-bearing** (`infer`/`agent` spend tokens) · **blue = PERMITS-bearing** (`exec`/`invoke` touch the world) · brightness = blast radius (`agent`>`infer`, `exec`>`invoke`). The DAG plan paints each verb in its gate colour + a self-documenting legend, so the structure reads at a glance AND ties to the COST/PERMITS sections that scrutinize it. Grounded in the Clack log-level model + clig.dev (context7). 5 theme-determinism tests (SGR table pinned · family logic · resolution precedence · paint idempotence). a11y verified: 0 ANSI under `NO_COLOR`, meaning fully carried by glyph + word. The reference seam `nika-cli` (L4) + `nika-vscode` derive from. |
| 2026-06-11 | theme owns glyphs too · first-class ASCII | The theme is now the source of truth for ALL presentation (colour AND glyph): a `Glyph` enum with BOTH a unicode and an ASCII rendering, selected once on `Theme` (the contract's «ASCII is a first-class theme, not a degraded mode» · CI · `TERM=dumb` · screen readers). `--ascii` flag + auto under `TERM=dumb`. render reads `t.glyph(Glyph::X)` (the `G_*` consts retired) + a themed rule. Both glyph sets pinned by test. The verb-family colour logic + the legend are unchanged; the ASCII path swaps the glyph grammar + rule (the contract's ascii scope). |
| 2026-06-11 | snapshots + themed chrome typography | Review of my own ascii-purity claim found the shell probe BROKEN (BSD grep + `LC_ALL=C` silently misses high bytes). Fixed structurally: chrome typography joins the theme (`·–—≤` → `- - -- <=`) so the ASCII theme is pure ASCII for every render-owned byte — proven by byte-level probe AND pinned by a Rust `is_ascii()` test. The contract's testing discipline lands: snapshot tests pin BOTH glyph themes byte-exact (chrome-only workflow — every byte is ours) + colour-on verb-gate SGR assertions. |
| 2026-06-11 | rustc-grade span diagnostics + contract conformance | `SchemaError::span()` — the ONE uniform span surface (30 arms · `Cycle`=None · the LSP primitive). `ConformanceViolation` gains `span: Option<ByteSpan>` (additive · `report_version` stays 1 · the JSON agent + LSP read offsets against the source they hold). The example renders source EXCERPTS under findings — `┌─ file:line:col` frame + the offending line + carets under the token (`,-`/`|` in ascii) — for conformance findings AND the PARSE error path (now routed through the same seam + glyph grammar instead of a raw un-themed `✗`). `--infer-permits` gains the clig.dev dual surface (stdout = pasteable artifact · stderr = themed human guidance). Contract §3.4 conformance: `TERM=dumb` now disables colour in the resolution chain (flag → `NO_COLOR` → `CLICOLOR_FORCE` → TTY → `TERM=dumb` · explicit force wins · `ColourEnv` struct keeps the pure core testable). 13 example tests (4 snippet · 3 render · 6 theme) · 442 lib. |
| 2026-06-11 | the verb theater (`examples/verbs`) | The four execution models as ANIMATED terminal storyboards — « the animation IS the data »: every frame is a pure function `frame(verb, step, theme) → String` (playback = iteration · `--frame N` renders any moment statically for CI/screenshots · reduced motion = the last frame · tests pin frames byte-exact). Scenes: infer (the `${{ }}` binding dot TRAVELS a rail into the prompt · dispatch · token streaming · schema-valid · cost card) · exec (permits gate · spawn · stdout scroll · exit card) · invoke (tools+fs gates · args bind · dispatch · result card) · agent (think→invoke→observe turns ×3 · `nika:done` sentinel · turns/tokens/cost card). Contract §3.2 honored: braille spinner ⠋…⠏ 80 ms ticks ONLY on the running line (`>` pulse in ascii) · motion gates orthogonal to colour (TTY + `NIKA_REDUCED_MOTION` + `--no-anim` — `NO_COLOR` kills colour, never motion) · cursor-up redraw is the only non-theme escape. `--legend` renders the canonical theme reference card (every glyph both renderings · every role painted · the verb-gate families · chrome typography) — the theme spec, self-documented from the source of truth, in the binary. Architecture: theme.rs shared by `#[path]` (ONE seam, two consumers); single-consumer typography (arrow/ellipsis) stays scene-local until a second surface needs it (Rams 10). 12 scene tests (totality · purity · pinned final frames · ascii purity · spinner-only-while-running · the travelling dot). |
| 2026-06-11 | the event tape — real telemetry, two renderers (`examples/verbs/tape.rs`) | The theater grounds in the CANONICAL `nika-event` vocabulary (the 11 `EventKind`s the engine actually emits · dev-dep only — the L0 prod graph untouched). ONE deterministic demo tape (17 real `Event`s · ids `Uuid::from_u128` · caller-supplied timestamps — nothing reads a clock) feeds TWO renderers, the contract's « one truth » law reference-implemented: `--events` = the tape view (every telemetry event one digestible line: relative `+Nms` · kind glyph painted by class · stable wire slug · compact `k=v` fields · then the folded final card) · `verbs workflow` = the motion view (the SAME tape folded live into animated DAG lanes — tasks light wave by wave, the dependency rail carries the travelling dot while a task runs, token/$/checkpoint counters tick, a `▰▱` progress bar fills, the run settles on its verdict). The fold (`TapeState × Event → TapeState`) is total over every kind (failure kinds included · future `#[non_exhaustive]` kinds no-op). Mode enum replaced the flag bools (the type encodes mode exclusivity) + ONE `animate()` playback for any pure frame fn. 18 verbs tests (tape determinism · fold totality · pinned tape line · pinned final frame · ascii purity · ticking counters). |
| 2026-06-11 | the tape speaks the FULL vocabulary | After the nika-event cohort (6 additive kinds), the demo tape exercises everything: a permit gate before each effect dispatch (`permit_checked` ✔ green / ✖ deny red) · a transient-failure RETRY ARC on fetch (`task_retrying` → ↻ yellow + transient `attempt 1/2` annotation that NEVER outlives the retry — the verb detail returns on restart; the stale-note bug was caught by the re-pin diff) · live streaming (`infer_chunk` deltas accumulate in the lane while extract runs — the run TALKS) · the cost meter ticks MID-RUN on `cost_incurred` deltas (cost=SPEND · completion=OUTCOME · never double-counted) · a permits counter joins the footer meters. `Cancelled ◼` joins the theme glyph grammar (ascii `x` dim per contract §3.1). The legend moved INTO the seam (theme.rs self-documents; BOTH surfaces expose `--legend`) — the dead-code lint structurally forced the right home. THE correspondence test: every UI `RowStatus` (8 states) is event-reachable — UI states ⊆ event-expressible states, pinned. 19 verbs tests · Mode enums replaced flag-bool sets on both mains. |
| 2026-06-11 | bug sweep — the contract matrix holds | Adversarial probe round caught 3 real bugs: (1) **`--json` + parse error emitted ZERO stdout bytes** — the machine promise broken exactly where the agent loop needs it most; now a structured payload (`{report_version, clean:false, error:{kind:"parse", code: NIKA-PARSE-NNN, message, span}}`) lands on stdout. (2) **exit codes were off the LOCKED contract §4**: findings returned 1 (contract: 2 — « CI gates on 2 ») and usage errors squatted 2 (the findings code). Now: 0 clean · 2 findings AND parse errors (file-content errors) · 64 `EX_USAGE` · 66 `EX_NOINPUT` · 70 `EX_SOFTWARE` (sysexits — additive per the contract's own law). Empirical matrix verified unpiped (a probe lesson: `cmd | head; echo $?` reads HEAD's exit). (3) **CRLF sources leaked the `\r` into the snippet display line** (`^M`) — stripped from display only (offsets/carets untouched), pinned by test. Empty-file probe confirmed the infallible model: parses → `NIKA-PARSE-002` lands IN-REPORT → exit 2. 14 check tests. |
| 2026-06-11 | the third renderer — NDJSON wire (`--events --json`) | « One truth, N renderers » completes: the SAME demo tape now has THREE — human tape · animated DAG fold · **NDJSON verbatim** (one serde-event per line · never coloured · the contract §3 machine surface a real `nika run --json` streams to CI/agents). The wire is PINNED byte-exact from reality (ids nest as `{"uuid":…}` — the newtype's named field; `Value` serializes untagged; timestamps are nanos — the L0 truth surfaced by pinning). Every line round-trips and carries the snake_case kind slug (26 events · 13 distinct kinds). `--json` outside `--events` is an EXPLICIT usage rejection (64) — silent flag-ignoring is the bug class this binary refuses. 20 verbs tests. |
| 2026-06-11 | review-swarm round — 10 findings, all folded | Battery: 31-crate workspace + 34-fixture mini-fuzz (0 unexpected exits) + garbage probes (urandom/truncated/1100-indent/5MB/bad-utf8 — all loud, zero panics). Two READ-ONLY reviewers (rust-pro + nika-canon) on the session diff found 3 P1 + 7 P2, all fixed: **ascii glyphs drifted off the LOCKED §3.1 table** (`ok` not `+` · err `X` ≠ cancelled `x` — my earlier arm-merge was a misreading; unmerged + re-pinned every snapshot) · **`TapeState.terminal: Option<bool>` erased the cancelled-vs-failed distinction this very arc introduced** → 3-way `Verdict` enum, cancelled draws dim ◼ never red · **`tests/event_contract.rs` was a landmine** (its `any_kind()`/taxonomy hardcoded the original 11 — refreshing it would have FAILED the terminal property encoding a false invariant; synced to 17 + SYNC-LAW comments + insta refreshed) · snippet hardened (mid-UTF-8 lying spans bail · end snaps back to boundary · tabs normalize in display · alignment claim scoped honestly) · `VerbInvoked` with empty detail no longer clobbers a `ToolInvoked` note · check rejects `--json --legend` (64) symmetric with verbs · spec §2 API snapshot synced (17 kinds + `EventClass`) · stale "11 EventKinds" doc count → census pointer. `fold` split under the 100-line cap (`fold_dispatch`). 16 check + 21 verbs tests. |
| 2026-06-11 | Gate-6 properties on the reference fold | The run-card fold gets the same property discipline as `suggest.rs`: (1) **totality law** — ANY event sequence (arbitrary kind × task addressing incl. unknown/garbage × hostile fields incl. negative spend) folds without panicking, `done() ≤ rows.len()`, and `terminal` arises IFF a workflow-outcome kind appeared · (2) renderers pure + total over arbitrary steps (0..10k · clamping) and colour-off frames NEVER leak ANSI · (3) `tape_line` total over arbitrary events both themes (chrome themed · data verbatim). 3 proptests · 24 verbs tests. Public spec checked: traces/observability are runtime surface, correctly OUT of the v0.1 language spec — the event vocabulary stays engine-internal (`nika-event` spec §2/§4bis is its contract). |
| 2026-06-11 | Gate-5 on the session's src deltas — the predicted survivor | Mutation infra was SILENTLY BROKEN for the whole crate: cargo-mutants copies the tree to a sandbox where the `../spec` sibling doesn't exist → `conformance_core` hard-fails its baseline BY DESIGN → every run forgetting `-- --lib` reported `Found 0 mutants`. Structural fix: `additional_cargo_test_args = ["--lib"]` baked into `.cargo/mutants.toml` (the same law as the repo-wide test invocation — config carries the law, not operator memory). Then the focused run on `error.rs` + `check/mod.rs`: 16 mutants, ONE survivor — `span() → Some(Default::default())`, **exactly the mutant predicted before the run** (the lib test's loose `start ≤ end < 1000` bound let 0,0 live). Killed by pinning the EXACT token offset (`span.start == src.find("ghost")`) — which also surfaced a real fact: the parser emits POINT spans (end==start) for flow-list scalars; full token ranges are the LSP-grade follow-up, and the pin will catch the day they land. Re-run: 11 caught · 5 unviable · **0 missed**. |
| 2026-06-12 | ladder #6 ships — gate reachability (`check/reach.rs`) | arXiv-grounded (the skill protocol): for ACYCLIC workflow structures, reachability is quadratic-with-diagnostics (Prinz/Schwanen/van der Aalst 2026 · arxiv.org/abs/2602.02447) while the general problem is EXPSPACE-complete (Blondin et al. 2022 · arxiv.org/abs/2201.05588) — **Nika's acyclicity-by-construction is the tractability moat, no Z3 needed**. Method: one topo pass of abstract interpretation over the 4-status terminal domain (spec §Task states: `success·failure·skipped·cancelled` — `skipped` reachable ONLY via `when:`-falsifiable or `on_error: skip`; `cancelled` always possible) + per-gate exact enumeration over referenced upstream sets in Kleene-3 (non-status atoms → Unknown — sound: never a dead-claim from uncertainty; >6 refs → satisfiable). Findings: **DeadTask** (gate unsatisfiable — contradiction · impossible status · upstream-dead CASCADE, with the « can only be {…} » diagnostic) + **BadStatusLiteral** (`'failed'` for `'failure'` — the wild-caught class, with a closed-vocabulary did-you-mean: the rustc threshold rejects distance-3 over open identifier spaces; over a fixed 4-word vocabulary ≤3 is unambiguous). `when: false` literal = the documented never-pattern, NOT a finding. New report field `gate_findings` (additive) + `is_clean` + REACH section in the renderer. 22 reach tests · 466 lib. **Gate 5**: 111 mutants → 84 caught + 1 timeout · 16 survivors ALL exempt by analysis (the documented-exemption form): 7× disjoint-bit `\|`→`^` (for disjoint constants `a\|b ≡ a^b` — equivalent by arithmetic) · 5× the unreachable-defensive `parse_gate→None` arm (the analyzer rejects bad islands before `check()` ever calls reach — kept for totality) · 4× the default-gate `runnable` check, equivalent-by-invariant (no fold arm produces a possible-set disjoint from `{success,skipped}`; by induction the branch never fires today — kept as the sound general transfer function, not hardcoding an accidental invariant). 100% viable-kill with exemptions. |
| 2026-06-12 | hint #6 — redundant success-gate | The spec names the anti-pattern VERBATIM (03 §the gate: « do NOT write `when: ${{ tasks.X.status == 'success' }}` as a plain gate — redundant — meaningful only when X may be skipped ») — now the ameliorateur teaches it: flags a gate that is EXACTLY that one relation (either operand order) on a dep that cannot be skipped (no `when:` · no `on_error: skip`); a conjunct inside a larger expression is a real condition beyond the default gate and never flagged; a skippable dep makes the gate meaningful and never flagged. DRY: reuses reach's `status_ref` atom matcher (`pub(super)`). ⭐ tooling trap caught live: python heredoc `\<newline>` is a PYTHON string continuation — it ate the Rust backslash and left the indentation INSIDE the single-line string (the rendered advice had 20-space runs); normalized via `\u{2014}` escapes. 3 hint tests. |
| 2026-06-12 | nuclear-review judo — `status_atom` | The skill's structural pass on the #6 diff found ONE real duplication: the either-order `(status_ref(lhs), status_ref(rhs))` destructure dance lived 3× (`eval_relation` · `collect_bad_literals` · `hints::sole_success_gate`) → one shared `pub(super) fn status_atom` (3 sites → 1 helper · behavior identical · 466 green unchanged). |
| 2026-06-12 | ladder #7 ships — the run certificate (`check/certificate.rs`) | arXiv-grounded (round 2): AARA derives *resource polynomials parametric in input sizes* via LP over typing derivations (Hoffmann/Das/Weng 2016 · arxiv.org/abs/1611.00692 · still active: Chu/Guo/Hoffmann 2026 · arxiv.org/abs/2603.02260) — **Nika needs no solver: the workflow IS its own derivation** (acyclic · every loop one `for_each` collection · retries capped · agent turn-capped default 10), so the degree-1 coefficients read directly off the structure. Termination is a THEOREM of the language — the certificate ALWAYS exists; its value is the quantitative envelope: `Bound = constant + Σ coeff·\|task\|` (the runtime `for_each` collection sizes) on THREE axes — task-attempts · LLM calls (infer=1 · agent≤max_turns) · effect calls (exec/invoke). Counting model: body runs ≤ attempts×iterations; `on_finally` once per ITERATION (never per attempt). `action_calls` is exhaustive ON PURPOSE (defining crate): a future 5th verb fails compilation until it declares its call class — the certificate can never silently under-count. Renderer: `✔ CERT terminates · ≤ 2 + 2·\|fan\| task-attempts · ≤ 2·\|fan\| LLM calls` (`*` in ascii). Report field `certificate` additive (version stays 1). 7 cert tests (exact constants · retry× · literal-list fold · parametric terms · agent default-10 · on_finally once · wire shape pinned) · 473 lib. Ladder: #1-#7 shipped · #8 Salsa (LSP infra) · #9 differential conformance remain. |
| 2026-06-12 | #7 deepened — the PARAMETRIC spend axis | The certificate gains `usd_micros: Option<Bound>` (micro-USD integers — exact on the wire): **the bound COST cannot express** — a `for_each`-expression fan-out is « unknown iterations » to the ceiling but a degree-1 term here: `✔ CERT … · ≤ $0.0060·\|fan\| spend` printed exactly where COST says « FLOOR (unbounded) ». Agent spend = `max_tokens_total` per body run (the budget is CUMULATIVE across turns — never ×turns, pinned by test). `None` when any spender is unpriceable (no token bound · no catalog price — COST names why); the COUNT axes stay exact regardless. `on_finally` spend counted once per iteration. 9 cert tests (wire pinned with the 75-µ$ parametric term). |
| 2026-06-12 | ladder #9 first slice — metamorphic conformance (`check/metamorphic.rs`) | arXiv round 3: differential testing without a second engine = equivalence transformations on ONE system (Wu/Zheng/Yang/Yu 2025 · arxiv.org/abs/2504.04321; pairs-of-executions methodology: Ba/Jiang/Rigger 2025 · arxiv.org/abs/2508.16307). A proptest generator emits random VALID workflows as YAML **through the real front door** (`parse→analyze→check`, never constructed ASTs) — structure rendered with a parameterizable id prefix (what makes R2 exact). Three relations × 64 cases: **R0** generator↔engine validity differential (a disagreement = a bug in one of them) · **R1** task-block-order permutation invariance (full verdict preserved: conformance · `is_clean` · certificate as order-free multisets · cost total · finding counts) · **R2** alpha-renaming invariance (certificates map term-for-term after prefix erasure). All three passed FIRST RUN — the post-swarm system holds under its own equivalence group. 478 lib. ⭐ trap: `let _ = write!` is a STATEMENT — match-arm expression positions need braces (the python regex that tried to auto-convert died exactly there). |
| 2026-06-12 | the certificate becomes CERTIFYING — witness + independent checker | arXiv round 4: a certifying algorithm outputs a result WITH a witness and a checker simpler than the solver (Shokry/Elmasry/Khalafallah/Aly 2024 · arxiv.org/abs/2412.06121); the execution-side architecture is Proposal–Certification–Execution — « generation is not permission » (Liu Yanglet/Wang/Capponi 2026 · arxiv.org/abs/2605.24462 — May 2026, directly on AI-agent certified traces). `RunCertificate` gains `derivation: Vec<TaskContribution>` (per-task witness rows: attempts · fanout shape · per-run call/spend counts · per-iteration finally counts) and **`audit(&wf)`** — the catalog-free checker: (a) every row matches the workflow's declared structure by FIELD EQUALITY, (b) the rows re-fold to the claimed bounds via THE one shared `fold_rows` (certify builds bounds FROM rows through the same fold — arithmetic cannot drift between builder and checker because it exists once). A foreign certificate (marketplace artifact · CI gate input) is re-checkable locally: tampered bound → « do not match the derivation »; doctored row → named field mismatch; truncated witness → row-count reject. 3 tamper tests + the wire pin now carries the witness. |
| 2026-06-12 | metamorphic deepened — R3-R6 | **R3 unfolding** (a literal 2-list `for_each` ≡ two duplicated plain tasks — total attempts/calls/spend equal · the certificate's fan-out semantics tested against itself) · **R4 frame** (adding an independent plain task shifts the bounds by EXACTLY its contribution — compositionality) · **R5 retry-1 identity** (`max_attempts: 1` ≡ no retry block — structural twin built by normalizing attempts then inserting explicit retry-1 after each task header; the naive string-replace died on duplicate-key, itself a parser-strictness confirmation) · **R6 audit-totality** (every honest certificate passes its own audit across the whole generator — the checker accepts what the analysis produces, 64 random workflows). 7 relations total · 483 lib. |
| 2026-06-12 | the span axis + the research-conformance suite | `RunCertificate.span_attempts` — the longest sequential dependency chain in attempts (Brent 1974 lineage · Tassarotti 2017 · arxiv.org/abs/1704.02061): retries are SERIAL (extend span) · `for_each` fan-out is element-parallel (extends work, never span) → the CERT line prints the full Brent envelope (`terminates · span ≤ 4 · ≤ 2 + 2·\|fan\| task-attempts · …`). Witness rows gain `deps`; span computed FROM the rows by the one shared fold (iterative DFS + memo · cycle-cut defensive since `certify` runs even on conformance-failing input); `audit` re-checks deps AND re-folds span (span/dep tampering rejected). `tests/research_conformance.rs` — one executable property per arXiv claim in check/: the **AARA substitution lemma** (parametric bound @ n ≡ concretized workflow, n∈{1,2,5,9}, all three axes incl. spend) · the **Brent envelope** (span==work on chains · span<work on wide DAGs · span≤work@1 over a family) · **reachability vs a brute-force oracle** (exact agreement on the ==/!= single-dep fragment) · **certifying audit under 16 systematic tampers** (all rejected · honest accepted) · **Denning transitivity at 3 alias hops**. `fold_rows` arithmetic saturating end-to-end. |
| 2026-06-16 | Gate-11 admission swarm + 12-gate ledger | §11ter ledger authored (11/12 green · Gate 5 mutation pending a quiet-window FLOOR run · 1711 mutants). rust-security final pass: every probed surface SOUND except **one P0 FIXED** — `check/flow.rs` IFC taint-trace was O(n²) time+memory (per-task `TaintTrace.hops` a cloned `Vec<String>`; a 0.89 MiB *valid* workflow → 5.2 GB RSS, reachable via `nika check` + the agent MCP `nika_check`). Fix · hop chain → `Arc` cons-list (`Hop`), `via()`/`clone()` O(1), `analyze_flow` O(n), leak semantics unchanged (596→597 tests); regression `long_secret_chain_stays_linear_not_quadratic`. Gate-7 `parse_bench.rs` shipped (parse/small 13µs · analyze/large 222µs). §11 reconciled (pre-parse byte/indent/task/depth caps confirmed SHIPPED). Fast-follows FIXED same arc · F2 (P1) `reach.rs` gate `in`-list → `max_list_len` back-off (`MAX_GATE_LIST_ITEMS` 256, sound widen) + regression test · P2 `flow.rs` secrets → `BTreeSet` membership (deterministic · `HashSet` disallowed). |


| 2026-06-18 | Gate 5 — the whole-crate mutation FLOOR + survivor close (rounds 1-2) | First full floor: **1601 mutants, 10h, ZERO machine reboots**. ⭐ The 4 prior reboots were NOT a HW fault — `--jobs 6` on an 18 GB Mac drove specific `check/analysis.rs` graph-fixpoint fns (`downstream_adjacency·descendant_closure·set_bits·hopcroft_karp·hk_bfs·hk_dfs·koenig_witness·scan_parallel_writers`) into non-converging loops that allocate unbounded when mutated → `vm-compressor-space-shortage` jetsam → watchdog reset (no `.panic`, no nvram panic-info = not a kernel panic). FIX: `--jobs 1 --timeout 20 --exclude-re` those 8 (diverge-by-design · covered by the König/reach property suite). Result **926 caught · 86 missed · 269 timeout · 320 unviable**. Survivor close: **rounds 1-2 killed 151** real-gap survivors (`b84c48dc` 4 clusters, 68 tests · `d7cb5f84` preference_rules 49/49, 31 tests) — the gap was collection/lint detection logic exercised but never asserted (`schema_paths` output-path pipeline · `preference_rules` rules 005/006/007 + is_value_producer/is_shard_chain/leaf_paths · `certificate` accounting · `hints` secret-walk). maker≠checker re-verification caught a 50-survivor head-truncation, a bad fixture, and 6 clippy lints the no-cargo agents could not. **Budget plan** (BUDGET mode · `survivors ≤ N`): 269 timeout-divergers + 16 reach-equivalents (see 2026-06-12 row) — final `<!-- GATE5-EXEMPT: N -->` set once the ~70-mutant long tail is killed/justified. |
| 2026-06-18 | Gate 5 close — rounds 3-7 + review swarm | Survivor close finished · §10 budget set to **300** = 269 timeout-divergers + 21 enumerated equivalents (`reach.rs` ×14 · `expression/parser.rs` ×5 · `template.rs` ×1 · `schema_lint.rs` ×1), each re-verified by scoped `cargo mutants`. **Rounds 3-7** (~190 tests) killed the real-gap tail: `read_dag` cap/pinch boundaries (round 6 · `9bcdbb1af`), the default-gate runnable path (`cb29d351e` — which ALSO killed two mutants the 2026-06-12 row had mislabelled "equivalent-by-invariant": the `reach.rs` runnable mask `\| → &` and `!= → ==`, both observably killable), and the **expression sub-language** parser/template/AST that rounds 1-6 never reached (round 7 · `58c9540c3` · 11 tests · `peek_offset` error offsets, the `MAX_DEPTH=128` boundary, the unary depth-exit leak across a relation's lhs→rhs, the quote-aware `}}` byte-scanner). **Two non-Gate-5 fixes** the 5-lens review swarm surfaced same-arc: a real O(n²) `when:`-gate DoS (`7312519f0` · §11) and 7 missing `#[non_exhaustive]` `source/` types (`c9158986f` · FCI-002, `cargo check --workspace` clean across all 6 consumers). One reviewer P2 (`declass.rs` host-glob bypass) was VERIFIED a false positive (`ends_with(".suffix")` is label-boundary-safe). Method · scoped `cargo mutants -f <glob>` reuses the cached baseline (minutes, restart-robust) vs the 10 h `--jobs 1` full floor; re-adjudicate prior "equivalent" labels rather than trusting them. |
| 2026-07-07 | hint — `schema-portability` (the grammar-blind keywords) | Companion to the guided-decoding audit (#246 closed the strict-portability half at the wire; this ships the authoring-surface half): a `schema:` leaning on `uniqueItems` / `not` / `if`+`then`/`else` compiles into NO provider grammar — **proven live** (llama.cpp b9890 + ollama both ACCEPT such a schema, then constrained decoding emits the forbidden value / duplicates on demand; openai strict strips `uniqueItems` at the wire). Only the engine's local validation holds these constraints, spending schema retries per violation — the new advisory hint names that price at check time, once per task, keywords listed. Precision: BINDING occurrences only (`uniqueItems: false` and a bare `if` without `then`/`else` constrain nothing — no claim) · property NAMES are never keywords (`properties: { not: … }` stays silent) · `$ref` opaque. Same-arc live e2e at the built binary: llama.cpp nested schema (enum·required·closed objects) 2.6s green · ollama qwen3.5:4b 22.4s green — the F2 recipe (native wire + LOCAL validation) held on both. 3 hint tests · 779 lib. |
