# Diamond Health — workspace numbers dashboard

Run these checks and report a clean table comparing actuals vs targets.
The canonical parity-enforced state block comes from
`bash scripts/refresh-status.sh` (hygiene vector 23) — run it first;
the checks below are the raw drill-down.

## Checks to run

```bash
# 1. Crates in workspace
grep -c '"crates/' Cargo.toml 2>/dev/null || echo 0

# 2. Total LOC per crate (src/ only, no tests)
for d in crates/*/src; do
  crate=$(basename $(dirname "$d"))
  loc=$(find "$d" -name '*.rs' -not -path '*/tests/*' 2>/dev/null | xargs wc -l 2>/dev/null | tail -1 | awk '{print $1}')
  echo "$crate: ${loc:-0} LOC"
done

# 3. Files >1500 LOC
find crates -name '*.rs' -not -path '*/target/*' | xargs wc -l 2>/dev/null | awk '$1 > 1500 && $2 !~ /total/ {print "⚠️", $1, $2}'

# 4. Unwrap count in src/ (prod only, skip tests)
rg '\.unwrap\(\)' crates/*/src --type rust -g '!*test*' 2>/dev/null | wc -l

# 5. Clippy status
cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -3

# 6. Test count
cargo test --workspace --lib -- --list 2>&1 | grep -c ': test$' || echo "0"

# 7. Largest file
find crates -name '*.rs' -not -path '*/target/*' | xargs wc -l 2>/dev/null | sort -rn | head -3
```

## Report format

```
💎 DIAMOND HEALTH — <date>

Crates admitted:  N / 42 target (cap 100 · reached additively across the 1.x minors)
Workspace LOC:    N (caps: ≤15k prod LOC/crate · ≤1500 LOC/file — no workspace-total target)
Files >1500:      N (target 0)
Unwraps src/:     N (target 0)
Clippy:           ✅ 0 warnings | ❌ N warnings
Tests:            N passing
Largest file:     <path> (N LOC)

Per-crate LOC:
  nika-X:      N LOC (budget per docs/crate-specs/nika-X.md)
```
