# Migration Verification — How to Prove Each Commit Safe

> Every verification command, every gate criterion, every evidence requirement before claiming a commit is "done".

## Baseline commands (run before any session starts)

```bash
cd /Users/thibaut/dev/supernovae/nika

# 1. Git state
git log --oneline -1
# EXPECTED (S12 start): c5ea27438 docs(engine): update ARCHITECTURE.md ...
git status
# EXPECTED: clean on tools/, may have Thibaut's parallel launch-prep work (NEVER touch)

cd tools

# 2. Test suite baseline
cargo test --workspace --lib 2>&1 | grep -E "^test result" > /tmp/baseline-tests.txt
awk '/^test result: ok/ {s+=$4} END{print s}' /tmp/baseline-tests.txt
# EXPECTED (S12 start): 10769

# 3. Clippy baseline
cargo clippy --workspace --lib -- -D warnings
# EXPECTED: no warnings, exit 0

# 4. LOC baseline
find nika-engine/src -name "*.rs" | xargs wc -l | tail -1
# EXPECTED (S12 start): 148792 total

# 5. Diamond layering guard
grep -rn "nika-engine" nika-builtin/Cargo.toml
# EXPECTED: empty

# 6. Release binary baseline (first time recording)
cargo build --release -p nika
ls -lh target/release/nika
# EXPECTED: ~118 MB
```

**If ANY of the above fails the expected value, STOP. Investigate before proceeding.**

---

## Per-commit verification checklist

Every commit in S12/S13/S14 MUST satisfy all of the following before marking complete:

### Gate 1 — Tests pass

```bash
# For the specific crate touched:
cargo test -p <crate> --lib 2>&1 | tail -10
# EXPECTED: "test result: ok. X passed; 0 failed"

# Full workspace as the final check:
cargo test --workspace --lib 2>&1 | tail -10
# EXPECTED: "test result: ok"
```

**Failure response:** fix the tests or the code. NEVER delete a failing test to pass the gate.

### Gate 2 — Clippy clean

```bash
# The crate touched:
cargo clippy -p <crate> --lib -- -D warnings 2>&1 | tail -5
# EXPECTED: "Finished" with no warnings

# Workspace-wide for safety:
cargo clippy --workspace --lib -- -D warnings
```

**Failure response:** fix the warnings. NEVER add `#[allow]` without a justification comment.

### Gate 3 — Formatting

```bash
cargo fmt --all -- --check
# EXPECTED: no output, exit 0
```

**Failure response:** run `cargo fmt --all` and amend the commit.

### Gate 4 — Diamond layering (for new crates)

```bash
# Verb crates must NOT depend on nika-engine:
cargo tree -p nika-verb-exec --edges normal | grep -c nika-engine
# EXPECTED: 0 (or documented TEMP with justification)

# Repeat for all new crates:
for crate in nika-policy nika-extract nika-runtime nika-verb-exec nika-verb-invoke nika-verb-fetch nika-verb-infer nika-verb-agent nika-shield; do
    count=$(cargo tree -p $crate --edges normal 2>/dev/null | grep -c nika-engine)
    echo "$crate: $count engine deps"
done
# EXPECTED: all 0, except explicitly-documented TEMP exceptions in Phase 14+
```

**Failure response:** audit the Cargo.toml. If a new engine dep is unavoidable, add a comment `# TEMP: <reason>, target Phase 15 cleanup` and track in the risk register.

### Gate 5 — No unwrap / expect in new code

```bash
# Check the files touched in this commit:
git diff HEAD~1 HEAD --name-only --diff-filter=AM | grep "\.rs$" | xargs grep -n "\.unwrap\|\.expect" 2>/dev/null | grep -v "^--" | grep -v "//"
# EXPECTED: empty (no unwrap/expect in production code)
```

**Failure response:** refactor to `?` with proper error variants. Exception: `#[cfg(test)]` code can use `unwrap` freely.

### Gate 6 — AGPL header present

```bash
# Check new files added in this commit:
git diff HEAD~1 HEAD --name-only --diff-filter=A | grep "\.rs$" | while read f; do
    if ! head -3 "$f" | grep -q "SPDX-License-Identifier: AGPL-3.0-or-later"; then
        echo "MISSING HEADER: $f"
    fi
done
# EXPECTED: no output
```

**Failure response:** add the header:
```rust
// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
```

### Gate 7 — Nika co-author line

```bash
git log -1 --format="%(trailers:key=Co-Authored-By)"
# EXPECTED: contains "Nika 🦋 <nika@supernovae.studio>"
# FORBIDDEN: Claude, Anthropic, any AI attribution
```

**Failure response:** amend the commit with the correct trailer. NEVER use Claude as co-author per `feedback_no_claude_coauthor.md`.

### Gate 8 — LOC tracking

```bash
# Engine LOC must move in the expected direction:
find tools/nika-engine/src -name "*.rs" | xargs wc -l | tail -1
```

Record in the commit body or session memory file. If LOC moves the WRONG direction (e.g., a `delete` commit increases engine LOC), STOP and investigate.

### Gate 9 — Golden e2e tests pass (for S13/S14 verb extraction commits)

The golden e2e suite is the regression oracle for behavior preservation during verb extraction. Every commit that touches verb dispatch, VerbCapabilities, or deletes executor/*.rs MUST pass:

```bash
# Once the golden suite is in place (Session 12 closure or S13 start):
cargo test --workspace --lib golden
# EXPECTED: all golden tests green
```

**Failure response:** CORRECTNESS REGRESSION. Diff the old vs new verb implementations. Fix the bridge, not the test.

### Gate 10 — Release binary compiles (session close only)

```bash
cargo build --release -p nika
ls -lh target/release/nika
```

Record the binary size in the session memory file. If size grows >5 MB vs the baseline (118 MB), investigate per R14-7.

---

## Per-wave verification

At the end of each "wave" (a logical group of 3-5 commits), run the full gate suite:

```bash
#!/bin/bash
# scripts/verify-wave.sh (create on-demand)

set -euxo pipefail

cd tools

echo "=== Gate 1: Tests ==="
cargo test --workspace --lib 2>&1 | tail -5

echo "=== Gate 2: Clippy ==="
cargo clippy --workspace --lib -- -D warnings 2>&1 | tail -5

echo "=== Gate 3: Formatting ==="
cargo fmt --all -- --check

echo "=== Gate 4: Diamond layering ==="
for crate in nika-policy nika-extract nika-runtime nika-shield nika-verb-exec nika-verb-invoke nika-verb-fetch nika-verb-infer nika-verb-agent; do
    if cargo tree -p $crate >/dev/null 2>&1; then
        count=$(cargo tree -p $crate --edges normal 2>/dev/null | grep -c nika-engine || echo "0")
        echo "  $crate: $count engine deps"
    fi
done

echo "=== Gate 8: LOC tracking ==="
find nika-engine/src -name "*.rs" | xargs wc -l | tail -1

echo "=== All gates passed ==="
```

**If any gate fails during wave verification, the wave is incomplete and the session pauses.**

---

## Bridge pattern verification (S13/S14)

When a verb is extracted but TaskExecutor still delegates (the "bridge stage"), both paths must produce identical behavior.

**Bridge pattern:** `TaskExecutor` has a method (e.g. `run_exec`) whose body becomes a 5-line delegation to the new verb crate's `run` free function. The old caller signature is preserved; the implementation is redirected.

**Bridge verification steps:**
1. All pre-existing tests for the verb (e.g. `cargo test -p nika-engine --lib exec`) still green
2. All new tests in the verb crate (e.g. `cargo test -p nika-verb-exec --lib`) green
3. Event log assertions match (the bridge must emit the same `EventKind` sequence as the original)
4. Error variants match (the bridge's `Into::into` must produce equivalent `NikaError` values)

**Test oracle:** snapshot-based event sequence comparison:

```rust
#[tokio::test]
async fn bridge_event_sequence_matches() {
    let events = /* run through bridge */;
    insta::assert_yaml_snapshot!(events);
    // Compare against the pre-extraction snapshot — must be byte-identical
}
```

---

## Delete-commit verification (S13/S14)

When deleting a file (`chore(engine): delete runtime/executor/<file>.rs`):

1. **Pre-delete grep:** list all references to the file's public items. Each must be either (a) the delegation in `executor/mod.rs`, (b) a test that now goes through the new crate, or (c) fixable by switching to the new crate's public API.

2. **Post-delete compile:** `cargo check -p nika-engine` — must be clean. If it fails, a caller was missed in step 1.

3. **Post-delete tests:** `cargo test --workspace --lib` — must be green. Orphaned tests that referenced the deleted file must be migrated or removed as part of the same commit.

4. **LOC decrement:** `find tools/nika-engine/src -name "*.rs" | xargs wc -l | tail -1` must show the expected decrement (471 for exec file, 1327 for extract file, etc.).

---

## Session close verification

At the end of each session (S12/S13/S14), before pushing to remote:

### Full workspace verification

```bash
cd /Users/thibaut/dev/supernovae/nika/tools

# 1. Full test suite
cargo test --workspace --lib 2>&1 > /tmp/session-close-tests.txt
awk '/^test result: ok/ {s+=$4} END{print s}' /tmp/session-close-tests.txt
# EXPECTED: >= pre-session count (tests only grow)

# 2. Clippy workspace
cargo clippy --workspace --lib -- -D warnings
# EXPECTED: clean

# 3. Release build + size
cargo build --release -p nika 2>&1 | tail -5
ls -lh target/release/nika
# Record size delta vs session start

# 4. Engine LOC
find nika-engine/src -name "*.rs" | xargs wc -l | tail -1
# EXPECTED: matches session plan target

# 5. Crate count
grep -c "^\s*\"" Cargo.toml
# EXPECTED: matches session plan target

# 6. Diamond layering for all new crates
for crate in $(ls -d */ | grep "^nika-"); do
    crate_name=${crate%/}
    count=$(cargo tree -p $crate_name --edges normal 2>/dev/null | grep -c nika-engine || echo "N/A")
    echo "  $crate_name: $count"
done
```

### Memory files updated

```bash
ls -la ~/.claude/projects/-Users-thibaut-dev-supernovae-nika/memory/project_constellation_session*.md
# EXPECTED: new file for the just-finished session

head ~/.claude/projects/-Users-thibaut-dev-supernovae-nika/memory/MEMORY.md
# EXPECTED: updated Quick State + new session entry
```

### Documentation updated

```bash
grep -c "Session {N}" tools/nika-engine/ARCHITECTURE.md
# EXPECTED: mention of the session's changes

git log --oneline | head -20
# EXPECTED: the session's commits with clean Conventional Commits messages
```

### User authorization for push

**NEVER push without explicit user authorization.** Ask:
> "Session {N} complete. Ready to `git push origin main`. Authorize?"

Only proceed after "yes" / "go" / "push".

---

## Red flags that mean STOP

If any of the following appear, STOP the session and investigate:

1. **Tests count decreases unexpectedly** — code was deleted without tests being migrated
2. **Clippy warnings count > 0** — regression on the zero-warnings policy
3. **Engine LOC increases during a delete commit** — wrong direction, something wasn't deleted
4. **Diamond layering breach** — a new `nika-verb-*` crate pulls in nika-engine without TEMP justification
5. **Golden test failure** — CORRECTNESS regression, do not proceed with bridge delete
6. **unwrap / expect appears in new production code** — zero-unwrap ratchet violation
7. **Nika co-author line missing** — wrong attribution
8. **Push blocked by hooks** — investigate the hook failure, don't skip with `--no-verify`

---

## The oracle: golden e2e tests

Golden e2e tests are the highest-value safety net for the refactor. They must:

1. **Go through `Runner::run`** — not TaskExecutor, not individual verb methods
2. **Use deterministic mocks** — `MockShellExecutor`, `MockHttpClient`, `MockProvider` from `nika-kernel-mock`
3. **Assert output content** — not just exit code, the actual string or JSON
4. **Assert event sequence** — via `EventLog::snapshot` + `insta::assert_yaml_snapshot`
5. **Cover all 5 verbs** — each with at least one golden workflow
6. **Live in `tests/golden/`** — not in individual verb crates (they test end-to-end, not unit)

**Setup in Session 12 closure OR early Session 13.** Without this oracle, Sessions 13-14 proceed blind. The rust-architect research agent flagged this as R-ALL-3 (cross-session blocker).

---

## Final session close command

```bash
cd /Users/thibaut/dev/supernovae/nika

# After all gates pass, commits landed, memory updated:
git log --oneline | head -25
# Review the session's commits

# User approval step:
# > "Ready to push N commits. Authorize?"

# On "yes":
git push origin main

# Verify push succeeded:
git log origin/main..HEAD
# EXPECTED: empty (local == remote)
```

Session closed.
