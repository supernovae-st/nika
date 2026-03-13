# Binding System Radical Redesign

**Date**: 2026-03-13
**Status**: PROPOSAL v2 -- radical redesign (no backwards compat)
**Scope**: Nika binding module (`src/binding/`) + AST + DAG + templates
**Research**: 6 agents, 3 research docs, PDF analysis, full code review (4,119 lines)

---

## Executive Summary

Nika's binding DSL has fundamental naming and design problems that make workflows hard
to read and write. We're at v0 with zero users -- **this is the time to get it right.**

This is a **clean break**, not an incremental improvement. Everything changes at once:

| Before | After | Why |
|--------|-------|-----|
| `use:` | `with:` | Natural English, GitHub Actions precedent |
| `{{use.alias}}` | `{{alias}}` | Drop the noisy prefix |
| `path:` field | `from:` field | Natural English |
| `flows:` section | REMOVED | Replaced by implicit deps + `depends_on:` |
| No transforms | `\| pipe` syntax | 8/9 engines have this, we don't |
| No types | `type:` field | Errors by default |
| `use:` only refs tasks | `$context.*`, `$inputs.*`, `$env.*` | Unified path syntax |
| 3-pass template | 2-pass template | Simpler model |

**Blast radius**: 274 Rust references, 557 template references, 349 YAML `use:` fields,
78 `flows:` sections. One release. Clean cut.

---

## Design Principles

1. **Clean break**: No deprecation warnings, no shims, no backwards compat
2. **YAML-first**: Every feature must read naturally in YAML
3. **Progressive disclosure**: Simple things stay simple, complex things are possible
4. **Low barrier, high ceiling**: Built-in transforms for 90%, JQ for 10%
5. **Errors by default**: Catch problems early, `type: any` to opt out
6. **Implicit is better**: Dependencies inferred from data references

---

## The New Syntax: Before/After

### Simple workflow

```yaml
# ============ BEFORE ============
schema: nika/workflow@0.11

tasks:
  - id: research
    infer: "Research AI trends"

  - id: generate
    use:
      findings: $research
    infer: "Summarize: {{use.findings}}"

flows:
  - source: research
    target: generate


# ============ AFTER =============
schema: nika/workflow@0.12

tasks:
  - id: research
    infer: "Research AI trends"

  - id: generate
    with:
      findings: $research              # implicit dependency on 'research'
    infer: "Summarize: {{findings}}"   # no 'use.' prefix
                                       # no flows: section needed
```

### Complex workflow

```yaml
# ============ BEFORE ============
schema: nika/workflow@0.11

context:
  files:
    brand: ./context/brand.md

tasks:
  - id: fetch_data
    fetch:
      url: "https://api.example.com/data"

  - id: analyze
    use:
      data: $fetch_data
    infer: |
      Analyze: {{use.data}}
      Brand: {{context.files.brand}}
      Locale: {{inputs.locale}}

  - id: save
    use:
      result: $analyze
    exec: "echo done"

flows:
  - source: fetch_data
    target: analyze
  - source: analyze
    target: save


# ============ AFTER =============
schema: nika/workflow@0.12

context:
  files:
    brand: ./context/brand.md

tasks:
  - id: fetch_data
    fetch:
      url: "https://api.example.com/data"

  - id: analyze
    with:
      data: $fetch_data                # implicit dep
      brand: $context.brand            # context access in with: block!
      locale: $inputs.locale ?? "en-US"  # input access + default
    infer: |
      Analyze: {{data}}
      Brand: {{brand}}
      Locale: {{locale}}

  - id: save
    with:
      result: $analyze
    exec: "echo done"
                                       # no flows: -- all deps are implicit
```

### Transforms

```yaml
tasks:
  - id: research
    fetch:
      url: "https://api.example.com/papers"

  - id: analyze
    with:
      # Pipe transforms -- extends existing |shell pattern
      papers: $research.data | sort | unique
      count: $research.data | length
      title: $research.meta.title | upper | trim
      top3: $research.data | sort | first(3)

      # Object form for complex cases
      summary:
        from: $research.abstract
        type: string
        transform: lower | trim
        default: "No abstract available"
        lazy: true
    infer: |
      Found {{count}} papers.
      Title: {{title}}
      Top 3: {{top3 | to_json}}
```

### Types

```yaml
with:
  name: $step1.title                  # type: any (default)
  count:
    from: $step1.items | length
    type: integer                     # ERROR if not integer
  tags:
    from: $step1.metadata.tags
    type: array                       # ERROR if not array
  config:
    from: $step1.settings
    type: object
```

Type mismatches are **errors** by default. Use `type: any` to accept anything.

### Implicit dependencies

```yaml
tasks:
  - id: a
    infer: "Step A"

  - id: b
    with:
      result_a: $a                    # implicit: b depends on a
    infer: "Step B uses {{result_a}}"

  - id: c
    depends_on: [a]                   # explicit: c depends on a (no data flow)
    exec: "cleanup.sh"
```

The DAG builder extracts dependencies from `with:` `$references` automatically.
`depends_on:` is only needed when there's an ordering requirement without data flow.

### for_each

```yaml
tasks:
  - id: process
    for_each: $research.items         # source array
    as: item                          # loop variable
    with:
      current: $item                  # loop var accessible via $
      locale: $inputs.locale
    infer: "Process {{current}} in {{locale}}"
```

---

## Unified `$` Path Syntax

All data sources use the same `$` prefix syntax in `with:` blocks:

```
$task_id               task output (full)
$task_id.field         task output field
$task_id.data[0].name  deep path (JSONPath, v0.29)
$task_id[*].name       wildcard (JSONPath, v0.29)
$context.files.brand   context file
$context.session       session data
$inputs.locale         workflow input
$inputs.items[0]       workflow input with path
$env.API_URL           environment variable (NEW)
$item                  for_each loop variable
```

Reserved namespaces: `context`, `inputs`, `env`
Everything else: task ID or loop variable

This replaces the 3-pass template system with a unified resolution model:
- Pass 1: Resolve `with:` aliases (all `$` paths resolved here)
- Pass 2: Resolve direct template refs (`{{context.*}}`, `{{inputs.*}}` still work for convenience)

---

## Transform Pipeline

### Built-in transforms (v0.28)

| Category | Transforms |
|----------|------------|
| String | `upper`, `lower`, `trim`, `trim_start`, `trim_end` |
| Collection | `length`, `first`, `last`, `first(n)`, `last(n)`, `keys`, `values` |
| Collection | `flatten`, `reverse`, `sort`, `unique`, `compact` |
| Type | `to_string`, `to_number`, `to_bool`, `to_json`, `parse_json` |
| Numeric | `round(n)`, `abs`, `ceil`, `floor` |
| Utility | `default(value)`, `typeof`, `join(sep)`, `split(sep)` |
| Existing | `shell` (already in template.rs) |

### In `with:` blocks (chains allowed)

```yaml
with:
  names: $users | sort | unique | join(", ")
  count: $items | length
  preview: $content | trim | first(100)
```

### In templates (single transform only)

```yaml
infer: |
  Title: {{title | upper}}
  Count: {{items | length}}
  Raw: {{data | to_json}}
```

Single transform in templates keeps them readable.
Multi-transform chains belong in the `with:` block.

### Array interpolation

Arrays JSON-serialize by default in templates:
- `{{items}}` --> `["a","b","c"]` (JSON)
- `{{items | join(", ")}}` --> `a, b, c` (joined)
- `{{items | first}}` --> `a` (single element)

JSON-serialize is the default because it's lossless and LLMs understand JSON natively.

---

## `flows:` Removal

`flows:` is removed entirely. Replaced by:

1. **Implicit dependencies** from `with:` `$task_id` references (automatic)
2. **`depends_on:`** for explicit ordering without data flow

Why remove:
- Redundant with implicit deps (data reference = dependency)
- Error-prone (easy to forget an edge, or have edge without data flow)
- Verbose (separate section with source/target objects)
- `depends_on:` covers the non-data ordering case
- Zero users = no migration cost
- The TUI's DAG visualization already shows the computed graph

### Migration examples

```yaml
# BEFORE: flows: section
flows:
  - source: a
    target: b
  - source: b
    target: c

# AFTER: if b uses data from a, it's automatic
tasks:
  - id: a
    infer: "Step A"
  - id: b
    with:
      result: $a              # automatic edge a->b
    infer: "Step B: {{result}}"
  - id: c
    depends_on: [b]           # explicit edge b->c (no data)
    exec: "notify.sh"
```

---

## Rust Type Changes

### Renames

| Before | After | File |
|--------|-------|------|
| `UseEntry` | `WithEntry` | `binding/entry.rs` |
| `WiringSpec` | `WithSpec` | `binding/entry.rs` |
| `UseEntryVisitor` | `WithEntryVisitor` | `binding/entry.rs` |
| `from_wiring_spec()` | `from_with_spec()` | `binding/resolve.rs` |
| `parse_use_entry()` | `parse_with_entry()` | `binding/entry.rs` |
| `LazyBinding` | `BindingState` | `binding/resolve.rs` |
| `ResolvedBindings` | `BindingMap` | `binding/resolve.rs` |

### New types

```rust
// binding/types.rs (NEW)

/// Source path for a binding -- parsed from "$task_id.field.path"
pub struct BindingPath {
    pub source: BindingSource,
    pub segments: Vec<PathSegment>,
}

/// Where the data comes from
pub enum BindingSource {
    Task(Arc<str>),       // $task_id
    Context(Arc<str>),    // $context.files.brand
    Input(Arc<str>),      // $inputs.locale
    Env(Arc<str>),        // $env.API_URL
    LoopVar(Arc<str>),    // $item (from for_each as:)
}

pub enum PathSegment {
    Field(Arc<str>),
    Index(usize),
}

/// Type constraint -- errors by default if violated
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BindingType {
    String, Number, Integer, Boolean, Array, Object, Any,
}


// binding/transform.rs (NEW)

pub enum TransformOp {
    // String
    Upper, Lower, Trim, TrimStart, TrimEnd,
    // Collection
    Length, First, Last, FirstN(usize), LastN(usize),
    Keys, Values, Flatten, Reverse, Sort, Unique, Compact,
    // Type
    ToString, ToNumber, ToBool, ToJson, ParseJson,
    // Numeric
    Round(Option<u32>), Abs, Ceil, Floor,
    // Utility
    Default(Value), TypeOf, Join(String), Split(String),
    // Existing
    Shell,
    // Future (feature-gated)
    #[cfg(feature = "jq")]
    Jq(Arc<str>),
}

pub struct TransformExpr {
    pub ops: SmallVec<[TransformOp; 2]>,
}


// binding/entry.rs (REFACTORED)

pub struct WithEntry {
    pub source: BindingPath,
    pub binding_type: BindingType,
    pub default: Option<Value>,
    pub lazy: bool,
    pub transform: Option<TransformExpr>,
}

pub type WithSpec = FxHashMap<String, WithEntry>;


// binding/resolve.rs (REFACTORED)

pub enum BindingState {
    Resolved { value: Arc<Value>, source: BindingSource },
    Pending { entry: WithEntry },
    Failed { source: BindingPath, error: Arc<BindingResolutionError> },
}
```

### AST changes

```rust
// ast/raw/task.rs
pub struct RawTask {
    pub id: Option<Marked<String>>,
    pub with: Option<Marked<WiringMap>>,     // was: use
    pub depends_on: Option<Vec<String>>,     // NEW
    // ... rest unchanged
}

// ast/analyzed/task.rs
pub struct AnalyzedTask {
    pub id: TaskId,
    pub with: WithSpec,                      // was: use / wiring
    pub depends_on: Vec<TaskId>,             // NEW
    pub implicit_deps: Vec<TaskId>,          // NEW -- extracted from with: $refs
    // ... rest unchanged
}
```

### DAG builder changes

```rust
// dag/mod.rs -- edge construction

fn build_edges(tasks: &[AnalyzedTask]) -> Vec<Edge> {
    let mut edges = Vec::new();
    for task in tasks {
        // 1. Implicit deps from with: $task_id references
        for dep in &task.implicit_deps {
            edges.push(Edge::new(dep.clone(), task.id.clone()));
        }
        // 2. Explicit depends_on:
        for dep in &task.depends_on {
            edges.push(Edge::new(dep.clone(), task.id.clone()));
        }
        // 3. flows: REMOVED -- no longer a source of edges
    }
    edges
}
```

---

## File Changes (v0.28)

| File | Action | Details |
|------|--------|---------|
| `binding/types.rs` | NEW | BindingPath, BindingSource, BindingType, PathSegment |
| `binding/transform.rs` | NEW | TransformOp, TransformExpr, pipe parser |
| `binding/entry.rs` | REWRITE | UseEntry->WithEntry, WiringSpec->WithSpec, `with:` YAML |
| `binding/resolve.rs` | REWRITE | LazyBinding->BindingState, unified resolution |
| `binding/template.rs` | REFACTOR | Drop `use.` prefix, 2-pass model, dedup helpers |
| `binding/mention.rs` | REFACTOR | Update to use WithSpec (was WiringSpec) |
| `binding/validate.rs` | MINOR | Update type references |
| `binding/mod.rs` | UPDATE | New re-exports |
| `ast/raw/task.rs` | MODIFY | `use:` -> `with:`, add `depends_on:` |
| `ast/raw/workflow.rs` | MODIFY | Remove `flows:` field |
| `ast/raw/parser.rs` | MODIFY | Parse `with:` instead of `use:` |
| `ast/analyzed/task.rs` | MODIFY | WithSpec, depends_on, implicit_deps |
| `ast/analyzed/workflow.rs` | MODIFY | Remove flows |
| `ast/analyzer/analyze.rs` | MODIFY | Extract implicit deps, remove flows validation |
| `dag/mod.rs` | MODIFY | Build edges from implicit + depends_on (no flows) |
| `dag/flow.rs` | REMOVE | No longer needed (flows: removed) |
| `dag/validate.rs` | MODIFY | Update edge source |
| `runtime/runner.rs` | MODIFY | WithSpec references |
| `runtime/executor/verbs.rs` | MODIFY | WithSpec references |
| `runtime/chat_workflow.rs` | MODIFY | Update mentions->WithSpec |
| `lib.rs` | MODIFY | Update re-exports |
| `error.rs` | MODIFY | Update error messages |
| `schemas/nika-workflow.schema.json` | REWRITE | `with:` schema, remove `flows:` |
| ALL `.nika.yaml` files | REWRITE | `use:`->`with:`, `{{use.x}}`->`{{x}}`, remove `flows:` |
| ALL test files (.rs) | UPDATE | `use:`->`with:` in inline YAML, update type names |

---

## Template System Simplification

### Before: 3-pass resolution

```
Pass 1: {{use.alias}}     resolved from WiringSpec
Pass 2: {{context.x}}     resolved from ContextData
Pass 3: {{inputs.x}}      resolved from WorkflowInputs
```

### After: 2-pass resolution

```
Pass 1: {{alias}}          resolved from WithSpec (all sources unified)
Pass 2: {{context.x}}     direct context/inputs access (convenience)
         {{inputs.x}}
```

The key insight: `with:` can now reference `$context.*` and `$inputs.*`, so most
templates only need Pass 1. Pass 2 exists for convenience (quick access without binding).

Templates in `{{...}}` support single transforms: `{{title | upper}}`, `{{count | to_string}}`.

---

## Phase Plan

All changes ship in v0.28. No multi-release phasing needed since there's no backwards compat.

### v0.28: The Big Rename + Transforms + Types (this release)

Everything described above. Single release, clean break.

### v0.29: Rich Paths (serde_json_path)

Replace `util/jsonpath.rs` with RFC 9535:

```yaml
with:
  all_names: "$users[*].name"          # Wildcard
  expensive: "$items[?@.price > 100]"  # Filter
  deep: "$..email"                     # Recursive descent
  last: "$items[-1]"                   # Negative indexing
  slice: "$items[0:3]"                 # Slice
```

### v0.30: Output Contracts

JSON Schema on task outputs, validated by analyzer:

```yaml
tasks:
  - id: fetch_data
    fetch:
      url: "https://api.example.com/data"
    output:
      schema:
        type: object
        required: [items, total]
```

### v0.31: JQ Escape Hatch

Feature-gated `jq()` transform via `jaq-core`:

```yaml
with:
  complex:
    from: $data
    transform: "jq('.items | map(select(.price > 100)) | sort_by(.name)')"
```

---

## Test Strategy

| Category | Count (est.) |
|----------|-------------|
| BindingPath + BindingSource parsing | 40 |
| BindingType validation (errors) | 25 |
| TransformOp (each op) | 50 |
| TransformExpr pipeline | 15 |
| WithEntry serde (string + object forms) | 40 |
| Implicit dependency extraction | 20 |
| depends_on merging | 15 |
| Template 2-pass resolution | 30 |
| Template single-transform | 20 |
| Unified $ path resolution | 25 |
| **Total new** | **~280** |
| **Existing (to update)** | **~136** |
| **Grand total** | **~416** |

---

## Decision Log

| # | Decision | Chosen | Rejected | Rationale |
|---|----------|--------|----------|-----------|
| 1 | Keyword | `with:` | `use:`, `bind:`, `data:`, `vars:` | Natural English, GH Actions precedent, not a Rust keyword |
| 2 | Template syntax | `{{alias}}` | `{{use.alias}}`, `{{with.alias}}` | Cleaner, less noise, alias is already scoped |
| 3 | Object field | `from:` | `path:`, `source:` | Natural English ("data FROM this source") |
| 4 | Dependencies | implicit + `depends_on:` | `flows:`, `after:` | Data ref = dep (automatic), explicit only when needed |
| 5 | `flows:` | REMOVED | Keep alongside | Redundant, error-prone, verbose. Zero users. |
| 6 | Type errors | Error by default | Warning by default | Catch early. `type: any` to opt out. v0 = strict is fine. |
| 7 | Array interpolation | JSON-serialize | Join, custom | Lossless, LLM-friendly, `\| join()` for custom |
| 8 | `with:` scope | All sources ($context, $inputs, $env) | Tasks only | Most powerful. Unified transforms on any source. |
| 9 | Migration speed | Instant (v0.28) | 4-release deprecation | Zero users. Clean break > slow transition. |
| 10 | Transform engine | Built-in + JQ opt-in | Full JQ, CEL | Low barrier (87/100), high ceiling |
| 11 | Template transforms | Single-op only | Multi-op chains | Readability in templates, chains in `with:` |
| 12 | Path engine (v0.29) | serde_json_path | jsonpath-rust, custom | RFC 9535, returns refs, 600+ tests |
| 13 | State pattern | BindingState enum | Type-state | Mixed-state maps require enum |
| 14 | Interior mutability | parking_lot::Mutex | RefCell, std::Mutex | Faster, no poisoning |

---

## Research Sources

| Source | Agent | Key Finding |
|--------|-------|-------------|
| Socratic Critique | ac9eeca | Pipe modifiers, template.rs dedup priority |
| Context7 jq/jsonpath | a150b47 | jaq-core + serde_json_path recommended |
| CNCF Serverless Workflow | a117dc7 | JQ mandatory but steep learning curve |
| Temporal/Windmill/Flyte/Hatchet | aad02b2 | Nika is the only engine limited to string interpolation |
| Perplexity 9-engine comparison | a59ac5c | Custom templates + built-in functions scored 87/100 |
| Rust Architect type system | a743d7c | 1,915-line design doc with concrete Rust types |
| PDF analysis | nika-binding-analysis.pdf | 8 problems, 5 recommendations (partially validated) |
| Code review | 6 binding files | 4,119 lines of actual code read in full |

---

## Companion Documents

| Document | Lines | Content |
|----------|-------|---------|
| `docs/research/2026-03-13-binding-type-system-design.md` | 1,915 | Full Rust types and architecture |
| `docs/research/2026-03-13-workflow-binding-patterns.md` | ~500 | 4-engine deep dive |
| `docs/research/2026-03-13-serverless-workflow-dsl-data-flow.md` | ~400 | CNCF Serverless Workflow |

---

*v0 with zero users. This is the time to get it right. Everything changes in v0.28.*
