#!/bin/bash
# Smoke test runner for e2e-overnight workflows
# Usage: ./tests/e2e-overnight/run-smoke.sh [nika-binary-path]

set -euo pipefail

NIKA="${1:-./tools/target/debug/nika}"
DIR="tests/e2e-overnight"
PASS=0
FAIL=0
SKIP=0
ERRORS=""

green() { printf "\033[32m%s\033[0m\n" "$1"; }
red() { printf "\033[31m%s\033[0m\n" "$1"; }
yellow() { printf "\033[33m%s\033[0m\n" "$1"; }

run_expect_success() {
  local file="$1"
  local name=$(basename "$file" .nika.yaml)
  local output
  output=$("$NIKA" run "$file" --no-live 2>&1) && rc=0 || rc=$?
  if [ $rc -eq 0 ]; then
    green "  PASS: $name"
    PASS=$((PASS + 1))
  else
    red "  FAIL: $name (exit $rc)"
    ERRORS="$ERRORS\n  $name: $(echo "$output" | grep -i 'NIKA-\|error' | head -1)"
    FAIL=$((FAIL + 1))
  fi
}

run_expect_fail() {
  local file="$1"
  local name=$(basename "$file" .nika.yaml)
  local output
  output=$("$NIKA" run "$file" --no-live 2>&1) && rc=0 || rc=$?
  if [ $rc -ne 0 ]; then
    green "  PASS: $name (correctly failed)"
    PASS=$((PASS + 1))
  else
    red "  SECURITY BUG: $name (should have failed but succeeded!)"
    ERRORS="$ERRORS\n  SECURITY: $name passed but should fail"
    FAIL=$((FAIL + 1))
  fi
}

run_if_feature() {
  local feature="$1"
  local file="$2"
  local name=$(basename "$file" .nika.yaml)
  if "$NIKA" features 2>/dev/null | grep -q "✓ $feature"; then
    run_expect_success "$file"
  else
    yellow "  SKIP: $name (needs $feature)"
    SKIP=$((SKIP + 1))
  fi
}

echo "============================================"
echo "  Nika E2E Overnight Smoke Tests"
echo "  Binary: $NIKA"
echo "  $("$NIKA" --version 2>/dev/null || echo 'version unknown')"
echo "============================================"
echo

echo "--- Security (G*) — must ALL fail ---"
for f in "$DIR"/G*.nika.yaml; do
  run_expect_fail "$f"
done
echo

echo "--- Exec + Invoke (E*) ---"
for f in "$DIR"/E01*.nika.yaml "$DIR"/E02*.nika.yaml "$DIR"/E04*.nika.yaml "$DIR"/E05*.nika.yaml "$DIR"/E06*.nika.yaml; do
  [ -f "$f" ] && run_expect_success "$f"
done
# E08 timeout is an expected failure (sleep 10 with timeout 2)
for f in "$DIR"/E08*.nika.yaml; do
  [ -f "$f" ] && run_expect_fail "$f"
done
echo

echo "--- DAG + for_each (D*) ---"
for f in "$DIR"/D*.nika.yaml; do
  name=$(basename "$f" .nika.yaml)
  if [ "$name" = "D04-foreach-failfast-false" ]; then
    # D04 has a deliberately failing item — workflow exits non-zero but items survive
    run_expect_fail "$f"
  else
    run_expect_success "$f"
  fi
done
echo

echo "--- Stress (S*) ---"
for f in "$DIR"/S*.nika.yaml; do
  run_expect_success "$f"
done
echo

echo "--- Media (F*) ---"
for f in "$DIR"/F01*.nika.yaml "$DIR"/F05*.nika.yaml "$DIR"/F06*.nika.yaml "$DIR"/F10*.nika.yaml; do
  [ -f "$f" ] && run_expect_success "$f"
done
# Thumbnail needs media-thumbnail feature
for f in "$DIR"/F02*.nika.yaml; do
  [ -f "$f" ] && run_if_feature "media-thumbnail" "$f"
done
echo

echo "--- Fetch (C*) ---"
for f in "$DIR"/C*.nika.yaml; do
  run_expect_success "$f"
done
echo

echo "--- Artifacts (R*) ---"
for f in "$DIR"/R03*.nika.yaml "$DIR"/D07*.nika.yaml; do
  [ -f "$f" ] && run_expect_success "$f"
done

echo
echo "--- Verification (V*) ---"
for f in "$DIR"/V02*.nika.yaml; do
  [ -f "$f" ] && run_expect_fail "$f"  # V02 timeout is expected failure
done
for f in "$DIR"/V04*.nika.yaml; do
  [ -f "$f" ] && run_expect_success "$f"
done
echo

echo "--- Artifact verification ---"
if [ -d "$DIR/output" ]; then
  echo "  Files in output/:"
  ls -la "$DIR/output/" 2>/dev/null | tail -10
  empty=$(find "$DIR/output/" -type f -empty 2>/dev/null | wc -l)
  [ "$empty" -gt 0 ] && red "  WARNING: $empty empty files!" || green "  All files non-empty"
else
  yellow "  No output directory (no artifact workflows ran)"
fi
echo

echo "============================================"
echo "  Results: $PASS passed, $FAIL failed, $SKIP skipped"
echo "============================================"
if [ -n "$ERRORS" ]; then
  echo
  red "Errors:"
  printf "$ERRORS\n"
fi

exit $FAIL
