#!/usr/bin/env bash
# next-tag-project.sh — what would the next tag actually contain?
#
# Operator command. Answers BEFORE tagging, not after. If this had existed
# on 2026-08-08 it would have said: the CHANGELOG promises the harness ·
# release.yml does not build it.
#
# Reads (all derived, never typed) ·
#   release.yml          the features the downloadable binary is built with
#   wiring.yaml          declared capabilities + proven_by job keys
#   CHANGELOG.md         the [Unreleased] section
#   .github/workflows    CI job keys (the judge that would prove a claim)
#   estate.yaml          the manifest the tag gate will compare the tree to
#
# Prints four columns · IN THE BINARY · CLAIMED · PROVEN · UNPROVEN.
# Exit 0 always for the human projection. --check exits 1 when UNPROVEN
# is non-empty (the S5 tag ceremony can arm this later).
#
# Co-Authored-By: Nika 🦋 <nika@supernovae.studio>

set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
ENGINE="${SPN_WIRING_REPO:-$(cd -- "$SCRIPT_DIR/../.." && pwd)}"
CHECK=0
JSON=0

while [ $# -gt 0 ]; do
  case "$1" in
    --repo)
      ENGINE="${2:-}"
      shift 2
      ;;
    --check)
      CHECK=1
      shift
      ;;
    --json)
      JSON=1
      shift
      ;;
    -h | --help)
      sed -n '2,22p' "$0"
      exit 0
      ;;
    *)
      printf 'next-tag-project: unknown argument %s\n' "$1" >&2
      exit 2
      ;;
  esac
done

[ -d "$ENGINE" ] || {
  printf 'next-tag-project: no engine at %s · cannot judge\n' "$ENGINE" >&2
  exit 2
}
[ -f "$ENGINE/Cargo.toml" ] || {
  printf 'next-tag-project: no Cargo.toml at %s · cannot judge\n' "$ENGINE" >&2
  exit 2
}

# ── release build features (comments stripped first · a comment that names
#    --features metal is not a build line. Measured live in wiring-gate.)
RELEASE_FEATURES=""
RELEASE_WF=""
if [ -f "$ENGINE/.github/workflows/release.yml" ]; then
  RELEASE_WF=release.yml
  RELEASE_FEATURES="$(
    sed -E 's/^[[:space:]]*#.*$//' "$ENGINE/.github/workflows/release.yml" \
      | grep -hoE '\-\-features[= ][A-Za-z0-9_,-]+' \
      | sed -E 's/--features[= ]//' | tr ',' '\n' | sed 's/^ *//;s/ *$//' | sort -u || true
  )"
fi

# ── CI job keys
JOBS=""
if [ -d "$ENGINE/.github/workflows" ]; then
  JOBS="$(
    awk '
      $0 ~ /^jobs:[[:space:]]*$/ { in_jobs=1; next }
      in_jobs && $0 ~ /^[^[:space:]#]/ { in_jobs=0 }
      in_jobs && $0 ~ /^  [A-Za-z0-9_-]+:/ {
        sub(/:.*/, "", $1); print $1
      }
    ' "$ENGINE"/.github/workflows/*.yml "$ENGINE"/.github/workflows/*.yaml 2>/dev/null \
      | sort -u || true
  )"
fi

# ── wiring.yaml capabilities
LEDGER="$ENGINE/wiring.yaml"
CAP_IDS=""
if [ -f "$LEDGER" ]; then
  CAP_IDS="$(
    awk '
      /^capabilities:/{grab=1; next}
      grab && /^[a-z_]+:/ && $0 !~ /^[[:space:]]/{grab=0}
      grab && $0 ~ /^[[:space:]]+-[[:space:]]*id:[[:space:]]*/ {
        sub(/.*id:[[:space:]]*/, ""); gsub(/["\047]/, ""); id=$0
      }
      # A YAML value starts its line; a comment that names the key does
      # not (the #1218 class — the issue-proof ledger reader already
      # anchors; this one read proven_by inside a comment and judged a
      # markdown-wrapped code span as a job that is in no workflow).
      grab && $0 ~ /^[[:space:]]*proven_by:[[:space:]]*/ {
        sub(/^[[:space:]]*proven_by:[[:space:]]*/, ""); gsub(/["\047]/, "")
        job=$0
        if (job == "" || job == "null") print id "\t"
        else print id "\t" job
      }
    ' "$LEDGER"
  )"
fi

# ── CHANGELOG [Unreleased] · the SECTION *and* the fragments
#
# The claims moved out of `## [Unreleased]` into `changelog.d/` (one file per
# change · the four-PR collision of 2026-08-24). Reading only the section
# would leave this projection counting ZERO claims forever — an instrument
# that stopped looking, reporting a clean board. It reads BOTH, so it is
# strictly wider than before: the fragments are where claims live now, and
# the section still catches a bullet someone hand-writes back into it.
UNRELEASED=""
if [ -f "$ENGINE/CHANGELOG.md" ]; then
  UNRELEASED="$(
    awk '
      /^## \[Unreleased\]/{grab=1; next}
      grab && /^## \[/{exit}
      grab && /^- \*\*/ { print }
    ' "$ENGINE/CHANGELOG.md"
  )"
fi
if [ -d "$ENGINE/changelog.d" ]; then
  FRAGMENT_CLAIMS="$(
    find "$ENGINE/changelog.d" -maxdepth 1 -type f -name '*.md' ! -name 'README.md' \
      -exec grep -h '^- \*\*' {} + 2>/dev/null || true
  )"
  if [ -n "$FRAGMENT_CLAIMS" ]; then
    UNRELEASED="${UNRELEASED:+$UNRELEASED$'\n'}$FRAGMENT_CLAIMS"
  fi
fi

has_line() { printf '%s\n' "$2" | grep -qxF "$1"; }

UNPROVEN=0
UNPROVEN_ROWS=""

while IFS=$'\t' read -r id job || [ -n "${id:-}" ]; do
  [ -n "$id" ] || continue
  if [ -z "$job" ]; then
    UNPROVEN=$((UNPROVEN + 1))
    UNPROVEN_ROWS="${UNPROVEN_ROWS}ledger ${id} · no proven_by"$'\n'
    continue
  fi
  if ! has_line "$job" "$JOBS"; then
    UNPROVEN=$((UNPROVEN + 1))
    UNPROVEN_ROWS="${UNPROVEN_ROWS}ledger ${id} · proven_by job '${job}' is in no workflow"$'\n'
  fi
done <<<"$CAP_IDS"

# CHANGELOG claims that name a feature absent from the release build line.
# Cheap heuristic: the ACCESS class — "harness" / "access-harness" in Unreleased
# while the feature is not on the build line.
if printf '%s\n' "$UNRELEASED" | grep -qiE 'access-harness|harness access|access class'; then
  if ! has_line "access-harness" "$RELEASE_FEATURES"; then
    UNPROVEN=$((UNPROVEN + 1))
    UNPROVEN_ROWS="${UNPROVEN_ROWS}CHANGELOG [Unreleased] names the harness · release.yml does not build --features access-harness"$'\n'
  fi
fi

# The estate manifest. release.yml refuses a tag whose tree its manifest
# does not describe ("The estate manifest is true of the tagged tree"), and
# that refusal fires on every build leg, AFTER the tag exists. v0.117.1 died
# there: the manifest was regenerated in the prep commit, one more commit
# moved a tracked file, and nothing on the operator's path asked again. The
# projection asks the tag gate's question BEFORE the tag, as one more claim
# without proof.
ESTATE_STALE=0
if [ -f "$ENGINE/estate.yaml" ] && [ -f "$ENGINE/scripts/estate.py" ]; then
  if ! (cd "$ENGINE" && python3 scripts/estate.py --check >/dev/null 2>&1); then
    ESTATE_STALE=1
    UNPROVEN=$((UNPROVEN + 1))
    UNPROVEN_ROWS="${UNPROVEN_ROWS}estate.yaml does not describe the tree · python3 scripts/estate.py --write && git add estate.yaml · in the LAST commit before the tag"$'\n'
  fi
fi

FEAT_CSV="$(printf '%s' "$RELEASE_FEATURES" | paste -sd, - || true)"
JOB_N="$(printf '%s\n' "$JOBS" | grep -c . || true)"
CAP_N="$(printf '%s\n' "$CAP_IDS" | grep -c . || true)"
CLAIM_N="$(printf '%s\n' "$UNRELEASED" | grep -c . || true)"

if [ "$JSON" -eq 1 ]; then
  printf '{"gate":"next-tag-project","engine":"%s","release_workflow":"%s","features":[' "$ENGINE" "$RELEASE_WF"
  sep=""
  while IFS= read -r f; do
    [ -n "$f" ] || continue
    printf '%s"%s"' "$sep" "$f"
    sep=","
  done <<<"$RELEASE_FEATURES"
  printf '],"changelog_unreleased":%s,"ledger_rows":%s,"ci_jobs":%s,"estate_stale":%s,"unproven":%s}\n' \
    "$CLAIM_N" "$CAP_N" "$JOB_N" "$ESTATE_STALE" "$UNPROVEN"
else
  printf 'next-tag-project · %s\n' "$ENGINE"
  printf 'scope · release.yml build line · wiring.yaml · CHANGELOG [Unreleased] · CI job keys · estate.yaml\n'
  printf 'NOT covered · G5/G6 source drops · website · docs\n\n'
  printf 'IN THE BINARY     %s\n' "${FEAT_CSV:-NONE}"
  printf 'CHANGELOG claims  %s Unreleased bullet(s)\n' "$CLAIM_N"
  printf 'LEDGER rows       %s\n' "$CAP_N"
  printf 'CI job keys       %s\n' "$JOB_N"
  printf '\n'
  if [ "$UNPROVEN" -eq 0 ]; then
    printf 'UNPROVEN          none · every ledger row names a live job · changelog does not promise a dark feature · the estate manifest describes the tree\n'
  else
    printf 'UNPROVEN          %s claim(s) without proof\n' "$UNPROVEN"
    printf '%s' "$UNPROVEN_ROWS" | sed 's/^/  · /'
  fi
fi

[ "$CHECK" -eq 1 ] && [ "$UNPROVEN" -gt 0 ] && exit 1
exit 0
