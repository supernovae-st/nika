#!/usr/bin/env bash
# check-layer-deps — vector 33 (Batch I.b)
#
# Enforces per-layer banned-dep policy: crates at a given layer MUST NOT
# declare any dependency listed under `[workspace.metadata.diamond]
# layer-bans.<layer>` in their own Cargo.toml `[dependencies]` section.
#
# Rationale: layer discipline (vector 22 check-layering) enforces
# UPWARD/DOWNWARD rules between workspace members. This ratchet extends
# that to THIRD-PARTY deps: L0 stays zero-async (no tokio/futures),
# L0.5 stays runtime-agnostic (traits-only via trait_variant, no
# direct tokio binding), etc.
#
# Exempt: a line-level marker `# LAYER-BAN-EXEMPT: <reason>` on the same
# line or the line above the dep. Use sparingly (build-deps for codegen,
# dev-deps for tests are already naturally exempt).
#
# Exit codes: 0 = green, 2 = red.

set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$HERE/../.." && pwd)"
cd "$REPO_ROOT" || exit 2

# Parse [workspace.metadata.diamond.layers] → associative crate→layer.
#
# The `=()` is load-bearing: `declare -A X` alone leaves the name UNSET,
# and under `set -u` every later `${#X[@]}` / `${!X[@]}` expansion fails
# instead of reading empty. With no `set -e` the script does not stop —
# it just skips whatever those expansions were guarding. Assigning once
# makes the name bound.
declare -A CRATE_LAYER=()
while IFS='=' read -r crate layer; do
  [ -z "$crate" ] && continue
  CRATE_LAYER["$crate"]="$layer"
done < <(awk '
  /^\[workspace\.metadata\.diamond\]/ { in_section = 1; next }
  /^\[/ && in_section { in_section = 0 }
  in_section && /^layers\./ {
    gsub(/^layers\./, "")
    gsub(/ /, "")
    gsub(/"/, "")
    print
  }
' Cargo.toml)

# Parse [workspace.metadata.diamond] layer-bans.<layer> lists.
# Format in Cargo.toml: `layer-bans.L0 = ["tokio", ...]` or
# `layer-bans."L0.5" = ["tokio", ...]`.
# Output: one "layer|dep" per line per banned entry.
declare -A LAYER_BANS=()
while IFS='|' read -r layer dep; do
  [ -z "$layer" ] && continue
  existing="${LAYER_BANS[$layer]:-}"
  LAYER_BANS["$layer"]="$existing $dep"
done < <(awk '
  /^\[workspace\.metadata\.diamond\]/ { in_section = 1; next }
  /^\[/ && in_section { in_section = 0 }
  in_section && /^layer-bans\./ {
    # Split `layer-bans.<layer> = [<list>]`
    line = $0
    sub(/^layer-bans\./, "", line)
    eq = index(line, "=")
    layer_raw = substr(line, 1, eq - 1)
    rhs = substr(line, eq + 1)
    gsub(/^[[:space:]]+|[[:space:]]+$/, "", layer_raw)
    gsub(/"/, "", layer_raw)
    # Strip brackets + split list
    gsub(/\[|\]/, "", rhs)
    n = split(rhs, items, ",")
    for (i = 1; i <= n; i++) {
      item = items[i]
      gsub(/^[[:space:]]*"?|"?[[:space:]]*$/, "", item)
      if (item != "") print layer_raw "|" item
    }
  }
' Cargo.toml)

# Fail CLOSED on an empty harvest. There was no check here at all: with no
# layers parsed the loop below has nothing to iterate, every ban is skipped,
# and the script reaches its OK line having enforced nothing. A rename of
# the metadata section — or a parse that stops matching it — reads exactly
# like a clean workspace.
if [ ${#CRATE_LAYER[@]} -eq 0 ]; then
  echo "RED: no [workspace.metadata.diamond] layers.* rows parsed from Cargo.toml" >&2
  echo "  (nothing to check is not the same as nothing to fix)" >&2
  exit 2
fi
if [ ${#LAYER_BANS[@]} -eq 0 ]; then
  echo "RED: no [workspace.metadata.diamond] layer-bans.* rows parsed from Cargo.toml" >&2
  echo "  (this vector exists to enforce them; zero bans enforces nothing)" >&2
  exit 2
fi

violations=0
for crate in "${!CRATE_LAYER[@]}"; do
  layer="${CRATE_LAYER[$crate]}"
  banned="${LAYER_BANS[$layer]:-}"
  [ -z "$banned" ] && continue

  crate_toml="crates/${crate}/Cargo.toml"
  [ -f "$crate_toml" ] || continue

  # Extract declared dep names from [dependencies] only (NOT dev/build-deps).
  # Format handled: `name = ...` and `name.workspace = true` and `name.version = ...`.
  declared=$(awk '
    /^\[dependencies\]/ { in_deps = 1; next }
    /^\[/ && in_deps { in_deps = 0 }
    in_deps && $0 ~ /LAYER-BAN-EXEMPT:/ { skip_next = 1; next }
    in_deps && /^[a-zA-Z0-9_-]+[[:space:]]*=/ {
      if (skip_next) { skip_next = 0; next }
      name = $1
      gsub(/[[:space:]]/, "", name)
      print name
    }
    in_deps && /^[a-zA-Z0-9_-]+\.[a-zA-Z_-]+[[:space:]]*=/ {
      if (skip_next) { skip_next = 0; next }
      split($0, parts, ".")
      name = parts[1]
      gsub(/[[:space:]]/, "", name)
      print name
    }
  ' "$crate_toml" | sort -u)

  for dep in $declared; do
    for b in $banned; do
      if [ "$dep" = "$b" ]; then
        echo "RED: ${crate} (${layer}) declares banned dep \`${dep}\`"
        violations=$((violations + 1))
      fi
    done
  done
done

if [ "$violations" -gt 0 ]; then
  echo "  (see [workspace.metadata.diamond] layer-bans in Cargo.toml; mark exempt with # LAYER-BAN-EXEMPT: <reason>)"
  exit 2
fi

total_banned=0
for layer in "${!LAYER_BANS[@]}"; do
  count=$(echo "${LAYER_BANS[$layer]}" | wc -w | tr -d '[:space:]')
  total_banned=$((total_banned + count))
done
echo "OK: ${#CRATE_LAYER[@]} crates clean across ${#LAYER_BANS[@]} layer(s), ${total_banned} banned deps enforced"
exit 0
