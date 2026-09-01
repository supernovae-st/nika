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
  if [ "${RELEASE_LOOKUP:-404}" = mixed ]; then
    echo 'gh: upstream unavailable (HTTP 500); auth (HTTP 401) unauthorized; secondary route (HTTP 404)' >&2
    exit 1
  fi
  endpoint="$2"
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
# target_commitish is creation routing metadata, not an existing release's
# identity. A release whose API target is the branch name must still bind by
# immutable release ID, exact tag/prerelease, and repeatedly resolved tag SHA.
[ -z "$(rg -n 'target_commitish' "$ROOT/scripts/release/prepare-draft-release.sh" || true)" ] \
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
    mixed) echo 'npm ERR! code E500; code E401 unauthorized; secondary npm ERR! code E404' >&2; exit 1 ;;
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
printf 'mixed\n' >"$NPM_STATE"
if PATH="$BIN:$PATH" NPM_STATE="$NPM_STATE" NPM_LOG="$NPM_LOG" NPM_SRI="$NPM_SRI" \
  NODE_AUTH_TOKEN=test bash "$ROOT/scripts/release/npm-publish-immutable.sh" \
  publish x "$NPM_TGZ" "$NPM_SHA" >/dev/null 2>&1; then
  fail 'mixed npm 500/E404/unauthorized granted publish authority'
fi
[ "$(wc -l <"$NPM_LOG" | tr -d ' ')" = 2 ] || fail 'mixed npm lookup reached publish'

# OCI: absent/equal/divergent and label drift. The fake exposes exactly two
# platforms and mutates only the version coordinate on imagetools create.
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
if [[ "$ref" == *:9.9.9 ]] && [ "$state" = credential-helper ]; then
  echo 'error getting credentials - exec: "docker-credential-pass": executable file not found' >&2
  exit 1
fi
if [[ "$ref" == *:9.9.9 ]] && [ "$state" = mixed ]; then
  echo 'unexpected status 500 Internal Server Error; 401 Unauthorized; secondary: manifest unknown (404 Not Found)' >&2
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

printf 'credential-helper\n' >"$OCI_STATE"
if PATH="$BIN:$PATH" OCI_STATE="$OCI_STATE" OCI_LOG="$OCI_LOG" \
  bash "$ROOT/scripts/release/oci-coordinate-immutable.sh" publish \
  ghcr.io/supernovae-st/nika 9.9.9 "$CANDIDATE" \
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa \
  https://github.com/supernovae-st/nika >/dev/null 2>&1; then
  fail 'credential-helper not-found error was classified as registry absence'
fi
[ "$(wc -l <"$OCI_LOG" | tr -d ' ')" = 1 ] \
  || fail 'unknown OCI lookup error reached a write'
printf 'mixed\n' >"$OCI_STATE"
if PATH="$BIN:$PATH" OCI_STATE="$OCI_STATE" OCI_LOG="$OCI_LOG" \
  bash "$ROOT/scripts/release/oci-coordinate-immutable.sh" publish \
  ghcr.io/supernovae-st/nika 9.9.9 "$CANDIDATE" \
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa \
  https://github.com/supernovae-st/nika >/dev/null 2>&1; then
  fail 'mixed OCI 500/unauthorized/manifest-unknown granted create authority'
fi
[ "$(wc -l <"$OCI_LOG" | tr -d ' ')" = 1 ] || fail 'mixed OCI lookup reached a write'

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
  if printf '%s\n' "$*" | grep -Fq 'draft=false'; then
    printf '%s\n' "$*" >"$FINALIZE_LOG"
    printf 'false\n' >"$RELEASE_DRAFT"
    [ "${FINALIZE_COMMIT_THEN_ERROR:-0}" = 1 ] && exit 1
    exit 0
  fi
  body_arg=""
  while [ "$#" -gt 0 ]; do
    case "$1" in
      -f) body_arg="$2"; shift 2 ;;
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
    printf '123\t%s\t%s\t%s\n' "$RELEASE_TAG" "$(cat "$RELEASE_DRAFT")" "$RELEASE_PRERELEASE"
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
if env "${release_env[@]}" RELEASE_TAG=v9.9.8 \
  bash "$ROOT/scripts/release/read-release-state.sh" supernovae-st/nika 123 \
  "$TAG" 2222222222222222222222222222222222222222 >/dev/null 2>&1; then
  fail 'stale release tag state passed'
fi

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
