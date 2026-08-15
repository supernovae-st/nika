#!/usr/bin/env bash
# The private-path patterns, in ONE place, for the two gates that need them.
#
# Why this file exists — measured 2026-08-15. The pre-commit hook
# (`scripts/hooks/block-private-paths.sh`) guarded TEN patterns plus the
# venture shape. The full-tree sweep (`scripts/hygiene/check-private-leaks.sh`)
# guarded ONE, hardcoded. So the sweep that looks EVERYWHERE searched for
# almost nothing, and 18 occurrences of a private path — one of them in a
# user-facing error hint of the PUBLIC engine — slept on main until a merge
# happened to put them in someone's staged diff.
#
# The class · a diff gate only ever sees the index of the commit it runs on.
# Anything committed on a base older than the gate escapes it forever, and a
# squash-merge runs no local hook at all. A gate that matters needs a
# full-tree twin with the SAME patterns, or it guards the flow and never the
# stock. Two pattern lists drift; one does not.
#
# Sourced, never executed. Consumers:
#   scripts/hooks/block-private-paths.sh    (staged diff · blocks the commit)
#   scripts/hygiene/check-private-leaks.sh  (whole tree · vector 14)
#
# SC2034 · every constant here is "unused" from this file's point of view.
# That is what a shared constant file IS; the consumers are named above and
# the test asserts both of them source it.
# shellcheck disable=SC2034

# Anchored at a path boundary. Unanchored `studio/` matched `lmstudio/` and
# `~/.cache/lm-studio/` — real, public LM Studio provider paths — so the gate
# blocked legitimate work. This also keeps `https://supernovae.studio/` out
# while still matching `/studio/`.
readonly NIKA_PRIVATE_BOUNDARY='(^|[^a-zA-Z0-9._-])'

# The studio + the agent substrate are named at the ROOT — the only spelling
# a rename inside them cannot invalidate. Enumerating their children would
# itself be the leak these patterns exist to stop, in a PUBLIC file.
# The `*/hq/` spellings are pre-migration: a frozen citation still leaks.
readonly NIKA_PRIVATE_PATTERNS=(
  "${NIKA_PRIVATE_BOUNDARY}studio/"
  "${NIKA_PRIVATE_BOUNDARY}dx/"
  "${NIKA_PRIVATE_BOUNDARY}\.claude/projects/"
  "${NIKA_PRIVATE_BOUNDARY}nika/hq/"
  "${NIKA_PRIVATE_BOUNDARY}studio-spn/"
  "${NIKA_PRIVATE_BOUNDARY}jungo/hq/"
  "${NIKA_PRIVATE_BOUNDARY}novanet/hq/"
  "${NIKA_PRIVATE_BOUNDARY}qrcodeai/hq/"
  "${NIKA_PRIVATE_BOUNDARY}supernovae-hq/"
)

# The venture tree, as a shape: everything under `ventures/<name>/` is
# private — the 9 poles, for every venture that exists or ever will — EXCEPT
# the one public tier, which is where this very repo lives.
readonly NIKA_VENTURE_SHAPE='ventures/[a-z0-9][a-z0-9-]*/[a-zA-Z0-9._/-]*'
readonly NIKA_VENTURE_PUBLIC='^ventures/nika/02-engineering/repos/'

# One alternation, for callers that grep a whole tree rather than a diff.
nika_private_alternation() {
  local IFS='|'
  printf '%s' "${NIKA_PRIVATE_PATTERNS[*]}"
}
