#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 SuperNovae Studio <contact@supernovae.studio>
#
# estate.py — the provenance manifest: every tracked file declares what it IS.
#
# OBSERVATION MODE (E0): estate.yaml declares provenance — authored ·
# authored-pin · generated · pinned-copy · testimonial · foreign — it
# enforces nothing. The point is to make the repo's derivation topology
# explicit: which files a human writes, which files a tool re-derives
# (and from what, gated by which CI check), so drift between "what we
# believe" and "what is" becomes observable.
#
# SCHEMA 2 — the glob estate. This repo tracks thousands of files;
# per-file rows (schema 1, nika-registry) do not scale here. Instead:
#   patterns:  ordered glob rows, FIRST-MATCH-WINS — class + shared
#              evidence + derivation template. Per pattern the manifest
#              records match_count (coverage drift is visible) and ONE
#              aggregate sha256 over the sorted (blob-sha, path) pairs
#              of its matches (cheap, deterministic, catches any change
#              under the pattern) — never per-file hashes.
#   files:     per-file EXCEPTIONS (singletons: SPEC_PIN, generated
#              lockfiles, bot-blocked docs) with a real per-file sha256.
#              files: entries take precedence over every pattern.
# TOTALITY: every tracked file must land in exactly one bucket — the
# script errors (exit 3) listing uncovered paths otherwise. The LAST
# pattern is a `**` catchall: class authored + note unverified-default —
# honesty over completeness; its matches are reported under unverified:.
#
# Classification is EVIDENCE-driven, never assumed: every evidence
# string below cites the marker, header, workflow or consumer that was
# read in the tree (SPEC_PIN's own header · pack-resync.yml · the
# public-api.yml regen comment · media/README.md's "Never edit exports
# by hand" · fuzz-nightly.yml's corpus note).
#
# Derived, never authored (the house projection pattern): --write
# regenerates estate.yaml from the tracked tree; --check re-emits and
# byte-compares. On divergence --check exits 5 (the ssot-compiler
# convention — deliberately distinct from the sibling drift gates'
# exit 1, so an estate divergence is distinguishable in CI logs).
#
# Usage:
#   python3 scripts/estate.py --write   # regenerate estate.yaml
#   python3 scripts/estate.py --check   # drift gate (exit 5 on divergence)

import hashlib
import json
import pathlib
import re
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
ESTATE = ROOT / "estate.yaml"
ESTATE_SCHEMA = 2
SELF = "scripts/estate.py"

CLASSES = {
    "authored": "a human writes here",
    "authored-pin": "a hand-meaningful pin a bot lane advances via PR-only — never HEAD; the derivations' INPUT",
    "generated": "derived by a tool from declared inputs — regenerate, never hand-edit",
    "pinned-copy": "byte-copy vendored from an upstream repo by a declared lane, integrity-gated",
    "testimonial": "impl-owned test evidence — fixtures, corpora, batteries: proves behavior; neither shipped code nor derived output",
    "foreign": "pointer to a third-party sovereign source",
}

# ── files: per-file EXCEPTIONS (precedence over every pattern) ──────────────

FILES = [
    {
        "path": SELF,
        "class": "authored",
        "evidence": "hand-written estate projector (this manifest's generator)",
    },
    {
        "path": "SPEC_PIN",
        "class": "authored-pin",
        "evidence": "its own header: 'Advanced by .github/workflows/spec-pin-heal.yml (PR-only · the Diamond CI tests leg judges at the new pin). Consumed by diamond-ci.yml. Never HEAD.'",
    },
    {
        "path": "Cargo.lock",
        "class": "generated",
        "evidence": "cargo's resolver writes it; no human edits a lockfile",
        "derivation": {
            "tool": "cargo (dependency resolution against Cargo.toml + crates/*/Cargo.toml)",
            "gate": "--locked builds across the CI rails (diamond-ci.yml · release.yml) — a stale lock fails resolution",
            "inputs": ["Cargo.toml", "crates/*/Cargo.toml"],
        },
    },
    {
        "path": "fuzz/Cargo.lock",
        "class": "generated",
        "evidence": "cargo's resolver writes it for the fuzz workspace; no human edits a lockfile",
        "derivation": {
            "tool": "cargo (dependency resolution against fuzz/Cargo.toml)",
            "gate": "fuzz-nightly.yml builds the fuzz workspace",
            "inputs": ["fuzz/Cargo.toml"],
        },
    },
    {
        "path": "CHANGELOG.md",
        "class": "generated",
        "evidence": "changelog-cliff.yml commits 'docs(changelog): append <tag> — auto-generated' on tag push (git-cliff), branch chore/changelog-<tag>, PR body 'Review, edit if needed, merge'",
        "derivation": {
            "tool": "git-cliff (cliff.toml) via .github/workflows/changelog-cliff.yml on tag push",
            "gate": "idempotence guard in the workflow (existing '## [<ver>]' section skips) + human review at the PR",
            "inputs": ["git tag + commit history", "cliff.toml"],
        },
        "note": "generated with a human gate — the maintainer may amend the section at PR review",
    },
    {
        "path": "docs/adr/index.toml",
        "class": "generated",
        "evidence": "in-file marker: 'ADR Index -- auto-generated by scripts/adr/generate-index.sh · Do not edit manually'",
        "derivation": {
            "tool": "scripts/adr/generate-index.sh",
            "gate": "scripts/hygiene/check-adr-index-parity.sh",
            "inputs": ["docs/adr/adr-*.md YAML frontmatter"],
        },
    },
    {
        "path": "crates/nika-catalog/data/model-pricing.toml",
        "class": "generated",
        "evidence": "in-file marker '── GENERATED · do not hand-edit' + [meta] source=https://models.dev/api.json · as_of · source_sha256_16 — a vendored snapshot, refreshed at maintenance time, never a runtime fetch",
        "derivation": {
            "tool": "bash scripts/refresh-pricing.sh (emission laws matched to src/data/models.rs lookup semantics)",
            "gate": ".github/workflows/catalog-verify.yml (daily probe of catalog entries against upstream registries)",
            "inputs": ["https://models.dev/api.json (MIT · sst/models.dev)"],
        },
    },
    {
        "path": ".claude/CLAUDE.md",
        "class": "authored",
        "evidence": "hand-written harness entry for the Diamond rebuild (rules prose · gate map)",
        "note": "carries the '<!-- AUTO-GENERATED by scripts/refresh-status.sh — do not edit by hand -->' status block (vector 23 parity-enforced) — a tool-maintained block inside an authored file",
    },
    {
        "path": "ROADMAP.md",
        "class": "authored",
        "evidence": "hand-written roadmap prose (real-semver ladder · layer plan)",
        "note": "ROADMAP.md:98 carries the same refresh-status.sh AUTO-GENERATED status block — a tool-maintained block inside an authored file",
    },
    {
        "path": "media/README.md",
        "class": "authored",
        "evidence": "the authored rules for the media lanes ('No fake commands · No fake output · budgets · Never edit exports by hand')",
    },
]

# ── patterns: ordered glob rows — FIRST-MATCH-WINS ──────────────────────────

PATTERNS = [
    {
        "glob": "crates/nika-pack/pack/**",
        "class": "pinned-copy",
        "evidence": "crates/nika-pack/src/lib.rs:16 'The snapshot under pack/ is vendored by scripts/sync-pack.sh' — a byte-copy of the spec surface, never edited here",
        "derivation": {
            "tool": "bash scripts/sync-pack.sh <nika-spec checkout> — healed daily by .github/workflows/pack-resync.yml (branch bot/pack-resync · PR-only, the 12-gate CI judges)",
            "gate": "cargo test -p nika-pack → tests/pack_integrity.rs re-hashes every workflow (sha256_16 over lean text) against pack/examples/manifest.yaml",
            "inputs": ["supernovae-st/nika-spec@main: VERSION · QUICKSTART.md · canon.yaml · conformance/coverage-matrix.tsv · design/{tokens,motion}.yaml · spec/ · schemas/ · examples/ · templates/ · stdlib/*.md"],
        },
        "note": "pack/examples/manifest.yaml is itself GENERATED upstream (spec scripts/showcase-projector.py) — vendored here as part of the copy",
    },
    {
        "glob": "crates/nika-runtime/fixtures/adversarial/**",
        "class": "testimonial",
        "evidence": "attack.nika.yaml + expected.json pairs consumed by crates/nika-runtime/src/adversarial/fixtures.rs — the injection red-team battery owned by the runtime",
    },
    {
        "glob": "fuzz/corpus/**",
        "class": "testimonial",
        "evidence": "fuzz-nightly.yml:12-13 'Corpus is committed under fuzz/corpus/ (seeded from nika-spec examples + conformance fixtures) · new corpus discovered by CI is NOT pushed back'",
    },
    {
        "glob": "scripts/test/battery/**",
        "class": "testimonial",
        "evidence": "manifest.tsv + s*.nika.yaml scenarios consumed by scripts/test/structured-live-battery.sh (Rail C · the LIVE provider rail)",
    },
    {
        "glob": "crates/*/public-api.txt",
        "class": "generated",
        "evidence": ".github/workflows/public-api.yml 'diff each covered lib crate' byte-compares each snapshot against cargo public-api output; macOS renders differently, so snapshots regenerate FROM THE UBUNTU ARTIFACT (workflow comment)",
        "derivation": {
            "tool": "cargo public-api -p <crate> --all-features --omit auto-trait-impls > crates/<crate>/public-api.txt (cargo-public-api 0.51.0 pinned · regenerate from the public-api-actual CI artifact)",
            "gate": ".github/workflows/public-api.yml (drift → RED) + semver-checks.yml (breaking-vs-additive) + scripts/ci/public-api-coverage-baseline.txt (the monotonic floor)",
            "inputs": ["crates/<crate>/src/**", "crates/<crate>/Cargo.toml", "cargo-public-api 0.51.0"],
        },
    },
    {
        "glob": "crates/**/src/**",
        "class": "authored",
        "evidence": "the code — SPDX AGPL-3.0-or-later headers · 12-gate admission (ADR-003) · zero-unwrap discipline; spot-checked file heads carry no generation marker (grep hits for 'generated' are prose mentions, not markers)",
    },
    {
        "glob": "crates/**/tests/**",
        "class": "authored",
        "evidence": "crate-owned tests + fixtures under tests/ — written with the crate per gate discipline, no generation markers",
    },
    {
        "glob": "crates/**",
        "class": "authored",
        "evidence": "the remaining crate surface — Cargo.toml manifests · build.rs · benches · READMEs · hand-curated data catalogs (llm-providers · mcp-servers · embeddings · model-capabilities, probed daily by catalog-verify.yml); the pricing snapshot is excepted in files:",
        "note": "crates/nika-pack/scripts/sync-pack.sh is a DIVERGENT older duplicate of scripts/sync-pack.sh — the pack-resync.yml lane runs the root one",
    },
    {
        "glob": "fuzz/**",
        "class": "authored",
        "evidence": "4 hand-written fuzz targets + the fuzz workspace config (fuzz/Cargo.toml · .gitignore); corpus excepted above, lockfile in files:",
    },
    {
        "glob": "docs/adr/**",
        "class": "authored",
        "evidence": "per-decision records authored at decision time (scripts/adr/new.sh scaffolds · adr-seal-check hook seals); index.toml excepted in files:",
    },
    {
        "glob": "docs/**",
        "class": "authored",
        "evidence": "crate-specs · architecture prose · perf notes — no generation markers at file heads",
    },
    {
        "glob": "scripts/**",
        "class": "authored",
        "evidence": "hand-written gates · hooks · hygiene vectors · media lanes; the ratchet baselines (scripts/ci/*-baseline.txt · scripts/hygiene/baselines/) are hand-kept monotonic floors ('Never remove a line by hand')",
    },
    {
        "glob": ".github/workflows/**",
        "class": "authored",
        "evidence": "hand-written CI rails, comments narrate design decisions; the heal lanes here rewrite OTHER files (spec-pin-heal.yml → SPEC_PIN · pack-resync.yml → the pack · changelog-cliff.yml → CHANGELOG.md), nothing rewrites the workflows themselves",
    },
    {
        "glob": ".github/**",
        "class": "authored",
        "evidence": "issue/PR templates · CODEOWNERS · dependabot config — hand-written review policy",
    },
    {
        "glob": ".agents/**",
        "class": "authored",
        "evidence": "the agent-plugin pack SOURCE (marketplace.json · plugin manifests · skills · rules · hooks) — authored here, mirrored downstream into supernovae-st/nika-agents (its plugins/ is a byte-pinned copy healed on releases)",
    },
    {
        "glob": ".claude/**",
        "class": "authored",
        "evidence": "harness rules · skills · commands for the Diamond rebuild — hand-written; .claude/CLAUDE.md excepted in files: (carries the refresh-status.sh auto-block)",
    },
    {
        "glob": ".claude-plugin/**",
        "class": "authored",
        "evidence": "the Claude plugin marketplace manifest — hand-written",
    },
    {
        "glob": ".cargo/**",
        "class": "authored",
        "evidence": "cargo config · audit.toml · mutants.toml — hand-kept workspace policy",
    },
    {
        "glob": "media/**",
        "class": "generated",
        "evidence": "media/README.md: 'Never edit exports by hand. Edit the motion scene or fixture, then re-render' — gifs/posters/videos/social render from scripts/media/motion/ scenes + tapes; raw/ transcripts are captured from the real binary",
        "derivation": {
            "tool": "scripts/media/render-motion.mjs + render-tape.sh + capture-transcripts.sh (scenes: scripts/media/motion/ · fixtures: scripts/media/fixtures/ · tapes: scripts/media/tapes/)",
            "gate": "scripts/media/validate-media.sh (every shown workflow passes nika check · budgets enforced)",
            "inputs": ["scripts/media/motion/**", "scripts/media/fixtures/**", "scripts/media/tapes/**", "the real nika binary (captured transcripts)"],
        },
        "note": "media/README.md excepted in files: (the authored rules for these lanes)",
    },
    {
        "glob": "examples/**",
        "class": "authored",
        "evidence": "examples/README.md — a pointer README (the runnable examples live in the vendored pack + the spec repo)",
    },
    {
        "glob": "*",
        "class": "authored",
        "evidence": "root prose + config surface (README · DIAMOND · LICENSE · llms.txt · Cargo.toml · deny/clippy/cliff/typos toml · lefthook.yml · flake.nix · Dockerfile) — no generation markers at file heads; CHANGELOG.md · Cargo.lock · SPEC_PIN · ROADMAP.md excepted in files:",
    },
    {
        "glob": "**",
        "class": "authored",
        "evidence": "no evidence gathered — honesty over completeness (the catchall; anything landing here is reported under unverified:)",
        "note": "unverified-default",
    },
]


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
    unverified = sorted(m for p in PATTERNS if p.get("note") == "unverified-default"
                        for m in matches[p["glob"]])

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
        "repo: supernovae-st/nika",
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
