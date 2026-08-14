#!/usr/bin/env bash
# Mutation proof for the pre-commit private-path gate.
#
# The gate reads `git diff --cached`, so every case here is a real staged
# change in a throwaway repo: write the line, stage it, run the gate,
# assert the colour.
#
# BOTH directions are pinned, because this gate has failed in both. It
# missed five private trees under the agent substrate and two whole
# ventures (an inventory that the tree had moved out from under), while
# flagging `lmstudio/` — a public provider path — as a leak. A gate that
# blocks everything is exactly as useless as one that blocks nothing, so
# the GREEN cases below are not decoration; they are half the proof.
#
# No private tree is spelled out here. This file ships in a PUBLIC repo,
# and an inventory of private names is the very leak the gate exists to
# stop. Every venture case uses an INVENTED name — which is the stronger
# proof anyway: the gate matches a shape, so a venture nobody has created
# yet is covered without editing a line of this file.
set -uo pipefail

# Git hooks export GIT_DIR, GIT_INDEX_FILE and friends, and vector 49 runs
# this file from inside the pre-push gate. Inherited, those variables win
# over `git -C`: the throwaway repos below would resolve to the REAL one,
# and `git add -A` would stage into the repo's own index instead of the
# scratch one. Measured on 2026-08-14 — it replaced a 4382-entry index
# with a single phantom entry. The working tree was untouched and `git
# reset` restored it, but a test that can do that from a hook is a
# liability, not a proof. Clear the inheritance once, here.
unset GIT_DIR GIT_INDEX_FILE GIT_WORK_TREE GIT_COMMON_DIR GIT_NAMESPACE \
  GIT_OBJECT_DIRECTORY GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_PREFIX

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
VECTOR="$ROOT/scripts/hooks/block-private-paths.sh"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

fails=0
cases=0

# The guarded roots are COMPOSED, never spelled on an executable line.
# Two reasons, and both are this repo's own rules. Vector 44 reads a
# literal `dx/…` or `docs/…` in a script as a claim that the file exists;
# these are payload strings the gate must FLAG, not files, and five of
# them showed up as stale path references the day this test landed.
# `${VAR}/` is the exemption that vector documents. And this file ships in
# a PUBLIC repo, where an inventory of private roots is the very leak the
# gate exists to stop — which is why block-private-paths.sh composes too.
DX='dx'
DOCS='docs'
SCRIPTS='scripts'

# expect <RED|GREEN> <label> <payload-line> [staged-path]
expect() {
  local want="$1" label="$2" payload="$3" rel="${4:-${DOCS}/note.md}"
  cases=$((cases + 1))
  local dir="$WORK/case-$cases"
  mkdir -p "$dir/$(dirname "$rel")"
  git -C "$dir" init -q
  printf '%s\n' "$payload" >"$dir/$rel"
  git -C "$dir" add -A
  (cd "$dir" && bash "$VECTOR" >/dev/null 2>&1)
  local rc=$?
  local got="RED"
  [ "$rc" -eq 0 ] && got="GREEN"
  if [ "$got" = "$want" ]; then
    printf '  ok   %s (%s, rc=%d)\n' "$label" "$got" "$rc"
  else
    fails=$((fails + 1))
    printf '  FAIL %s — wanted %s, got %s (rc=%d)\n' "$label" "$want" "$got" "$rc" >&2
  fi
}

echo "private-path gate · mutation proof"

# --- must BLOCK ------------------------------------------------------------
# Invented venture names: the gate matches the shape, not a list.
expect RED "a venture pole, invented venture" \
  'see ventures/example-co/01-product/roadmap.md for the plan'
expect RED "another invented venture, another pole" \
  'ventures/second-example/04-identity/brand.md'
expect RED "a venture that will not exist until tomorrow" \
  'ventures/not-created-yet/08-chronicle/launch.md'
# The agent substrate, matched at its ROOT — an invented child proves the
# gate no longer depends on knowing the children's names.
expect RED "agent substrate, invented child" \
  "per ${DX}/example-tree/thing.yaml"
expect RED "agent substrate, another invented child" \
  "the ledger at ${DX}/other-example/ledgers/x.yaml"
expect RED "the studio tree" \
  'per studio/07-operations/north-star/VISION.md'
expect RED "private memory" \
  'lives in .claude/projects/some-slug/memory/'
expect RED "a pre-migration spelling (frozen citation)" \
  'the old path nika/hq/strategy.md'
# The case a line-level filter would have missed: one line naming the
# PUBLIC tier and a private pole together. Matching per occurrence catches
# the private half instead of excusing the whole line for the innocent one.
expect RED "public tier and a private pole on one line" \
  'ventures/nika/02-engineering/repos/engine/README.md vs ventures/x-co/01-product/a.md'

# --- must NOT block --------------------------------------------------------
# The false positive that made the gate block legitimate work: unanchored
# `studio/` matched every LM Studio provider path.
expect GREEN "lmstudio provider path" \
  'model: lmstudio/qwen2.5-coder'
expect GREEN "lm-studio cache path" \
  'models live in ~/.cache/lm-studio/models'
expect GREEN "the brand URL, not the studio tree" \
  'contact us at https://supernovae.studio/ for details'
# The one public tier: this repo's own home.
expect GREEN "the public repos tier" \
  'ventures/nika/02-engineering/repos/engine/src/lib.rs'
expect GREEN "ordinary prose with no path at all" \
  'the engine ships a workflow language'
# Substring collisions the boundary anchor must reject.
expect GREEN "a word merely ending in the guarded root" \
  'the sdx/ shim and the index/ dir are fine'
# The gate deliberately does not scan its own kind — those files
# legitimately enumerate the patterns.
expect GREEN "a guarded path inside a self-excluded dir" \
  "${DX}/example-tree/thing.yaml" "${SCRIPTS}/ci/some-check.sh"

if [ "$fails" -gt 0 ]; then
  printf '\n%d/%d case(s) wrong — the gate does not block what it claims,\nor blocks what it must not.\n' \
    "$fails" "$cases" >&2
  exit 1
fi

printf '\n%d/%d cases correct in both directions.\n' "$cases" "$cases"
exit 0
