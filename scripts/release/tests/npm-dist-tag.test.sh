#!/usr/bin/env bash
# A prerelease is published under npm's `next` dist-tag, a stable version under
# npm's default one, and neither is ever forced. The fake npm refuses an
# untagged prerelease the way npm 11 does, and refuses `--force` outright.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
TEST_ROOT="$(mktemp -d)"
trap 'rm -r "$TEST_ROOT"' EXIT
mkdir -p "$TEST_ROOT/bin" "$TEST_ROOT/state"
printf 'fixture\n' >"$TEST_ROOT/fixture.tgz"
(cd "$TEST_ROOT" && shasum -a 256 fixture.tgz >fixture.tgz.sha256)
INTEGRITY="sha512-$(openssl dgst -sha512 -binary "$TEST_ROOT/fixture.tgz" | base64 | tr -d '\n')"
cat >"$TEST_ROOT/bin/npm" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
case "$1" in
  view)
    if [ -e "$STATE/published" ]; then
      printf '%s\n' "$INTEGRITY"
      exit 0
    fi
    echo 'npm error code E404' >&2
    exit 1
    ;;
  publish)
    shift
    printf '%s\n' "$*" >"$STATE/publish-args"
    tagged=false
    for arg in "$@"; do
      case "$arg" in
        --force) echo 'npm error: this fixture refuses --force' >&2; exit 91 ;;
        --tag) tagged=true ;;
      esac
    done
    if [ "$PRERELEASE" = true ] && [ "$tagged" = false ]; then
      echo 'npm error You must specify a tag using --tag when publishing a prerelease version.' >&2
      exit 1
    fi
    : >"$STATE/published"
    ;;
  *) exit 90 ;;
esac
EOF
cat >"$TEST_ROOT/bin/sha256sum" <<'EOF'
#!/usr/bin/env bash
exec shasum -a 256 "$@"
EOF
cat >"$TEST_ROOT/bin/sleep" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
chmod +x "$TEST_ROOT/bin/"*
fail() {
  cat "$TEST_ROOT/output" >&2 || true
  echo "npm-dist-tag.test: $1" >&2
  exit 1
}
publish() {
  local version="$1" prerelease="$2"
  rm -f "$TEST_ROOT/state/"*
  PATH="$TEST_ROOT/bin:$PATH" STATE="$TEST_ROOT/state" INTEGRITY="$INTEGRITY" \
    PRERELEASE="$prerelease" NIKA_NPM_READINESS_SECONDS=10 \
    ACTIONS_ID_TOKEN_REQUEST_URL=https://oidc.test ACTIONS_ID_TOKEN_REQUEST_TOKEN=test \
    bash "$ROOT/npm-publish-immutable.sh" publish "@test/fixture@${version}" \
    "$TEST_ROOT/fixture.tgz" "$TEST_ROOT/fixture.tgz.sha256" >"$TEST_ROOT/output" 2>&1
}
publish 9.9.9-preview.1 true || fail 'a prerelease did not publish'
grep -Fq -- '--tag next' "$TEST_ROOT/state/publish-args" \
  || fail 'a prerelease was not published under the next dist-tag'
publish 9.9.9 false || fail 'a stable version did not publish'
if grep -Fq -- '--tag' "$TEST_ROOT/state/publish-args"; then
  fail 'a stable version left the default dist-tag'
fi
echo 'npm-dist-tag.test: a prerelease publishes under next, a stable version under the default tag'
