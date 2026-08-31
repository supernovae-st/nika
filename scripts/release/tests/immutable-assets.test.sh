#!/usr/bin/env bash
# Regression proof: a release replay may heal absence, never replace bytes.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
TEST_ROOT="$(mktemp -d)"
REMOTE="$TEST_ROOT/remote"
LOCAL="$TEST_ROOT/local"
FAKE_BIN="$TEST_ROOT/bin"
LOG="$TEST_ROOT/gh.log"
mkdir -p "$REMOTE" "$LOCAL" "$FAKE_BIN"
trap 'rm -r "$TEST_ROOT"' EXIT

fail() {
  echo "immutable-assets.test: $1" >&2
  exit 1
}

cat >"$FAKE_BIN/gh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
case "$1 $2" in
  "api repos/supernovae-st/nika/releases/tags/v9.9.9")
    find "$REMOTE" -type f -maxdepth 1 -exec basename {} \; | sort
    ;;
  "release download")
    pattern=""
    destination=""
    shift 2
    while [ "$#" -gt 0 ]; do
      case "$1" in
        --pattern) pattern="$2"; shift 2 ;;
        --dir) destination="$2"; shift 2 ;;
        *) shift ;;
      esac
    done
    cp "$REMOTE/$pattern" "$destination/$pattern"
    ;;
  "release upload")
    asset="${@: -1}"
    cp "$asset" "$REMOTE/$(basename "$asset")"
    echo "upload $(basename "$asset")" >>"$LOG"
    ;;
  *)
    echo "unexpected gh invocation: $*" >&2
    exit 90
    ;;
esac
EOF
chmod +x "$FAKE_BIN/gh"

printf 'alpha\n' >"$LOCAL/new.tgz"
PATH="$FAKE_BIN:$PATH" REMOTE="$REMOTE" LOG="$LOG" \
  bash "$ROOT/scripts/release/upload-assets-immutable.sh" \
  v9.9.9 supernovae-st/nika "$LOCAL/new.tgz" >/dev/null
cmp -s "$LOCAL/new.tgz" "$REMOTE/new.tgz" \
  || fail 'a missing asset was not uploaded'
[ "$(grep -c '^upload new.tgz$' "$LOG")" -eq 1 ] \
  || fail 'a missing asset was not uploaded exactly once'

PATH="$FAKE_BIN:$PATH" REMOTE="$REMOTE" LOG="$LOG" \
  bash "$ROOT/scripts/release/upload-assets-immutable.sh" \
  v9.9.9 supernovae-st/nika "$LOCAL/new.tgz" >/dev/null
[ "$(grep -c '^upload new.tgz$' "$LOG")" -eq 1 ] \
  || fail 'an identical replay uploaded the asset again'

printf 'different\n' >"$LOCAL/new.tgz"
if PATH="$FAKE_BIN:$PATH" REMOTE="$REMOTE" LOG="$LOG" \
  bash "$ROOT/scripts/release/upload-assets-immutable.sh" \
  v9.9.9 supernovae-st/nika "$LOCAL/new.tgz" \
  >"$TEST_ROOT/refusal.out" 2>&1; then
  fail 'different bytes replaced an occupied asset name'
fi
grep -q 'REFUSED different bytes' "$TEST_ROOT/refusal.out" \
  || fail 'the mismatch refusal did not name the occupied asset'
printf 'alpha\n' | cmp -s - "$REMOTE/new.tgz" \
  || fail 'a refused replay mutated the remote asset'

printf 'occupied\n' >"$REMOTE/z-existing.tgz"
printf 'different\n' >"$LOCAL/z-existing.tgz"
printf 'missing\n' >"$LOCAL/a-missing.tgz"
if PATH="$FAKE_BIN:$PATH" REMOTE="$REMOTE" LOG="$LOG" \
  bash "$ROOT/scripts/release/upload-assets-immutable.sh" \
  v9.9.9 supernovae-st/nika \
  "$LOCAL/a-missing.tgz" "$LOCAL/z-existing.tgz" \
  >"$TEST_ROOT/two-pass.out" 2>&1; then
  fail 'a divergent occupied set unexpectedly succeeded'
fi
[ ! -e "$REMOTE/a-missing.tgz" ] \
  || fail 'a missing asset was uploaded before the occupied set was validated'

if PATH="$FAKE_BIN:$PATH" REMOTE="$REMOTE" LOG="$LOG" \
  bash "$ROOT/scripts/release/upload-assets-immutable.sh" \
  latest supernovae-st/nika "$LOCAL/new.tgz" >/dev/null 2>&1; then
  fail 'a non-semver tag reached the release API'
fi

echo 'immutable-assets.test: PASS'
