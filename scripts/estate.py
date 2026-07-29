#!/usr/bin/env python3
"""estate.py — the estate manifest generator. THE canonical implementation.

    Every artifact is AUTHORED bytes or a proven derivation
    (hashed inputs, tool, pin) -> output + proof.

This file has ONE home: github.com/supernovae-st/nika-estate. Every repo that
carries an estate runs a byte-identical copy of it, mirrored and gated exactly
like the spec pack. If you are reading this inside another repository and feel
the urge to edit it, that is the lost gesture: change it upstream, mirror it
down, and the gate will tell you when the copy drifts.

What stays per-repo is the DATA, never the tool:

    scripts/estate_rules.py     FILES = [...]      the per-file exceptions
                                PATTERNS = [...]   ordered globs, first-match-wins

Both live next to this file in the consuming repo. This script imports them
and refuses to run without them, because a manifest with no rules would
silently declare an empty estate and pass.

    python3 scripts/estate.py --write    regenerate estate.yaml
    python3 scripts/estate.py --check    re-emit and byte-compare

    exit 0  in sync          exit 3  coverage hole or stale rule
    exit 2  bad usage        exit 5  drift

Dependency-free by design: the hundred-year machinery must run on a stock
interpreter in 2126, with no package manager still alive to serve it.
"""

import hashlib
import importlib.util
import json
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
ESTATE = ROOT / "estate.yaml"
ESTATE_SCHEMA = 2
SELF = "scripts/estate.py"
RULES = "scripts/estate_rules.py"

# The six classes. This vocabulary is the law and it is NOT per-repo: a repo
# that re-words a definition to fit its own domain has forked the law without
# saying so. The words below are the ones in nika-estate/SCHEMA.md.
CLASSES = {
    "authored": "a human writes here",
    "authored-pin": "a hand-meaningful pin a bot lane advances via PR-only \u2014 never HEAD; the derivations' INPUT",
    "generated": "derived by a tool from declared inputs \u2014 regenerate, never hand-edit",
    "pinned-copy": "byte-copy vendored from an upstream source by a declared lane, provenance declared, integrity-gated",
    "testimonial": "captured evidence \u2014 fixtures, corpora, traces, receipts: proves behavior; neither shipped source nor regenerable output",
    "foreign": "pointer to a third-party sovereign source",
}


def _repo_slug() -> str:
    """owner/name, from the origin remote. Derived, never typed."""
    try:
        url = subprocess.run(["git", "-C", str(ROOT), "remote", "get-url", "origin"],
                             capture_output=True, check=True).stdout.decode().strip()
    except (subprocess.CalledProcessError, FileNotFoundError):
        print("estate.py: no origin remote \u2014 cannot derive the repo slug", file=sys.stderr)
        sys.exit(3)
    slug = url.rsplit(":", 1)[-1].split("/")[-2:] if "/" in url else [url]
    return "/".join(slug).removesuffix(".git")


def _load_rules():
    """FILES + PATTERNS from the consuming repo. Absent is fatal, never empty."""
    path = ROOT / RULES
    if not path.is_file():
        print(f"estate.py: {RULES} not found \u2014 the tool is shared, the rules are yours",
              file=sys.stderr)
        sys.exit(3)
    # Importing writes a .pyc next to the rules unless we say otherwise, and
    # a manifest generator that litters the tree it measures is its own bug.
    sys.dont_write_bytecode = True
    spec = importlib.util.spec_from_file_location("estate_rules", path)
    mod = importlib.util.module_from_spec(spec)
    # The tool hands its well-known names down; the rules may cite them.
    mod.ROOT, mod.SELF, mod.RULES, mod.CLASSES = ROOT, SELF, RULES, CLASSES
    spec.loader.exec_module(mod)
    for name in ("FILES", "PATTERNS"):
        if not hasattr(mod, name):
            print(f"estate.py: {RULES} declares no {name}", file=sys.stderr)
            sys.exit(3)
    bad = {f["class"] for f in mod.FILES} | {p["class"] for p in mod.PATTERNS}
    bad -= set(CLASSES)
    if bad:
        print(f"estate.py: {RULES} uses classes outside the law: {sorted(bad)}", file=sys.stderr)
        sys.exit(3)
    return mod.FILES, mod.PATTERNS


REPO = _repo_slug()
FILES, PATTERNS = _load_rules()


def glob_to_re(glob: str) -> "re.Pattern":
    """Translate the manifest glob dialect to an anchored regex.

    `**` matches across path segments · `*` within one segment · `?` one
    non-slash char. Deterministic, dependency-free (pathlib.full_match
    needs 3.13; CI runners float)."""
    out, i = [], 0
    while i < len(glob):
        c = glob[i]
        if c == "*":
            if glob[i : i + 2] == "**":
                out.append(".*")
                i += 2
            else:
                out.append("[^/]*")
                i += 1
        elif c == "?":
            out.append("[^/]")
            i += 1
        else:
            out.append(re.escape(c))
            i += 1
    return re.compile("".join(out) + r"\Z")


def tracked_index() -> dict:
    """path → index blob sha, from `git ls-files -s -z`."""
    out = subprocess.run(["git", "-C", str(ROOT), "ls-files", "-s", "-z"],
                         capture_output=True, check=True)
    index = {}
    for rec in out.stdout.decode().split("\0"):
        if not rec:
            continue
        meta, path = rec.split("\t", 1)
        _mode, sha, _stage = meta.split(" ")
        index[path] = sha
    # The generator always classifies itself (a files: row hashing real
    # bytes, so it works pre-commit too); the manifest never lists itself
    # (its sha256 cannot contain its own hash).
    index.setdefault(SELF, "")
    index.pop("estate.yaml", None)
    return index


def q(s) -> str:
    """A JSON string is a valid YAML scalar — deterministic quoting for free."""
    return json.dumps(s, ensure_ascii=False)


def render() -> str:
    index = tracked_index()
    universe = sorted(index)

    # files: precedence — validate, hash real bytes.
    file_paths = [f["path"] for f in FILES]
    if len(set(file_paths)) != len(file_paths):
        print("estate.py: duplicate files: entries", file=sys.stderr)
        sys.exit(3)
    stale = [p for p in file_paths if p not in index or not (ROOT / p).is_file()]
    if stale:
        print("estate.py: stale files: exceptions (not in the tracked tree):",
              file=sys.stderr)
        for p in stale:
            print(f"  - {p}", file=sys.stderr)
        sys.exit(3)
    taken = set(file_paths)

    # patterns: first-match-wins over the remainder.
    compiled = [(glob_to_re(p["glob"]), p) for p in PATTERNS]
    matches = {p["glob"]: [] for p in PATTERNS}
    uncovered = []
    for path in universe:
        if path in taken:
            continue
        for rx, p in compiled:
            if rx.match(path):
                matches[p["glob"]].append(path)
                break
        else:
            uncovered.append(path)
    if uncovered:
        print("estate.py: COVERAGE HOLE — tracked files matched by no "
              "files: entry and no pattern (totality is the whole point):",
              file=sys.stderr)
        for p in uncovered:
            print(f"  - {p}", file=sys.stderr)
        sys.exit(3)

    counts = {c: 0 for c in CLASSES}
    for f in FILES:
        counts[f["class"]] += 1
    for p in PATTERNS:
        counts[p["class"]] += len(matches[p["glob"]])
    # Every unverified thing, however it got classified. A files: row can be
    # as honest about its ignorance as a pattern row, and the count is the
    # repo's honesty budget: it must not shrink because a file moved shape.
    unverified = sorted(
        [f["path"] for f in FILES if f.get("note") == "unverified-default"]
        + [m for p in PATTERNS if p.get("note") == "unverified-default"
           for m in matches[p["glob"]]])

    lines = [
        "# GENERATED by scripts/estate.py — do not hand-edit.",
        "# The estate manifest: every tracked file declares its provenance.",
        "# OBSERVATION MODE (E0): this declares what IS — it enforces nothing.",
        "# SCHEMA 2 — the glob estate: ordered patterns (first-match-wins) cover",
        "# the tree in bulk; files: carries the per-file exceptions. Per pattern:",
        "# match_count + one aggregate sha256 over the sorted (blob-sha, path)",
        "# pairs of its matches — coverage drift and content drift both surface.",
        "# Re-generate: python3 scripts/estate.py --write",
        "# Check:       python3 scripts/estate.py --check   (re-emits · byte-compares · exit 5 on divergence)",
        f"estate_schema: {ESTATE_SCHEMA}",
        "mode: observation",
        f"repo: {REPO}",
        "classes:",
    ]
    for c in CLASSES:
        lines.append(f"  {c}: {q(CLASSES[c])}")
    lines.append("summary:")
    lines.append(f"  classified_files: {len(universe)}")
    lines.append(f"  file_rows: {len(FILES)}")
    lines.append(f"  pattern_rows: {len(PATTERNS)}")
    lines.append("  by_class:")
    for c in CLASSES:
        lines.append(f"    {c}: {counts[c]}")
    lines.append(f"  unverified-default: {len(unverified)}")

    lines.append("files:")
    for f in sorted(FILES, key=lambda f: f["path"]):
        body = (ROOT / f["path"]).read_bytes()
        lines.append(f"- path: {q(f['path'])}")
        lines.append(f"  class: {f['class']}")
        lines.append(f"  sha256: {hashlib.sha256(body).hexdigest()}")
        lines.append(f"  evidence: {q(f['evidence'])}")
        if "derivation" in f:
            d = f["derivation"]
            lines.append("  derivation:")
            lines.append(f"    tool: {q(d['tool'])}")
            lines.append(f"    gate: {q(d['gate'])}")
            lines.append("    inputs:")
            for i in d["inputs"]:
                lines.append(f"    - {q(i)}")
        if "note" in f:
            lines.append(f"  note: {q(f['note'])}")

    lines.append("patterns:")
    for p in PATTERNS:
        matched = matches[p["glob"]]
        agg = hashlib.sha256(
            "".join(f"{index[m]}  {m}\n" for m in matched).encode()
        ).hexdigest()
        lines.append(f"- glob: {q(p['glob'])}")
        lines.append(f"  class: {p['class']}")
        lines.append(f"  match_count: {len(matched)}")
        lines.append(f"  aggregate_sha256: {agg}")
        lines.append(f"  evidence: {q(p['evidence'])}")
        if "derivation" in p:
            d = p["derivation"]
            lines.append("  derivation:")
            lines.append(f"    tool: {q(d['tool'])}")
            lines.append(f"    gate: {q(d['gate'])}")
            lines.append("    inputs:")
            for i in d["inputs"]:
                lines.append(f"    - {q(i)}")
        if "note" in p:
            lines.append(f"  note: {q(p['note'])}")

    lines.append("unverified:")
    if unverified:
        for m in unverified:
            lines.append(f"- {q(m)}")
    else:
        lines[-1] = "unverified: []"
    return "\n".join(lines) + "\n"


def main() -> int:
    mode = sys.argv[1] if len(sys.argv) > 1 else "--check"
    if mode not in ("--write", "--check"):
        print(f"estate.py: unknown mode {mode!r} (--write | --check)", file=sys.stderr)
        return 2
    rendered = render()
    if mode == "--write":
        ESTATE.write_text(rendered)
        n = rendered.count("\n- glob: ") + rendered.count("\n- path: ")
        print(f"✓ estate.yaml · schema {ESTATE_SCHEMA} · {n} rows")
        return 0
    if not ESTATE.is_file() or ESTATE.read_text() != rendered:
        print("✗ estate drift · estate.yaml diverges from the tracked tree — run scripts/estate.py --write", file=sys.stderr)
        return 5
    print("✓ estate.yaml in sync with the tracked tree")
    return 0


if __name__ == "__main__":
    sys.exit(main())
