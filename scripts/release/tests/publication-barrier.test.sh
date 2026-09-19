#!/usr/bin/env bash
# Fake-CLI decision table for the future-only cross-registry visibility barrier.
set -euo pipefail

unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_PREFIX GIT_COMMON_DIR \
  GIT_OBJECT_DIRECTORY GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_NAMESPACE \
  GIT_QUARANTINE_PATH

CALLER_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || true)"
CALLER_HEAD=""
CALLER_STATUS=""
if [ -n "$CALLER_ROOT" ]; then
  CALLER_HEAD="$(git -C "$CALLER_ROOT" rev-parse HEAD)"
  CALLER_STATUS="$(git -C "$CALLER_ROOT" status --porcelain)"
fi

verify_caller_untouched() {
  [ -z "$CALLER_ROOT" ] && return 0
  [ "$(git -C "$CALLER_ROOT" rev-parse HEAD)" = "$CALLER_HEAD" ] \
    || {
      echo 'publication-barrier.test: caller HEAD moved' >&2
      exit 1
    }
  [ "$(git -C "$CALLER_ROOT" status --porcelain)" = "$CALLER_STATUS" ] \
    || {
      echo 'publication-barrier.test: caller worktree changed' >&2
      exit 1
    }
}

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
TEST_ROOT="$(mktemp -d)"
trap 'rm -r "$TEST_ROOT"; verify_caller_untouched' EXIT

fail() {
  echo "publication-barrier.test: $1" >&2
  exit 1
}

VERSION=9.9.9
TAG="v$VERSION"
RELEASE_ID=123
RELEASE_SHA=2222222222222222222222222222222222222222
LOCAL="$TEST_ROOT/assets"
REMOTE="$TEST_ROOT/remote"
BIN="$TEST_ROOT/bin"
LOG="$TEST_ROOT/log"
TAG_MOVED_FILE="$TEST_ROOT/tag-moved"
mkdir -p "$LOCAL" "$REMOTE" "$BIN"
: >"$LOG"
# macOS has shasum only. The scripts under test call GNU sha256sum (Linux CI).
if ! command -v sha256sum >/dev/null 2>&1; then
  cat >"$BIN/sha256sum" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [ "${1:-}" = -c ]; then
  shift
  exec shasum -a 256 -c "$@"
fi
exec shasum -a 256 "$@"
EOF
  chmod +x "$BIN/sha256sum"
fi
export RELEASE_SHA TAG TAG_MOVED_FILE

names=(
  "nika-macos-arm64-${VERSION}.tar.gz"
  "nika-macos-x64-${VERSION}.tar.gz"
  "nika-linux-arm64-${VERSION}.tar.gz"
  "nika-linux-x64-${VERSION}.tar.gz"
  SHA256SUMS
  multiple.intoto.jsonl
  "supernovae-st-nika-check-wasm-${VERSION}.tgz"
  "supernovae-st-nika-check-wasm-${VERSION}.tgz.sha256"
)
assets=()
for name in "${names[@]}"; do
  printf 'bytes:%s\n' "$name" >"$LOCAL/$name"
  assets+=("$LOCAL/$name")
done

cat >"$BIN/git" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [ "$1" = ls-remote ]; then
  tag="${4#refs/tags/}"
  sha="$RELEASE_SHA"
  [ ! -e "$TAG_MOVED_FILE" ] || sha=3333333333333333333333333333333333333333
  printf '%s\trefs/tags/%s\n' 1111111111111111111111111111111111111111 "$tag"
  printf '%s\trefs/tags/%s^{}\n' "$sha" "$tag"
  exit 0
fi
exit 90
EOF

cat >"$BIN/gh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
asset_id() {
  printf '%s' "$1" | cksum | awk '{ print $1 }'
}
[ "$1" = api ] || { echo "unexpected gh: $*" >&2; exit 90; }
endpoint="$2"
if [ "${GH_LOOKUP:-ok}" = unknown ]; then
  echo 'gh: timeout' >&2
  exit 1
fi
if [ "$endpoint" = 'repos/supernovae-st/nika/releases/123' ]; then
  if printf '%s\n' "$*" | grep -Fq 'upload_url'; then
    printf 'https://uploads.github.com/repos/supernovae-st/nika/releases/123/assets{?name,label}\n'
  else
    printf '123\t%s\ttrue\tfalse\n' "$TAG"
  fi
  exit 0
fi
if [ "$endpoint" = 'repos/supernovae-st/nika/releases/123/assets' ]; then
  while IFS= read -r name; do
    [ -n "$name" ] && printf '%s\t%s\n' "$(asset_id "$name")" "$name"
  done < <(find "$REMOTE" -maxdepth 1 -type f -exec basename {} \; | LC_ALL=C sort)
  if [ "${GH_DUPLICATE:-0}" = 1 ]; then
    printf '%s\t%s\n' "$(asset_id "$GH_DUPLICATE_NAME")" "$GH_DUPLICATE_NAME"
  fi
  [ "${MOVE_AFTER_CENSUS:-0}" != 1 ] || touch "$TAG_MOVED_FILE"
  exit 0
fi
case "$endpoint" in
  repos/supernovae-st/nika/releases/assets/*)
    wanted="${endpoint##*/}"
    while IFS= read -r name; do
      if [ "$(asset_id "$name")" = "$wanted" ]; then
        cat "$REMOTE/$name"
        exit 0
      fi
    done < <(find "$REMOTE" -maxdepth 1 -type f -exec basename {} \; | LC_ALL=C sort)
    exit 1
    ;;
esac
echo "unexpected gh api: $*" >&2
exit 90
EOF

cat >"$BIN/curl" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
asset=""
url="${*: -1}"
while [ "$#" -gt 0 ]; do
  case "$1" in
    --data-binary) asset="${2#@}"; shift 2 ;;
    *) shift ;;
  esac
done
name="${url##*name=}"
cp "$asset" "$REMOTE/$name"
printf 'upload %s\n' "$name" >>"$LOG"
[ "${GH_COMMIT_THEN_FAIL:-}" != "$name" ] || exit 22
EOF
chmod +x "$BIN/gh" "$BIN/git" "$BIN/curl"

# First publication writes each exact name once; identical replay writes none.
GH_TOKEN=test PATH="$BIN:$PATH" REMOTE="$REMOTE" LOG="$LOG" \
  bash "$ROOT/scripts/release/release-assets-barrier.sh" \
  stage supernovae-st/nika "$RELEASE_ID" "$TAG" "$RELEASE_SHA" \
  "${assets[@]}" >/dev/null
[ "$(wc -l <"$LOG" | tr -d ' ')" = 8 ] || fail 'first publish did not upload eight assets'
GH_TOKEN=test PATH="$BIN:$PATH" REMOTE="$REMOTE" LOG="$LOG" \
  bash "$ROOT/scripts/release/release-assets-barrier.sh" \
  stage supernovae-st/nika "$RELEASE_ID" "$TAG" "$RELEASE_SHA" \
  "${assets[@]}" >/dev/null
[ "$(wc -l <"$LOG" | tr -d ' ')" = 8 ] || fail 'identical replay uploaded again'

# Every individual missing asset is healable after all occupied identities pass.
for name in "${names[@]}"; do
  rm "$REMOTE/$name"
  GH_TOKEN=test PATH="$BIN:$PATH" REMOTE="$REMOTE" LOG="$LOG" \
    bash "$ROOT/scripts/release/release-assets-barrier.sh" \
    stage supernovae-st/nika "$RELEASE_ID" "$TAG" "$RELEASE_SHA" \
    "${assets[@]}" >/dev/null
  cmp -s "$LOCAL/$name" "$REMOTE/$name" || fail "missing heal failed for $name"
done

# Divergence and extras fail before a missing identity is healed.
rm "$REMOTE/${names[0]}"
printf 'divergent\n' >"$REMOTE/${names[1]}"
if GH_TOKEN=test PATH="$BIN:$PATH" REMOTE="$REMOTE" LOG="$LOG" \
  bash "$ROOT/scripts/release/release-assets-barrier.sh" \
  stage supernovae-st/nika "$RELEASE_ID" "$TAG" "$RELEASE_SHA" \
  "${assets[@]}" >/dev/null 2>&1; then
  fail 'divergent occupied asset passed'
fi
[ ! -e "$REMOTE/${names[0]}" ] || fail 'missing asset healed before divergence refusal'
cp "$LOCAL/${names[1]}" "$REMOTE/${names[1]}"
printf 'extra\n' >"$REMOTE/extra.bin"
if GH_TOKEN=test PATH="$BIN:$PATH" REMOTE="$REMOTE" LOG="$LOG" \
  bash "$ROOT/scripts/release/release-assets-barrier.sh" \
  stage supernovae-st/nika "$RELEASE_ID" "$TAG" "$RELEASE_SHA" \
  "${assets[@]}" >/dev/null 2>&1; then
  fail 'extra public asset passed'
fi
rm "$REMOTE/extra.bin"
cp "$LOCAL/${names[0]}" "$REMOTE/${names[0]}"
if GH_DUPLICATE=1 GH_DUPLICATE_NAME="${names[0]}" PATH="$BIN:$PATH" \
  REMOTE="$REMOTE" LOG="$LOG" bash "$ROOT/scripts/release/release-assets-barrier.sh" \
  verify supernovae-st/nika "$RELEASE_ID" "$TAG" "$RELEASE_SHA" \
  "${assets[@]}" >/dev/null 2>&1; then
  fail 'duplicate public identity passed'
fi

# A same-tag concurrent publisher may commit then report failure; equality is
# re-queried and the public identity is still written only once.
rm "$REMOTE/${names[0]}"
before="$(grep -Fc "upload ${names[0]}" "$LOG" || true)"
GH_COMMIT_THEN_FAIL="${names[0]}" GH_TOKEN=test PATH="$BIN:$PATH" \
  REMOTE="$REMOTE" LOG="$LOG" \
  bash "$ROOT/scripts/release/release-assets-barrier.sh" \
  stage supernovae-st/nika "$RELEASE_ID" "$TAG" "$RELEASE_SHA" \
  "${assets[@]}" >/dev/null
after="$(grep -Fc "upload ${names[0]}" "$LOG" || true)"
[ "$((after - before))" = 1 ] || fail 'concurrent publish was attempted more than once'

# A tag move after the release-ID census but before the first upload refuses
# with zero writes. The missing asset remains missing on the original release.
rm "$REMOTE/${names[0]}"
rm -f "$TAG_MOVED_FILE"
before="$(wc -l <"$LOG" | tr -d ' ')"
if MOVE_AFTER_CENSUS=1 GH_TOKEN=test PATH="$BIN:$PATH" REMOTE="$REMOTE" \
  LOG="$LOG" bash "$ROOT/scripts/release/release-assets-barrier.sh" \
  stage supernovae-st/nika "$RELEASE_ID" "$TAG" "$RELEASE_SHA" \
  "${assets[@]}" >"$TEST_ROOT/moved-before-upload.out" 2>&1; then
  fail 'tag move before upload passed'
fi
after="$(wc -l <"$LOG" | tr -d ' ')"
[ "$after" = "$before" ] || fail 'tag move before upload performed a write'
[ ! -e "$REMOTE/${names[0]}" ] \
  || fail 'tag move before upload filled the original release asset'
rm -f "$TAG_MOVED_FILE"
cp "$LOCAL/${names[0]}" "$REMOTE/${names[0]}"

# Tag peeling and movement are judged before writes.
cat >"$BIN/git" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [ "$1" = ls-remote ]; then
  tag="${4#refs/tags/}"
  printf '%s\trefs/tags/%s\n' "${TAG_OBJECT:-1111111111111111111111111111111111111111}" "$tag"
  printf '%s\trefs/tags/%s^{}\n' "${TAG_SHA:-2222222222222222222222222222222222222222}" "$tag"
  exit 0
fi
exit 90
EOF
chmod +x "$BIN/git"
resolved="$(PATH="$BIN:$PATH" bash "$ROOT/scripts/release/resolve-release-tag.sh" \
  "$TAG" supernovae-st/nika)"
[ "$resolved" = 2222222222222222222222222222222222222222 ] || fail 'annotated tag was not peeled'
if TAG_SHA=3333333333333333333333333333333333333333 PATH="$BIN:$PATH" \
  bash "$ROOT/scripts/release/resolve-release-tag.sh" "$TAG" supernovae-st/nika \
  2222222222222222222222222222222222222222 >/dev/null 2>&1; then
  fail 'tag move passed'
fi

# Draft creation happens only after the release list answers EMPTY (the
# by-tag endpoint never sees a draft: v0.118.0 died on its 404); an unknown
# lookup cannot be converted into a create. Also the pre-barrier failure proof.
cat >"$BIN/gh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [ "$1" = api ]; then
  if [ "${RELEASE_LOOKUP:-404}" = unknown ]; then
    echo 'gh: upstream unavailable (HTTP 500)' >&2
    exit 1
  fi
  if [ "${RELEASE_LOOKUP:-404}" = mixed ]; then
    echo 'gh: upstream unavailable (HTTP 500); auth (HTTP 401) unauthorized; secondary route (HTTP 404)' >&2
    exit 1
  fi
  endpoint="$2"
  if [[ "$endpoint" == */releases ]]; then
    if [[ " $* " == *" --method POST "* ]]; then
      : >"$RELEASE_STATE"
      printf 'create\n' >>"$RELEASE_LOG"
      printf '123\n'
      exit 0
    fi
    # the list answers 200 with what it carries, drafts included; an empty
    # list is the only absence GitHub states for a tag
    [ ! -e "$RELEASE_STATE" ] || printf '123\n'
    exit 0
  fi
  if [ ! -e "$RELEASE_STATE" ]; then
    echo 'gh: Not Found (HTTP 404)' >&2
    exit 1
  fi
  if [[ "$endpoint" == */releases/123 ]]; then
    printf '123\tv9.9.9\ttrue\tfalse\n'
  else
    printf '123\n'
  fi
  exit 0
fi
exit 90
EOF
chmod +x "$BIN/gh"
RELEASE_STATE="$TEST_ROOT/release-state"
RELEASE_LOG="$TEST_ROOT/release-log"
NOTES="$TEST_ROOT/notes.md"
: >"$RELEASE_LOG"
printf 'notes\n' >"$NOTES"
PATH="$BIN:$PATH" RELEASE_STATE="$RELEASE_STATE" RELEASE_LOG="$RELEASE_LOG" \
  bash "$ROOT/scripts/release/prepare-draft-release.sh" "$TAG" \
  supernovae-st/nika "$NOTES" \
  2222222222222222222222222222222222222222 >/dev/null
[ "$(wc -l <"$RELEASE_LOG" | tr -d ' ')" = 1 ] || fail 'explicit 404 did not create one draft'
# target_commitish is creation routing metadata, not an existing release's
# identity. A release whose API target is the branch name must still bind by
# immutable release ID, exact tag/prerelease, and repeatedly resolved tag SHA.
[ -z "$(rg -n 'target_commitish' "$ROOT/scripts/release/read-release-state.sh" "$ROOT/scripts/release/resolve-release-tag.sh" || true)" ] \
  || fail 'existing release identity still trusts target_commitish'
PATH="$BIN:$PATH" RELEASE_STATE="$RELEASE_STATE" RELEASE_LOG="$RELEASE_LOG" \
  bash "$ROOT/scripts/release/prepare-draft-release.sh" "$TAG" \
  supernovae-st/nika "$NOTES" \
  2222222222222222222222222222222222222222 >"$TEST_ROOT/reused-release"
grep -Fqx 'id=123' "$TEST_ROOT/reused-release" \
  || fail 'target_commitish=main release was not reused by bound identity'
rm "$RELEASE_STATE"
if RELEASE_LOOKUP=unknown PATH="$BIN:$PATH" RELEASE_STATE="$RELEASE_STATE" \
  RELEASE_LOG="$RELEASE_LOG" bash "$ROOT/scripts/release/prepare-draft-release.sh" \
  "$TAG" supernovae-st/nika "$NOTES" \
  2222222222222222222222222222222222222222 >/dev/null 2>&1; then
  fail 'unknown release lookup created a draft'
fi
[ "$(wc -l <"$RELEASE_LOG" | tr -d ' ')" = 1 ] || fail 'unknown release lookup mutated state'
if RELEASE_LOOKUP=mixed PATH="$BIN:$PATH" RELEASE_STATE="$RELEASE_STATE" \
  RELEASE_LOG="$RELEASE_LOG" bash "$ROOT/scripts/release/prepare-draft-release.sh" \
  "$TAG" supernovae-st/nika "$NOTES" \
  2222222222222222222222222222222222222222 >/dev/null 2>&1; then
  fail 'mixed 500/404 release lookup created a draft'
fi
[ "$(wc -l <"$RELEASE_LOG" | tr -d ' ')" = 1 ] || fail 'mixed release lookup mutated state'

# npm: equal, divergent, explicit E404, unknown lookup, absent verification,
# successful first publish, and an error whose publish nevertheless committed.
NPM_TGZ="$TEST_ROOT/package.tgz"
NPM_SHA="$NPM_TGZ.sha256"
printf 'npm bytes\n' >"$NPM_TGZ"
(cd "$TEST_ROOT" && PATH="$BIN:$PATH" sha256sum package.tgz >package.tgz.sha256)
NPM_SRI="sha512-$(openssl dgst -sha512 -binary "$NPM_TGZ" | base64 | tr -d '\n')"
cat >"$BIN/npm" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
state="$(cat "$NPM_STATE")"
if [ "$1" = view ]; then
  case "$state" in
    equal|committed) printf '%s\n' "$NPM_SRI" ;;
    divergent) echo 'sha512-wrong' ;;
    lag|lag-divergent)
      count_file="$NPM_VIEW_COUNT"
      n=0
      [ ! -e "$count_file" ] || n="$(cat "$count_file")"
      n=$((n + 1))
      printf '%s\n' "$n" >"$count_file"
      if [ "$n" -ge "${NPM_VISIBLE_AFTER:-8}" ]; then
        [ "$state" = lag ] && printf '%s\n' "$NPM_SRI" || echo 'sha512-wrong'
      else
        echo 'npm ERR! code E404' >&2
        exit 1
      fi
      ;;
    absent) echo 'npm ERR! code E404' >&2; exit 1 ;;
    unknown) echo 'npm ERR! code E500' >&2; exit 1 ;;
    mixed) echo 'npm ERR! code E500; code E401 unauthorized; secondary npm ERR! code E404' >&2; exit 1 ;;
  esac
  exit 0
fi
if [ "$1" = publish ]; then
  printf 'publish\n' >>"$NPM_LOG"
  case "$state" in
    lag|lag-divergent) : ;; # visibility lag outlives the publish call
    *) printf 'committed\n' >"$NPM_STATE" ;;
  esac
  [ "${NPM_PUBLISH_ERROR:-0}" = 1 ] && exit 1
  exit 0
fi
exit 90
EOF
cat >"$BIN/sleep" <<'EOF'
#!/usr/bin/env bash
[ -z "${SLEEP_LOG:-}" ] || printf '%s\n' "$1" >>"$SLEEP_LOG"
exit 0
EOF
chmod +x "$BIN/npm" "$BIN/sleep"
NPM_STATE="$TEST_ROOT/npm-state"
NPM_LOG="$TEST_ROOT/npm-log"
NPM_VIEW_COUNT="$TEST_ROOT/npm-view-count"
SLEEP_LOG="$TEST_ROOT/sleep-log"
: >"$NPM_LOG"
printf 'equal\n' >"$NPM_STATE"
PATH="$BIN:$PATH" NPM_STATE="$NPM_STATE" NPM_LOG="$NPM_LOG" NPM_SRI="$NPM_SRI" \
  bash "$ROOT/scripts/release/npm-publish-immutable.sh" verify \
  '@supernovae-st/nika-check-wasm@9.9.9' "$NPM_TGZ" "$NPM_SHA" >/dev/null
printf 'divergent\n' >"$NPM_STATE"
if PATH="$BIN:$PATH" NPM_STATE="$NPM_STATE" NPM_LOG="$NPM_LOG" NPM_SRI="$NPM_SRI" \
  bash "$ROOT/scripts/release/npm-publish-immutable.sh" verify x "$NPM_TGZ" "$NPM_SHA" >/dev/null 2>&1; then
  fail 'npm divergent version passed'
fi
printf 'unknown\n' >"$NPM_STATE"
if PATH="$BIN:$PATH" NPM_STATE="$NPM_STATE" NPM_LOG="$NPM_LOG" NPM_SRI="$NPM_SRI" \
  bash "$ROOT/scripts/release/npm-publish-immutable.sh" publish x "$NPM_TGZ" "$NPM_SHA" >/dev/null 2>&1; then
  fail 'npm unknown lookup was treated as absence'
fi
printf 'absent\n' >"$NPM_STATE"
if PATH="$BIN:$PATH" NPM_STATE="$NPM_STATE" NPM_LOG="$NPM_LOG" NPM_SRI="$NPM_SRI" \
  bash "$ROOT/scripts/release/npm-publish-immutable.sh" verify x "$NPM_TGZ" "$NPM_SHA" >/dev/null 2>&1; then
  fail 'npm absent verification passed'
fi
PATH="$BIN:$PATH" NPM_STATE="$NPM_STATE" NPM_LOG="$NPM_LOG" NPM_SRI="$NPM_SRI" \
  ACTIONS_ID_TOKEN_REQUEST_URL=https://oidc.test ACTIONS_ID_TOKEN_REQUEST_TOKEN=test bash "$ROOT/scripts/release/npm-publish-immutable.sh" publish \
  x "$NPM_TGZ" "$NPM_SHA" >/dev/null
[ "$(wc -l <"$NPM_LOG" | tr -d ' ')" = 1 ] || fail 'npm first publish count differs'
printf 'absent\n' >"$NPM_STATE"
PATH="$BIN:$PATH" NPM_STATE="$NPM_STATE" NPM_LOG="$NPM_LOG" NPM_SRI="$NPM_SRI" \
  NPM_PUBLISH_ERROR=1 ACTIONS_ID_TOKEN_REQUEST_URL=https://oidc.test ACTIONS_ID_TOKEN_REQUEST_TOKEN=test \
  bash "$ROOT/scripts/release/npm-publish-immutable.sh" publish x \
  "$NPM_TGZ" "$NPM_SHA" >/dev/null
[ "$(wc -l <"$NPM_LOG" | tr -d ' ')" = 2 ] || fail 'npm committed publish error retried publish'
printf 'mixed\n' >"$NPM_STATE"
if PATH="$BIN:$PATH" NPM_STATE="$NPM_STATE" NPM_LOG="$NPM_LOG" NPM_SRI="$NPM_SRI" \
  ACTIONS_ID_TOKEN_REQUEST_URL=https://oidc.test ACTIONS_ID_TOKEN_REQUEST_TOKEN=test bash "$ROOT/scripts/release/npm-publish-immutable.sh" \
  publish x "$NPM_TGZ" "$NPM_SHA" >/dev/null 2>&1; then
  fail 'mixed npm 500/E404/unauthorized granted publish authority'
fi
[ "$(wc -l <"$NPM_LOG" | tr -d ' ')" = 2 ] || fail 'mixed npm lookup reached publish'

printf 'absent\n' >"$NPM_STATE"
if PATH="$BIN:$PATH" NPM_STATE="$NPM_STATE" NPM_LOG="$NPM_LOG" NPM_SRI="$NPM_SRI" \
  bash "$ROOT/scripts/release/npm-publish-immutable.sh" publish x "$NPM_TGZ" "$NPM_SHA" >/dev/null 2>&1; then
  fail 'an absent version published without GitHub OIDC'
fi
[ "$(wc -l <"$NPM_LOG" | tr -d ' ')" = 2 ] || fail 'the OIDC barrier reached npm publish'

# The 35396510947 race: the publish commits but the registry keeps answering
# E404 past the old ~52s window. A bounded readiness budget rides out the lag,
# still publishes exactly once, and never retries an occupied version.
printf 'lag\n' >"$NPM_STATE"
rm -f "$NPM_VIEW_COUNT"
: >"$SLEEP_LOG"
PATH="$BIN:$PATH" NPM_STATE="$NPM_STATE" NPM_LOG="$NPM_LOG" NPM_SRI="$NPM_SRI" \
  NPM_VIEW_COUNT="$NPM_VIEW_COUNT" NPM_VISIBLE_AFTER=8 NIKA_NPM_READINESS_SECONDS=120 \
  SLEEP_LOG="$SLEEP_LOG" \
  ACTIONS_ID_TOKEN_REQUEST_URL=https://oidc.test ACTIONS_ID_TOKEN_REQUEST_TOKEN=test \
  bash "$ROOT/scripts/release/npm-publish-immutable.sh" publish x \
  "$NPM_TGZ" "$NPM_SHA" >"$TEST_ROOT/npm-lag.out"
grep -Fq 'publish committed with exact SRI' "$TEST_ROOT/npm-lag.out" \
  || fail 'lagged visibility inside the readiness budget did not commit'
[ "$(wc -l <"$NPM_LOG" | tr -d ' ')" = 3 ] || fail 'lagged visibility published more than once'
[ "$(cat "$NPM_VIEW_COUNT")" = 8 ] || fail 'readiness stopped polling before the version was visible'
[ "$(wc -l <"$SLEEP_LOG" | tr -d ' ')" = 6 ] \
  || fail 'readiness polling did not sleep once per invisible cadence'
[ -z "$(grep -vFx '10' "$SLEEP_LOG" || true)" ] \
  || fail 'readiness polling slept a non-cadence interval'

# A version that never becomes visible fails 69 inside the bounded window and
# names the budget; the lookup count stays finite.
printf 'lag\n' >"$NPM_STATE"
rm -f "$NPM_VIEW_COUNT"
: >"$SLEEP_LOG"
rc=0
PATH="$BIN:$PATH" NPM_STATE="$NPM_STATE" NPM_LOG="$NPM_LOG" NPM_SRI="$NPM_SRI" \
  NPM_VIEW_COUNT="$NPM_VIEW_COUNT" NPM_VISIBLE_AFTER=99 NIKA_NPM_READINESS_SECONDS=20 \
  SLEEP_LOG="$SLEEP_LOG" \
  ACTIONS_ID_TOKEN_REQUEST_URL=https://oidc.test ACTIONS_ID_TOKEN_REQUEST_TOKEN=test \
  bash "$ROOT/scripts/release/npm-publish-immutable.sh" publish x \
  "$NPM_TGZ" "$NPM_SHA" >"$TEST_ROOT/npm-timeout.out" 2>&1 || rc=$?
[ "$rc" -eq 69 ] || fail 'never-visible publish did not fail 69 inside the budget'
grep -Fq 'never became visible within 20s' "$TEST_ROOT/npm-timeout.out" \
  || fail 'timeout refusal did not name the readiness budget'
[ "$(cat "$NPM_VIEW_COUNT")" = 3 ] || fail 'readiness polling was not bounded by the budget'
[ "$(cat "$SLEEP_LOG")" = 10 ] \
  || fail 'a two-attempt budget did not sleep exactly one cadence'

# A committed publish whose bytes diverge refuses on first sight, without
# burning the budget.
printf 'lag-divergent\n' >"$NPM_STATE"
rm -f "$NPM_VIEW_COUNT"
rc=0
PATH="$BIN:$PATH" NPM_STATE="$NPM_STATE" NPM_LOG="$NPM_LOG" NPM_SRI="$NPM_SRI" \
  NPM_VIEW_COUNT="$NPM_VIEW_COUNT" NPM_VISIBLE_AFTER=2 NIKA_NPM_READINESS_SECONDS=120 \
  ACTIONS_ID_TOKEN_REQUEST_URL=https://oidc.test ACTIONS_ID_TOKEN_REQUEST_TOKEN=test \
  bash "$ROOT/scripts/release/npm-publish-immutable.sh" publish x \
  "$NPM_TGZ" "$NPM_SHA" >"$TEST_ROOT/npm-divergent.out" 2>&1 || rc=$?
[ "$rc" -eq 73 ] || fail 'divergent committed publish was not refused'
grep -Fq 'REFUSED divergent committed publish' "$TEST_ROOT/npm-divergent.out" \
  || fail 'divergent committed publish refusal lost its diagnosis'
[ "$(cat "$NPM_VIEW_COUNT")" = 2 ] || fail 'divergent identity was not refused on first sight'

# The budget validates as an integer of seconds and floors at one cadence, so
# a tiny window still performs exactly one readiness lookup.
rc=0
before="$(wc -l <"$NPM_LOG" | tr -d ' ')"
PATH="$BIN:$PATH" NPM_STATE="$NPM_STATE" NPM_LOG="$NPM_LOG" NPM_SRI="$NPM_SRI" \
  NPM_VIEW_COUNT="$NPM_VIEW_COUNT" NIKA_NPM_READINESS_SECONDS=soon \
  ACTIONS_ID_TOKEN_REQUEST_URL=https://oidc.test ACTIONS_ID_TOKEN_REQUEST_TOKEN=test \
  bash "$ROOT/scripts/release/npm-publish-immutable.sh" publish x \
  "$NPM_TGZ" "$NPM_SHA" >/dev/null 2>&1 || rc=$?
[ "$rc" -eq 64 ] || fail 'a non-integer readiness budget was accepted'
[ "$(wc -l <"$NPM_LOG" | tr -d ' ')" = "$before" ] || fail 'budget validation reached npm publish'
# A leading zero reads as decimal for `[ -ge ]` but octal for `$(( ))`, which
# used to defeat the floor (010) or die mid-publish (090); zero is not
# positive either. All refuse 64 before any registry write.
for bad_budget in 010 090 0; do
  rc=0
  before="$(wc -l <"$NPM_LOG" | tr -d ' ')"
  PATH="$BIN:$PATH" NPM_STATE="$NPM_STATE" NPM_LOG="$NPM_LOG" NPM_SRI="$NPM_SRI" \
    NPM_VIEW_COUNT="$NPM_VIEW_COUNT" NIKA_NPM_READINESS_SECONDS="$bad_budget" \
    ACTIONS_ID_TOKEN_REQUEST_URL=https://oidc.test ACTIONS_ID_TOKEN_REQUEST_TOKEN=test \
    bash "$ROOT/scripts/release/npm-publish-immutable.sh" publish x \
    "$NPM_TGZ" "$NPM_SHA" >/dev/null 2>&1 || rc=$?
  [ "$rc" -eq 64 ] || fail "leading-zero readiness budget $bad_budget was accepted"
  [ "$(wc -l <"$NPM_LOG" | tr -d ' ')" = "$before" ] \
    || fail "refused readiness budget $bad_budget reached npm publish"
done
# A budget below one cadence floors to exactly one attempt: the version never
# shows, so the refusal is 69 within the floored 10s window after one
# readiness lookup (the second view; the first is the pre-publish check).
printf 'lag\n' >"$NPM_STATE"
rm -f "$NPM_VIEW_COUNT"
: >"$SLEEP_LOG"
rc=0
PATH="$BIN:$PATH" NPM_STATE="$NPM_STATE" NPM_LOG="$NPM_LOG" NPM_SRI="$NPM_SRI" \
  NPM_VIEW_COUNT="$NPM_VIEW_COUNT" NPM_VISIBLE_AFTER=99 NIKA_NPM_READINESS_SECONDS=5 \
  SLEEP_LOG="$SLEEP_LOG" \
  ACTIONS_ID_TOKEN_REQUEST_URL=https://oidc.test ACTIONS_ID_TOKEN_REQUEST_TOKEN=test \
  bash "$ROOT/scripts/release/npm-publish-immutable.sh" publish x \
  "$NPM_TGZ" "$NPM_SHA" >"$TEST_ROOT/npm-floor.out" 2>&1 || rc=$?
[ "$rc" -eq 69 ] || fail 'the floored one-cadence budget did not refuse 69'
grep -Fq 'never became visible within 10s' "$TEST_ROOT/npm-floor.out" \
  || fail 'the floored budget refusal did not name the floored window'
[ "$(cat "$NPM_VIEW_COUNT")" = 2 ] \
  || fail 'the budget floor did not perform exactly one readiness lookup'
[ ! -s "$SLEEP_LOG" ] || fail 'a single-attempt budget slept'

# The default budget is pinned: with the variable unset the version stays
# invisible through 30 cadence lookups and the refusal names 300s.
printf 'lag\n' >"$NPM_STATE"
rm -f "$NPM_VIEW_COUNT"
: >"$SLEEP_LOG"
rc=0
PATH="$BIN:$PATH" NPM_STATE="$NPM_STATE" NPM_LOG="$NPM_LOG" NPM_SRI="$NPM_SRI" \
  NPM_VIEW_COUNT="$NPM_VIEW_COUNT" NPM_VISIBLE_AFTER=99 SLEEP_LOG="$SLEEP_LOG" \
  ACTIONS_ID_TOKEN_REQUEST_URL=https://oidc.test ACTIONS_ID_TOKEN_REQUEST_TOKEN=test \
  bash "$ROOT/scripts/release/npm-publish-immutable.sh" publish x \
  "$NPM_TGZ" "$NPM_SHA" >"$TEST_ROOT/npm-default.out" 2>&1 || rc=$?
[ "$rc" -eq 69 ] || fail 'the default readiness budget did not refuse 69'
grep -Fq 'never became visible within 300s' "$TEST_ROOT/npm-default.out" \
  || fail 'the default readiness window is not 300s'
[ "$(cat "$NPM_VIEW_COUNT")" = 31 ] \
  || fail 'the default budget did not perform 30 readiness lookups'
[ "$(wc -l <"$SLEEP_LOG" | tr -d ' ')" = 29 ] \
  || fail 'the default budget did not sleep once per cadence between lookups'
[ -z "$(grep -vFx '10' "$SLEEP_LOG" || true)" ] \
  || fail 'the default budget slept a non-cadence interval'

# OCI: absent/equal/divergent and label drift. The fake exposes two runnable
# platforms plus their BuildKit attestations, like the real release index.
cat >"$BIN/docker" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [ "$1" = pull ]; then
  platform=""
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --platform) platform="$2"; shift 2 ;;
      *) shift ;;
    esac
  done
  printf 'pull %s\n' "$platform" >>"$PAYLOAD_LOG"
  exit 0
fi
if [ "$1" = create ]; then
  platform=""
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --platform) platform="$2"; shift 2 ;;
      *) shift ;;
    esac
  done
  case "$platform" in
    linux/amd64) container=aaaaaaaaaaaa ;;
    linux/arm64) container=bbbbbbbbbbbb ;;
    *) exit 90 ;;
  esac
  printf 'create %s\n' "$platform" >>"$PAYLOAD_LOG"
  printf '%s\n' "$container"
  exit 0
fi
if [ "$1" = cp ]; then
  case "$2" in
    aaaaaaaaaaaa:*) source="$PAYLOAD_SOURCE_AMD64" ;;
    bbbbbbbbbbbb:*) source="$PAYLOAD_SOURCE_ARM64" ;;
    *) exit 90 ;;
  esac
  cp "$source" "$3"
  printf 'cp %s\n' "$2" >>"$PAYLOAD_LOG"
  exit 0
fi
if [ "$1" = rm ]; then
  printf 'rm %s\n' "$2" >>"$PAYLOAD_LOG"
  exit 0
fi
if [ "$1 $2 $3" != 'buildx imagetools inspect' ] \
  && [ "$1 $2 $3" != 'buildx imagetools create' ]; then exit 90; fi
if [ "$3" = create ]; then
  [ "$4" = --tag ] || exit 90
  tag="${5##*:}"
  state=equal
  [ -z "${OCI_STATE:-}" ] || state="$(cat "$OCI_STATE")"
  case "$state" in
    alias-create-fail)
      case "$tag" in v*) exit 1 ;; esac
      ;;
    alias-create-diverge)
      case "$tag" in
        v*)
          printf 'create %s\n' "$5" >>"$OCI_LOG"
          printf 'sha256:%064d\n' 9 >"$OCI_TAGS/$tag"
          exit 0
          ;;
      esac
      ;;
  esac
  printf '%s\n' "${6##*@}" >"$OCI_TAGS/$tag"
  printf 'create %s\n' "$5" >>"$OCI_LOG"
  exit 0
fi
ref="$4"
tag="${ref##*:}"
state=equal
[ -z "${OCI_STATE:-}" ] || state="$(cat "$OCI_STATE")"
if [ "${5:-}" = --raw ]; then
  jq -n '
    def digest($n): "sha256:" + ([range(64) | $n] | join(""));
    def descriptor($n; $os; $arch):
      {mediaType:"application/vnd.oci.image.manifest.v1+json",digest:digest($n),size:675,
       platform:{os:$os,architecture:$arch}};
    {schemaVersion:2,mediaType:"application/vnd.oci.image.index.v1+json",manifests:[
      descriptor("1";"linux";"amd64"), descriptor("2";"linux";"arm64"),
      (descriptor("3";"unknown";"unknown") + {annotations:{
        "vnd.docker.reference.type":"attestation-manifest","vnd.docker.reference.digest":digest("1")}}),
      (descriptor("4";"unknown";"unknown") + {annotations:{
        "vnd.docker.reference.type":"attestation-manifest","vnd.docker.reference.digest":digest("2")}})]}'
  exit 0
fi
if printf '%s\n' "$*" | grep -Fq '.Manifest.Digest'; then
  [ -z "${OCI_LOOKUP_LOG:-}" ] || printf '%s\n' "$ref" >>"$OCI_LOOKUP_LOG"
  case "$ref" in
    *@sha256:*) printf '"%s"\n' "${ref##*@}"; exit 0 ;;
  esac
  case "$state" in
    credential-helper)
      echo 'error getting credentials - exec: "docker-credential-pass": executable file not found' >&2
      exit 1
      ;;
    mixed)
      echo 'unexpected status 500 Internal Server Error; 401 Unauthorized; secondary: manifest unknown (404 Not Found)' >&2
      exit 1
      ;;
    divergent)
      [ "$tag" != 9.9.9 ] || {
        printf '"sha256:%064d"\n' 9
        exit 0
      }
      ;;
    alias-error)
      case "$tag" in
        v*)
          echo 'unexpected status 500 Internal Server Error' >&2
          exit 1
          ;;
      esac
      ;;
  esac
  if [ -f "$OCI_TAGS/$tag" ]; then
    printf '"%s"\n' "$(cat "$OCI_TAGS/$tag")"
    exit 0
  fi
  case "$tag" in
    v*)
      # buildx 0.30.1 answers an absent tag with exactly this line
      printf 'ERROR: %s: not found\n' "$ref" >&2
      exit 1
      ;;
  esac
  echo 'manifest unknown' >&2
  exit 1
fi
revision="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
[ "$state" = label-drift ] && revision=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
printf '{"org.opencontainers.image.revision":"%s","org.opencontainers.image.version":"9.9.9","org.opencontainers.image.source":"https://github.com/supernovae-st/nika","org.opencontainers.image.licenses":"AGPL-3.0-or-later"}\n' "$revision"
EOF
chmod +x "$BIN/docker"
OCI_STATE="$TEST_ROOT/oci-state"
OCI_LOG="$TEST_ROOT/oci-log"
OCI_TAGS="$TEST_ROOT/oci-tags"
OCI_LOOKUP_LOG="$TEST_ROOT/oci-lookup-log"
mkdir -p "$OCI_TAGS"
: >"$OCI_LOG"
CANDIDATE="sha256:$(printf '%064d' 1)"
printf 'absent\n' >"$OCI_STATE"
: >"$OCI_LOOKUP_LOG"
out="$(PATH="$BIN:$PATH" OCI_STATE="$OCI_STATE" OCI_LOG="$OCI_LOG" OCI_TAGS="$OCI_TAGS" \
  OCI_LOOKUP_LOG="$OCI_LOOKUP_LOG" \
  bash "$ROOT/scripts/release/oci-coordinate-immutable.sh" publish \
  ghcr.io/supernovae-st/nika 9.9.9 "$CANDIDATE" \
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa \
  https://github.com/supernovae-st/nika)"
[ "$out" = "$CANDIDATE" ] || fail 'OCI publish did not print the committed digest on stdout'
# nika#1634: the alias converges BEFORE the version tag, which stays the
# commit marker — the create order is v9.9.9 then 9.9.9, exactly once each,
# and the absent alias lookup matched the exact buildx not-found line.
printf 'create ghcr.io/supernovae-st/nika:v9.9.9\ncreate ghcr.io/supernovae-st/nika:9.9.9\n' \
  >"$TEST_ROOT/oci-log-want"
diff -u "$TEST_ROOT/oci-log-want" "$OCI_LOG" \
  || fail 'OCI absent coordinate did not create exactly the v-alias then the version tag'
PATH="$BIN:$PATH" OCI_STATE="$OCI_STATE" OCI_LOG="$OCI_LOG" OCI_TAGS="$OCI_TAGS" \
  OCI_LOOKUP_LOG="$OCI_LOOKUP_LOG" \
  bash "$ROOT/scripts/release/oci-coordinate-immutable.sh" verify \
  ghcr.io/supernovae-st/nika 9.9.9 - \
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa \
  https://github.com/supernovae-st/nika >/dev/null
# A replay over the occupied version with the alias ABSENT exits on the equal
# digest without ever reading the alias: no retag of an already published
# release, and the early exit is pinned to not touch the alias at all.
printf 'equal\n' >"$OCI_STATE"
rm "$OCI_TAGS/v9.9.9"
: >"$OCI_LOOKUP_LOG"
out="$(PATH="$BIN:$PATH" OCI_STATE="$OCI_STATE" OCI_LOG="$OCI_LOG" OCI_TAGS="$OCI_TAGS" \
  OCI_LOOKUP_LOG="$OCI_LOOKUP_LOG" \
  bash "$ROOT/scripts/release/oci-coordinate-immutable.sh" publish \
  ghcr.io/supernovae-st/nika 9.9.9 "$CANDIDATE" \
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa \
  https://github.com/supernovae-st/nika)"
[ "$out" = "$CANDIDATE" ] || fail 'replay over an occupied equal version did not print its digest'
[ "$(wc -l <"$OCI_LOG" | tr -d ' ')" = 2 ] \
  || fail 'an occupied equal version retagged an already published coordinate'
[ ! -e "$OCI_TAGS/v9.9.9" ] || fail 'the occupied early exit recreated the absent v-alias'
if grep -Fqx 'ghcr.io/supernovae-st/nika:v9.9.9' "$OCI_LOOKUP_LOG"; then
  fail 'the occupied early exit read the v-alias'
fi
printf 'divergent\n' >"$OCI_STATE"
if PATH="$BIN:$PATH" OCI_STATE="$OCI_STATE" OCI_LOG="$OCI_LOG" OCI_TAGS="$OCI_TAGS" \
  bash "$ROOT/scripts/release/oci-coordinate-immutable.sh" publish \
  ghcr.io/supernovae-st/nika 9.9.9 "$CANDIDATE" \
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa \
  https://github.com/supernovae-st/nika >/dev/null 2>&1; then
  fail 'OCI divergent version passed'
fi
printf 'label-drift\n' >"$OCI_STATE"
if PATH="$BIN:$PATH" OCI_STATE="$OCI_STATE" OCI_LOG="$OCI_LOG" OCI_TAGS="$OCI_TAGS" \
  bash "$ROOT/scripts/release/oci-coordinate-immutable.sh" verify \
  ghcr.io/supernovae-st/nika 9.9.9 - \
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa \
  https://github.com/supernovae-st/nika >/dev/null 2>&1; then
  fail 'OCI label drift passed'
fi

printf 'credential-helper\n' >"$OCI_STATE"
if PATH="$BIN:$PATH" OCI_STATE="$OCI_STATE" OCI_LOG="$OCI_LOG" OCI_TAGS="$OCI_TAGS" \
  bash "$ROOT/scripts/release/oci-coordinate-immutable.sh" publish \
  ghcr.io/supernovae-st/nika 9.9.9 "$CANDIDATE" \
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa \
  https://github.com/supernovae-st/nika >/dev/null 2>&1; then
  fail 'credential-helper not-found error was classified as registry absence'
fi
[ "$(wc -l <"$OCI_LOG" | tr -d ' ')" = 2 ] \
  || fail 'unknown OCI lookup error reached a write'
printf 'mixed\n' >"$OCI_STATE"
if PATH="$BIN:$PATH" OCI_STATE="$OCI_STATE" OCI_LOG="$OCI_LOG" OCI_TAGS="$OCI_TAGS" \
  bash "$ROOT/scripts/release/oci-coordinate-immutable.sh" publish \
  ghcr.io/supernovae-st/nika 9.9.9 "$CANDIDATE" \
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa \
  https://github.com/supernovae-st/nika >/dev/null 2>&1; then
  fail 'mixed OCI 500/unauthorized/manifest-unknown granted create authority'
fi
[ "$(wc -l <"$OCI_LOG" | tr -d ' ')" = 2 ] || fail 'mixed OCI lookup reached a write'

# Alias present-equal with the version absent: the equal alias is a no-op and
# only the version tag is created (kills the present-equal-turned-refusal
# mutation).
printf 'equal\n' >"$OCI_STATE"
rm "$OCI_TAGS/9.9.9"
printf '%s\n' "$CANDIDATE" >"$OCI_TAGS/v9.9.9"
out="$(PATH="$BIN:$PATH" OCI_STATE="$OCI_STATE" OCI_LOG="$OCI_LOG" OCI_TAGS="$OCI_TAGS" \
  OCI_LOOKUP_LOG="$OCI_LOOKUP_LOG" \
  bash "$ROOT/scripts/release/oci-coordinate-immutable.sh" publish \
  ghcr.io/supernovae-st/nika 9.9.9 "$CANDIDATE" \
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa \
  https://github.com/supernovae-st/nika)"
[ "$out" = "$CANDIDATE" ] || fail 'alias-equal publish did not print the committed digest'
[ "$(wc -l <"$OCI_LOG" | tr -d ' ')" = 3 ] || fail 'an equal v-alias was recreated'
[ "$(sed -n '$p' "$OCI_LOG")" = 'create ghcr.io/supernovae-st/nika:9.9.9' ] \
  || fail 'the alias-equal path did not create exactly the version tag'
[ "$(cat "$OCI_TAGS/v9.9.9")" = "$CANDIDATE" ] || fail 'the equal v-alias was moved'

# A v-alias lookup error after the version is proven absent fails 69 with
# ZERO writes — the version tag must not commit ahead of the alias.
printf 'alias-error\n' >"$OCI_STATE"
rm "$OCI_TAGS/9.9.9" "$OCI_TAGS/v9.9.9"
rc=0
PATH="$BIN:$PATH" OCI_STATE="$OCI_STATE" OCI_LOG="$OCI_LOG" OCI_TAGS="$OCI_TAGS" \
  OCI_LOOKUP_LOG="$OCI_LOOKUP_LOG" \
  bash "$ROOT/scripts/release/oci-coordinate-immutable.sh" publish \
  ghcr.io/supernovae-st/nika 9.9.9 "$CANDIDATE" \
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa \
  https://github.com/supernovae-st/nika >"$TEST_ROOT/oci-alias-err.out" 2>&1 || rc=$?
[ "$rc" -eq 69 ] || fail 'a v-alias lookup error did not fail 69'
grep -Fq 'v-alias lookup failed without explicit absence' "$TEST_ROOT/oci-alias-err.out" \
  || fail 'the v-alias lookup error lost its diagnosis'
[ "$(wc -l <"$OCI_LOG" | tr -d ' ')" = 3 ] || fail 'a v-alias lookup error reached a write'
[ ! -e "$OCI_TAGS/9.9.9" ] || fail 'the version tag committed before the v-alias converged'

# A failed v-alias write fails 69 leaving NO tag behind; the ordinary re-run
# re-enters the create path and recovers both tags.
printf 'alias-create-fail\n' >"$OCI_STATE"
rc=0
PATH="$BIN:$PATH" OCI_STATE="$OCI_STATE" OCI_LOG="$OCI_LOG" OCI_TAGS="$OCI_TAGS" \
  OCI_LOOKUP_LOG="$OCI_LOOKUP_LOG" \
  bash "$ROOT/scripts/release/oci-coordinate-immutable.sh" publish \
  ghcr.io/supernovae-st/nika 9.9.9 "$CANDIDATE" \
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa \
  https://github.com/supernovae-st/nika >"$TEST_ROOT/oci-alias-fail.out" 2>&1 || rc=$?
[ "$rc" -eq 69 ] || fail 'a failed v-alias write did not fail 69'
grep -Fq 'v-alias write failed and the version tag remains absent' "$TEST_ROOT/oci-alias-fail.out" \
  || fail 'the failed v-alias write lost its diagnosis'
if [ -e "$OCI_TAGS/9.9.9" ] || [ -e "$OCI_TAGS/v9.9.9" ]; then
  fail 'a failed v-alias write left a tag behind'
fi
[ "$(wc -l <"$OCI_LOG" | tr -d ' ')" = 3 ] || fail 'a failed v-alias write logged a create'
printf 'equal\n' >"$OCI_STATE"
out="$(PATH="$BIN:$PATH" OCI_STATE="$OCI_STATE" OCI_LOG="$OCI_LOG" OCI_TAGS="$OCI_TAGS" \
  OCI_LOOKUP_LOG="$OCI_LOOKUP_LOG" \
  bash "$ROOT/scripts/release/oci-coordinate-immutable.sh" publish \
  ghcr.io/supernovae-st/nika 9.9.9 "$CANDIDATE" \
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa \
  https://github.com/supernovae-st/nika)"
[ "$out" = "$CANDIDATE" ] || fail 'the re-run after a failed v-alias write did not recover'
[ "$(wc -l <"$OCI_LOG" | tr -d ' ')" = 5 ] \
  || fail 'the recovery re-run did not create exactly the v-alias and the version tag'

# A v-alias create that commits OTHER bytes (read-back mismatch) refuses 73
# with the version tag still absent; the replay refuses identically with zero
# further writes.
printf 'alias-create-diverge\n' >"$OCI_STATE"
rm "$OCI_TAGS/9.9.9" "$OCI_TAGS/v9.9.9"
rc=0
PATH="$BIN:$PATH" OCI_STATE="$OCI_STATE" OCI_LOG="$OCI_LOG" OCI_TAGS="$OCI_TAGS" \
  OCI_LOOKUP_LOG="$OCI_LOOKUP_LOG" \
  bash "$ROOT/scripts/release/oci-coordinate-immutable.sh" publish \
  ghcr.io/supernovae-st/nika 9.9.9 "$CANDIDATE" \
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa \
  https://github.com/supernovae-st/nika >"$TEST_ROOT/oci-alias-diverge.out" 2>&1 || rc=$?
[ "$rc" -eq 73 ] || fail 'a v-alias read-back mismatch did not fail 73'
grep -Fq 'committed v-alias digest differs' "$TEST_ROOT/oci-alias-diverge.out" \
  || fail 'the v-alias read-back mismatch lost its diagnosis'
[ ! -e "$OCI_TAGS/9.9.9" ] || fail 'the version tag committed after a divergent v-alias read-back'
[ "$(cat "$OCI_TAGS/v9.9.9")" = "sha256:$(printf '%064d' 9)" ] \
  || fail 'the fake did not record the divergent v-alias'
rc=0
PATH="$BIN:$PATH" OCI_STATE="$OCI_STATE" OCI_LOG="$OCI_LOG" OCI_TAGS="$OCI_TAGS" \
  OCI_LOOKUP_LOG="$OCI_LOOKUP_LOG" \
  bash "$ROOT/scripts/release/oci-coordinate-immutable.sh" publish \
  ghcr.io/supernovae-st/nika 9.9.9 "$CANDIDATE" \
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa \
  https://github.com/supernovae-st/nika >/dev/null 2>&1 || rc=$?
[ "$rc" -eq 73 ] || fail 'the replay after a divergent v-alias read-back did not refuse again'
[ "$(wc -l <"$OCI_LOG" | tr -d ' ')" = 6 ] || fail 'the divergent v-alias replay wrote'

# A v-alias already occupied by foreign bytes refuses 73 with zero writes on
# every replay — the tag is never moved.
printf 'equal\n' >"$OCI_STATE"
rm -f "$OCI_TAGS/9.9.9"
rc=0
PATH="$BIN:$PATH" OCI_STATE="$OCI_STATE" OCI_LOG="$OCI_LOG" OCI_TAGS="$OCI_TAGS" \
  OCI_LOOKUP_LOG="$OCI_LOOKUP_LOG" \
  bash "$ROOT/scripts/release/oci-coordinate-immutable.sh" publish \
  ghcr.io/supernovae-st/nika 9.9.9 "$CANDIDATE" \
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa \
  https://github.com/supernovae-st/nika >"$TEST_ROOT/oci-alias.out" 2>&1 || rc=$?
[ "$rc" -eq 73 ] || fail 'divergent occupied v-alias was not refused'
grep -Fq 'REFUSED divergent occupied v-alias digest' "$TEST_ROOT/oci-alias.out" \
  || fail 'divergent v-alias refusal lost its diagnosis'
[ "$(cat "$OCI_TAGS/v9.9.9")" = "sha256:$(printf '%064d' 9)" ] \
  || fail 'the refusal moved the occupied v-alias'
[ "$(wc -l <"$OCI_LOG" | tr -d ' ')" = 6 ] \
  || fail 'divergent v-alias refusal wrote'
rc=0
PATH="$BIN:$PATH" OCI_STATE="$OCI_STATE" OCI_LOG="$OCI_LOG" OCI_TAGS="$OCI_TAGS" \
  OCI_LOOKUP_LOG="$OCI_LOOKUP_LOG" \
  bash "$ROOT/scripts/release/oci-coordinate-immutable.sh" publish \
  ghcr.io/supernovae-st/nika 9.9.9 "$CANDIDATE" \
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa \
  https://github.com/supernovae-st/nika >/dev/null 2>&1 || rc=$?
[ "$rc" -eq 73 ] || fail 'divergent occupied v-alias was not refused on replay'
[ "$(wc -l <"$OCI_LOG" | tr -d ' ')" = 6 ] \
  || fail 'divergent v-alias refusal wrote on replay'

# A v-prefixed version is a usage error at validation, before any registry
# lookup (kills the moved v-prefix guard).
: >"$OCI_LOOKUP_LOG"
rc=0
PATH="$BIN:$PATH" OCI_STATE="$OCI_STATE" OCI_LOG="$OCI_LOG" OCI_TAGS="$OCI_TAGS" \
  OCI_LOOKUP_LOG="$OCI_LOOKUP_LOG" \
  bash "$ROOT/scripts/release/oci-coordinate-immutable.sh" publish \
  ghcr.io/supernovae-st/nika v9.9.9 "$CANDIDATE" \
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa \
  https://github.com/supernovae-st/nika >/dev/null 2>&1 || rc=$?
[ "$rc" -eq 64 ] || fail 'a v-prefixed version was accepted'
[ ! -s "$OCI_LOOKUP_LOG" ] || fail 'the v-prefix guard ran after a registry lookup'
[ "$(wc -l <"$OCI_LOG" | tr -d ' ')" = 6 ] || fail 'the v-prefix guard ran after a write'

# Payload proof runs both exact digest platforms and compares each container
# binary checksum with the corresponding extracted native tarball.
PAYLOAD_ARTIFACTS="$TEST_ROOT/payload-artifacts"
mkdir -p "$PAYLOAD_ARTIFACTS" "$TEST_ROOT/payload-x64" "$TEST_ROOT/payload-arm64"
printf 'x64 binary bytes\n' >"$TEST_ROOT/payload-x64/nika"
printf 'arm64 binary bytes\n' >"$TEST_ROOT/payload-arm64/nika"
tar -czf "$PAYLOAD_ARTIFACTS/nika-linux-x64-${VERSION}.tar.gz" \
  -C "$TEST_ROOT/payload-x64" nika
tar -czf "$PAYLOAD_ARTIFACTS/nika-linux-arm64-${VERSION}.tar.gz" \
  -C "$TEST_ROOT/payload-arm64" nika
PAYLOAD_LOG="$TEST_ROOT/payload-log"
: >"$PAYLOAD_LOG"
PATH="$BIN:$PATH" PAYLOAD_LOG="$PAYLOAD_LOG" \
  PAYLOAD_SOURCE_AMD64="$TEST_ROOT/payload-x64/nika" \
  PAYLOAD_SOURCE_ARM64="$TEST_ROOT/payload-arm64/nika" \
  bash "$ROOT/scripts/release/verify-oci-payload.sh" \
  ghcr.io/supernovae-st/nika "$CANDIDATE" "$VERSION" "$PAYLOAD_ARTIFACTS" >/dev/null
[ "$(grep -Ec '^(pull|create|cp|rm) ' "$PAYLOAD_LOG")" -eq 8 ] \
  || fail 'OCI payload proof did not pull/create/copy/remove both stopped containers'
if PATH="$BIN:$PATH" PAYLOAD_LOG="$PAYLOAD_LOG" \
  PAYLOAD_SOURCE_AMD64="$TEST_ROOT/payload-x64/nika" \
  PAYLOAD_SOURCE_ARM64="$TEST_ROOT/payload-x64/nika" \
  bash "$ROOT/scripts/release/verify-oci-payload.sh" \
  ghcr.io/supernovae-st/nika "$CANDIDATE" "$VERSION" \
  "$PAYLOAD_ARTIFACTS" >/dev/null 2>&1; then
  fail 'OCI payload drift passed'
fi

# Generic SLSA provenance must be cryptographically checked by the official
# verifier against the exact repository, tag, and four native subjects. This
# is the same helper used to verify a prior run's already-staged statement.
SLSA_LOG="$TEST_ROOT/slsa-log"
: >"$SLSA_LOG"
cat >"$BIN/slsa-verifier" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"$SLSA_LOG"
[ "$1" = verify-artifact ]
printf '%s\n' "$*" | grep -Fq -- '--source-uri github.com/supernovae-st/nika'
printf '%s\n' "$*" | grep -Fq -- '--source-tag v9.9.9'
[ "${SLSA_REFUSE:-0}" != 1 ]
EOF
chmod +x "$BIN/slsa-verifier"
native=()
for platform in linux-arm64 linux-x64 macos-arm64 macos-x64; do
  native+=("$LOCAL/nika-${platform}-${VERSION}.tar.gz")
done
SLSA_REMOTE="$TEST_ROOT/prior-run-provenance"
SLSA_RECOVERED="$TEST_ROOT/recovered-provenance"
cp "$LOCAL/multiple.intoto.jsonl" "$SLSA_REMOTE"
mkdir -p "$SLSA_RECOVERED"
cat >"$BIN/gh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [ "$1" = api ]; then
  printf '1\n'
  exit 0
fi
if [ "$1 $2" = 'release download' ]; then
  destination=""
  shift 2
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --dir) destination="$2"; shift 2 ;;
      *) shift ;;
    esac
  done
  cp "$SLSA_REMOTE" "$destination/multiple.intoto.jsonl"
  exit 0
fi
exit 90
EOF
chmod +x "$BIN/gh"
matches="$(PATH="$BIN:$PATH" SLSA_REMOTE="$SLSA_REMOTE" \
  gh api repos/supernovae-st/nika/releases/123)"
[ "$matches" = 1 ] || fail 'prior-run SLSA asset identity was not unique'
PATH="$BIN:$PATH" SLSA_REMOTE="$SLSA_REMOTE" \
  gh release download "$TAG" --repo supernovae-st/nika \
  --pattern multiple.intoto.jsonl --dir "$SLSA_RECOVERED"
PATH="$BIN:$PATH" SLSA_LOG="$SLSA_LOG" \
  bash "$ROOT/scripts/release/verify-slsa-provenance.sh" \
  "$TAG" supernovae-st/nika "$SLSA_RECOVERED/multiple.intoto.jsonl" \
  "${native[@]}"
[ "$(wc -w <"$SLSA_LOG" | tr -d ' ')" -gt 10 ] \
  || fail 'prior-run SLSA verification did not invoke source/subject verification'
if SLSA_REFUSE=1 PATH="$BIN:$PATH" SLSA_LOG="$SLSA_LOG" \
  bash "$ROOT/scripts/release/verify-slsa-provenance.sh" \
  "$TAG" supernovae-st/nika "$LOCAL/multiple.intoto.jsonl" \
  "${native[@]}" >/dev/null 2>&1; then
  fail 'cryptographically rejected prior-run SLSA provenance passed'
fi

# Release body digest persistence and finalization share one fake GitHub API.
# It models manual drift, stale release metadata, and commit-then-error writes.
# The 0.118.5 body-only PATCH orphaned the draft despite returning success.
# Omitted tag_name must not be modeled as an identity-preserving write.
cat >"$BIN/gh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
[ -n "${GH_TOKEN:-}" ] || { echo 'GH_TOKEN is required' >&2; exit 77; }
[ "$1" = api ] || { echo "unexpected gh: $*" >&2; exit 90; }
shift
method=GET
if [ "${1:-}" = --method ]; then method="$2"; shift 2; fi
endpoint="$1"
shift
if [ "$method" = PATCH ]; then
  printf 'PATCH\n' >>"$MARKER_PATCH_LOG"
  printf '%s\n' "$@" >>"$MARKER_PATCH_LOG"
  patched_tag=untagged-fixture
  for arg in "$@"; do
    case "$arg" in tag_name=*) patched_tag="${arg#tag_name=}" ;; esac
  done
  printf '%s\n' "${POST_PATCH_TAG:-$patched_tag}" >"${RELEASE_BODY}.tag"
  if printf '%s\n' "$*" | grep -Fq 'draft=false'; then
    printf '%s\n' "$*" >"$FINALIZE_LOG"
    printf 'false\n' >"$RELEASE_DRAFT"
    [ "${FINALIZE_COMMIT_THEN_ERROR:-0}" = 1 ] && exit 1
    exit 0
  fi
  body_arg=""
  while [ "$#" -gt 0 ]; do
    case "$1" in
      -f)
        case "$2" in body=*) body_arg="$2" ;; esac
        shift 2
        ;;
      *) shift ;;
    esac
  done
  printf '%s' "${body_arg#body=}" >"$RELEASE_BODY"
  [ "${MARKER_COMMIT_THEN_ERROR:-0}" = 1 ] && exit 1
  exit 0
fi
if [[ "$endpoint" == */releases/123 ]]; then
  if printf '%s\n' "$*" | grep -Fq '.body'; then
    if [ "${RELEASE_BODY_GET_ERROR:-0}" = 1 ]; then
      echo 'gh: body read failed (HTTP 500)' >&2
      exit 1
    fi
    cat "$RELEASE_BODY"
  else
    if [ "${RELEASE_STATE_GET_ERROR:-0}" = 1 ]; then
      echo 'gh: state read failed (HTTP 500)' >&2
      exit 1
    fi
    actual_tag="$RELEASE_TAG"
    [ ! -f "${RELEASE_BODY}.tag" ] || actual_tag="$(cat "${RELEASE_BODY}.tag")"
    printf '123\t%s\t%s\t%s\n' "$actual_tag" "$(cat "$RELEASE_DRAFT")" "$RELEASE_PRERELEASE"
  fi
  exit 0
fi
if [[ "$endpoint" == */releases ]]; then
  cat "$RELEASE_LIST"
  exit 0
fi
echo "unexpected gh api endpoint: $endpoint $*" >&2
exit 90
EOF
chmod +x "$BIN/gh"
RELEASE_BODY="$TEST_ROOT/release-body"
RELEASE_DRAFT="$TEST_ROOT/release-draft"
RELEASE_LIST="$TEST_ROOT/release-list"
FINALIZE_LOG="$TEST_ROOT/finalize-log"
MARKER_PATCH_LOG="$TEST_ROOT/marker-patch-log"
: >"$MARKER_PATCH_LOG"
printf 'curated release body\n' >"$RELEASE_BODY"
: >"$FINALIZE_LOG"
printf 'true\n' >"$RELEASE_DRAFT"
printf '123\tv9.9.9\n' >"$RELEASE_LIST"
DIGEST="sha256:$(printf '%064d' 7)"
OTHER_DIGEST="sha256:$(printf '%064d' 8)"
release_env=(
  PATH="$BIN:$PATH"
  GH_TOKEN=test
  RELEASE_BODY="$RELEASE_BODY"
  RELEASE_DRAFT="$RELEASE_DRAFT"
  RELEASE_LIST="$RELEASE_LIST"
  FINALIZE_LOG="$FINALIZE_LOG"
  MARKER_PATCH_LOG="$MARKER_PATCH_LOG"
  RELEASE_TAG="$TAG"
  RELEASE_PRERELEASE=false
)
env "${release_env[@]}" MARKER_COMMIT_THEN_ERROR=1 \
  bash "$ROOT/scripts/release/release-digest-marker.sh" stage \
  supernovae-st/nika 123 "$TAG" \
  2222222222222222222222222222222222222222 "$DIGEST" >/dev/null
grep -Fqx 'curated release body' "$RELEASE_BODY" \
  || fail 'digest staging replaced the existing release body'
cp "$RELEASE_BODY" "$TEST_ROOT/valid-release-body"
grep -Fqx "tag_name=$TAG" "$MARKER_PATCH_LOG" || fail 'marker PATCH omitted its tag'
grep -Fqx "target_commitish=$RELEASE_SHA" "$MARKER_PATCH_LOG" \
  || fail 'marker PATCH omitted its proven SHA'

# Explicit coordinate fields do not waive post-write identity verification.
# An observed divergent tag is refused, not repaired by a second PATCH.
printf 'curated release body\n' >"$RELEASE_BODY"
: >"$MARKER_PATCH_LOG"
if env "${release_env[@]}" POST_PATCH_TAG=v9.9.8 \
  bash "$ROOT/scripts/release/release-digest-marker.sh" stage \
  supernovae-st/nika 123 "$TAG" "$RELEASE_SHA" "$DIGEST" \
  >"$TEST_ROOT/post-patch-drift.out" 2>&1; then
  fail 'marker stage accepted post-write release identity drift'
fi
[ "$(grep -c '^PATCH$' "$MARKER_PATCH_LOG")" = 1 ] \
  || fail 'marker stage retried an observed identity drift'
grep -Fq 'REFUSED tag v9.9.8' "$TEST_ROOT/post-patch-drift.out" \
  || fail 'post-write refusal did not identify the tag drift'
: >"$MARKER_PATCH_LOG"
if env "${release_env[@]}" \
  bash "$ROOT/scripts/release/release-digest-marker.sh" stage \
  supernovae-st/nika 123 "$TAG" "$RELEASE_SHA" "$DIGEST" >/dev/null 2>&1; then
  fail 'marker stage accepted pre-existing release identity drift'
fi
[ ! -s "$MARKER_PATCH_LOG" ] || fail 'marker stage overwrote a mismatched release tag'
printf '%s\n' "$TAG" >"${RELEASE_BODY}.tag"
cp "$TEST_ROOT/valid-release-body" "$RELEASE_BODY"
if env "${release_env[@]}" RELEASE_STATE_GET_ERROR=1 \
  bash "$ROOT/scripts/release/release-digest-marker.sh" read \
  supernovae-st/nika 123 "$TAG" \
  2222222222222222222222222222222222222222 >/dev/null 2>&1; then
  fail 'digest marker read passed after release-state GET failure'
fi
if env "${release_env[@]}" RELEASE_BODY_GET_ERROR=1 \
  bash "$ROOT/scripts/release/release-digest-marker.sh" read \
  supernovae-st/nika 123 "$TAG" \
  2222222222222222222222222222222222222222 >/dev/null 2>&1; then
  fail 'digest marker read passed after release-body GET failure'
fi
printf 'curated release body\n<!-- nika-ghcr-digest: malformed -->\n' >"$RELEASE_BODY"
if env "${release_env[@]}" \
  bash "$ROOT/scripts/release/release-digest-marker.sh" read \
  supernovae-st/nika 123 "$TAG" \
  2222222222222222222222222222222222222222 >/dev/null 2>&1; then
  fail 'malformed digest marker passed'
fi
printf 'curated release body\n  <!-- nika-ghcr-digest: %s -->\n' \
  "$DIGEST" >"$RELEASE_BODY"
if env "${release_env[@]}" \
  bash "$ROOT/scripts/release/release-digest-marker.sh" read \
  supernovae-st/nika 123 "$TAG" \
  2222222222222222222222222222222222222222 >/dev/null 2>&1; then
  fail 'indented digest marker passed as absent'
fi
printf 'curated prose naming nika-ghcr-digest: %s\n' "$DIGEST" >"$RELEASE_BODY"
if env "${release_env[@]}" \
  bash "$ROOT/scripts/release/release-digest-marker.sh" read \
  supernovae-st/nika 123 "$TAG" \
  2222222222222222222222222222222222222222 >/dev/null 2>&1; then
  fail 'embedded digest marker token passed as absent'
fi
printf 'curated release body\n<!-- nika-ghcr-digest: %s -->\n<!-- nika-ghcr-digest: malformed -->\n' \
  "$DIGEST" >"$RELEASE_BODY"
if env "${release_env[@]}" \
  bash "$ROOT/scripts/release/release-digest-marker.sh" read \
  supernovae-st/nika 123 "$TAG" \
  2222222222222222222222222222222222222222 >/dev/null 2>&1; then
  fail 'duplicate marker-prefix lines passed'
fi
cp "$TEST_ROOT/valid-release-body" "$RELEASE_BODY"
if env "${release_env[@]}" \
  bash "$ROOT/scripts/release/release-digest-marker.sh" stage \
  supernovae-st/nika 123 "$TAG" \
  2222222222222222222222222222222222222222 "$OTHER_DIGEST" >/dev/null 2>&1; then
  fail 'digest marker drift passed'
fi
printf 'v9.9.8\n' >"${RELEASE_BODY}.tag"
if env "${release_env[@]}" \
  bash "$ROOT/scripts/release/read-release-state.sh" supernovae-st/nika 123 \
  "$TAG" 2222222222222222222222222222222222222222 >/dev/null 2>&1; then
  fail 'stale release tag state passed'
fi
printf '%s\n' "$TAG" >"${RELEASE_BODY}.tag"

# The finalizer's complete proof and exact PATCH decision table have their own
# executable regression; the pointer checks below model an already-public tag.
printf 'false\n' >"$RELEASE_DRAFT"

# Both floating pointers must refuse an old tag even after it is public.
printf '123\tv9.9.9\n124\tv10.0.0\n' >"$RELEASE_LIST"
if env "${release_env[@]}" \
  bash "$ROOT/scripts/release/assert-newest-public-stable.sh" \
  supernovae-st/nika 123 "$TAG" \
  2222222222222222222222222222222222222222 >/dev/null 2>&1; then
  fail 'old stable tag passed the floating-pointer downgrade guard'
fi
printf '123\tv9.9.9\n' >"$RELEASE_LIST"
env "${release_env[@]}" \
  bash "$ROOT/scripts/release/assert-newest-public-stable.sh" \
  supernovae-st/nika 123 "$TAG" \
  2222222222222222222222222222222222222222 >/dev/null

# A stable replay after publication heals a failed latest job by digest and
# becomes a no-op once converged. Unknown credential-helper failures still
# refuse before the write.
cat >"$BIN/docker" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [ "$1 $2 $3" = 'buildx imagetools inspect' ]; then
  state="$(cat "$POINTER_STATE")"
  case "$state" in
    absent) echo 'manifest unknown' >&2; exit 1 ;;
    credential-helper)
      echo 'error getting credentials - exec: "docker-credential-pass": executable file not found' >&2
      exit 1
      ;;
    mixed)
      echo '500 Internal Server Error; 401 Unauthorized; manifest unknown (404 Not Found)' >&2
      exit 1
      ;;
    equal) printf '"%s"\n' "$POINTER_TARGET" ;;
    old) printf '"sha256:%064d"\n' 6 ;;
  esac
  exit 0
fi
if [ "$1 $2 $3" = 'buildx imagetools create' ]; then
  printf 'equal\n' >"$POINTER_STATE"
  printf 'create\n' >>"$POINTER_LOG"
  [ "${POINTER_COMMIT_THEN_ERROR:-0}" = 1 ] && exit 1
  exit 0
fi
exit 90
EOF
chmod +x "$BIN/docker"
POINTER_STATE="$TEST_ROOT/pointer-state"
POINTER_LOG="$TEST_ROOT/pointer-log"
: >"$POINTER_LOG"
printf 'old\n' >"$POINTER_STATE"
PATH="$BIN:$PATH" POINTER_STATE="$POINTER_STATE" POINTER_LOG="$POINTER_LOG" \
  POINTER_TARGET="$DIGEST" POINTER_COMMIT_THEN_ERROR=1 \
  bash "$ROOT/scripts/release/converge-oci-pointer.sh" \
  ghcr.io/supernovae-st/nika latest "$DIGEST" >/dev/null
[ "$(wc -l <"$POINTER_LOG" | tr -d ' ')" = 1 ] \
  || fail 'public stable replay did not heal latest exactly once'
PATH="$BIN:$PATH" POINTER_STATE="$POINTER_STATE" POINTER_LOG="$POINTER_LOG" \
  POINTER_TARGET="$DIGEST" \
  bash "$ROOT/scripts/release/converge-oci-pointer.sh" \
  ghcr.io/supernovae-st/nika latest "$DIGEST" >/dev/null
[ "$(wc -l <"$POINTER_LOG" | tr -d ' ')" = 1 ] \
  || fail 'equal latest replay wrote again'
printf 'credential-helper\n' >"$POINTER_STATE"
if PATH="$BIN:$PATH" POINTER_STATE="$POINTER_STATE" POINTER_LOG="$POINTER_LOG" \
  POINTER_TARGET="$DIGEST" \
  bash "$ROOT/scripts/release/converge-oci-pointer.sh" \
  ghcr.io/supernovae-st/nika latest "$DIGEST" >/dev/null 2>&1; then
  fail 'unknown latest lookup error granted pointer write authority'
fi
[ "$(wc -l <"$POINTER_LOG" | tr -d ' ')" = 1 ] \
  || fail 'unknown latest lookup error mutated the pointer'
printf 'mixed\n' >"$POINTER_STATE"
if PATH="$BIN:$PATH" POINTER_STATE="$POINTER_STATE" POINTER_LOG="$POINTER_LOG" \
  POINTER_TARGET="$DIGEST" \
  bash "$ROOT/scripts/release/converge-oci-pointer.sh" \
  ghcr.io/supernovae-st/nika latest "$DIGEST" >/dev/null 2>&1; then
  fail 'mixed latest 500/unauthorized/manifest-unknown granted write authority'
fi
[ "$(wc -l <"$POINTER_LOG" | tr -d ' ')" = 1 ] \
  || fail 'mixed latest lookup mutated the pointer'

echo 'publication-barrier.test: PASS'
