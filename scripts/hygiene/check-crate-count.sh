#!/usr/bin/env bash
# Vector 2: Admitted crates count vs MEMORY.md.
set -u
MEMORY="$HOME/.claude/projects/-Users-thibaut-dev-supernovae-nika/memory/MEMORY.md"

actual="$(find crates -maxdepth 2 -name Cargo.toml 2>/dev/null | wc -l | tr -d ' ')"
recorded="$(grep -oE 'Crates admitted: \*\*[0-9]+' "$MEMORY" 2>/dev/null | head -1 | grep -oE '[0-9]+' | tail -1)"

if [ -z "$recorded" ]; then
  echo "no crate count recorded"
  exit 1
fi

if [ "$actual" = "$recorded" ]; then
  echo "OK ($actual admitted)"
  exit 0
else
  echo "drift: MEMORY=$recorded actual=$actual"
  exit 2
fi
