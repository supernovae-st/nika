#!/usr/bin/env bash
# COVERS: scripts/release/changelog-assemble.sh
# Exercise the real fold in disposable repositories: a failed stat can emit
# stdout before the portable fallback succeeds (GNU stat -f on Linux).
set -euo pipefail

unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_PREFIX GIT_COMMON_DIR \
  GIT_OBJECT_DIRECTORY GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_NAMESPACE \
  GIT_QUARANTINE_PATH

HERE="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd -- "$HERE/../../.." && pwd)"
TEST_ROOT="$(mktemp -d)"
trap 'rm -rf -- "$TEST_ROOT"' EXIT
mkdir "$TEST_ROOT/tmp" "$TEST_ROOT/bin"
export TMPDIR="$TEST_ROOT/tmp"

# Model the observed partial-output failure on both macOS and Linux. The
# successful fallback reads the real permission bits, not a canned answer.
cat >"$TEST_ROOT/bin/stat" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
case "$1" in
  -f)
    printf 'filesystem metadata from a failed stat invocation\n'
    exit 1
    ;;
  -c)
    python3 - "$3" <<'PY'
import os
import stat
import sys
print(format(stat.S_IMODE(os.stat(sys.argv[1]).st_mode), "o"))
PY
    ;;
  *) exit 2 ;;
esac
EOF
chmod +x "$TEST_ROOT/bin/stat"

for variant in native partial-output; do
  fixture="$TEST_ROOT/$variant"
  mkdir -p "$fixture/changelog.d"
  cat >"$fixture/CHANGELOG.md" <<'EOF'
# Changelog

## [Unreleased]

One file per change under changelog.d/.

## [0.114.0] - 2026-08-23
EOF
  printf '%s\n' '# Changelog fragments' >"$fixture/changelog.d/README.md"
  printf '%s\n' '- **Permission preservation.** Fold one isolated fragment.' \
    >"$fixture/changelog.d/1.fixed.md"
  chmod 640 "$fixture/CHANGELOG.md"
  git -C "$fixture" init -q
  git -C "$fixture" add -- CHANGELOG.md changelog.d/README.md changelog.d/1.fixed.md

  fold_path="$PATH"
  if [ "$variant" = partial-output ]; then
    fold_path="$TEST_ROOT/bin:$PATH"
  fi
  PATH="$fold_path" SPN_CHANGELOG_REPO="$fixture" \
    bash "$ROOT/scripts/release/changelog-assemble.sh" --fold 0.115.0 --date 2026-09-12

  python3 - "$fixture/CHANGELOG.md" <<'PY'
import os
import stat
import sys
mode = stat.S_IMODE(os.stat(sys.argv[1]).st_mode)
assert mode == 0o640, f"fold changed permissions to {mode:o}"
PY
  grep -qF '## [0.115.0]' "$fixture/CHANGELOG.md"
  [ ! -e "$fixture/changelog.d/1.fixed.md" ]
  [ -z "$(git -C "$fixture" ls-files -- changelog.d/1.fixed.md)" ]
  printf 'changelog-mode.test: PASS · %s fold preserves mode 640\n' "$variant"
done
