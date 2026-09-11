#!/usr/bin/env bash
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
# COVERS: scripts/hygiene/check-clone-armed.sh scripts/dev/bootstrap.sh
# The arming probes use Git's effective hook location, including worktrees.
# All config, repositories and the fake installer live in a disposable HOME.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHECK="$HERE/../check-clone-armed.sh"
BOOTSTRAP="$HERE/../../dev/bootstrap.sh"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT
mkdir -p "$SCRATCH/home" "$SCRATCH/bin"
GIT_BIN="$(command -v git)"
ln -s "$GIT_BIN" "$SCRATCH/bin/git"
TEST_PATH="$SCRATCH/bin:/usr/bin:/bin"

fixture_env() {
  env -i PATH="$TEST_PATH" HOME="$SCRATCH/home" \
    GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null \
    GIT_AUTHOR_NAME=Fixture GIT_AUTHOR_EMAIL=fixture@example.invalid \
    GIT_COMMITTER_NAME=Fixture GIT_COMMITTER_EMAIL=fixture@example.invalid \
    TEST_INSTALL_LOG="$SCRATCH/installs" TEST_INSTALL_NOOP="${INSTALL_NOOP:-0}" "$@"
}

cat >"$SCRATCH/bin/lefthook" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
[ "$1" = install ]
printf 'install\n' >>"$TEST_INSTALL_LOG"
[ "$TEST_INSTALL_NOOP" = 0 ] || exit 0
hook=$(git rev-parse --git-path hooks/pre-commit)
mkdir -p "$(dirname "$hook")"
printf '#!/usr/bin/env bash\nexit 0\n' >"$hook"
chmod +x "$hook"
MOCK
chmod +x "$SCRATCH/bin/lefthook"

fail() {
  printf 'FAIL: %s\n' "$1" >&2
  cat "$SCRATCH/output" >&2
  exit 1
}

run_script() {
  local repo="$1" script="$2" expected="$3" label="$4" status=0
  (cd "$repo" && fixture_env bash "$script") >"$SCRATCH/output" 2>&1 || status=$?
  [ "$status" -eq "$expected" ] || fail "$label: expected exit $expected, got $status"
}

install_count() {
  if [ -f "$SCRATCH/installs" ]; then
    wc -l <"$SCRATCH/installs" | tr -d ' '
  else
    echo 0
  fi
}

cases=0
for layout in clone worktree; do
  for location in default relative absolute; do
    label="$layout/$location"
    origin="$SCRATCH/$layout-$location origin"
    fixture_env git init -q "$origin"
    fixture_env git -C "$origin" commit -q --allow-empty -m fixture
    repo="$origin"
    if [ "$layout" = worktree ]; then
      repo="$SCRATCH/$layout-$location linked tree"
      fixture_env git -C "$origin" worktree add -q --detach "$repo"
      [ -f "$repo/.git" ] || fail "$label must exercise a gitfile"
    fi
    printf 'pre-commit:\n  commands:\n    proof:\n      run: true\n' >"$repo/lefthook.yml"
    fixture_env git -C "$repo" config merge.ours.driver true
    case "$location" in
      relative) fixture_env git -C "$repo" config core.hooksPath 'custom hooks' ;;
      absolute) fixture_env git -C "$repo" config core.hooksPath "$SCRATCH/$layout absolute hooks" ;;
    esac
    hook="$(cd "$repo" && fixture_env git rev-parse --git-path hooks/pre-commit)"
    case "$hook" in
      /*) ;;
      *) hook="$repo/$hook" ;;
    esac
    if [ "$location" != default ]; then
      # An executable in the default directory cannot rescue a missing
      # core.hooksPath hook: Git does not fall back to that directory.
      printf '#!/usr/bin/env bash\nexit 0\n' >"$origin/.git/hooks/pre-commit"
      chmod +x "$origin/.git/hooks/pre-commit"
    fi
    run_script "$repo" "$CHECK" 1 "$label missing hook"
    mkdir -p "$(dirname "$hook")"
    printf '#!/usr/bin/env bash\nexit 0\n' >"$hook"
    chmod 644 "$hook"
    run_script "$repo" "$CHECK" 1 "$label non-executable hook"
    chmod +x "$hook"
    run_script "$repo" "$CHECK" 0 "$label executable hook"
    fixture_env git -C "$repo" config --unset merge.ours.driver
    run_script "$repo" "$CHECK" 1 "$label missing driver"
    grep -q 'merge.ours.driver' "$SCRATCH/output" || fail "$label driver diagnostic"

    before="$(install_count)"
    run_script "$repo" "$BOOTSTRAP" 0 "$label bootstrap existing hook"
    [ "$(install_count)" = "$before" ] || fail "$label reinstalled an executable hook"
    [ "$(fixture_env git -C "$repo" config --get merge.ours.driver)" = true ] || fail "$label driver not armed"
    run_script "$repo" "$CHECK" 0 "$label bootstrap armed"

    for state in missing non-executable; do
      if [ "$state" = missing ]; then rm "$hook"; else chmod 644 "$hook"; fi
      before="$(install_count)"
      run_script "$repo" "$BOOTSTRAP" 0 "$label bootstrap $state hook"
      [ "$(install_count)" -eq "$((before + 1))" ] || fail "$label $state installer not called"
      [ -f "$hook" ] && [ -x "$hook" ] || fail "$label $state hook not armed"
      run_script "$repo" "$CHECK" 0 "$label $state repaired"
    done

    rm "$hook"
    INSTALL_NOOP=1 run_script "$repo" "$BOOTSTRAP" 0 "$label no-op installer"
    if grep -q 'gates now reachable' "$SCRATCH/output"; then fail "$label installer exit zero fabricated arming"; fi
    grep -q 'FAILED' "$SCRATCH/output" || fail "$label unarmed installer not diagnosed"
    run_script "$repo" "$CHECK" 1 "$label no-op stays unarmed"
    cases=$((cases + 1))
    printf 'ok: %s (missing/non-executable/effective hook, driver, bootstrap repair and no-op)\n' "$label"
  done
done
printf 'clone-armed: %s layouts passed\n' "$cases"
