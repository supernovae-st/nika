# Session Handoff — 2026-03-27 TUI Deep Fix v2 + Cleanup v3

> **For Claude:** This is a handoff document. Read it FULLY before starting any work. It contains session context, findings, plans, and exact instructions for the next session.

---

## Session Summary

**What was done:** 21-task TUI deep fix plan (v2), all completed.
- 5 CRITICAL bugs (streaming, xAI sync, hardcoded models, MCP cap, pause phase)
- 6 IMPORTANT logic bugs (MCP phase, thresholds, double-push, line-count, byte_to_line_col, navigate_up)
- 3 Plan gaps (InfoPanel scroll, browser_index clamp, retry MCP cleanup)
- 2 SEC+PERF (Zeroizing API keys, ExecBox Vec alloc removal)
- 5 Test fixes (vacuous assertion, provider count, CI false-positive, dedup edge cases, WorkflowFailed)

**Final state:** 2145 tests pass, 0 clippy warnings, pushed to origin/main.

**Plans written:**
- `docs/plans/2026-03-27-tui-deep-fixes-v2.md` — Original 21-task plan (COMPLETED)
- `docs/plans/2026-03-27-tui-cleanup-v3.md` — Follow-up 9-task cleanup plan (TODO)

---

## What Remains — v3 Plan (9 tasks)

Read `docs/plans/2026-03-27-tui-cleanup-v3.md` for full details. Summary:

| # | Task | Type | Priority |
|---|------|------|----------|
| 1 | on_mcp_invoke clobbers Pause phase | BUG | HIGH |
| 2 | Replace magic 7 with CLOUD_PROVIDER_COUNT | REFACTOR | MEDIUM |
| 3 | Extract CONTEXT_WINDOW to shared constant | REFACTOR | LOW |
| 4 | Extract notification DEFAULT_MAX_ITEMS | REFACTOR | LOW |
| 5 | Create test_helpers.rs module | INFRA | MEDIUM |
| 6 | Add temp_env for safe env var tests | INFRA | MEDIUM |
| 7 | Remove EdgeStyle::Smooth dead variant | CLEANUP | LOW |
| 8 | Narrow dead_code suppressions on chat widgets | CLEANUP | LOW |
| 9 | Document hook interference (memory only) | PROCESS | LOW |

---

## WIP Files (Not Part of Plan)

These files have uncommitted/auto-committed changes from parallel work:

1. **`tools/nika-cli/src/model.rs`** — indicatif progress bar for `nika model pull`. Auto-committed in `5d23ebf81` and `22a97db9f`. Needs review — the refactor looks complete but was not planned.

2. **`tools/nika-daemon/src/server.rs`** — cron scheduler. Auto-committed in `38badc8ba`. The `098c2d594` commit fixed a compile error it introduced (auth_token gating).

3. **`tools/nika-engine/src/provider/cost.rs`** — MODEL_META array reformatting. Bundled into `1ce62841d` accidentally.

**Action:** Review these 3 auto-commits for correctness. They were not TDD'd.

---

## Findings From 4-Agent Deep Scan

### Agent 1: Magic Numbers (48 tool calls)

| Location | Magic | Should Be |
|----------|-------|-----------|
| `modal.rs:180` | `0..7` | `CLOUD_PROVIDER_COUNT` |
| `modal.rs:247` | `.max(7)` | `CLOUD_PROVIDER_COUNT` |
| `verification_effect.rs:506` | `0..7` (test) | `CLOUD_PROVIDER_COUNT` |
| `provider_card.rs:102` | `.min(7)` | `SPARKLINE_LEVELS - 1` |
| `gauge.rs:133` | `.min(7)` | `PARTIAL_CHARS.len() - 1` |
| `mission_control.rs:695` | `.min(7)` | `MAX_ACTIVITY_ITEMS` |
| `provider.rs:79` | `100_000` | Already `const CONTEXT_WINDOW` but local scope |
| `notification_state.rs:26` | `10` | `DEFAULT_MAX_ITEMS` |
| `agent_state.rs:55` | `50` | `RECENT_TEMPLATES_MAX` |
| `cache.rs:22` | `50` | Already `max_entries` field but hardcoded in `new()` |

### Agent 2: Phase Clobbering (9 tool calls)

| Location | Phase Set | Guarded? | Risk |
|----------|----------|----------|------|
| `provider.rs:164` | Rendezvous | **NO** | **HIGH — clobbers Pause/Abort** |
| `task.rs:41` | Launch | Yes (if Countdown) | Safe |
| `task.rs:43` | Orbital | Yes (else) | Safe |
| `workflow.rs:18` | Countdown | No but intentional (start) | OK |
| `workflow.rs:33` | MissionSuccess | No but terminal | OK |
| `workflow.rs:53` | Abort | No but terminal | OK |
| `workflow.rs:85` | Abort | No but terminal | OK |
| `provider.rs:250` | Orbital | **Yes (Rendezvous guard)** | **FIXED in v2** |
| `workflow_ops.rs:120` | Preflight | No but intentional (retry reset) | OK |

**Action:** Only `provider.rs:164` needs fixing — Task 1 of v3 plan.

### Agent 3: Dead Code (39 tool calls)

| Item | Location | Severity |
|------|----------|----------|
| `EdgeStyle::Smooth` dead variant | `dag/edge.rs:75` | Low — delete |
| Solarized palette unused colors | `theme/palette.rs` | Low — keep for theming |
| `#[allow(dead_code)]` on 4 chat widget modules | `chat/widgets/mod.rs` | Low — narrow scope |
| Native model discovery TODO | `provider_modal/loader.rs:147` | Medium — implement or remove |
| Dummy const for unused import | `highlight/theme.rs:110` | Low — noise |

### Agent 4: Test Infrastructure (27 tool calls)

| Metric | Value |
|--------|-------|
| Test files | 6 |
| Largest file | `state/tests.rs` — 4536 lines |
| `TuiState::new()` repetitions | 134x |
| `Arc::from()` patterns | 89x |
| EventKind boilerplate | 80+ constructions |
| Potential lines saved | 500-1000 |
| `serial_test` available | Yes |
| `temp_env` available | **No — needs adding** |

**Action:** Task 5 of v3 plan creates `test_helpers.rs`. Task 6 adds `temp_env`.

---

## Key Files to Read for Context Recovery

```
# Plans
docs/plans/2026-03-27-tui-deep-fixes-v2.md    # v2 plan (DONE)
docs/plans/2026-03-27-tui-cleanup-v3.md        # v3 plan (TODO)
docs/plans/2026-03-27-handoff-session.md        # This file

# Core files touched in v2
tools/nika-tui/src/state/event_handler/provider.rs   # Thresholds, MCP, phase
tools/nika-tui/src/state/workflow_ops.rs              # toggle_pause, reset_for_retry
tools/nika-tui/src/state/tests.rs                     # 4536-line test file
tools/nika-tui/src/widgets/provider_modal/state/modal.rs  # Provider state
tools/nika-tui/src/widgets/provider_modal/handler.rs      # Keyboard + Zeroizing
tools/nika-tui/src/widgets/task_box/exec.rs               # ExecBox render
tools/nika-tui/src/highlight/treesitter.rs                # byte_to_line_col
tools/nika-tui/src/widgets/panels/info.rs                 # InfoPanel scroll
tools/nika-tui/src/standalone/state.rs                    # browser_index clamp
tools/nika-tui/src/state/notification_state.rs            # Dedup, max_items
tools/nika-tui/src/app/events.rs                          # StreamChunk::Done

# CLAUDE.md files (conventions)
tools/nika/CLAUDE.md                 # Crate dev reference
CLAUDE.md                            # Nika project conventions
```

---

## Development Method for Next Session

### Workflow

```
Read handoff → Read v3 plan → Execute task-by-task → Verify → Push
```

### Per-Task TDD Cycle

```
1. Mark task in_progress (TaskUpdate)
2. Write failing test FIRST
3. Run test — confirm it FAILS
4. Write minimal fix
5. Run test — confirm it PASSES
6. Run full suite: cargo test -p nika-tui --lib
7. Commit: git add <specific files> && git commit
8. Mark task completed (TaskUpdate)
```

### Skills to Use

| Skill | When |
|-------|------|
| `spn-rust:rust-core` | Ownership, error handling, type-state patterns |
| `spn-powers:test-driven-development` | Every task — test first, always |
| `spn-powers:verification-before-completion` | Before claiming any task done |
| `spn-powers:systematic-debugging` | If any test fails unexpectedly |
| `spn-powers:requesting-code-review` | After completing each batch |

### Commit Convention

```
type(scope): concise description

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

Types: `fix`, `refactor`, `test`, `perf`, `sec`
Scopes: `tui`, `provider`, `dag`, `event`

### Testing Commands

```bash
# TUI tests only (safe — no keychain)
cd tools && cargo test -p nika-tui --lib 2>&1 | tail -5

# Specific test
cd tools && cargo test -p nika-tui --lib -- test_name

# Clippy
cd tools && cargo clippy -p nika-tui --no-deps -- -D warnings

# Full workspace (WARNING: may trigger keychain on macOS without --lib)
cd tools && cargo test --workspace --lib 2>&1 | tail -5
```

### Known Gotchas

1. **Never `cargo test` without `--lib`** — triggers macOS Keychain popups
2. **EventKind fields:** `task_id` is `Arc<str>` not `String` — use `.into()` or `Arc::from()`
3. **ProviderResponded** has `request_id: Option<String>` field — don't forget it
4. **AgentComplete** has `turns: u32` and `stop_reason: String` — don't forget them
5. **Pre-commit hooks** may auto-commit or reformat — be aware of unexpected commits
6. **`CLOUD_PROVIDER_COUNT`** is in `providers.rs` but not `pub` — may need visibility change

---

## Architecture Assessment

### What's Clean
- State machine split: `state/event_handler/{workflow,provider,task,agent}.rs` — one file per domain
- Dirty flags system: granular invalidation per panel
- MCP state: VecDeque with eviction cap, seq numbers, selected_idx
- Security: Zeroizing for API keys, redacted Debug impls
- Test coverage: 2145 tests for 86k LOC (good density)

### What Could Be Better (Future)
- `state/tests.rs` at 4536 lines — split by domain like the handlers
- Test helpers module (Task 5 of v3)
- Magic numbers still scattered (Tasks 2-4 of v3)
- No integration tests (TUI + engine together)
- `loader.rs:147` — native model discovery is a TODO placeholder

### No Major Concerns
- No dead code accumulation (cleaned in v0.41.1)
- No legacy patterns (all using current AST pipeline)
- No architecture debt (event handler split is clean)
- Dependencies are current (ratatui 0.28, crossterm, tree-sitter)
