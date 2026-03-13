# Design Analysis: Nika Binding Type System Redesign

**Date**: 2026-03-13
**Author**: Claude (Opus 4) + Thibaut
**Status**: RESEARCH & DESIGN -- no implementation
**Scope**: Type-safe binding layer for Nika's `use:` block and data-flow pipeline
**Companion docs**: `workflow-binding-patterns.md`, `workflow-engine-binding-patterns.md`

---

## Table of Contents

1. [Current Problems](#1-current-problems)
2. [Design Requirements](#2-design-requirements)
3. [Crate Evaluation](#3-crate-evaluation)
4. [Design Area 1: Unified Binding Syntax](#4-design-area-1-unified-binding-syntax)
5. [Design Area 2: Typed Bindings (BindingType)](#5-design-area-2-typed-bindings-bindingtype)
6. [Design Area 3: Path Expressions](#6-design-area-3-path-expressions)
7. [Design Area 4: Transform Pipeline](#7-design-area-4-transform-pipeline)
8. [Design Area 5: Lifecycle Type-State](#8-design-area-5-lifecycle-type-state)
9. [Design Area 6: Output Contracts](#9-design-area-6-output-contracts)
10. [Design Area 7: Zero-Copy & Performance](#10-design-area-7-zero-copy--performance)
11. [YAML Serialization Layer](#11-yaml-serialization-layer)
12. [Migration Strategy](#12-migration-strategy)
13. [Recommended Architecture](#13-recommended-architecture)
14. [Appendix: Decision Log](#appendix-decision-log)

---

## 1. Current Problems

Five fundamental issues in `src/binding/` prevent Nika's binding system from scaling:

### Problem 1: Two Incompatible Binding Syntaxes

`UseEntry` accepts both string and object forms via a custom serde `Visitor`:

```yaml
# String form
use:
  result: step1.output.field ?? "default"

# Object form
use:
  result:
    path: step1.output.field
    lazy: true
    default: "default"
```

The string form is parsed by `parse_use_entry()` with a hand-rolled `??` operator
parser (including `find_operator_outside_quotes()`). The object form uses standard
serde. They produce the same `UseEntry` struct, but the string parser has edge cases
around JSON defaults and quoted values.

**Impact**: Maintenance burden, subtle parsing bugs, no room to add fields (type, transform).

### Problem 2: No Type Information

`UseEntry` is:

```rust
pub struct UseEntry {
    pub path: String,          // Untyped string
    pub default: Option<Value>, // Untyped JSON
    pub lazy: bool,
}
```

Everything resolves to `serde_json::Value` at runtime. There is no way to declare
"this binding should be a number" or "this binding should be an array of strings."
The analyzer cannot validate binding compatibility.

**Impact**: Runtime errors that could be caught at parse time. No IDE support for
binding type mismatches.

### Problem 3: Regex-Based Template Resolution

Templates like `{{use.alias}}` are resolved by 5 pre-compiled regexes:

```rust
static USE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\{\{\s*use\.(\w+(?:\.\w+)*)(?:\s*\|\s*(shell))?\s*\}\}").unwrap()
});
```

This is a 3-pass architecture (use -> context -> inputs). Each pass does string
replacement on the output of the previous pass, with security isolation preventing
re-evaluation of injected markers.

**Impact**: String-only transforms (only `|shell` exists). Cannot transform values
before interpolation (e.g., uppercase, parse JSON, extract array length). Adding
new modifiers requires regex changes.

### Problem 4: No Output Contracts

Tasks do not declare what they produce. `output_policy` exists for structured
output enforcement on `infer:` tasks, but there is no general mechanism to say
"this task outputs `{ name: string, price: number }`."

**Impact**: Binding validation is impossible at parse time. When `use: { p: $calc.price }`,
the analyzer cannot verify that `calc` produces a `price` field.

### Problem 5: No Inline Transforms

The jsonpath module (`util/jsonpath.rs`) supports only `Field(String)` and
`Index(usize)` segments. There are no filters, wildcards, slices, or transform
expressions. Workflow authors must create intermediate tasks for trivial
transformations.

```yaml
# Cannot do this:
use:
  count: "$items | length"
  names: "$users[*].name"
  first: "$results | first"

# Must create a whole task instead:
- id: count_items
  exec: "echo '{{use.items}}' | jq length"
```

**Impact**: Workflow verbosity. Simple data transformations require full task
definitions.

---

## 2. Design Requirements

Seven requirements guide the redesign, ordered by priority:

| # | Requirement | Priority | Rationale |
|---|-------------|----------|-----------|
| R1 | YAML-friendly serialization | Critical | Workflows are YAML files; binding syntax must be clean |
| R2 | Typed bindings at parse-time | High | Catch type mismatches in the analyzer, not at runtime |
| R3 | Path expressions | High | Navigate nested JSON: `task.output.items[0].name` |
| R4 | Transform expressions | Medium | JQ-like operations: `| length`, `| first`, `| keys` |
| R5 | Lazy resolution | Medium | Already exists; must be preserved and enhanced |
| R6 | Default values | Medium | Already exists; must work with typed bindings |
| R7 | Zero-copy where possible | Low | `Cow<str>`, `Arc<Value>` to avoid clones |

---

## 3. Crate Evaluation

### 3.1 serde_json_path (v0.7.2)

**What it does**: Implements RFC 9535 (JSONPath standard) for `serde_json::Value`.

**API**:

```rust
use serde_json::json;
use serde_json_path::JsonPath;

let data = json!({"store": {"book": [{"title": "Rust"}, {"title": "Go"}]}});
let path = JsonPath::parse("$.store.book[*].title")?;
let titles: Vec<&Value> = path.query(&data).all();
// titles = [Value::String("Rust"), Value::String("Go")]

// Single value
let first = path.query(&data).at_most_one()?;

// Existence check
let exists = path.query(&data).exactly_one().is_ok();
```

**Supported features**:
- Dot notation: `$.a.b.c`
- Bracket notation: `$['a']['b']`
- Array index: `$[0]`, `$[-1]` (negative indexing)
- Wildcard: `$[*]`, `$.a[*]`
- Slice: `$[0:5]`, `$[::2]`
- Filter: `$[?@.price > 10]`
- Recursive descent: `$..name`

**Strengths**:
- Full RFC 9535 compliance (standardized, well-documented)
- Returns references (`&Value`), not clones
- `NodeList` type with `all()`, `exactly_one()`, `at_most_one()` ergonomics
- 600+ tests in the crate

**Weaknesses**:
- Query only -- no transformation (cannot mutate values)
- Returns references, which complicates ownership in async contexts
- `JsonPath::parse()` allocates; would need caching for repeated use

**Verdict**: Excellent for path navigation. Replace `util/jsonpath.rs` with this crate.
It covers R3 (path expressions) completely.

### 3.2 jaq-core (v2.2.1)

**What it does**: A Rust implementation of jq. Parses jq filter strings, compiles
them to an AST, and evaluates them against JSON values.

**API**:

```rust
use jaq_core::{load, Ctx, RcIter};
use jaq_json::Val;
use serde_json::json;

// 1. Parse filter
let (filter_ast, errs) = jaq_parse::parse(".items | length", jaq_parse::main());
assert!(errs.is_empty());

// 2. Load standard library definitions
let defs = load::parse("", |p| jaq_std::std().chain(jaq_std::defs()).find(p));

// 3. Compile
let filter = defs.compile(filter_ast.unwrap());

// 4. Evaluate
let input = Val::from(json!({"items": [1, 2, 3]}));
let inputs = RcIter::new(core::iter::empty());
let ctx = Ctx::new([], &inputs);

let results: Vec<Val> = filter.run((ctx, input)).collect::<Result<Vec<_>, _>>()?;
// results = [Val::Int(3)]
```

**Supported operations** (subset relevant to Nika):
- Path access: `.field`, `.field.subfield`
- Array indexing: `.[0]`, `.[-1]`
- Array/object iteration: `.[]`
- Pipe: `| ` (compose operations)
- Map: `map(f)`, `select(f)`
- Length: `length`
- Keys/values: `keys`, `values`
- Type functions: `type`, `tostring`, `tonumber`
- String interpolation: `"Hello \(.name)"`
- Conditionals: `if-then-else-end`
- Try-catch: `try-catch`
- Reduce: `reduce .[] as $x (0; . + $x)`
- Object construction: `{name: .title, count: (.items | length)}`

**Strengths**:
- Full jq language (extremely powerful)
- Streaming evaluation (lazy, memory-efficient)
- `Val` type wraps `serde_json::Value` with `Arc` (reference-counted, cheap clone)
- Compilation step enables caching compiled filters

**Weaknesses**:
- Heavy dependency (jaq-core + jaq-std + jaq-parse + jaq-json = 4 crates)
- jq learning curve is steep for workflow authors
- Error messages from jq are terse and hard to contextualize
- `Val` vs `Value` conversion overhead (though minimal with `Arc`)

**Verdict**: Too heavy as the primary binding expression language. However, could be
offered as an opt-in "power mode" behind a `|jq(...)` transform modifier. Cover R4
(transform expressions) with built-in functions first, jq as escape hatch.

### 3.3 jsonpath-rust (v0.7+)

**What it does**: Another JSONPath implementation, with broader operator support
than `serde_json_path` but non-standard extensions.

**Comparison with serde_json_path**:

| Feature | serde_json_path | jsonpath-rust |
|---------|----------------|---------------|
| RFC 9535 | Full | Partial + extensions |
| Returns | `&Value` (refs) | Owned `Value` (clones) |
| Filters | Standard | Extended (custom functions) |
| Performance | Faster (refs) | Slower (cloning) |
| Maturity | High (500+ tests) | Moderate |

**Verdict**: `serde_json_path` is preferred due to RFC compliance and reference returns.

### 3.4 CUE Language Concepts

CUE (Configure, Unify, Execute) uses a **lattice-based type system** where values
and types live on the same hierarchy. A value "is" a more specific version of its
type:

```cue
// Type and value unified
name: string    // constraint: must be a string
name: "Alice"   // valid: "Alice" satisfies string

price: >0 & <1000  // constraint: must be positive, under 1000
price: 42.99        // valid: satisfies both constraints
```

**Relevant pattern for Nika**: CUE's approach of "constraints as types" could inform
binding validation. Instead of rigid types (`string`, `number`), bindings could
declare constraints:

```yaml
use:
  price:
    path: $product.price
    type: number       # constraint
    min: 0             # constraint
    max: 1000          # constraint
```

However, implementing a full CUE-like unification system would be massive overkill.
The relevant insight is **constraints as optional validators on bindings**, not a
full lattice type system.

### 3.5 Type-State Pattern in Rust

The type-state pattern uses Rust's type system to enforce state transitions at
compile time:

```rust
// States are types (zero-sized)
struct Unresolved;
struct Resolved;
struct Consumed;

struct Binding<State> {
    path: String,
    _state: PhantomData<State>,
}

impl Binding<Unresolved> {
    fn resolve(self, store: &DataStore) -> Binding<Resolved> {
        // ...
    }
}

impl Binding<Resolved> {
    fn value(&self) -> &Value { /* ... */ }
    fn consume(self) -> (Value, Binding<Consumed>) {
        // ...
    }
}

// Compile-time error: cannot call value() on Unresolved
// let v = unresolved_binding.value(); // ERROR
```

**Assessment for Nika**: While elegant, the type-state pattern is impractical for
Nika's binding system because:

1. Bindings are stored in `FxHashMap<String, LazyBinding>` -- the map must hold
   bindings in mixed states (some resolved, some pending).
2. Binding resolution happens at runtime, not compile time.
3. The `DashMap<Arc<str>, TaskResult>` DataStore is inherently dynamic.

**Alternative**: An enum-based state machine (which Nika already uses with
`LazyBinding::Resolved | Pending`) is the correct approach. The type-state pattern
works better for builder APIs and connection state machines.

---

## 4. Design Area 1: Unified Binding Syntax

### Problem

Two syntaxes produce the same `UseEntry`. This makes it hard to add new features
(type hints, transforms) to both syntaxes consistently.

### Proposal: Single Object Syntax with Shorthand

Keep the string shorthand for simple cases, but define a clear **canonical form**
that supports all features:

```yaml
use:
  # Shorthand (backwards compatible) -- for simple eager bindings
  result: $step1                    # Whole output
  name: $step1.name                 # Path access
  name: $step1.name ?? "fallback"   # With default

  # Canonical form -- for typed/lazy/transformed bindings
  price:
    from: $product.price            # Source path (renamed from 'path')
    type: number                    # Optional type constraint
    default: 0                      # Optional default
    lazy: true                      # Optional lazy flag
    transform: "round(2)"          # Optional transform expression
```

### Rust Types

```rust
/// Source path for a binding -- parsed from YAML string
#[derive(Debug, Clone, PartialEq)]
pub struct BindingPath {
    /// Task ID that produces the value
    pub task_id: Arc<str>,
    /// JSONPath segments into the task's output (empty = whole output)
    pub segments: Vec<PathSegment>,
}

/// A single segment in a binding path
#[derive(Debug, Clone, PartialEq)]
pub enum PathSegment {
    /// Object field access: `.field`
    Field(Arc<str>),
    /// Array index access: `[N]`
    Index(usize),
    /// Recursive descent: `..field` (if serde_json_path is used)
    Recursive(Arc<str>),
}

/// Optional type constraint on a binding
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BindingType {
    String,
    Number,
    Integer,
    Boolean,
    Array,
    Object,
    /// Any type (default, no constraint)
    Any,
}

/// A single binding entry in the `use:` block
#[derive(Debug, Clone)]
pub struct UseEntry {
    /// Source path: task_id + optional field navigation
    pub source: BindingPath,
    /// Expected type (optional, validated at resolution)
    pub binding_type: BindingType,
    /// Default value if source is null/missing
    pub default: Option<Value>,
    /// Deferred resolution until first access
    pub lazy: bool,
    /// Optional transform expression applied after resolution
    pub transform: Option<TransformExpr>,
}

/// Type alias for the full use: block
pub type WiringSpec = FxHashMap<Arc<str>, UseEntry>;
```

### Key Design Decision: `from:` instead of `path:`

Renaming `path` to `from` makes the YAML read more naturally:

```yaml
use:
  price:
    from: $product.price    # "price comes FROM product.price"
    type: number
```

This also disambiguates from filesystem paths in `context:` blocks.

### Backwards Compatibility

The `from:` field is new. For backwards compatibility, the serde deserializer
should accept both `path:` and `from:`, with `path:` emitting a deprecation
warning in the analyzer.

---

## 5. Design Area 2: Typed Bindings (BindingType)

### Why Not Full JSON Schema?

Full JSON Schema on every binding would be verbose and fragile in YAML:

```yaml
# Too verbose for common cases:
use:
  name:
    from: $user.name
    schema:
      type: string
      minLength: 1
      maxLength: 100
```

Instead, use a **simple type hint** for common cases and reserve full schemas for
output contracts (Design Area 6):

```yaml
use:
  name:
    from: $user.name
    type: string          # Simple hint

  items:
    from: $search.results
    type: array            # Simple hint -- "this should be an array"
```

### Type Validation at Resolution Time

When a binding resolves, the type hint is validated:

```rust
impl UseEntry {
    /// Validate resolved value against type constraint
    pub fn validate_type(&self, value: &Value) -> Result<(), BindingTypeError> {
        match (&self.binding_type, value) {
            (BindingType::Any, _) => Ok(()),
            (BindingType::String, Value::String(_)) => Ok(()),
            (BindingType::Number, Value::Number(_)) => Ok(()),
            (BindingType::Integer, Value::Number(n)) if n.is_i64() || n.is_u64() => Ok(()),
            (BindingType::Boolean, Value::Bool(_)) => Ok(()),
            (BindingType::Array, Value::Array(_)) => Ok(()),
            (BindingType::Object, Value::Object(_)) => Ok(()),
            (expected, actual) => Err(BindingTypeError {
                alias: self.alias.clone(),
                expected: expected.clone(),
                actual_type: value_type_name(actual),
                path: self.source.to_string(),
            }),
        }
    }
}

fn value_type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}
```

### Type Propagation in the Analyzer

The Phase 2 analyzer can propagate types through the DAG:

```rust
/// Type information inferred for a task's output
#[derive(Debug, Clone)]
pub enum InferredOutputType {
    /// Unknown (no output schema declared)
    Unknown,
    /// Simple type (from output_policy or heuristic)
    Simple(BindingType),
    /// Structured (from JSON Schema in output_policy)
    Schema(Arc<serde_json::Value>),
}

/// During analysis, build a type map
pub struct TypeMap {
    /// task_id -> inferred output type
    outputs: FxHashMap<TaskId, InferredOutputType>,
}

impl TypeMap {
    /// Check if a binding path is compatible with the source task's output
    pub fn validate_binding(
        &self,
        entry: &UseEntry,
    ) -> Result<(), BindingCompatibilityWarning> {
        let source_type = self.outputs.get(&entry.source.task_id);
        match source_type {
            Some(InferredOutputType::Schema(schema)) => {
                // Validate that entry.source.segments navigates
                // to a valid path in the schema
                self.validate_path_against_schema(
                    &entry.source.segments,
                    schema,
                )
            }
            _ => Ok(()), // Unknown output -- skip validation
        }
    }
}
```

This is a **warning**, not an error, because tasks may produce dynamic output that
cannot be statically predicted (e.g., LLM output).

---

## 6. Design Area 3: Path Expressions

### Replace util/jsonpath.rs with serde_json_path

The current `Segment::Field | Segment::Index` parser is 86 lines and supports
only dot notation and numeric indexing. Replace it entirely with `serde_json_path`:

```rust
use serde_json_path::JsonPath;

/// Cached compiled JSONPath for a binding
pub struct CompiledPath {
    raw: String,
    compiled: JsonPath,
}

impl CompiledPath {
    pub fn new(path: &str) -> Result<Self, PathError> {
        // Convert Nika's dot-path notation to JSONPath
        // "items[0].name" -> "$.items[0].name"
        let jsonpath_str = if path.starts_with('$') {
            path.to_string()
        } else {
            format!("$.{path}")
        };

        let compiled = JsonPath::parse(&jsonpath_str)
            .map_err(|e| PathError::InvalidPath {
                path: path.to_string(),
                reason: e.to_string(),
            })?;

        Ok(Self {
            raw: path.to_string(),
            compiled,
        })
    }

    /// Query a value, returning a reference (zero-copy for single results)
    pub fn query_one<'a>(&self, value: &'a Value) -> Option<&'a Value> {
        self.compiled.query(value).at_most_one().ok().flatten()
    }

    /// Query a value, returning all matches
    pub fn query_all<'a>(&self, value: &'a Value) -> Vec<&'a Value> {
        self.compiled.query(value).all()
    }
}
```

### Path Compilation Cache

Since the same path expressions appear in every workflow execution, cache compiled
paths:

```rust
use std::sync::LazyLock;
use dashmap::DashMap;

/// Global cache for compiled JSONPath expressions
static PATH_CACHE: LazyLock<DashMap<Arc<str>, Arc<CompiledPath>>> =
    LazyLock::new(DashMap::new);

pub fn get_or_compile(path: &str) -> Result<Arc<CompiledPath>, PathError> {
    if let Some(cached) = PATH_CACHE.get(path) {
        return Ok(Arc::clone(cached.value()));
    }
    let compiled = Arc::new(CompiledPath::new(path)?);
    PATH_CACHE.insert(Arc::from(path), Arc::clone(&compiled));
    Ok(compiled)
}
```

### New Path Capabilities

With `serde_json_path`, these expressions become available in bindings:

```yaml
use:
  # Current (still works)
  name: $user.name
  first: $items.0

  # New with serde_json_path
  all_names: "$users[*].name"           # Wildcard -- all names as array
  expensive: "$items[?@.price > 100]"   # Filter
  deep: "$..email"                      # Recursive descent
  last: "$items[-1]"                    # Negative indexing
  slice: "$items[0:3]"                  # Slice
```

### YAML Quoting Rules

JSONPath expressions containing `[`, `*`, `?`, or `@` must be YAML-quoted
(double or single quotes). Simple dot paths do not need quoting:

```yaml
use:
  simple: $user.name               # No quotes needed
  complex: "$items[?@.active]"     # Quotes required (special chars)
```

---

## 7. Design Area 4: Transform Pipeline

### Design Philosophy

Nika should offer three tiers of transformation power:

| Tier | Syntax | Power | Use Case |
|------|--------|-------|----------|
| **Built-in** | `\| upper` | Low | Common string/array operations |
| **Path** | `$task.items[?@.active]` | Medium | JSON navigation and filtering |
| **JQ** | `\| jq('.items \| length')` | High | Complex data transformation |

Tier 1 (built-in) and Tier 2 (path) should be available everywhere. Tier 3 (JQ)
is an optional dependency behind a feature flag.

### Built-in Transform Functions

A small set of pure, sandboxed transform functions:

```rust
/// A transform operation applied to a resolved binding value
#[derive(Debug, Clone, PartialEq)]
pub enum TransformOp {
    // String transforms
    Upper,
    Lower,
    Trim,

    // Collection transforms
    Length,
    First,
    Last,
    Keys,
    Values,
    Flatten,
    Reverse,
    Sort,
    Unique,

    // Type coercion
    ToString,
    ToNumber,
    ToJson,
    ParseJson,

    // Numeric
    Round(Option<u32>),  // decimal places
    Abs,

    // Utility
    Default(Value),      // fallback if null
    TypeOf,              // returns type name string

    // JQ escape hatch (feature-gated)
    #[cfg(feature = "jq")]
    Jq(Arc<str>),
}

/// A pipeline of transforms: value | op1 | op2 | op3
#[derive(Debug, Clone)]
pub struct TransformExpr {
    pub ops: SmallVec<[TransformOp; 2]>,
}

impl TransformExpr {
    /// Apply the transform pipeline to a value
    pub fn apply(&self, mut value: Value) -> Result<Value, TransformError> {
        for op in &self.ops {
            value = match op {
                TransformOp::Upper => match value {
                    Value::String(s) => Value::String(s.to_uppercase()),
                    other => return Err(TransformError::TypeMismatch {
                        op: "upper",
                        expected: "string",
                        got: value_type_name(&other),
                    }),
                },
                TransformOp::Lower => match value {
                    Value::String(s) => Value::String(s.to_lowercase()),
                    other => return Err(TransformError::TypeMismatch {
                        op: "lower",
                        expected: "string",
                        got: value_type_name(&other),
                    }),
                },
                TransformOp::Length => match &value {
                    Value::String(s) => Value::Number(s.len().into()),
                    Value::Array(a) => Value::Number(a.len().into()),
                    Value::Object(o) => Value::Number(o.len().into()),
                    other => return Err(TransformError::TypeMismatch {
                        op: "length",
                        expected: "string, array, or object",
                        got: value_type_name(other),
                    }),
                },
                TransformOp::First => match value {
                    Value::Array(mut a) if !a.is_empty() => a.remove(0),
                    Value::Array(_) => Value::Null,
                    other => return Err(TransformError::TypeMismatch {
                        op: "first",
                        expected: "array",
                        got: value_type_name(&other),
                    }),
                },
                TransformOp::Last => match value {
                    Value::Array(mut a) if !a.is_empty() => a.pop().unwrap(),
                    Value::Array(_) => Value::Null,
                    other => return Err(TransformError::TypeMismatch {
                        op: "last",
                        expected: "array",
                        got: value_type_name(&other),
                    }),
                },
                TransformOp::Keys => match value {
                    Value::Object(o) => {
                        Value::Array(o.keys().map(|k| Value::String(k.clone())).collect())
                    }
                    other => return Err(TransformError::TypeMismatch {
                        op: "keys",
                        expected: "object",
                        got: value_type_name(&other),
                    }),
                },
                TransformOp::Values => match value {
                    Value::Object(o) => Value::Array(o.into_iter().map(|(_, v)| v).collect()),
                    other => return Err(TransformError::TypeMismatch {
                        op: "values",
                        expected: "object",
                        got: value_type_name(&other),
                    }),
                },
                TransformOp::Flatten => match value {
                    Value::Array(a) => {
                        let mut flat = Vec::new();
                        for item in a {
                            if let Value::Array(inner) = item {
                                flat.extend(inner);
                            } else {
                                flat.push(item);
                            }
                        }
                        Value::Array(flat)
                    }
                    other => return Err(TransformError::TypeMismatch {
                        op: "flatten",
                        expected: "array",
                        got: value_type_name(&other),
                    }),
                },
                TransformOp::ToString => {
                    Value::String(match &value {
                        Value::String(s) => s.clone(),
                        Value::Null => "null".to_string(),
                        Value::Bool(b) => b.to_string(),
                        Value::Number(n) => n.to_string(),
                        other => serde_json::to_string(other)
                            .unwrap_or_else(|_| "null".to_string()),
                    })
                }
                TransformOp::ToNumber => match &value {
                    Value::Number(_) => value,
                    Value::String(s) => {
                        if let Ok(n) = s.parse::<i64>() {
                            Value::Number(n.into())
                        } else if let Ok(n) = s.parse::<f64>() {
                            Value::Number(serde_json::Number::from_f64(n)
                                .unwrap_or(0.into()))
                        } else {
                            return Err(TransformError::ConversionFailed {
                                op: "to_number",
                                value: s.clone(),
                            });
                        }
                    }
                    other => return Err(TransformError::TypeMismatch {
                        op: "to_number",
                        expected: "string or number",
                        got: value_type_name(other),
                    }),
                },
                TransformOp::ParseJson => match value {
                    Value::String(s) => serde_json::from_str(&s)
                        .map_err(|e| TransformError::ParseFailed {
                            op: "parse_json",
                            reason: e.to_string(),
                        })?,
                    other => other, // Already parsed
                },
                TransformOp::ToJson => {
                    Value::String(serde_json::to_string(&value)
                        .unwrap_or_else(|_| "null".to_string()))
                }
                TransformOp::Round(places) => match value {
                    Value::Number(n) => {
                        let f = n.as_f64().unwrap_or(0.0);
                        let p = places.unwrap_or(0);
                        let factor = 10f64.powi(p as i32);
                        let rounded = (f * factor).round() / factor;
                        Value::Number(serde_json::Number::from_f64(rounded)
                            .unwrap_or(0.into()))
                    }
                    other => return Err(TransformError::TypeMismatch {
                        op: "round",
                        expected: "number",
                        got: value_type_name(&other),
                    }),
                },
                TransformOp::Abs => match value {
                    Value::Number(n) => {
                        if let Some(i) = n.as_i64() {
                            Value::Number(i.unsigned_abs().into())
                        } else if let Some(f) = n.as_f64() {
                            Value::Number(serde_json::Number::from_f64(f.abs())
                                .unwrap_or(0.into()))
                        } else {
                            value
                        }
                    }
                    other => return Err(TransformError::TypeMismatch {
                        op: "abs",
                        expected: "number",
                        got: value_type_name(&other),
                    }),
                },
                TransformOp::Reverse => match value {
                    Value::Array(mut a) => {
                        a.reverse();
                        Value::Array(a)
                    }
                    Value::String(s) => {
                        Value::String(s.chars().rev().collect())
                    }
                    other => return Err(TransformError::TypeMismatch {
                        op: "reverse",
                        expected: "array or string",
                        got: value_type_name(&other),
                    }),
                },
                TransformOp::Sort => match value {
                    Value::Array(mut a) => {
                        a.sort_by(|a, b| {
                            a.to_string().cmp(&b.to_string())
                        });
                        Value::Array(a)
                    }
                    other => return Err(TransformError::TypeMismatch {
                        op: "sort",
                        expected: "array",
                        got: value_type_name(&other),
                    }),
                },
                TransformOp::Unique => match value {
                    Value::Array(a) => {
                        let mut seen = FxHashSet::default();
                        let unique: Vec<Value> = a.into_iter()
                            .filter(|v| seen.insert(v.to_string()))
                            .collect();
                        Value::Array(unique)
                    }
                    other => return Err(TransformError::TypeMismatch {
                        op: "unique",
                        expected: "array",
                        got: value_type_name(&other),
                    }),
                },
                TransformOp::Trim => match value {
                    Value::String(s) => Value::String(s.trim().to_string()),
                    other => return Err(TransformError::TypeMismatch {
                        op: "trim",
                        expected: "string",
                        got: value_type_name(&other),
                    }),
                },
                TransformOp::Default(fallback) => {
                    if value.is_null() {
                        fallback.clone()
                    } else {
                        value
                    }
                }
                TransformOp::TypeOf => {
                    Value::String(value_type_name(&value).to_string())
                }
                #[cfg(feature = "jq")]
                TransformOp::Jq(filter) => {
                    apply_jq_filter(&value, filter)?
                }
            };
        }
        Ok(value)
    }
}
```

### YAML Syntax for Transforms

Transforms use pipe syntax in string values or an explicit `transform:` field:

```yaml
use:
  # Pipe syntax in shorthand (parsed from string)
  count: "$items | length"
  names: "$users[*].name | sort | unique"
  label: "$product.name | upper"

  # Explicit transform field in canonical form
  summary:
    from: $analysis.text
    transform: "lower | trim"
    type: string

  # JQ escape hatch (feature-gated)
  complex:
    from: $data
    transform: "jq('.items | map(select(.active)) | length')"
```

### Transform Parser

Parse the pipe-separated transform string into a `TransformExpr`:

```rust
impl TransformExpr {
    /// Parse a transform expression string like "upper | trim | length"
    pub fn parse(input: &str) -> Result<Self, TransformParseError> {
        let mut ops = SmallVec::new();

        for part in input.split('|').map(str::trim) {
            if part.is_empty() {
                continue;
            }

            let op = match part {
                "upper" => TransformOp::Upper,
                "lower" => TransformOp::Lower,
                "trim" => TransformOp::Trim,
                "length" | "len" => TransformOp::Length,
                "first" => TransformOp::First,
                "last" => TransformOp::Last,
                "keys" => TransformOp::Keys,
                "values" => TransformOp::Values,
                "flatten" => TransformOp::Flatten,
                "reverse" | "rev" => TransformOp::Reverse,
                "sort" => TransformOp::Sort,
                "unique" | "uniq" => TransformOp::Unique,
                "to_string" | "tostring" | "str" => TransformOp::ToString,
                "to_number" | "tonumber" | "num" => TransformOp::ToNumber,
                "to_json" | "tojson" | "json" => TransformOp::ToJson,
                "parse_json" | "fromjson" => TransformOp::ParseJson,
                "abs" => TransformOp::Abs,
                "type" | "typeof" => TransformOp::TypeOf,
                s if s.starts_with("round") => {
                    let places = parse_round_arg(s)?;
                    TransformOp::Round(places)
                }
                s if s.starts_with("default(") => {
                    let fallback = parse_default_arg(s)?;
                    TransformOp::Default(fallback)
                }
                #[cfg(feature = "jq")]
                s if s.starts_with("jq(") => {
                    let filter = parse_jq_arg(s)?;
                    TransformOp::Jq(Arc::from(filter))
                }
                unknown => return Err(TransformParseError::UnknownFunction(
                    unknown.to_string(),
                )),
            };

            ops.push(op);
        }

        if ops.is_empty() {
            return Err(TransformParseError::Empty);
        }

        Ok(Self { ops })
    }
}

fn parse_round_arg(s: &str) -> Result<Option<u32>, TransformParseError> {
    if s == "round" {
        return Ok(None); // round to integer
    }
    // "round(2)" -> Some(2)
    let inner = s.strip_prefix("round(")
        .and_then(|s| s.strip_suffix(')'))
        .ok_or(TransformParseError::InvalidSyntax(s.to_string()))?;
    let places: u32 = inner.parse()
        .map_err(|_| TransformParseError::InvalidArgument {
            function: "round",
            argument: inner.to_string(),
        })?;
    Ok(Some(places))
}

fn parse_default_arg(s: &str) -> Result<Value, TransformParseError> {
    let inner = s.strip_prefix("default(")
        .and_then(|s| s.strip_suffix(')'))
        .ok_or(TransformParseError::InvalidSyntax(s.to_string()))?;

    // Try parsing as JSON value
    serde_json::from_str(inner)
        .or_else(|_| {
            // Bare string (no quotes) -> treat as string value
            Ok(Value::String(inner.to_string()))
        })
}
```

### Integration with Template Resolution

Transforms execute AFTER path resolution but BEFORE template interpolation:

```
Step 1: Parse UseEntry (YAML -> UseEntry with BindingPath + TransformExpr)
Step 2: Resolve BindingPath (navigate DataStore to get raw Value)
Step 3: Apply TransformExpr (pipe the Value through transform ops)
Step 4: Type-check (validate result against BindingType)
Step 5: Template interpolation (insert string representation into prompt)
```

---

## 8. Design Area 5: Lifecycle Type-State

### Why Not Phantom Types

As discussed in Section 3.5, phantom type-state does not work for `FxHashMap`-stored
bindings with mixed resolution states. The existing `LazyBinding` enum is correct.

### Enhanced LazyBinding

Extend `LazyBinding` to carry more resolution context:

```rust
/// The resolution state of a single binding
#[derive(Debug, Clone)]
pub enum BindingState {
    /// Value resolved eagerly during binding construction
    Resolved {
        value: Arc<Value>,
        source_task: Arc<str>,
        resolved_at: std::time::Instant,
    },

    /// Value deferred until first access
    Pending {
        source: BindingPath,
        default: Option<Value>,
        transform: Option<TransformExpr>,
        binding_type: BindingType,
    },

    /// Resolution failed with an error
    Failed {
        source: BindingPath,
        error: Arc<BindingResolutionError>,
    },
}

impl BindingState {
    /// Get the resolved value, resolving lazily if needed
    pub fn resolve(
        &mut self,
        store: &DataStore,
        inputs: Option<&Value>,
    ) -> Result<&Value, BindingResolutionError> {
        match self {
            BindingState::Resolved { value, .. } => Ok(value),
            BindingState::Pending { source, default, transform, binding_type } => {
                let raw_value = resolve_from_store(source, store, inputs)?;

                // Apply default if null
                let value = match raw_value {
                    Some(v) => v,
                    None => match default {
                        Some(d) => d.clone(),
                        None => return Err(BindingResolutionError::NotFound {
                            path: source.to_string(),
                        }),
                    },
                };

                // Apply transform pipeline
                let value = if let Some(xform) = transform {
                    xform.apply(value)?
                } else {
                    value
                };

                // Type-check
                validate_type(&value, binding_type)?;

                // Transition to Resolved
                let arc_value = Arc::new(value);
                *self = BindingState::Resolved {
                    value: Arc::clone(&arc_value),
                    source_task: source.task_id.clone(),
                    resolved_at: std::time::Instant::now(),
                };

                match self {
                    BindingState::Resolved { value, .. } => Ok(value),
                    _ => unreachable!(),
                }
            }
            BindingState::Failed { error, .. } => {
                Err(error.as_ref().clone())
            }
        }
    }

    /// Check if this binding has been resolved
    pub fn is_resolved(&self) -> bool {
        matches!(self, BindingState::Resolved { .. })
    }

    /// Check if this binding failed to resolve
    pub fn is_failed(&self) -> bool {
        matches!(self, BindingState::Failed { .. })
    }
}
```

### ResolvedBindings with Interior Mutability

Since lazy bindings need to transition from `Pending` to `Resolved` on first access,
use `RefCell` or `parking_lot::Mutex` for interior mutability:

```rust
use parking_lot::Mutex;

/// Container for all resolved/pending bindings for a single task
pub struct ResolvedBindings {
    bindings: FxHashMap<Arc<str>, Mutex<BindingState>>,
}

impl ResolvedBindings {
    /// Get a binding value, resolving lazily if needed
    pub fn get(
        &self,
        alias: &str,
        store: &DataStore,
        inputs: Option<&Value>,
    ) -> Result<Arc<Value>, BindingResolutionError> {
        let binding = self.bindings.get(alias)
            .ok_or(BindingResolutionError::UnknownAlias(alias.to_string()))?;

        let mut state = binding.lock();
        state.resolve(store, inputs)?;

        match &*state {
            BindingState::Resolved { value, .. } => Ok(Arc::clone(value)),
            _ => unreachable!("resolve() succeeded but state is not Resolved"),
        }
    }

    /// Get all bindings as a serializable map (for event logging)
    pub fn to_value(&self) -> Value {
        let mut map = serde_json::Map::new();
        for (alias, binding) in &self.bindings {
            let state = binding.lock();
            let v = match &*state {
                BindingState::Resolved { value, .. } => value.as_ref().clone(),
                BindingState::Pending { source, .. } => json!({
                    "__pending__": true,
                    "source": source.to_string(),
                }),
                BindingState::Failed { error, .. } => json!({
                    "__failed__": true,
                    "error": error.to_string(),
                }),
            };
            map.insert(alias.to_string(), v);
        }
        Value::Object(map)
    }
}
```

### Note on Current Design

The current `ResolvedBindings` intentionally does NOT cache lazy resolution
(each `get_resolved()` call re-resolves from the DataStore). The design above
introduces caching via the `Pending -> Resolved` state transition. This is a
trade-off: caching is faster but means bindings see a snapshot, not live values.

For Nika's use case (task bindings resolve once, then used in templates), caching
is correct. If a use case for "live" bindings emerges, add a `watch: true` mode
later.

---

## 9. Design Area 6: Output Contracts

### Extending output_policy to All Task Types

Currently, `output_policy` only works on `infer:` tasks for structured output
enforcement. Extend it to all 5 verbs:

```yaml
tasks:
  - id: fetch_weather
    fetch:
      url: "https://api.weather.com/v1/current"
      method: GET
    output:
      schema:
        type: object
        required: [temperature, humidity]
        properties:
          temperature: { type: number }
          humidity: { type: number }
          description: { type: string }

  - id: use_weather
    use:
      temp: $fetch_weather.temperature    # Validated at parse-time
      humid: $fetch_weather.humidity      # via output schema
    infer: "Temperature is {{use.temp}}F with {{use.humid}}% humidity"
```

### OutputContract Type

```rust
/// Output contract for a task -- defines what the task produces
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputContract {
    /// JSON Schema for the output value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<Value>,

    /// Named fields with individual schemas
    /// Enables: use: { price: $task.price }
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields: Option<FxHashMap<String, FieldContract>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldContract {
    /// JSON Schema for this field
    pub schema: Value,
    /// Human-readable description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl OutputContract {
    /// Validate a task output against this contract
    pub fn validate(&self, output: &Value) -> Result<(), OutputValidationError> {
        if let Some(schema) = &self.schema {
            validate_json_schema(output, schema)?;
        }

        if let Some(fields) = &self.fields {
            let obj = output.as_object()
                .ok_or(OutputValidationError::ExpectedObject)?;

            for (name, contract) in fields {
                if let Some(value) = obj.get(name) {
                    validate_json_schema(value, &contract.schema)?;
                }
                // Missing fields are OK if not in "required" in the schema
            }
        }

        Ok(())
    }

    /// Check if a path expression is valid against this contract
    pub fn validate_path(&self, segments: &[PathSegment]) -> PathValidation {
        if segments.is_empty() {
            return PathValidation::Valid;
        }

        // Try to navigate the schema following the path segments
        match &self.schema {
            Some(schema) => navigate_schema(schema, segments),
            None => PathValidation::Unknown, // No schema, cannot validate
        }
    }
}

pub enum PathValidation {
    Valid,                    // Path exists in schema
    Unknown,                 // No schema to check against
    Warning(String),         // Path not found but schema is permissive
    Invalid(String),         // Path definitely wrong (schema is strict)
}
```

### Analyzer Integration

During Phase 2 analysis, build a registry of output contracts and validate
bindings against them:

```rust
/// In the analyzer, after all tasks are parsed:
fn validate_binding_compatibility(
    tasks: &TaskTable,
    wiring: &WiringSpec,
    contracts: &FxHashMap<TaskId, OutputContract>,
) -> Vec<AnalyzeWarning> {
    let mut warnings = Vec::new();

    for (alias, entry) in wiring {
        if let Some(contract) = contracts.get(&entry.source.task_id) {
            match contract.validate_path(&entry.source.segments) {
                PathValidation::Valid => {}
                PathValidation::Unknown => {}
                PathValidation::Warning(msg) => {
                    warnings.push(AnalyzeWarning::BindingPathWarning {
                        alias: alias.to_string(),
                        path: entry.source.to_string(),
                        message: msg,
                    });
                }
                PathValidation::Invalid(msg) => {
                    warnings.push(AnalyzeWarning::BindingPathInvalid {
                        alias: alias.to_string(),
                        path: entry.source.to_string(),
                        message: msg,
                    });
                }
            }
        }
    }

    warnings
}
```

---

## 10. Design Area 7: Zero-Copy & Performance

### Arc<Value> for Shared Ownership

Task outputs are read by multiple downstream tasks. Use `Arc<Value>` to avoid
cloning:

```rust
/// Task result stored in the DataStore
pub struct TaskResult {
    /// The task's output value (shared across all consumers)
    pub output: Arc<Value>,
    /// Named output fields (for structured outputs)
    pub fields: Option<FxHashMap<Arc<str>, Arc<Value>>>,
    /// Task completion metadata
    pub metadata: TaskResultMetadata,
}
```

### Cow<str> for Template Strings

The current `template::resolve()` already returns `Cow<str>`:

```rust
pub fn resolve<'a>(
    template: &'a str,
    bindings: &ResolvedBindings,
    store: &DataStore,
    context: Option<&Value>,
    inputs: Option<&Value>,
) -> Result<Cow<'a, str>, SmallVec<[NikaError; 4]>> {
    // Returns Cow::Borrowed when no templates found (zero alloc)
}
```

This is already optimal. No changes needed.

### Arc<str> for Frequently-Used Strings

Task IDs, aliases, and path segments are used repeatedly. Interning them with
`Arc<str>` avoids repeated allocations:

```rust
pub struct BindingPath {
    pub task_id: Arc<str>,      // Interned task ID
    pub segments: Vec<PathSegment>,
}

pub enum PathSegment {
    Field(Arc<str>),            // Interned field names
    Index(usize),
}

pub type WiringSpec = FxHashMap<Arc<str>, UseEntry>;  // Interned aliases
```

### SmallVec for Transform Pipelines

Most transform pipelines have 1-2 operations. Use `SmallVec<[TransformOp; 2]>`
to avoid heap allocation:

```rust
pub struct TransformExpr {
    pub ops: SmallVec<[TransformOp; 2]>,  // Stack-allocated up to 2 ops
}
```

### Compiled Path Cache

JSONPath compilation is non-trivial. Cache compiled paths globally
(see Section 6 for the `PATH_CACHE` implementation).

### Performance Budget

| Operation | Current | Target | Method |
|-----------|---------|--------|--------|
| UseEntry parse | ~450ns | ~500ns | Acceptable: adding type/transform parsing |
| Path resolution | ~6ns (DataStore get) | ~6ns | No change (DashMap) |
| JSONPath apply | ~800ns (custom) | ~200ns | serde_json_path returns refs |
| Transform | N/A | <1us per op | Built-in ops are O(n) on value size |
| Template resolve | ~4us (regex) | ~4us | No change |

---

## 11. YAML Serialization Layer

### Custom Deserializer for UseEntry

The deserializer must handle three forms:

```yaml
use:
  # Form 1: Simple string (backwards compatible)
  result: $step1.field

  # Form 2: String with default (backwards compatible)
  result: "$step1.field ?? fallback"

  # Form 3: String with transform (NEW)
  count: "$items | length"

  # Form 4: Full object (NEW canonical form)
  price:
    from: $product.price
    type: number
    default: 0
    lazy: true
    transform: "round(2)"
```

```rust
impl<'de> Deserialize<'de> for UseEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct UseEntryVisitor;

        impl<'de> serde::de::Visitor<'de> for UseEntryVisitor {
            type Value = UseEntry;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a string path or a binding object with 'from' field")
            }

            fn visit_str<E>(self, value: &str) -> Result<UseEntry, E>
            where
                E: serde::de::Error,
            {
                parse_string_binding(value).map_err(E::custom)
            }

            fn visit_map<M>(self, map: M) -> Result<UseEntry, M::Error>
            where
                M: serde::de::MapAccess<'de>,
            {
                parse_object_binding(map)
            }
        }

        deserializer.deserialize_any(UseEntryVisitor)
    }
}

/// Parse a string-form binding
/// Handles: "$step.field", "$step.field ?? default", "$step.field | transform"
fn parse_string_binding(input: &str) -> Result<UseEntry, BindingParseError> {
    let input = input.trim();

    // Step 1: Split on ?? for default (respecting quotes)
    let (path_and_transform, default) = split_default(input)?;

    // Step 2: Split on | for transforms (only if not inside path brackets)
    let (path_str, transform_str) = split_transform(path_and_transform)?;

    // Step 3: Parse the path
    let source = BindingPath::parse(path_str.trim())?;

    // Step 4: Parse transform if present
    let transform = if let Some(xform) = transform_str {
        Some(TransformExpr::parse(xform.trim())?)
    } else {
        None
    };

    // Step 5: Parse default if present
    let default_value = if let Some(default_str) = default {
        Some(parse_default_value(default_str.trim())?)
    } else {
        None
    };

    Ok(UseEntry {
        source,
        binding_type: BindingType::Any,
        default: default_value,
        lazy: false,
        transform,
    })
}

/// Split a binding string on the ?? operator, respecting quotes and JSON
fn split_default(input: &str) -> Result<(&str, Option<&str>), BindingParseError> {
    // Reuse existing find_operator_outside_quotes logic
    if let Some(pos) = find_operator_outside_quotes(input, "??") {
        let path_part = &input[..pos];
        let default_part = &input[pos + 2..];
        Ok((path_part, Some(default_part)))
    } else {
        Ok((input, None))
    }
}

/// Split a binding string on | for transforms
/// Must not split inside JSONPath bracket expressions like [?@.x]
fn split_transform(input: &str) -> Result<(&str, Option<&str>), BindingParseError> {
    // Find first | that is not inside brackets or quotes
    let mut depth = 0;
    let mut in_quotes = false;

    for (i, ch) in input.char_indices() {
        match ch {
            '"' | '\'' => in_quotes = !in_quotes,
            '[' if !in_quotes => depth += 1,
            ']' if !in_quotes => depth -= 1,
            '|' if !in_quotes && depth == 0 => {
                return Ok((&input[..i], Some(&input[i + 1..])));
            }
            _ => {}
        }
    }

    Ok((input, None))
}
```

### Serialization

For round-trip capability and event logging:

```rust
impl Serialize for UseEntry {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // Use shorthand string form when possible
        if self.binding_type == BindingType::Any
            && !self.lazy
            && self.transform.is_none()
        {
            let mut s = self.source.to_string();
            if let Some(default) = &self.default {
                s.push_str(" ?? ");
                s.push_str(&serde_json::to_string(default)
                    .unwrap_or_else(|_| "null".to_string()));
            }
            return serializer.serialize_str(&s);
        }

        // Otherwise, use object form
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("from", &self.source.to_string())?;
        if self.binding_type != BindingType::Any {
            map.serialize_entry("type", &self.binding_type)?;
        }
        if let Some(default) = &self.default {
            map.serialize_entry("default", default)?;
        }
        if self.lazy {
            map.serialize_entry("lazy", &true)?;
        }
        if let Some(transform) = &self.transform {
            map.serialize_entry("transform", &transform.to_string())?;
        }
        map.end()
    }
}
```

---

## 12. Migration Strategy

### Phase 1: Non-Breaking Additions (v0.28)

Add `type`, `from`, and `transform` fields as optional. Keep full backwards
compatibility:

```yaml
# All existing syntax continues to work unchanged
use:
  result: step1.output              # Still works
  result: step1.output ?? "default" # Still works
  result:
    path: step1.output              # 'path' still accepted (deprecated)
    lazy: true                      # Still works
    default: "fallback"             # Still works

# New syntax is opt-in
use:
  result:
    from: $step1.output             # New 'from' field
    type: string                    # New type hint
    transform: "upper"              # New transform
```

### Phase 2: serde_json_path Integration (v0.29)

Replace `util/jsonpath.rs` with `serde_json_path`. Extend path expression support
in templates:

```yaml
# New path capabilities (v0.29)
use:
  names: "$users[*].name"           # Wildcard
  active: "$items[?@.active]"       # Filter
  deep: "$..email"                  # Recursive descent
```

### Phase 3: Transform Pipeline (v0.30)

Enable the built-in transform functions:

```yaml
# New transforms (v0.30)
use:
  count: "$items | length"
  label: "$product.name | upper"
  top3: "$scores | sort | reverse | first"
```

### Phase 4: Output Contracts (v0.31)

Extend `output:` to all task types with optional schema validation and binding
compatibility checking in the analyzer:

```yaml
tasks:
  - id: fetch_data
    fetch:
      url: "https://api.example.com/data"
    output:
      schema:
        type: object
        properties:
          items: { type: array }
          total: { type: integer }

  - id: process
    use:
      items: $fetch_data.items       # Validated against output schema
    infer: "Process {{use.items | length}} items"
```

### Phase 5: JQ Escape Hatch (v0.32, feature-gated)

Add `jq()` as an optional transform behind `--features jq`:

```yaml
use:
  complex:
    from: $data
    transform: "jq('.items | map(select(.price > 100)) | sort_by(.name)')"
```

### Deprecation Timeline

| Version | Change |
|---------|--------|
| v0.28 | `path:` accepted with deprecation warning, `from:` recommended |
| v0.29 | `path:` still works, log warning in analyzer |
| v0.30 | `path:` still works, warning in CLI output |
| v0.32 | `path:` removed, error if used |

---

## 13. Recommended Architecture

### Dependency Graph

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  BINDING MODULE ARCHITECTURE (v0.28+)                                       │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  binding/                                                                   │
│  ├── mod.rs          Re-exports, module docs                                │
│  ├── types.rs        BindingPath, BindingType, PathSegment (NEW)            │
│  ├── entry.rs        UseEntry, WiringSpec, serde (REFACTORED)               │
│  ├── transform.rs    TransformOp, TransformExpr, parser (NEW)               │
│  ├── resolve.rs      BindingState, ResolvedBindings (REFACTORED)            │
│  ├── template.rs     Template resolution (UNCHANGED)                        │
│  ├── contract.rs     OutputContract, FieldContract (NEW)                    │
│  ├── validate.rs     Task ID validation (UNCHANGED)                         │
│  └── mention.rs      Chat mention parsing (UNCHANGED)                       │
│                                                                             │
│  Dependencies:                                                              │
│  ├── serde_json_path  (replaces util/jsonpath.rs)                           │
│  ├── serde_json       (existing)                                            │
│  ├── rustc_hash       (existing, FxHashMap/FxHashSet)                       │
│  ├── smallvec         (existing)                                            │
│  ├── parking_lot      (for Mutex on BindingState)                           │
│  └── jaq-core         (optional, feature = "jq")                            │
│                                                                             │
│  Data Flow:                                                                 │
│                                                                             │
│  YAML use: block                                                            │
│       │                                                                     │
│       ▼                                                                     │
│  UseEntry { source: BindingPath, type: BindingType, transform: Option }     │
│       │                                                                     │
│       ├──▶ Analyzer: validate path against OutputContract (warnings)        │
│       │                                                                     │
│       ▼                                                                     │
│  BindingState::Pending { source, default, transform, type }                 │
│       │                                                                     │
│       ├──▶ Lazy: stays Pending until first access                           │
│       │                                                                     │
│       ▼  (on access)                                                        │
│  resolve_from_store(source, DataStore)                                      │
│       │                                                                     │
│       ▼                                                                     │
│  serde_json_path::query(value, path) → &Value                              │
│       │                                                                     │
│       ▼                                                                     │
│  TransformExpr::apply(value) → Value                                        │
│       │                                                                     │
│       ▼                                                                     │
│  validate_type(value, BindingType)                                          │
│       │                                                                     │
│       ▼                                                                     │
│  BindingState::Resolved { value: Arc<Value> }                               │
│       │                                                                     │
│       ▼                                                                     │
│  template::resolve("{{use.alias}}", bindings) → Cow<str>                    │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### New Crate Dependencies

| Crate | Version | Size | Purpose | Phase |
|-------|---------|------|---------|-------|
| `serde_json_path` | 0.7 | ~50KB | RFC 9535 JSONPath | v0.29 |
| `parking_lot` | 0.12 | ~30KB | Fast mutex for BindingState | v0.28 |
| `jaq-core` | 2.2 | ~200KB | JQ expressions (optional) | v0.32 |
| `jaq-std` | 2.2 | ~100KB | JQ stdlib (optional) | v0.32 |
| `jaq-parse` | 2.2 | ~80KB | JQ parser (optional) | v0.32 |
| `jaq-json` | 2.2 | ~20KB | JQ JSON integration (optional) | v0.32 |

Note: `parking_lot` may already be a transitive dependency (check Cargo.lock).
The jaq crates total ~400KB but are behind a feature flag.

### Test Strategy

| Test Category | Count (est.) | Description |
|---------------|-------------|-------------|
| BindingPath parsing | 30 | Path syntax variations, edge cases |
| BindingType validation | 20 | Type checking for all 7 variants |
| TransformOp (each op) | 50 | Each transform with valid/invalid inputs |
| TransformExpr pipeline | 15 | Multi-op pipelines, error propagation |
| UseEntry serde | 40 | All 4 YAML forms, round-trip |
| BindingState lifecycle | 20 | Pending -> Resolved, Failed states |
| OutputContract | 25 | Schema validation, path checking |
| Integration (full flow) | 30 | End-to-end: YAML -> resolve -> template |
| **Total** | **~230** | On top of existing 136 binding tests |

---

## Appendix: Decision Log

| Decision | Chosen | Rejected | Rationale |
|----------|--------|----------|-----------|
| Path engine | serde_json_path | jsonpath-rust, custom | RFC 9535 standard, returns refs |
| Transform engine | Built-in + jq opt-in | Full jq, CEL, custom DSL | Low barrier, high ceiling |
| State pattern | Enum (BindingState) | Type-state (PhantomData) | Mixed-state maps require enum |
| Interior mutability | parking_lot::Mutex | RefCell, std::Mutex | parking_lot is faster, no poisoning |
| Interning | Arc<str> | String, &str | Shared ownership without lifetimes |
| Transform syntax | Pipe `\| op` | Dot `.op()`, method chain | Consistent with shell/jq conventions |
| Default syntax | `?? value` | `\| default(value)` | Backwards compatible |
| Field rename | `from:` (new) | Keep `path:` | Natural English, no conflict |
| Type hints | Simple enum | JSON Schema on bindings | Ergonomic for 90% of cases |
| Output contracts | JSON Schema | Custom DSL | Standards-based, tooling exists |
| Cache strategy | Cache on resolve | Re-resolve every time | Single resolution per task execution |

---

## Open Questions

1. **Should transforms be allowed in template expressions too?**
   Currently proposed: transforms only in `use:` block. Should `{{use.name | upper}}`
   work inside templates? The current `|shell` modifier suggests yes, but adding
   a general transform pipeline to regex-based template resolution is complex.
   **Recommendation**: Yes, but limited to single-op transforms in templates. Multi-op
   pipelines only in `use:` block.

2. **Should type errors be hard errors or warnings?**
   LLM output is inherently unpredictable. A strict type mismatch error would break
   workflows that work 95% of the time. **Recommendation**: Warning by default,
   `strict_types: true` at workflow level to opt into errors.

3. **Should jq filters have a timeout?**
   A malicious or poorly-written jq filter could consume unbounded CPU.
   **Recommendation**: Yes, 5 second timeout on jq evaluation (configurable).

4. **How should array results from wildcard paths be interpolated in templates?**
   `$users[*].name` returns an array. In a template like `"Names: {{use.names}}"`,
   should it JSON-serialize the array or join with commas?
   **Recommendation**: JSON-serialize by default. Add `| join(",")` transform for
   comma-separated output.

5. **Should BindingPath support `inputs.*` and `context.*` prefixes?**
   Currently these are handled in template resolution (Pass 2 and 3). Should they
   be first-class in the `use:` block too?
   **Recommendation**: Not yet. Keep the 3-pass architecture. The `use:` block is
   specifically for inter-task bindings. Context and inputs have their own resolution
   passes.

---

*This document is a design analysis for discussion. No implementation should begin
without review and consensus on the phasing strategy.*
