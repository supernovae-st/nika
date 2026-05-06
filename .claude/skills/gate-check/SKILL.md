---
name: gate-check
description: Run the 12 gates against a named crate and report pass/fail per gate. Use before crate admission or when verifying gate progress on an in-flight crate.
argument-hint: [crate-name]
allowed-tools: Bash, Read, Grep
---

# 12-Gate Check — `$ARGUMENTS`

Run each gate sequentially on crate `$ARGUMENTS`. Report PASS / FAIL / N/A with a 1-line reason.

## Gate 1 — SPEC

```bash
test -f docs/crate-specs/$ARGUMENTS.md && echo PASS || echo FAIL
```

## Gate 2 — TDD (tests written first)

```bash
grep -rc -E '#\[(test|tokio::test)\]' crates/$ARGUMENTS/src 2>/dev/null | awk -F: '{s+=$2} END{print s " tests"}'
```
PASS if tests > 20 per 1k LOC. Check `git log --oneline crates/$ARGUMENTS/` for test-first commit order.

## Gate 3 — IMPL (compiles, tests pass)

```bash
cargo test -p $ARGUMENTS --lib 2>&1 | tail -5
```

## Gate 4 — CLIPPY 0

```bash
cargo clippy -p $ARGUMENTS --all-targets -- -D warnings 2>&1 | grep -E '(error|warning)' | wc -l
```
PASS if output is 0.

## Gate 5 — MUTATION ≥ 90 %

```bash
cargo mutants -p $ARGUMENTS --timeout 30 2>&1 | tail -5
```

## Gate 6 — PROPERTY (proptest if parser/security/encoding)

```bash
grep -rE 'proptest!|quickcheck!' crates/$ARGUMENTS/ 2>/dev/null | wc -l
```
N/A if crate is pure types. Document exemption in spec.

## Gate 7 — BENCHMARKS (if hot path)

```bash
test -d crates/$ARGUMENTS/benches && echo PASS || echo "N/A (not hot path)"
```

## Gate 8 — DOCS (cargo doc 0 warnings, pub items documented)

```bash
cargo doc --no-deps -p $ARGUMENTS 2>&1 | grep -iE 'warning' | wc -l
```

## Gate 9 — CANARY E2E

```bash
test -f tests/canary-$ARGUMENTS.nika.yaml && echo PASS || echo "N/A"
```

## Gate 10 — PARITY LEGACY

Crate-specific. Check golden-test file against `git show brouillon:…` output.

## Gate 11 — REVIEW SWARM

Launch 3 parallel agents. See `.claude/agents/review-swarm.md`.

## Gate 12 — ATOMIC COMMIT

```bash
git log --oneline --grep "admit to workspace" crates/$ARGUMENTS/ | head -5
```
PASS = exactly 1 commit `feat($ARGUMENTS): admit to workspace — all 12 gates passed`, co-authored Nika 🦋.

---

After running all 12 gates, print a summary table:

```
Gate  Status  Detail
────  ──────  ──────────────
1     PASS    docs/crate-specs/$ARGUMENTS.md exists
2     PASS    87 tests
3     PASS    all green
...
```

If any FAIL → stop, report why. Do NOT proceed with admission.
