#!/usr/bin/env python3
"""Reject retired authoring doors in live engine surfaces, including vendored teaching.

History is deliberately retained: release notes, ADRs and dated plans describe
past behavior. Public API projections and generated pack teaching remain live
and MUST pass; regenerate them through their owners instead of adding exclusions.
"""
from pathlib import Path
import re
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
HISTORY = ("CHANGELOG.md", "changelog.d/", "docs/adr/", "docs/plans/",
           ".agents/plugins/nika/CHANGELOG.md")
SELF = "scripts/hygiene/check-compile-cutover.py"
PATTERNS = (
    re.compile(r"\bnika new\b"),
    re.compile(r'''\[\s*["']new["']\s*,'''),
    re.compile(r"/nika:new\b"),
    re.compile(r"\bCommand::New\b"),
    re.compile(r"\bverbs::new\b"),
    re.compile(r"\b(?:nika_onboard|crate)::guided\b"),
    re.compile(r"\bDraftSource::(?:New|Guided)\b"),
    re.compile(r"(?:verbs/new\.rs|commands/new\.md|verb\.new\b)"),
)
DELETED = (
    "crates/nika-cli/src/verbs/new.rs",  # formerly: the retired implementation must be absent
    "crates/nika-onboard/src/guided.rs",  # formerly: the retired guided fork
    "crates/nika-onboard/src/guided/tests.rs",  # formerly: tests of the retired fork
    ".agents/plugins/nika/commands/new.md",  # formerly: the retired plugin door
)


def findings(root, paths):
    bad = []
    for rel in sorted(set(paths)):
        if rel == SELF or any(rel == h or (h.endswith("/") and rel.startswith(h)) for h in HISTORY):
            continue
        path = root / rel
        if not path.is_file():
            continue
        if rel in DELETED:
            bad.append(f"{rel}: retired implementation exists")
        try:
            lines = path.read_text().splitlines()
        except UnicodeError:
            continue
        for line, text in enumerate(lines, 1):
            if any(pattern.search(text) for pattern in PATTERNS):
                bad.append(f"{rel}:{line}: retired authoring reference")
    return bad


def mutation_proof():
    with tempfile.TemporaryDirectory(prefix="nika-cutover-ratchet-") as folder:
        root = Path(folder)
        script = root / SELF
        script.parent.mkdir(parents=True)
        script.write_text(Path(__file__).read_text())
        subprocess.run(["git", "init", "--quiet", str(root)], check=True)
        def gate():
            return subprocess.run([sys.executable, str(script)], capture_output=True, text=True)
        (root / "README.md").write_text("nika compile hello\n")
        assert gate().returncode == 0, "clean control failed"
        (root / "README.md").write_text("nika new hello\n")
        negative = gate()
        assert negative.returncode == 1 and "README.md:1" in negative.stdout, "negative teaching mutation escaped"
        (root / "README.md").write_text("nika compile hello\n")
        path = root / DELETED[0]
        path.parent.mkdir(parents=True)
        path.write_text("// resurrected implementation\n")
        negative = gate()
        assert negative.returncode == 1 and DELETED[0] in negative.stdout, "negative implementation mutation escaped"
        path.unlink()
        argv_script = root / "scripts/authoring-gauntlet.py"
        argv_script.write_text('run("case", ["new", name, dest], room)\n')
        negative = gate()
        assert negative.returncode == 1 and "scripts/authoring-gauntlet.py:1" in negative.stdout, "argv invocation escaped"
        argv_script.unlink()
        (root / "CHANGELOG.md").write_text("Historical nika new\n")
        assert gate().returncode == 0, "history was erased"
    print("PASS cutover negative mutations: README resurrection, retired module, argv invocation; explicit history retained")


if __name__ == "__main__":
    if sys.argv[1:] == ["--self-test"]:
        mutation_proof()
    elif sys.argv[1:]:
        sys.exit("usage: check-compile-cutover.py [--self-test]")
    else:
        listed = subprocess.check_output(
            ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=ROOT
        ).decode().split("\0")
        bad = findings(ROOT, [p for p in listed if p])
        if bad:
            print("\n".join(bad))
            sys.exit(1)
        print("PASS Compile cutover: no retired authoring references on live surfaces")
