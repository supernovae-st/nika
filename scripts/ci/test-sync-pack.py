#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Exercise the pack owner's sync against disposable source/destination trees."""

import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]


class SyncPackTest(unittest.TestCase):
    def test_sync_prunes_navigation_and_removed_files_without_changing_pack_bytes(self):
        with tempfile.TemporaryDirectory(prefix="nika pack sync ") as temporary:
            root = Path(temporary)
            engine = root / "engine"
            spec = root / "spec"
            pack = engine / "crates/nika-pack/pack"
            (engine / "scripts").mkdir(parents=True)
            pack.mkdir(parents=True)
            shutil.copyfile(ROOT / "scripts/sync-pack.sh", engine / "scripts/sync-pack.sh")

            expected = {}
            for source, destination in (
                ("VERSION", "VERSION"),
                ("QUICKSTART.md", "QUICKSTART.md"),
                ("canon.yaml", "canon.yaml"),
                ("conformance/coverage-matrix.tsv", "coverage-matrix.tsv"),
                ("design/tokens.yaml", "design-tokens.yaml"),
                ("design/motion.yaml", "design-motion.yaml"),
                ("stdlib/reference.md", "stdlib/reference.md"),
            ):
                path = spec / source
                path.parent.mkdir(parents=True, exist_ok=True)
                expected[destination] = f"source bytes: {source}\n".encode()
                path.write_bytes(expected[destination])

            (spec / "stdlib/authoring-shapes.yaml").write_text("not a pack input\n")
            for directory in ("spec", "schemas", "examples", "templates"):
                (spec / directory / "nested").mkdir(parents=True)
                (pack / directory / "nested").mkdir(parents=True)
                for name in ("README.md", "nested/README.md"):
                    (spec / directory / name).write_text("current repo navigation\n")
                    (pack / directory / name).write_text("obsolete embedded navigation\n")
                (pack / directory / "removed.nika").write_text("removed at source\n")
                name = f"{directory}/nested/kept.nika"
                expected[name] = b"nika: kept\n# exact source bytes\n"
                (spec / name).write_bytes(expected[name])

            expected["schemas/project.schema.json"] = b'{"type": "object"}\n'
            (spec / "schemas/project.schema.json").write_bytes(expected["schemas/project.schema.json"])

            def git(*args):
                return subprocess.check_output(["git", "-C", str(spec), *args], text=True).strip()

            git("init", "--quiet")
            git("add", ".")
            git("-c", "user.name=Pack Test", "-c", "user.email=pack@example.invalid",
                "-c", "commit.gpgsign=false", "commit", "--quiet", "-m", "pack fixture")
            expected["SPEC_SHA"] = (git("rev-parse", "HEAD") + "\n").encode()

            # Run twice to prove stale content disappears and the next sync is stable.
            for _ in range(2):
                subprocess.run(["bash", str(engine / "scripts/sync-pack.sh"), str(spec)],
                               cwd=root, check=True, capture_output=True, text=True)
                actual = {str(path.relative_to(pack)): path.read_bytes()
                          for path in pack.rglob("*") if path.is_file()}
                self.assertEqual(actual, expected)


if __name__ == "__main__":
    unittest.main()
