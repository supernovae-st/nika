#!/usr/bin/env python3
"""The release index distinguishes runnable images from bound BuildKit metadata."""

import copy
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[3]
IMAGE = "ghcr.io/supernovae-st/nika"
SHA = "a" * 40
DIGEST = "sha256:" + "f" * 64
MEDIA = "application/vnd.oci.image.manifest.v1+json"


def index():
    images = [
        {"mediaType": MEDIA, "digest": "sha256:" + digit * 64, "size": 675,
         "platform": {"os": "linux", "architecture": arch}}
        for digit, arch in [("1", "amd64"), ("2", "arm64")]
    ]
    attestations = [
        {"mediaType": MEDIA, "digest": "sha256:" + digit * 64, "size": 837,
         "platform": {"os": "unknown", "architecture": "unknown"},
         "annotations": {"vnd.docker.reference.type": "attestation-manifest",
                         "vnd.docker.reference.digest": image["digest"]}}
        for digit, image in zip(["3", "4"], images)
    ]
    return {"schemaVersion": 2, "mediaType": "application/vnd.oci.image.index.v1+json",
            "manifests": images + attestations}


class OciIndexTests(unittest.TestCase):
    def inspect(self, payload, extra=""):
        with tempfile.TemporaryDirectory(prefix="nika-oci-index-test-") as directory:
            scratch = Path(directory)
            (scratch / "index.json").write_text(json.dumps(payload) + extra)
            fake = scratch / "docker"
            fake.write_text(f"#!{sys.executable}\n" + '''
import json, os, pathlib, sys
root = pathlib.Path(os.environ["OCI_TEST_ROOT"])
assert sys.argv[1:4] == ["buildx", "imagetools", "inspect"]
if sys.argv[-1] == "--raw":
    print((root / "index.json").read_text())
else:
    with (root / "labels-read").open("a") as log: log.write("read\\n")
    print(json.dumps({"org.opencontainers.image.revision": "a" * 40,
      "org.opencontainers.image.version": "9.9.9",
      "org.opencontainers.image.source": "https://github.com/supernovae-st/nika",
      "org.opencontainers.image.licenses": "AGPL-3.0-or-later"}))
''')
            fake.chmod(0o755)
            result = subprocess.run(
                ["bash", str(ROOT / "scripts/release/oci-coordinate-immutable.sh"),
                 "inspect", IMAGE, "9.9.9", DIGEST, SHA, "https://github.com/supernovae-st/nika"],
                env={**os.environ, "PATH": f"{scratch}:{os.environ['PATH']}",
                     "OCI_TEST_ROOT": str(scratch)},
                capture_output=True, text=True, timeout=10,
            )
            reads = (scratch / "labels-read").read_text() if (scratch / "labels-read").exists() else ""
            return result, reads

    def test_two_runnable_platforms_and_their_two_attestations(self):
        result, reads = self.inspect(index())
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), DIGEST)
        self.assertEqual(reads, "read\nread\n")

    def test_index_order_is_not_authority(self):
        payload = index()
        payload["manifests"].reverse()
        result, reads = self.inspect(payload)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(reads, "read\nread\n")

    def test_invalid_shapes_refuse_before_reading_labels(self):
        mutations = {
            "missing-platform": lambda m: m.pop(1),
            "missing-attestation": lambda m: m.pop(),
            "no-attestations": lambda m: m.__delitem__(slice(2, None)),
            "duplicate-platform": lambda m: m[1].update(platform=m[0]["platform"]),
            "extra-platform": lambda m: m.append(copy.deepcopy(m[0])),
            "unmarked-unknown": lambda m: m[2].pop("annotations"),
            "wrong-attestation-kind": lambda m: m[2]["annotations"].update({"vnd.docker.reference.type": "something-else"}),
            "runnable-attestation": lambda m: m[2].update(platform=m[0]["platform"]),
            "foreign-subject": lambda m: m[2]["annotations"].update({"vnd.docker.reference.digest": DIGEST}),
            "duplicate-subject": lambda m: m[3]["annotations"].update(m[2]["annotations"]),
            "duplicate-digest": lambda m: m[3].update(digest=m[2]["digest"]),
            "invalid-digest": lambda m: m[0].update(digest="sha256:not-a-digest"),
            "invalid-size": lambda m: m[0].update(size=-1),
            "unexpected-media": lambda m: m[0].update(mediaType="application/json"),
        }
        for name, mutate in mutations.items():
            with self.subTest(name=name):
                payload = index()
                mutate(payload["manifests"])
                result, reads = self.inspect(payload)
                self.assertEqual(result.returncode, 73, result.stderr)
                self.assertEqual(reads, "")

    def test_one_json_document_is_required(self):
        for payload, extra in [(None, ""), ({}, ""), (index(), "{}"), ({}, json.dumps(index()))]:
            with self.subTest(payload=payload, extra=extra):
                result, reads = self.inspect(payload, extra)
                self.assertEqual(result.returncode, 73, result.stderr)
                self.assertEqual(reads, "")


if __name__ == "__main__":
    unittest.main()
