---
name: nika-sync
description: Run full Nika alignment validation - spec/code/docs sync check + Rust quality
---

# Nika Sync Validation

Run comprehensive validation of the Nika project alignment.

## What This Command Does

1. **Spec-Code Alignment**
   - Schema version match (spec vs code constants)
   - Action count match (spec sections vs enum variants)
   - Error codes sync (spec definitions vs error.rs)

2. **Documentation Sync**
   - CLAUDE.md version matches spec
   - Anti-hallucination list is current

3. **Rust Quality**
   - cargo check (compilation)
   - cargo clippy (lints)
   - cargo test --lib (unit tests)
   - Unused code detection

4. **Report Generation**
   - Checklist with pass/fail status
   - Specific fix suggestions for any drift

## Execution

Use the `nika-sync` agent to run the full validation:

```
Task: Run nika-sync agent for full validation report
```

## Quick Checks

For quick status without full validation:

```bash
# Check cached status
cat .claude/.nika-status

# Run health check manually
.claude/hooks/nika-health-check.sh
```

## When to Run

- After editing `spec/SPEC.md`
- After editing `src/ast/` or `src/binding/`
- After editing `CLAUDE.md` or `.claude/` files
- Before creating a PR
- When unsure about alignment

## Expected Output

```
## Nika Sync Report

**Status:** 🟢 ALIGNED | 🟡 DRIFT | 🔴 BROKEN

### Alignment Checks
- [x] Schema: workflow@0.1
- [x] Actions: 3 (infer, exec, fetch)
- [x] Version: 0.1
- [x] Error codes: spec=X, code=Y

### Rust Quality
- [x] Compiles: yes
- [x] Clippy: 0 warnings
- [x] Tests: PASS
- [x] Unused: 0 items

### Fix Suggestions (if any)
1. ...
2. ...
```
