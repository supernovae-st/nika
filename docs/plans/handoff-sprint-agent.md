# Handoff: Agent Sprint (~8h)

> Copy this file as first message in a new Claude Code session.

## Context
Agent verb works end-to-end (B01, B02, B05 pass) but has feature gaps and hardcoded limits.

## Codebase
```
cd /Users/thibaut/dev/supernovae/nika
cargo test --workspace --lib  # 9057 pass
```

## Items

### 1. max_tokens(8192) hardcoded — 22 instances (~4h)
**File:** `nika-engine/src/provider/rig/mod.rs`
**Bug:** `max_tokens(8192)` is hardcoded in 22 places across all provider constructors.
**Fix:** Create `effective_max_tokens(task_max_tokens: Option<u32>, model: &str) -> u32` that uses task-level override → model default → 8192 fallback. Replace all 22 instances.

### 2. Agent scope not wired (~2h)
**File:** `nika-engine/src/runtime/rig_agent_loop/mod.rs:285`
**Bug:** `scope: full | minimal | debug` is parsed from YAML but ignored at runtime.
**Fix:** In agent system prompt construction, filter available tools based on scope:
- `full`: all tools (current behavior)
- `minimal`: only explicitly listed tools
- `debug`: all tools + introspection tools (dag_info, task_status)

### 3. LLM guardrails not implemented (~3h)
**File:** `nika-engine/src/runtime/rig_agent_loop/thinking.rs:57`
**Bug:** `type: llm` guardrails are parsed but return hard error at runtime.
**Fix:** Implement the LLM judge guardrail: send agent output to a separate LLM with `judge_prompt`, check if response matches `pass_pattern`.

### 4. 8 named agent presets not wired (~2h)
**File:** `nika-engine/src/provider/presets/`
**Bug:** 8 presets (think, lite, search, vision, judge, coder, summary, creative) exist but `from: preset_name` doesn't apply them at runtime.
**Fix:** Load preset config in agent loop initialization and merge with task-level overrides.

## Verification
```bash
cargo test --workspace --lib
./tools/target/debug/nika run tests/e2e-overnight/B01-agent-basic.nika.yaml --no-live
```
