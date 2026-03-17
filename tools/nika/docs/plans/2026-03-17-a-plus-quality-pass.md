# A+++ Quality Pass — AST/LSP/Proptest Deep Improvement

## Scope

Fix all real bugs, eliminate all false-confidence tests, add missing coverage,
add algebraic property tests. Target: zero silent bugs, zero tautological tests.

## Phase 1: Fix CRLF Bugs (BUG-1/2/3)

**File**: `src/lsp/conversion.rs`

The current `\r` handling ONLY works for `\r\n` sequences. Per LSP spec 3.17,
`\r` alone is also a valid line terminator.

### Tasks

1. Rewrite `offset_to_position` with byte-level state machine:
   - `\n` → always line break
   - `\r` followed by `\n` → single line break (skip `\n`)
   - `\r` followed by non-`\n` → standalone line break
   - Handle `\r` at EOF

2. Rewrite `position_to_offset` with same logic

3. Add tests:
   - `test_isolated_cr_line_ending` — "abc\rdef"
   - `test_double_cr_before_lf` — "abc\r\r\ndef"
   - `test_cr_at_eof` — "abc\r"
   - `test_mixed_line_endings` — "a\nb\r\nc\rd"
   - `test_roundtrip_isolated_cr`
   - `test_unicode_with_isolated_cr`

4. Verify all existing 23 tests still pass

## Phase 2: Fix thinking_budget Truncation (BUG-4)

**File**: `src/ast/lower.rs:537`

Replace `as u32` with `try_into().unwrap_or(u32::MAX)` to prevent silent wrap.

### Tasks

1. Fix the truncation: `b.try_into().unwrap_or(u32::MAX)`
2. Add test: `roundtrip_thinking_budget_large_value`
3. Add test: `roundtrip_thinking_budget_max_u32`

## Phase 3: Eliminate Silent-Skip Anti-Pattern (6 locations)

**File**: `tests/proptest_fuzzing.rs`

### Tasks

1. `assert_roundtrip` (line 652): Replace `Err(_) => return Ok(())` on parse
   with `.expect("Generator should produce valid YAML")`
2. `test_self_reference_fails` (line 408): `.expect()` on parse
3. `test_cycle_detection` (line 426): `.expect()` on parse
4. `test_nonexistent_task_fails` (line 448): `.expect()` on parse
5. `test_template_with_substitution_returns_owned` (line 88): `.expect()` on
   both regex capture and template_resolve
6. `test_invalid_schema_rejected` (line 270): Keep match but document why
   parse failure is acceptable (some invalid schemas ARE invalid YAML)

## Phase 4: Remove Tautological Tests + Wire for_each

**File**: `tests/proptest_fuzzing.rs`

### Tasks

1. Replace `json_fuzzing` module (tests serde_json, not Nika) with
   Nika-specific binding storage tests
2. Replace `test_valid_task_id_passes` with actual parser acceptance test
3. Wire `test_for_each_empty_array_fails` to parse()+analyze()
4. Wire `test_for_each_non_array_fails` to parse()+analyze()

## Phase 5: Lower Roundtrip Documentation Tests

**File**: `src/ast/lower.rs` (test module)

These are DOCUMENTATION tests that prove we know about lossy conversions.

### Tasks

1. `roundtrip_provider_none_becomes_claude`
2. `roundtrip_markdown_output_becomes_text`
3. `roundtrip_implicit_deps_merge_into_depends_on`
4. `roundtrip_context_files_are_lost`
5. `roundtrip_agent_from_field_is_lost`
6. `roundtrip_sse_server_permanently_lost`

## Phase 6: Algebraic Property Tests

**File**: `tests/proptest_fuzzing.rs`

### Tasks

1. `test_analyze_deterministic` — same YAML parsed twice gives same results
2. `test_cycle_error_symmetry` — A→B cycle same error as B→A
3. `test_roundtrip_preserves_action_types` — infer stays infer, exec stays exec

## Verification

After each phase: `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check`
