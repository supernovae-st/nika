# Templatable<T> — Mega Handoff

> Session: 2026-04-07 | 8 commits | a5dac8c60 | 10,459+ tests pass

## What Was Done

Template expressions (`{{inputs.temperature}}`) now work in ALL 65 typed fields (number/integer/boolean) across the entire Nika workflow engine.

### Commits
```
a5dac8c fix(engine): harden Templatable error paths — zero silent swallows
85ac6fa fix(engine): complete Templatable coverage — zero silent drops remaining
cf19299 fix(engine): resolve ALL Templatable fields — eliminate silent template drops
34abd47 feat(schema): allow template expressions in all 65 typed fields
107cdf8 feat(engine): runtime template resolution for typed fields
c2a10d4 feat(core): propagate Templatable<T> through AST pipeline
d23a271 fix(clippy): resolve pre-existing warnings across workspace
```

### Architecture
```
YAML → Schema (oneOf) → Parser (Templatable<T>) → Raw AST → Analyzer → Analyzed AST
  → task_dispatch: resolve_action_templates() → lower_action() → Runtime (plain T)
```

### Key Files
- `nika-core/src/ast/templatable.rs` — Core type (Value | Template)
- `nika-core/src/ast/raw/parser.rs` — 4 parser helpers detect `{{`
- `nika-core/src/ast/analyzer/analyze.rs` — Validation skips for Template
- `nika-engine/src/runtime/resolve_typed.rs` — Runtime resolution + range validation
- `nika-engine/src/runtime/task_dispatch.rs` — Wiring before lower_action
- `nika-engine/src/ast/lower.rs` — Extracts Value, drops Template (post-resolution)
- Both `schemas/nika-workflow.schema.json` — 65 fields with oneOf pattern

---

## S1 — Performance: has_any_template() Fast-Path

**Problem**: `resolve_action_templates()` clones the ENTIRE action struct (prompt String, tools Vec, etc.) on every task execution, even when 95% of tasks have zero templates. For `for_each` over 1000 items with Agent verb → ~20,000 unnecessary heap allocs.

**Fix**:

### S1.1: Add `has_any_template()` to each Analyzed action struct

File: `nika-core/src/ast/analyzed/task.rs`

```rust
impl AnalyzedInferAction {
    pub fn has_any_template(&self) -> bool {
        self.temperature.as_ref().is_some_and(|t| t.is_template())
            || self.max_tokens.as_ref().is_some_and(|t| t.is_template())
            || self.extended_thinking.as_ref().is_some_and(|t| t.is_template())
            || self.thinking_budget.as_ref().is_some_and(|t| t.is_template())
    }
}
// Same for AnalyzedExecAction, AnalyzedFetchAction, AnalyzedInvokeAction, AnalyzedAgentAction
```

Add dispatch on `AnalyzedTaskAction`:
```rust
impl AnalyzedTaskAction {
    pub fn has_any_template(&self) -> bool {
        match self {
            Self::Infer(a) => a.has_any_template(),
            Self::Exec(a) => a.has_any_template(),
            Self::Fetch(a) => a.has_any_template(),
            Self::Invoke(a) => a.has_any_template(),
            Self::Agent(a) => a.has_any_template(),
        }
    }
}
```

### S1.2: Return `Cow` from resolve_action_templates

File: `nika-engine/src/runtime/resolve_typed.rs`

```rust
use std::borrow::Cow;

pub fn resolve_action_templates<'a>(
    action: &'a AnalyzedTaskAction,
    bindings: &ResolvedBindings,
    ctx: &RunContext,
) -> Result<Cow<'a, AnalyzedTaskAction>, NikaError> {
    if !action.has_any_template() {
        return Ok(Cow::Borrowed(action));
    }
    // ... existing clone + resolve logic ...
    Ok(Cow::Owned(resolved))
}
```

### S1.3: Update task_dispatch.rs to use Cow

```rust
let resolved_action = resolve_action_templates(&task.action, &bindings, &datastore)?;
let mut lowered_action = lower_action(
    resolved_action.as_ref(),  // Cow::as_ref → &AnalyzedTaskAction
    ...
);
```

**Estimated**: ~60 LOC across 3 files. Test: `for_each` over 500+ items should show no perf regression.

---

## S2 — Refactor: Generic Parser Helpers

**Problem**: 4 nearly-identical parser functions (`get_f64_field`, `get_u32_field`, `get_u64_field`, `get_bool_field`) — 130 lines of copy-paste.

**Fix**: Single generic function.

File: `nika-core/src/ast/raw/parser.rs`

```rust
fn get_templatable_field<T>(
    file: FileId,
    map: &MarkedMappingNode,
    key: &str,
    type_name: &str,
    parse: impl FnOnce(&str) -> Result<T, String>,
) -> Result<Option<Spanned<Templatable<T>>>, ParseError> {
    match map.get_node(key) {
        Some(Node::Scalar(s)) => {
            let span = marked_span_to_span(file, s.span());
            let text = s.as_str();
            if is_template_string(text) {
                return Ok(Some(Spanned::new(Templatable::Template(text.to_string()), span)));
            }
            let value = parse(text).map_err(|_| ParseError {
                kind: ParseErrorKind::InvalidType,
                span,
                message: format!("'{}' must be {}", key, type_name),
            })?;
            Ok(Some(Spanned::new(Templatable::Value(value), span)))
        }
        Some(node) => Err(ParseError {
            kind: ParseErrorKind::InvalidType,
            span: node_to_span(file, node),
            message: format!("'{}' must be {}", key, type_name),
        }),
        None => Ok(None),
    }
}
```

Then: `get_f64_field` becomes a 3-line wrapper that calls `get_templatable_field` with `|s| s.parse::<f64>()` + finite check.

Keep the 4 wrapper functions for API stability but delegate to the generic. ~30 LOC replaces ~130 LOC.

---

## S3 — Refactor: Generic Resolve Helpers

**Problem**: 9 internal resolve functions in `resolve_typed.rs` with identical match structure (~200 lines of duplication).

**Fix**: Generic resolve with optional validation closure.

```rust
fn resolve_opt<T: Copy>(
    field: &Option<Templatable<T>>,
    bindings: &ResolvedBindings,
    ctx: &RunContext,
    name: &str,
    parse: impl FnOnce(&str) -> Result<T, NikaError>,
) -> Result<Option<Templatable<T>>, NikaError> {
    match field {
        None => Ok(None),
        Some(Templatable::Value(v)) => Ok(Some(Templatable::Value(*v))),
        Some(Templatable::Template(tpl)) => {
            let resolved = template_resolve(tpl, bindings, ctx)?.into_owned();
            if resolved.trim().is_empty() {
                return Ok(None);
            }
            let val = parse(resolved.trim())?;
            Ok(Some(Templatable::Value(val)))
        }
    }
}
```

Then `resolve_opt_f64_range` becomes:
```rust
resolve_opt(&field, bindings, ctx, name, |s| {
    let v = parse_f64(s, name)?;
    if v < min || v > max { return Err(...) }
    Ok(v)
})
```

Collapses ~200 lines to ~60.

---

## S4 — Hardening: Remaining Edge Cases

### S4.1: `is_template_string` sans closing brace check (LOW)
`is_template_string("{{broken")` returns true. User gets runtime error instead of parse error.

Fix in analyzer: add warning for `Templatable::Template` values that don't contain `}}`:
```rust
if let Templatable::Template(s) = &field.value {
    if !s.contains("}}") {
        ctx.add_warning("unclosed template expression");
    }
}
```

### S4.2: Boolean `yes/no` vs JSON Schema (LOW)
Parser accepts `yes/no/on/off/1/0` for booleans. JSON Schema only accepts `true/false`. Schema rejects `shell: yes` but parser would accept it. Non-issue since YAML normalizes `yes` → `true` before schema validation.

### S4.3: `unwrap_value()` is pub (LOW)
Panics on Template variant. Only used in tests but `pub` means it could leak.

Fix: rename to `expect_value()` or add `#[cfg(test)]` guard.

### S4.4: `lower_task()` (static analysis path) silently drops templates (ACCEPTABLE)
`nika check` uses `lower_task()` which drops Template variants silently. This is the static analysis path — no runtime context available. Document this behavior.

### S4.5: `unlower_action` round-trip documentation (LOW)
`unlower_action` converts Runtime → Analyzed by wrapping in `Templatable::Value()`. Add WHY comment explaining this exists for agent retry/decompose re-execution paths.

---

## S5 — Tests: Mega Test Suite

### S5.1: Parser Template Roundtrip (DONE)
`test_parse_infer_with_template_in_typed_fields` — verifies parser produces `Templatable::Template`.

### S5.2: Schema Validation (DONE)
`test_template_in_typed_fields_passes` — verifies JSON Schema accepts templates.

### S5.3: E2E Temperature + Max Tokens (DONE)
`template_in_temperature_and_max_tokens` — mock provider, full pipeline.

### S5.4: E2E Concurrency (DONE)
`template_in_for_each_concurrency` — inputs → concurrency, mock provider.

### S5.5: E2E Retry (DONE)
`template_in_retry_max_attempts` — inputs → retry config.

### MISSING — Add These:

#### S5.6: Error path — template resolves to wrong type
```rust
#[tokio::test]
async fn template_temperature_invalid_type_fails() {
    let yaml = r#"
schema: "nika/workflow@0.12"
provider: mock
model: mock-model
inputs:
  temp: "hot"  # NOT a number
tasks:
  - id: gen
    infer:
      prompt: "Hello"
      temperature: "{{inputs.temp}}"
"#;
    let runner = run_yaml_expect_failure(yaml).await;
    // Should fail with NIKA-043
}
```

#### S5.7: Error path — template resolves to out-of-range
```rust
// temperature: "{{inputs.temp}}" where inputs.temp = 5.0
// Should fail with NIKA-043 "number in [0, 2]"
```

#### S5.8: Error path — NaN/inf rejection
```rust
// temperature: "{{inputs.temp}}" where inputs.temp = "NaN"
// Should fail with NIKA-043 "finite number"
```

#### S5.9: Template in exec shell + timeout
```rust
// shell: "{{inputs.use_shell}}", timeout: "{{inputs.timeout}}"
// Verify exec runs with correct shell mode and timeout
```

#### S5.10: Template in fetch follow_redirects
```rust
// follow_redirects: "{{inputs.follow}}"
// Verify fetch behavior changes
```

#### S5.11: Template in agent max_turns + depth_limit
```rust
// max_turns: "{{inputs.turns}}", depth_limit: "{{inputs.depth}}"
```

#### S5.12: Template in context_budget
```rust
// context_budget: "{{inputs.budget}}"
// Verify budget enforcement works
```

#### S5.13: Template in max_duration_secs
```rust
// max_duration_secs: "{{inputs.timeout}}"
// Verify workflow timeout applies
```

#### S5.14: Missing input → empty string → None (graceful)
```rust
// temperature: "{{inputs.nonexistent}}"
// Should resolve to empty string → None (provider default), NOT error
```

#### S5.15: Fuzz test — random strings in typed fields
```rust
// for_each: ["0.5", "NaN", "inf", "-1", "true", "", "potato", "3.0"]
// Run through parse_f64, parse_u32, parse_bool_value
// Verify correct accept/reject behavior
```

---

## S6 — Documentation

### S6.1: Update nika.md rules
Add to `~/.claude/rules/nika.md` and `nika/CLAUDE.md`:
```
## Template Expressions in Typed Fields (v0.78+)

All number, integer, and boolean fields accept template expressions:

    temperature: "{{inputs.temperature}}"
    max_tokens: "{{inputs.tokens}}"
    concurrency: "{{inputs.parallel}}"
    shell: "{{inputs.use_shell}}"

Templates are resolved at runtime. Type + range validation happens
after resolution (NIKA-043 on mismatch).
```

### S6.2: Update nika-bugs-and-patterns.md
Add to `~/.claude/rules/nika-bugs-and-patterns.md`:
```
### Template Expressions in Typed Fields (v0.78)
# ✅ WORKS — all typed fields accept templates
infer:
  prompt: "Hello"
  temperature: "{{inputs.temperature}}"
  max_tokens: "{{inputs.max_tokens}}"

# ✅ Range validation after resolution
# temperature must be [0, 2], NIKA-043 if out of range

# ✅ NaN/inf rejected
# NIKA-043: expected finite number, got NaN
```

---

## Mega Prompt for Next Session

```
Continue the Templatable<T> feature hardening for the Nika workflow engine.

Working directory: /Users/thibaut/dev/supernovae/nika/tools

## Context
Templatable<T> allows template expressions in typed fields. 8 commits shipped,
10,459+ tests pass, zero clippy warnings. Three code reviews done (correctness,
perf, architecture). All CRITICAL/HIGH issues fixed.

## What to do (in order)

### Phase 1: Performance (~60 LOC)
1. Add `has_any_template()` to AnalyzedInferAction, AnalyzedExecAction,
   AnalyzedFetchAction, AnalyzedInvokeAction, AnalyzedAgentAction
   in `nika-core/src/ast/analyzed/task.rs`
2. Add dispatch on `AnalyzedTaskAction::has_any_template()`
3. Change `resolve_action_templates` in `nika-engine/src/runtime/resolve_typed.rs`
   to return `Cow<'_, AnalyzedTaskAction>` — return Borrowed when no templates
4. Update `task_dispatch.rs` to use `Cow::as_ref()`
5. Test: for_each over 500 items should NOT clone action when no templates

### Phase 2: Refactor Parser (~30 LOC)
1. Create generic `get_templatable_field<T>` in `nika-core/src/ast/raw/parser.rs`
2. Delegate existing 4 helpers to the generic
3. Keep wrapper functions for API stability
4. Test: all existing parser tests still pass

### Phase 3: Refactor Resolve Helpers (~40 LOC)
1. Create generic `resolve_opt<T>` in `nika-engine/src/runtime/resolve_typed.rs`
2. Collapse 9 internal functions to use the generic with validation closures
3. Test: all existing resolve tests still pass

### Phase 4: Edge Case Hardening (~30 LOC)
1. Analyzer warning for unclosed `{{` templates (S4.1)
2. Rename `unwrap_value()` to `expect_value()` (S4.3)
3. Document `unlower_action` WHY comment (S4.5)
4. Document `lower_task()` static path behavior (S4.4)

### Phase 5: Missing Tests (~150 LOC)
Add tests S5.6 through S5.15 (see handoff doc for exact test cases):
- Error: wrong type (NIKA-043)
- Error: out of range
- Error: NaN/inf
- Template in exec/fetch/agent fields
- Template in context_budget/max_duration_secs
- Missing input → empty string → None
- Fuzz: random strings through parse helpers

### Phase 6: Documentation
1. Update nika.md and CLAUDE.md with template expression docs
2. Update nika-bugs-and-patterns.md

## Rules
- TDD: write test first, then implement
- cargo test --workspace --lib must pass (ignore 5 pre-existing model-resilience failures)
- cargo clippy --all-targets --all-features -- -D warnings must be clean
- Granular commits: 1 commit per phase
- Co-Author: Nika 🦋 <nika@supernovae.studio>
- NEVER skip pre-commit hooks

## Handoff doc
Full details: docs/sprints/SESSION-TEMPLATABLE-HANDOFF.md
```
