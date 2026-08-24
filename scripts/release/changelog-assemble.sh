#!/usr/bin/env bash
# changelog-assemble.sh — one fragment per change, assembled at tag time.
#
# WHY THIS EXISTS · `CHANGELOG.md` was the single file every concurrent
# branch wrote to. Measured 2026-08-24 on four security pull requests
# (#1162 #1163 #1164 #1165): `git merge-tree` reported ONE conflict each,
# always `CHANGELOG.md`, and ZERO source-file overlap — the four touched
# nine distinct crate files between them and collided on none. The same
# shape had already been met once here: `estate.yaml`, four PRs, four
# conflicts, 2026-08-20 (see `.gitattributes`). A shared append target is
# a shared index, and a shared index conflicts by construction.
#
# The fix is structural, not disciplinary: a change describes itself in
# its OWN file under `changelog.d/`, and nothing assembles them until the
# tag. Two branches can never write the same path, so they can never
# collide. (towncrier · changie · the Rust project's `triagebot` all
# converged on this shape for the same reason.)
#
# `merge=union` on CHANGELOG.md was the cheaper candidate and was
# rejected on a measurement, not a preference: GitHub does not apply
# `.gitattributes` merge drivers when it computes PR mergeability, so
# the pull requests would stay DIRTY on the page while resolving locally
# — and a union of two curated narratives interleaves them into prose
# nobody wrote. The repo already learned that a driver nothing arms is
# a fix that does not fire (`.gitattributes`, the `merge=ours` arming
# note).
#
# Usage ·
#   changelog-assemble.sh                print the assembled [Unreleased] body
#   changelog-assemble.sh --check        gate: fragments well-formed · no hand-edit
#   changelog-assemble.sh --list         one line per fragment (path · section)
#   changelog-assemble.sh --fold <ver> [--date YYYY-MM-DD]
#                                        rewrite CHANGELOG.md · consume fragments
#
# Exit · 0 ok · 1 a refusal the author must fix · 2 a usage error.
#
# Co-Authored-By: Nika 🦋 <nika@supernovae.studio>

set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
ROOT="${SPN_CHANGELOG_REPO:-$(cd -- "$SCRIPT_DIR/../.." && pwd -P)}"

# Keep a Changelog 1.1.0 order. The suffix set is CLOSED on purpose: a
# free-form section would let two fragments invent two spellings of the
# same heading, and the assembled body would grow a duplicate section
# nobody notices until the release page renders it.
readonly SECTIONS=(added changed deprecated removed fixed security)

FRAGDIR="$ROOT/changelog.d"
CHANGELOG="$ROOT/CHANGELOG.md"

die() {
  printf 'changelog-assemble: %s\n' "$1" >&2
  exit "${2:-1}"
}

section_title() {
  # `added` → `Added`. Portable: bash 3.2 on macOS has no ${x^}.
  printf '%s%s' "$(printf '%s' "$1" | cut -c1 | tr '[:lower:]' '[:upper:]')" \
    "$(printf '%s' "$1" | cut -c2-)"
}

# Fragments of one section, in sort order. `sort -V` (BSD 2.3 + GNU) reads
# digit runs numerically, so 761 < 905 < 1068 — an issue number is a valid
# sort key without zero-padding, and a slug sorts after the numbers.
fragments_of() {
  find "$FRAGDIR" -maxdepth 1 -type f -name "*.$1.md" 2>/dev/null | sort -V
}

all_fragments() {
  find "$FRAGDIR" -maxdepth 1 -type f -name '*.md' ! -name 'README.md' 2>/dev/null | sort -V
}

# ── assemble: the [Unreleased] body, byte-for-byte what the section held ────
assemble() {
  local sec title frag any=0
  for sec in "${SECTIONS[@]}"; do
    [ -n "$(fragments_of "$sec")" ] || continue
    any=1
    title="$(section_title "$sec")"
    printf '\n### %s\n\n' "$title"
    while IFS= read -r frag; do
      [ -n "$frag" ] || continue
      cat "$frag"
    done < <(fragments_of "$sec")
  done
  # The trailing blank line separates the section from the next `## [`
  # header. Emitted only when something was emitted at all, so an empty
  # changelog.d prints nothing rather than a lone newline.
  if [ "$any" -eq 1 ]; then printf '\n'; fi
  return 0
}

# ── check: the gate ─────────────────────────────────────────────────────────
check() {
  local rc=0 frag base name sec first n line strays
  local sec_alt
  sec_alt="$(
    IFS='|'
    printf '%s' "${SECTIONS[*]}"
  )"

  [ -d "$FRAGDIR" ] || die "no changelog.d/ at $ROOT — the mechanism is not installed"
  [ -f "$FRAGDIR/README.md" ] || {
    printf 'changelog.d/README.md is missing — it is the contract a contributor reads,\n' >&2
    printf 'and the file that keeps the estate glob non-empty.\n' >&2
    rc=1
  }

  while IFS= read -r frag; do
    [ -n "$frag" ] || continue
    base="$(basename "$frag")"
    if ! printf '%s' "$base" | grep -qE "^[A-Za-z0-9][A-Za-z0-9._-]*\.($sec_alt)\.md$"; then
      printf '%s\n' "$base" >&2
      printf '  name must be <sort-key>.<section>.md · section one of: %s\n' "${SECTIONS[*]}" >&2
      rc=1
      continue
    fi
    name="${base%.md}"
    sec="${name##*.}"
    first="$(head -n 1 "$frag")"
    # `next-tag-project.sh` counts the claims in this release by matching
    # `^- \*\*`. A fragment that opens any other way is invisible to the
    # projection — the exact shape this whole map exists to kill.
    case "$first" in
      '- **'*) ;;
      *)
        printf '%s\n' "$base" >&2
        printf "  line 1 must open '- **<headline>.**' — next-tag-project.sh counts\n" >&2
        printf '  the release claims by that prefix; anything else is not projected.\n' >&2
        rc=1
        ;;
    esac
    n=0
    while IFS= read -r line || [ -n "$line" ]; do
      n=$((n + 1))
      [ "$n" -eq 1 ] && continue
      if [ -z "${line// /}" ]; then
        printf '%s:%s\n' "$base" "$n" >&2
        printf '  blank line inside a fragment — a blank between list items makes the\n' >&2
        printf '  whole rendered list loose. One fragment is one bullet, unbroken.\n' >&2
        rc=1
        continue
      fi
      case "$line" in
        '  '*) ;;
        *)
          printf '%s:%s\n' "$base" "$n" >&2
          printf '  continuation lines are indented two spaces (the bullet body).\n' >&2
          rc=1
          ;;
      esac
    done <"$frag"
    # A last line without its newline would glue two bullets together in
    # the assembled body. `wc -l` cannot see that; the last byte can.
    if [ -n "$(tail -c 1 "$frag")" ]; then
      printf '%s\n' "$base" >&2
      printf '  no trailing newline — the assembled body would glue two bullets.\n' >&2
      rc=1
    fi
  done < <(all_fragments)

  # The other half: the section this mechanism replaced must stay empty.
  # Without this the next author writes a bullet into CHANGELOG.md, the
  # collision comes back, and nothing says why.
  if [ -f "$CHANGELOG" ]; then
    strays="$(
      awk '/^## \[Unreleased\]/{g=1;next} g && /^## \[/{exit} g && /^- /{print NR": "$0}' \
        "$CHANGELOG"
    )"
    if [ -n "$strays" ]; then
      printf 'CHANGELOG.md · %s bullet(s) hand-written into [Unreleased]:\n' \
        "$(printf '%s\n' "$strays" | grep -c . || true)" >&2
      printf '%s\n' "$strays" | sed 's/^/    /' >&2
      printf '  That section is assembled, not authored. Move each bullet to its own\n' >&2
      printf '  file so two branches can never write the same path:\n\n' >&2
      printf '      changelog.d/<issue-or-slug>.changed.md\n\n' >&2
      printf '  Four pull requests collided on this one file on 2026-08-24, on the\n' >&2
      printf '  changelog alone, with zero source overlap between them.\n' >&2
      rc=1
    fi
  fi

  if [ "$rc" -eq 0 ]; then
    printf '[changelog-fragments] %s fragment(s) · [Unreleased] is assembled, not authored\n' \
      "$(all_fragments | grep -c . || true)"
  fi
  return "$rc"
}

# ── fold: the tag ceremony ──────────────────────────────────────────────────
fold_release() {
  local ver="$1" date="$2" body prev repo tmp
  [ -f "$CHANGELOG" ] || die "no CHANGELOG.md at $ROOT" 2
  check >/dev/null || die "fragments do not pass --check — refusing to fold"

  body="$(assemble)"
  [ -n "${body//[[:space:]]/}" ] || die "changelog.d/ holds no fragment — nothing to fold"

  # The previous version is the first `## [x.y.z]` under Unreleased: derived
  # from the file being edited, never from a tag that may not exist yet.
  # Plain POSIX awk — `match(s, re, arr)` is a gawk extension and macOS ships
  # the one-true-awk, where it would silently take the two-argument form.
  prev="$(
    awk '/^## \[Unreleased\]/{g=1;next} g && /^## \[/{
        line=$0; sub(/^## \[/,"",line); sub(/\].*/,"",line); print line; exit }' "$CHANGELOG"
  )"
  [ -n "$prev" ] || die "no previous ## [x.y.z] section under [Unreleased] — cannot build the compare link"
  repo="${GITHUB_REPOSITORY:-supernovae-st/nika}"

  tmp="$(mktemp)"
  {
    awk '/^## \[Unreleased\]/{exit} {print}' "$CHANGELOG"
    unreleased_stub
    printf '## [%s](https://github.com/%s/compare/v%s..v%s) - %s\n' \
      "$ver" "$repo" "$prev" "$ver" "$date"
    printf '%s\n' "$body"
    awk '/^## \[Unreleased\]/{g=1;next} g && /^## \[/{p=1} p{print}' "$CHANGELOG"
  } >"$tmp"
  mv "$tmp" "$CHANGELOG"

  while IFS= read -r frag; do
    [ -n "$frag" ] || continue
    rm -f "$frag"
  done < <(all_fragments)

  printf 'changelog-assemble: folded %s fragment(s) into ## [%s] · changelog.d/ is empty\n' \
    "$(printf '%s\n' "$body" | grep -c '^- \*\*' || true)" "$ver"
}

unreleased_stub() {
  cat <<'STUB'
## [Unreleased]

One file per change under [`changelog.d/`](changelog.d/), assembled into the
section below at tag time (`bash scripts/release/changelog-assemble.sh --fold
<version>`). Do not write bullets here: this file is where four concurrent
pull requests collided on 2026-08-24 with no source overlap between them, and
`--check` refuses a hand-written bullet in this section.

STUB
}

# ── entry ───────────────────────────────────────────────────────────────────
MODE='print'
VER=""
DATE="$(date -u +%F)"

while [ $# -gt 0 ]; do
  case "$1" in
    --check)
      MODE='check'
      shift
      ;;
    --list)
      MODE='list'
      shift
      ;;
    --fold)
      MODE='fold'
      VER="${2:-}"
      [ -n "$VER" ] || die "--fold needs a version (e.g. --fold 0.115.0)" 2
      shift 2
      ;;
    --date)
      DATE="${2:-}"
      shift 2
      ;;
    --repo)
      ROOT="${2:-}"
      FRAGDIR="$ROOT/changelog.d"
      CHANGELOG="$ROOT/CHANGELOG.md"
      shift 2
      ;;
    -h | --help)
      sed -n '2,40p' "$0"
      exit 0
      ;;
    *)
      die "unknown argument $1" 2
      ;;
  esac
done

case "$MODE" in
  print) assemble ;;
  check) check ;;
  list)
    while IFS= read -r frag; do
      [ -n "$frag" ] || continue
      base="$(basename "$frag")"
      name="${base%.md}"
      printf '%-52s %s\n' "changelog.d/$base" "${name##*.}"
    done < <(all_fragments)
    ;;
  fold) fold_release "$VER" "$DATE" ;;
esac
