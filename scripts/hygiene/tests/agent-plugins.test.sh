#!/usr/bin/env bash
# COVERS: scripts/hygiene/check-agent-plugins.sh
# Mutation proof for vector 48 (check-agent-plugins.sh).
#
# A gate that has never been seen to fail is decoration. This copies the
# real plugin tree to a scratch dir, breaks it one way at a time, and
# asserts the vector goes RED for each break — then asserts the pristine
# copy is GREEN, so a vector that simply always fails cannot pass either.
#
# Each mutation is a real client behaviour: a fatal manifest error makes
# a conformant client reject the whole plugin (§5.2, §11.3), a bad server
# entry makes it skip that server (§7.2.2), a bad skill makes it skip that
# skill (§7.1). All of them are silent in our own CI without this vector.
#
# Two shellcheck notes are wrong here by construction. Every mutator
# reaches its call site through `expect`, which takes the function name as
# an argument (SC2329), and `$schema` in single quotes is a literal JSON
# key travelling to python, never a shell variable (SC2016).
# shellcheck disable=SC2329,SC2016
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
VECTOR="$ROOT/scripts/hygiene/check-agent-plugins.sh"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

fails=0
cases=0

# The vector resolves its root from its own location, so the scratch tree
# has to look like the repo: Cargo.toml, the script, the plugin dir.
seed() {
  local dest="$1"
  rm -rf "$dest"
  mkdir -p "$dest/scripts/hygiene" "$dest/.agents/plugins"
  cp "$ROOT/Cargo.toml" "$dest/Cargo.toml"
  cp "$VECTOR" "$dest/scripts/hygiene/check-agent-plugins.sh"
  cp -R "$ROOT/.agents/plugins/nika" "$dest/.agents/plugins/nika"
}

# poke <dir> <file> set|del <dotted.path> [json-value]
poke() {
  local dir="$1" rel="$2" op="$3" path="$4" value="${5:-null}"
  M_FILE="$dir/.agents/plugins/nika/$rel" M_OP="$op" M_PATH="$path" M_VALUE="$value" \
    python3 - <<'PY'
import json
import os

path = os.environ["M_FILE"]
keys = os.environ["M_PATH"].split(".")
with open(path, encoding="utf-8") as fh:
    doc = json.load(fh)

node = doc
for key in keys[:-1]:
    node = node[key]

if os.environ["M_OP"] == "del":
    del node[keys[-1]]
else:
    node[keys[-1]] = json.loads(os.environ["M_VALUE"])

with open(path, "w", encoding="utf-8") as fh:
    json.dump(doc, fh, indent=2)
PY
}

# expect <GREEN|RED> <label> <mutator...>
expect() {
  local want="$1" label="$2"
  shift 2
  cases=$((cases + 1))
  local dir="$WORK/case-$cases"
  seed "$dir"
  "$@" "$dir"
  bash "$dir/scripts/hygiene/check-agent-plugins.sh" >/dev/null 2>&1
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

untouched() { :; }
m_unknown_key() { poke "$1" plugin.json set skills '"./skills/"'; }
m_bad_name() { poke "$1" plugin.json set name '"Nika"'; }
m_double_hyphen() { poke "$1" plugin.json set name '"ni--ka"'; }
m_no_schema() { poke "$1" plugin.json del '$schema'; }
m_old_schema() { poke "$1" plugin.json set '$schema' '"https://agent-plugins.org/schemas/0.9.0/plugin.schema.json"'; }
m_author_extra() { poke "$1" plugin.json set author.twitter '"@nika"'; }
m_version_drift() { poke "$1" plugin.json set version '"0.0.1"'; }
m_mcp_no_schema() { poke "$1" mcp.json del '$schema'; }
m_mcp_no_type() { poke "$1" mcp.json del mcpServers.nika.type; }
m_mcp_foreign() { poke "$1" mcp.json set mcpServers.nika.url '"https://example.com"'; }
m_mcp_shell() { poke "$1" mcp.json set mcpServers.nika.command '"nika mcp"'; }
m_mcp_reserved() { poke "$1" mcp.json set mcpServers.nika.env '{"PLUGIN_ROOT": "/tmp"}'; }
m_mcp_extra_top() { poke "$1" mcp.json set inputs '[]'; }
m_mcp_mismatch() { poke "$1" mcp.json set '$schema' '"https://agent-plugins.org/schemas/1.1.0/mcp.schema.json"'; }

m_plugin_gone() { rm -f "$1/.agents/plugins/nika/plugin.json"; }
m_mcp_gone() { rm -f "$1/.agents/plugins/nika/mcp.json"; }
m_skill_rename() { mv "$1/.agents/plugins/nika/skills/nika-authoring" "$1/.agents/plugins/nika/skills/nika-writing"; }
m_skill_no_front() { printf '# no frontmatter\n' >"$1/.agents/plugins/nika/skills/nika-authoring/SKILL.md"; }
m_skills_empty() {
  rm -rf "$1/.agents/plugins/nika/skills"
  mkdir -p "$1/.agents/plugins/nika/skills"
}

echo "vector 48 · mutation proof"

expect GREEN "pristine tree" untouched
expect RED "plugin.json unknown top-level key" m_unknown_key
expect RED "plugin.json uppercase name" m_bad_name
expect RED "plugin.json consecutive hyphens" m_double_hyphen
expect RED "plugin.json missing \$schema" m_no_schema
expect RED "plugin.json stale \$schema version" m_old_schema
expect RED "plugin.json author extra field" m_author_extra
expect RED "plugin.json version drifts" m_version_drift
expect RED "plugin.json deleted" m_plugin_gone
expect RED "mcp.json missing \$schema" m_mcp_no_schema
expect RED "mcp.json server without type" m_mcp_no_type
expect RED "mcp.json stdio server with url" m_mcp_foreign
expect RED "mcp.json command as shell string" m_mcp_shell
expect RED "mcp.json env sets PLUGIN_ROOT" m_mcp_reserved
expect RED "mcp.json extra top-level field" m_mcp_extra_top
expect RED "mcp.json targets another spec" m_mcp_mismatch
expect RED "mcp.json deleted" m_mcp_gone
expect RED "skill name no longer matches dir" m_skill_rename
expect RED "skill without frontmatter" m_skill_no_front
expect RED "skills/ empty — a blind harvest" m_skills_empty

if [ "$fails" -gt 0 ]; then
  printf '\n%d/%d mutation(s) survived — the vector does not catch what it claims.\n' \
    "$fails" "$cases" >&2
  exit 1
fi

printf '\n%d/%d mutations killed.\n' "$cases" "$cases"
exit 0
