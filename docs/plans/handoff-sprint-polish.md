# Handoff: Polish Sprint (~4h)

> Copy this file as first message in a new Claude Code session.

## Context
Final polish before v0.54 tag. All critical and high bugs are fixed. These are quality-of-life improvements.

## Codebase
```
cd /Users/thibaut/dev/supernovae/nika
cargo test --workspace --lib  # 9057 pass
```

## Items

### 1. TUI ProviderName migration (~2h)
**File:** `nika-tui/src/state/lifecycle.rs:66`
**Bug:** TUI uses raw strings for provider names instead of `ProviderName` enum. Inconsistent with engine.
**Fix:** Replace `String` provider fields with `ProviderName` enum throughout TUI state.

### 2. Skills path resolution (~1h)
**File:** `nika-engine/src/runtime/skill_injector.rs`
**Bug:** Skill file paths are resolved relative to CWD, not the workflow file's directory. Causes NIKA-270 when running from a different directory.
**Fix:** Resolve skill paths relative to `workflow_base_path` (available in RunContext).

### 3. Orchestrator system prompt YAML examples (~30min)
**File:** `nika-engine/src/runtime/orchestrate.rs:20`
**Bug:** Orchestrator agent doesn't include YAML syntax examples in its system prompt. LLMs generate invalid YAML for `nika:run`.
**Fix:** Add a minimal YAML template example to `build_system_prompt()`.

### 4. CHANGELOG update (~30min)
**File:** `CHANGELOG.md`
Update with all overnight session fixes:
- 4 security fixes (exec.rs, policy.rs, SECRET_RE, MCP redaction)
- 2 silent bug fixes (transforms, null coercion)
- 5 new telemetry events
- 2 edge case hardening (for_each limit, timeout=0)
- 91 E2E test workflows added

### 5. Missing E2E workflow assertions (~1h)
Several workflows test "exit 0" but not output correctness. Add `nika:assert` to:
- E01 (verify word count = 2)
- D01 (verify chain output matches expected string)
- D03 (verify array length = 5)

## Verification
```bash
cargo test --workspace --lib
cargo clippy --workspace -- -D warnings
bash tests/e2e-overnight/run-smoke.sh ./tools/target/debug/nika
```
