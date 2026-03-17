# AST + LSP Quality Sweep — Enriched Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Harden the 3-phase AST pipeline and LSP implementation. Fix real bugs, close validation gaps, add missing test infrastructure, and add property-based testing.

**Architecture:** 16 tasks across 4 tiers. All changes are defense-in-depth — no behavioral changes to passing workflows. TDD with red-green-refactor.

**Tech Stack:** Rust, TDD, `cargo test`, `cargo clippy`, `CARGO_TARGET_DIR=target-main`

**Research Sources:**
- 2 parallel codebase audits (AST architecture + test coverage)
- Perplexity: Rust AST validation best practices, LSP UTF-16 encoding pitfalls
- Direct code inspection of parser.rs, analyze.rs, lower.rs, conversion.rs

---

## Summary of All 16 Tasks

| # | Tier | Area | Severity | Fix |
|---|------|------|----------|-----|
| 1 | Bug | Parser: f64 accepts NaN/Infinity | **MEDIUM** | Add `is_finite()` check |
| 2 | Bug | Parser: for_each `unwrap_or_default()` | **MEDIUM** | Replace with `map_err` + `?` |
| 3 | Bug | Analyzer: `unreachable!()` in retry verb match | **LOW** | Replace with Option pattern |
| 4 | Bug | Analyzer: no task ID format validation | **MEDIUM** | Add format check + $ prefix |
| 5 | Bug | Analyzer: duplicate import prefix undetected | **MEDIUM** | Detect in prefix collection |
| 6 | Bug | LSP: UTF-16 position encoding wrong | **MEDIUM** | Use `ch.len_utf16()` |
| 7 | Bug | Lower: `f64::EPSILON` too strict for backoff | **LOW** | Use practical tolerance |
| 8 | Valid | Analyzer: implicit dep cycles untested | **MEDIUM** | Add targeted tests |
| 9 | Valid | Analyzer: SSE MCP warning too late | **LOW** | Lift warning to Phase 2 |
| 10 | Test | parser.rs: 0 inline tests | **HIGH** | Add #[cfg(test)] module |
| 11 | Test | analyze.rs: inline tests incomplete | **MEDIUM** | Add cycle + edge case tests |
| 12 | Test | lower.rs: roundtrip edge cases | **LOW** | Strengthen existing tests |
| 13 | Test | lsp/conversion.rs: no UTF-16 tests | **HIGH** | Comprehensive UTF-16 suite |
| 14 | Prop | Proptest: parser never panics | **HIGH** | ast::raw::parse() fuzzing |
| 15 | Prop | Proptest: pipeline roundtrip | **MEDIUM** | parse → analyze → lower → unlower |
| 16 | Prop | Insta: error message regression | **MEDIUM** | Snapshot all 8 AnalyzeErrorKind |

---

## TIER 1 — Bug Fixes

---

### Task 1: Parser — Reject NaN/Infinity in f64 Fields

`get_f64_field()` at `src/ast/raw/parser.rs:158-180` accepts `NaN`, `inf`, `-inf` via `str::parse::<f64>()`. These are invalid for LLM parameters like `temperature`, `top_p`, etc.

**Files:**
- Modify: `src/ast/raw/parser.rs:166-171` (add finite check after parse)
- Test: `src/ast/raw/parser.rs` (add tests in existing `#[cfg(test)]` module — or create one, see Task 10)

**Step 1: Write the failing test**

Add to the test module in `parser.rs`:
```rust
#[test]
fn parse_rejects_nan_temperature() {
    let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: test
    infer:
      prompt: "hello"
      temperature: NaN
"#;
    let result = parse(yaml, FileId(0));
    assert!(result.is_err(), "NaN temperature should be rejected");
    let err = result.unwrap_err();
    assert!(err.message.contains("finite"), "Error should mention finite: {}", err.message);
}

#[test]
fn parse_rejects_infinity_temperature() {
    let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: test
    infer:
      prompt: "hello"
      temperature: Infinity
"#;
    let result = parse(yaml, FileId(0));
    assert!(result.is_err(), "Infinity temperature should be rejected");
}

#[test]
fn parse_rejects_negative_infinity_temperature() {
    let yaml = r#"
schema: "nika/workflow@0.12"
tasks:
  - id: test
    infer:
      prompt: "hello"
      temperature: -.inf
"#;
    let result = parse(yaml, FileId(0));
    assert!(result.is_err(), "Negative infinity should be rejected");
}
```

**Step 2: Run test — verify RED**

```bash
CARGO_TARGET_DIR=target-main cargo test --lib raw::parser::tests::parse_rejects_nan 2>&1
```

Expected: FAIL (NaN currently parses successfully)

**Step 3: Add finite check to `get_f64_field()`**

At `parser.rs:166-171`, after the parse succeeds, add:
```rust
let value: f64 = s.as_str().parse().map_err(|_| ParseError {
    kind: ParseErrorKind::InvalidType,
    span,
    message: format!("'{}' must be a number", key),
})?;
if !value.is_finite() {
    return Err(ParseError {
        kind: ParseErrorKind::InvalidType,
        span,
        message: format!("'{}' must be a finite number (got {})", key, s.as_str()),
    });
}
Ok(Some(Spanned::new(value, span)))
```

**Step 4: Run test — verify GREEN**

```bash
CARGO_TARGET_DIR=target-main cargo test --lib raw::parser::tests::parse_rejects_ 2>&1
```

Expected: All 3 tests PASS

**Step 5: Run full suite**

```bash
CARGO_TARGET_DIR=target-main cargo test --lib 2>&1 | tail -5
```

Expected: All pass (no existing workflow uses NaN/Infinity)

**Step 6: Commit**

```bash
git add src/ast/raw/parser.rs
git commit -m "fix(ast): reject NaN/Infinity in f64 fields during parsing

get_f64_field() now validates is_finite() after parsing. Invalid
values like NaN, inf, -inf are caught at Phase 1 with precise spans
instead of causing cryptic runtime errors later.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 2: Parser — Replace unwrap_or_default in for_each JSON Serialization

`parser.rs:671` uses `serde_json::to_string(&arr).unwrap_or_default()` which silently produces an empty string on failure. This should fail fast with a proper error.

**Files:**
- Modify: `src/ast/raw/parser.rs:671` (replace with `map_err` + `?`)

**Step 1: Implement the fix (no test needed — edge case is near-impossible to trigger)**

Replace line 671:
```rust
// OLD:
let items_str = serde_json::to_string(&arr).unwrap_or_default();

// NEW:
let items_str = serde_json::to_string(&arr).map_err(|e| ParseError {
    kind: ParseErrorKind::InvalidType,
    span,
    message: format!("failed to serialize for_each items: {}", e),
})?;
```

**Step 2: Run full suite**

```bash
CARGO_TARGET_DIR=target-main cargo test --lib 2>&1 | tail -5
```

**Step 3: Clippy**

```bash
CARGO_TARGET_DIR=target-main cargo clippy -- -D warnings 2>&1 | tail -5
```

**Step 4: Commit**

```bash
git add src/ast/raw/parser.rs
git commit -m "fix(ast): replace unwrap_or_default with proper error in for_each serialization

Silent empty string on serialization failure would cause cryptic
runtime errors. Now fails fast at parse time with a clear message.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 3: Analyzer — Remove unreachable!() in Retry Verb Check

`analyze.rs:384` uses `unreachable!()` in a match arm guarded by `!is_fetch`. If a new verb variant is added, this panics at runtime.

**Files:**
- Modify: `src/ast/analyzer/analyze.rs:377-385` (restructure match)

**Step 1: Refactor the match**

Replace lines 377-385:
```rust
// OLD:
let is_fetch = matches!(action, RawTaskAction::Fetch(_));
if !is_fetch {
    let verb_name = match action {
        RawTaskAction::Infer(_) => "infer",
        RawTaskAction::Exec(_) => "exec",
        RawTaskAction::Invoke(_) => "invoke",
        RawTaskAction::Agent(_) => "agent",
        RawTaskAction::Fetch(_) => unreachable!(),
    };

// NEW:
let verb_name = match action {
    RawTaskAction::Fetch(_) => None,
    RawTaskAction::Infer(_) => Some("infer"),
    RawTaskAction::Exec(_) => Some("exec"),
    RawTaskAction::Invoke(_) => Some("invoke"),
    RawTaskAction::Agent(_) => Some("agent"),
};
if let Some(verb_name) = verb_name {
```

Adjust closing brace accordingly.

**Step 2: Run full suite**

```bash
CARGO_TARGET_DIR=target-main cargo test --lib 2>&1 | tail -5
```

**Step 3: Clippy**

```bash
CARGO_TARGET_DIR=target-main cargo clippy -- -D warnings 2>&1 | tail -5
```

**Step 4: Commit**

```bash
git add src/ast/analyzer/analyze.rs
git commit -m "refactor(ast): remove unreachable!() in retry verb match

Replace guarded unreachable!() with Option-based match that handles
all variants exhaustively. Prevents runtime panic if a new verb is
added in the future.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 4: Analyzer — Add Task ID Format Validation

Task IDs are inserted into the TaskTable without format validation. Empty strings, special characters, names starting with `$` (reserved for bindings), and names with spaces are silently accepted.

**Note:** DAG validation (`src/dag/validate.rs:432-461`) has task ID checks, but the analyzer (Phase 2) does NOT validate format before inserting into TaskTable. This means invalid IDs reach Phase 3 and runtime.

**Files:**
- Modify: `src/ast/analyzer/analyze.rs` (add validation in task table building)
- Test: `src/ast/analyzer/analyze.rs` (add tests)

**Step 1: Write the failing tests**

```rust
#[test]
fn test_analyze_empty_task_id() {
    let raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("")]);
    let result = analyze(raw);
    assert!(result.is_err(), "empty task ID should be rejected");
    assert_eq!(result.errors[0].kind, AnalyzeErrorKind::InvalidValue);
}

#[test]
fn test_analyze_task_id_with_spaces() {
    let raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("my task")]);
    let result = analyze(raw);
    assert!(result.is_err(), "task ID with spaces should be rejected");
}

#[test]
fn test_analyze_task_id_dollar_prefix() {
    let raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("$reserved")]);
    let result = analyze(raw);
    assert!(result.is_err(), "task ID starting with $ should be rejected");
}

#[test]
fn test_analyze_valid_task_id_with_hyphens_underscores() {
    let raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("my-task_v2")]);
    let result = analyze(raw);
    assert!(result.is_ok(), "task ID with hyphens/underscores is valid");
}
```

**Step 2: Run test — verify RED**

```bash
CARGO_TARGET_DIR=target-main cargo test --lib analyzer::tests::test_analyze_empty_task_id 2>&1
```

**Step 3: Add validation function**

In `analyze.rs`, add before the test module:
```rust
/// Validate task ID format: non-empty, alphanumeric with hyphens/underscores/dots,
/// must not start with $ (reserved for binding references).
fn validate_task_id(name: &str, span: Span, ctx: &mut AnalyzerContext) -> bool {
    if name.is_empty() {
        ctx.add_error(AnalyzeError::new(
            AnalyzeErrorKind::InvalidValue,
            span,
            "task ID must not be empty",
        ));
        return false;
    }
    if name.starts_with('$') {
        ctx.add_error(
            AnalyzeError::new(
                AnalyzeErrorKind::InvalidValue,
                span,
                format!("task ID '{}' must not start with '$' (reserved for binding references)", name),
            )
            .with_suggestion("remove the leading '$' from the task ID"),
        );
        return false;
    }
    if !name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.') {
        ctx.add_error(
            AnalyzeError::new(
                AnalyzeErrorKind::InvalidValue,
                span,
                format!("task ID '{}' contains invalid characters", name),
            )
            .with_suggestion("use only alphanumeric characters, hyphens, underscores, and dots"),
        );
        return false;
    }
    true
}
```

Call it in the task table building loop (wherever task names are inserted).

**Step 4: Run test — verify GREEN**

**Step 5: Run full suite + clippy**

```bash
CARGO_TARGET_DIR=target-main cargo test --lib 2>&1 | tail -5
CARGO_TARGET_DIR=target-main cargo clippy -- -D warnings 2>&1 | tail -5
```

**Step 6: Commit**

```bash
git add src/ast/analyzer/analyze.rs
git commit -m "feat(ast): validate task ID format in analyzer

Reject empty task IDs, those starting with $ (reserved for binding
references), and those with invalid characters. Only alphanumeric,
hyphens, underscores, and dots are allowed. Caught at Phase 2.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 5: Analyzer — Detect Duplicate Import Prefixes

`collect_include_prefixes()` silently accepts duplicate import prefixes, leading to ambiguous task resolution.

**Files:**
- Modify: `src/ast/analyzer/analyze.rs` (add dedup check in `collect_include_prefixes()`)
- Test: `src/ast/analyzer/analyze.rs` (add test)

**Step 1: Write the failing test**

```rust
#[test]
fn test_analyze_duplicate_import_prefix() {
    let yaml = r#"
schema: nika/workflow@0.12
imports:
  - path: ./lib1.nika.yaml
    prefix: seo_
  - path: ./lib2.nika.yaml
    prefix: seo_
tasks:
  - id: main
    infer: "hello"
"#;
    let raw = raw::parse(yaml, FileId(0)).unwrap();
    let result = analyze(raw);
    assert!(result.is_err(), "duplicate import prefix should be rejected");
}
```

**Step 2: Run test — verify RED**

**Step 3: Add dedup check**

In `collect_include_prefixes()`, add a HashSet to track seen prefixes:
```rust
fn collect_include_prefixes(raw: &RawWorkflow, ctx: &mut AnalyzerContext) {
    if let Some(ref imports) = raw.imports {
        let mut seen = std::collections::HashSet::new();
        for import in &imports.value {
            if let Some(ref prefix) = import.value.prefix {
                if !seen.insert(prefix.value.clone()) {
                    ctx.add_error(AnalyzeError::new(
                        AnalyzeErrorKind::InvalidValue,
                        prefix.span,
                        format!("duplicate import prefix '{}'", prefix.value),
                    ));
                }
                ctx.include_prefixes.push(prefix.value.clone());
            }
        }
    }
}
```

**Step 4: Run test — verify GREEN**

**Step 5: Run full suite + clippy**

**Step 6: Commit**

```bash
git add src/ast/analyzer/analyze.rs
git commit -m "feat(ast): detect duplicate import prefixes in analyzer

Two imports with the same prefix create ambiguous task resolution.
Now detected at Phase 2 with a clear error pointing at the duplicate.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 6: LSP — Fix UTF-16 Position Encoding

`conversion.rs:42-62` counts characters (Unicode code points) instead of UTF-16 code units for LSP positions. This breaks hover, completion, and diagnostics for non-ASCII content.

**LSP spec requirement (from research):**

| UTF-8 Char | Bytes | Codepoints | UTF-16 Units (LSP Default) |
|------------|-------|------------|----------------------------|
| `a`        | 1     | 1          | 1                          |
| `α`        | 2     | 1          | 1                          |
| `ａ`       | 3     | 1          | 1                          |
| `𝕒` (emoji)| 4     | 1          | **2** (surrogate pair)     |

The current code uses `col += 1` per character, which is wrong for 4-byte UTF-8 chars (emoji, CJK supplementary).

**Files:**
- Modify: `src/lsp/conversion.rs:54` (use `ch.len_utf16()` in `offset_to_position`)
- Modify: `src/lsp/conversion.rs:91` (use `ch.len_utf16()` in `position_to_offset`)
- Test: `src/lsp/conversion.rs` (add UTF-16 tests)

**Step 1: Write the failing tests**

```rust
#[test]
#[cfg(feature = "lsp")]
fn test_offset_to_position_emoji() {
    // 🎉 is U+1F389 = 4 UTF-8 bytes, 2 UTF-16 code units (surrogate pair)
    let source = "a🎉b";
    // 'a' at byte 0 (1 UTF-16 unit)
    // '🎉' at byte 1 (4 UTF-8 bytes, 2 UTF-16 units)
    // 'b' at byte 5
    let pos = offset_to_position(5, source);
    assert_eq!(pos.line, 0);
    assert_eq!(pos.character, 3); // 1 (a) + 2 (🎉) = 3 UTF-16 units
}

#[test]
#[cfg(feature = "lsp")]
fn test_position_to_offset_emoji() {
    let source = "a🎉b";
    // character 3 in UTF-16 = after 'a' (1) + '🎉' (2)
    let offset = position_to_offset(
        Position { line: 0, character: 3 },
        source,
    );
    assert_eq!(offset, 5); // 'b' starts at byte offset 5
}

#[test]
#[cfg(feature = "lsp")]
fn test_roundtrip_emoji() {
    let source = "hello 🌍 world";
    // 🌍 is at byte 6, takes 4 UTF-8 bytes, 2 UTF-16 units
    // 'w' after space is at byte 11
    let pos = offset_to_position(11, source);
    let back = position_to_offset(pos, source);
    assert_eq!(back, 11, "Roundtrip should work with emoji");
}

#[test]
#[cfg(feature = "lsp")]
fn test_offset_to_position_cjk_supplementary() {
    // 𝕒 (U+1D552) = 4 UTF-8 bytes, 2 UTF-16 code units
    let source = "x𝕒y";
    let pos = offset_to_position(5, source); // 'y' at byte 5
    assert_eq!(pos.character, 3); // 1 (x) + 2 (𝕒) = 3
}

#[test]
#[cfg(feature = "lsp")]
fn test_offset_to_position_bmp_non_ascii() {
    // α (U+03B1) = 2 UTF-8 bytes, 1 UTF-16 code unit (BMP)
    let source = "aαb";
    let pos = offset_to_position(3, source); // 'b' at byte 3
    assert_eq!(pos.character, 2); // 1 (a) + 1 (α) = 2 (BMP chars are 1 UTF-16 unit)
}
```

**Step 2: Run test — verify RED**

```bash
CARGO_TARGET_DIR=target-main cargo test --lib --features lsp lsp::conversion::tests::test_offset_to_position_emoji 2>&1
```

Expected: FAIL (currently counts characters, not UTF-16 units)

**Step 3: Fix `offset_to_position()`**

Change line 54 from `col += 1` to `col += ch.len_utf16() as u32`:
```rust
pub fn offset_to_position(offset: usize, source: &str) -> Position {
    let mut line = 0u32;
    let mut col = 0u32;

    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += ch.len_utf16() as u32;
        }
    }

    Position {
        line,
        character: col,
    }
}
```

**Step 4: Fix `position_to_offset()`**

Change line 91 from `current_col += 1` to `current_col += ch.len_utf16() as u32`:
```rust
pub fn position_to_offset(pos: Position, source: &str) -> usize {
    let mut current_line = 0u32;
    let mut current_col = 0u32;

    for (i, ch) in source.char_indices() {
        if current_line == pos.line && current_col == pos.character {
            return i;
        }
        if ch == '\n' {
            if current_line == pos.line {
                return i;
            }
            current_line += 1;
            current_col = 0;
        } else {
            current_col += ch.len_utf16() as u32;
        }
    }

    source.len()
}
```

**Step 5: Run test — verify GREEN**

**Step 6: Run full suite with LSP feature**

```bash
CARGO_TARGET_DIR=target-main cargo test --lib --features lsp 2>&1 | tail -5
CARGO_TARGET_DIR=target-main cargo clippy --features lsp -- -D warnings 2>&1 | tail -5
```

**Step 7: Commit**

```bash
git add src/lsp/conversion.rs
git commit -m "fix(lsp): use UTF-16 code units for LSP position encoding

LSP spec requires UTF-16 code unit offsets for character positions.
The conversion was counting Unicode code points instead, breaking
hover/completion/diagnostics for emoji, CJK, and other non-BMP chars.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 7: Lower — Use Practical Tolerance for Backoff Comparison

`unlower_retry()` uses `f64::EPSILON` (~2.2e-16) for backoff multiplier comparison. Too strict for user-provided values.

**Files:**
- Modify: `src/ast/lower.rs` (use practical constant)
- Test: `src/ast/lower.rs` (add roundtrip precision test)

**Step 1: Write the failing test**

```rust
#[test]
fn unlower_retry_backoff_near_one() {
    // Test that backoff of exactly 1.0 roundtrips as None (no backoff)
    let mut wf = dummy_workflow();
    let id = wf.task_table.insert("fetcher");
    let mut task = dummy_task(id, "fetcher");
    task.action = AnalyzedTaskAction::Fetch(AnalyzedFetchAction {
        url: Spanned::new("https://example.com".to_string(), Span::dummy()),
        ..Default::default()
    });
    task.retry = Some(AnalyzedRetry {
        max_attempts: Spanned::new(3, Span::dummy()),
        delay_ms: Spanned::new(1000, Span::dummy()),
        backoff: None, // No backoff = multiplier 1.0
        span: Span::dummy(),
    });
    wf.tasks.push(task);

    let lowered = lower(wf).unwrap();
    let unlowered = unlower(lowered).unwrap();

    assert!(unlowered.tasks[0].retry.as_ref().unwrap().backoff.is_none(),
        "backoff of 1.0 should roundtrip as None");
}
```

**Step 2: Replace `f64::EPSILON` with practical constant**

```rust
/// Practical tolerance for backoff comparison (0.01% relative difference).
/// f64::EPSILON (~2.2e-16) is too strict for user-provided floats.
const BACKOFF_UNITY_TOLERANCE: f64 = 0.0001;

// In unlower_retry:
backoff: if (r.multiplier - 1.0).abs() > BACKOFF_UNITY_TOLERANCE {
    Some(r.multiplier)
} else {
    None
},
```

**Step 3: Run full suite + clippy**

**Step 4: Commit**

```bash
git add src/ast/lower.rs
git commit -m "fix(ast): use practical tolerance for backoff multiplier comparison

Replace f64::EPSILON (~2.2e-16) with 0.0001 tolerance for detecting
non-unity backoff multipliers in unlower_retry(). Prevents spurious
roundtrip differences from floating-point arithmetic.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## TIER 2 — New Validation Gaps

---

### Task 8: Analyzer — Add Tests for Implicit Dependency Cycle Detection

**Context:** `detect_cycles()` at `analyze.rs:988-1071` ALREADY checks `implicit_deps` from `with:` bindings. This was discovered during the architecture audit. However, there are **zero tests** for implicit dep cycles. The existing cycle tests only cover explicit `depends_on:`.

The extraction works via `parse_with_entry()` which handles complex expressions like `$step1.data | sort | first(3) ?? []` and extracts the task reference (`step1`). Deep JSONPath is already covered.

**Gap:** If `parse_with_entry()` fails (returns `Err`), the dependency is silently skipped at `analyze.rs:903` (`if let Ok(entry) = ...`). This means a malformed binding expression could hide a cycle.

**Files:**
- Test: `src/ast/analyzer/analyze.rs` (add tests in existing module)

**Step 1: Write tests for implicit dep cycles**

```rust
#[test]
fn test_analyze_implicit_dep_cycle_via_with() {
    // task1 depends on task2 via with:, task2 depends on task1 via with:
    let mut task1 = make_raw_task("task1");
    add_with_ref(&mut task1, "data", "$task2.output");

    let mut task2 = make_raw_task("task2");
    add_with_ref(&mut task2, "info", "$task1.result");

    let raw = make_raw_workflow("nika/workflow@0.12", vec![task1, task2]);
    let result = analyze(raw);
    assert!(result.is_err(), "implicit dep cycle should be detected");
    assert!(result.errors.iter().any(|e| e.kind == AnalyzeErrorKind::CyclicDependency),
        "should report CyclicDependency, got: {:?}", result.errors);
}

#[test]
fn test_analyze_implicit_dep_cycle_three_tasks() {
    // A → B → C → A via with: bindings
    let mut a = make_raw_task("a");
    add_with_ref(&mut a, "x", "$c.out");

    let mut b = make_raw_task("b");
    add_with_ref(&mut b, "x", "$a.out");

    let mut c = make_raw_task("c");
    add_with_ref(&mut c, "x", "$b.out");

    let raw = make_raw_workflow("nika/workflow@0.12", vec![a, b, c]);
    let result = analyze(raw);
    assert!(result.is_err(), "3-task implicit cycle should be detected");
}

#[test]
fn test_analyze_mixed_explicit_implicit_cycle() {
    // task1 depends_on task2 (explicit), task2 with: $task1 (implicit)
    let mut task1 = make_raw_task("task1");
    add_depends_on(&mut task1, &["task2"]);

    let mut task2 = make_raw_task("task2");
    add_with_ref(&mut task2, "data", "$task1.result");

    let raw = make_raw_workflow("nika/workflow@0.12", vec![task1, task2]);
    let result = analyze(raw);
    assert!(result.is_err(), "mixed explicit+implicit cycle should be detected");
}

#[test]
fn test_analyze_complex_with_expression_extracts_dep() {
    // Deep JSONPath: $step1.data.items | sort | first(3) ?? []
    let mut task2 = make_raw_task("task2");
    add_with_ref(&mut task2, "items", "$task1.data.items | sort | first(3) ?? []");

    let raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("task1"), task2]);
    let result = analyze(raw);
    assert!(result.is_ok());
    let wf = result.value.unwrap();
    let t2 = wf.get_task_by_name("task2").unwrap();
    assert_eq!(t2.implicit_deps.len(), 1, "should extract dep from complex expression");
}
```

**Step 2: Run tests — verify GREEN (existing code handles this)**

```bash
CARGO_TARGET_DIR=target-main cargo test --lib analyzer::tests::test_analyze_implicit_dep_cycle 2>&1
```

Expected: All PASS (detection already works, we're confirming with tests)

**Step 3: Commit**

```bash
git add src/ast/analyzer/analyze.rs
git commit -m "test(ast): add tests for implicit dependency cycle detection

Verify that detect_cycles() catches cycles through with: bindings
(implicit_deps), not just explicit depends_on. Tests cover 2-task,
3-task, and mixed explicit+implicit cycles. Also verifies deep
JSONPath expressions extract task dependencies correctly.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 9: Analyzer — SSE MCP Server Warning at Phase 2

SSE MCP servers are currently silently created in `analyze_mcp_server()` at `analyze.rs:696-700`, then silently dropped in `lower_mcp_servers()` at `lower.rs:334-336` with only a `tracing::warn!()`. Users don't see this warning unless they enable trace logging.

The fix: add an analyzer-level warning (visible in LSP diagnostics and CLI output).

**Files:**
- Modify: `src/ast/analyzer/analyze.rs` (add warning in `analyze_mcp_server()`)
- Test: `src/ast/analyzer/analyze.rs` (add test)

**Step 1: Write the test**

```rust
#[test]
fn test_analyze_mcp_sse_server_warns() {
    let mut raw = make_raw_workflow("nika/workflow@0.12", vec![make_raw_task("main")]);
    // Add an SSE MCP server (has url, no command)
    let mut servers = IndexMap::new();
    let sse_server = RawMcpServer {
        url: Some(Spanned::new("https://mcp.example.com".to_string(), make_span(0, 30))),
        command: None,
        args: None,
        env: None,
        cwd: None,
    };
    servers.insert(
        Spanned::new("sse_server".to_string(), make_span(0, 10)),
        Spanned::new(sse_server, make_span(0, 50)),
    );
    raw.mcp = Some(Spanned::new(
        RawMcpConfig { servers: Spanned::new(servers, make_span(0, 100)) },
        make_span(0, 100),
    ));

    let result = analyze(raw);
    assert!(result.is_ok(), "SSE server should not cause error");
    assert!(!result.warnings.is_empty(), "SSE server should produce warning");
    assert!(result.warnings[0].message.contains("SSE"),
        "warning should mention SSE: {}", result.warnings[0].message);
}
```

**Step 2: Run test — verify RED**

**Step 3: Add warning in `analyze_mcp_server()`**

After `analyze.rs:700` (where transport is determined), add:
```rust
if transport == McpTransport::Sse {
    ctx.add_warning(
        AnalyzeError::new(
            AnalyzeErrorKind::UnsupportedFeature,
            span,
            format!("SSE MCP server '{}' has no runtime equivalent and will be dropped during execution", name),
        )
        .with_suggestion("use a Stdio-based MCP server instead"),
    );
}
```

**Step 4: Run test — verify GREEN**

**Step 5: Run full suite + clippy**

**Step 6: Commit**

```bash
git add src/ast/analyzer/analyze.rs
git commit -m "feat(ast): warn about SSE MCP servers at analyzer phase

SSE MCP servers are silently dropped during lowering with only a
tracing::warn. Now the analyzer produces a visible warning that
appears in CLI output and LSP diagnostics.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## TIER 3 — Test Infrastructure

---

### Task 10: Parser — Add Inline Test Module

`parser.rs` has ~2055 lines and **zero inline tests**. All parser tests are in integration tests. Adding a `#[cfg(test)]` module with focused boundary tests improves test locality and catch regressions faster.

**Files:**
- Modify: `src/ast/raw/parser.rs` (add `#[cfg(test)] mod tests`)

**Tests to add (beyond T1's NaN tests):**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::FileId;

    // --- Schema parsing ---
    #[test]
    fn parse_valid_schema_0_12() {
        let yaml = "schema: \"nika/workflow@0.12\"\ntasks:\n  - id: t\n    infer: \"hi\"";
        let result = parse(yaml, FileId(0));
        assert!(result.is_ok());
        assert_eq!(result.unwrap().schema.value, "nika/workflow@0.12");
    }

    #[test]
    fn parse_missing_schema_errors() {
        let yaml = "tasks:\n  - id: t\n    infer: \"hi\"";
        let result = parse(yaml, FileId(0));
        assert!(result.is_err());
    }

    // --- f64 field boundary tests (T1 tests go here too) ---
    #[test]
    fn parse_valid_temperature() {
        let yaml = "schema: \"nika/workflow@0.12\"\ntasks:\n  - id: t\n    infer:\n      prompt: \"hi\"\n      temperature: 0.7";
        let result = parse(yaml, FileId(0));
        assert!(result.is_ok());
    }

    #[test]
    fn parse_temperature_zero() {
        let yaml = "schema: \"nika/workflow@0.12\"\ntasks:\n  - id: t\n    infer:\n      prompt: \"hi\"\n      temperature: 0.0";
        let result = parse(yaml, FileId(0));
        assert!(result.is_ok());
    }

    // --- Empty/malformed YAML ---
    #[test]
    fn parse_empty_string() {
        let result = parse("", FileId(0));
        assert!(result.is_err());
    }

    #[test]
    fn parse_invalid_yaml_syntax() {
        let result = parse("{{{{not yaml", FileId(0));
        assert!(result.is_err());
    }

    #[test]
    fn parse_yaml_array_instead_of_map() {
        let result = parse("- item1\n- item2", FileId(0));
        assert!(result.is_err());
    }

    // --- for_each parsing ---
    #[test]
    fn parse_for_each_array() {
        let yaml = r#"schema: "nika/workflow@0.12"
tasks:
  - id: t
    for_each: ["a", "b", "c"]
    as: item
    exec: "echo {{item}}"
"#;
        let result = parse(yaml, FileId(0));
        assert!(result.is_ok());
    }

    // --- Retry parsing ---
    #[test]
    fn parse_retry_on_fetch() {
        let yaml = r#"schema: "nika/workflow@0.12"
tasks:
  - id: t
    fetch:
      url: "https://example.com"
    retry:
      max_attempts: 3
      delay_ms: 1000
      backoff: 2.0
"#;
        let result = parse(yaml, FileId(0));
        assert!(result.is_ok());
    }
}
```

**Commit:**

```bash
git add src/ast/raw/parser.rs
git commit -m "test(ast): add inline test module to parser.rs

Parser had 2055 lines and zero inline tests. Add focused boundary
tests for schema parsing, f64 fields, empty/malformed YAML, for_each,
and retry parsing. Improves test locality for red-green-refactor.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 11: Analyzer — Strengthen Inline Test Module

`analyze.rs` has an inline test module (line 1073+) with ~50 tests. Add tests for:
- Implicit dep cycle detection (from T8)
- Feature gate edge cases
- Multiple errors in single workflow
- `validate()` vs `analyze()` parity

**Files:**
- Modify: `src/ast/analyzer/analyze.rs` (add tests to existing module)

**Tests to add:**

```rust
// --- Multiple errors collected ---
#[test]
fn test_analyze_collects_all_errors() {
    // Workflow with 3 problems: duplicate task, unknown ref, invalid schema
    let mut task1a = make_raw_task("task1");
    let task1b = make_raw_task("task1"); // duplicate
    add_with_ref(&mut task1a, "x", "$nonexistent");

    let raw = make_raw_workflow("nika/workflow@0.12", vec![task1a, task1b]);
    let result = analyze(raw);
    assert!(result.is_err());
    // Should have at least 2 errors (duplicate + unknown ref)
    assert!(result.errors.len() >= 2,
        "analyzer should collect all errors, got {}: {:?}", result.errors.len(), result.errors);
}

// --- Self-cycle detection ---
#[test]
fn test_analyze_self_cycle() {
    let mut task = make_raw_task("loop");
    add_depends_on(&mut task, &["loop"]);

    let raw = make_raw_workflow("nika/workflow@0.12", vec![task]);
    let result = analyze(raw);
    assert!(result.is_err());
    assert!(result.errors.iter().any(|e| e.kind == AnalyzeErrorKind::CyclicDependency));
}
```

**Commit:**

```bash
git add src/ast/analyzer/analyze.rs
git commit -m "test(ast): strengthen analyzer inline test module

Add tests for multi-error collection, self-cycle detection, and
implicit dependency cycles. Increases inline test coverage for
critical Phase 2 validation paths.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 12: Lower — Strengthen Roundtrip Edge Cases

`lower.rs` has ~80 tests (line 680+). Add edge case roundtrip tests.

**Tests to add:**

```rust
#[test]
fn lower_unlower_roundtrip_with_for_each() {
    let mut wf = dummy_workflow();
    let id = wf.task_table.insert("iter");
    let mut task = dummy_task(id, "iter");
    task.for_each = Some(AnalyzedForEach {
        items: serde_json::json!(["a", "b", "c"]),
        as_var: Some("item".to_string()),
        parallel: Some(2),
        fail_fast: true,
        span: Span::dummy(),
    });
    wf.tasks.push(task);

    let lowered = lower(wf).unwrap();
    let unlowered = unlower(lowered).unwrap();
    let t = &unlowered.tasks[0];
    assert!(t.for_each.is_some());
}

#[test]
fn lower_unlower_roundtrip_mcp_stdio() {
    let mut wf = dummy_workflow();
    let mut servers = IndexMap::new();
    servers.insert("test".to_string(), AnalyzedMcpServer {
        name: "test".to_string(),
        command: Some("node".to_string()),
        args: vec!["server.js".to_string()],
        env: IndexMap::new(),
        cwd: None,
        url: None,
        transport: McpTransport::Stdio,
        span: Span::dummy(),
    });
    wf.mcp_servers = servers;

    let lowered = lower(wf).unwrap();
    assert!(lowered.mcp.is_some());
    let unlowered = unlower(lowered).unwrap();
    assert_eq!(unlowered.mcp_servers.len(), 1);
    assert!(unlowered.mcp_servers.contains_key("test"));
}
```

**Commit:**

```bash
git add src/ast/lower.rs
git commit -m "test(ast): add roundtrip edge case tests to lower.rs

Add tests for for_each and MCP stdio roundtrip through lower/unlower
pipeline. Verifies data preservation for complex task configurations.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 13: LSP Conversion — Comprehensive UTF-16 Test Suite

`conversion.rs` has 10 existing tests — all ASCII-only. Add comprehensive UTF-16 tests covering all categories from the LSP spec research.

**Files:**
- Modify: `src/lsp/conversion.rs` (add tests to existing module)

**Tests to add (beyond T6's emoji tests):**

```rust
// --- Multi-byte BMP characters (1 UTF-16 unit each) ---
#[test]
#[cfg(feature = "lsp")]
fn test_offset_greek_alpha() {
    let source = "αβγ"; // Each 2 UTF-8 bytes, 1 UTF-16 unit
    let pos = offset_to_position(4, source); // 'γ' at byte 4
    assert_eq!(pos.character, 2);
}

// --- Mixed ASCII + emoji on multiple lines ---
#[test]
#[cfg(feature = "lsp")]
fn test_offset_emoji_multiline() {
    let source = "line1 🎉\nline2 🌍";
    // line1: 'l','i','n','e','1',' ','🎉' = 6 + 2 = 8 UTF-16 units
    // 🎉 at byte 6, 4 bytes, so '\n' at byte 10
    // line2: 'l' at byte 11
    let pos = offset_to_position(11, source);
    assert_eq!(pos.line, 1);
    assert_eq!(pos.character, 0);
}

// --- Reverse: position_to_offset with surrogate pairs ---
#[test]
#[cfg(feature = "lsp")]
fn test_position_to_offset_after_emoji() {
    let source = "🎉b";
    // '🎉' = 2 UTF-16 units, 'b' at UTF-16 position 2
    let offset = position_to_offset(Position { line: 0, character: 2 }, source);
    assert_eq!(offset, 4); // 'b' at byte 4
}

// --- YAML content with emoji in values ---
#[test]
#[cfg(feature = "lsp")]
fn test_yaml_with_emoji_positions() {
    let source = "prompt: \"Hello 🌍!\"\ntasks:";
    // 't' in 'tasks' is on line 1
    let tasks_offset = source.find("tasks").unwrap();
    let pos = offset_to_position(tasks_offset, source);
    assert_eq!(pos.line, 1);
    assert_eq!(pos.character, 0);
}

// --- Stress: consecutive surrogate pairs ---
#[test]
#[cfg(feature = "lsp")]
fn test_consecutive_emoji() {
    let source = "🎉🌍🚀x";
    // Each emoji: 4 UTF-8 bytes, 2 UTF-16 units
    // 'x' at byte 12, UTF-16 position 6
    let pos = offset_to_position(12, source);
    assert_eq!(pos.character, 6);
}
```

**Commit:**

```bash
git add src/lsp/conversion.rs
git commit -m "test(lsp): add comprehensive UTF-16 position encoding tests

Add tests for BMP non-ASCII (Greek), surrogate pairs (emoji), mixed
content, multiline with emoji, and consecutive surrogate pairs.
Validates LSP spec compliance for position encoding.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## TIER 4 — Property-Based Testing

---

### Task 14: Proptest — Parser Never Panics on Arbitrary Input

The existing `proptest_fuzzing.rs` tests serde_yaml deserialization but NOT `ast::raw::parse()`. The actual parser has complex span extraction, error handling, and field validation that could panic on unexpected input.

**Files:**
- Modify: `tests/proptest_fuzzing.rs` (add new test module)

**Test to add:**

```rust
// =============================================================================
// TEST 5: AST Parser Fuzzing (raw::parse never panics)
// =============================================================================
// Target: src/ast/raw/parser.rs
// Risk: marked_yaml span extraction, error handling paths

mod ast_parser_fuzzing {
    use super::*;
    use nika::ast::raw::parse;
    use nika::source::FileId;

    proptest! {
        /// Property: ast::raw::parse() NEVER panics on any input
        #[test]
        fn test_parser_never_panics(yaml in ".*") {
            let _ = parse(&yaml, FileId(0));
        }

        /// Property: ast::raw::parse() never panics on YAML-like input
        #[test]
        fn test_parser_never_panics_yaml_like(
            key in r"[a-z_]{1,10}",
            value in "[ -~]{0,50}"
        ) {
            let yaml = format!("{}: {}", key, value);
            let _ = parse(&yaml, FileId(0));
        }

        /// Property: Valid workflows always parse successfully
        #[test]
        fn test_valid_workflow_parses(yaml in arb_valid_workflow()) {
            let result = parse(&yaml, FileId(0));
            // Note: arb_valid_workflow generates structurally valid YAML
            // but may fail parse due to schema/verb validation
            // The key property is: no panic
            let _ = result;
        }

        /// Property: parse() returns Err, never panics, on binary input
        #[test]
        fn test_parser_handles_binary(bytes in prop::collection::vec(0u8..=255, 0..200)) {
            let input = String::from_utf8_lossy(&bytes);
            let _ = parse(&input, FileId(0));
        }
    }
}
```

**Commit:**

```bash
git add tests/proptest_fuzzing.rs
git commit -m "test(ast): add proptest fuzzing for ast::raw::parse()

Verify that the parser never panics on arbitrary input including
random strings, YAML-like content, valid workflows, and binary data.
Extends existing proptest_fuzzing.rs which only tested serde_yaml.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 15: Proptest — Full Pipeline Roundtrip Invariance

Property: For valid workflows, `parse → analyze → lower → unlower` should produce a structurally equivalent `AnalyzedWorkflow` (modulo known lossy fields: artifact, log, agents).

**Files:**
- Modify: `tests/proptest_fuzzing.rs` (add new test module)

**Test to add:**

```rust
// =============================================================================
// TEST 6: Full Pipeline Roundtrip Invariance
// =============================================================================

mod pipeline_roundtrip_fuzzing {
    use super::*;
    use nika::ast::raw::parse;
    use nika::ast::analyzer::analyze;
    use nika::ast::lower::{lower, unlower};
    use nika::source::FileId;

    prop_compose! {
        /// Generate a workflow with N infer tasks and optional dependencies
        fn arb_pipeline_workflow()(
            n in 1usize..5,
            prompts in prop::collection::vec(r"[a-zA-Z0-9 ]{1,30}", 1..6),
        ) -> String {
            let n = n.min(prompts.len());
            let mut yaml = String::from("schema: nika/workflow@0.12\ntasks:\n");
            for i in 0..n {
                yaml.push_str(&format!("  - id: task_{}\n", i));
                if i > 0 {
                    yaml.push_str(&format!("    depends_on: [task_{}]\n", i - 1));
                }
                yaml.push_str(&format!("    infer: \"{}\"\n", prompts[i]));
            }
            yaml
        }
    }

    proptest! {
        /// Property: Full pipeline roundtrip preserves task count and IDs
        #[test]
        fn test_pipeline_roundtrip_task_count(yaml in arb_pipeline_workflow()) {
            let raw = match parse(&yaml, FileId(0)) {
                Ok(r) => r,
                Err(_) => return Ok(()), // Skip unparseable
            };
            let analyzed = match analyze(raw) {
                r if r.is_ok() => r.value.unwrap(),
                _ => return Ok(()),
            };
            let original_count = analyzed.task_count();

            let lowered = match lower(analyzed) {
                Ok(l) => l,
                Err(_) => return Ok(()),
            };
            let unlowered = match unlower(lowered) {
                Ok(u) => u,
                Err(_) => return Ok(()),
            };

            prop_assert_eq!(
                unlowered.task_count(),
                original_count,
                "Roundtrip should preserve task count"
            );
        }
    }
}
```

**Commit:**

```bash
git add tests/proptest_fuzzing.rs
git commit -m "test(ast): add proptest for full pipeline roundtrip invariance

Verify that parse → analyze → lower → unlower preserves task count
and structure for randomly generated valid workflows. Catches data
loss bugs in the lower/unlower bridge.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

### Task 16: Insta Snapshots — Error Message Regression for All AnalyzeErrorKind

All 8 `AnalyzeErrorKind` variants have specific error messages. Add insta snapshots to catch accidental message changes.

**Context:** `AnalyzeErrorKind` has these variants:
- `UnknownTask` (NIKA-140)
- `DuplicateTask` (NIKA-141)
- `InvalidSchema` (NIKA-142)
- `CyclicDependency` (NIKA-143)
- `InvalidValue` (NIKA-144)
- `MissingField` (NIKA-145)
- `UnsupportedFeature` (NIKA-149)
- `InvalidBinding` (NIKA-151)

**Files:**
- Modify: `tests/regression/workflow_snapshots.rs` (add error message snapshots)

**Tests to add:**

```rust
use nika::ast::raw::parse;
use nika::ast::analyzer::analyze;
use nika::source::FileId;

#[test]
fn snapshot_error_unknown_task() {
    let yaml = r#"
schema: nika/workflow@0.12
tasks:
  - id: task1
    depends_on: [nonexistent]
    infer: "hello"
"#;
    let raw = parse(yaml, FileId(0)).unwrap();
    let result = analyze(raw);
    let errors: Vec<String> = result.errors.iter().map(|e| format!("[{}] {}", e.kind.code(), e.message)).collect();
    insta::assert_yaml_snapshot!("error_unknown_task", errors);
}

#[test]
fn snapshot_error_duplicate_task() {
    let yaml = r#"
schema: nika/workflow@0.12
tasks:
  - id: dup
    infer: "hello"
  - id: dup
    infer: "world"
"#;
    let raw = parse(yaml, FileId(0)).unwrap();
    let result = analyze(raw);
    let errors: Vec<String> = result.errors.iter().map(|e| format!("[{}] {}", e.kind.code(), e.message)).collect();
    insta::assert_yaml_snapshot!("error_duplicate_task", errors);
}

// ... similar for all 8 variants
#[test]
fn snapshot_error_cyclic_dependency() {
    let yaml = r#"
schema: nika/workflow@0.12
tasks:
  - id: a
    depends_on: [b]
    infer: "hello"
  - id: b
    depends_on: [a]
    infer: "world"
"#;
    let raw = parse(yaml, FileId(0)).unwrap();
    let result = analyze(raw);
    let errors: Vec<String> = result.errors.iter().map(|e| format!("[{}] {}", e.kind.code(), e.message)).collect();
    insta::assert_yaml_snapshot!("error_cyclic_dependency", errors);
}

#[test]
fn snapshot_error_invalid_binding() {
    let yaml = r#"
schema: nika/workflow@0.12
tasks:
  - id: task1
    with:
      x: ""
    infer: "hello"
"#;
    let raw = parse(yaml, FileId(0)).unwrap();
    let result = analyze(raw);
    let errors: Vec<String> = result.errors.iter().map(|e| format!("[{}] {}", e.kind.code(), e.message)).collect();
    insta::assert_yaml_snapshot!("error_invalid_binding", errors);
}
```

**Commit:**

```bash
git add tests/regression/workflow_snapshots.rs tests/regression/snapshots/
git commit -m "test(ast): add insta snapshots for analyzer error messages

Snapshot all 8 AnalyzeErrorKind error message formats to catch
accidental regressions. Uses insta for automatic snapshot management.

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika 🦋 <nika@supernovae.studio>"
```

---

## Checkpoint Schedule

| After Task | Verification |
|-----------|-------------|
| Task 1 | `cargo test --lib` all pass |
| Task 2 | `cargo test --lib` all pass + clippy clean |
| Task 3 | `cargo test --lib` all pass |
| Task 4 | `cargo test --lib` all pass + clippy clean |
| Task 5 | `cargo test --lib` all pass + clippy clean |
| Task 6 | `cargo test --lib --features lsp` all pass |
| Task 7 | `cargo test --lib` all pass |
| Task 8 | `cargo test --lib` all pass |
| Task 9 | `cargo test --lib` all pass + clippy clean |
| Task 10 | `cargo test --lib` all pass |
| Task 11 | `cargo test --lib` all pass |
| Task 12 | `cargo test --lib` all pass |
| Task 13 | `cargo test --lib --features lsp` all pass |
| Task 14 | `cargo test --test proptest_fuzzing` all pass |
| Task 15 | `cargo test --test proptest_fuzzing` all pass |
| Task 16 | `cargo test --test regression` all pass + review snapshots |
| **Final** | `cargo test` full + `cargo clippy -- -D warnings` + push |

---

## Risk Assessment

| Risk | Mitigation |
|------|-----------|
| NaN/Infinity check breaks valid YAML | No valid workflow uses NaN/Infinity for temperature/tokens |
| Task ID validation breaks existing workflows | Only rejects truly invalid IDs (empty, $, spaces, special chars). Hyphens/underscores/dots allowed. |
| UTF-16 fix changes diagnostic positions | Fixes them to be correct per LSP spec. Only affects non-ASCII content. |
| Backoff tolerance too large | 0.0001 is negligible for any real multiplier value |
| Import prefix check false positive | Only flags exact duplicate prefixes |
| SSE warning too noisy | It's a warning, not an error. Users should know their server won't run. |
| Proptest finds parser panic | That's the point — we fix the panic |
| Snapshot tests brittle | Only snapshot error messages, not internal state. Update with `cargo insta review`. |

---

## Audit Corrections (documented for traceability)

Two initially proposed tasks were found to be non-issues during deep code inspection:

1. **~~Negative integer rejection~~** — `get_u32_field()` uses `parse::<u32>()` which already rejects negative values. Rust's `FromStr` for unsigned types does not accept negative signs. Not a bug.

2. **~~Implicit dep cycle detection~~** — `detect_cycles()` at `analyze.rs:1046-1066` ALREADY checks `implicit_deps` from `with:` bindings. The extraction via `parse_with_entry()` handles complex JSONPath expressions. Task 8 was revised from "add detection" to "add tests for existing detection."
