# Master Implementation Plan: Binding + Import System Redesign

**Date**: 2026-03-13
**Status**: PLAN -- reviewed, 5 critical fixes applied
**Scope**: v0.28 -- all breaking changes shipping at once (no backwards compat)
**Predecessor**: `2026-03-13-binding-redesign-proposal.md` (approved)
**Tests target**: 6,157 existing − 18 deleted + ~323 new = ~6,462 total

---

## Table of Contents

1. [Overview](#overview)
2. [Crate Dependencies](#crate-dependencies)
3. [Implementation Phases](#implementation-phases)
   - [Phase 1: Core Types](#phase-1-core-types)
   - [Phase 2: Transform Engine](#phase-2-transform-engine)
   - [Phase 3: WithEntry + Serde](#phase-3-withentry--serde)
   - [Phase 4: Template Rewrite](#phase-4-template-rewrite)
   - [Phase 5: AST Changes](#phase-5-ast-changes)
   - [Phase 6: DAG Builder Rewrite](#phase-6-dag-builder-rewrite)
   - [Phase 7: Import System Redesign](#phase-7-import-system-redesign)
   - [Phase 8: Runtime Integration](#phase-8-runtime-integration)
   - [Phase 9: Rich JSONPath](#phase-9-rich-jsonpath)
   - [Phase 10: Output Contracts](#phase-10-output-contracts)
   - [Phase 11: JQ Escape Hatch](#phase-11-jq-escape-hatch)
   - [Phase 12: YAML + Schema + Test Migration](#phase-12-yaml--schema--test-migration)
4. [File Change Matrix](#file-change-matrix)
5. [Test Strategy](#test-strategy)
6. [Commit Strategy](#commit-strategy)
7. [Risk Assessment](#risk-assessment)

---

## Overview

This plan ships **everything in one release** (v0.28). No multi-release phasing.
Nika is v0 with zero users -- clean break, no backwards compat, no deprecation shims.

### What Changes

| System | Before | After |
|--------|--------|-------|
| **Bindings** | `use:` / `{{use.alias}}` / `path:` / 3-pass | `with:` / `{{alias}}` / `from:` / 2-pass |
| **Dependencies** | `flows:` section (explicit edges) | Implicit from `with:` $refs + `depends_on:` |
| **Transforms** | Only `\|shell` in templates | 27 built-in transforms + pipe chains |
| **Types** | None | `type:` field with 7 variants, errors by default |
| **Path Syntax** | `$task_id` only in `use:` | Unified `$task_id`, `$context.*`, `$inputs.*`, `$env.*` |
| **JSONPath** | Custom `util/jsonpath.rs` | RFC 9535 via `serde_json_path` |
| **Output Contracts** | None | JSON Schema on task outputs |
| **JQ** | None | Feature-gated `jq()` transform via `jaq-core` |
| **Import System** | `include:` + `skills:` separate | Unified `imports:` with kind-aware resolution |
| **Schema Version** | `nika/workflow@0.11` | `nika/workflow@0.12` |

### What Stays the Same

- 5 semantic verbs (infer, exec, fetch, invoke, agent)
- Workflow structure (schema, tasks, mcp, context)
- Event system (22 variants)
- TUI (4 views)
- Provider system (6 cloud + 1 native)
- MCP client (rmcp v0.16)
- Artifact system (io:: modules)
- rig-core integration

---

## Crate Dependencies

### New Dependencies (Cargo.toml)

```toml
[dependencies]
# JSONPath (RFC 9535) -- replaces custom util/jsonpath.rs
serde_json_path = "0.7"     # RFC 9535, returns refs, 600+ tests, maintained

# JQ transforms (optional, feature-gated)
jaq-core = { version = "2.0", optional = true }
jaq-std = { version = "2.0", optional = true }
jaq-parse = { version = "2.0", optional = true }

# Already in Cargo.toml (confirm versions):
# smallvec = "1.13"         # SmallVec<[TransformOp; 2]>
# rustc-hash = "2.0"        # FxHashMap for WithSpec
# serde = { version = "1", features = ["derive"] }
# serde_json = "1"
# regex = "1"
# parking_lot = "0.12"      # Already used via rig-core/rmcp deps
# compact_str / arcstr       # NOT adding -- Arc<str> is sufficient

[features]
jq = ["dep:jaq-core", "dep:jaq-std", "dep:jaq-parse"]
```

### Crate Selection Rationale

| Crate | Why | Alternatives Rejected |
|-------|-----|----------------------|
| `serde_json_path` | RFC 9535 compliance, returns `&Value` refs (zero-copy), 600+ tests, 30K downloads/mo | `jsonpath-rust` (non-standard), `jsonpath_lib` (unmaintained) |
| `jaq-core` 2.0 | Full jq spec, compile-once execute-many, Rust-native, 1,500+ tests | `jaq-interpret` (old API), `jq-rs` (C bindings) |
| `smallvec` | Already used in dag/flow.rs for DepVec, stack-alloc for small transform chains | `tinyvec` (API less ergonomic) |
| `parking_lot` | Already transitive dep (via rmcp/rig-core), faster than std::Mutex | `std::sync::Mutex` (poisoning) |

---

## Error Types and Null Handling

### New Error Code Range: NIKA-150-169 (Binding Redesign)

These error codes cover all new error types introduced by the binding + import redesign.
They fit in the gap between NIKA-140-149 (AST analysis) and NIKA-200-209 (File Tools).

```rust
// src/error.rs additions

// ═══════════════════════════════════════════
// BINDING REDESIGN ERRORS (150-169)
// ═══════════════════════════════════════════

#[error("[NIKA-150] Invalid binding path: {input} — {reason}")]
#[diagnostic(code(nika::binding_path_error), help("Binding paths must start with $"))]
BindingPathError { input: String, reason: String },

#[error("[NIKA-151] Transform parse error in '{input}': {reason}")]
#[diagnostic(code(nika::transform_parse_error), help("Check transform syntax: 'sort | unique | first(3)'"))]
TransformParseError { input: String, reason: String },

#[error("[NIKA-152] Transform '{op}' failed: expected {expected}, got {got}")]
#[diagnostic(code(nika::transform_type_mismatch))]
TransformTypeMismatch { op: String, expected: String, got: String },

#[error("[NIKA-153] Transform '{op}' received null — {policy}")]
#[diagnostic(code(nika::transform_null_input), help("Use default() transform or ?? operator to handle nulls"))]
TransformNullInput { op: String, policy: String },

#[error("[NIKA-154] Binding type mismatch for '{alias}': expected {expected}, got {got}")]
#[diagnostic(code(nika::binding_type_error))]
BindingTypeError { alias: String, expected: String, got: String },

#[error("[NIKA-155] WithEntry parse error in '{input}': {reason}")]
#[diagnostic(code(nika::with_entry_parse_error), help("Format: '$task.field | transform ?? default'"))]
WithEntryParseError { input: String, reason: String },

#[error("[NIKA-156] Import error: {reason}")]
#[diagnostic(code(nika::import_error))]
ImportError { reason: String },

#[error("[NIKA-157] Circular import detected: {path}")]
#[diagnostic(code(nika::circular_import), help("Check import chain for cycles"))]
CircularImport { path: String },

#[error("[NIKA-158] Import missing 'as' alias for {kind}: {path}")]
#[diagnostic(code(nika::missing_import_alias), help("Add 'as: alias_name' to the import"))]
MissingImportAlias { kind: String, path: String },

#[error("[NIKA-159] JSONPath error in '{path}': {cause}")]
#[diagnostic(code(nika::jsonpath_error))]
JsonPathError { path: String, cause: String },

#[error("[NIKA-160] Output contract validation failed: {errors}")]
#[diagnostic(code(nika::output_contract_failed))]
OutputContractFailed { errors: String },

#[error("[NIKA-161] Template expression error: {reason}")]
#[diagnostic(code(nika::template_expr_error))]
TemplateExprError { reason: String },

#[cfg(feature = "jq")]
#[error("[NIKA-162] JQ evaluation error in '{expr}': {cause}")]
#[diagnostic(code(nika::jq_error))]
JqError { expr: String, cause: String },
```

### Null Handling Policy

Every transform has explicit null behavior. There are **two categories**:

| Category | Behavior | Example |
|----------|----------|---------|
| **Propagating** | Null in → Null out (no error) | `length`, `keys`, `type_of`, `to_string`, `to_json` |
| **Failing** | Null in → NIKA-153 error | `upper`, `lower`, `trim`, `sort`, `split`, `join`, `round`, `abs` |

The `default()` transform and `??` operator are the escape hatches:

```yaml
with:
  # Safe: default() runs BEFORE other transforms
  name: $step1.name | default("Unknown") | upper

  # Safe: ?? sets default when entire chain is null
  count: $step1.items | length ?? 0
```

**Precedence**: `??` default is applied AFTER the transform chain.
`default()` as a transform is applied at its position in the chain.

```
$step1.items | sort | first ?? "none"
  1. Resolve $step1.items → null (task failed)
  2. sort(null) → NIKA-153 error (sort is "failing" category)

$step1.items | default([]) | sort | first ?? "none"
  1. Resolve $step1.items → null
  2. default([])(null) → []
  3. sort([]) → []
  4. first([]) → null
  5. ?? "none" → "none"
```

### Error Type Summary

| Error Type | NIKA Code | Used In |
|------------|-----------|---------|
| `BindingPathError` | NIKA-150 | `BindingPath::parse()` in types.rs |
| `TransformParseError` | NIKA-151 | `TransformExpr::parse()` in transform.rs |
| `TransformTypeMismatch` | NIKA-152 | `TransformOp::apply()` in transform.rs |
| `TransformNullInput` | NIKA-153 | `TransformOp::apply()` in transform.rs |
| `BindingTypeError` | NIKA-154 | `validate_type()` in resolve.rs |
| `WithEntryParseError` | NIKA-155 | `parse_with_entry()` in entry.rs |
| `ImportError` | NIKA-156 | `resolve_imports()` in import_loader.rs |
| `CircularImport` | NIKA-157 | `resolve_imports()` in import_loader.rs |
| `MissingImportAlias` | NIKA-158 | `resolve_imports()` in import_loader.rs |
| `JsonPathError` | NIKA-159 | `query()` in jsonpath.rs |
| `OutputContractFailed` | NIKA-160 | `validate_output()` in output_contract.rs |
| `TemplateExprError` | NIKA-161 | `parse_template_expr()` in template.rs |
| `JqError` | NIKA-162 | `evaluate()` in jq.rs (feature-gated) |

---

## Implementation Phases

### Phase 1: Core Types

**New file**: `src/binding/types.rs`
**Estimated tests**: 45

Create the foundational types that everything else depends on.

#### 1.1 BindingPath

```rust
// src/binding/types.rs

use std::sync::Arc;
use std::fmt;
use serde::Deserialize;

/// Source path for a binding -- parsed from "$task_id.field.path"
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BindingPath {
    /// Where the data comes from
    pub source: BindingSource,
    /// Property access segments after the source
    pub segments: Vec<PathSegment>,
}

/// Where data originates
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BindingSource {
    /// Task output: $task_id
    Task(Arc<str>),
    /// Context file: $context.files.brand or $context.session
    Context(Arc<str>),
    /// Workflow input: $inputs.locale
    Input(Arc<str>),
    /// Environment variable: $env.API_URL
    Env(Arc<str>),
    /// Loop variable: $item (from for_each as:)
    LoopVar(Arc<str>),
}

/// Single segment in a property path
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PathSegment {
    /// Named field: .name
    Field(Arc<str>),
    /// Array index: [0]
    Index(usize),
}
```

#### 1.2 BindingType

```rust
/// Type constraint on a binding value
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BindingType {
    #[default]
    Any,
    String,
    Number,
    Integer,
    Boolean,
    Array,
    Object,
}
```

#### 1.3 Parsing

```rust
impl BindingPath {
    /// Parse a binding path string like "$task_id.field[0].name"
    ///
    /// Reserved namespaces: context, inputs, env
    /// Everything else is a task reference or loop variable.
    pub fn parse(input: &str) -> Result<Self, BindingPathError> { ... }

    /// Extract the task ID if this is a Task source
    pub fn task_id(&self) -> Option<&Arc<str>> { ... }

    /// Returns true if this binding references a task output
    pub fn is_task_ref(&self) -> bool { ... }
}

impl BindingSource {
    /// Returns true if this is a task reference
    pub fn is_task(&self) -> bool { ... }
}

impl fmt::Display for BindingPath { ... }
impl fmt::Display for BindingSource { ... }
impl fmt::Display for PathSegment { ... }
```

#### 1.4 Tests for Phase 1

```
test_parse_simple_task_ref()           "$step1"
test_parse_task_with_field()           "$step1.output"
test_parse_task_deep_path()            "$step1.data.items[0].name"
test_parse_context_file()              "$context.files.brand"
test_parse_context_session()           "$context.session"
test_parse_input()                     "$inputs.locale"
test_parse_input_nested()              "$inputs.config.theme"
test_parse_env()                       "$env.API_URL"
test_parse_loop_var()                  "$item" with LoopVar hint
test_parse_index_segment()             "$data[0]"
test_parse_multiple_indexes()          "$data[0].items[1]"
test_parse_missing_dollar()            "step1" -> error
test_parse_empty()                     "$" -> error
test_parse_reserved_context()          context -> BindingSource::Context
test_parse_reserved_inputs()           inputs -> BindingSource::Input
test_parse_reserved_env()              env -> BindingSource::Env
test_display_roundtrip()               parse -> display -> parse = same
test_task_id_extraction()              $step1.field -> Some("step1")
test_is_task_ref()                     Task vs Context vs Input
test_binding_type_default()            BindingType::Any
test_binding_type_deserialize()        "string" -> BindingType::String
test_binding_type_all_variants()       Each JSON string -> correct variant
```

---

### Phase 2: Transform Engine

**New file**: `src/binding/transform.rs`
**Estimated tests**: 60

#### 2.1 TransformOp Enum

```rust
// src/binding/transform.rs

use serde_json::Value;
use smallvec::SmallVec;
use std::sync::Arc;

/// A single transform operation
#[derive(Debug, Clone, PartialEq)]
pub enum TransformOp {
    // -- String --
    Upper,
    Lower,
    Trim,
    TrimStart,
    TrimEnd,

    // -- Collection --
    Length,
    First,
    Last,
    FirstN(usize),
    LastN(usize),
    Keys,
    Values,
    Flatten,
    Reverse,
    Sort,
    Unique,
    Compact,  // remove nulls

    // -- Type conversion --
    ToString,
    ToNumber,
    ToBool,
    ToJson,
    ParseJson,

    // -- Numeric --
    Round(Option<u32>),
    Abs,
    Ceil,
    Floor,

    // -- Utility --
    Default(Value),
    TypeOf,
    Join(String),
    Split(String),
    Shell, // existing |shell from template.rs

    // -- JQ (feature-gated) --
    #[cfg(feature = "jq")]
    Jq(Arc<str>),
}
```

#### 2.2 TransformExpr

```rust
/// A chain of transform operations: `sort | unique | first(3)`
#[derive(Debug, Clone, PartialEq)]
pub struct TransformExpr {
    pub ops: SmallVec<[TransformOp; 2]>,
}

impl TransformExpr {
    /// Parse a transform expression from pipe-separated string
    /// e.g., "sort | unique | first(3)"
    pub fn parse(input: &str) -> Result<Self, TransformParseError> { ... }

    /// Apply all transforms to a value
    pub fn apply(&self, value: &Value) -> Result<Value, TransformError> { ... }

    /// Returns true if this expression is empty (no-op)
    pub fn is_empty(&self) -> bool { self.ops.is_empty() }
}
```

#### 2.3 Transform Application

Each `TransformOp` implements application logic:

```rust
impl TransformOp {
    /// Apply this single transform to a JSON value
    pub fn apply(&self, value: &Value) -> Result<Value, TransformError> {
        match self {
            TransformOp::Upper => match value {
                Value::String(s) => Ok(Value::String(s.to_uppercase())),
                _ => Err(TransformError::TypeMismatch {
                    op: "upper", expected: "string", got: value_type_name(value)
                }),
            },
            TransformOp::Length => match value {
                Value::Array(arr) => Ok(Value::Number(arr.len().into())),
                Value::String(s) => Ok(Value::Number(s.len().into())),
                Value::Object(obj) => Ok(Value::Number(obj.len().into())),
                _ => Err(TransformError::TypeMismatch { ... }),
            },
            TransformOp::Sort => match value {
                Value::Array(arr) => {
                    let mut sorted = arr.clone();
                    sorted.sort_by(|a, b| {
                        a.to_string().cmp(&b.to_string())
                    });
                    Ok(Value::Array(sorted))
                },
                _ => Err(TransformError::TypeMismatch { ... }),
            },
            // ... 27 ops total
        }
    }
}
```

#### 2.4 Pipe Parser

```rust
/// Parse a single transform op from string
///
/// Examples: "upper", "first(3)", "join(', ')", "default('N/A')", "round(2)"
fn parse_single_op(input: &str) -> Result<TransformOp, TransformParseError> { ... }

/// Parse a pipe-separated chain: "sort | unique | first(3)"
pub fn parse_pipeline(input: &str) -> Result<TransformExpr, TransformParseError> {
    let ops: SmallVec<[TransformOp; 2]> = input
        .split('|')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(parse_single_op)
        .collect::<Result<_, _>>()?;
    Ok(TransformExpr { ops })
}
```

#### 2.5 Tests for Phase 2

```
// Parse tests (per-op)
test_parse_upper()                     "upper"
test_parse_lower()                     "lower"
test_parse_trim()                      "trim"
test_parse_length()                    "length"
test_parse_first()                     "first"
test_parse_first_n()                   "first(3)"
test_parse_last_n()                    "last(5)"
test_parse_join()                      "join(', ')"
test_parse_split()                     "split('/')"
test_parse_default_string()            "default('N/A')"
test_parse_default_number()            "default(42)"
test_parse_round()                     "round(2)"
test_parse_shell()                     "shell"
test_parse_to_json()                   "to_json"
test_parse_parse_json()                "parse_json"
test_parse_unknown()                   "bogus" -> error
test_parse_pipeline()                  "sort | unique | first(3)"

// Apply tests (per-op)
test_apply_upper_string()              "hello" -> "HELLO"
test_apply_upper_non_string()          42 -> error
test_apply_lower_string()              "HELLO" -> "hello"
test_apply_trim()                      " hello " -> "hello"
test_apply_length_array()              [1,2,3] -> 3
test_apply_length_string()             "abc" -> 3
test_apply_length_object()             {a:1,b:2} -> 2
test_apply_first_array()               [1,2,3] -> 1
test_apply_first_empty()               [] -> null
test_apply_last_array()                [1,2,3] -> 3
test_apply_first_n()                   [1,2,3,4,5] | first(3) -> [1,2,3]
test_apply_keys()                      {a:1,b:2} -> ["a","b"]
test_apply_values()                    {a:1,b:2} -> [1,2]
test_apply_sort()                      [3,1,2] -> [1,2,3]
test_apply_unique()                    [1,2,2,3] -> [1,2,3]
test_apply_compact()                   [1,null,2,null] -> [1,2]
test_apply_flatten()                   [[1,2],[3]] -> [1,2,3]
test_apply_reverse()                   [1,2,3] -> [3,2,1]
test_apply_to_string()                 42 -> "42"
test_apply_to_number()                 "42" -> 42
test_apply_to_bool()                   1 -> true, 0 -> false
test_apply_to_json()                   [1,2] -> "[1,2]"
test_apply_parse_json()                "{\"a\":1}" -> {a:1}
test_apply_round()                     3.14159 | round(2) -> 3.14
test_apply_abs()                       -5 -> 5
test_apply_ceil()                      3.2 -> 4
test_apply_floor()                     3.8 -> 3
test_apply_join()                      ["a","b"] | join(", ") -> "a, b"
test_apply_split()                     "a/b/c" | split("/") -> ["a","b","c"]
test_apply_default_with_null()         null | default("N/A") -> "N/A"
test_apply_default_with_value()        "hello" | default("N/A") -> "hello"
test_apply_typeof()                    42 -> "number", "x" -> "string"

// Pipeline tests
test_pipeline_sort_unique()            [3,1,2,1] -> [1,2,3]
test_pipeline_sort_first_n()           [3,1,2] | sort | first(2) -> [1,2]
test_pipeline_upper_trim()             " hello " | trim | upper -> "HELLO"
test_pipeline_empty()                  "" -> no-op
test_pipeline_single()                 "upper" -> 1 op
```

---

### Phase 3: WithEntry + Serde

**File**: `src/binding/entry.rs` (REWRITE)
**Estimated tests**: 50

#### 3.1 WithEntry Struct

```rust
// src/binding/entry.rs (rewritten)

use rustc_hash::FxHashMap;
use serde::Deserialize;
use serde_json::Value;

use super::types::{BindingPath, BindingType};
use super::transform::TransformExpr;

/// A single binding entry in the `with:` block
#[derive(Debug, Clone)]
pub struct WithEntry {
    /// Parsed source path (e.g., $step1.data.items)
    pub source: BindingPath,
    /// Type constraint (default: Any)
    pub binding_type: BindingType,
    /// Default value if source is null/missing
    pub default: Option<Value>,
    /// Defer resolution until first access
    pub lazy: bool,
    /// Transform pipeline to apply after resolution
    pub transform: Option<TransformExpr>,
}

/// Map of alias -> WithEntry
pub type WithSpec = FxHashMap<String, WithEntry>;
```

#### 3.2 String Form Parsing

The compact `with:` string form supports transforms via `|`:

```
"$step1.data"                            # Simple
"$step1.data ?? fallback"                # With default
"$step1.data | upper"                    # With transform
"$step1.data | sort | unique"           # Transform chain
"$step1.data | sort | first(3) ?? []"   # Transform + default
```

```rust
/// Parse a with-entry from string form
///
/// Grammar:
///   entry := path ("|" transform)* ("??" default)?
///   path := "$" identifier ("." identifier | "[" index "]")*
///   transform := name | name "(" args ")"
///   default := json_value
pub fn parse_with_entry(input: &str) -> Result<WithEntry, WithEntryParseError> {
    // 1. Split by " ?? " to get path+transforms and default
    // 2. Split path+transforms by "|" to get path and transform chain
    // 3. Parse path with BindingPath::parse()
    // 4. Parse transforms with TransformExpr::parse()
    // 5. Parse default as JSON value
    ...
}
```

#### 3.3 Custom Deserializer

Must handle both string and object YAML forms:

```yaml
# String form (most common)
with:
  result: $step1
  title: $step1.title | upper
  count: $step1.items | length ?? 0

# Object form (for complex cases)
with:
  summary:
    from: $step1.abstract
    type: string
    transform: lower | trim
    default: "No abstract"
    lazy: true
```

```rust
impl<'de> Deserialize<'de> for WithEntry {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: serde::Deserializer<'de> {
        struct WithEntryVisitor;

        impl<'de> serde::de::Visitor<'de> for WithEntryVisitor {
            type Value = WithEntry;

            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                parse_with_entry(v).map_err(E::custom)
            }

            fn visit_map<M: serde::de::MapAccess<'de>>(self, map: M) -> Result<Self::Value, M::Error> {
                // Parse object form: { from, type, transform, default, lazy }
                #[derive(Deserialize)]
                struct WithEntryObject {
                    from: String,
                    #[serde(rename = "type", default)]
                    binding_type: BindingType,
                    #[serde(default)]
                    transform: Option<String>,
                    #[serde(default)]
                    default: Option<Value>,
                    #[serde(default)]
                    lazy: bool,
                }
                let obj = WithEntryObject::deserialize(
                    serde::de::value::MapAccessDeserializer::new(map)
                )?;
                // Parse from: field as BindingPath
                // Parse transform: field as TransformExpr
                ...
            }
        }

        deserializer.deserialize_any(WithEntryVisitor)
    }
}
```

#### 3.4 Tests for Phase 3

```
// String form parsing
test_parse_simple()                     "$step1"
test_parse_with_field()                 "$step1.output"
test_parse_with_default_string()        "$step1 ?? 'fallback'"
test_parse_with_default_number()        "$step1 ?? 42"
test_parse_with_default_array()         "$step1 ?? []"
test_parse_with_default_object()        "$step1 ?? {}"
test_parse_with_transform()             "$step1 | upper"
test_parse_with_transform_chain()       "$step1 | sort | unique"
test_parse_with_transform_and_default() "$step1 | length ?? 0"
test_parse_context_ref()                "$context.files.brand"
test_parse_input_ref()                  "$inputs.locale"
test_parse_env_ref()                    "$env.API_URL"

// Object form deserialization
test_deser_object_minimal()             { from: "$step1" }
test_deser_object_typed()               { from: "$step1", type: string }
test_deser_object_transform()           { from: "$step1", transform: "upper | trim" }
test_deser_object_full()                All fields populated
test_deser_object_lazy()                { from: "$step1", lazy: true }

// WithSpec deserialization (YAML map)
test_deser_spec_mixed()                 String + object entries in same map
test_deser_spec_empty()                 Empty map
test_deser_spec_single()                One entry

// Edge cases
test_parse_empty_string()               "" -> error
test_parse_no_dollar()                  "step1" -> error
test_parse_double_dollar()              "$$step1" -> "$step1" (escaped)
test_parse_pipe_only()                  "| upper" -> error
test_parse_default_only()               "?? 42" -> error
test_parse_whitespace()                 "  $step1  |  upper  " -> OK (trimmed)

// Compatibility: ensure old UseEntry tests have equivalents
test_compat_simple_path()               "$step1" replaces "step1"
test_compat_deep_path()                 "$step1.data.name" replaces "step1.data.name"
test_compat_default_string()            "$step1 ?? 'N/A'" replaces 'step1 ?? "N/A"'
```

---

### Phase 4: Template Rewrite

**File**: `src/binding/template.rs` (REFACTOR)
**Estimated tests**: 35

#### 4.1 Template Patterns: Iterative Parser (not regex for captures)

The old `WITH_RE` regex with `(?!context\.|inputs\.)` negative lookahead is **buggy**:
it incorrectly rejects words like `contextual` and can't capture arbitrary-length
transform chains. Instead, use regex ONLY to find `{{ ... }}` blocks, then parse
the content inside with a small iterative parser.

```rust
// BEFORE:
// static USE_RE: ... = Regex::new(r"\{\{\s*use\.(\w+(?:\.\w+)*)(?:\s*\|\s*(shell))?\s*\}\}");
// static CONTEXT_RE: ...
// static INPUTS_RE: ...

// AFTER:

/// Matches ANY {{...}} block. Content is parsed by parse_template_expr().
static TEMPLATE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\{\{(.*?)\}\}").unwrap()
});

/// Parsed template expression from inside {{ ... }}
#[derive(Debug, Clone, PartialEq)]
pub enum TemplateExpr {
    /// Alias from with: block, with optional transforms
    /// e.g., "title | upper | trim"
    Alias {
        path: String,  // "title" or "data.items[0]"
        transforms: Vec<String>,  // ["upper", "trim"]
    },
    /// Direct context reference: "context.files.brand" or "context.session.key"
    Context(String),
    /// Direct input reference: "inputs.locale" or "inputs.config.theme"
    Input(String),
}

/// Parse the content inside {{ ... }} into a TemplateExpr.
///
/// Grammar:
///   expr := "context." path        → Context
///         | "inputs." path         → Input
///         | alias_path ("|" transform)*  → Alias
///
/// This replaces the buggy negative-lookahead regex approach.
/// Handles arbitrary-length transform chains correctly.
fn parse_template_expr(content: &str) -> Result<TemplateExpr, TemplateError> {
    let trimmed = content.trim();

    // Check for context.* and inputs.* FIRST (exact prefix match)
    if let Some(rest) = trimmed.strip_prefix("context.") {
        return Ok(TemplateExpr::Context(rest.to_string()));
    }
    if let Some(rest) = trimmed.strip_prefix("inputs.") {
        return Ok(TemplateExpr::Input(rest.to_string()));
    }

    // Everything else is an alias (possibly with transforms)
    // Split by | to get alias path and transforms
    let parts: Vec<&str> = trimmed.split('|').map(str::trim).collect();
    let path = parts[0].to_string();
    let transforms = parts[1..].iter().map(|s| s.to_string()).collect();

    if path.is_empty() {
        return Err(TemplateError::EmptyAlias);
    }

    Ok(TemplateExpr::Alias { path, transforms })
}
```

**Why iterative parser over regex:**
1. No negative lookahead bugs — exact `strip_prefix("context.")` is unambiguous
2. Arbitrary transform chains — `split('|')` handles any number of pipes
3. Better error messages — parser can report exactly what's wrong
4. Simpler to maintain — no complex regex to debug

**Pass 2 regex stays simple** — only used for `{{context.*}}` and `{{inputs.*}}` that
survived Pass 1 (i.e., references used directly in templates without `with:` aliases):

```rust
/// Matches {{context.files.alias}} or {{context.session.key}} (Pass 2)
static CONTEXT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\{\{\s*context\.(\w+(?:\.\w+)*)\s*\}\}").unwrap()
});

/// Matches {{inputs.param}} or {{inputs.param.nested}} (Pass 2)
static INPUTS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\{\{\s*inputs\.(\w+(?:\.\w+)*)\s*\}\}").unwrap()
});
```

#### 4.2 Two-Pass Resolution

```rust
/// Resolve all template references in a string
///
/// Pass 1: {{alias}} and {{alias | transform}} -- resolved from WithSpec
/// Pass 2: {{context.*}} and {{inputs.*}} -- direct access (convenience)
///
/// Security: Pass 2 values are NOT re-evaluated by Pass 1 patterns
pub fn resolve(
    template: &str,
    with_values: &FxHashMap<String, Value>,
    context: &ContextData,
    inputs: &Value,
) -> Result<String, TemplateError> {
    // Pass 1: Find all {{...}} blocks, parse each, resolve aliases
    let pass1 = TEMPLATE_RE.replace_all(template, |caps: &Captures| {
        let content = &caps[1];
        match parse_template_expr(content) {
            Ok(TemplateExpr::Alias { path, transforms }) => {
                // Resolve alias from with_values
                let value = resolve_alias_path(&path, with_values)?;

                // Apply transform chain (arbitrary length)
                let mut current = value;
                for transform_str in &transforms {
                    let op = TransformOp::parse_single(transform_str)?;
                    current = op.apply(&current)?;
                }
                Ok(value_to_display(&current))
            }
            Ok(TemplateExpr::Context(_) | TemplateExpr::Input(_)) => {
                // Leave context/inputs refs for Pass 2 — re-emit as {{...}}
                Ok(format!("{{{{{}}}}}", content))
            }
            Err(e) => Err(e),
        }
    });

    // Pass 2: Resolve {{context.*}} and {{inputs.*}} (convenience shortcuts)
    let pass2 = resolve_direct_refs(&pass1, context, inputs)?;

    Ok(pass2)
}

/// Convert a Value to its display string for template interpolation
///
/// - String: raw string (no quotes)
/// - Number/Bool: to_string()
/// - Array/Object: JSON-serialize (lossless for LLMs)
/// - Null: empty string
fn value_to_display(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(), // JSON representation
    }
}
```

#### 4.3 Dedup: extract_refs, validate_refs

```rust
/// Extract all template references from a string
/// Returns aliases used in {{alias}} patterns
pub fn extract_refs(template: &str) -> Vec<String> { ... }

/// Validate that all template references can be resolved
pub fn validate_refs(
    template: &str,
    available_aliases: &FxHashSet<String>,
) -> Result<(), Vec<UnresolvedRef>> { ... }
```

#### 4.4 Backward-compat removal

- DELETE `detect_deprecated_dollar_syntax()` -- no longer needed
- DELETE old `USE_RE` pattern
- DELETE 3rd pass (inputs was separate, now in pass 2 alongside context)
- KEEP `escape_for_shell()` -- still needed for `|shell` transform
- KEEP `resolve_for_shell()` -- used by exec: tasks

#### 4.5 Tests for Phase 4

```
// Pass 1: alias resolution
test_resolve_simple_alias()             "Hello {{name}}" with name="World"
test_resolve_deep_alias()               "{{data.items}}" with data={items:[1,2]}
test_resolve_with_transform()           "{{title | upper}}" with title="hello"
test_resolve_array_default()            "{{items}}" -> JSON serialization
test_resolve_null_alias()               "{{missing}}" -> empty string
test_resolve_multiple_aliases()         "{{a}} and {{b}}"

// Pass 2: direct refs
test_resolve_context_file()             "{{context.files.brand}}"
test_resolve_context_session()          "{{context.session.key}}"
test_resolve_inputs()                   "{{inputs.locale}}"
test_resolve_inputs_nested()            "{{inputs.config.theme}}"

// Security
test_no_reevaluation()                  Pass 2 value containing {{}} is literal
test_shell_escape()                     "{{data | shell}}" escapes for shell

// Edge cases
test_empty_template()                   "" -> ""
test_no_templates()                     "plain text" -> "plain text"
test_unclosed_braces()                  "{{incomplete" -> literal
test_nested_braces()                    "{{{{double}}}}" -> error/literal

// Removal verification
test_old_use_prefix_not_matched()       "{{use.alias}}" -> NOT resolved (literal)

// Template expression parser (new — verifies Fix 2)
test_parse_expr_alias()                 "title" -> Alias { path: "title", transforms: [] }
test_parse_expr_alias_transform()       "title | upper" -> Alias { transforms: ["upper"] }
test_parse_expr_multi_transform()       "x | sort | unique | first(3)" -> 3 transforms
test_parse_expr_context()               "context.files.brand" -> Context("files.brand")
test_parse_expr_inputs()                "inputs.locale" -> Input("locale")
test_parse_expr_contextual_alias()      "contextual" -> Alias (NOT Context)
test_parse_expr_inputstream_alias()     "inputstream" -> Alias (NOT Input)
test_parse_expr_empty()                 "" -> error
```

---

### Phase 5: AST Changes

**Files**: `src/ast/raw/task.rs`, `src/ast/raw/workflow.rs`, `src/ast/raw/parser.rs`,
`src/ast/analyzed/task.rs`, `src/ast/analyzed/workflow.rs`, `src/ast/analyzer/analyze.rs`
**Estimated tests**: 30

#### 5.1 Raw AST

```rust
// src/ast/raw/task.rs
pub struct RawTask {
    pub id: Option<Marked<String>>,
    pub with: Option<Marked<WiringMap>>,     // RENAMED from `use`
    pub depends_on: Option<Vec<String>>,     // NEW
    // ... action, for_each, as_, concurrency, fail_fast, output remain unchanged
}

// src/ast/raw/workflow.rs
pub struct RawWorkflow {
    pub schema: Option<Marked<String>>,
    pub workflow: Option<Marked<String>>,
    pub provider: Option<Marked<String>>,
    pub tasks: Vec<RawTask>,
    pub mcp: Option<Marked<RawMcpConfig>>,
    pub context: Option<ContextConfig>,
    pub inputs: Option<Marked<serde_yaml::Value>>,
    pub imports: Option<Vec<ImportSpec>>,     // NEW (replaces include + skills)
    // REMOVED: flows, include, skills
}
```

#### 5.2 Analyzed AST

```rust
// src/ast/analyzed/task.rs
pub struct AnalyzedTask {
    pub id: TaskId,
    pub with: WithSpec,                      // RENAMED from wiring
    pub depends_on: Vec<TaskId>,             // NEW: explicit deps
    pub implicit_deps: Vec<TaskId>,          // NEW: extracted from with: $refs
    pub action: AnalyzedTaskAction,
    pub for_each: Option<ForEachSpec>,
    pub output: Option<OutputSpec>,
    // ...
}
```

#### 5.3 Analyzer Changes

```rust
// src/ast/analyzer/analyze.rs

/// Extract implicit dependencies from WithSpec $task_id references
fn extract_implicit_deps(
    with: &WithSpec,
    known_tasks: &FxHashSet<TaskId>,
) -> Vec<TaskId> {
    with.values()
        .filter_map(|entry| entry.source.task_id())
        .filter(|id| known_tasks.contains(id.as_ref()))
        .map(|id| TaskId::new(id.as_ref()))
        .collect()
}

// Remove: validate_flows() -- flows: no longer exists
// Remove: flows-related error variants
// Add: validate_depends_on() -- check that depends_on refs exist
```

#### 5.4 Tests for Phase 5

```
test_parse_with_keyword()               YAML with `with:` parses correctly
test_parse_depends_on()                 `depends_on: [a, b]` parses
test_parse_imports()                    `imports:` section parses
test_parse_no_flows()                   YAML without flows: is valid
test_old_use_keyword_rejected()         YAML with `use:` -> parse error (clean break)
test_extract_implicit_deps()            WithSpec -> task IDs
test_validate_depends_on_unknown()      depends_on ref to unknown task -> error
test_analyzer_full_workflow()           Complete workflow with with: + depends_on
```

---

### Phase 6: DAG Builder Evolution

**Files**: `src/dag/flow.rs` (EVOLVE → merge into `dag/mod.rs`), `src/dag/mod.rs` (REWRITE edge building)
**Estimated tests**: 25 new, 18 existing tests evolved (not simply deleted)

#### 6.1 Acknowledging Existing Implicit Dep Code (BUG-003 FIX)

`src/dag/flow.rs` lines 123-165 **already implements** implicit dependency extraction
from `use:` wiring (added in v0.22.4 as BUG-003 FIX). This code:

- Iterates task `use_wiring` entries
- Extracts `task_id()` references
- Skips self-references (`dep_task_id == task.id`)
- Skips non-task paths (context.*, inputs.* via `!task_set.contains(dep_task_id)`)
- Deduplicates edges before adding to adjacency/predecessors

**This logic is NOT deleted — it MOVES** into two places:
1. **Phase 5 (analyzer)**: `extract_implicit_deps()` extracts task IDs from `WithSpec`
2. **Phase 6 (DAG builder)**: Uses `task.implicit_deps` (pre-computed by analyzer)

The existing 6 `test_use_wiring_*` tests **evolve** into `test_dag_*` tests with `with:`
syntax. The 12 other flow.rs tests (cycle detection, deepest terminal, duplicate IDs)
**stay in `dag/mod.rs`** — they test DAG structure, not flows.

After merging the implicit dep logic into the analyzer + DAG builder, `flow.rs` is
deleted because `flows:` section no longer exists. But none of the logic is lost.

#### 6.2 New Edge Building in dag/mod.rs

```rust
impl Dag {
    /// Build a DAG from an analyzed workflow.
    ///
    /// Edges come from two sources:
    /// 1. Implicit deps: extracted from with: $task_id references
    /// 2. Explicit deps: from depends_on: field
    ///
    /// flows: is no longer a source of edges (removed in v0.28).
    pub fn from_analyzed(workflow: &AnalyzedWorkflow) -> Result<Self, NikaError> {
        let capacity = workflow.tasks.len();
        let mut adjacency = FxHashMap::with_capacity_and_hasher(capacity, Default::default());
        let mut predecessors = FxHashMap::with_capacity_and_hasher(capacity, Default::default());
        let mut task_ids = Vec::with_capacity(capacity);
        let mut task_set = FxHashSet::with_capacity_and_hasher(capacity, Default::default());

        // Register all tasks
        for task in workflow.tasks() {
            let id = intern(&task.id.as_str());
            task_ids.push(Arc::clone(&id));
            task_set.insert(Arc::clone(&id));
            adjacency.entry(Arc::clone(&id)).or_insert_with(DepVec::new);
            predecessors.entry(Arc::clone(&id)).or_insert_with(DepVec::new);
        }

        // Build edges from implicit_deps + depends_on
        for task in workflow.tasks() {
            let target = intern(&task.id.as_str());

            // 1. Implicit deps from with: $task_id
            for dep_id in &task.implicit_deps {
                let source = intern(dep_id.as_str());
                adjacency.get_mut(&source).unwrap().push(Arc::clone(&target));
                predecessors.get_mut(&target).unwrap().push(Arc::clone(&source));
            }

            // 2. Explicit depends_on:
            for dep_id in &task.depends_on {
                let source = intern(dep_id.as_str());
                if !predecessors[&target].contains(&source) {
                    adjacency.get_mut(&source).unwrap().push(Arc::clone(&target));
                    predecessors.get_mut(&target).unwrap().push(source);
                }
            }
        }

        let dag = Dag { adjacency, predecessors, task_ids, task_set };
        dag.validate_acyclic()?;
        Ok(dag)
    }
}
```

#### 6.3 Tests for Phase 6

**New tests (25):**
```
test_dag_implicit_deps()                with: $a creates edge a -> b
test_dag_explicit_depends_on()          depends_on: [a] creates edge a -> b
test_dag_mixed_deps()                   Both implicit and explicit
test_dag_no_duplicate_edges()           Same dep in with: and depends_on: -> one edge
test_dag_parallel_tasks()               No deps -> all tasks in parallel
test_dag_diamond()                      a -> b, a -> c, b -> d, c -> d
test_dag_cycle_detection()              a -> b -> a -> cycle error
test_dag_self_reference()               with: $self -> cycle error
test_dag_topological_order()            Returns valid topological sort
test_dag_empty_workflow()               No tasks -> empty DAG
```

**Evolved from flow.rs (18 existing → rewritten with with: syntax):**
```
test_use_wiring_creates_implicit_dependency → test_dag_with_creates_implicit_dep
test_use_wiring_no_duplicate_edges         → test_dag_with_no_duplicate_edges
test_use_wiring_skips_context_refs         → test_dag_with_skips_context_refs
test_use_wiring_skips_inputs_refs          → test_dag_with_skips_inputs_refs
test_use_wiring_multiple_deps              → test_dag_with_multiple_deps
test_use_wiring_with_field_path            → test_dag_with_field_path
test_deepest_final_task_*                  → KEPT AS-IS (6 tests, DAG structure)
test_detect_cycle_*                        → KEPT AS-IS (3 tests, cycle detection)
test_duplicate_task_id_*                   → KEPT AS-IS (2 tests, validation)
test_unique_task_ids_ok                    → KEPT AS-IS (1 test)
```

---

### Phase 7: Import System Redesign

**Files**: `src/ast/imports.rs` (NEW), `src/ast/include.rs` (DELETE),
`src/ast/skill_def.rs` (REWRITE), `src/ast/include_loader.rs` (REWRITE -> `import_loader.rs`)
**Estimated tests**: 40

#### 7.1 Design: Unified `imports:` Block

Replace separate `include:` and `skills:` with a unified `imports:` block:

```yaml
schema: nika/workflow@0.12

imports:
  # Workflow inclusion (was include:)
  - workflow: ./partials/setup.nika.yaml
    prefix: setup_

  # Skill loading (was skills:)
  - skill: ./skills/seo-writer.skill.md
    as: seo

  # Package reference (both workflows and skills)
  - workflow: pkg:@supernovae/common@1.0.0/setup.nika.yaml
    prefix: common_

  - skill: pkg:@supernovae/skills@1.0.0/rust.md
    as: rust

  # Agent definition (future)
  - agent: ./agents/researcher.agent.yaml
    as: researcher
```

#### 7.2 ImportSpec Type

```rust
// src/ast/imports.rs (NEW)

use serde::Deserialize;

/// What kind of resource is being imported
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImportKind {
    /// Workflow DAG fusion (was include:)
    Workflow,
    /// Skill file for agent prompts (was skills:)
    Skill,
    /// Agent definition (future)
    Agent,
    /// Schema/type definition (future)
    Schema,
}

/// A single import declaration
#[derive(Debug, Clone, Deserialize)]
pub struct ImportSpec {
    /// Path or pkg: URI to the resource
    /// This is the value of the kind field (workflow, skill, context, agent, schema)
    #[serde(flatten)]
    pub kind_and_path: ImportKindPath,

    /// Alias for referencing this import
    /// For skills: used in agent task `skills: [alias]`
    /// For context: used in `$context.alias` bindings
    /// For agents: used in `agent: alias` (future)
    #[serde(rename = "as")]
    pub alias: Option<String>,

    /// Prefix for task IDs (workflow imports only)
    /// Prevents ID collisions when importing multiple workflows
    pub prefix: Option<String>,

    /// Version pin (pkg: URIs only, overrides version in URI)
    pub version: Option<String>,
}

/// The kind + path pair, deserialized from the kind field name
#[derive(Debug, Clone, PartialEq)]
pub struct ImportKindPath {
    pub kind: ImportKind,
    pub path: String,
}

// Custom deserializer for ImportKindPath that reads from one of:
// { workflow: "path" } | { skill: "path" } | { context: "path" } | ...
```

#### 7.3 Backwards mapping from old syntax

| Old Syntax | New Syntax |
|------------|-----------|
| `include: [{path: ./x.yaml, prefix: p_}]` | `imports: [{workflow: ./x.yaml, prefix: p_}]` |
| `skills: {seo: ./skills/seo.md}` | `imports: [{skill: ./skills/seo.md, as: seo}]` |
| `include: [{pkg: "@wf/seo"}]` | `imports: [{workflow: pkg:@wf/seo, prefix: seo_}]` |

**Decision (FINAL)**: `context:` section stays **entirely as-is**. It works, it's simple,
it's orthogonal to the import system. `imports:` replaces `include:` + `skills:` ONLY.
No `ImportKind::Context` — context files are NOT imports.

```yaml
# FINAL SYNTAX:
schema: nika/workflow@0.12

# Context stays as-is (session + files)
context:
  files:
    brand: ./context/brand.md
  session: .nika/sessions/prev.json

# imports: replaces include: + skills:
imports:
  - workflow: ./partials/setup.nika.yaml
    prefix: setup_
  - skill: ./skills/seo.skill.md
    as: seo
  - skill: pkg:@supernovae/skills@1.0.0/rust.md
    as: rust
```

#### 7.4 Import Loader

```rust
// src/ast/import_loader.rs (replaces include_loader.rs)

use super::imports::{ImportSpec, ImportKind};
use super::pkg_resolver::PkgUri;

/// Resolve all imports for a workflow
///
/// - Workflow imports: Load and merge tasks (DAG fusion with prefix)
/// - Skill imports: Load skill content and register by alias
/// - Context imports: Handled elsewhere (still in context: section)
pub fn resolve_imports(
    imports: &[ImportSpec],
    base_dir: &Path,
    visited: &mut FxHashSet<PathBuf>,  // cycle detection
) -> Result<ResolvedImports, NikaError> {
    let mut resolved = ResolvedImports::default();

    for import in imports {
        match import.kind() {
            ImportKind::Workflow => {
                let path = resolve_import_path(&import.path(), base_dir)?;
                validate_path_boundary(base_dir, &path)?;

                if !visited.insert(path.clone()) {
                    return Err(NikaError::CircularImport { path: path.display().to_string() });
                }

                let sub_workflow = load_workflow(&path)?;
                let prefix = import.prefix.as_deref().unwrap_or("");

                // Merge tasks with prefix
                for task in sub_workflow.tasks {
                    let mut prefixed = task.clone();
                    prefixed.id = format!("{}{}", prefix, task.id);
                    resolved.tasks.push(prefixed);
                }

                // Merge skills from imported workflow
                resolved.skills.extend(sub_workflow.skills);

                // Recurse into imported workflow's imports
                if let Some(sub_imports) = &sub_workflow.imports {
                    let sub_resolved = resolve_imports(sub_imports, &path.parent().unwrap(), visited)?;
                    resolved.merge(sub_resolved);
                }

                visited.remove(&path);
            }
            ImportKind::Skill => {
                let path = resolve_import_path(&import.path(), base_dir)?;
                let alias = import.alias.as_ref().ok_or(NikaError::MissingImportAlias {
                    kind: "skill", path: path.display().to_string()
                })?;
                resolved.skills.insert(alias.clone(), path);
            }
            ImportKind::Agent | ImportKind::Schema => {
                // Future: agent/schema import handling
            }
        }
    }

    Ok(resolved)
}
```

#### 7.5 Tests for Phase 7

```
// ImportSpec parsing
test_parse_workflow_import()            { workflow: "./x.yaml", prefix: "p_" }
test_parse_skill_import()              { skill: "./skills/seo.md", as: "seo" }
test_parse_pkg_import()                { workflow: "pkg:@scope/name@1.0/file.yaml" }
test_parse_skill_pkg()                 { skill: "pkg:@supernovae/skills@1.0/rust.md", as: "rust" }
test_parse_missing_kind()              {} -> error
test_parse_multiple_kinds()            { workflow: "x", skill: "y" } -> error

// Import resolution
test_resolve_workflow_import()          Loads and merges tasks
test_resolve_skill_import()            Loads skill content
test_resolve_pkg_uri()                 Resolves pkg: to filesystem
test_resolve_circular_import()         A imports B imports A -> error
test_resolve_missing_file()            Path doesn't exist -> error
test_resolve_path_traversal()          ../../../etc/passwd -> error
test_resolve_prefix_application()      Task IDs get prefixed
test_resolve_skill_missing_alias()     Skill without `as:` -> error
test_resolve_recursive_imports()       A imports B which imports C
test_resolve_skill_dedup()             Same skill imported twice -> deduplicated
```

---

### Phase 8: Runtime Integration

**Files**: `src/runtime/runner.rs`, `src/runtime/executor/verbs.rs`,
`src/runtime/chat_workflow.rs`, `src/binding/resolve.rs`, `src/store.rs`
**Estimated tests**: 25

#### 8.1 Binding Resolution (resolve.rs rewrite)

```rust
// src/binding/resolve.rs (rewritten)

/// State of a single binding during resolution
#[derive(Debug, Clone)]
pub enum BindingState {
    /// Value has been resolved
    Resolved {
        value: Arc<Value>,
        source: BindingSource,
    },
    /// Value has not been resolved yet (lazy)
    Pending {
        entry: WithEntry,
    },
    /// Resolution failed
    Failed {
        source: BindingPath,
        error: Arc<BindingResolutionError>,
    },
}

/// Map of alias -> resolved state
pub type BindingMap = FxHashMap<String, BindingState>;

/// Resolve all eager bindings in a WithSpec
pub fn resolve_eager(
    spec: &WithSpec,
    store: &RunContext,
    context: &ContextData,
    inputs: &Value,
) -> BindingMap {
    spec.iter()
        .map(|(alias, entry)| {
            if entry.lazy {
                (alias.clone(), BindingState::Pending { entry: entry.clone() })
            } else {
                let state = resolve_single(entry, store, context, inputs);
                (alias.clone(), state)
            }
        })
        .collect()
}

/// Resolve a single binding entry
fn resolve_single(
    entry: &WithEntry,
    store: &RunContext,
    context: &ContextData,
    inputs: &Value,
) -> BindingState {
    // 1. Resolve source path to raw value
    let raw_value = match &entry.source.source {
        BindingSource::Task(id) => store.get(id.as_ref()),
        BindingSource::Context(key) => context.get(key.as_ref()),
        BindingSource::Input(key) => inputs.pointer(&format!("/{}", key.replace('.', "/"))),
        BindingSource::Env(name) => std::env::var(name.as_ref()).ok().map(Value::String),
        BindingSource::LoopVar(name) => store.get(&format!("__loop_{}", name)),
    };

    // 2. Navigate path segments
    let navigated = navigate_path(raw_value, &entry.source.segments);

    // 3. Apply default if null
    let with_default = match navigated {
        Some(v) if !v.is_null() => v.clone(),
        _ => entry.default.clone().unwrap_or(Value::Null),
    };

    // 4. Apply transforms
    let transformed = if let Some(ref expr) = entry.transform {
        match expr.apply(&with_default) {
            Ok(v) => v,
            Err(e) => return BindingState::Failed {
                source: entry.source.clone(),
                error: Arc::new(e.into()),
            },
        }
    } else {
        with_default
    };

    // 5. Validate type constraint
    if entry.binding_type != BindingType::Any {
        if let Err(e) = validate_type(&transformed, entry.binding_type) {
            return BindingState::Failed {
                source: entry.source.clone(),
                error: Arc::new(e.into()),
            };
        }
    }

    BindingState::Resolved {
        value: Arc::new(transformed),
        source: entry.source.source.clone(),
    }
}
```

#### 8.2 Runner Changes

```rust
// src/runtime/runner.rs
// Change all `WiringSpec` -> `WithSpec`
// Change all `UseEntry` -> `WithEntry`
// Change binding resolution call to use new resolve_eager()
// Remove flows: handling from workflow preparation
```

#### 8.3 Executor Changes

```rust
// src/runtime/executor/verbs.rs
// Change all `use` -> `with` in variable names
// Update template resolution calls to new 2-pass API
// Remove flows: references
```

#### 8.4 Tests for Phase 8

```
test_resolve_eager_simple()             $step1 -> resolved value
test_resolve_eager_with_transform()     $step1 | upper -> transformed
test_resolve_eager_with_default()       missing | default -> default value
test_resolve_eager_lazy()               lazy: true -> Pending state
test_resolve_type_check_pass()          string value + type: string -> OK
test_resolve_type_check_fail()          number value + type: string -> Failed
test_resolve_env_var()                  $env.HOME -> actual env value
test_resolve_context_file()             $context.files.brand -> file content
test_resolve_input()                    $inputs.locale -> input value
test_resolve_chain_transform()          $step1 | sort | unique -> sorted+unique
test_runner_with_spec()                 End-to-end workflow with with:
test_executor_with_spec()               Task execution with with: bindings
```

---

### Phase 9: Rich JSONPath

**Files**: `src/binding/jsonpath.rs` (NEW), delete `src/util/jsonpath.rs`
**Estimated tests**: 30

#### 9.1 Integration with serde_json_path

```rust
// src/binding/jsonpath.rs (NEW)

use serde_json::Value;
use serde_json_path::JsonPath;

/// Evaluate a JSONPath expression against a value
///
/// Supports RFC 9535 syntax:
/// - Wildcards: $[*], $.items[*].name
/// - Filters: $.items[?@.price > 100]
/// - Recursive descent: $..email
/// - Slices: $[0:3], $[-1]
pub fn query(value: &Value, path: &str) -> Result<Value, JsonPathError> {
    let jp = JsonPath::parse(path)
        .map_err(|e| JsonPathError::ParseError { path: path.to_string(), cause: e.to_string() })?;

    let nodes = jp.query(value);
    let results: Vec<&Value> = nodes.all();

    match results.len() {
        0 => Ok(Value::Null),
        1 => Ok(results[0].clone()),
        _ => Ok(Value::Array(results.into_iter().cloned().collect())),
    }
}

/// Check if a path is a JSONPath expression (contains $, [, ?, *)
pub fn is_jsonpath(path: &str) -> bool {
    path.contains('[') && (path.contains('*') || path.contains('?') || path.contains(':'))
}
```

#### 9.2 Integration Points

Rich JSONPath is used in:
1. **BindingPath segments** -- when path contains `[*]`, `[?...]`, etc.
2. **WithEntry string form** -- `$step1.data[*].name`
3. **Transform** -- not a transform, but a path feature

#### 9.3 Tests for Phase 9

```
test_query_root()                       "$" -> whole value
test_query_field()                      "$.name" -> field value
test_query_nested()                     "$.data.items" -> nested field
test_query_index()                      "$.items[0]" -> first element
test_query_negative_index()             "$.items[-1]" -> last element
test_query_wildcard()                   "$.items[*].name" -> all names
test_query_slice()                      "$.items[0:3]" -> first 3
test_query_filter_gt()                  "$.items[?@.price > 100]"
test_query_filter_eq()                  "$.items[?@.status == 'active']"
test_query_recursive()                  "$..email" -> all email fields
test_query_no_match()                   "$.missing" -> null
test_query_invalid_path()               "$$invalid" -> error
test_is_jsonpath_simple()               "name" -> false
test_is_jsonpath_wildcard()             "items[*]" -> true
test_is_jsonpath_filter()               "items[?@.x > 1]" -> true
```

---

### Phase 10: Output Contracts

**Files**: `src/ast/output_contract.rs` (NEW or extend `src/ast/output.rs`),
`src/runtime/executor/verbs.rs` (validation hook)
**Estimated tests**: 20

#### 10.1 Output Contract Syntax

```yaml
tasks:
  - id: fetch_data
    fetch:
      url: "https://api.example.com/data"
    output:
      schema:
        type: object
        required: [items, total]
        properties:
          items:
            type: array
          total:
            type: integer
```

#### 10.2 Implementation

```rust
// Extend existing OutputSpec or create OutputContract

/// Output contract for a task
#[derive(Debug, Clone, Deserialize)]
pub struct OutputContract {
    /// JSON Schema to validate output against
    pub schema: Option<Value>,
}

/// Validate task output against its contract
pub fn validate_output(
    output: &Value,
    contract: &OutputContract,
) -> Result<(), OutputValidationError> {
    if let Some(schema) = &contract.schema {
        // Use jsonschema crate (already in deps for structured output)
        let compiled = jsonschema::JSONSchema::compile(schema)
            .map_err(|e| OutputValidationError::InvalidSchema(e.to_string()))?;

        compiled.validate(output)
            .map_err(|errors| OutputValidationError::ValidationFailed {
                errors: errors.map(|e| e.to_string()).collect()
            })?;
    }
    Ok(())
}
```

#### 10.3 Tests for Phase 10

```
test_contract_valid_output()            Schema match -> OK
test_contract_missing_required()        Missing required field -> error
test_contract_wrong_type()              Wrong type -> error
test_contract_no_schema()               No contract -> pass through
test_contract_nested_schema()           Nested object validation
test_contract_array_items()             Array with item schema
test_contract_optional_field()          Non-required field missing -> OK
```

---

### Phase 11: JQ Escape Hatch

**Files**: `src/binding/jq.rs` (NEW, feature-gated)
**Estimated tests**: 20 (only run with `cargo test --features jq`)

#### 11.1 JQ Transform

```rust
// src/binding/jq.rs

#[cfg(feature = "jq")]
pub mod jq {
    use jaq_core::{Ctx, RcIter, Val};
    use jaq_std;
    use serde_json::Value;

    /// Compile and execute a jq expression against a JSON value
    pub fn evaluate(expr: &str, input: &Value) -> Result<Value, JqError> {
        // 1. Parse jq expression
        let (filter, errs) = jaq_parse::parse(expr, jaq_parse::main());
        if !errs.is_empty() {
            return Err(JqError::ParseError(errs));
        }

        // 2. Compile with standard library
        let mut defs = jaq_std::std();
        defs.extend(jaq_std::core());
        let filter = defs.finish(filter.unwrap(), Vec::new(), &mut Vec::new());

        // 3. Execute
        let inputs = RcIter::new(std::iter::empty());
        let val = Val::from(input.clone());
        let ctx = Ctx::new([], &inputs);

        let results: Vec<Val> = filter.run((ctx, val)).collect::<Result<Vec<_>, _>>()
            .map_err(JqError::RuntimeError)?;

        // 4. Convert back to serde_json::Value
        match results.len() {
            0 => Ok(Value::Null),
            1 => Ok(val_to_value(results.into_iter().next().unwrap())),
            _ => Ok(Value::Array(results.into_iter().map(val_to_value).collect())),
        }
    }
}
```

#### 11.2 Integration with TransformOp

```rust
// In transform.rs:
#[cfg(feature = "jq")]
TransformOp::Jq(expr) => {
    crate::binding::jq::evaluate(expr.as_ref(), value)
        .map_err(|e| TransformError::JqError(e))
}
```

#### 11.3 YAML Syntax

```yaml
with:
  complex:
    from: $data
    transform: "jq('.items | map(select(.price > 100)) | sort_by(.name)')"
```

#### 11.4 Tests for Phase 11

```
test_jq_identity()                      "." -> same value
test_jq_field_access()                  ".name" -> field
test_jq_pipe()                          ".items | length"
test_jq_map()                           ".items | map(.name)"
test_jq_select()                        ".items | map(select(.price > 100))"
test_jq_sort_by()                       ".items | sort_by(.name)"
test_jq_group_by()                      ".items | group_by(.category)"
test_jq_arithmetic()                    ".price * 1.2"
test_jq_string_interp()                 '"\(.name): \(.price)"'
test_jq_parse_error()                   "invalid{}" -> error
test_jq_null_input()                    null input -> handles gracefully
```

---

### Phase 12: YAML + Schema + Test Migration

**Files**: ALL `.nika.yaml` files (100+), `schemas/nika-workflow.schema.json`,
ALL test files with inline YAML
**Estimated tests**: Update ~136 existing tests

#### 12.1 YAML Migration Script

Create a one-shot Rust script or use sed/awk:

```
For each .nika.yaml file:
1. Replace `use:` with `with:` (YAML key level)
2. Replace `{{use.xxx}}` with `{{xxx}}` in all string values
3. Remove `flows:` sections entirely
4. Replace `include:` with `imports:` + add `workflow:` kind
5. Replace `skills:` with `imports:` entries + add `skill:` kind + `as:` alias
6. Update schema version to nika/workflow@0.12
```

#### 12.2 JSON Schema Update

```json
{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "title": "Nika Workflow",
  "properties": {
    "schema": { "const": "nika/workflow@0.12" },
    "tasks": {
      "items": {
        "properties": {
          "with": {
            "type": "object",
            "additionalProperties": {
              "oneOf": [
                { "type": "string" },
                {
                  "type": "object",
                  "properties": {
                    "from": { "type": "string" },
                    "type": { "enum": ["string","number","integer","boolean","array","object","any"] },
                    "transform": { "type": "string" },
                    "default": {},
                    "lazy": { "type": "boolean" }
                  },
                  "required": ["from"]
                }
              ]
            }
          },
          "depends_on": {
            "type": "array",
            "items": { "type": "string" }
          }
        }
      }
    },
    "imports": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "workflow": { "type": "string" },
          "skill": { "type": "string" },
          "context": { "type": "string" },
          "as": { "type": "string" },
          "prefix": { "type": "string" },
          "version": { "type": "string" }
        }
      }
    }
  }
}
```

#### 12.3 Test File Updates

Update all inline YAML in test files:
- `binding/entry.rs` tests: `use:` -> `with:`, path format changes
- `binding/template.rs` tests: `{{use.x}}` -> `{{x}}`
- `dag/flow.rs` tests: DELETE (file deleted)
- `dag/validate.rs` tests: Remove flows-related tests
- `ast/raw/parser.rs` tests: Update YAML with new keywords
- `ast/analyzer/analyze.rs` tests: Update expectations
- `runtime/` tests: Update binding references

---

## File Change Matrix

### New Files

| File | Lines (est.) | Purpose |
|------|-------------|---------|
| `binding/types.rs` | 200 | BindingPath, BindingSource, BindingType, PathSegment |
| `binding/transform.rs` | 450 | TransformOp, TransformExpr, pipe parser, apply logic |
| `binding/jsonpath.rs` | 100 | serde_json_path wrapper, query(), is_jsonpath() |
| `binding/jq.rs` | 80 | Feature-gated jaq-core wrapper |
| `ast/imports.rs` | 200 | ImportSpec, ImportKind, ImportKindPath |
| `ast/import_loader.rs` | 250 | resolve_imports(), path resolution, cycle detection |
| `ast/output_contract.rs` | 80 | OutputContract, validate_output() |

### Rewritten Files

| File | Lines (est.) | Changes |
|------|-------------|---------|
| `binding/entry.rs` | 500 | UseEntry->WithEntry, WiringSpec->WithSpec, new parser+serde |
| `binding/resolve.rs` | 300 | LazyBinding->BindingState, ResolvedBindings->BindingMap |
| `binding/mod.rs` | 80 | New re-exports, updated doc comments |
| `dag/mod.rs` | 400 | New edge building from implicit_deps + depends_on |

### Refactored Files

| File | Changes |
|------|---------|
| `binding/template.rs` | New regex, 2-pass model, transform support |
| `binding/mention.rs` | WiringSpec -> WithSpec references |
| `binding/validate.rs` | Type reference updates |
| `ast/raw/task.rs` | `use` -> `with`, add `depends_on` |
| `ast/raw/workflow.rs` | Remove `flows`, `include`, `skills`; add `imports` |
| `ast/raw/parser.rs` | Parse `with:` instead of `use:` |
| `ast/analyzed/task.rs` | WithSpec, depends_on, implicit_deps |
| `ast/analyzed/workflow.rs` | Remove flows |
| `ast/analyzer/analyze.rs` | Extract implicit deps, remove flows validation |
| `runtime/runner.rs` | WithSpec references, new resolution API |
| `runtime/executor/verbs.rs` | WithSpec references, output contract validation |
| `runtime/chat_workflow.rs` | Update mention/binding references |
| `lib.rs` | Update re-exports |
| `error.rs` | New error variants, remove flows errors |

### Deleted Files

| File | Reason |
|------|--------|
| `dag/flow.rs` | `flows:` section removed |
| `ast/include.rs` | Replaced by `ast/imports.rs` |
| `ast/include_loader.rs` | Replaced by `ast/import_loader.rs` |
| `util/jsonpath.rs` | Replaced by `binding/jsonpath.rs` (serde_json_path) |

### Updated Files (bulk)

| Category | Count | Change |
|----------|-------|--------|
| `.nika.yaml` files | ~100 | use->with, {{use.x}}->{{x}}, flows removed, schema version |
| Test files (.rs) | ~50 | Inline YAML updates, type name updates |
| `schemas/nika-workflow.schema.json` | 1 | New schema with with:, imports:, depends_on: |
| `CLAUDE.md` files | 3 | Updated syntax examples |
| `CHANGELOG.md` | 1 | v0.28 release notes |

---

## Test Strategy

### Test Counts by Phase

| Phase | New Tests | Updated Tests | Deleted Tests | Net Change |
|-------|-----------|---------------|---------------|------------|
| 1. Core Types | 25 | 0 | 0 | +25 |
| 2. Transform Engine | 60 | 0 | 0 | +60 |
| 3. WithEntry + Serde | 50 | 40 | 0 | +50 |
| 4. Template Rewrite | 43 | 30 | 0 | +43 |
| 5. AST Changes | 30 | 20 | 0 | +30 |
| 6. DAG Builder | 25 | 18 | 18* | +25 |
| 7. Import System | 40 | 11 | 0 | +40 |
| 8. Runtime Integration | 25 | 10 | 0 | +25 |
| 9. Rich JSONPath | 15 | 5 | 0 | +15 |
| 10. Output Contracts | 7 | 3 | 0 | +7 |
| 11. JQ Escape Hatch | 11 | 0 | 0 | +11 |
| 12. YAML + Schema Migration | 0 | ~136 | 0 | 0 |
| **TOTAL** | **~331** | **~273** | **18** | **+~313** |

*Phase 6: 18 flow.rs tests are deleted but 18 are "updated" (evolved with new syntax).
The 25 new tests + 18 evolved tests = 43 tests in dag/mod.rs after rewrite.

**Final count: 6,157 − 18 + ~331 = ~6,470 total tests**

### Testing Approach

1. **TDD**: Write tests first for each new type/function
2. **Snapshot tests**: insta for YAML parsing outputs
3. **Property tests**: proptest for BindingPath parser fuzzing
4. **Feature-gated tests**: `#[cfg(feature = "jq")]` for JQ tests
5. **Integration tests**: End-to-end workflow execution with new syntax

### Validation Gates

After each phase:
```bash
cargo test                    # All tests pass
cargo clippy -- -D warnings   # Zero warnings
cargo fmt --check             # Formatted
```

---

## Commit Strategy

Following the granular commit rules (1 fix = 1 commit):

| Phase | Commits (est.) | Pattern |
|-------|---------------|---------|
| 1 | 3 | `feat(binding): add BindingPath type`, `feat(binding): add BindingSource enum`, `feat(binding): add BindingType enum` |
| 2 | 4 | `feat(binding): add TransformOp enum`, `feat(binding): add TransformExpr parser`, `feat(binding): implement transform application`, `test(binding): add transform tests` |
| 3 | 3 | `refactor(binding): rename UseEntry to WithEntry`, `feat(binding): add WithEntry serde for string+object`, `refactor(binding): rename WiringSpec to WithSpec` |
| 4 | 3 | `refactor(binding): simplify template to 2-pass`, `feat(binding): add template transform support`, `refactor(binding): drop use. prefix from templates` |
| 5 | 4 | `refactor(ast): rename use to with in raw AST`, `feat(ast): add depends_on field`, `feat(ast): add imports field`, `refactor(ast): remove flows from AST` |
| 6 | 2 | `refactor(dag): build edges from with refs + depends_on`, `refactor(dag): remove flow.rs` |
| 7 | 3 | `feat(ast): add ImportSpec and ImportKind types`, `feat(ast): add import_loader with cycle detection`, `refactor(ast): remove include.rs and include_loader.rs` |
| 8 | 3 | `refactor(binding): rewrite resolve.rs with BindingState`, `refactor(runtime): update runner for WithSpec`, `refactor(runtime): update executor for WithSpec` |
| 9 | 2 | `feat(binding): add serde_json_path integration`, `refactor(util): remove custom jsonpath.rs` |
| 10 | 1 | `feat(ast): add output contracts with JSON Schema validation` |
| 11 | 1 | `feat(binding): add jq transform (feature-gated)` |
| 12 | 4 | `refactor(yaml): migrate all workflows to with: syntax`, `refactor(schema): update JSON schema for v0.12`, `refactor(test): update all inline YAML in tests`, `docs: update CLAUDE.md for v0.28 syntax` |
| **TOTAL** | **~33** | |

---

## Risk Assessment

### High Risk

| Risk | Mitigation |
|------|-----------|
| Breaking all 100+ YAML files at once | Automated migration script + compile-time validation |
| Regex change in template.rs breaks edge cases | Extensive test coverage, snapshot tests for all patterns |
| DAG builder rewrite creates subtle ordering bugs | Existing DAG tests adapted + new implicit dep tests |
| Custom serde deserializer for WithEntry is complex | Fuzz testing with proptest, exhaustive format tests |

### Medium Risk

| Risk | Mitigation |
|------|-----------|
| serde_json_path API differences from custom impl | Wrapper module isolates the dependency |
| jaq-core 2.0 API may change | Feature-gated, can be swapped |
| Import cycle detection in recursive imports | FxHashSet<PathBuf> tracking with depth limit |
| Transform pipeline edge cases (null handling) | Each op has explicit null behavior tests |

### Low Risk

| Risk | Mitigation |
|------|-----------|
| BindingType validation too strict | Default is `Any`, opt-in strictness |
| New Cargo deps increase build time | serde_json_path is small; jq is feature-gated |
| Output contracts JSON Schema validation | Already have jsonschema crate for structured output |

---

## Execution Order

```
Phase 1  → Phase 2  → Phase 3  → Phase 4  → Phase 5
(types)    (transform) (entry)    (template)  (ast)
                                      ↓
Phase 6  → Phase 7  → Phase 8  → Phase 9
(dag)      (imports)   (runtime)   (jsonpath)
                                      ↓
Phase 10 → Phase 11 → Phase 12
(contracts) (jq)       (migration)
```

Phases are sequential because each builds on the previous.
Within each phase, files can be worked on in parallel where independent.

---

## Schema Version

`nika/workflow@0.12` -- the new schema version for v0.28.

Feature gate in analyzer:
```rust
// @0.12 features
SchemaVersion::V0_12 => {
    // with: keyword (required)
    // depends_on: field (optional)
    // imports: section (optional)
    // output contracts (optional)
    // All new binding syntax
}
```

---

*v0 with zero users. Everything ships in v0.28. No backwards compat.*
*Plan written 2026-03-13. Ready for review.*
