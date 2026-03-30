# Plan: `from_example` V2 — Architectural Improvements

> Identified by rust-architect + rust-pro agents after V1 code review.
> V1 is correct and tested. These are quality-of-life / correctness improvements.

## Background

V1 (`from_example` in `StructuredOutputSpec`) works correctly but has 3 structural issues
found during review:

1. **Placeholder schema lie** — `schema: SchemaRef::Inline(json!({}))` when `from_example` is set
2. **OutputPolicy bridge** — `from_example` only accessible via `source_structured_spec` blob
3. **Prompt injection asymmetry** — file-based `from_example` can't show the example in the prompt

---

## Task 1: `schema: Option<SchemaRef>` — Remove the placeholder

**Priority:** HIGH | **Breaking:** Yes (~15 callsites in nika-engine) | **Crate:** nika-core + nika-engine

**Problem:** When `from_example` is set, `spec.schema` holds `json!({})` — a lie.
Any code reading `spec.schema` without checking `from_example` first gets garbage.

**Change:**
```rust
// Before
pub schema: SchemaRef,

// After
/// None when `from_example` is set — schema is derived at runtime.
pub schema: Option<SchemaRef>,
```

**Cascading updates:**
- `with_schema()`, `with_inline_schema()`, `with_file_schema()` — keep signature, set `from_example: None`
- `with_example_file()`, `with_example_inline()` — set `schema: None`
- `to_output_policy()` — only copy schema to OutputPolicy when `Some`
- `schema()` getter on `StructuredOutputEngine` → returns `Option<&SchemaRef>`
- `is_structured()` in `OutputPolicy` — must also check `source_structured_spec.from_example`
- Deserializer `visit_map` — already has correct logic, just remove `unwrap_or(json!({}))`
- All tests asserting `matches!(spec.schema, SchemaRef::Inline(_))` for example specs → remove

---

## Task 2: `OutputPolicy.from_example` direct field

**Priority:** MEDIUM | **Breaking:** No | **Crate:** nika-core

**Problem:** `from_example` only survives the `structured: →OutputPolicy` roundtrip via
`source_structured_spec: Some(self.clone())`. The `output: { format: json, from_example: ... }`
syntax is impossible.

**Change:**
```rust
pub struct OutputPolicy {
    pub format: OutputFormat,
    pub schema: Option<SchemaRef>,
    pub from_example: Option<SchemaRef>,  // NEW
    pub max_retries: Option<u8>,
    #[serde(skip)]
    pub source_structured_spec: Option<StructuredOutputSpec>,
}
```

`is_structured()` becomes:
```rust
pub fn is_structured(&self) -> bool {
    self.format == OutputFormat::Json
        && (self.schema.is_some() || self.from_example.is_some())
}
```

`build_json_schema_instruction` can then read `policy.from_example` directly instead of
going through `source_structured_spec`.

---

## Task 3: File-based `from_example` prompt injection

**Priority:** HIGH | **Breaking:** No | **Crate:** nika-engine

**Problem:** File-based `from_example` gets generic "must be valid JSON" prompt (no structure
shown to LLM) while inline gets the full example. This is because `build_json_schema_instruction`
is synchronous and can't read the file.

**Solution (rust-architect recommendation):**
Add `example_value: Option<Value>` to `StructuredOutputEngine`. When `load_schema()` runs
and `from_example` is a file, store the raw example before deriving schema from it:

```rust
pub struct StructuredOutputEngine {
    spec: StructuredOutputSpec,
    log: Arc<EventLog>,
    compiled_schema: Option<Arc<Value>>,
    cached_example: Option<Value>,  // NEW — set when from_example is File
    infer_fn: Option<InferCallback>,
    original_prompt: Option<String>,
}
```

Executor calls `engine.load_schema()` BEFORE `build_json_schema_instruction`, then:
```rust
// In executor:
engine.load_schema().await?;
let instruction = engine.build_instruction_from_example_or_schema(policy);
```

New method on engine:
```rust
pub fn example_value(&self) -> Option<&Value> {
    self.cached_example.as_ref()
}
```

---

## Task 4: `strict: bool` — opt-in `additionalProperties: false`

**Priority:** MEDIUM | **Breaking:** No | **Crate:** nika-core

**Problem:** Derived schemas allow extra fields (LLM can add `reasoning`, `_debug` keys).
Power users want tight enforcement.

**Change:**
```rust
pub struct StructuredOutputSpec {
    // ...existing fields...

    /// When true, derived from_example schema adds `additionalProperties: false`
    /// to all object schemas. Default: false.
    /// Mirrors OpenAI Structured Outputs `strict: true` behavior.
    #[serde(default)]
    pub strict: Option<bool>,
}
```

In `json_to_schema()` (or a variant `json_to_schema_strict()`):
```rust
if strict {
    json!({
        "type": "object",
        "properties": ...,
        "required": [...],
        "additionalProperties": false
    })
}
```

---

## Task 5: Move `json_to_schema()` to `nika-core/src/schema/`

**Priority:** LOW | **Breaking:** No (re-export from structured.rs) | **Crate:** nika-core

**Rationale:** `json_to_schema` is a pure data transformation utility, not an AST definition.
It belongs in a dedicated `schema` module that can grow to include `merge_schemas()` for
future `from_examples: Vec<SchemaRef>` support.

```
nika-core/src/
├── ast/
│   └── structured.rs  # pub use crate::schema::json_to_schema; (re-export)
└── schema/
    └── mod.rs         # json_to_schema(), merge_schemas() (future)
```

---

## Ordering

1. Task 2 (OutputPolicy.from_example) — no breaking, enables Task 3 cleanly
2. Task 1 (schema: Option) — breaking, do in dedicated PR
3. Task 3 (file prompt injection) — can do after Task 1 or independently
4. Task 4 (strict flag) — isolated, any time
5. Task 5 (module move) — low priority refactor

---

## Non-Goals (explicitly deferred)

- `ExampleRef` newtype — over-engineering for current usage
- `from_examples: Vec<SchemaRef>` (multi-example union) — future feature, architecture supports it
- Making `schema()` getter async — V1 lazy load is correct and sufficient
