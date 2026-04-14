# Commit Granularity — rules for atomic, traceable history

## Core principle

One logical change = one commit. The git log is the changelog.
Every commit must be self-contained: compiles, tests pass, clippy 0.

## Commit taxonomy

### `feat(nika-X): admit to workspace — all 12 gates passed`
Crate admission. The most important commit type.
**Must include**: LOC count, test count, mutation score in body.
**Must not batch**: never admit two crates in one commit.

### `feat(nika-X): <what changed>`
New capability added to an admitted crate.
**Must include**: which gate(s) affected, test delta.
**Granularity**: one feature per commit. Do not mix features.

### `fix(nika-X): <what was wrong>`
Bug fix in admitted crate code.
**Must include**: what was wrong, why the fix is correct.
**Granularity**: one bug per commit.

### `refactor(nika-X): <what changed structurally>`
Code reorganization with no behavior change.
**Must prove**: tests still pass (include `cargo test` output summary).
**Granularity**: one module / one pattern per commit.

### `test(nika-X): <what is now tested>`
Tests added to an already-admitted crate.
**Must include**: which invariant is now covered.

### `docs(<scope>): <what changed>`
Documentation or `.mdx` file changes.
**Scope**: `mintlify`, `dx`, `spec`, `changelog`, `roadmap`.
**Granularity**: one logical doc area per commit (don't mix mintlify + rules).

### `refactor(dx): <what changed>`
DX tooling, scripts, hooks, rules changes.
**Granularity**: one tool/file/rule per commit when possible.

### `chore(<scope>): <what changed>`
Mechanical changes: dep bumps, CI config, `.gitignore`.

## What must NOT be batched in one commit

| ❌ Don't batch | ✅ Instead |
|---|---|
| Two crate admissions | Two separate feat commits |
| Feature + refactor | Separate commits |
| Code + docs for different crates | Per-crate commits |
| Multiple unrelated DX fixes | One commit per fix |
| Bugfix + new feature | Separate commits |

## Crate admission commit body (mandatory format)

```
feat(nika-X): admit to workspace — all 12 gates passed

Gate 1  SPEC     ✅  docs/crate-specs/nika-X.md
Gate 2  TDD      ✅  RED before GREEN confirmed
Gate 3  IMPL     ✅  <N> LOC, compiles, tests pass
Gate 4  CLIPPY   ✅  0 warnings
Gate 5  MUTATION ✅  <N>% killed
Gate 6  PROPERTY ✅  <N> proptest cases
Gate 7  BENCHMARKS ✅ / N/A (justified in spec)
Gate 8  DOCS     ✅  0 cargo doc warnings
Gate 9  CANARY   ✅ / N/A (justified)
Gate 10 PARITY   ✅  golden test vs legacy
Gate 11 REVIEW   ✅  3-agent swarm, P0/P1 fixed
Gate 12 ATOMIC   ✅  this commit

LOC: <N> src | Tests: <N> | Mutation: <N>%

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

## Significant DX/docs commit checklist

Before committing docs or DX changes, verify:
- [ ] CHANGELOG.md updated if code behavior changed
- [ ] ROADMAP.md consistent with new state
- [ ] constellation.mdx updated if crate count changed
- [ ] status.mdx updated if phase/session changed
- [ ] MEMORY.md Quick State HEAD updated after push

## Commit message style

- Imperative mood: "add", "fix", "remove", not "added", "fixed"
- 72 chars max on subject line
- Blank line between subject and body
- Body explains WHY, not WHAT (the diff shows what)
- Always co-authored by Nika 🦋 (never Claude)

## Forbidden patterns

- ❌ `git commit -m "wip"` — never commit work in progress
- ❌ `git commit -m "fix stuff"` — must name what was fixed
- ❌ `git commit --amend` on pushed commits
- ❌ `--no-verify` — fix the hook instead
- ❌ Two crates in one admission commit

🦋
