# 05 -- AST Pipeline

Nika uses a three-phase pipeline inspired by rustc's compilation model. Each phase has distinct responsibilities and produces a different intermediate representation.

```
YAML Source (.nika.yaml)
        |
        | marked-yaml (Phase 1)
        v
   RawWorkflow         <-- Spanned<T>, strings, no validation
        |
        | analyze() (Phase 2)
        v
   AnalyzedWorkflow    <-- TaskId(u32), resolved refs, validated
        |
        | lower() (Phase 3)
        v
   Runtime Workflow    <-- String names, consumed by Runner
```

---

## Phase 1: Raw Parsing

**Module:** `nika-core/src/ast/raw/parser.rs`

**Input:** YAML text

**Output:** `RawWorkflow` with `Spanned<T>` fields

### What Happens

The parser uses `marked-yaml` to parse YAML with full byte-offset tracking. Every value in the resulting AST is wrapped in `Spanned<T>`:

```rust
pub struct Spanned<T> {
    pub value: T,
    pub span: Span,
}

pub struct Span {
    pub file: FileId,
    pub start: u32,  // byte offset
    pub end: u32,    // byte offset
}
```

This enables the analyzer and error display to report precise source locations: "line 14, column 5 in workflow.nika.yaml".

### Design Principles

1. **Every node has a span.** From the top-level `schema:` to nested `env:` values in `exec:`, every parsed value knows where it came from.

2. **Strings are unresolved.** Task IDs, dependency references, binding expressions -- all are raw strings. No validation happens here.

3. **No validation.** The parser accepts any structurally valid YAML. A workflow with references to nonexistent tasks passes Phase 1. Semantic checks are Phase 2's job.

4. **Preserves YAML order.** `IndexMap` is used instead of `HashMap` to maintain key insertion order.

### Key Types

```rust
pub struct RawWorkflow {
    pub schema: Spanned<String>,
    pub workflow: Option<Spanned<String>>,
    pub description: Option<Spanned<String>>,
    pub provider: Option<Spanned<String>>,
    pub model: Option<Spanned<String>>,
    pub mcp: Option<Spanned<RawMcpConfig>>,
    pub context: Option<Spanned<RawContextConfig>>,
    pub imports: Option<Spanned<Vec<Spanned<RawImportSpec>>>>,
    pub inputs: Option<Spanned<IndexMap<Spanned<String>, Spanned<serde_json::Value>>>>,
    pub agents: Option<Spanned<serde_json::Value>>,
    pub skills: Option<Spanned<IndexMap<Spanned<String>, Spanned<String>>>>,
    pub artifacts: Option<Spanned<serde_json::Value>>,
    pub log: Option<Spanned<serde_json::Value>>,
    pub tasks: Spanned<Vec<Spanned<RawTask>>>,
    pub span: Span,
}

pub struct RawTask {
    pub id: Spanned<String>,
    pub description: Option<Spanned<String>>,
    pub action: Option<RawTaskAction>,
    pub provider: Option<Spanned<String>>,
    pub model: Option<Spanned<String>>,
    pub with_refs: Option<Spanned<IndexMap<Spanned<String>, Spanned<String>>>>,
    pub depends_on: Option<Spanned<Vec<Spanned<String>>>>,
    pub output: Option<Spanned<RawOutputConfig>>,
    pub for_each: Option<Spanned<RawForEach>>,
    pub retry: Option<Spanned<RawRetryConfig>>,
    pub decompose: Option<Spanned<DecomposeSpec>>,
    pub structured: Option<StructuredOutputSpec>,
    pub artifact: Option<Spanned<serde_json::Value>>,
    pub span: Span,
}

pub enum RawTaskAction {
    Infer(Spanned<RawInferAction>),
    Exec(Spanned<RawExecAction>),
    Fetch(Spanned<RawFetchAction>),
    Invoke(Spanned<RawInvokeAction>),
    Agent(Box<Spanned<RawAgentAction>>),
}
```

### Error Codes (Phase 1)

| Code | Variant | Description |
|------|---------|-------------|
| NIKA-160 | `ParseErrorKind::Syntax` | Invalid YAML syntax |
| NIKA-161 | `ParseErrorKind::MissingField` | Required field missing |

Phase 1 errors are rare since most validation happens in Phase 2. They occur when the YAML itself is malformed (bad indentation, unclosed quotes) or when the top-level structure cannot be parsed (missing `schema:` or `tasks:`).

---

## Phase 2: Analysis

**Module:** `nika-core/src/ast/analyzer/analyze.rs`

**Input:** `RawWorkflow`

**Output:** `AnalyzedWorkflow` (or collected errors)

### What Happens

The analyzer performs a single-pass transformation with comprehensive validation:

1. **Schema version validation**: Checks `schema: "nika/workflow@0.12"`. Unknown versions produce an error with a "did you mean?" suggestion.

2. **Task table construction**: Builds a `TaskTable` that maps string task names to `TaskId(u32)`. This enables O(1) comparison and deduplication detection.

3. **Reference resolution**: Every `depends_on:` entry and every `with:` binding source is resolved from string to `TaskId`. Unknown references produce errors.

4. **Implicit dependency extraction**: `with: { data: $task_a }` creates an implicit dependency from the current task to `task_a`, even without an explicit `depends_on:`.

5. **Cycle detection**: DFS three-color algorithm (White -> Gray -> Black). If a Gray node is revisited, a cycle exists. The cycle path is reported.

6. **Duplicate ID detection**: Each task ID must be unique. Duplicates produce `NIKA-022 DuplicateTaskId`.

7. **Binding validation**: `with:` entry syntax is validated against the binding grammar (path traversal, default values, transform pipes).

8. **Error collection**: ALL errors are collected in a single pass, not fail-fast. This enables IDEs to show every problem at once.

### Suggestion Engine

The analyzer uses Jaro-Winkler similarity from the `strsim` crate for fuzzy matching:

```
Unknown task "taks1" -> did you mean "task1"? (similarity: 0.93)
Invalid schema "nika/workfow@0.10" -> did you mean "nika/workflow@0.12"?
```

The suggestion threshold is typically 0.7 similarity. Only the best match is suggested.

### Key Types

```rust
pub struct AnalyzedWorkflow {
    pub schema_version: SchemaVersion,
    pub name: String,
    pub description: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub task_table: TaskTable,
    pub tasks: Vec<AnalyzedTask>,
    pub mcp_servers: Vec<AnalyzedMcpServer>,
    pub context_files: Vec<AnalyzedContextFile>,
    pub imports: Vec<AnalyzedImportSpec>,
    pub inputs: IndexMap<String, serde_json::Value>,
    pub artifacts: Option<serde_json::Value>,
    pub log: Option<serde_json::Value>,
    pub agents: Option<IndexMap<String, serde_json::Value>>,
    pub skills_map: Option<IndexMap<String, String>>,
    pub span: Span,
}

pub struct TaskId(u32);  // Interned identifier

pub struct TaskTable {
    name_to_id: FxHashMap<String, TaskId>,
    id_to_name: Vec<String>,
}

pub struct AnalyzedTask {
    pub id: TaskId,
    pub name: String,
    pub description: Option<String>,
    pub action: AnalyzedTaskAction,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub with_spec: WithSpec,
    pub depends_on: Vec<TaskId>,      // Explicit ordering edges
    pub implicit_deps: Vec<TaskId>,   // Auto-extracted from with:
    pub output: Option<AnalyzedOutput>,
    pub for_each: Option<AnalyzedForEach>,
    pub retry: Option<AnalyzedRetry>,
    pub decompose: Option<DecomposeSpec>,
    pub structured: Option<StructuredOutputSpec>,
    pub span: Span,
}
```

### Benefits of Interning

| Aspect | Raw (String) | Analyzed (TaskId) |
|--------|-------------|-------------------|
| Comparison | O(n) string compare | O(1) integer compare |
| Memory | One String per reference | 4 bytes per TaskId |
| Hashing | String hash (crypto-quality) | Integer hash (trivial) |
| Dedup detection | Manual scanning | Built into TaskTable |

### Error Codes (Phase 2)

| Code | Variant | Description |
|------|---------|-------------|
| NIKA-140 | `UnknownTask` | Referenced task does not exist |
| NIKA-141 | `DuplicateTask` | Task ID appears more than once |
| NIKA-142 | `InvalidSchemaVersion` | Unrecognized schema version |
| NIKA-143 | `CycleDetected` | Circular dependency in DAG |
| NIKA-144 | `InvalidWithEntry` | Malformed `with:` binding syntax |
| NIKA-145 | `EmptyWorkflow` | No tasks defined |
| NIKA-146 | `InvalidVerb` | Unknown verb (not infer/exec/fetch/invoke/agent) |
| NIKA-147 | `MissingAction` | Task has no verb defined |
| NIKA-148 | `InvalidField` | Unknown field on a known structure |
| NIKA-149 | `InvalidValue` | Value does not match expected type |
| NIKA-150 | `DependsOnSelf` | Task references itself |
| NIKA-151 | `TransformParseError` | Invalid transform pipe syntax |

### Error Format

```rust
pub struct AnalyzeError {
    pub kind: AnalyzeErrorKind,
    pub span: Span,
    pub message: String,
    pub suggestion: Option<String>,
}
```

Each error carries:
- **kind**: Enum variant with error code
- **span**: Precise source location
- **message**: Human-readable description
- **suggestion**: Optional "did you mean?" correction

### AnalyzeResult

```rust
pub struct AnalyzeResult {
    workflow: Option<AnalyzedWorkflow>,
    errors: Vec<AnalyzeError>,
}

impl AnalyzeResult {
    pub fn into_result(self) -> Result<AnalyzedWorkflow, Vec<AnalyzeError>>;
    pub fn is_ok(&self) -> bool;
    pub fn is_err(&self) -> bool;
}
```

The result can contain both a partial workflow AND errors (for IDE diagnostics that need to show completions even in invalid files).

---

## Phase 3: Lowering

**Module:** `nika-engine/src/ast/lower.rs`

**Location**: `nika-engine/src/ast/lower.rs`

**Input:** `AnalyzedWorkflow`

**Output:** Runtime `Workflow`

### What Happens

Lowering converts the validated analyzed AST into the runtime types consumed by the execution engine. This is primarily a structural transformation with no additional validation:

1. **TaskId -> String**: Resolved back via `TaskTable::name(id)`. The runtime uses string task IDs for human-readable display and datastore keys.

2. **Action conversion**: `AnalyzedInferAction` -> `InferParams`, `AnalyzedExecAction` -> `ExecParams`, etc.

3. **MCP config conversion**: `AnalyzedMcpServer` -> `McpConfigInline` with command, args, env, cwd.

4. **Output policy conversion**: `AnalyzedOutput` -> `OutputPolicy` with format, schema, max_retries.

5. **Context conversion**: `AnalyzedContextFile` -> `ContextConfig` with file mappings.

6. **Import conversion**: `AnalyzedImportSpec` -> `IncludeSpec` for DAG fusion.

### Key Function

```rust
pub fn lower(analyzed: AnalyzedWorkflow) -> Result<Workflow, NikaError> {
    // Destructure analyzed workflow
    // Convert each task via lower_task()
    // Convert MCP servers via lower_mcp_servers()
    // Convert inputs via lower_inputs()
    // Build runtime Workflow
}
```

### Runtime Types

```rust
pub struct Workflow {
    pub schema: String,
    pub name: String,
    pub provider: String,
    pub model: Option<String>,
    pub mcp: FxHashMap<String, McpConfigInline>,
    pub context: Option<ContextConfig>,
    pub include: Vec<IncludeSpec>,
    pub agents: Option<IndexMap<String, serde_json::Value>>,
    pub skills: Option<IndexMap<String, String>>,
    pub artifacts: Option<serde_json::Value>,
    pub log: Option<serde_json::Value>,
    pub inputs: Option<IndexMap<String, serde_json::Value>>,
    pub tasks: Vec<Arc<Task>>,
}

pub struct Task {
    pub id: Arc<str>,
    pub description: Option<String>,
    pub action: TaskAction,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub binding_spec: BindingSpec,
    pub depends_on: Vec<Arc<str>>,
    pub output: Option<OutputPolicy>,
    pub for_each: Option<ForEachConfig>,
    pub retry: Option<RetryConfig>,
}
```

### Unlowering

The `unlower()` function converts back from runtime types to analyzed types. This is used by the TUI's DAG visualization and the `nika workflow graph` command.

---

## Direct Analyzed Path

Since v0.49.0, the `Runner` directly consumes `AnalyzedWorkflow` and performs bridge conversions at the `TaskExecutor` boundary. This eliminates the full `lower()` step during execution:

```
YAML -> raw::parse -> analyzer::analyze -> AnalyzedWorkflow -> Runner
                                                     |
                                                     +-> lower_action() at TaskExecutor boundary
```

The `parse_analyzed()` function exposes this shorter pipeline:

```rust
pub fn parse_analyzed(yaml: &str) -> Result<AnalyzedWorkflow, NikaError> {
    let raw = raw::parse(yaml, FileId(0))?;
    analyzer::analyze(raw).into_result()
}
```

The full `lower()` path is still used by:
- `nika check` (CLI validation)
- `expand_includes()` (include loader)
- Test helpers
- The `parse_workflow()` convenience function

---

## Pipeline Entry Points

| Function | Pipeline | Used By |
|----------|----------|---------|
| `raw::parse(yaml, file_id)` | Phase 1 only | LSP, testing |
| `analyzer::analyze(raw)` | Phase 2 only | After Phase 1 |
| `parse_analyzed(yaml)` | Phase 1 + 2 | Runner, validation |
| `parse_workflow(yaml)` | Phase 1 + 2 + 3 | CLI check, includes |
| `lower(analyzed)` | Phase 3 only | After Phase 2 |

---

## Source Tracking

The `source` module provides the infrastructure for precise error locations:

```rust
pub struct FileId(pub u16);

pub struct SourceFile {
    pub name: String,
    pub content: String,
}

pub struct SourceRegistry {
    files: Vec<SourceFile>,
}
```

The `SourceRegistry` can hold multiple files (for imports and includes). Each `FileId` indexes into the registry. `Span` references a `FileId` to identify which file a source location belongs to.

The `Spanned<T>` wrapper is zero-cost in release builds when the span is not accessed -- the value and span are stored inline with no heap allocation.
