#!/usr/bin/env bash
# End-to-end fake-CLI proof of the write-only GitHub finalization barrier.
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
SHA=2222222222222222222222222222222222222222
DIGEST="sha256:$(printf '%064d' 7)"
mkdir -p "$BIN" "$ARTIFACTS" "$REMOTE"

make_assets() {
  local version="$1"
  rm -f "$ARTIFACTS"/* "$REMOTE"/*
  for platform in linux-x64 linux-arm64 macos-x64 macos-arm64; do
    printf 'native bytes for %s\n' "$platform" \
      >"$ARTIFACTS/nika-${platform}-${version}.tar.gz"
  done
  (cd "$ARTIFACTS" && sha256sum nika-*.tar.gz | LC_ALL=C sort >SHA256SUMS)
  printf 'tag-bound provenance\n' >"$ARTIFACTS/multiple.intoto.jsonl"
  printf 'npm package bytes for %s\n' "$version" \
    >"$ARTIFACTS/supernovae-st-nika-check-wasm-${version}.tgz"
  (cd "$ARTIFACTS" && sha256sum \
    "supernovae-st-nika-check-wasm-${version}.tgz" \
    >"supernovae-st-nika-check-wasm-${version}.tgz.sha256")
  cp "$ARTIFACTS"/* "$REMOTE/"
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
asset_id() {
  printf '%s' "$1" | cksum | awk '{ print $1 }'
}
[ "$1" = api ] || { echo "unexpected gh: $*" >&2; exit 90; }
shift
method=GET
if [ "${1:-}" = --method ]; then method="$2"; shift 2; fi
endpoint="$1"
shift
if [ "$method" = PATCH ]; then
  printf '%s\n' "$*" >>"$PATCH_LOG"
  # Body/state-only updates can orphan a draft. The real 0.118.5 run
  # supplies the counterexample; explicitly bound writes preserve the tag.
  patched_tag=untagged-fixture
  for arg in "$@"; do
    case "$arg" in tag_name=*) patched_tag="${arg#tag_name=}" ;; esac
  done
  printf '%s\n' "${POST_PATCH_TAG:-$patched_tag}" >"${RELEASE_BODY}.tag"
  printf 'false\n' >"$RELEASE_DRAFT"
  [ "${FINALIZE_COMMIT_THEN_ERROR:-0}" != 1 ] || exit 1
  exit 0
fi
if [[ "$endpoint" == */releases/123 ]]; then
  [ "${STATE_MODE:-ok}" != fail ] || exit 1
  if printf '%s\n' "$*" | grep -Fq '.body'; then
    cat "$RELEASE_BODY"
  else
    actual_tag="$RELEASE_TAG"
    [ ! -f "${RELEASE_BODY}.tag" ] || actual_tag="$(cat "${RELEASE_BODY}.tag")"
    printf '123\t%s\t%s\t%s\n' \
      "$actual_tag" "$(cat "$RELEASE_DRAFT")" "$RELEASE_PRERELEASE"
  fi
  exit 0
fi
if [[ "$endpoint" == */releases/123/assets ]]; then
  if [ "${ASSET_MODE:-full}" != zero ]; then
    while IFS= read -r name; do
      [ -n "$name" ] && printf '%s\t%s\n' "$(asset_id "$name")" "$name"
    done < <(find "$REMOTE" -maxdepth 1 -type f -exec basename {} \; | LC_ALL=C sort)
    if [ "${ASSET_MODE:-full}" = delete-after-list ]; then
      rm -f "$REMOTE/nika-linux-arm64-${RELEASE_TAG#v}.tar.gz"
    fi
  fi
  exit 0
fi
case "$endpoint" in
  */releases/assets/*)
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
echo "unexpected gh api: $endpoint $*" >&2
exit 90
EOF

chmod +x "$BIN"/*

run_finalizer() {
  local asset_mode="$1"
  local tap_ready="$2"
  printf '%s\n' "$RELEASE_TAG" >"${RELEASE_BODY}.tag"
  env PATH="$BIN:$PATH" REMOTE="$REMOTE" PATCH_LOG="$PATCH_LOG" \
    RELEASE_BODY="$RELEASE_BODY" RELEASE_DRAFT="$RELEASE_DRAFT" \
    RELEASE_TAG="$RELEASE_TAG" RELEASE_PRERELEASE="$RELEASE_PRERELEASE" \
    RELEASE_SHA="$SHA" ASSET_MODE="$asset_mode" \
    STATE_MODE="${STATE_MODE:-ok}" \
    POST_PATCH_TAG="${POST_PATCH_TAG:-}" \
    FINALIZE_COMMIT_THEN_ERROR="${FINALIZE_COMMIT_THEN_ERROR:-0}" \
    bash "$ROOT/scripts/release/finalize-release.sh" \
    supernovae-st/nika 123 "$RELEASE_TAG" "$SHA" "$DIGEST" \
    "$ARTIFACTS" "$tap_ready"
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
  if run_finalizer zero true >"$TEST_ROOT/zero-${draft}.out" 2>&1; then
    fail ".assets=0 passed for draft=${draft}"
  fi
  [ ! -s "$PATCH_LOG" ] || fail ".assets=0 reached PATCH for draft=${draft}"
  ! grep -q '^transitioned=' "$TEST_ROOT/zero-${draft}.out" \
    || fail ".assets=0 reported success for draft=${draft}"
done

# Deletion after the initial remote-name read refuses both paths.
for draft in true false; do
  make_assets 9.9.9
  printf '%s\n' "$draft" >"$RELEASE_DRAFT"
  : >"$PATCH_LOG"
  if run_finalizer delete-after-list true \
    >"$TEST_ROOT/deleted-${draft}.out" 2>&1; then
    fail "asset deletion passed for draft=${draft}"
  fi
  [ ! -s "$PATCH_LOG" ] || fail "asset deletion reached PATCH for draft=${draft}"
  ! grep -q '^transitioned=' "$TEST_ROOT/deleted-${draft}.out" \
    || fail "asset deletion reported success for draft=${draft}"
done

# A bad checksum manifest, marker drift, and release read failure each refuse
# both success paths even when all eight asset names and bytes otherwise match.
for failure in checksum marker state; do
  for draft in true false; do
    make_assets 9.9.9
    printf '<!-- nika-ghcr-digest: %s -->\n' "$DIGEST" >"$RELEASE_BODY"
    STATE_MODE=ok
    case "$failure" in
      checksum)
        printf '%064d  nika-linux-arm64-9.9.9.tar.gz\n' 0 \
          >"$ARTIFACTS/SHA256SUMS"
        cp "$ARTIFACTS/SHA256SUMS" "$REMOTE/SHA256SUMS"
        ;;
      marker)
        printf '<!-- nika-ghcr-digest: sha256:%064d -->\n' 8 \
          >"$RELEASE_BODY"
        ;;
      state) STATE_MODE=fail ;;
    esac
    export STATE_MODE
    printf '%s\n' "$draft" >"$RELEASE_DRAFT"
    : >"$PATCH_LOG"
    if run_finalizer full true \
      >"$TEST_ROOT/${failure}-${draft}.out" 2>&1; then
      fail "${failure} failure passed for draft=${draft}"
    fi
    [ ! -s "$PATCH_LOG" ] \
      || fail "${failure} failure reached PATCH for draft=${draft}"
    ! grep -q '^transitioned=' "$TEST_ROOT/${failure}-${draft}.out" \
      || fail "${failure} failure reported success for draft=${draft}"
  done
done
unset STATE_MODE

# Already-public success is available only after the mutable proof reruns.
make_assets 9.9.9
printf '<!-- nika-ghcr-digest: %s -->\n' "$DIGEST" >"$RELEASE_BODY"
printf 'false\n' >"$RELEASE_DRAFT"
: >"$PATCH_LOG"
result="$(run_finalizer full false)"
[ "$result" = 'transitioned=false' ] \
  || fail 'fully proven public replay did not validate'
[ ! -s "$PATCH_LOG" ] || fail 'public validation replay patched the release'

# Stable draft publication refuses a false tap-readiness boolean.
printf 'true\n' >"$RELEASE_DRAFT"
: >"$PATCH_LOG"
if run_finalizer full false >"$TEST_ROOT/tap-not-ready.out" 2>&1; then
  fail 'stable draft passed without tap readiness'
fi
[ ! -s "$PATCH_LOG" ] || fail 'missing tap readiness reached PATCH'

# Stable draft publication uses the exact non-regressing Latest policy.
printf 'true\n' >"$RELEASE_DRAFT"
: >"$PATCH_LOG"
FINALIZE_COMMIT_THEN_ERROR="1"
export FINALIZE_COMMIT_THEN_ERROR
result="$(run_finalizer full true)"
unset FINALIZE_COMMIT_THEN_ERROR
[ "$result" = 'transitioned=true' ] || fail 'fully proven stable draft did not publish'
grep -Fqx -- "-f tag_name=$RELEASE_TAG -f target_commitish=$SHA -F draft=false -f discussion_category_name=Announcements -f make_latest=legacy" \
  "$PATCH_LOG" || fail 'stable finalizer PATCH arguments drifted'

# A successful write response is not permission to accept a different tag.
printf 'true\n' >"$RELEASE_DRAFT"
: >"$PATCH_LOG"
POST_PATCH_TAG=v9.9.8
if run_finalizer full true >"$TEST_ROOT/post-patch-drift.out" 2>&1; then
  fail 'finalizer accepted post-write release identity drift'
fi
unset POST_PATCH_TAG
[ "$(wc -l <"$PATCH_LOG" | tr -d ' ')" = 1 ] \
  || fail 'finalizer retried an observed identity drift'
grep -Fq 'REFUSED tag v9.9.8' "$TEST_ROOT/post-patch-drift.out" \
  || fail 'finalizer did not identify post-write tag drift'
! grep -q '^transitioned=' "$TEST_ROOT/post-patch-drift.out" \
  || fail 'finalizer claimed publication after tag drift'

# Prerelease publication explicitly refuses Latest selection.
make_assets 9.9.9-rc.1
RELEASE_TAG=v9.9.9-rc.1
RELEASE_PRERELEASE=true
printf '<!-- nika-ghcr-digest: %s -->\n' "$DIGEST" >"$RELEASE_BODY"
printf 'true\n' >"$RELEASE_DRAFT"
: >"$PATCH_LOG"
result="$(run_finalizer full false)"
[ "$result" = 'transitioned=true' ] || fail 'fully proven prerelease draft did not publish'
grep -Fqx -- "-f tag_name=$RELEASE_TAG -f target_commitish=$SHA -F draft=false -f discussion_category_name=Announcements -f make_latest=false" \
  "$PATCH_LOG" || fail 'prerelease finalizer PATCH arguments drifted'

echo 'finalize-release.test: PASS'
