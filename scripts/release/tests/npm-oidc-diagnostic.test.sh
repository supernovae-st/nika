#!/usr/bin/env bash
# Diagnose a failed npm publish without disclosing its private debug log.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/.." && pwd)"
TEST_ROOT="$(mktemp -d)"
trap 'rm -r "$TEST_ROOT"' EXIT
mkdir -p "$TEST_ROOT/bin"
printf 'fixture\n' >"$TEST_ROOT/fixture.tgz"
(cd "$TEST_ROOT" && shasum -a 256 fixture.tgz >fixture.tgz.sha256)
cat >"$TEST_ROOT/bin/npm" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [ "$1" = view ]; then
  echo 'npm error code E404' >&2
  exit 1
fi
[ "$1" = publish ] || exit 90
while [ "$#" -gt 0 ]; do
  if [ "$1" = --logs-dir ]; then
    printf '%s\n' "$2" >"$LOG_LOCATION"
    [ "$OIDC_MESSAGE" != missing ] || break
    mkdir -p "$2"
    printf '0 verbose oidc %s\n1 secret DO_NOT_PRINT_TEST_TOKEN\n' "$OIDC_MESSAGE" >"$2/test-debug-0.log"
    break
  fi
  shift
done
echo 'npm error code E404' >&2
exit 1
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
check_case() {
  local message="$1" expected="$2" rc=0
  PATH="$TEST_ROOT/bin:$PATH" OIDC_MESSAGE="$message" LOG_LOCATION="$TEST_ROOT/log-location" \
    ACTIONS_ID_TOKEN_REQUEST_URL=https://oidc.test \
    ACTIONS_ID_TOKEN_REQUEST_TOKEN=DO_NOT_PRINT_TEST_TOKEN \
    bash "$ROOT/npm-publish-immutable.sh" publish '@test/fixture@9.9.9' \
    "$TEST_ROOT/fixture.tgz" "$TEST_ROOT/fixture.tgz.sha256" \
    >"$TEST_ROOT/output" 2>&1 || rc=$?
  [ "$rc" -eq 69 ] || {
    cat "$TEST_ROOT/output"
    exit 1
  }
  grep -Fq "$expected" "$TEST_ROOT/output"
  [ ! -e "$(cat "$TEST_ROOT/log-location")" ] || {
    echo 'npm diagnostic retained its private debug log' >&2
    exit 1
  }
  if grep -Fq DO_NOT_PRINT_TEST_TOKEN "$TEST_ROOT/output"; then
    echo 'npm diagnostic disclosed private log content' >&2
    exit 1
  fi
}
check_case 'Successfully retrieved and set token' 'OIDC exchange succeeded; check that the trusted publisher allows direct npm publish'
check_case 'Failed token exchange request with body message: DO_NOT_PRINT_TEST_TOKEN' 'OIDC exchange was rejected; verify the trusted publisher organization'
check_case 'Failed to fetch id_token from GitHub: missing value' 'GitHub did not return an OIDC identity'
check_case 'Failure with message: DO_NOT_PRINT_TEST_TOKEN' 'OIDC exchange outcome unavailable'
check_case missing 'OIDC exchange outcome unavailable'
echo 'npm OIDC diagnostic: five refusal cases pass without log disclosure or retention'
