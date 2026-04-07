#!/usr/bin/env python3
"""Extract Nika keywords from Rust source code into a single JSON file.

Parses the known-keyword arrays in nika-core to produce a canonical
nika-keywords.json consumed by all editor extensions (VS Code, Zed,
Neovim, Helix) and the LSP.

Usage:
  python3 editors/shared/extract-keywords.py > editors/shared/nika-keywords.json
  python3 editors/shared/extract-keywords.py --check  # verify freshness

Run from the nika repo root.

AGPL-3.0-or-later — SuperNovae Studio
"""

import json
import re
import sys
from datetime import datetime, timezone
from pathlib import Path

# ── Paths ────────────────────────────────────────────────────────────────────

SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parent.parent
CORE_DIR = REPO_ROOT / "tools" / "nika-core" / "src"

TRANSFORM_FILE = CORE_DIR / "binding" / "transform.rs"
BUILTINS_FILE = CORE_DIR / "catalogs" / "builtins.rs"
PARSER_FILE = CORE_DIR / "ast" / "raw" / "parser.rs"
ACTION_FILE = CORE_DIR / "ast" / "raw" / "action.rs"
PROVIDER_FILE = CORE_DIR / "provider_name.rs"
EXTRACT_FILE = CORE_DIR / "ast" / "extract.rs"
WORKSPACE_TOML = REPO_ROOT / "tools" / "Cargo.toml"

OUTPUT_FILE = SCRIPT_DIR / "nika-keywords.json"


def die(msg: str) -> None:
  print(f"ERROR: {msg}", file=sys.stderr)
  sys.exit(2)


# ── Extraction helpers ───────────────────────────────────────────────────────

def extract_rust_str_array(content: str, pattern: str) -> list[str]:
  """Extract string literals from a Rust array matched by pattern."""
  m = re.search(pattern, content, re.DOTALL)
  if not m:
    return []
  return re.findall(r'"([^"]+)"', m.group(1))


def extract_version() -> str:
  """Read workspace version from tools/Cargo.toml."""
  text = WORKSPACE_TOML.read_text()
  m = re.search(r'^version\s*=\s*"([^"]+)"', text, re.MULTILINE)
  return m.group(1) if m else "unknown"


def extract_verbs() -> list[str]:
  """Extract verb names from RawTaskAction enum variants."""
  content = ACTION_FILE.read_text()
  # Each variant line: Infer(...), Exec(...), etc.
  variants = re.findall(r"^\s+(\w+)\(", content, re.MULTILINE)
  # Map variant names to lowercase verb strings, skip Default impl
  verbs = []
  for v in variants:
    low = v.lower()
    if low in ("infer", "exec", "fetch", "invoke", "agent"):
      verbs.append(low)
  # Deduplicate while preserving order
  seen = set()
  return [v for v in verbs if not (v in seen or seen.add(v))]


def extract_transforms() -> list[str]:
  content = TRANSFORM_FILE.read_text()
  return extract_rust_str_array(
    content, r"pub static KNOWN_TRANSFORM_NAMES.*?=.*?\[([^\]]+)\]"
  )


def extract_builtins() -> list[str]:
  content = BUILTINS_FILE.read_text()
  return extract_rust_str_array(
    content, r"pub static KNOWN_BUILTIN_TOOLS.*?=.*?\[([^\]]+)\]"
  )


def extract_workflow_keys() -> list[str]:
  content = PARSER_FILE.read_text()
  return extract_rust_str_array(
    content, r"let known_workflow_keys.*?=.*?\[([^\]]+)\]"
  )


def extract_task_keys() -> list[str]:
  content = PARSER_FILE.read_text()
  return extract_rust_str_array(
    content, r"const KNOWN_TASK_KEYS.*?=.*?\[([^\]]+)\]"
  )


def extract_providers() -> dict:
  """Extract canonical provider names and their aliases."""
  content = PROVIDER_FILE.read_text()
  # Parse match arms: "canonical" | "alias" => Self::Variant,
  canonical = []
  aliases = {}
  for line in content.splitlines():
    m = re.match(r'\s+"(\w+)"(?:\s*\|\s*"(\w+)")*\s*(?:\|\s*"(\w+)")?\s*=>\s*Self::(\w+)', line)
    if not m:
      continue
    names = re.findall(r'"(\w+)"', line)
    if not names:
      continue
    # First name is canonical
    canon = names[0]
    canonical.append(canon)
    if len(names) > 1:
      aliases[canon] = names[1:]
  return {"canonical": canonical, "aliases": aliases}


def extract_extract_modes() -> list[str]:
  """Extract ALL_NAMES from ExtractMode."""
  content = EXTRACT_FILE.read_text()
  return extract_rust_str_array(
    content, r"pub const ALL_NAMES.*?=.*?\[([^\]]+)\]"
  )


# ── Main ─────────────────────────────────────────────────────────────────────

def main() -> None:
  # Verify source files exist
  for f in (TRANSFORM_FILE, BUILTINS_FILE, PARSER_FILE, ACTION_FILE, PROVIDER_FILE, EXTRACT_FILE):
    if not f.exists():
      die(f"Source file not found: {f}")

  verbs = extract_verbs()
  transforms = extract_transforms()
  builtins = extract_builtins()
  workflow_keys = extract_workflow_keys()
  task_keys = extract_task_keys()
  providers = extract_providers()
  extract_modes = extract_extract_modes()

  # Sanity checks
  if len(verbs) != 5:
    die(f"Expected 5 verbs, got {len(verbs)}: {verbs}")
  if len(transforms) < 60:
    die(f"Expected 60+ transforms, got {len(transforms)}")
  if len(builtins) < 60:
    die(f"Expected 60+ builtins, got {len(builtins)}")
  if len(extract_modes) < 9:
    die(f"Expected 9+ extract modes, got {len(extract_modes)}")

  version = extract_version()
  now = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")

  result = {
    "version": version,
    "generated": now,
    "verbs": verbs,
    "workflow_keys": workflow_keys,
    "task_keys": task_keys,
    "transforms": transforms,
    "builtins": builtins,
    "providers": providers,
    "extract_modes": extract_modes,
  }

  # --check mode: compare with existing file
  if "--check" in sys.argv:
    if not OUTPUT_FILE.exists():
      die(f"No existing {OUTPUT_FILE.name} — run without --check first")
    existing = json.loads(OUTPUT_FILE.read_text())
    # Compare everything except version and generated timestamp
    drift = False
    for key in ("verbs", "workflow_keys", "task_keys", "transforms", "builtins", "providers", "extract_modes"):
      if existing.get(key) != result[key]:
        print(f"DRIFT: {key} differs", file=sys.stderr)
        drift = True
    if drift:
      die("nika-keywords.json is stale — regenerate with: python3 editors/shared/extract-keywords.py > editors/shared/nika-keywords.json")
    print("nika-keywords.json is up to date", file=sys.stderr)
    sys.exit(0)

  print(json.dumps(result, indent=2))


if __name__ == "__main__":
  main()
