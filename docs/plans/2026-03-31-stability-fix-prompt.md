# Stability Fix Session — Nika v0.56.1

## Mission

You are fixing bugs found by a 27-agent deep audit of the Nika workflow engine. All bugs are documented with exact file locations, failing test code, and fix code. Your job is to execute the fixes in TDD order.

## Documents to Read FIRST (in order)

1. **Implementation Plan (YOUR MAIN DOC):**
   `docs/plans/HANDOFF-v0.56.1-stability.md`
   — 16 fixes with RED/GREEN/REFACTOR for each. Follow this step by step.

2. **Full Bug Report (REFERENCE):**
   `docs/plans/2026-03-31-stability-audit-v0.56.1.md`
   — 77 findings from the audit. Context for each bug. Consult when you need more detail.

3. **Known Bugs & Patterns:**
   `~/.claude/rules/nika-bugs-and-patterns.md`
   — Existing bug catalog. Do NOT re-introduce fixed bugs.

## Methodology

### TDD — Red-Green-Refactor (MANDATORY)

For EACH fix in the handoff:

1. **RED:** Write the failing test EXACTLY as specified in the handoff doc
2. Run `cargo test --workspace --lib` — confirm the NEW test FAILS (and all old tests still pass)
3. **GREEN:** Apply the minimal fix EXACTLY as specified
4. Run `cargo test --workspace --lib` — confirm ALL tests pass (old + new)
5. **REFACTOR:** Clean up only if needed (no gold-plating)
6. Run `cargo clippy --workspace -- -D warnings` — zero warnings
7. **COMMIT:** One fix = one commit. Format: `type(scope): description`

### Commit Format

```
type(scope): description

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

Types: `fix`, `test`, `ci`
Scopes: `serve`, `runtime`, `ast`, `core`, `media`, `tools`, `engine`

### Fix Order (FOLLOW THIS EXACT ORDER)

| # | ID | File | What |
|---|-----|------|------|
| 1 | P0-002 | nika-serve/src/routes/workflows.rs | CAS loop for job queue |
| 2 | P0-003 | nika-engine/src/runtime/executor/fetch.rs | safe_backoff_delay() |
| 3 | P1-011 | nika-serve/src/config.rs | 127.0.0.1 default bind |
| 4 | P1-012 | nika-serve/src/lib.rs | TimeoutLayer 30s |
| 5 | P0-004 | nika-engine/src/ast/import_loader.rs | Duplicate task ID check |
| 6 | P0-005 | nika-engine/src/runtime/executor/invoke.rs | timeout=0 reject |
| 7 | P1-014 | nika-engine/src/tools/write.rs | create_new(true) atomic |
| 8 | P2-007 | nika-media/src/tools/safety.rs | DOCTYPE block in SVG |
| 9 | P2-001 | nika-core/src/binding/transform.rs | LastN string/object |
| 10 | P1-NEW-1 | nika-engine/src/binding/resolve.rs | Redact secret defaults |
| 11 | P2-015 | nika-serve/src/worker.rs | WorkerGuard.incremented |
| 12 | P2-002 | nika-engine/src/ast/action.rs | invoke timeout=0 |
| 13 | P2-NEW-2 | nika-core analyzer | max_attempts: 0 reject |
| 14 | P1-NEW-4 | .github/workflows/ci.yml | Add windows-latest |
| 15 | P1-NEW-5 | .github/workflows/release-plz.yml | Fix manifest path |
| 16 | P2-NEW-16 | nika-engine/src/runtime/executor/fetch.rs | llm_txt skip HTML |

## Rules

### DO
- Read `HANDOFF-v0.56.1-stability.md` BEFORE writing any code
- Write the test FIRST, see it fail, THEN fix
- Run `cargo test --workspace --lib` after EVERY change (ALWAYS --lib to avoid keychain)
- Commit after each fix (not batched)
- Stop and investigate if a test you didn't write breaks

### DO NOT
- Do NOT skip tests ("the fix is obvious" — write the test anyway)
- Do NOT batch multiple fixes in one commit
- Do NOT refactor surrounding code (fix ONLY what the handoff says)
- Do NOT add features or "improvements" beyond what is specified
- Do NOT use `cargo test` without `--lib` (macOS Keychain popup)
- Do NOT modify code not mentioned in the handoff without asking

### If You Get Stuck
- Re-read the specific fix section in `HANDOFF-v0.56.1-stability.md`
- Check `2026-03-31-stability-audit-v0.56.1.md` for the agent's full analysis
- The exact line numbers may have shifted — use the code patterns to find the right spot
- If a test in the handoff doesn't compile, adapt it to the actual types/imports

## Verification (After ALL 16 Fixes)

```bash
cd tools

# 1. All tests pass
cargo test --workspace --lib

# 2. Zero warnings
cargo clippy --workspace -- -D warnings

# 3. Format
cargo fmt --all --check

# 4. Count new tests (should be ~20+ new)
cargo test --workspace --lib 2>&1 | grep "test result" | awk '{sum += $4} END {print "TOTAL:", sum}'

# 5. Smoke test
cargo run --bin nika -- check ../examples/gates/**/*.nika.yaml 2>/dev/null; echo "exit: $?"
```

Expected: 9,130+ tests passing, 0 warnings, all formats clean.

## Context

- Working dir: `/Users/thibaut/dev/supernovae/nika/tools`
- Branch: `main` (or create `fix/v0.56.1-stability` if preferred)
- Current: 9,109 tests passing, 0 clippy warnings
- License: AGPL-3.0-or-later on all crates
- Launch: May 5, 2026 — these are pre-launch stability fixes
