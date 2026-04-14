#!/usr/bin/env bash
# Vector 6: docs/crate-specs/nika-X.md exists for each admitted crate.
set -u
missing=""
for cargo in tools/*/Cargo.toml; do
  [ -f "$cargo" ] || continue
  crate="$(basename "$(dirname "$cargo")")"
  spec="docs/crate-specs/${crate}.md"
  [ -f "$spec" ] || missing="${missing}${crate} "
done

if [ -n "$missing" ]; then
  echo "missing specs: ${missing}"; exit 2
fi
echo "OK (all specs present)"; exit 0
