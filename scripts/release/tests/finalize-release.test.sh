#!/usr/bin/env bash
# End-to-end fake-CLI proof that the finalizer itself owns the full barrier.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
TEST_ROOT="$(mktemp -d)"
trap 'rm -r "$TEST_ROOT"' EXIT

fail() {
  echo "finalize-release.test: $1" >&2
  exit 1
}

BIN="$TEST_ROOT/bin"
ARTIFACTS="$TEST_ROOT/artifacts"
REMOTE="$TEST_ROOT/remote"
PATCH_LOG="$TEST_ROOT/patch-log"
RELEASE_BODY="$TEST_ROOT/release-body"
RELEASE_DRAFT="$TEST_ROOT/release-draft"
PAYLOAD_AMD64="$TEST_ROOT/payload-amd64/nika"
PAYLOAD_ARM64="$TEST_ROOT/payload-arm64/nika"
SHA=2222222222222222222222222222222222222222
DIGEST="sha256:$(printf '%064d' 7)"
IMAGE=ghcr.io/supernovae-st/nika
mkdir -p "$BIN" "$ARTIFACTS" "$REMOTE" \
  "$(dirname "$PAYLOAD_AMD64")" "$(dirname "$PAYLOAD_ARM64")"
printf 'linux amd64 binary\n' >"$PAYLOAD_AMD64"
printf 'linux arm64 binary\n' >"$PAYLOAD_ARM64"

make_assets() {
  local version="$1"
  rm -f "$ARTIFACTS"/* "$REMOTE"/*
  tar -czf "$ARTIFACTS/nika-linux-x64-${version}.tar.gz" \
    -C "$(dirname "$PAYLOAD_AMD64")" nika
  tar -czf "$ARTIFACTS/nika-linux-arm64-${version}.tar.gz" \
    -C "$(dirname "$PAYLOAD_ARM64")" nika
  tar -czf "$ARTIFACTS/nika-macos-x64-${version}.tar.gz" \
    -C "$(dirname "$PAYLOAD_AMD64")" nika
  tar -czf "$ARTIFACTS/nika-macos-arm64-${version}.tar.gz" \
    -C "$(dirname "$PAYLOAD_ARM64")" nika
  (cd "$ARTIFACTS" && sha256sum nika-*.tar.gz | LC_ALL=C sort >SHA256SUMS)
  printf 'tag-bound provenance\n' >"$ARTIFACTS/multiple.intoto.jsonl"
  printf 'npm package bytes for %s\n' "$version" \
    >"$ARTIFACTS/supernovae-st-nika-check-wasm-${version}.tgz"
  (cd "$ARTIFACTS" && sha256sum \
    "supernovae-st-nika-check-wasm-${version}.tgz" \
    >"supernovae-st-nika-check-wasm-${version}.tgz.sha256")
  cp "$ARTIFACTS"/* "$REMOTE/"
  NPM_SRI="sha512-$(openssl dgst -sha512 -binary \
    "$ARTIFACTS/supernovae-st-nika-check-wasm-${version}.tgz" \
    | base64 | tr -d '\n')"
}

cat >"$BIN/git" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [ "$1" = ls-remote ]; then
  tag="${4#refs/tags/}"
  printf '%s\trefs/tags/%s\n' 1111111111111111111111111111111111111111 "$tag"
  printf '%s\trefs/tags/%s^{}\n' "$RELEASE_SHA" "$tag"
  exit 0
fi
exit 90
EOF

cat >"$BIN/gh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [ "$1 $2" = 'attestation verify' ]; then
  [ "${FAILURE_MODE:-none}" != attestation ]
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
[ "$1" = api ] || { echo "unexpected gh: $*" >&2; exit 90; }
shift
method=GET
if [ "${1:-}" = --method ]; then method="$2"; shift 2; fi
endpoint="$1"
shift
if [ "$method" = PATCH ]; then
  printf '%s\n' "$*" >>"$PATCH_LOG"
  printf 'false\n' >"$RELEASE_DRAFT"
  [ "${FINALIZE_COMMIT_THEN_ERROR:-0}" != 1 ] || exit 1
  exit 0
fi
if [[ "$endpoint" == */releases/123 ]]; then
  if printf '%s\n' "$*" | grep -Fq '.body'; then
    cat "$RELEASE_BODY"
  else
    printf '123\t%s\t%s\t%s\n' \
      "$RELEASE_TAG" "$(cat "$RELEASE_DRAFT")" "$RELEASE_PRERELEASE"
  fi
  exit 0
fi
if [[ "$endpoint" == */releases/tags/* ]]; then
  if printf '%s\n' "$*" | grep -Fq '.assets | length'; then
    if [ "${ASSET_MODE:-full}" = zero ]; then
      printf '0\n'
    else
      find "$REMOTE" -maxdepth 1 -type f | wc -l | tr -d ' '
    fi
  elif [ "${ASSET_MODE:-full}" != zero ]; then
    find "$REMOTE" -maxdepth 1 -type f -exec basename {} \; | LC_ALL=C sort
  fi
  exit 0
fi
echo "unexpected gh api: $endpoint $*" >&2
exit 90
EOF

cat >"$BIN/npm" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
[ "$1" = view ] || exit 90
if [ "${FAILURE_MODE:-none}" = npm ]; then
  printf 'sha512-wrong\n'
else
  printf '%s\n' "$NPM_SRI"
fi
EOF

cat >"$BIN/slsa-verifier" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
[ "$1" = verify-artifact ]
[ "${FAILURE_MODE:-none}" != provenance ]
printf '%s\n' "$*" | grep -Fq -- "--source-tag $RELEASE_TAG"
EOF

cat >"$BIN/docker" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [ "$1 $2 $3" = 'buildx imagetools inspect' ]; then
  if [ "${5:-}" = --raw ]; then
    printf '%s\n' '{"manifests":[{"platform":{"os":"linux","architecture":"amd64"}},{"platform":{"os":"linux","architecture":"arm64"}}]}'
    exit 0
  fi
  if printf '%s\n' "$*" | grep -Fq '.Manifest.Digest'; then
    if [ "${FAILURE_MODE:-none}" = oci ]; then
      printf '"sha256:%064d"\n' 8
    else
      printf '"%s"\n' "$GHCR_DIGEST"
    fi
    exit 0
  fi
  printf '{"org.opencontainers.image.revision":"%s","org.opencontainers.image.version":"%s","org.opencontainers.image.source":"https://github.com/supernovae-st/nika","org.opencontainers.image.licenses":"AGPL-3.0-or-later"}\n' \
    "$RELEASE_SHA" "${RELEASE_TAG#v}"
  exit 0
fi
if [ "$1" = pull ]; then exit 0; fi
if [ "$1" = create ]; then
  case "$*" in
    *linux/amd64*) printf 'aaaaaaaaaaaa\n' ;;
    *linux/arm64*) printf 'bbbbbbbbbbbb\n' ;;
    *) exit 90 ;;
  esac
  exit 0
fi
if [ "$1" = cp ]; then
  case "$2" in
    aaaaaaaaaaaa:*) source="$PAYLOAD_AMD64" ;;
    bbbbbbbbbbbb:*) source="$PAYLOAD_ARM64" ;;
    *) exit 90 ;;
  esac
  [ "${FAILURE_MODE:-none}" != payload ] || source="$PAYLOAD_AMD64"
  cp "$source" "$3"
  exit 0
fi
if [ "$1" = rm ]; then exit 0; fi
exit 90
EOF
chmod +x "$BIN"/*

run_finalizer() {
  local failure="$1"
  local asset_mode="$2"
  env PATH="$BIN:$PATH" REMOTE="$REMOTE" PATCH_LOG="$PATCH_LOG" \
    RELEASE_BODY="$RELEASE_BODY" RELEASE_DRAFT="$RELEASE_DRAFT" \
    RELEASE_TAG="$RELEASE_TAG" RELEASE_PRERELEASE="$RELEASE_PRERELEASE" \
    RELEASE_SHA="$SHA" FAILURE_MODE="$failure" ASSET_MODE="$asset_mode" \
    NPM_SRI="$NPM_SRI" GHCR_DIGEST="$DIGEST" \
    PAYLOAD_AMD64="$PAYLOAD_AMD64" PAYLOAD_ARM64="$PAYLOAD_ARM64" \
    TAP_DEPLOY_KEY="${TAP_DEPLOY_KEY:-}" \
    FINALIZE_COMMIT_THEN_ERROR="${FINALIZE_COMMIT_THEN_ERROR:-0}" \
    bash "$ROOT/scripts/release/finalize-release.sh" \
    supernovae-st/nika 123 "$RELEASE_TAG" "$SHA" "$DIGEST" \
    "$ARTIFACTS" "$IMAGE"
}

make_assets 9.9.9
RELEASE_TAG=v9.9.9
RELEASE_PRERELEASE=false
printf '<!-- nika-ghcr-digest: %s -->\n' "$DIGEST" >"$RELEASE_BODY"

# An empty current GitHub asset set refuses both draft publication and an
# already-public validation success.
for draft in true false; do
  printf '%s\n' "$draft" >"$RELEASE_DRAFT"
  : >"$PATCH_LOG"
  if run_finalizer none zero >"$TEST_ROOT/zero-${draft}.out" 2>&1; then
    fail ".assets=0 passed for draft=${draft}"
  fi
  [ ! -s "$PATCH_LOG" ] || fail ".assets=0 reached PATCH for draft=${draft}"
  ! grep -q '^transitioned=' "$TEST_ROOT/zero-${draft}.out" \
    || fail ".assets=0 reported success for draft=${draft}"
done

# Every late proof failure refuses before either success path or PATCH.
TAP_DEPLOY_KEY="test"
export TAP_DEPLOY_KEY
for failure in provenance npm oci payload; do
  printf 'true\n' >"$RELEASE_DRAFT"
  : >"$PATCH_LOG"
  if run_finalizer "$failure" full \
    >"$TEST_ROOT/${failure}.out" 2>&1; then
    fail "wrong ${failure} proof passed"
  fi
  [ ! -s "$PATCH_LOG" ] || fail "wrong ${failure} proof reached PATCH"
  ! grep -q '^transitioned=' "$TEST_ROOT/${failure}.out" \
    || fail "wrong ${failure} proof reported success"
done

# Already-public success is available only after the complete proof reruns.
printf 'false\n' >"$RELEASE_DRAFT"
: >"$PATCH_LOG"
result="$(run_finalizer none full)"
[ "$result" = 'transitioned=false' ] \
  || fail 'fully proven public replay did not validate'
[ ! -s "$PATCH_LOG" ] || fail 'public validation replay PATCHed the release'

# Stable draft publication uses the exact non-regressing Latest policy.
printf 'true\n' >"$RELEASE_DRAFT"
: >"$PATCH_LOG"
FINALIZE_COMMIT_THEN_ERROR="1"
export FINALIZE_COMMIT_THEN_ERROR
result="$(run_finalizer none full)"
unset FINALIZE_COMMIT_THEN_ERROR
[ "$result" = 'transitioned=true' ] || fail 'fully proven stable draft did not publish'
grep -Fqx -- '-F draft=false -f discussion_category_name=Announcements -f make_latest=legacy' \
  "$PATCH_LOG" || fail 'stable finalizer PATCH arguments drifted'

# Prerelease publication explicitly refuses Latest selection.
make_assets 9.9.9-rc.1
RELEASE_TAG=v9.9.9-rc.1
RELEASE_PRERELEASE=true
printf '<!-- nika-ghcr-digest: %s -->\n' "$DIGEST" >"$RELEASE_BODY"
printf 'true\n' >"$RELEASE_DRAFT"
: >"$PATCH_LOG"
result="$(run_finalizer none full)"
[ "$result" = 'transitioned=true' ] || fail 'fully proven prerelease draft did not publish'
grep -Fqx -- '-F draft=false -f discussion_category_name=Announcements -f make_latest=false' \
  "$PATCH_LOG" || fail 'prerelease finalizer PATCH arguments drifted'

echo 'finalize-release.test: PASS'
