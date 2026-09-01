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
LOCAL="$TEST_ROOT/assets"
REMOTE="$TEST_ROOT/remote"
BIN="$TEST_ROOT/bin"
LOG="$TEST_ROOT/log"
mkdir -p "$LOCAL" "$REMOTE" "$BIN"
: >"$LOG"

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

cat >"$BIN/gh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [ "$1" = api ]; then
  if [ "${GH_LOOKUP:-ok}" = unknown ]; then
    echo 'gh: timeout' >&2
    exit 1
  fi
  if printf '%s\n' "$*" | grep -Fq '.assets | length'; then
    find "$REMOTE" -maxdepth 1 -type f | wc -l | tr -d ' '
  else
    find "$REMOTE" -maxdepth 1 -type f -exec basename {} \; | sort
    [ "${GH_DUPLICATE:-0}" = 1 ] && printf '%s\n' "${GH_DUPLICATE_NAME}"
  fi
  exit 0
fi
if [ "$1 $2" = 'release download' ]; then
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
  exit 0
fi
if [ "$1 $2" = 'release upload' ]; then
  asset="${*: -1}"
  cp "$asset" "$REMOTE/$(basename "$asset")"
  printf 'upload %s\n' "$(basename "$asset")" >>"$LOG"
  if [ "${GH_COMMIT_THEN_FAIL:-}" = "$(basename "$asset")" ]; then
    exit 1
  fi
  exit 0
fi
echo "unexpected gh: $*" >&2
exit 90
EOF
chmod +x "$BIN/gh"

# First publication writes each exact name once; identical replay writes none.
PATH="$BIN:$PATH" REMOTE="$REMOTE" LOG="$LOG" \
  bash "$ROOT/scripts/release/release-assets-barrier.sh" \
  stage "$TAG" supernovae-st/nika "${assets[@]}" >/dev/null
[ "$(wc -l <"$LOG" | tr -d ' ')" = 8 ] || fail 'first publish did not upload eight assets'
PATH="$BIN:$PATH" REMOTE="$REMOTE" LOG="$LOG" \
  bash "$ROOT/scripts/release/release-assets-barrier.sh" \
  stage "$TAG" supernovae-st/nika "${assets[@]}" >/dev/null
[ "$(wc -l <"$LOG" | tr -d ' ')" = 8 ] || fail 'identical replay uploaded again'

# Every individual missing asset is healable after all occupied identities pass.
for name in "${names[@]}"; do
  rm "$REMOTE/$name"
  PATH="$BIN:$PATH" REMOTE="$REMOTE" LOG="$LOG" \
    bash "$ROOT/scripts/release/release-assets-barrier.sh" \
    stage "$TAG" supernovae-st/nika "${assets[@]}" >/dev/null
  cmp -s "$LOCAL/$name" "$REMOTE/$name" || fail "missing heal failed for $name"
done

# Divergence and extras fail before a missing identity is healed.
rm "$REMOTE/${names[0]}"
printf 'divergent\n' >"$REMOTE/${names[1]}"
if PATH="$BIN:$PATH" REMOTE="$REMOTE" LOG="$LOG" \
  bash "$ROOT/scripts/release/release-assets-barrier.sh" \
  stage "$TAG" supernovae-st/nika "${assets[@]}" >/dev/null 2>&1; then
  fail 'divergent occupied asset passed'
fi
[ ! -e "$REMOTE/${names[0]}" ] || fail 'missing asset healed before divergence refusal'
cp "$LOCAL/${names[1]}" "$REMOTE/${names[1]}"
printf 'extra\n' >"$REMOTE/extra.bin"
if PATH="$BIN:$PATH" REMOTE="$REMOTE" LOG="$LOG" \
  bash "$ROOT/scripts/release/release-assets-barrier.sh" \
  stage "$TAG" supernovae-st/nika "${assets[@]}" >/dev/null 2>&1; then
  fail 'extra public asset passed'
fi
rm "$REMOTE/extra.bin"
cp "$LOCAL/${names[0]}" "$REMOTE/${names[0]}"
if GH_DUPLICATE=1 GH_DUPLICATE_NAME="${names[0]}" PATH="$BIN:$PATH" \
  REMOTE="$REMOTE" LOG="$LOG" bash "$ROOT/scripts/release/release-assets-barrier.sh" \
  verify "$TAG" supernovae-st/nika "${assets[@]}" >/dev/null 2>&1; then
  fail 'duplicate public identity passed'
fi

# A same-tag concurrent publisher may commit then report failure; equality is
# re-queried and the public identity is still written only once.
rm "$REMOTE/${names[0]}"
before="$(grep -Fc "upload ${names[0]}" "$LOG" || true)"
GH_COMMIT_THEN_FAIL="${names[0]}" PATH="$BIN:$PATH" REMOTE="$REMOTE" LOG="$LOG" \
  bash "$ROOT/scripts/release/release-assets-barrier.sh" \
  stage "$TAG" supernovae-st/nika "${assets[@]}" >/dev/null
after="$(grep -Fc "upload ${names[0]}" "$LOG" || true)"
[ "$((after - before))" = 1 ] || fail 'concurrent publish was attempted more than once'

# Tag peeling and movement are judged before writes.
cat >"$BIN/git" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [ "$1" = ls-remote ]; then
  printf '%s\trefs/tags/v9.9.9\n' "${TAG_OBJECT:-1111111111111111111111111111111111111111}"
  printf '%s\trefs/tags/v9.9.9^{}\n' "${TAG_SHA:-2222222222222222222222222222222222222222}"
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

# Draft creation happens only after an explicit 404; an unknown lookup cannot
# be converted into a create. This is also the pre-barrier failure proof.
cat >"$BIN/gh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [ "$1" = api ]; then
  if [ "${RELEASE_LOOKUP:-404}" = unknown ]; then
    echo 'gh: upstream unavailable (HTTP 500)' >&2
    exit 1
  fi
  if [ ! -e "$RELEASE_STATE" ]; then
    echo 'gh: Not Found (HTTP 404)' >&2
    exit 1
  fi
  if printf '%s\n' "$*" | grep -Fq '@tsv'; then
    printf '123\ttrue\tfalse\t2222222222222222222222222222222222222222\n'
  else
    printf '123\n'
  fi
  exit 0
fi
if [ "$1 $2" = 'release create' ]; then
  : >"$RELEASE_STATE"
  printf 'create\n' >>"$RELEASE_LOG"
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
rm "$RELEASE_STATE"
if RELEASE_LOOKUP=unknown PATH="$BIN:$PATH" RELEASE_STATE="$RELEASE_STATE" \
  RELEASE_LOG="$RELEASE_LOG" bash "$ROOT/scripts/release/prepare-draft-release.sh" \
  "$TAG" supernovae-st/nika "$NOTES" \
  2222222222222222222222222222222222222222 >/dev/null 2>&1; then
  fail 'unknown release lookup created a draft'
fi
[ "$(wc -l <"$RELEASE_LOG" | tr -d ' ')" = 1 ] || fail 'unknown release lookup mutated state'

# npm: equal, divergent, explicit E404, unknown lookup, absent verification,
# successful first publish, and an error whose publish nevertheless committed.
NPM_TGZ="$TEST_ROOT/package.tgz"
NPM_SHA="$NPM_TGZ.sha256"
printf 'npm bytes\n' >"$NPM_TGZ"
(cd "$TEST_ROOT" && sha256sum package.tgz >package.tgz.sha256)
NPM_SRI="sha512-$(openssl dgst -sha512 -binary "$NPM_TGZ" | base64 | tr -d '\n')"
cat >"$BIN/npm" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
state="$(cat "$NPM_STATE")"
if [ "$1" = view ]; then
  case "$state" in
    equal|committed) printf '%s\n' "$NPM_SRI" ;;
    divergent) echo 'sha512-wrong' ;;
    absent) echo 'npm ERR! code E404' >&2; exit 1 ;;
    unknown) echo 'npm ERR! code E500' >&2; exit 1 ;;
  esac
  exit 0
fi
if [ "$1" = publish ]; then
  printf 'publish\n' >>"$NPM_LOG"
  printf 'committed\n' >"$NPM_STATE"
  [ "${NPM_PUBLISH_ERROR:-0}" = 1 ] && exit 1
  exit 0
fi
exit 90
EOF
cat >"$BIN/sleep" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
chmod +x "$BIN/npm" "$BIN/sleep"
NPM_STATE="$TEST_ROOT/npm-state"
NPM_LOG="$TEST_ROOT/npm-log"
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
  NODE_AUTH_TOKEN=test bash "$ROOT/scripts/release/npm-publish-immutable.sh" publish \
  x "$NPM_TGZ" "$NPM_SHA" >/dev/null
[ "$(wc -l <"$NPM_LOG" | tr -d ' ')" = 1 ] || fail 'npm first publish count differs'
printf 'absent\n' >"$NPM_STATE"
PATH="$BIN:$PATH" NPM_STATE="$NPM_STATE" NPM_LOG="$NPM_LOG" NPM_SRI="$NPM_SRI" \
  NPM_PUBLISH_ERROR=1 NODE_AUTH_TOKEN=test \
  bash "$ROOT/scripts/release/npm-publish-immutable.sh" publish x \
  "$NPM_TGZ" "$NPM_SHA" >/dev/null
[ "$(wc -l <"$NPM_LOG" | tr -d ' ')" = 2 ] || fail 'npm committed publish error retried publish'

# OCI: absent/equal/divergent and label drift. The fake exposes exactly two
# platforms and mutates only the version coordinate on imagetools create.
cat >"$BIN/docker" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [ "$1 $2 $3" != 'buildx imagetools inspect' ] \
  && [ "$1 $2 $3" != 'buildx imagetools create' ]; then exit 90; fi
if [ "$3" = create ]; then
  printf 'equal\n' >"$OCI_STATE"
  printf 'create\n' >>"$OCI_LOG"
  exit 0
fi
ref="$4"
state="$(cat "$OCI_STATE")"
if [[ "$ref" == *:9.9.9 ]] && [ "$state" = absent ]; then
  echo 'manifest unknown' >&2
  exit 1
fi
if [ "${5:-}" = --raw ]; then
  printf '%s\n' '{"manifests":[{"platform":{"os":"linux","architecture":"amd64"}},{"platform":{"os":"linux","architecture":"arm64"}}]}'
  exit 0
fi
if printf '%s\n' "$*" | grep -Fq '.Manifest.Digest'; then
  case "$state" in divergent) printf '"sha256:%064d"\n' 9 ;; *) printf '"sha256:%064d"\n' 1 ;; esac
  exit 0
fi
revision="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
[ "$state" = label-drift ] && revision=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
printf '{"org.opencontainers.image.revision":"%s","org.opencontainers.image.version":"9.9.9","org.opencontainers.image.source":"https://github.com/supernovae-st/nika","org.opencontainers.image.licenses":"AGPL-3.0-or-later"}\n' "$revision"
EOF
chmod +x "$BIN/docker"
OCI_STATE="$TEST_ROOT/oci-state"
OCI_LOG="$TEST_ROOT/oci-log"
: >"$OCI_LOG"
CANDIDATE="sha256:$(printf '%064d' 1)"
printf 'absent\n' >"$OCI_STATE"
PATH="$BIN:$PATH" OCI_STATE="$OCI_STATE" OCI_LOG="$OCI_LOG" \
  bash "$ROOT/scripts/release/oci-coordinate-immutable.sh" publish \
  ghcr.io/supernovae-st/nika 9.9.9 "$CANDIDATE" \
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa \
  https://github.com/supernovae-st/nika >/dev/null
[ "$(wc -l <"$OCI_LOG" | tr -d ' ')" = 1 ] || fail 'OCI absent coordinate was not created once'
PATH="$BIN:$PATH" OCI_STATE="$OCI_STATE" OCI_LOG="$OCI_LOG" \
  bash "$ROOT/scripts/release/oci-coordinate-immutable.sh" verify \
  ghcr.io/supernovae-st/nika 9.9.9 - \
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa \
  https://github.com/supernovae-st/nika >/dev/null
printf 'divergent\n' >"$OCI_STATE"
if PATH="$BIN:$PATH" OCI_STATE="$OCI_STATE" OCI_LOG="$OCI_LOG" \
  bash "$ROOT/scripts/release/oci-coordinate-immutable.sh" publish \
  ghcr.io/supernovae-st/nika 9.9.9 "$CANDIDATE" \
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa \
  https://github.com/supernovae-st/nika >/dev/null 2>&1; then
  fail 'OCI divergent version passed'
fi
printf 'label-drift\n' >"$OCI_STATE"
if PATH="$BIN:$PATH" OCI_STATE="$OCI_STATE" OCI_LOG="$OCI_LOG" \
  bash "$ROOT/scripts/release/oci-coordinate-immutable.sh" verify \
  ghcr.io/supernovae-st/nika 9.9.9 - \
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa \
  https://github.com/supernovae-st/nika >/dev/null 2>&1; then
  fail 'OCI label drift passed'
fi

echo 'publication-barrier.test: PASS'
