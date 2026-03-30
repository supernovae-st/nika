# Next Session Prompt — Copy-Paste This

> Copy everything below the line into a new Claude Code session.

---

ultrathink Execute `docs/plans/2026-03-27-tui-cleanup-v3.md` — 9 tasks, batches A through E.

**Context recovery:** Read these files first:
1. `docs/plans/2026-03-27-handoff-session.md` — Full session handoff with findings from 4 deep-scan agents
2. `docs/plans/2026-03-27-tui-cleanup-v3.md` — The 9-task plan to execute
3. `docs/plans/2026-03-27-tui-deep-fixes-v2.md` — Previous 21-task plan (completed) for context on what was already fixed

**State:** 2145 tests pass, 0 clippy warnings, all on main branch, pushed.

**Method:**
- Use `spn-powers:executing-plans` skill
- TDD: write failing test FIRST, then fix, then verify
- 1 fix = 1 commit with co-author lines
- Use `spn-powers:requesting-code-review` agent after each batch
- `cargo test -p nika-tui --lib` (never without --lib)

**Priority order:** Task 1 (bug) → Tasks 2-4 (magic numbers) → Tasks 5-6 (test infra) → Tasks 7-8 (dead code) → Task 9 (memory)

After completing v3, do a full audit pass:
1. Run `cargo test --workspace --lib` to verify nothing broke cross-crate
2. Run `cargo clippy --workspace -- -D warnings` for zero-warning check
3. `git log --oneline -15` to verify commit hygiene
4. Report final test count and any new findings
