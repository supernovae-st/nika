# Quality Methodology — Autonomous Execution for 60 Hours

**Date**: 2026-03-28
**Status**: ACTIVE — governs all autonomous Claude Code sessions on Nika
**Scope**: 15 sessions (A-O), ~60 hours, 0 human intervention

---

## PART 0: WHY THIS DOCUMENT EXISTS

Claude Code sessions have a finite context window. After ~50k tokens of work, older
context gets compressed or evicted. The mega-prompt alone is ~3k tokens. Session plans
are ~5-10k each. After 2-3 hours of work, the model may have lost the original rules.

**This methodology must be self-contained.** A fresh Claude session should be able to:
1. Read `progress.md` to know what was done
2. Read this methodology for rules
3. Read the next session plan
4. Resume work with zero information loss

---

## PART 1: THE SYSTEMIC PROBLEM

After 30+ agents, 350+ workflows, 55 bugs found, bugs keep appearing. Root causes:

1. **Silent failures** -- code returns 0/None/default instead of erroring
2. **Duplicated logic** -- fixes applied to 1 of 3 copies
3. **Weak tests** -- tests that pass regardless of code correctness
4. **Missing events** -- state transitions without telemetry
5. **Stringly-typed APIs** -- string matching instead of enum dispatch

These are not individual bugs. They are systemic gaps that produce infinite individual bugs.
The sessions fix root causes, not symptoms.

---

## PART 2: ABSOLUTE RULES (non-negotiable)

### Rule 1: No Silent Zeros

```rust
// FORBIDDEN -- silently returns 0 tokens
let tokens = usage.map(|u| u.input_tokens).unwrap_or(0);

// REQUIRED -- log when fallback is used
let tokens = match usage {
    Some(u) => u.input_tokens,
    None => {
        tracing::warn!(task_id = %id, "No token usage from provider, estimating");
        estimate_tokens(prompt.len())
    }
};
```

### Rule 2: No Catch-All Match Arms

```rust
// FORBIDDEN
match event {
    Known1 => handle1(),
    _ => {} // swallows unknown variants
}

// REQUIRED
match event {
    Known1 => handle1(),
    other => {
        tracing::warn!(?other, "Unhandled event variant");
    }
}
```

### Rule 3: Every State Transition Emits an Event

Task: `Pending -> Scheduled -> Running -> Completed/Failed`
Agent: `Started -> Streaming -> Completed -> GuardrailCheck -> Retry/Done`
MCP: `Connecting -> Connected -> CallStarted -> CallCompleted -> Disconnected`

Every arrow MUST have an EventKind emission. Missing event = bug.

### Rule 4: Tests Must Assert VALUES, Not Just Ok/Err

```rust
// FORBIDDEN
assert!(result.is_ok());

// REQUIRED
let result = result.unwrap();
assert_eq!(result.tokens, 42);
assert!(result.cost > 0.0, "Cost must be positive for paid providers");
```

### Rule 5: Every Bug Fix Needs a Regression Test (TDD)

Not "add a test" -- the test must:
1. FAIL before the fix (prove the bug exists)
2. PASS after the fix (prove the fix works)
3. Test the edge case, not just the happy path

### Rule 6: No Duplication -- Extract or Die

If writing similar code in 2+ places: STOP. Extract a generic function.

### Rule 7: Errors Must Include Context

```rust
// FORBIDDEN
return Err(NikaError::PromptError(e.to_string()));

// REQUIRED
return Err(NikaError::ProviderError {
    task_id: id.to_string(),
    provider: "anthropic".to_string(),
    operation: "infer_stream",
    source: e.to_string(),
});
```

### Rule 8: Never Mask Bugs

A bug is DONE only when there is code + test proving it is fixed.
- "Investigated and determined correct" requires PROOF (a test, not an assumption)
- "Deferred" means "I failed to fix this" -- say it honestly in progress.md
- NEVER say "the provider will reject it anyway" to skip Nika-level validation
- NEVER say "this is architectural" to skip a fix -- propose the architecture change

### Rule 9: Zero Dead Code

V0 philosophy: if not called now, delete it.
- No `#[allow(dead_code)]`
- No "for future features" functions
- No "backup" implementations
- If clippy says dead, delete

### Rule 10: No Keychain Popups

Never run `nika provider list` or any command that accesses OS Keychain.
Use environment variables: `ANTHROPIC_API_KEY`, `OPENAI_API_KEY`.

---

## PART 3: TDD WORKFLOW (mandatory for every fix)

Every bug fix MUST follow this exact sequence. No exceptions.

```
1. UNDERSTAND: Read the bug. Read the code. Identify root cause.
2. WRITE TEST: Write a test that exercises the bug.
3. RUN TEST: cargo test --workspace --lib -- test_name
4. VERIFY FAIL: The test MUST fail. If it passes, the test is wrong.
   - If test passes: either the bug is already fixed (verify!) or
     the test does not actually exercise the bug. Fix the test.
5. IMPLEMENT: Write the minimal fix.
6. RUN TEST: cargo test --workspace --lib -- test_name
7. VERIFY PASS: The test MUST pass. If it fails, the fix is wrong.
8. FULL SUITE: cargo test --workspace --lib
9. CLIPPY: cargo clippy --workspace -- -D warnings
10. COMMIT: git add <files> && git commit
```

**If step 4 passes (test does not fail):**
- Check if bug was already fixed in a previous session
- Check if the test actually triggers the buggy path
- If the bug genuinely does not exist, mark as "VERIFIED NOT A BUG" with proof

**Skill invocation**: Use `/spn-powers:test-driven-development` at the start of each
bug fix to enforce discipline.

---

## PART 4: STUCK LOOP PROTOCOL (3-attempt max)

### Problem

Claude can get stuck retrying the same approach to a failing test or compilation error,
burning 15-20 minutes of context on something that needs a fundamentally different approach.

### Rule: 3 Attempts Maximum Per Bug

```
Attempt 1: Try the obvious fix.
  - If tests pass: done.
  - If tests fail: analyze WHY.

Attempt 2: Try a different approach based on root cause analysis.
  - Invoke /spn-powers:systematic-debugging
  - If tests pass: done.
  - If tests fail: analyze PATTERN.

Attempt 3: Last try with completely fresh analysis.
  - Re-read the code from scratch.
  - If tests pass: done.
  - If tests fail: SKIP.

SKIP PROTOCOL:
  1. Revert ALL changes for this bug: git checkout -- <files>
  2. Write detailed notes in progress.md:
     - Bug ID and description
     - What was tried (3 approaches)
     - Why each failed
     - Hypothesis for the real root cause
     - Suggested approach for next session
  3. Move to the next bug immediately
```

### Compilation Stuck Loop

If `cargo check` fails 3 times on the same error after different fix attempts:
1. `git stash` the current work
2. Write notes about what went wrong
3. Move to a different task in the same session
4. Return to the stashed work later with fresh perspective (or skip)

### Test Stuck Loop

If a test passes locally but the fix clearly changes behavior:
1. Check if the test is a tautological test (always passes)
2. Check if you are running the right test (name typo, wrong crate)
3. Check if `--lib` flag is filtering out the test (integration tests need different flags)

---

## PART 5: REGRESSION ROLLBACK PROTOCOL

### Problem

Session B refactors the agent loop (1505 -> 600 LOC). If done wrong, it could break
hundreds of tests. The session needs a safety net.

### Protocol: Checkpoint Before Risky Refactors

```
BEFORE any refactor that touches > 100 LOC:

1. TAG: git tag checkpoint-session-X-before-refactor
2. TEST: cargo test --workspace --lib (record count: e.g., 8613)
3. REFACTOR: Do the work in small commits
4. TEST: cargo test --workspace --lib (count must be >= previous)
5. VERIFY:
   - If count dropped by > 5: INVESTIGATE (did tests get deleted?)
   - If failures > 0: fix them
   - If failures > 20: ROLLBACK

ROLLBACK PROCEDURE:
  1. git log --oneline checkpoint-session-X-before-refactor..HEAD
     (record what was attempted)
  2. git reset --hard checkpoint-session-X-before-refactor
  3. Write in progress.md:
     - "ROLLED BACK session X refactor"
     - Commits that were reverted (copy the log)
     - What went wrong
     - Smaller decomposition plan for next attempt
  4. Continue with the next session
```

### Threshold Table

| Tests broken | Action |
|-------------|--------|
| 0 | Continue |
| 1-5 | Fix immediately |
| 6-20 | Investigate. Fix if obvious. Otherwise rollback. |
| 21+ | Rollback immediately. No exceptions. |

### Specific Risk: Session B (Agent Loop Refactor)

This is the highest-risk session. 1505 LOC -> 600 LOC across 6 atomic steps.
Each of the 6 steps MUST be a separate commit with full test suite passing.
If step N breaks tests that step N-1 did not, rollback step N only.

---

## PART 6: SESSION ORDERING DEPENDENCIES

### Identified Conflicts

**Session C (TaskEventGuard) vs Session F (EventKind grouping):**

Session C creates `TaskEventGuard` that emits `EventKind::TaskStarted`,
`EventKind::TaskFailed`, `EventKind::TaskCompleted` as flat enum variants.

Session F Part 4 restructures EventKind into grouped sub-enums:
`EventKind::Task(TaskEvent::Started)`, `EventKind::Task(TaskEvent::Failed)`, etc.

**Resolution**: Session F MUST update TaskEventGuard to use the new grouped variants.
The session plan must include TaskEventGuard as a migration target. This is safe because
Session C runs before Session F, so the guard exists when F modifies it.

**Session B (Agent Refactor) vs Session F (ProviderName enum):**

Session B refactors the agent loop which uses provider name strings extensively.
Session F replaces those strings with `ProviderName` enum.

**Resolution**: Session B should use string constants (not raw literals) where possible
to make Session F's find-and-replace easier. But Session B must NOT block on Session F.

**Session D (Quality Infra) vs Session E (Test Hardening):**

Session D adds cargo-mutants and proptest infrastructure. Session E fixes weak tests.
D should run first so E can use the new tools.

### Dependency Graph

```
A (security) -----> C (silent failures) -----> F (enums)
                                         \
B (agent refactor) -----------------------> F (enums)
                                         /
D (quality infra) -> E (test hardening) -/

G (split rig.rs) -- independent, but after B
H (LSP) ----------- independent
I (TUI) ----------- independent, but after C (uses events)

J (Phase 0) ------- after A-F (quality first)
K (routing) ------- after J
L (P-MODEL) ------- after B (needs clean agent loop)
M (P-RECORD) ------ after L
N (P-CONTEXT) ----- after M
O (polish) --------- last
```

### Safe Parallel Pairs (if running multiple agents)

- A + D (security + quality infra -- different files)
- H + I (LSP + TUI -- different crates)
- G is independent but touches provider/rig.rs which B also touches -- DO NOT parallelize

---

## PART 7: SKILLS USAGE (mandatory)

Skills are not optional suggestions. They enforce methodology.

### Per-Activity Skill Map

| Activity | Skill | When |
|----------|-------|------|
| Any bug fix | `/spn-powers:test-driven-development` | Before writing any code |
| Complex bug (attempt 2+) | `/spn-powers:systematic-debugging` | When first attempt fails |
| Session completion | `/spn-powers:verification-before-completion` | Before marking session done |
| Architecture change (Session B, G) | `/spn-powers:brainstorming` | Before first line of refactor code |
| Code review between sessions | `/spn-powers:requesting-code-review` | After committing session work |
| Multiple independent bugs | `/spn-powers:dispatching-parallel-agents` | When 3+ bugs are independent |
| Plan execution | `/spn-powers:executing-plans` | At session start, load the plan |

### Skill Enforcement

If a session involves a refactor > 200 LOC without invoking `/spn-powers:brainstorming`
first, the approach is likely wrong. Stop and invoke it.

If a session is declared "done" without `/spn-powers:verification-before-completion`,
the declaration is invalid.

---

## PART 8: E2E TESTING (after every session)

### Problem

Unit tests pass but real workflow execution fails. E2E testing catches integration bugs.

### Required E2E Checks

After EVERY session, run these commands:

```bash
# 1. Basic smoke test (mock provider, no API keys needed)
cd /Users/thibaut/dev/supernovae/nika
echo 'schema: "nika/workflow@0.12"
workflow: smoke-test
provider: mock
tasks:
  - id: hello
    infer: "Say hello"' > /tmp/smoke.nika.yaml
nika run /tmp/smoke.nika.yaml

# 2. Multi-task DAG (mock)
echo 'schema: "nika/workflow@0.12"
workflow: dag-test
provider: mock
tasks:
  - id: step1
    infer: "First step"
  - id: step2
    depends_on: [step1]
    with:
      prev: $step1
    infer: "Second step using {{with.prev}}"' > /tmp/dag.nika.yaml
nika run /tmp/dag.nika.yaml

# 3. Exec verb
echo 'schema: "nika/workflow@0.12"
workflow: exec-test
tasks:
  - id: run
    exec: "echo hello world"' > /tmp/exec.nika.yaml
nika run /tmp/exec.nika.yaml

# 4. Fetch verb (real HTTP)
echo 'schema: "nika/workflow@0.12"
workflow: fetch-test
tasks:
  - id: get
    fetch:
      url: "https://httpbin.org/get"
      extract: jsonpath
      selector: "$.origin"' > /tmp/fetch.nika.yaml
nika run /tmp/fetch.nika.yaml

# 5. Validation check
nika check /tmp/dag.nika.yaml

# 6. Dry run
nika run /tmp/dag.nika.yaml --dry-run
```

### Session-Specific E2E Tests

| Session | Extra E2E |
|---------|-----------|
| A (security) | Verify blocked commands: `exec: "python3 -c 'import os'"` must fail |
| B (agent) | `nika agent "Say hello" --turns 2 --provider mock` |
| C (events) | Run workflow, check trace file has all expected events |
| F (enums) | Workflow with every extract mode: markdown, article, text, jsonpath |
| G (rig split) | `nika infer "hello" --provider mock` (provider routing still works) |
| H (LSP) | If LSP binary builds: `nika-lsp --version` |
| K (routing) | `nika run workflow.nika.yaml --provider mock` with custom endpoint config |

### E2E with Real Provider (only if ANTHROPIC_API_KEY is set)

```bash
# Only run if API key exists
if [ -n "$ANTHROPIC_API_KEY" ]; then
  echo 'schema: "nika/workflow@0.12"
workflow: real-test
provider: anthropic
model: claude-haiku-4-5
tasks:
  - id: real
    infer: "Reply with exactly: NIKA_OK"' > /tmp/real.nika.yaml
  nika run /tmp/real.nika.yaml
fi
```

---

## PART 9: PROGRESS TRACKING FORMAT

### File: `docs/plans/sessions/progress.md`

This file is the source of truth for session handoffs. It MUST be parseable by both
humans (Thibaut monitoring from phone) and machines (next Claude session resuming).

### Strict Format

```markdown
# Autonomous Execution Progress

## Current State
- **Active session**: C (Silent Failures)
- **Current task**: Part 2: unwrap_or(0) sweep (bug 7 of 93)
- **Test count**: 8647 (was 8613 at start)
- **Clippy**: 0 warnings
- **Last push**: 2026-03-28T14:32:00Z (commit abc1234)
- **Branch**: main

## Completed Sessions

### Session A: Security (2026-03-28, 2.5h)
- **Commits**: 4 (abc1234, def5678, ghi9012, jkl3456)
- **Bugs fixed**: S1, S2, SF1, SF5, M-sec1
- **Tests added**: 12
- **Test count after**: 8625
- **Skipped**: SF6 (EventLog trace drops -- needs nika-event refactor, 3 attempts failed)
- **E2E**: smoke OK, security-blocked OK, dag OK

### Session B: Agent Refactor (2026-03-28, 4h)
- **Commits**: 6 (one per atomic step)
- **LOC before/after**: 1505 -> 612
- **Tests added**: 8
- **Test count after**: 8633
- **Skipped**: none
- **E2E**: agent 2-turn OK, mock OK

## Skipped Bugs (for next session to pick up)

### SF6: EventLog trace drops (Session A, attempt 3/3)
- **What**: `let _ = trace_writer.write()` drops write errors
- **Tried**: (1) Result propagation, (2) channel-based async write, (3) fallback file
- **Why failed**: EventLog is sync, trace_writer is async. Needs architectural decision.
- **Suggestion**: Add `on_error: log` callback to EventLog::new()

## Current Session Detail

### Session C: Silent Failures (in progress)
- [x] Part 1: TaskEventGuard (commit mno7890)
- [ ] Part 2: unwrap_or(0) sweep (7/93 done)
  - [x] cost.rs (3 instances)
  - [x] infer.rs (4 instances)
  - [ ] rig.rs (12 instances) <-- CURRENT
  - [ ] streaming.rs (8 instances)
  - ...
- [ ] Part 3: SchemaGuardrail validation
- [ ] Part 4: Event emission coverage
```

### Update Frequency

- After each commit: update "Current task" line
- After each session: add to "Completed Sessions"
- After each skip: add to "Skipped Bugs"
- After each push: update "Last push" timestamp

### Recovery Protocol (When Context Window Fills)

When the context window is getting compressed (you notice older messages disappearing
or you have been working for > 3 hours):

```
1. STOP current work.
2. cargo test --workspace --lib (record result)
3. cargo clippy --workspace -- -D warnings (record result)
4. git add . && git commit (save work in progress)
5. git push
6. Update progress.md with:
   a. Move current session to "Current Session Detail" with exact checkpoint
   b. List which file you were editing and what line
   c. List which bugs remain in current session
   d. Write "HANDOFF: Resume from [exact description]"
7. The next Claude session starts with:
   cat docs/plans/sessions/progress.md
   cat docs/plans/2026-03-28-quality-methodology.md
   cat docs/plans/sessions/session-X-name.md  (current session)
```

---

## PART 10: WATCHDOG COMMANDS (for Thibaut)

```bash
# Quick status from phone
git log --oneline -20
cat docs/plans/sessions/progress.md | head -30

# Test health
cargo test --workspace --lib 2>&1 | tail -5

# Detailed progress
cat docs/plans/sessions/progress.md

# Check if stuck (no commits in > 30 min)
git log --format="%ar %s" -5

# Check for rollbacks
git log --oneline --all | grep -i "rollback\|revert"
```

---

## PART 11: ANTI-PATTERNS (learned from past sessions)

These are real mistakes from previous autonomous sessions. Each one wasted 30+ minutes.

### Anti-Pattern 1: Claiming "Not a Bug"

**What happened**: In v0.51, 5+ bugs were marked "investigated = not a bug" when they
were actually real bugs.
**Rule**: A bug is not "not a bug" unless you have a TEST proving the behavior is correct.

### Anti-Pattern 2: Pre-Commit Hook Interference

**What happened**: Pre-commit hooks auto-committed unrelated formatter changes during
bulk-fix sessions, breaking 1-fix-1-commit atomicity.
**Rule**: Always `git status` before each commit. If unexpected files are staged, unstage them.
For background agents, use `isolation: "worktree"`.

### Anti-Pattern 3: Infinite Retry Loop

**What happened**: Agent spent 25 minutes trying 7 variations of the same fix for a
compilation error that needed a completely different approach.
**Rule**: 3 attempts max (see Part 4).

### Anti-Pattern 4: Testing the Mock Instead of the Code

**What happened**: Tests passed because they asserted mock behavior, not real behavior.
When mocks were updated, tests still passed because they were tautological.
**Rule**: Tests must assert specific values from computation, not just Ok/Err.

### Anti-Pattern 5: Silent .ok() Swallowing

**What happened**: `jsonschema::validator_for().ok()` silently disabled validation when
the schema was invalid. The entire structured output feature was broken but tests passed.
**Rule**: `.ok()` is forbidden on any operation that can meaningfully fail. Use `match`
or `map_err` with logging.

### Anti-Pattern 6: Forgetting --lib Flag

**What happened**: `cargo test` (without --lib) triggered macOS Keychain popups,
blocking the machine.
**Rule**: ALWAYS `cargo test --workspace --lib`. Never `cargo test` bare.

### Anti-Pattern 7: Large Refactor Without Checkpoints

**What happened**: A 300-LOC refactor broke 47 tests. No checkpoint existed, so
30 minutes of git archaeology was needed to understand what changed.
**Rule**: Tag before refactor, commit atomically, rollback if > 20 tests break.

### Anti-Pattern 8: Editing the Wrong Copy

**What happened**: Bug fixed in `run_claude()` but not in `run_openai()` or `run_generic()`.
Same bug reappeared 2 weeks later from the other copy.
**Rule**: Before fixing, grep for ALL instances. Fix ALL of them in the same commit.

---

## PART 12: INTER-SESSION CHECKLIST

Run this checklist between EVERY session. No exceptions.

```
[ ] cargo test --workspace --lib         -> 0 failures
[ ] cargo clippy --workspace -- -D warnings -> 0 warnings
[ ] E2E smoke test (mock provider)       -> passes
[ ] git push                             -> pushed to remote
[ ] progress.md updated                  -> current state accurate
[ ] Skipped bugs documented              -> with 3 attempts + notes
[ ] Session-specific E2E tests           -> passed (see Part 8 table)
[ ] /spn-powers:verification-before-completion invoked -> confirmed
```

If ANY check fails, fix it before starting the next session.

---

## PART 13: TOOLING ADDITIONS (for sessions D and beyond)

### 13.1 cargo-mutants -- Find Weak Tests

```bash
cargo install --locked cargo-mutants
cargo mutants -p nika-engine -- --lib
cargo mutants -p nika-engine -f src/provider/cost.rs -- --lib
cargo mutants -p nika-engine -f src/runtime/security.rs -- --lib
```

Each surviving mutant = a weak test. Fix the test, not the production code.

### 13.2 proptest -- Property-Based Testing

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn cost_never_negative(
        input_tokens in 0u64..1_000_000,
        output_tokens in 0u64..1_000_000,
        cached in 0u64..1_000_000,
    ) {
        let cost = calculate_cost_with_cache(
            ProviderKind::Claude, "claude-sonnet-4-6",
            input_tokens, output_tokens, cached
        );
        prop_assert!(cost >= 0.0);
        prop_assert!(cost.is_finite());
    }
}
```

### 13.3 Strict Clippy Configuration

```toml
[workspace.lints.clippy]
unwrap_used = "deny"
wildcard_enum_match_arm = "warn"
manual_assert = "warn"
redundant_else = "warn"

[workspace.lints.rust]
unsafe_code = "deny"
missing_debug_implementations = "warn"
```

### 13.4 CI grep checks (fast, every commit)

```bash
# These patterns should produce 0 matches (excluding tests)
grep -rn "unwrap_or(0)" src/ --include='*.rs' | grep -v '#\[cfg(test)\]' | grep -v 'mod tests'
grep -rn '_ => {}' src/ --include='*.rs' | grep -v '#\[cfg(test)\]' | grep -v 'mod tests'
```

---

## PART 14: DECISION LOG

Record architectural decisions made during autonomous execution so Thibaut can review.

### Format (append to progress.md)

```markdown
## Decisions Made

### D1: TaskEventGuard uses Arc<str> for task_id (Session C)
- **Context**: task_id is cloned into events
- **Decision**: Arc<str> avoids allocation on each emit
- **Alternative**: String clone (simpler but slower)
- **Rationale**: Events are emitted on hot path, Arc<str> is clone-cheap

### D2: ExtractMode lives in nika-core, not nika-engine (Session F)
- **Context**: Both crates need the enum
- **Decision**: Define in nika-core, re-export from nika-engine
- **Alternative**: Define in nika-engine (current pattern)
- **Rationale**: nika-core is the AST/type crate, nika-engine is the runtime
```

Decisions that affect public API, error codes, or file organization MUST be logged.
Minor implementation choices (variable names, test structure) do not need logging.

---

## PART 15: EMERGENCY PROCEDURES

### If All Tests Suddenly Break (> 100 failures)

```
1. DO NOT PANIC. DO NOT try random fixes.
2. git stash (save current work)
3. git log --oneline -10 (find last known-good commit)
4. git checkout <last-good-commit> -- .
5. cargo test --workspace --lib (verify it passes)
6. git diff HEAD (see what the stash would change)
7. Apply changes one file at a time, testing after each
```

### If cargo check Takes > 5 Minutes

Workspace dependency issue. Check for circular deps or massive recompile:
```bash
cargo check --workspace 2>&1 | head -20
# If it shows "Compiling" 100+ crates, something is wrong with deps
```

### If Git Gets Into a Bad State

```bash
git status
git log --oneline -5
# If detached HEAD:
git checkout main
# If merge conflict:
git merge --abort
# If rebase gone wrong:
git rebase --abort
```

### If Context Window Is Almost Full

Priority order:
1. Commit current work (even if incomplete)
2. Push
3. Update progress.md with HANDOFF section
4. STOP. Do not try to squeeze in one more fix.

---

*This methodology is the law. Every session, every fix, every commit follows it.
No shortcuts. No exceptions. Quality > speed.*
