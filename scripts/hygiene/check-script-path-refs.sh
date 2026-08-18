#!/usr/bin/env bash
# Vector 44: a repo-relative path named by a script must exist.
#
# The ratchet for a class that showed FIVE faces on 2026-07-30, all of them
# "a tool asserting something false with its own authority":
#
#   - a commit-msg warning telling the reader to edit commitlint.config.js,
#     a file this repo does not contain
#   - a media capture reading examples/showcase/t3-*.nika.yaml, a layout the
#     pack had flattened away — and it did not fail, because the capture
#     redirects stderr INTO its artifact, so graph-fanout.mmd shipped
#     "cannot read ..." where a mermaid DAG belongs
#   - an estate evidence string naming src/adversarial/fixtures.rs as the
#     consumer of the fixtures when it is src/adversarial/tests.rs
#   - an ADR evidence path (vector 20's subject, fixed separately)
#   - a verb's own --help describing a taxonomy it had outgrown
#
# The cost is never the wrong string: it is that a surface which lies once
# teaches everyone to stop reading it. Discipline caught these one at a
# time; this vector catches the class.
#
# YELLOW, never RED, deliberately: a stale reference is a documentation
# defect, and a naming convention must not hold the house's pushes hostage
# (the same reasoning that keeps vector 1 advisory).
#
# Exit codes:
#   0 -- GREEN (every path named by a script exists)
#   1 -- YELLOW (some are stale — the message, or the input, is lying)

set -uo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/../.." || exit 0

missing=0
checked=0

# The four exemptions below are not guesses: each is a false positive
# measured on 2026-07-30 while sweeping this exact class by hand.
#
#   1. a URL — `{RAW}/supernovae-st/nika-registry/main/scripts/cert.py`
#      names a path in ANOTHER repo, which is correct and unresolvable here
#   2. a deliberate non-present reference — "the future docs/workspace.toml
#      SSOT (ADR-026)", or an "ex <old path>" provenance citation
#   3. a placeholder — `docs/crate-specs/nika-X.md`, or any glob/brace
#   4. an OUTPUT — `printf ... > docs/note.md` creates the file it names
#   5. a SELF-TEST's fixture — `scripts/hygiene/tests/*.test.sh` builds
#      fake trees to prove a vector, so its paths must NOT exist: the
#      names are `check-alpha.sh`, `check-beta.sh`, and one called
#      `check-gone.sh` precisely because the case under test IS a stale
#      declaration. Measured 2026-08-18 · 6 findings, all fixtures, zero
#      real. Same shape as the privacy-boundary sweep accusing the
#      private-path gate's own red examples the same evening: a gate that
#      reports another gate's fixtures teaches the reader to stop reading
#      it, which is the exact cost this vector exists to prevent.
#      File-wide, like the OUTPUT exemption, and no wider — the sweep
#      keeps judging every non-test script under `scripts/`.
# A first draft judged one line at a time and produced four false positives
# out of five hits — including two that pointed at THIS FILE's own comments.
# Shipping that would have added a sixth face to the very class it ratchets,
# so the scope narrowed to what is mechanically decidable:
#
#   - COMMENTS are prose and prose legitimately cites paths that are gone,
#     elsewhere, or planned; the wrap alone defeats a line-scoped marker
#     ("... — ex" ending a line, the path opening the next). Only executable
#     lines are judged, which is also where all three real defects lived: a
#     message string, an input assignment, an evidence field.
#   - an OUTPUT is exempt FILE-WIDE, not line-wide, because a script writes
#     on one line and refers to the file on the next (`>docs/note.md` then
#     `git add docs/note.md`).
#   - a URL may be assembled from a variable, and the variable's NAME is not
#     a signal: the same file joins on `{RAW}/` and on `{R}/`. The signal is
#     the join itself — an upper-case placeholder followed by a slash — so
#     the rule matches the shape instead of a spelling.
exempt_line() {
  # A URL base assembled from a variable: {RAW}/… {R}/… ${BASE}/…
  [[ "$1" =~ \$?\{[A-Z_][A-Z_0-9]*\}/ ]] && return 0
  case "$1" in
    *http* | *raw.githubusercontent*) return 0 ;;
    *future* | *will* | *planned* | *" ex "* | *formerly* | *" was "* | *upstream*) return 0 ;;
    *'>'*) return 0 ;;
    *) return 1 ;;
  esac
}

while IFS= read -r script; do
  # Paths this script CREATES are its outputs, not claims about the tree.
  outputs="$(grep -oE '(>|tee[[:space:]]+|open\()[[:space:]]*"?'"'"'?(crates|docs|scripts|dx|fuzz)/[A-Za-z0-9_./-]+' "$script" 2>/dev/null \
    | grep -oE '(crates|docs|scripts|dx|fuzz)/[A-Za-z0-9_./-]+' || true)"

  lineno=0
  while IFS= read -r line; do
    lineno=$((lineno + 1))
    # Prose is exempt: only executable lines make claims the tree must honour.
    case "${line#"${line%%[![:space:]]*}"}" in
      '#'*) continue ;;
    esac
    exempt_line "$line" && continue
    # Word-boundary anchored: a bare extension match reads `index.js`
    # inside `index.json` and invents phantoms (measured, same sweep).
    for p in $(printf '%s\n' "$line" \
      | grep -oE '(crates|docs|scripts|dx|fuzz)/[A-Za-z0-9_./-]+\.(sh|py|yml|yaml|toml|md|rs|txt|json|ndjson)([^A-Za-z0-9]|$)' \
      | sed -E 's/[^A-Za-z0-9]$//' || true); do
      case "$p" in
        *'*'* | *'{'* | *'<'* | *X.md) continue ;;
      esac
      printf '%s\n' "$outputs" | grep -qxF "$p" && continue
      checked=$((checked + 1))
      if [ ! -e "$p" ]; then
        echo "MISSING: $script:$lineno names \`$p\` which does not exist" >&2
        missing=$((missing + 1))
      fi
    done
  done <"$script"
done < <(find scripts -type f \( -name '*.sh' -o -name '*.py' \) \
  -not -path 'scripts/hygiene/tests/*' | sort)

if [ "$missing" -gt 0 ]; then
  echo "$missing stale path reference(s) in scripts ($checked checked)"
  echo "Hint: fix the path, or make the reference honest — a URL, an \`ex\`"
  echo "provenance note, a 'future' marker, or an output redirect are all"
  echo "exempt because each one is true as written."
  exit 1
fi

echo "OK ($checked script path references verified)"
exit 0
