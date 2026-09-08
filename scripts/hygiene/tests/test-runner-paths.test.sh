#!/usr/bin/env bash
# COVERS: scripts/hygiene/check-kit-script-tests.sh scripts/hygiene/check-hygiene-self-tests.sh
# A checkout path is one argument, including whitespace and glob characters.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
TEMP_ROOT="$(mktemp -d)"
trap 'rm -rf "$TEMP_ROOT"' EXIT
fixture="$TEMP_ROOT/checkout with [spaces]"
mkdir -p "$fixture/scripts/hygiene/tests" "$fixture/.agents/plugins/nika/scripts/tests"
for name in check-kit-script-tests.sh check-hygiene-self-tests.sh; do
  cp "$ROOT/scripts/hygiene/$name" "$fixture/scripts/hygiene/$name"
done
printf '%s\n' 'check-kit-script-tests.sh' 'check-hygiene-self-tests.sh' >"$fixture/scripts/hygiene/check-all.sh"
for dir in "$fixture/scripts/hygiene/tests" "$fixture/.agents/plugins/nika/scripts/tests"; do
  mkdir -p "$dir/nested folder"
  cat >"$dir/nested folder/has space.test.sh" <<'CHILD'
#!/usr/bin/env bash
# COVERS: scripts/hygiene/check-kit-script-tests.sh scripts/hygiene/check-hygiene-self-tests.sh
exit 0
CHILD
done
for name in check-kit-script-tests.sh check-hygiene-self-tests.sh; do
  bash "$fixture/scripts/hygiene/$name" >"$TEMP_ROOT/green" 2>&1
  grep -q 'OK:' "$TEMP_ROOT/green"
done
# Negative control: a failing child must still be discovered and make RED.
for dir in "$fixture/scripts/hygiene/tests" "$fixture/.agents/plugins/nika/scripts/tests"; do
  printf '%s\n' '#!/usr/bin/env bash' 'exit 7' >"$dir/nested folder/has space.test.sh"
done
for name in check-kit-script-tests.sh check-hygiene-self-tests.sh; do
  status=0
  bash "$fixture/scripts/hygiene/$name" >"$TEMP_ROOT/red" 2>&1 || status=$?
  [ "$status" -eq 2 ] || {
    cat "$TEMP_ROOT/red"
    exit 1
  }
  grep -q 'FAIL has space.test.sh' "$TEMP_ROOT/red"
done
printf 'OK: both discovery paths preserve filenames and propagate a failing child\n'
