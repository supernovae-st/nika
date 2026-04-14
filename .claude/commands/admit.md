# Admit — guided 12-gate crate admission

Walks through the 12 gates sequentially for a candidate crate and
helps draft the atomic admission commit. Stops on the first failure.

Usage: `/admit nika-foo`

## Argument

$ARGUMENTS = candidate crate name (e.g. `nika-foo`). Must already
exist on disk under `tools/$ARGUMENTS/` with at minimum a `Cargo.toml`
and `src/lib.rs`, but **must not yet** be in `[workspace] members`.

## Pre-flight (run first, abort if any fail)

```bash
CRATE="$ARGUMENTS"

test -d "tools/$CRATE" || { echo "❌ tools/$CRATE not found"; exit 1; }
grep -q "\"tools/$CRATE\"" Cargo.toml && { echo "❌ already in workspace"; exit 1; }
test -f "docs/crate-specs/$CRATE.md" || { echo "❌ Gate 1 fail: missing spec"; exit 1; }
git diff --quiet || { echo "❌ working tree dirty — commit or stash first"; exit 1; }
```

## Gates in order — stop on first ❌

```bash
echo "── Gate 1: SPEC ──"
test -f "docs/crate-specs/$CRATE.md" && echo "✅" || exit 1

echo "── Gate 2: TDD (manual review) ──"
echo "Inspect git log for $CRATE — were #[test] commits BEFORE impl commits?"
git log --oneline --reverse -- "tools/$CRATE/" | head -10

echo "── Gate 3: IMPL compiles ──"
# Temporarily add to workspace for the rest of the gates
sed -i.bak "s|members  = \[|members  = [\"tools/$CRATE\", |" Cargo.toml
cargo check -p "$CRATE" 2>&1 | tail -3 || { mv Cargo.toml.bak Cargo.toml; exit 1; }

echo "── Gate 3b: TESTS PASS ──"
cargo test -p "$CRATE" --lib 2>&1 | tail -5 || { mv Cargo.toml.bak Cargo.toml; exit 1; }

echo "── Gate 4: CLIPPY 0 ──"
cargo clippy -p "$CRATE" --all-targets -- -D warnings 2>&1 | tail -5 \
  || { mv Cargo.toml.bak Cargo.toml; exit 1; }

echo "── Gate 4b: ZERO UNWRAP IN src/ ──"
UNWRAPS=$(rg '\.unwrap\(\)|\.expect\(' "tools/$CRATE/src" --type rust -g '!*test*' | wc -l | tr -d ' ')
[ "$UNWRAPS" = "0" ] && echo "✅ 0 unwraps" \
  || { echo "❌ $UNWRAPS unwrap/expect in src/"; mv Cargo.toml.bak Cargo.toml; exit 1; }

echo "── Gate 4c: ZERO #[allow(dead_code)] ──"
DEAD=$(rg '#\[allow\(dead_code\)\]' "tools/$CRATE/src" --type rust | wc -l | tr -d ' ')
[ "$DEAD" = "0" ] && echo "✅ 0 dead_code allows" \
  || { echo "❌ $DEAD dead_code allows"; mv Cargo.toml.bak Cargo.toml; exit 1; }

echo "── Gate 4d: FILE SIZE ≤1500 LOC ──"
OVER=$(find "tools/$CRATE/src" -name '*.rs' | xargs wc -l 2>/dev/null \
       | awk '$1 > 1500 && $2 != "total" {print}')
[ -z "$OVER" ] && echo "✅ all files ≤1500 LOC" \
  || { echo "❌ files over 1500 LOC: $OVER"; mv Cargo.toml.bak Cargo.toml; exit 1; }

echo "── Gate 5: MUTATION ≥90% (manual) ──"
echo "Run: cargo mutants -p $CRATE --timeout 300"
echo "Must report ≥90% killed before continuing."

echo "── Gate 6: PROPERTY (manual) ──"
grep -rl proptest "tools/$CRATE/" 2>/dev/null && echo "✅ proptest present" \
  || echo "⚠️  no proptest — exempt only if non-security-sensitive (justify in spec)"

echo "── Gate 7: BENCHMARKS (manual) ──"
test -d "tools/$CRATE/benches" && echo "✅ benches/ present" \
  || echo "⚠️  no benches — exempt only if not hot-path (justify in spec)"

echo "── Gate 8: DOCS — 0 cargo doc warnings ──"
cargo doc --no-deps -p "$CRATE" 2>&1 | grep -i warning && exit 1 || echo "✅"

echo "── Gate 9: CANARY E2E (manual) ──"
test -f "tests/canary-$CRATE.nika.yaml" && echo "✅ canary exists" \
  || echo "⚠️  no canary — exempt only for L0/L0.5 (justify in spec)"

echo "── Gate 10: PARITY vs legacy main (manual) ──"
echo "Compare: git show main:tools/$CRATE/... vs current"
echo "Run any golden parity test in the suite."

echo "── Gate 11: REVIEW SWARM (manual) ──"
echo "Dispatch in parallel:"
echo "  - spn-nika:code-reviewer"
echo "  - spn-rust:rust-pro"
echo "  - feature-dev:code-reviewer"
echo "All P0/P1 findings must be fixed in this same session."

echo "── ADR coverage ──"
bash scripts/ci/check-adr-coverage.sh
echo "If $CRATE is not yet covered, write or extend an ADR before commit."

# Cleanup the temp workspace edit — final commit must be a clean
# single insertion, not an sed-bak-revert noise diff.
mv Cargo.toml.bak Cargo.toml
echo "── Workspace edit reverted. Re-add tools/$CRATE to members manually for the commit. ──"
```

## Gate 12 — atomic commit

After all 11 gates green:

1. Edit `Cargo.toml` `members = […]` — add `"tools/$CRATE"` (one line)
2. Stage exactly: `git add Cargo.toml tools/$CRATE/`
3. Verify staged scope: `git diff --cached --stat`
4. Commit with the canonical body (per `.claude/rules/commit-granularity.md`):

```
feat($CRATE): admit to workspace — all 12 gates passed

Gate 1  SPEC     ✅  docs/crate-specs/$CRATE.md
Gate 2  TDD      ✅  RED before GREEN confirmed
Gate 3  IMPL     ✅  <N> LOC, compiles, tests pass
Gate 4  CLIPPY   ✅  0 warnings
Gate 5  MUTATION ✅  <N>% killed
Gate 6  PROPERTY ✅ / N/A (justified in spec)
Gate 7  BENCHMARKS ✅ / N/A (justified)
Gate 8  DOCS     ✅  0 cargo doc warnings
Gate 9  CANARY   ✅ / N/A (justified)
Gate 10 PARITY   ✅  golden test vs legacy
Gate 11 REVIEW   ✅  3-agent swarm, P0/P1 fixed
Gate 12 ATOMIC   ✅  this commit

LOC: <N> src | Tests: <N> | Mutation: <N>%

Co-Authored-By: Nika 🦋 <nika@supernovae.studio>
```

5. Verify post-commit: `git log --oneline -1 --stat`
6. Run hygiene one final time: `cargo test --workspace --lib && cargo clippy --workspace -- -D warnings`
7. **Do not push** — wait for explicit user GO.

## See also

- [docs/golden-commits.md](../../docs/golden-commits.md) — exemplary admission commit SHAs to imitate
- [.claude/skills/crate-admit/](../skills/crate-admit/) — the multi-step skill version
- [/diamond-gates](diamond-gates.md) — gate-status read-only check (no Cargo.toml mutation)
