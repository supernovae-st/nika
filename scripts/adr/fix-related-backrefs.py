#!/usr/bin/env python3
"""Fix bidirectional related backrefs in ADR frontmatter.

For every ADR pair (A, B) where A.related includes B but B.related does NOT
include A, add A to B.related (sorted · idempotent · re-run safe).

Canonical maintenance tool · runs zero-arg from anywhere · scans
nika/engine/docs/adr/adr-*.md · preserves all other frontmatter and body.

Per Diamond Rule 4 (rewrite · not verbatim copy) this script is the
SuperNovae batch-fix tool · not a port. Ships as a permanent maintenance
utility alongside scripts/adr/{validate.sh,new.sh,generate-index.sh}.

Usage:
    python3 scripts/adr/fix-related-backrefs.py

Exit codes:
    0 · zero changes needed (graph already consistent) OR changes applied cleanly
    1 · parse error / write error
"""
from __future__ import annotations

import re
import sys
import pathlib

ADR_DIR = pathlib.Path(__file__).resolve().parent.parent.parent / "docs" / "adr"

ID_RE = re.compile(r"^id:\s*(\S+)", re.MULTILINE)
RELATED_RE = re.compile(r"^related:\s*\[(.*?)\]", re.MULTILINE)
QUOTED_RE = re.compile(r'"([^"]+)"')


def parse_adr(file_path: pathlib.Path):
    """Extract id + related list + raw text from an ADR file."""
    text = file_path.read_text()
    id_match = ID_RE.search(text)
    related_match = RELATED_RE.search(text)
    if not id_match or not related_match:
        return None, None, None
    adr_id = id_match.group(1).strip().strip('"')
    related = QUOTED_RE.findall(related_match.group(1))
    return adr_id, related, text


def main() -> int:
    if not ADR_DIR.is_dir():
        print(f"ERROR: ADR_DIR does not exist: {ADR_DIR}", file=sys.stderr)
        return 1

    adrs: dict[str, dict] = {}
    for f in sorted(ADR_DIR.glob("adr-*.md")):
        adr_id, related, text = parse_adr(f)
        if adr_id is None:
            print(f"  skip (no id/related): {f.name}")
            continue
        adrs[adr_id] = {"file": f, "related": list(related), "text": text}

    # Build missing backref map: target_adr_id -> set of adr_ids to add
    fixes: dict[str, set[str]] = {}
    for adr_id, info in adrs.items():
        for ref in info["related"]:
            if ref in adrs and adr_id not in adrs[ref]["related"]:
                fixes.setdefault(ref, set()).add(adr_id)

    if not fixes:
        print("Graph already consistent · no backrefs to add.")
        return 0

    files_modified = 0
    pairs_fixed = 0
    for target_id in sorted(fixes.keys()):
        to_add = fixes[target_id]
        target = adrs[target_id]
        new_related = sorted(set(target["related"]) | to_add)
        old_match = RELATED_RE.search(target["text"])
        if old_match is None:
            print(f"  ERROR: cannot relocate related: line in {target['file'].name}", file=sys.stderr)
            return 1
        old_line = old_match.group(0)
        new_line = "related: [" + ", ".join(f'"{r}"' for r in new_related) + "]"
        new_text = target["text"].replace(old_line, new_line, 1)
        target["file"].write_text(new_text)
        files_modified += 1
        pairs_fixed += len(to_add)
        print(f"  {target_id}: added {sorted(to_add)}")

    print()
    print(f"Files modified: {files_modified}")
    print(f"Pairs fixed:    {pairs_fixed}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
