#!/usr/bin/env bash
# COVERS: scripts/hygiene/check-cargo-audit.sh
# Synthetic cargo-audit decision table, never a real vulnerability scan.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
scratch="$(mktemp -d)"
trap 'rm -r "$scratch"' EXIT
mkdir -p "$scratch/scripts/hygiene" "$scratch/fuzz" \
  "$scratch/crates/future" "$scratch/bin"
cp "$here/../check-cargo-audit.sh" "$scratch/scripts/hygiene/"
touch "$scratch/Cargo.lock" "$scratch/fuzz/Cargo.lock" \
  "$scratch/crates/future/Cargo.lock"
cat >"$scratch/bin/cargo" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
file=Cargo.lock
deny=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --file) file="$2"; shift 2 ;;
    --deny) deny="$2"; shift 2 ;;
    *) shift ;;
  esac
done
printf '%s\n' "$file" >>"$AUDIT_LOG"
[ -f "$file" ] || { echo 'missing lockfile' >&2; exit 1; }
if [ "$file" = "$FINDING_LOCK" ]; then
  case "$FINDING_KIND" in
    vulnerability|tool-error) echo 'synthetic refusal' >&2; exit 1 ;;
    unsound)
      echo 'synthetic unsound warning' >&2
      [ "$deny" != unsound ] || exit 1
      ;;
  esac
fi
SH
cp "$scratch/bin/cargo" "$scratch/bin/cargo-audit"
chmod +x "$scratch/bin/cargo" "$scratch/bin/cargo-audit"
export PATH="$scratch/bin:$PATH" AUDIT_LOG="$scratch/calls"
cases=0
failed=0
expect() {
  local expected="$1" label="$2" status=0
  : >"$AUDIT_LOG"
  (cd "$scratch" && bash scripts/hygiene/check-cargo-audit.sh) \
    >"$scratch/out" 2>&1 || status=$?
  cases=$((cases + 1))
  if [ "$status" -eq "$expected" ]; then
    printf 'ok %s\n' "$label"
  else
    printf 'FAIL %s: expected %s, got %s\n' "$label" "$expected" "$status" >&2
    failed=$((failed + 1))
  fi
}
export FINDING_LOCK="" FINDING_KIND=""
expect 0 'clean three-family scan'
if [ "$(sort -u "$AUDIT_LOG" | wc -l | tr -d ' ')" != 3 ]; then
  echo 'FAIL the scan does not cover all lock families' >&2
  failed=$((failed + 1))
fi
for FINDING_LOCK in Cargo.lock fuzz/Cargo.lock crates/future/Cargo.lock; do
  for FINDING_KIND in vulnerability unsound tool-error; do
    expect 2 "$FINDING_LOCK refuses $FINDING_KIND"
  done
done
FINDING_LOCK=""
rm "$scratch/fuzz/Cargo.lock"
expect 2 'missing mandatory fuzz lock refuses'
printf '%s cases, %s failures\n' "$cases" "$failed"
[ "$failed" -eq 0 ]
