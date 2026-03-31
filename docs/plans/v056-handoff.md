# v0.56 Handoff — Runtime Stabilization Complete

> Date: 2026-03-31
> Previous: v0.55.0 (VPS production hardening)
> Session: 7 commits, 9,093 tests, 0 failures

## What Was Done

### P0 Fixes (both resolved)
1. **Agent scope wiring** (`77db47c`) — `scope: full|minimal|debug` now controls builtin tools. `minimal` = complete + log only. `debug` = all + introspection. Explicit `tools:` always overrides.
2. **LLM guardrails** (`3ab291c`) — `type: llm` no longer returns hard error. Calls a judge LLM via RigProvider, checks response against `pass_pattern`, respects `on_failure:` action. 30s timeout.

### P1 Fixes (3/4 resolved, 1 skipped)
3. **Failed task binding warning** (`86aeecc`) — `tracing::warn` when `$failed_task` binding resolves null.
4. **Cancellation before bindings** (`5529a59`) — Cancel check before sync binding resolution in task iteration + for_each.
5. **SSRF redirect re-pinning** (`3930677`) — DNS-pinned fetch clients now have SSRF redirect policy.
6. **EventLog ring buffer** — SKIPPED: Vec with amortized drain(..half) is already O(1) amortized. VecDeque breaks contiguous `&[Event]` API.

### P2 Fixes (both resolved)
7. **StructuredOutputTimeout event** (`39403eb`) — New EventKind emitted before 600s timeout error.
8. **TUI ProviderName** (`e1649ee`) — Typed enum replaces hardcoded strings.

## Remaining Work for v0.56

### From Original Handoff (still open)
| # | Bug | Priority | Effort |
|---|-----|----------|--------|
| 1 | repair_model not validated at config time | P3 | 30m |
| 2 | TOCTOU symlink race in file tools | P3 | 2h |
| 3 | Vec::with_capacity() missing in hot paths | P3 | 1h |
| 4 | E2E assertions expansion (10+ more workflows) | P2 | 2h |
| 5 | MCP reconnection event | P2 | 1h |
| 6 | Skills path resolution (engine general case) | P2 | 1h |

### New Items Discovered
| # | Bug | Priority | Effort |
|---|-----|----------|--------|
| 7 | `check_guardrails` now async — verify all callers handle errors gracefully | P1 | 1h |
| 8 | Agent scope: RunContext for TaskStatusTool is empty (minimal, not shared with runner) | P2 | 1h |
| 9 | LLM guardrail: no event for "judge call started" | P3 | 30m |
| 10 | nika-lsp Cargo.toml warnings (default-features ignored) | P3 | 15m |

### Cortex (v0.57+ — NOT v0.56)
Research complete in `docs/research/2026-03-31-nika-cortex-*.md`. Zero conflicts with current code. Sequence: stabilize runtime first (done), then add new crate.

## Test Counts
```
nika-init:       0   nika-daemon:   165   nika-lsp-core:  230
nika-core:     156   nika-engine: 4,348   nika-mcp:       388
nika-event:    886   nika-media:    329   nika-tui:     2,153
nika-cli:      146   nika-lsp:        0   nika:             0
                                          TOTAL:        9,093
```

## Git State
- Branch: main, fully pushed
- Clippy: CLEAN (0 warnings, 2 Cargo.toml cosmetic only)
- Last commit: `e1649ee95 refactor(tui): use ProviderName enum for provider verification`
