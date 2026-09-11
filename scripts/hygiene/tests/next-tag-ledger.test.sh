#!/usr/bin/env bash
# COVERS: scripts/ci/next-tag-project.sh
# A YAML comment documents a proof; it is not part of the CI job key.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
FIXTURE="$(mktemp -d)"
trap 'rm -rf "$FIXTURE"' EXIT
mkdir -p "$FIXTURE/.github/workflows"
printf '[workspace]\n' >"$FIXTURE/Cargo.toml"
printf 'jobs:\n  rust:\n    runs-on: ubuntu-latest\n' >"$FIXTURE/.github/workflows/proof.yml"

expect() {
  local want="$1" value="$2" label="$3" status=0
  printf 'capabilities:\n  - id: example\n    # proven_by: absent\n    proven_by: %s\n' \
    "$value" >"$FIXTURE/wiring.yaml"
  bash "$ROOT/scripts/ci/next-tag-project.sh" --repo "$FIXTURE" --check \
    >"$FIXTURE/verdict" 2>&1 || status=$?
  if [ "$status" -ne "$want" ]; then
    printf 'FAIL: %s expected %s, got %s\n' "$label" "$want" "$status" >&2
    cat "$FIXTURE/verdict" >&2
    exit 1
  fi
  printf 'ok: %s\n' "$label"
}

expect 0 'rust' 'plain job'
expect 0 'rust   # path/to/proof.rs' 'inline comment'
expect 0 'rust   ' 'trailing whitespace'
expect 0 '"rust" # scoped proof' 'double quoted job'
expect 0 "'rust' # scoped proof" 'single quoted job'
expect 1 'absent # rust exists' 'unknown job stays refused'
expect 1 'null # awaiting proof' 'null stays unproven'
expect 1 '# awaiting proof' 'comment alone supplies no proof'
expect 1 'rust#absent' 'hash without separation is data'
expect 1 '"rust # absent"' 'quoted hash is data'
expect 1 "'rust # absent'" 'single quoted hash is data'
expect 1 '"rust" garbage' 'suffix cannot be discarded'
expect 1 '"rust # no closing quote' 'unclosed quote stays refused'

expect 1 $'"rust\t"' 'quoted trailing tab cannot collapse to a live job'
expect 1 $'"\trust"' 'quoted leading tab cannot collapse to a live job'
expect 0 $'rust\t# scoped proof' 'tab separates a real comment'

printf 'next-tag-ledger.test: PASS\n'
