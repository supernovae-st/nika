#!/usr/bin/env python3
"""Hermetic source-selection and stable release-ID download regressions."""

import hashlib
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]
SCRIPTS = ROOT / "scripts/release"
TAG = "v9.9.9"
SHA = "2" * 40
NATIVES = [f"nika-{p}-9.9.9.tar.gz" for p in
           ("macos-arm64", "macos-x64", "linux-arm64", "linux-x64")]
NPM = "supernovae-st-nika-check-wasm-9.9.9.tgz"
NAMES = [*NATIVES, "SHA256SUMS", NPM, NPM + ".sha256"]


def fixture(directory, prefix):
    directory.mkdir()
    for name in [*NATIVES, NPM]:
        (directory / name).write_bytes(f"{prefix}:{name}\n".encode())
    def checksum(name):
        return hashlib.sha256((directory / name).read_bytes()).hexdigest() + "  " + name + "\n"
    (directory / "SHA256SUMS").write_text("".join(checksum(n) for n in NATIVES))
    (directory / (NPM + ".sha256")).write_text(checksum(NPM))


class Payload(unittest.TestCase):
    def setUp(self):
        scratch = tempfile.TemporaryDirectory(prefix="release-payload-test-")
        self.addCleanup(scratch.cleanup)
        self.root = Path(scratch.name)
        self.original = self.root / "original"
        self.rebuilt = self.root / "rebuilt"
        fixture(self.original, "original-signed")
        fixture(self.rebuilt, "recompiled-different")
        self.output = self.root / "selected"

    def select(self, event):
        return subprocess.run(
            [sys.executable, str(SCRIPTS / "release-payload.py"), "select", event, TAG,
             str(self.rebuilt), str(self.rebuilt), str(self.original), str(self.output)],
            capture_output=True, text=True, check=False,
        )

    def test_replay_uses_original_native_manifest_npm_and_sidecar_bytes(self):
        result = self.select("workflow_dispatch")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(sorted(p.name for p in self.output.iterdir()), sorted(NAMES))
        for name in NAMES:
            self.assertEqual((self.output / name).read_bytes(), (self.original / name).read_bytes())
            self.assertNotEqual((self.output / name).read_bytes(), (self.rebuilt / name).read_bytes())

    def test_push_preserves_generated_bytes_and_ignores_replay_sources(self):
        for path in self.original.iterdir():
            path.write_text("untrusted replay input")
        result = self.select("push")
        self.assertEqual(result.returncode, 0, result.stderr)
        for name in NAMES:
            self.assertEqual((self.output / name).read_bytes(), (self.rebuilt / name).read_bytes())

    def test_replay_never_falls_back_to_rebuilt_bytes_for_each_missing_asset(self):
        for name in NAMES:
            with self.subTest(name=name):
                path = self.original / name
                content = path.read_bytes()
                path.unlink()
                result = self.select("workflow_dispatch")
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(self.output.exists())
                path.write_bytes(content)

    def test_tampered_native_or_npm_bytes_are_refused(self):
        for name in [*NATIVES, NPM]:
            with self.subTest(name=name):
                path = self.original / name
                content = path.read_bytes()
                path.write_text("replacement")
                result = self.select("workflow_dispatch")
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("checksum mismatch", result.stderr)
                self.assertFalse(self.output.exists())
                path.write_bytes(content)

    def test_manifest_cannot_omit_duplicate_or_escape_a_payload_name(self):
        path = self.original / "SHA256SUMS"
        good = path.read_text()
        for bad in ("", good.splitlines()[0] + "\n", good + good.splitlines()[0] + "\n",
                    good.replace(NATIVES[0], "../" + NATIVES[0])):
            with self.subTest(manifest=bad):
                path.write_text(bad)
                self.assertNotEqual(self.select("workflow_dispatch").returncode, 0)
                self.assertFalse(self.output.exists())
        path.write_text(good)
        (self.original / (NPM + ".sha256")).write_text(good)
        self.assertNotEqual(self.select("workflow_dispatch").returncode, 0)
        self.assertFalse(self.output.exists())

    def test_existing_output_and_symlink_input_are_refused(self):
        self.output.mkdir()
        (self.output / "evidence").write_text("retained")
        self.assertNotEqual(self.select("workflow_dispatch").returncode, 0)
        self.assertEqual((self.output / "evidence").read_text(), "retained")
        (self.output / "evidence").unlink()
        self.output.rmdir()
        (self.original / NATIVES[0]).unlink()
        (self.original / NATIVES[0]).symlink_to(self.rebuilt / NATIVES[0])
        self.assertNotEqual(self.select("workflow_dispatch").returncode, 0)
        self.assertFalse(self.output.exists())

    def test_unknown_event_never_selects_a_default(self):
        self.assertNotEqual(self.select("pull_request").returncode, 0)
        self.assertFalse(self.output.exists())


class Download(unittest.TestCase):
    def setUp(self):
        Payload.setUp(self)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        (self.original / "multiple.intoto.jsonl").write_text("existing-statement")
        git = self.bin / "git"
        git.write_text('#!/bin/sh\n[ "$1" = ls-remote ] || exit 90\n'
                       f"printf '%s\\trefs/tags/{TAG}\\n' {SHA}\n")
        gh = self.bin / "gh"
        gh.write_text('''#!/usr/bin/env python3
import os, sys
from pathlib import Path
root = Path(os.environ['FIXTURE_ROOT'])
case = os.environ.get('FIXTURE_CASE', 'ok')
args = sys.argv[1:]
with (root / 'calls').open('a') as log:
    log.write(' '.join(args) + '\\n')
if args[0] != 'api' or '--method' in args:
    sys.exit(90)
endpoint = args[1]
if case == 'forbidden':
    sys.exit(1)
if endpoint == 'repos/example/nika/releases/123':
    count = (root / 'calls').read_text().count(endpoint + ' --jq')
    tag = 'v8.8.8' if case == 'state-drift' and count > 1 else 'v9.9.9'
    print('123\\t' + tag + '\\ttrue\\tfalse')
elif endpoint == 'repos/example/nika/releases/123/assets':
    assert '--paginate' in args
    count = (root / 'calls').read_text().count(endpoint + ' --paginate')
    for i, path in enumerate(sorted((root / 'original').iterdir()), 100):
        if case == 'missing' and i == 100:
            continue
        asset_id = i + 1000 if case == 'id-drift' and count > 1 else i
        print(str(asset_id) + '\\t' + path.name)
        if case == 'duplicate' and i == 100:
            print(str(asset_id + 1000) + '\\t' + path.name)
        if case == 'duplicate-id' and i == 100:
            print(str(asset_id) + '\\textra')
elif endpoint.startswith('repos/example/nika/releases/assets/'):
    assert 'Accept: application/octet-stream' in args
    index = int(endpoint.rsplit('/', 1)[1]) - 100
    path = sorted((root / 'original').iterdir())[index]
    sys.stdout.buffer.write(b'tampered' if case == 'tampered' else path.read_bytes())
    if case == 'partial':
        sys.exit(1)
else:
    sys.exit(90)
''')
        for script in (git, gh):
            script.chmod(0o755)
        self.env = {**os.environ, "PATH": str(self.bin) + os.pathsep + os.environ["PATH"],
                    "FIXTURE_ROOT": str(self.root), "GH_TOKEN": "synthetic-fixture"}

    def download(self, case="ok"):
        return subprocess.run(["bash", str(SCRIPTS / "read-release-payload.sh"),
                               "example/nika", "123", TAG, SHA, str(self.output)],
                              env={**self.env, "FIXTURE_CASE": case},
                              capture_output=True, text=True, check=False, timeout=15)

    def test_download_pins_exact_asset_ids_and_preserves_original_bytes(self):
        result = self.download()
        self.assertEqual(result.returncode, 0, result.stderr)
        for name in NAMES:
            self.assertEqual((self.output / name).read_bytes(), (self.original / name).read_bytes())
        calls = (self.root / "calls").read_text()
        self.assertEqual(calls.count("Accept: application/octet-stream"), 7)
        self.assertNotIn("releases/tags/", calls)
        self.assertNotIn("--method", calls)

    def test_missing_changed_duplicate_partial_and_tampered_assets_refuse(self):
        for case in ("forbidden", "missing", "duplicate", "duplicate-id", "id-drift",
                     "state-drift", "partial", "tampered"):
            with self.subTest(case=case):
                (self.root / "calls").write_text("")
                result = self.download(case)
                self.assertNotEqual(result.returncode, 0, result.stderr)
                self.assertFalse(self.output.exists())

    def test_existing_download_output_is_never_overwritten(self):
        self.output.mkdir()
        (self.output / "evidence").write_text("original")
        self.assertNotEqual(self.download().returncode, 0)
        self.assertEqual((self.output / "evidence").read_text(), "original")
        self.assertFalse((self.root / "calls").exists())


if __name__ == "__main__":
    unittest.main()
