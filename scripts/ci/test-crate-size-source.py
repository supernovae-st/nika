#!/usr/bin/env python3
"""The crate wall must see embedded code and refuse an empty source inventory."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


HERE = Path(__file__).resolve().parent


class SourceInventory(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="nika-crate-source-")
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name)
        shutil.copytree(HERE, self.root / "scripts/ci", ignore=shutil.ignore_patterns("__pycache__"))
        self.env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
        self.env.pop("NIKA_SKIP_FILTER_SELFTEST", None)
        self.env["CRATE_SIZE_MAX"] = "4"
        self.git("init", "-q")
        self.write("crates/small/Cargo.toml", '[package]\nname = "small"\nversion = "0.1.0"\n')

    def git(self, *args):
        subprocess.run(["git", *args], cwd=self.root, env=self.env,
                       check=True, capture_output=True)

    def write(self, path, text):
        dest = self.root / path
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_text(text)

    def gate(self):
        return subprocess.run(["bash", "scripts/ci/check-crate-size.sh"],
                              cwd=self.root, env=self.env, text=True,
                              capture_output=True, timeout=30)

    def test_unindexed_export_cannot_be_green(self):
        self.write("crates/small/src/lib.rs", "pub fn a() {}\n")
        result = self.gate()
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertIn("manifest", result.stderr)

    def test_manifest_without_tracked_source_cannot_be_green(self):
        self.git("add", "crates/small/Cargo.toml")
        result = self.gate()
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertIn("source", result.stderr)

    def test_only_embedded_jq_takes_a_crate_over_the_wall(self):
        self.write("crates/small/src/lib.rs", "pub const LAW: &str = include_str!(\"law.jq\");\n")
        # A Rust-shaped comment in jq must not trigger Rust test exclusion.
        self.write("crates/small/src/law.jq", "#[cfg(test)]\n# kept comment\n\n.\n")
        self.git("add", "crates")
        result = self.gate()
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("5 LOC (max 4)", result.stdout)

    def test_exact_boundary_preserves_rust_test_exclusion(self):
        self.write("crates/small/src/lib.rs", "// kept\npub fn a() {}\n#[cfg(test)]\nmod tests { fn b() {} }\n")
        self.write("crates/small/src/law.jq", "# kept\n.")
        self.git("add", "crates")
        self.write("crates/small/src/untracked.jq", ".\n" * 10)
        result = self.gate()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.env["CRATE_SIZE_MAX"] = "3"
        result = self.gate()
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("4 LOC (max 3)", result.stdout)

    def test_missing_indexed_source_cannot_be_green(self):
        self.write("crates/small/src/lib.rs", "pub fn a() {}\n")
        self.git("add", "crates")
        (self.root / "crates/small/src/lib.rs").unlink()
        result = self.gate()
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertIn("lib.rs", result.stderr)

    def test_native_ceiling_is_bounded_and_the_probe_cannot_raise_it(self):
        self.write("crates/small/src/lib.rs", "// retained production documentation\n" * 15000)
        self.write("crates/nika-tui/Cargo.toml", '[package]\nname = "nika-tui"\nversion = "0.1.0"\n')
        self.write("crates/nika-tui/src/lib.rs", "// retained native composition documentation\n" * 18000)
        self.git("add", "crates")
        self.env.pop("CRATE_SIZE_MAX")
        at_boundary = self.gate()
        self.assertEqual(at_boundary.returncode, 0, at_boundary.stdout + at_boundary.stderr)
        self.write("crates/nika-tui/src/lib.rs", "// native composition\n" * 18001)
        self.env["CRATE_SIZE_MAX"] = "99999"
        above = self.gate()
        self.assertEqual(above.returncode, 1, above.stdout + above.stderr)
        self.assertIn("crates/nika-tui  18001 LOC (max 18000)", above.stdout)
        self.write("crates/small/src/lib.rs", "// other crate\n" * 15001)
        self.write("crates/nika-tui/src/lib.rs", "// native composition\n")
        other = self.gate()
        self.assertEqual(other.returncode, 1, other.stdout + other.stderr)
        self.assertIn("crates/small  15001 LOC (max 15000)", other.stdout)
        self.env["CRATE_SIZE_MAX"] = "4"
        self.write("crates/small/src/lib.rs", "// other crate\n")
        self.write("crates/nika-tui/src/lib.rs", "// native composition\n" * 5)
        lowered = self.gate()
        self.assertEqual(lowered.returncode, 1, lowered.stdout + lowered.stderr)
        self.assertIn("crates/nika-tui  5 LOC (max 4)", lowered.stdout)

    def test_fuzz_targets_stay_outside_production_scope_but_need_inventory(self):
        self.write("crates/small/src/lib.rs", "pub fn a() {}\n")
        self.write("fuzz/Cargo.toml", '[package]\nname = "fuzz"\nversion = "0.1.0"\n[package.metadata]\ncargo-fuzz = true\n')
        self.write("fuzz/fuzz_targets/check.rs", "// test code\n" * 10)
        self.git("add", "crates", "fuzz/Cargo.toml")
        missing = self.gate()
        self.assertEqual(missing.returncode, 2, missing.stdout + missing.stderr)
        self.git("add", "fuzz/fuzz_targets/check.rs")
        result = self.gate()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("SKIP  fuzz: cargo-fuzz test targets", result.stdout)


if __name__ == "__main__":
    unittest.main()
