#!/usr/bin/env bash
# Vector 48: the kit stays loadable by an Agent Plugins 1.0.0 client.
#
# Agent Plugins 1.0.0 (published 2026-08-06 · agent-plugins.org) is the
# portable package format Cursor, GitHub Copilot, Kiro, VS Code and
# ChatGPT/Codex read. We already shipped three client-native manifests in
# hidden directories; this vector guards the fourth, portable one at the
# plugin root.
#
# It matters because the portable manifest fails CLOSED. §5.2: the schema
# is closed, and "any schema violation other than an unknown top-level
# field or a non-object extensions field is fatal: the client MUST reject
# the plugin and MUST NOT discover or execute any of its components."
# One stray key of the shape the OTHER three manifests use — "skills",
# "hooks", "mcpServers", "logo" — is reported and ignored; one bad `name`
# character or one wrong `$schema` and every client drops the whole kit
# silently. Nothing else in this repo would notice.
#
# The rules are encoded here rather than fetched: a gate that needs the
# network is a gate that goes yellow on a plane. The spec text is
# authoritative and the version is pinned in SPEC_VERSION below; when
# 1.1.0 lands, this file is where the diff goes.
#
# Checked (spec section in parentheses):
#   plugin.json  present · valid JSON · $schema canonical (§5.2)
#                name present, 1-64, [a-z0-9.-], alnum ends, no -- or ..
#                (§5.5) · top-level keys within the closed set (§5.2)
#                author sub-keys within {name,email,url}, strings (§5.4)
#                version agrees with the workspace (vector 47's law)
#   mcp.json     present · valid JSON · $schema canonical, SAME version as
#                plugin.json (§10.1) · exactly {$schema, mcpServers} (§7.2.1)
#                each server: closed variant, required fields, no unknown
#                field, no PLUGIN_ROOT/PLUGIN_DATA in env (§7.2.1, §9.2)
#   skills/      each immediate child with SKILL.md conforms to the Agent
#                Skills spec the format defers to (§7.1): name 1-64,
#                [a-z0-9-], no leading/trailing/double hyphen, MATCHES the
#                directory name, description 1-1024 non-empty
#
# Exit: 0 GREEN · 2 RED (a client would reject or skip something) ·
#       1 YELLOW (cannot read the authority — not a clean bill).
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
cd "$ROOT" || exit 1

PLUGIN_DIR=".agents/plugins/nika"

want="$(grep -m1 -E '^version[[:space:]]*=' Cargo.toml | grep -oE '[0-9]+\.[0-9]+\.[0-9]+[^"]*')"
if [ -z "$want" ]; then
  echo "YELLOW: no workspace version in Cargo.toml — cannot compare"
  exit 1
fi

PLUGIN_DIR="$PLUGIN_DIR" WORKSPACE_VERSION="$want" python3 - <<'PY'
import json
import os
import re
import sys

SPEC_VERSION = "1.0.0"
PLUGIN_SCHEMA = f"https://agent-plugins.org/schemas/{SPEC_VERSION}/plugin.schema.json"
MCP_SCHEMA = f"https://agent-plugins.org/schemas/{SPEC_VERSION}/mcp.schema.json"

# §5.2 — the closed top-level set. Order is the spec's.
MANIFEST_KEYS = {
    "$schema", "name", "version", "description", "author",
    "homepage", "repository", "license", "keywords", "extensions",
}
AUTHOR_KEYS = {"name", "email", "url"}  # §5.4

# §5.5 plugin name · §7.1 defers skill names to the Agent Skills spec.
PLUGIN_NAME = re.compile(r"^(?!.*(?:--|\.\.))[a-z0-9](?:[a-z0-9.-]*[a-z0-9])?$")
SKILL_NAME = re.compile(r"^(?!.*--)[a-z0-9](?:[a-z0-9-]*[a-z0-9])?$")

# §7.2.1 — the closed server union: required fields, then optional ones.
SERVER_VARIANTS = {
    "stdio": ({"type", "command"}, {"args", "env", "cwd"}),
    "streamable-http": ({"type", "url"}, {"headers"}),
    "sse": ({"type", "url"}, {"headers"}),
}
RESERVED_ENV = {"PLUGIN_ROOT", "PLUGIN_DATA"}  # §9.2

root = os.environ["PLUGIN_DIR"]
workspace_version = os.environ["WORKSPACE_VERSION"]
reds = []


def red(msg):
    reds.append(msg)


def load(path, label):
    """Read JSON or record why a client would reject the plugin."""
    full = os.path.join(root, path)
    if not os.path.isfile(full):
        red(f"{full} is missing — {label}")
        return None
    try:
        with open(full, encoding="utf-8") as handle:
            return json.load(handle)
    except (OSError, ValueError) as exc:
        red(f"{full} is not readable JSON ({exc}) — a client rejects the plugin")
        return None


def schema_version(value):
    """Pull the version out of a canonical schema id, or None."""
    match = re.match(
        r"^https://agent-plugins\.org/schemas/([0-9.]+)/(plugin|mcp)\.schema\.json$",
        value if isinstance(value, str) else "",
    )
    return match.group(1) if match else None


# ── plugin.json (§5) ────────────────────────────────────────────────────
manifest = load("plugin.json", "the portable Agent Plugins manifest (§5.1)")
if manifest is not None:
    if not isinstance(manifest, dict):
        red("plugin.json is not a JSON object (§5.2) — fatal")
        manifest = {}

    got_schema = manifest.get("$schema")
    if got_schema != PLUGIN_SCHEMA:
        red(f"plugin.json $schema is {got_schema!r}, must be {PLUGIN_SCHEMA!r} (§5.2) — fatal")

    unknown = sorted(set(manifest) - MANIFEST_KEYS)
    if unknown:
        red(
            f"plugin.json carries {unknown} — the schema is closed (§5.2). "
            "Client-specific data belongs under `extensions` or in the client's own directory"
        )

    name = manifest.get("name")
    if not isinstance(name, str) or not name:
        red(f"plugin.json name is {name!r} — required, non-empty string (§5.3) — fatal")
    elif len(name) > 64 or not PLUGIN_NAME.match(name):
        red(f"plugin.json name {name!r} breaks the naming constraints (§5.5) — fatal")

    author = manifest.get("author")
    if author is not None:
        if not isinstance(author, dict):
            red("plugin.json author is not an object (§5.4) — fatal")
        else:
            extra = sorted(set(author) - AUTHOR_KEYS)
            if extra:
                red(f"plugin.json author carries {extra} — only name/email/url (§5.4) — fatal")
            for key, value in author.items():
                if not isinstance(value, str):
                    red(f"plugin.json author.{key} is not a string (§5.4) — fatal")

    for key in ("version", "description", "homepage", "repository", "license"):
        if key in manifest and not isinstance(manifest[key], str):
            red(f"plugin.json {key} is not a string (§5.4) — fatal")
    keywords = manifest.get("keywords")
    if keywords is not None and (
        not isinstance(keywords, list) or any(not isinstance(k, str) for k in keywords)
    ):
        red("plugin.json keywords is not a string array (§5.4) — fatal")

    # Vector 47's law, extended to the surface it does not know about.
    version = manifest.get("version")
    if isinstance(version, str) and version != workspace_version:
        red(
            f"plugin.json says {version}, the workspace says {workspace_version} "
            "— the portable manifest is a version surface too"
        )

# ── mcp.json (§7.2) ─────────────────────────────────────────────────────
mcp = load("mcp.json", "the portable MCP configuration (§7.2.1)")
if mcp is not None:
    if not isinstance(mcp, dict):
        red("mcp.json is not a JSON object (§7.2.1) — MCP disabled for the plugin")
        mcp = {}

    got_mcp_schema = mcp.get("$schema")
    if got_mcp_schema != MCP_SCHEMA:
        red(f"mcp.json $schema is {got_mcp_schema!r}, must be {MCP_SCHEMA!r} (§7.2.1)")

    extra_top = sorted(set(mcp) - {"$schema", "mcpServers"})
    if extra_top:
        red(f"mcp.json carries {extra_top} — only $schema and mcpServers (§7.2.1)")

    if manifest:
        plugin_v = schema_version(manifest.get("$schema"))
        mcp_v = schema_version(got_mcp_schema)
        if plugin_v and mcp_v and plugin_v != mcp_v:
            red(
                f"mcp.json targets Agent Plugins {mcp_v} but plugin.json targets {plugin_v} "
                "— a mismatch invalidates the MCP configuration (§10.1)"
            )

    servers = mcp.get("mcpServers")
    if servers is None:
        red("mcp.json has no mcpServers (§7.2.1) — required, may be empty")
    elif not isinstance(servers, dict):
        red("mcp.json mcpServers is not an object (§7.2.1)")
    else:
        for server_name, config in sorted(servers.items()):
            where = f"mcp.json server {server_name!r}"
            if not isinstance(config, dict):
                red(f"{where} is not an object (§7.2.1) — the server is skipped")
                continue
            kind = config.get("type")
            if kind not in SERVER_VARIANTS:
                red(f"{where} has type {kind!r} — not one of {sorted(SERVER_VARIANTS)} (§7.2.1)")
                continue
            required, optional = SERVER_VARIANTS[kind]
            missing = sorted(required - set(config))
            if missing:
                red(f"{where} is missing {missing} for type {kind!r} (§7.2.1)")
            foreign = sorted(set(config) - required - optional)
            if foreign:
                red(f"{where} carries {foreign} — the {kind!r} variant is closed (§7.2.1)")
            command = config.get("command")
            if kind == "stdio" and isinstance(command, str):
                if not command:
                    red(f"{where} command is empty (§7.2.1)")
                elif " " in command:
                    red(
                        f"{where} command {command!r} is a shell string — it MUST be one "
                        "executable token, arguments go in args (§7.2.1)"
                    )
                elif command.startswith("../") or command.startswith("/"):
                    red(
                        f"{where} command {command!r} must be a bare name or a ./ path "
                        "inside the plugin root (§4.1, §7.2.1)"
                    )
            env = config.get("env")
            if isinstance(env, dict):
                reserved = sorted(RESERVED_ENV & set(env))
                if reserved:
                    red(f"{where} env sets {reserved} — the client supplies those (§9.2)")

# ── skills/ (§7.1 → the Agent Skills spec) ──────────────────────────────
skills_dir = os.path.join(root, "skills")
if not os.path.isdir(skills_dir):
    red(f"{skills_dir} is not a directory — the fixed skills location (§6.1, §6.2)")
else:
    found = 0
    for entry in sorted(os.listdir(skills_dir)):
        skill_md = os.path.join(skills_dir, entry, "SKILL.md")
        if not os.path.isfile(skill_md):
            continue
        found += 1
        with open(skill_md, encoding="utf-8") as handle:
            text = handle.read()
        block = re.match(r"^---\n(.*?)\n---\n", text, re.S)
        if not block:
            red(f"{skill_md} has no YAML frontmatter — the client skips this skill (§7.1)")
            continue
        front = block.group(1)
        name_line = re.search(r"^name:[ \t]*(.+?)[ \t]*$", front, re.M)
        desc_line = re.search(r"^description:[ \t]*(.+?)[ \t]*$", front, re.M | re.S)
        if not name_line:
            red(f"{skill_md} frontmatter has no name — required (§7.1)")
        else:
            skill_name = name_line.group(1).strip().strip("\"'")
            if len(skill_name) > 64 or not SKILL_NAME.match(skill_name):
                red(f"{skill_md} name {skill_name!r} breaks the Agent Skills naming rules")
            if skill_name != entry:
                red(f"{skill_md} name {skill_name!r} must match its directory {entry!r}")
        if not desc_line:
            red(f"{skill_md} frontmatter has no description — required (§7.1)")
        else:
            description = desc_line.group(1).strip()
            if not description:
                red(f"{skill_md} description is empty (§7.1)")
            elif len(description) > 1024:
                red(f"{skill_md} description is {len(description)} chars — the ceiling is 1024")
    if found == 0:
        # A silent zero is the failure mode this whole file exists to catch.
        red(f"{skills_dir} holds no SKILL.md — an empty harvest is blind, not clean")

if reds:
    for message in reds:
        print(f"RED: {message}", file=sys.stderr)
    print(
        f"\n{len(reds)} Agent Plugins {SPEC_VERSION} violation(s). "
        "A conformant client rejects the plugin outright on a fatal manifest error;\n"
        "the portable surface is the one that reaches Cursor, Copilot, Kiro, VS Code "
        "and ChatGPT/Codex.",
        file=sys.stderr,
    )
    sys.exit(2)

print(f"OK: the kit conforms to Agent Plugins {SPEC_VERSION} (manifest · mcp · skills)")
PY
