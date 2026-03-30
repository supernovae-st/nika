# v0.51 Deep Fixes — Session Handoff Note

**Date:** 2026-03-28
**Status:** NOT STARTED — session interrupted before any code changes
**Baseline:** clean working tree on `main`, HEAD = `baabd0ca7`

---

## What Was Done

- Loaded skills: rust-core, test-driven-development, executing-plans
- Created task #1 (Phase 0.1 baseline verification) — not executed
- **Zero code changes, zero commits**

## What Remains (ALL 3 BUGS)

The full handoff prompt is at:
`docs/plans/2026-03-28-v051-deep-fixes-handoff.md`

### Bug 1: Thinking Tokens Not Priced Separately
- **Files:** `cost.rs`, `thinking.rs`, `streaming.rs`
- **Fix:** Add `calculate_with_thinking()` — thinking tokens at INPUT rate, not output
- **Est:** ~150 lines

### Bug 2: Structured Output Retries Ignore Temperature/System
- **Files:** `structured_output.rs`, `executor/infer.rs`
- **Fix:** Store original system/temperature on `StructuredOutputEngine`, inject into retry prompt
- **Est:** ~110 lines

### Bug 3: Extended Thinking Agent Drops Tools
- **Files:** `thinking.rs`, `streaming.rs`, `providers.rs`
- **Fix:** Depends on rig-core AgentBuilder API research (Phase 0.2 BLOCKING)
- **Est:** 65-285 lines

## Execution Order

```
Phase 0: Research (rig-core API, Anthropic thinking fields, 3 swarm agents)
Phase 1: Bug 2 — Retry temp/system (easiest, no external deps)
Phase 2: Bug 1 — Thinking tokens cost (medium, self-contained)
Phase 3: Bug 3 — Thinking + tools (hardest, depends on Phase 0.2)
Phase 4: Verification (test, clippy, push)
```

## Critical Research Needed Before Bug 3

- Does rig-core's `AgentBuilder` support `additional_params()`?
- Check: `grep "rig-core" Cargo.lock | head -3`
- Check: rig-core source in `target/` for `additional_params` on AgentBuilder
- This determines if Bug 3 is 5 lines or 200+ lines

## Rules

- TDD strict (RED → GREEN → REFACTOR)
- `cargo test --workspace --lib` (always `--lib`)
- 1 fix = 1 commit, push after each
- Co-authors: Claude + Nika 🦋
