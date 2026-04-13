# Diamond Gates — 12-gate checker for crate admission

Check all 12 gates for a specific crate. Usage: `/diamond-gates nika-error`

## Argument

$ARGUMENTS = crate name (e.g. "nika-error")

## Automated gates (run these)

```bash
CRATE="$ARGUMENTS"

echo "=== GATE 1: SPEC ==="
test -f "docs/crate-specs/$CRATE.md" && echo "✅ Spec exists" || echo "❌ Missing docs/crate-specs/$CRATE.md"

echo "=== GATE 3: IMPL (compiles) ==="
cargo check --workspace 2>&1 | tail -3

echo "=== GATE 4: CLIPPY 0 ==="
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -5

echo "=== GATE 4b: UNWRAP 0 ==="
UNWRAPS=$(rg '\.unwrap\(\)' "tools/$CRATE/src" --type rust -g '!*test*' 2>/dev/null | wc -l | tr -d ' ')
echo "Unwraps in src/: $UNWRAPS (target: 0)"

echo "=== GATE 4c: DEAD CODE 0 ==="
rg '#\[allow\(dead_code\)\]' "tools/$CRATE/src" --type rust 2>/dev/null | wc -l

echo "=== GATE 4d: FILE SIZE ==="
find "tools/$CRATE/src" -name '*.rs' | xargs wc -l 2>/dev/null | awk '$1 > 1500 {print "❌", $0}'
find "tools/$CRATE/src" -name '*.rs' | xargs wc -l 2>/dev/null | tail -1

echo "=== GATE 8: DOCS ==="
cargo doc --no-deps -p "$CRATE" 2>&1 | grep -c 'warning' || echo "0 warnings"

echo "=== GATE 3b: TESTS PASS ==="
cargo test -p "$CRATE" --lib 2>&1 | tail -5
```

## Manual gates (check these yourself)

```
GATE 2  — TDD: Were tests written BEFORE implementation?
           → Review git log for test-first commits

GATE 5  — MUTATION: cargo mutants -p $CRATE (≥90% killed?)
           → Run: cargo mutants -p $CRATE

GATE 6  — PROPERTY: proptest exists if security-sensitive?
           → Check: grep -r 'proptest' tools/$CRATE/

GATE 7  — BENCHMARKS: benches/ exists if hot path?
           → Check: ls tools/$CRATE/benches/ 2>/dev/null

GATE 9  — CANARY E2E: tests/canary-$CRATE.nika.yaml exists?
           → Check: ls tests/canary-$CRATE.nika.yaml 2>/dev/null
           → (exempt for L0/L1 crates without runtime)

GATE 10 — PARITY: golden test vs git show main:... output?
           → Check parity test exists in test suite

GATE 11 — REVIEW SWARM: 3 agents ran? P0/P1 fixed?
           → Dispatch: spn-nika:code-reviewer + spn-rust:rust-pro + feature-dev:code-reviewer

GATE 12 — ATOMIC COMMIT: Ready to commit?
           → Format: feat($CRATE): admit to workspace — all 12 gates passed
```

## Summary

After running, report:

```
🚦 GATE STATUS — $CRATE

 1. SPEC         ✅|❌
 2. TDD          ✅|❌ (manual)
 3. IMPL         ✅|❌
 4. CLIPPY 0     ✅|❌
 5. MUTATION     ✅|❌ (manual, score: N%)
 6. PROPERTY     ✅|❌|EXEMPT
 7. BENCHMARKS   ✅|❌|EXEMPT
 8. DOCS         ✅|❌
 9. CANARY E2E   ✅|❌|EXEMPT
10. PARITY       ✅|❌
11. REVIEW SWARM ✅|❌ (manual)
12. ATOMIC COMMIT ⏳ (ready when all above green)

Result: N/12 gates green. Ready to admit: YES|NO
```
