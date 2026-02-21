# Priority 1 Fixes - Execution Plan

**Date:** 2026-02-21
**Target:** Fix critical issues before v0.7.0 release
**Estimated Time:** 4-6 hours

## Tasks

### Task 1: Fix Version Mismatch (15 min)
**Files:** `Cargo.toml`, `CLAUDE.md`
**Action:** Update Cargo.toml to v0.7.0 (keeping CLAUDE.md as source of truth)
**Verification:** `cargo build` succeeds

### Task 2: Fix Test Count in CLAUDE.md (5 min)
**Files:** `CLAUDE.md`
**Action:** Update test count from 1842 to actual count (~1635)
**Verification:** Run `cargo test --lib 2>&1 | grep "passed"` to get exact count

### Task 3: Add Provider Method Tests (90 min)
**Files:** `tests/rig_provider_methods_test.rs` (NEW)
**Tests to add:**
- `test_run_mistral_creates_correct_result()`
- `test_run_groq_creates_correct_result()`
- `test_run_deepseek_creates_correct_result()`
- `test_run_ollama_creates_correct_result()`
- `test_run_mistral_handles_error()`
- `test_run_groq_handles_error()`
- `test_run_deepseek_handles_error()`
- `test_run_ollama_handles_error()`
**Approach:** Use mock clients, verify result structure
**Verification:** All 8 tests pass

### Task 4: Add Chat Continuation Tests (60 min)
**Files:** `tests/chat_continuation_test.rs` (NEW)
**Tests to add:**
- `test_chat_continue_updates_history()`
- `test_chat_continue_claude_multi_turn()`
- `test_chat_continue_openai_multi_turn()`
- `test_chat_continue_emits_events()`
- `test_chat_continue_respects_max_turns()`
- `test_history_persists_across_calls()`
**Approach:** Use mock providers, verify history state
**Verification:** All 6 tests pass

### Task 5: Implement View Shortcuts a/h/s/m (30 min)
**Files:** `src/tui/app.rs`
**Action:** Add key handlers for 'a', 'h', 's', 'm' in handle_unified_key()
**Current:** Only 1/2/3/4 work
**Target:** Both number keys AND letter keys work
**Verification:** Manual TUI test or unit test

### Task 6: Fix InputMode on View Switch (15 min)
**Files:** `src/tui/app.rs`
**Action:** Reset `self.input_mode = InputMode::Normal` in SwitchView/NextView/PrevView
**Verification:** Unit test that mode resets on view change

## Execution Order

```
1. Version fix (unblocks everything)
2. Test count fix (documentation accuracy)
3. Provider tests (critical coverage gap)
4. Chat tests (v0.6 feature coverage)
5. View shortcuts (DX improvement)
6. InputMode fix (bug fix)
```

## Success Criteria

- [ ] Cargo.toml version = "0.7.0"
- [ ] CLAUDE.md test count accurate
- [ ] 8 new provider tests passing
- [ ] 6 new chat continuation tests passing
- [ ] a/h/s/m shortcuts working
- [ ] InputMode resets on view switch
- [ ] All 1650+ tests passing
- [ ] `cargo clippy` clean
