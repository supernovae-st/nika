#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
"""Reuse the real build-script executable across different package locations."""

import os
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]


class BuildScriptRelocationTests(unittest.TestCase):
    def check_package(self, package):
        with tempfile.TemporaryDirectory(prefix="nika-build-relocation-") as directory:
            root = Path(directory)
            locations = [root / "checkout-a", root / "checkout-b"]
            pins = ["a" * 40, "b" * 40]
            for checkout, pin in zip(locations, pins):
                (checkout / "crates" / package).mkdir(parents=True)
                pack = checkout / "crates/nika-pack/pack"
                pack.mkdir(parents=True)
                (checkout / "SPEC_PIN").write_text(f"# identity fixture\n{pin}\n")
                (pack / "SPEC_SHA").write_text(f"{pin}\n")

            executable = root / ("build-script.exe" if os.name == "nt" else "build-script")
            env = dict(os.environ)
            # The fixture has no repository, credentials or ambient Git identity.
            for name in tuple(env):
                if name.startswith("GIT_"):
                    env.pop(name)
            env.update(CARGO_PKG_VERSION="0.0.0-test", NIKA_BUILD_SHA="relocation-test")
            env["CARGO_MANIFEST_DIR"] = str(locations[0] / "crates" / package)
            compiled = subprocess.run(
                ["rustc", "--edition=2024", str(ROOT / "crates" / package / "build.rs"),
                 "-o", str(executable)],
                cwd=ROOT, env=env, capture_output=True, text=True, timeout=60,
            )
            self.assertEqual(compiled.returncode, 0, compiled.stderr)

            def execute(manifest):
                run_env = dict(env)
                if manifest is None:
                    run_env.pop("CARGO_MANIFEST_DIR")
                else:
                    run_env["CARGO_MANIFEST_DIR"] = str(manifest)
                return subprocess.run(
                    [str(executable)], cwd=root, env=run_env,
                    capture_output=True, text=True, timeout=10,
                )

            # A -> B -> A reuses ONE executable, as a shared Cargo target can.
            for index in [0, 1, 0]:
                with self.subTest(package=package, checkout=index):
                    manifest = locations[index] / "crates" / package
                    result = execute(manifest)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertIn(f"cargo:rustc-env=NIKA_SPEC_SHA={pins[index]}\n", result.stdout)
                    watched = [line for line in result.stdout.splitlines()
                               if line.startswith("cargo:rerun-if-changed=")]
                    self.assertTrue(watched)
                    self.assertTrue(all(str(manifest) in line for line in watched), watched)
                    self.assertNotIn(str(locations[1 - index]), result.stdout)

            # Do not read the old checkout when the current checkout is invalid.
            (locations[1] / "crates/nika-pack/pack/SPEC_SHA").write_text(pins[0])
            refused = execute(locations[1] / "crates" / package)
            self.assertNotEqual(refused.returncode, 0)
            self.assertIn("differs from embedded pack identity", refused.stderr)
            self.assertNotIn("cargo:rustc-env=NIKA_SPEC_SHA=", refused.stdout)
            for missing in [None, ""]:
                with self.subTest(package=package, manifest=missing):
                    refused = execute(missing)
                    self.assertNotEqual(refused.returncode, 0)
                    self.assertIn("CARGO_MANIFEST_DIR is missing or empty", refused.stderr)

    def test_runtime_uses_the_package_being_built(self):
        self.check_package("nika-runtime")

    def test_runtime_laws_uses_the_package_being_built(self):
        self.check_package("nika-runtime-laws")


if __name__ == "__main__":
    unittest.main()
