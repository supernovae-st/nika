#!/usr/bin/env bash
# Vector 5: ROADMAP.md checkboxes for admitted crates should be ticked.
set -u
ROADMAP="ROADMAP.md"
[ -f "$ROADMAP" ] || { echo "ROADMAP.md not found"; exit 1; }

# For each admitted crate, check the roadmap box.
missed=""
for cargo in tools/*/Cargo.toml; do
  [ -f "$cargo" ] || continue
  crate="$(basename "$(dirname "$cargo")")"
  # Look for "- [ ] nika-X" or "- [ ] `nika-X`" or similar unticked entries
  if grep -qE "^- \[ \][^[:alnum:]]+\`?${crate}\`?" "$ROADMAP" 2>/dev/null; then
    missed="${missed}${crate} "
  fi
done

if [ -n "$missed" ]; then
  echo "unchecked boxes: ${missed}"; exit 1
fi
echo "OK (roadmap consistent)"; exit 0
