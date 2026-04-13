# Deletion-First Rules

**The single most important refactoring principle for Constellation.**

## The D/A Ratio

**D/A (Deletion/Addition) ratio > 2.0** per session.

For every line of code added, at least 2 must be deleted. Measure at the WORKSPACE level, not per commit. A session that bundles "add 500 LOC helper" with "delete 1,200 LOC fossil" = D/A 2.4 = valid.

A session where D/A < 1.0 is AUTOMATICALLY re-labeled as an "investment session" in the memo, and the NEXT session MUST be D/A > 3.0 to compensate.

## The Anti-Pattern: "Abstraction-First"

S14-S17 added +1,177 LOC to the engine across 5 sessions. Why? Because each session designed abstractions (traits, adapters, capability bundles) WITHOUT deleting the code they were meant to replace. The old code stayed. The new code piled up.

**Rule**: never add an abstraction without deleting what it replaces in the same PR. If you can't delete yet, don't add yet. Write an ADR describing the design and wait.

## The Nuke-Over-Shim Rule

When you find dead code, theatrical code, or "reserved for future use" code:

- **Don't add a `# TEMP` marker** — that's a debt IOU with no maturity date
- **Don't wrap it in `#[cfg(feature = "legacy")]`** — zero users means zero compat debt
- **Don't put a `// TODO: remove when X lands`** — nobody will
- **DELETE IT NOW**

If you're wrong and someone needed it, git still has it. Undelete takes 30 seconds.

## Deletion Priority Order

When you have time to delete code, do it in this order:

1. **Dead crates** (no dependants): biggest wins, zero risk
2. **Dead functions** (no callers): easy grep verification
3. **Theatrical code** (runs in tests only, not production)
4. **Duplicate code** (same logic in multiple places)
5. **Phantom error variants** (defined, never constructed)
6. **Unused struct fields** (`#[allow(dead_code)]`)
7. **Stale comments** (reference deleted code, old session numbers)
8. **Redundant tests** (cover what other tests already cover)

## Verification Before Deletion

For each candidate, run:

```bash
# Dead crate check
grep -r "use <crate_name>\|<crate_name>::" tools --include="*.rs"

# Dead function check
grep -rn "<function_name>" tools --include="*.rs"

# Dead enum variant check
grep -rn "<EnumName>::<Variant>" tools --include="*.rs"

# Dead field check
grep -rn "\.<field_name>" tools --include="*.rs"
```

If grep returns 0 hits (or only hits within the same file's tests) = safe to delete.

## The Scanner Effect

**Extractions reveal bugs that coupling hid.**

Every Constellation session that extracted code has found previously-invisible bugs:

- S18: redact_value stack overflow DoS (no depth bound for 4,500 commits)
- S18: 6 providers leaked API keys in traces
- S19: kill_on_drop missing (orphan processes possible)
- S20: `\r` bypassed blocklist ASCII fast-path
- S22: unwrap count was wrong (script bug)

When extraction reveals a bug, **fix it in the same session**. Don't defer to a "follow-up". The coupling is what hid it; exposing it and fixing it is the WHOLE POINT of extraction.

## Zombie Debt

Every deferred item goes into `project_zombie_backlog.md` with:
- First mention (session number)
- Last mention (session number)
- Root cause (architectural blocker vs execution discipline)
- Owner session (when it resolves)
- Effort estimate

**If a zombie is open 4+ sessions after being assigned an owner**, the session that should have resolved it is DECLARED A FAILURE and a dedicated zombie-only session is scheduled.

## Commit Message Format

Deletion commits should clearly state what was removed and why:

```
nuke(scope): delete <thing> — <reason>

- <what was removed>
- Grep evidence: <command showing 0 callers>
- Risk: <None | Low | Medium>
- Enables: <what this unblocks>
```

Example:

```
nuke(runtime): delete nika-runtime crate — 0 production callers

- Removed tools/nika-runtime/ (8 files, 738 LOC)
- Removed from workspace Cargo.toml
- Grep evidence: `grep -r "use nika_runtime" tools --include="*.rs"` returns 0 hits
- Risk: None — crate was theatrical since S13
- Enables: clean path to M8 runner rewire without legacy dispatch layer
```
