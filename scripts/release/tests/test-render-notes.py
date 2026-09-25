#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Render real release notes without touching the caller's changelog."""
import pathlib
import subprocess
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).resolve().parents[1] / "render-notes.sh"


class ReleaseNotes(unittest.TestCase):
    def render(self, section: str) -> str:
        with tempfile.TemporaryDirectory() as scratch:
            path = pathlib.Path(scratch) / "CHANGELOG.md"
            source = f"# Changelog\n\n## [0.121.0]\n\n{section}\n\n## [0.120.3]\nOLD\n"
            path.write_text(source)
            result = subprocess.run(
                ["bash", str(SCRIPT), "v0.121.0"], cwd=scratch,
                text=True, capture_output=True, check=True,
            ).stdout
            self.assertEqual(path.read_text(), source)
            self.assertNotIn("OLD", result)
            self.assertIn("## Install", result)
            self.assertIn("## Provenance", result)
            return result

    def test_small_section_is_preserved(self):
        self.assertIn("- Preserve the user's columns.",
                      self.render("- Preserve the user's columns."))

    def test_oversized_section_links_complete_tagged_source(self):
        for text in ("x" * 133392, "🦋" * 20000):
            with self.subTest(bytes=len(text.encode())):
                result = self.render(text)
                self.assertLess(len(result.encode()), 64000)
                self.assertIn("/blob/v0.121.0/CHANGELOG.md", result)
                self.assertNotIn(text, result)

    def test_large_release_preserves_its_curated_introduction(self):
        intro = "Pair SDK 0.121.0 with this engine. Keep old histories when rolling back."
        result = self.render(intro + "\n\n### Fixed\n" + "x" * 133392)
        self.assertIn(intro, result)
        self.assertIn("/blob/v0.121.0/CHANGELOG.md", result)


if __name__ == "__main__":
    unittest.main()
