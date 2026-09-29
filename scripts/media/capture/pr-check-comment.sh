#!/usr/bin/env bash
# pr-check-comment.sh — capture the sticky comment supernovae-st/nika-action
# posts on a pull request, for two pushes of one workflow: a push with a
# finding, then the push that fixes it.
#
# The action cannot run on this machine (it is a GitHub composite action),
# so its three steps are replayed exactly as its action.yml runs them:
#
#   nika check "$WORKFLOW" --json          → check.json, exit code kept
#   nika inspect "$WORKFLOW" --format mermaid → graph.mmd (emptied when
#                                            inspect fails, as the action does)
#   python3 scripts/render_comment.py …    → comment.md, the comment body
#
# render_comment.py is the action's own script, read from a nika-action
# checkout (NIKA_ACTION, default: a sibling of this repository). The
# script first checks that action.yml still runs these exact commands,
# so a change in the action fails the capture instead of drifting from it.
#
# The two pushes, in a scratch repository at flows/pr-risk-review.nika:
#   red   · scripts/media/fixtures/fixed-pr-review.nika with its `comment`
#           task reading `tasks.asses` (the typo broken-pr-review.nika
#           carries); `nika check` must refuse it (exit 2, NIKA-DAG-002)
#   clean · scripts/media/fixtures/fixed-pr-review.nika as committed; it
#           must pass (exit 0)
# Everything runs offline in a scratch directory by relative names, so no
# path of this machine reaches a capture. Re-running rewrites the same bytes.
#
# Usage · NIKA_ACTION=/path/to/nika-action bash scripts/media/capture/pr-check-comment.sh
# Output · media/raw/pr-check-comment-{red,clean}.nika   the file at each push
#          media/raw/pr-check-comment-{red,clean}.json   nika check --json
#          media/raw/pr-check-comment-{red,clean}.mmd    graph.mmd as the action keeps it
#          media/raw/pr-check-comment-{red,clean}.md     the comment the action posts
#          media/raw/pr-check-comment-run.json           exit codes, the action's
#                                                        step names, script hashes
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT"

RAW="media/raw"
FIX="scripts/media/fixtures"
ACTION="${NIKA_ACTION:-$ROOT/../nika-action}"
WORKFLOW="flows/pr-risk-review.nika"
mkdir -p "$RAW"

fail() {
  echo "FATAL: $*" >&2
  exit 1
}

command -v nika >/dev/null || fail "nika binary not found on PATH"
command -v python3 >/dev/null || fail "python3 not found (the action's renderer needs it)"
if [ ! -f "$ACTION/action.yml" ] || [ ! -f "${ACTION}/scripts/render_comment.py" ]; then
  fail "no nika-action checkout at $ACTION (set NIKA_ACTION)"
fi
ACTION="$(cd "$ACTION" && pwd)"

# ── the action still runs what this replay runs ─────────────────────────
# These are action.yml's own lines, matched literally: nothing in them is
# meant to expand here.
# shellcheck disable=SC2016,SC1003
for pinned in \
  'flags=(--json)' \
  'nika check "${WORKFLOW}" "${flags[@]}" > "${RUNNER_TEMP}/check.json" 2> "${RUNNER_TEMP}/check.stderr"' \
  'if ! nika inspect "${WORKFLOW}" --format mermaid > "${RUNNER_TEMP}/graph.mmd" 2> "${RUNNER_TEMP}/graph.stderr"; then' \
  ': > "${RUNNER_TEMP}/graph.mmd"' \
  'python3 "${GITHUB_ACTION_PATH}/scripts/render_comment.py" \' \
  '--check-json "${RUNNER_TEMP}/check.json" \' \
  '--check-exit "${{ steps.check.outputs.exit }}" \' \
  '--workflow "${WORKFLOW}" \' \
  '--mermaid "${RUNNER_TEMP}/graph.mmd" \' \
  '--trace-verdict "${TRACE_VERDICT}" \' \
  '--engine-version "${ENGINE_VERSION}" \' \
  '--out "${RUNNER_TEMP}/comment.md"'; do
  grep -qF -- "$pinned" "$ACTION/action.yml" \
    || fail "action.yml no longer runs: $pinned — update this replay"
done

# the engine version the action passes to the renderer is the release it
# installed: here, the binary on PATH
VERSION="$(nika --version | awk '{print $2}')"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$WORK/flows"

# ── the two pushes ──────────────────────────────────────────────────────
sed 's/tasks\.assess\.output\.risk/tasks.asses.output.risk/' "$FIX/fixed-pr-review.nika" >"$WORK/red.nika"
cp "$FIX/fixed-pr-review.nika" "$WORK/clean.nika"
changed="$(diff "$WORK/red.nika" "$WORK/clean.nika" | grep -c '^[<>]' || true)"
[ "$changed" = "2" ] || fail "the red push must differ from the fixture by one line (got $changed changed lines)"

# replay the action's steps on one push; prints the two exit codes
replay() {
  local state="$1" check_rc=0 inspect_rc=0
  cp "$WORK/$state.nika" "$WORK/$WORKFLOW"
  (
    cd "$WORK"
    nika check "$WORKFLOW" --json >check.json 2>check.stderr || check_rc=$?
    # the action keeps no graph when the projector fails
    nika inspect "$WORKFLOW" --format mermaid >graph.mmd 2>graph.stderr || inspect_rc=$?
    [ "$inspect_rc" = "0" ] || : >graph.mmd
    # TRACE_VERDICT is empty in the default check mode (no golden lane)
    python3 "${ACTION}/scripts/render_comment.py" \
      --check-json check.json \
      --check-exit "$check_rc" \
      --workflow "$WORKFLOW" \
      --mermaid graph.mmd \
      --trace-verdict "" \
      --engine-version "$VERSION" \
      --out comment.md >/dev/null
    [ ! -s check.stderr ] || fail "nika check wrote to stderr on the $state push: $(head -c 300 check.stderr)"
    cp "$WORKFLOW" "$ROOT/$RAW/pr-check-comment-$state.nika"
    cp check.json "$ROOT/$RAW/pr-check-comment-$state.json"
    cp graph.mmd "$ROOT/$RAW/pr-check-comment-$state.mmd"
    cp comment.md "$ROOT/$RAW/pr-check-comment-$state.md"
    echo "$check_rc $inspect_rc"
  )
}

read -r red_check red_inspect < <(replay red)
read -r clean_check clean_inspect < <(replay clean)

# ── the story the clip tells must still be true ─────────────────────────
[ "$red_check" = "2" ] || fail "the red push must fail nika check with exit 2 (got $red_check)"
[ "$clean_check" = "0" ] || fail "the clean push must pass nika check (got exit $clean_check)"
head -1 "$RAW/pr-check-comment-red.md" | grep -q '^❌ \*\*nika check\*\* — 1 finding(s)' \
  || fail "the red comment no longer opens with one finding"
# shellcheck disable=SC2016 # the backticks are the comment's markdown
grep -qF '| conformance | `NIKA-DAG-002` |' "$RAW/pr-check-comment-red.md" \
  || fail "the red comment no longer names NIKA-DAG-002"
head -1 "$RAW/pr-check-comment-clean.md" | grep -q '^✅ \*\*nika check\*\* — clean' \
  || fail "the clean comment is not clean"
grep -q '^```mermaid$' "$RAW/pr-check-comment-clean.md" \
  || fail "the clean comment carries no DAG"

# exit codes, the action's step names and the hashes of the files replayed
python3 - "$ACTION" "$RAW/pr-check-comment-run.json" "$WORKFLOW" "$VERSION" \
  "$red_check" "$red_inspect" "$clean_check" "$clean_inspect" <<'PY'
import hashlib, json, pathlib, re, sys

action, out, workflow, version, rc, ri, cc, ci = sys.argv[1:]
yml = pathlib.Path(action, "action.yml").read_text(encoding="utf-8")
sha = lambda p: "sha256:" + hashlib.sha256(pathlib.Path(action, p).read_bytes()).hexdigest()
run = {
    "workflow": workflow,
    "engine": version,
    "pushes": {
        "red": {"check_exit": int(rc), "inspect_exit": int(ri)},
        "clean": {"check_exit": int(cc), "inspect_exit": int(ci)},
    },
    "action": {
        "name": re.search(r"^name: '(.+)'$", yml, re.M).group(1),
        "steps": re.findall(r"^    - name: (.+)$", yml, re.M),
        "action.yml": sha("action.yml"),
        "scripts/render_comment.py": sha("scripts/render_comment.py"),  # upstream: nika-action's file
    },
}
pathlib.Path(out).write_text(json.dumps(run, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
PY

echo "pr-check-comment captured: red exit $red_check · clean exit $clean_check"
