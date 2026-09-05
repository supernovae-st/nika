#!/usr/bin/env bash
# The draft-release lookup against a GitHub that hides drafts the way GitHub
# does: the by-tag endpoint answers 404 for a draft, the release list carries
# it. v0.118.0 died on that 404 right after creating its own draft (2026-09-04);
# this proof would have read RED on the tree that shipped it.
# shellcheck disable=SC2016  # the mutant's sed patterns and greps quote ${repo}/${tag} on purpose
set -euo pipefail

unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_PREFIX GIT_COMMON_DIR \
  GIT_OBJECT_DIRECTORY GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_NAMESPACE \
  GIT_QUARANTINE_PATH

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
TEST_ROOT="$(mktemp -d)"
trap 'rm -rf "$TEST_ROOT"' EXIT

fail() {
  printf 'draft-release.test: %s\n' "$1" >&2
  exit 1
}

TAG=v9.9.9
REPO=example/nika
SHA=2222222222222222222222222222222222222222
BIN="$TEST_ROOT/bin"
STATE="$TEST_ROOT/state" # absent · draft:<id> · published:<id>
LOG="$TEST_ROOT/log"
NOTES="$TEST_ROOT/notes.md"
mkdir -p "$BIN"
printf 'notes\n' >"$NOTES"

# The fake `git`: only `ls-remote` is asked (resolve-release-tag.sh), and it
# answers the tag at its expected commit.
cat >"$BIN/git" <<EOF
#!/usr/bin/env bash
case "\$1" in
  ls-remote)
    printf '%s\trefs/tags/%s\n%s\trefs/tags/%s^{}\n' "$SHA" "$TAG" "$SHA" "$TAG"
    ;;
  *) echo "fake git: unexpected \$*" >&2; exit 97 ;;
esac
EOF
chmod +x "$BIN/git"

# The fake `gh`, shaped like GitHub: the by-tag endpoint sees PUBLISHED
# releases only; the list can lag a committed draft; POST returns its id;
# a release read by id answers its own stored state, not the requested id.
cat >"$BIN/gh" <<EOF
#!/usr/bin/env bash
state="\$(cat "$STATE" 2>/dev/null || echo absent)"
kind="\${state%%:*}"
id="\${state#*:}"
printf '%s\n' "\$*" >>"$LOG"
if [ "\$1" = api ]; then
  path="\$2"
  case "\$path" in
    repos/$REPO/releases/tags/$TAG)
      if [ "\$kind" = published ]; then printf '%s\n' "\$id"; exit 0; fi
      echo "gh: Not Found (HTTP 404)" >&2; exit 1 ;;
    repos/$REPO/releases)
      case " \$* " in
        *" --method POST "*)
          if [ -n "\${GH_CREATE_FAIL:-}" ]; then echo "gh: create unavailable (HTTP 503)" >&2; exit 1; fi
          printf 'draft:%s\n' "\${GH_NEW_ID:-777}" >"$STATE"
          printf '%s\n' "\${GH_CREATE_RESPONSE_ID-\${GH_NEW_ID:-777}}"
          exit 0 ;;
      esac
      if [ -n "\${GH_LIST_FAIL:-}" ]; then echo "gh: Unauthorized (HTTP 401)" >&2; exit 1; fi
      if [ -n "\${GH_HIDE_DRAFT_IN_LIST:-}" ] && [ "\$kind" = draft ]; then exit 0; fi
      if [ "\$kind" != absent ]; then printf '%s\n' "\$id"; fi
      exit 0 ;;
    repos/$REPO/releases/[0-9]*)
      rid="\${path##*/}"
      if [ "\$rid" != "\$id" ] || [ "\$kind" = absent ]; then echo "gh: Not Found (HTTP 404)" >&2; exit 1; fi
      draft=false; [ "\$kind" = draft ] && draft=true
      printf '%s\t%s\t%s\tfalse\n' "\$rid" "$TAG" "\$draft"; exit 0 ;;
    *) echo "fake gh: unexpected api \$path" >&2; exit 97 ;;
  esac
fi
echo "fake gh: unexpected \$*" >&2; exit 97
EOF
chmod +x "$BIN/gh"

run_script() {
  PATH="$BIN:$PATH" GH_TOKEN=test bash "$ROOT/scripts/release/prepare-draft-release.sh" \
    "$TAG" "$REPO" "$NOTES" "$SHA"
}

# 1 · nothing exists: the draft is created and its response id is retained.
printf 'absent\n' >"$STATE"
: >"$LOG"
out="$(GH_NEW_ID=4242 run_script)" || fail "create path exited $?"
grep -qx 'created=true' <<<"$out" || fail "create path did not report created=true: $out"
grep -qx 'id=4242' <<<"$out" || fail "create path did not find the draft it created: $out"
grep -qx 'draft=true' <<<"$out" || fail "create path did not read the draft state: $out"
grep -q -- '--method POST' "$LOG" || fail "create path never created the draft"
grep -q -- '--field draft=true' "$LOG" || fail "creation is not draft-only"
grep -q -- '--field generate_release_notes=true' "$LOG" || fail "generated notes were lost"
grep -Fq -- "--raw-field target_commitish=$SHA" "$LOG" || fail "creation routing was not pinned"
grep -Fq -- "--field body=@$NOTES" "$LOG" || fail "curated notes were lost"

# A create has committed, but the list still hides the new draft. The POST
# response's immutable id, not immediate list visibility, owns the next read.
printf 'absent\n' >"$STATE"
: >"$LOG"
out="$(GH_NEW_ID=4243 GH_HIDE_DRAFT_IN_LIST=1 run_script)" || fail "a committed draft depended on immediate list visibility"
grep -qx 'id=4243' <<<"$out" || fail "create response identity was not retained"
grep -qx 'created=true' <<<"$out" || fail "committed draft was not named as created"
[ "$(grep -c -- '--paginate' "$LOG")" = 1 ] || fail "a post-create list read is still required"

# Malformed or foreign create ids do not become a published/reusable object.
for response in '' null 0 wrong 999; do
  printf 'absent\n' >"$STATE"
  : >"$LOG"
  if GH_NEW_ID=4243 GH_CREATE_RESPONSE_ID="$response" run_script >"$TEST_ROOT/out" 2>"$TEST_ROOT/err"; then
    fail "invalid create response id was accepted: $response"
  fi
  [ "$(grep -c -- '--method POST' "$LOG")" = 1 ] || fail "invalid response retried creation"
done
printf 'absent\n' >"$STATE"
: >"$LOG"
if GH_CREATE_FAIL=1 run_script >"$TEST_ROOT/out" 2>"$TEST_ROOT/err"; then
  fail "a failed create was accepted"
fi
grep -qx 'absent' "$STATE" || fail "a failed create mutated the fixture"

# 2 · a DRAFT already carries the tag (a replay after a dead train): reused,
#     never created twice.
printf 'draft:555\n' >"$STATE"
: >"$LOG"
out="$(run_script)" || fail "draft reuse exited $?"
grep -qx 'created=false' <<<"$out" || fail "draft reuse reported a creation: $out"
grep -qx 'id=555' <<<"$out" || fail "draft reuse did not carry the draft's id: $out"
grep -qx 'draft=true' <<<"$out" || fail "draft reuse did not read draft=true: $out"
if grep -q -- '--method POST' "$LOG"; then fail "draft reuse created a second draft"; fi

# 3 · a PUBLISHED release carries the tag (a validation-only replay): reused.
printf 'published:31\n' >"$STATE"
: >"$LOG"
out="$(run_script)" || fail "published reuse exited $?"
grep -qx 'created=false' <<<"$out" || fail "published reuse reported a creation: $out"
grep -qx 'draft=false' <<<"$out" || fail "published reuse did not read draft=false: $out"

# 4 · the list call fails (not an absence): the barrier refuses, nothing is created.
printf 'absent\n' >"$STATE"
: >"$LOG"
if GH_LIST_FAIL=1 run_script >/dev/null 2>"$TEST_ROOT/err"; then
  fail "a failed list answer was read as an absence"
fi
grep -q 'release barrier' "$TEST_ROOT/err" || fail "the list failure did not name the barrier"
if grep -q -- '--method POST' "$LOG"; then fail "the barrier created a draft on a failed lookup"; fi

# 5 · the mutation: a lookup through the by-tag endpoint cannot see the draft it
#     just created — the shape that killed v0.118.0 must read RED here.
mutant="$TEST_ROOT/mutant.sh"
sed 's|gh api "repos/${repo}/releases" --paginate \\|gh api "repos/${repo}/releases/tags/${tag}" \\|; s|    --jq "\[.\[\] \| select(.tag_name == \\"${tag}\\")\] \| map(.id) \| first // empty" \\|    --jq ".id" \\|' \
  "$ROOT/scripts/release/prepare-draft-release.sh" >"$mutant"
grep -q 'releases/tags/${tag}' "$mutant" || fail "the mutant was not produced"
cp "$mutant" "$TEST_ROOT/prepare-draft-release.sh"
cp "$ROOT/scripts/release/check-release-tag.sh" "$ROOT/scripts/release/resolve-release-tag.sh" \
  "$ROOT/scripts/release/read-release-state.sh" "$TEST_ROOT/"
printf 'absent\n' >"$STATE"
if PATH="$BIN:$PATH" GH_TOKEN=test GH_NEW_ID=4242 bash "$TEST_ROOT/prepare-draft-release.sh" \
  "$TAG" "$REPO" "$NOTES" "$SHA" >/dev/null 2>&1; then
  fail "the by-tag lookup mutant passed: the fake GitHub does not hide drafts"
fi

# Reintroducing an immediate list lookup after POST must also read RED.
sed '/^state="$(bash/i\
release_id="$(lookup_release_id)"\
' "$ROOT/scripts/release/prepare-draft-release.sh" >"$TEST_ROOT/prepare-draft-release.sh"
printf 'absent\n' >"$STATE"
if PATH="$BIN:$PATH" GH_TOKEN=test GH_NEW_ID=4243 GH_HIDE_DRAFT_IN_LIST=1 \
  bash "$TEST_ROOT/prepare-draft-release.sh" "$TAG" "$REPO" "$NOTES" "$SHA" \
  >/dev/null 2>&1; then
  fail "the immediate-list-after-create mutant passed"
fi

echo "draft-release.test: PASS · committed POST identity · delayed list · malformed/foreign ids · create failure · reuse · lookup barrier · two lookup mutants read RED"
