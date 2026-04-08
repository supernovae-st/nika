# Phase 3 — nika-macros — MEGA HANDOFF

> **Copy-paste this ENTIRE file as context for a fresh Claude Code session.**
> **Launch this in a SEPARATE terminal from Phase 15 (they don't conflict).**
>
> **Philosophy:** `perfection > timing`. TDD mandatory. Zero shortcuts.

---

## 0. META — READ THIS FIRST

### 0.1 Baseline verification (run BEFORE touching anything)

```bash
cd /Users/thibaut/dev/supernovae/nika/tools/nika
git log --oneline -5                        # confirm you're on main
cargo test --workspace --lib 2>&1 | grep -E "^test result: ok" | awk '{s+=$4} END{print s}'
# Expected: ≥10790
cargo clippy --workspace --lib -- -D warnings 2>&1 | tail -3
# Expected: clean
```

**If baseline is broken, STOP and investigate.**

### 0.2 What you're building

A **proc-macro crate** called `nika-macros` that eliminates ~8,800 LOC of boilerplate via:
- **D1:** `#[builtin_tool]` attribute macro — 40 impls × 90 LOC → 25 LOC each
- **D2:** `#[nika_error(code = "NIKA-XXX")]` derive — auto-generates `code()` method from 44+ match arms
- **D3:** `#[event_task_id]` derive — auto-generates `task_id()` from 100 EventKind variants
- **D4:** `transform!{}` declarative macro — 69 transform apply() match arms → one-liner registrations

### 0.3 Skills you MUST use

| Skill | When |
|-------|------|
| `spn-powers:test-driven-development` | **Before every impl file.** RED first. |
| `spn-powers:verification-before-completion` | **Before every commit.** Full workspace test + clippy. |
| `spn-powers:systematic-debugging` | **When proc-macro expansion fails.** 4-phase framework. |
| `spn-rust:rust-core` | **When designing the attribute syntax.** Ownership, Send+Sync, trait bounds. |

### 0.4 Agents to delegate to

| Agent | When | Prompt template |
|-------|------|-----------------|
| `spn-rust:rust-architect` | **BEFORE commit 1.** Review sealed trait + proc-macro architecture. | *"I'm creating `nika-macros` proc-macro crate. Here's the BuiltinTool trait: <paste trait.rs>. Here's a representative impl: <paste sleep.rs>. Design the `#[builtin_tool]` attribute syntax that: (1) captures name, description, schema from struct attributes, (2) wraps the async fn body in Pin<Box<dyn Future>>, (3) generates impl sealed::Sealed, (4) generates linkme::distributed_slice entry. Give me the EXACT attribute syntax + expanded code. Not options — a verdict."* |
| `feature-dev:code-reviewer` | **After each commit's impl, BEFORE committing.** | *"Review this proc-macro implementation for Phase 3 commit N. Check: (1) hygiene — no identifier collision, (2) span propagation — errors point to user code, (3) trybuild tests cover failure paths, (4) expanded code matches hand-written impls byte-for-byte. Report real issues only."* |

### 0.5 What you may NOT touch

- The 5 verbs — NEVER change
- Schema `nika/workflow@0.12` — NEVER change
- AGPL license — NEVER change
- Existing BuiltinTool impls — DON'T rewrite them in this phase. Phase 12 does the rewrite.
- Shield files — DON'T touch

### 0.6 Git rules

- 1 logical change = 1 commit
- `type(scope): description` format
- Co-author: `Co-Authored-By: Nika 🦋 <nika@supernovae.studio>` — NEVER Claude/Anthropic
- `git add <specific files>` — NEVER `git add -A`
- Do NOT push unless explicitly asked

---

## 1. WHAT EXISTS TODAY (the boilerplate to kill)

### 1.1 BuiltinTool trait — `tools/nika-engine/src/runtime/builtin/trait.rs`

```rust
pub trait BuiltinTool: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str { "" }
    fn parameters_schema(&self) -> serde_json::Value { serde_json::json!({}) }
    fn call<'a>(
        &'a self,
        args: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, NikaError>> + Send + 'a>>;
}
```

**40 impls** across `tools/nika-engine/src/runtime/builtin/*.rs` + `data/*.rs`.

**Example (sleep.rs, 130 LOC of which ~70 is boilerplate):**
```rust
pub struct SleepTool;

impl BuiltinTool for SleepTool {
    fn name(&self) -> &'static str { "sleep" }
    fn description(&self) -> &'static str { "Pause execution for the specified duration" }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "duration": {
                    "type": "string",
                    "description": "Duration to sleep in humantime format"
                }
            },
            "required": ["duration"],
            "additionalProperties": false
        })
    }
    fn call<'a>(&'a self, args: String)
        -> Pin<Box<dyn Future<Output = Result<String, NikaError>> + Send + 'a>>
    {
        Box::pin(async move {
            let params: SleepParams = serde_json::from_str(&args)
                .map_err(|e| NikaError::BuiltinInvalidParams {
                    tool: "nika:sleep".into(),
                    reason: format!("Invalid JSON parameters: {}", e),
                })?;
            // ... business logic ...
            serde_json::to_string(&response)
                .map_err(|e| NikaError::BuiltinToolError { ... })
        })
    }
}
```

**Manual registration in router.rs:**
```rust
tools.insert("sleep", Arc::new(SleepTool));
tools.insert("log", Arc::new(LogTool));
// ... 38 more lines
```

### 1.2 NikaError — `tools/nika-engine/src/error.rs` (2,874 LOC)

- 44+ variants with `#[error("[NIKA-XXX] ...")]` + `#[diagnostic(...)]`
- `code()` method: **44+ match arms** at lines 1089-1290, each `Self::Variant { .. } => "NIKA-XXX"`
- The code string is DUPLICATED: once in `#[error("[NIKA-001] ...")]` and once in `code()` return

**Pattern (repeated 44× across 200 LOC):**
```rust
#[error("[NIKA-001] Failed to parse workflow: {details}")]
#[diagnostic(code(nika::parse_error), help("Check YAML syntax"))]
ParseError { details: String },
// ...
// 1000 lines later:
Self::ParseError { .. } => "NIKA-001",
```

### 1.3 EventKind — `tools/nika-event/src/log.rs` (4,547 LOC)

- **~100 variants** (struct variants with fields)
- `task_id(&self) -> Option<&str>` method at line 1186: ~80 match arms extracting `task_id` field
- `is_workflow_event(&self) -> bool` at line 1295: ~15 match arms

### 1.4 TransformOp — `tools/nika-core/src/binding/transform/`

- **69 variants** in `mod.rs:45-166`
- `apply()` in `apply.rs`: **1,346 LOC**, 69 match arms with repetitive null-guard pattern:
  ```rust
  TransformOp::Upper => match value {
      Value::Null => Err(TransformError::NullInput { op: "upper" }),
      Value::String(s) => Ok(Value::String(s.to_uppercase())),
      _ => Err(type_mismatch("upper", "string", value)),
  },
  ```

### 1.5 Current workspace deps (NONE of the macro deps exist yet)

❌ `syn`, `quote`, `proc-macro2` — not in workspace  
❌ `darling` — not in workspace  
❌ `trybuild`, `macrotest` — not in workspace  
❌ `linkme` — not in workspace  
❌ sealed trait pattern — zero instances in codebase  

---

## 2. TARGET — what the macros produce

### D1: `#[builtin_tool]` → target syntax

**BEFORE (90 LOC per tool):** see §1.1 sleep.rs example above.

**AFTER (25 LOC per tool):**
```rust
use nika_macros::builtin_tool;

#[derive(Debug, Deserialize)]
struct SleepParams {
    duration: String,
}

#[derive(Debug, Serialize)]
struct SleepResponse {
    slept_for_ms: u64,
}

#[builtin_tool(
    name = "sleep",
    description = "Pause execution for the specified duration",
)]
async fn sleep_tool(params: SleepParams) -> Result<SleepResponse, NikaError> {
    let duration = humantime::parse_duration(&params.duration)
        .map_err(|e| NikaError::BuiltinInvalidParams {
            tool: "nika:sleep".into(),
            reason: format!("Invalid duration '{}': {}", params.duration, e),
        })?;
    if duration > MAX_SLEEP_DURATION {
        return Err(NikaError::BuiltinInvalidParams { ... });
    }
    tokio::time::sleep(duration).await;
    Ok(SleepResponse { slept_for_ms: duration.as_millis() as u64 })
}
```

**What the macro generates:**
```rust
pub struct SleepToolStub;

impl crate::sealed::Sealed for SleepToolStub {}

impl BuiltinTool for SleepToolStub {
    fn name(&self) -> &'static str { "sleep" }
    fn description(&self) -> &'static str { "Pause execution for the specified duration" }
    fn parameters_schema(&self) -> serde_json::Value {
        // AUTO-DERIVED from SleepParams via schemars::JsonSchema
        // OR: require the user to impl JsonSchema on the params struct
        schemars::schema_for!(SleepParams).to_value()
    }
    fn call<'a>(&'a self, args: String)
        -> Pin<Box<dyn Future<Output = Result<String, NikaError>> + Send + 'a>>
    {
        Box::pin(async move {
            let params: SleepParams = serde_json::from_str(&args)
                .map_err(|e| NikaError::BuiltinInvalidParams {
                    tool: "nika:sleep".into(),
                    reason: format!("Invalid JSON parameters: {}", e),
                })?;
            let result = sleep_tool(params).await?;
            serde_json::to_string(&result)
                .map_err(|e| NikaError::BuiltinToolError {
                    tool: "nika:sleep".into(),
                    reason: e.to_string(),
                })
        })
    }
}
```

**IMPORTANT DESIGN DECISION:** the `parameters_schema()` generation has two options:
- **Option A:** Use `schemars` crate — `#[derive(JsonSchema)]` on params struct, macro calls `schemars::schema_for!()`. Clean but adds a dep.
- **Option B:** Keep hand-written `serde_json::json!({})` schemas — macro doesn't touch `parameters_schema()`, user overrides via attribute. Less magic, more control.

**Spawn `spn-rust:rust-architect` to decide BEFORE implementing.** Ask explicitly.

### D2: `#[nika_error(code = "NIKA-XXX")]` → target syntax

**BEFORE:** code duplicated in `#[error(...)]` AND in `code()` method.

**AFTER:**
```rust
#[derive(Error, Debug, Diagnostic, NikaErrorCode)]
pub enum NikaError {
    #[error("[NIKA-001] Failed to parse workflow: {details}")]
    #[diagnostic(code(nika::parse_error), help("Check YAML syntax"))]
    #[nika_code("NIKA-001")]
    ParseError { details: String },
    // ...
}
// The NikaErrorCode derive generates:
// impl NikaError {
//     pub fn code(&self) -> &'static str {
//         match self {
//             Self::ParseError { .. } => "NIKA-001",
//             // ... auto-generated for every variant with #[nika_code]
//         }
//     }
// }
```

**Saves:** 200 LOC (44 match arms in `code()`) + eliminates code/error-string sync bugs.

### D3: `#[derive(EventTaskId)]` → target syntax

**BEFORE:** 80+ match arms in `task_id()` method.

**AFTER:**
```rust
#[derive(Debug, Clone, Serialize, Deserialize, EventTaskId)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventKind {
    #[has_task_id]
    TaskScheduled { task_id: Arc<str>, layer: u32, verb: String },
    
    #[has_task_id]
    TaskStarted { task_id: Arc<str>, started_at: Instant },
    
    // No #[has_task_id] → returns None
    WorkflowStarted { workflow: String, task_count: usize },
    // ...
}
// Generates: fn task_id(&self) -> Option<&str> { match self { ... } }
```

**Saves:** ~100 LOC of match arms.

### D4: `transform!{}` → target syntax

**BEFORE:** 69 match arms × ~10 LOC each in `apply.rs` (1,346 LOC).

**AFTER (declarative macro, NOT proc-macro):**
```rust
transform_dispatch! {
    // String transforms (strict null)
    Upper(str) => |s| Ok(Value::String(s.to_uppercase())),
    Lower(str) => |s| Ok(Value::String(s.to_lowercase())),
    Trim(str) => |s| Ok(Value::String(s.trim().to_string())),
    TrimStart(str) => |s| Ok(Value::String(s.trim_start().to_string())),
    TrimEnd(str) => |s| Ok(Value::String(s.trim_end().to_string())),
    
    // Collection transforms (strict null)
    First(array) => |arr| Ok(arr.first().cloned().unwrap_or(Value::Null)),
    Last(array) => |arr| Ok(arr.last().cloned().unwrap_or(Value::Null)),
    
    // Propagating transforms (null → null)
    Length(propagating) => |value| match value {
        Value::Array(a) => Ok(Value::Number(a.len().into())),
        Value::String(s) => Ok(Value::Number(s.chars().count().into())),
        Value::Object(o) => Ok(Value::Number(o.len().into())),
        _ => Err(type_mismatch("length", "array, string, or object", value)),
    },
    
    // Parametric transforms
    Default(val: Value) => |value| /* custom logic */,
    Join(sep: String) => |value| /* custom logic */,
    Slice(start: usize, end: Option<usize>) => |value| /* custom logic */,
}
```

The macro generates the full `match self { TransformOp::X => ... }` with null guards per type annotation (`str`, `array`, `propagating`).

**Saves:** ~500-700 LOC of repetitive null-guard boilerplate.

**NOTE:** Parametric transforms (with args) and complex multi-type transforms (Length, ToJson, TypeOf) need custom bodies inside the macro. The macro handles the DISPATCH + NULL GUARD, the user provides the BODY. This is realistic — a macro that writes the business logic too is vaporware.

---

## 3. COMMIT PLAN — TDD-driven, 7 commits

### Commit 3.1 — Create crate skeleton + workspace wiring

**What:**
- Create `tools/nika-macros/Cargo.toml`
- Create `tools/nika-macros/src/lib.rs` (empty `proc_macro` crate)
- Add to workspace `Cargo.toml` members
- Add workspace deps: `syn = "2"`, `quote = "1"`, `proc-macro2 = "1"`, `darling = "0.20"`
- Add workspace dev-deps: `trybuild = "1"`
- Wire `nika-macros = { path = "../nika-macros" }` in workspace deps

**TDD test (RED):**
```rust
// tools/nika-macros/tests/smoke.rs
#[test]
fn macro_crate_exists() {
    // Compile-time proof: the crate exports something
    let _ = std::any::type_name::<nika_macros::__private::Dummy>();
}
```
This fails until the crate exists.

**Verification:** `cargo test --workspace --lib` still ≥10790. `cargo build -p nika-macros` works.

**Commit message:** `feat(macros): create nika-macros crate skeleton`

---

### Commit 3.2 — `#[nika_error_code]` derive (D2 — EASIEST, start here)

**Why first:** smallest derive, cleanest pattern, good warmup for proc-macro development.

**TDD tests (RED):**
```rust
// tools/nika-macros/tests/nika_error_code.rs
use nika_macros::NikaErrorCode;

#[derive(NikaErrorCode)]
enum TestError {
    #[nika_code("TEST-001")]
    First { reason: String },
    
    #[nika_code("TEST-002")]
    Second,
    
    #[nika_code("TEST-003")]
    Third(String),
}

#[test]
fn code_returns_correct_string() {
    let e = TestError::First { reason: "x".into() };
    assert_eq!(e.code(), "TEST-001");
}

#[test]
fn code_works_for_unit_variant() {
    let e = TestError::Second;
    assert_eq!(e.code(), "TEST-002");
}

#[test]
fn code_works_for_tuple_variant() {
    let e = TestError::Third("x".into());
    assert_eq!(e.code(), "TEST-003");
}
```

**trybuild failure test:**
```rust
// tools/nika-macros/tests/trybuild.rs
#[test]
fn compile_fail_tests() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile_fail/*.rs");
}
```

```rust
// tools/nika-macros/tests/compile_fail/missing_nika_code.rs
use nika_macros::NikaErrorCode;

#[derive(NikaErrorCode)]
enum Bad {
    MissingCode,  // no #[nika_code] attribute → compile error
}

fn main() {}
```

**Implementation:** `tools/nika-macros/src/nika_error_code.rs` (~150 LOC)
- Parse enum with `darling::FromDeriveInput`
- For each variant, extract `#[nika_code("...")]` string
- Generate `impl $name { pub fn code(&self) -> &'static str { match self { ... } } }`
- Error if any variant lacks `#[nika_code]`

**GREEN:** run tests, they pass.

**REFACTOR:** nothing yet.

**Verification:** full workspace test + clippy.

**Then:** wire into NikaError. Replace the hand-written `code()` method in `tools/nika-engine/src/error.rs:1089-1290` with `#[derive(NikaErrorCode)]` + `#[nika_code("NIKA-XXX")]` on each variant. **Delete the 200-LOC match block.**

**Commit message:** `feat(macros): add #[derive(NikaErrorCode)] — auto-generates code() from #[nika_code]`

---

### Commit 3.3 — Wire NikaErrorCode into error.rs

**What:** Apply `#[derive(NikaErrorCode)]` to the real `NikaError` enum in nika-engine.

**TDD test (RED):**
```rust
// In existing error tests
#[test]
fn error_code_from_derive_matches_known_codes() {
    assert_eq!(NikaError::ParseError { details: "x".into() }.code(), "NIKA-001");
    assert_eq!(NikaError::CycleDetected { cycle: "x".into() }.code(), "NIKA-020");
    assert_eq!(NikaError::ProviderApiError { message: "x".into() }.code(), "NIKA-031");
}
```

This test ALREADY passes with the hand-written `code()`. After wiring the derive, it must STILL pass (regression gate).

**GREEN steps:**
1. Add `nika-macros = { workspace = true }` to `nika-engine/Cargo.toml`
2. Add `#[derive(nika_macros::NikaErrorCode)]` to `pub enum NikaError`
3. Add `#[nika_code("NIKA-XXX")]` to each variant (mechanical — one per variant)
4. DELETE the hand-written `code()` method (lines 1089-1290)
5. Run tests — must still pass

**Verification:** `cargo test --workspace --lib` — all pass. `cargo clippy` clean. No behavior change.

**Commit message:** `refactor(error): wire NikaErrorCode derive, delete 200 LOC hand-written code()`

---

### Commit 3.4 — `#[derive(EventTaskId)]` (D3)

**TDD tests:**
```rust
#[derive(EventTaskId)]
enum TestEvent {
    #[has_task_id]
    Started { task_id: Arc<str>, extra: u32 },
    
    NoTaskId { workflow: String },
    
    #[has_task_id]
    Completed { task_id: Arc<str> },
}

#[test]
fn task_id_some_for_tagged_variant() {
    let e = TestEvent::Started { task_id: Arc::from("t1"), extra: 42 };
    assert_eq!(e.task_id(), Some("t1"));
}

#[test]
fn task_id_none_for_untagged_variant() {
    let e = TestEvent::NoTaskId { workflow: "wf".into() };
    assert_eq!(e.task_id(), None);
}
```

**Implementation:** `tools/nika-macros/src/event_task_id.rs` (~120 LOC)
- Parse enum variants
- Variants with `#[has_task_id]` MUST have a `task_id: Arc<str>` field (error otherwise)
- Generate `fn task_id(&self) -> Option<&str> { match self { ... } }`

**Then wire into EventKind** — replace hand-written `task_id()` at `log.rs:1186`.

**Commit message:** `feat(macros): add #[derive(EventTaskId)] — auto-generates task_id() method`

---

### Commit 3.5 — Wire EventTaskId into EventKind

**What:** Apply derive to real EventKind, delete hand-written `task_id()` + potentially `is_workflow_event()`.

**Same pattern as commit 3.3:** regression test → wire derive → delete old code → verify.

**Commit message:** `refactor(event): wire EventTaskId derive, delete ~100 LOC hand-written task_id()`

---

### Commit 3.6 — `transform_dispatch!{}` declarative macro (D4)

**TDD tests:**
```rust
// In nika-core transform tests
#[test]
fn transform_upper_via_macro_matches_manual() {
    let result = TransformOp::Upper.apply(&json!("hello")).unwrap();
    assert_eq!(result, json!("HELLO"));
}
// This test ALREADY exists and passes. It must STILL pass after the macro rewrite.
```

**Implementation:** `tools/nika-core/src/binding/transform/dispatch_macro.rs` (~100 LOC)
- Declarative `macro_rules!` (NOT proc-macro — it's pattern-based dispatch)
- Generates the `match self { ... }` body of `apply()`
- Three null-handling modes: `str` (strict), `array` (strict), `propagating` (null→null)
- Complex transforms keep inline closure bodies

**Wire into apply.rs:** replace the 1,346-LOC `apply()` body with `transform_dispatch!{ ... }` invocation.

**Verification:** ALL 3,240 transform tests must still pass.

**Commit message:** `refactor(transform): replace 1346-LOC apply() with transform_dispatch! macro`

---

### Commit 3.7 — `#[builtin_tool]` attribute macro (D1 — LAST, biggest)

**Why last:** depends on the sealed trait pattern established in commits 3.2-3.5. Also biggest, most complex.

**TDD tests:**
```rust
// tools/nika-macros/tests/builtin_tool.rs
use nika_macros::builtin_tool;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct EchoParams { message: String }

#[derive(Serialize)]
struct EchoResponse { echo: String }

#[builtin_tool(
    name = "echo",
    description = "Echo the message back",
)]
async fn echo_tool(params: EchoParams) -> Result<EchoResponse, nika_macros::BuiltinError> {
    Ok(EchoResponse { echo: params.message })
}

#[test]
fn builtin_tool_name() {
    assert_eq!(EchoToolStub.name(), "echo");
}

#[test]
fn builtin_tool_description() {
    assert_eq!(EchoToolStub.description(), "Echo the message back");
}

#[tokio::test]
async fn builtin_tool_call_roundtrip() {
    let args = r#"{"message":"hello"}"#;
    let result = EchoToolStub.call(args.to_string()).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["echo"], "hello");
}
```

**trybuild failure tests:**
```rust
// tests/compile_fail/builtin_missing_name.rs — error: missing `name` attribute
// tests/compile_fail/builtin_wrong_return.rs — error: must return Result<T, BuiltinError>
// tests/compile_fail/builtin_not_async.rs — error: function must be async
```

**Implementation:** `tools/nika-macros/src/builtin_tool.rs` (~350 LOC)
- Parse `#[builtin_tool(name = "...", description = "...")]` via `darling`
- Generate `XxxToolStub` struct (ZST)
- Generate `impl BuiltinTool for XxxToolStub` with:
  - `name()` → attribute literal
  - `description()` → attribute literal
  - `parameters_schema()` → `serde_json::json!({})` default (user overrides manually, OR use schemars later)
  - `call()` → parse JSON → call the user's async fn → serialize response
- Generate `impl sealed::Sealed for XxxToolStub` (IF sealed trait exists)

**NOTE:** Do NOT wire into real builtins in this commit. That's Phase 12's job (40 tools × rewrite). This commit proves the macro WORKS on a test example.

**Commit message:** `feat(macros): add #[builtin_tool] attribute macro — generates BuiltinTool impl`

---

## 4. PER-COMMIT TDD CYCLE

```
┌──────────────────────────────────────────────────────────┐
│  1. Announce: "I'm using test-driven-development"        │
│  2. Write failing test(s) — trybuild + unit              │
│  3. cargo test -p nika-macros → RED (fails)              │
│  4. Implement the minimum code to pass                   │
│  5. cargo test -p nika-macros → GREEN (passes)           │
│  6. Refactor: remove duplication, tighten visibility      │
│  7. Announce: "I'm using verification-before-completion" │
│  8. cargo test --workspace --lib → ≥10790               │
│  9. cargo clippy --workspace --lib -- -D warnings → 0    │
│ 10. Spawn code-reviewer agent with diff                  │
│ 11. Fix real issues (not style nits)                     │
│ 12. git add <specific files>                             │
│ 13. git commit with Nika 🦋 co-author                    │
└──────────────────────────────────────────────────────────┘
```

---

## 5. CRATE STRUCTURE

```
tools/nika-macros/
├── Cargo.toml
├── src/
│   ├── lib.rs                  — #[proc_macro_derive] + #[proc_macro_attribute] exports
│   ├── nika_error_code.rs      — D2: #[derive(NikaErrorCode)]
│   ├── event_task_id.rs        — D3: #[derive(EventTaskId)]
│   └── builtin_tool.rs         — D1: #[builtin_tool]
└── tests/
    ├── smoke.rs                — crate existence test
    ├── nika_error_code.rs      — D2 unit tests
    ├── event_task_id.rs        — D3 unit tests
    ├── builtin_tool.rs         — D1 unit + async tests
    ├── trybuild.rs             — compile-failure runner
    └── compile_fail/
        ├── missing_nika_code.rs
        ├── builtin_missing_name.rs
        ├── builtin_wrong_return.rs
        └── builtin_not_async.rs
```

**`transform_dispatch!` lives in nika-core, NOT in nika-macros** — it's a `macro_rules!` declarative macro, not a proc-macro. Declarative macros don't need a separate crate.

### Cargo.toml

```toml
[package]
name = "nika-macros"
version.workspace = true
edition.workspace = true
authors.workspace = true
description = "Proc-macros for Nika — #[builtin_tool], #[derive(NikaErrorCode)], #[derive(EventTaskId)]"
license.workspace = true
repository.workspace = true
rust-version.workspace = true
publish = true

[lib]
proc-macro = true

[dependencies]
syn = { version = "2", features = ["full", "extra-traits"] }
quote = "1"
proc-macro2 = "1"
darling = "0.20"

[dev-dependencies]
trybuild = "1"
serde = { workspace = true, features = ["derive"] }
serde_json = { workspace = true }
tokio = { workspace = true, features = ["rt", "macros"] }

[lints]
workspace = true
```

---

## 6. DEPENDENCIES TO ADD TO WORKSPACE

In `tools/Cargo.toml` `[workspace.dependencies]`:

```toml
# Proc-macro tooling
syn = { version = "2", features = ["full", "extra-traits"] }
quote = "1"
proc-macro2 = "1"
darling = "0.20"

# Testing proc-macros
trybuild = "1"
```

---

## 7. RISK REGISTER

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| `darling` API breaks between versions | LOW | Build fail | Pin exact version |
| proc-macro hygiene bug (identifier collision) | MEDIUM | Runtime bug | trybuild compile-fail tests catch it |
| NikaError derive doesn't handle `#[error(transparent)]` variants | HIGH | Compile fail | Special-case `transparent` variants in D2 — they have no NIKA code |
| EventKind has variants WITHOUT `task_id` field even when tagged | MEDIUM | Wrong expansion | Validate field exists in derive, emit clear error |
| `transform_dispatch!` can't handle parametric transforms (Join, Slice, etc.) | HIGH | Macro incomplete | Parametric transforms keep manual bodies — macro handles dispatch+null only |
| Parallel Phase 15 agent conflicts with workspace Cargo.toml edits | LOW | Merge conflict | Phase 15 doesn't touch workspace Cargo.toml |
| proc-macro crate slows workspace compile | LOW | DX regression | proc-macros compile once, cached thereafter |

---

## 8. DECISION TREE

```
"Derive doesn't compile on real NikaError?"
├── #[error(transparent)] variant? → skip it, no #[nika_code] needed
├── Tuple variant? → handle (Self::X(inner) => inner.code())
├── Complex expression in #[error("...")]? → irrelevant, code() is separate
└── Other → systematic-debugging skill, read the syn parse error

"transform_dispatch! doesn't handle variant X?"
├── Parametric (has args)? → keep manual body, macro wraps dispatch only
├── Multi-type (accepts string OR array)? → custom match body, not auto-null-guard
└── Complex (jq, regex)? → definitely manual body

"trybuild test fails on CI but passes locally?"
├── Different Rust version? → pin msrv
├── Feature flag difference? → check workspace default features
└── Path difference? → use relative paths in test files
```

---

## 9. VERIFICATION COMMANDS

```bash
# Per-commit
cd /Users/thibaut/dev/supernovae/nika/tools/nika
cargo test -p nika-macros                      # macro tests only
cargo test --workspace --lib 2>&1 | grep -E "^test result: ok" | awk '{s+=$4} END{print s}'
cargo clippy --workspace --lib -- -D warnings 2>&1 | tail -3

# After wiring commits (3.3, 3.5, 3.6)
cargo test -p nika-engine --lib error          # error.rs tests
cargo test -p nika-event --lib                 # event tests
cargo test -p nika-core --lib transform        # transform tests

# Final
wc -l tools/nika-engine/src/error.rs           # should drop ~200 LOC
wc -l tools/nika-event/src/log.rs              # should drop ~100 LOC
wc -l tools/nika-core/src/binding/transform/apply.rs  # should drop ~500+ LOC
```

---

## 10. MANDATORY READS

```
1. This file (complete — 500+ lines of context)
2. tools/nika-engine/src/runtime/builtin/trait.rs       — BuiltinTool trait
3. tools/nika-engine/src/runtime/builtin/sleep.rs       — representative tool (simplest)
4. tools/nika-engine/src/runtime/builtin/assert.rs      — another example
5. tools/nika-engine/src/error.rs:88-350                — NikaError enum + variants
6. tools/nika-engine/src/error.rs:1089-1290             — code() method to DELETE
7. tools/nika-event/src/log.rs:150-400                  — EventKind enum
8. tools/nika-event/src/log.rs:1186-1310                — task_id() + is_workflow_event()
9. tools/nika-core/src/binding/transform/mod.rs:45-166  — TransformOp enum
10. tools/nika-core/src/binding/transform/apply.rs      — apply() to rewrite
```

---

## 11. TL;DR FOR THE AGENT

> **You are building `nika-macros` — a proc-macro crate that kills 8,800 LOC of boilerplate.**
>
> **7 commits in order:**
> 1. Crate skeleton + workspace wiring
> 2. `#[derive(NikaErrorCode)]` — easiest derive, good warmup
> 3. Wire NikaErrorCode into real NikaError — delete 200 LOC
> 4. `#[derive(EventTaskId)]` — second derive
> 5. Wire EventTaskId into real EventKind — delete ~100 LOC
> 6. `transform_dispatch!{}` declarative macro — rewrite 1346 LOC apply()
> 7. `#[builtin_tool]` attribute macro — biggest, proven on test example (Phase 12 wires it for real)
>
> **TDD every commit.** RED first. trybuild for compile-fail paths. Full workspace verify after every commit.
>
> **Spawn `spn-rust:rust-architect`** before commit 7 for the `#[builtin_tool]` design (schemars vs manual schema, sealed trait wiring, linkme registration).
>
> **Spawn `feature-dev:code-reviewer`** after each commit's impl but BEFORE committing.
>
> **Baseline:** ≥10,790 tests, 0 clippy warnings. Never regress.
>
> **Do NOT push** unless explicitly asked. Do NOT rewrite existing builtins — that's Phase 12.
>
> GOOO
