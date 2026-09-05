#!/usr/bin/env python3
"""The release index distinguishes runnable images from bound BuildKit metadata."""

import copy
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
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


class OciPayloadTests(unittest.TestCase):
    def verify(self, payload=None, extra="", drift="", unavailable=""):
        with tempfile.TemporaryDirectory(prefix="nika-oci-payload-test-") as directory:
            scratch = Path(directory)
            (scratch / "index.json").write_text(json.dumps(index() if payload is None else payload) + extra)
            for arch in ["x64", "arm64"]:
                content = f"{arch} native binary\n".encode()
                with tarfile.open(scratch / f"nika-linux-{arch}-9.9.9.tar.gz", "w:gz") as archive:
                    member = tarfile.TarInfo("nika")
                    member.size = len(content)
                    archive.addfile(member, io.BytesIO(content))
            fake = scratch / "docker"
            fake.write_text(f"#!{sys.executable}\n" + '''
import json, os, pathlib, sys
root = pathlib.Path(os.environ["OCI_TEST_ROOT"])
args = sys.argv[1:]
with (root / "calls").open("a") as log: log.write(json.dumps(args) + "\\n")
image = "ghcr.io/supernovae-st/nika@sha256:"
if args[:3] == ["buildx", "imagetools", "inspect"]:
    assert args[3:] == [image + "f" * 64, "--raw"]
    print((root / "index.json").read_text())
elif args[0] in ["pull", "create"]:
    platform = args[args.index("--platform") + 1]
    ref = args[-1]
    expected = image + {"linux/amd64": "1", "linux/arm64": "2"}[platform] * 64
    # Model the classic Docker image store: one index reference cannot
    # resolve to two platform images. A containerd store masked this in QA.
    if ref != image + "f" * 64:
        assert ref == expected, (ref, expected)
    state_path = root / "pulled.json"
    state = json.loads(state_path.read_text()) if state_path.exists() else {}
    if args[0] == "pull":
        if platform == os.environ["OCI_UNAVAILABLE"]: sys.exit(11)
        if ref in state and state[ref] != platform:
            print("cannot overwrite digest " + ref.split("@", 1)[1], file=sys.stderr)
            sys.exit(1)
        state[ref] = platform
        state_path.write_text(json.dumps(state))
    else:
        assert state.get(ref) == platform, (ref, state)
        print({"linux/amd64": "a", "linux/arm64": "b"}[platform] * 12)
elif args[0] == "cp":
    arch = {"a" * 12: "x64", "b" * 12: "arm64"}[args[1].split(":")[0]]
    assert args[1].endswith(":/usr/local/bin/nika")
    content = f"{arch} native binary\\n"
    if arch == os.environ["OCI_DRIFT"]: content += "drift\\n"
    pathlib.Path(args[2]).write_text(content)
elif args[0] == "rm":
    assert args[1] in ["a" * 12, "b" * 12]
else:
    raise AssertionError(args)
''')
            fake.chmod(0o755)
            result = subprocess.run(
                ["bash", str(ROOT / "scripts/release/verify-oci-payload.sh"),
                 IMAGE, DIGEST, "9.9.9", str(scratch)],
                env={**os.environ, "PATH": f"{scratch}:{os.environ['PATH']}",
                     "OCI_TEST_ROOT": str(scratch), "OCI_DRIFT": drift,
                     "OCI_UNAVAILABLE": unavailable},
                capture_output=True, text=True, timeout=20,
            )
            calls = [json.loads(line) for line in (scratch / "calls").read_text().splitlines()]
            return result, calls

    def test_both_platforms_use_their_bound_child_on_classic_docker(self):
        for reverse in [False, True]:
            with self.subTest(reverse=reverse):
                payload = index()
                if reverse:
                    payload["manifests"].reverse()
                result, calls = self.verify(payload)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(calls[0], ["buildx", "imagetools", "inspect", IMAGE + "@" + DIGEST, "--raw"])
                for verb in ["pull", "create"]:
                    selected = [call for call in calls if call[0] == verb]
                    self.assertEqual([call[-1] for call in selected],
                                     [IMAGE + "@sha256:" + digit * 64 for digit in ["1", "2"]])
                    if verb == "create":
                        self.assertTrue(all("--pull=never" in call for call in selected))
                self.assertEqual([call[1] for call in calls if call[0] == "rm"], ["a" * 12, "b" * 12])

    def test_invalid_parent_refuses_before_any_daemon_mutation(self):
        duplicate = index()
        duplicate["manifests"][1] = copy.deepcopy(duplicate["manifests"][0])
        missing = index()
        missing["manifests"].pop()
        for payload, extra in [(duplicate, ""), (missing, ""), ({}, ""), (index(), "{}")]:
            with self.subTest(payload=payload, extra=extra):
                result, calls = self.verify(payload, extra)
                self.assertEqual(result.returncode, 73, result.stderr)
                self.assertEqual(len(calls), 1, calls)

    def test_each_binary_drift_refuses_and_cleans_owned_containers(self):
        for arch, containers in [("x64", ["a" * 12]), ("arm64", ["a" * 12, "b" * 12])]:
            with self.subTest(arch=arch):
                result, calls = self.verify(drift=arch)
                self.assertEqual(result.returncode, 73, result.stderr)
                self.assertIn("REFUSED binary drift", result.stderr)
                self.assertEqual([call[1] for call in calls if call[0] == "rm"], containers)

    def test_failed_child_pull_is_not_a_successful_payload_proof(self):
        result, calls = self.verify(unavailable="linux/arm64")
        self.assertEqual(result.returncode, 11, result.stderr)
        self.assertEqual([call[1] for call in calls if call[0] == "rm"], ["a" * 12])
        self.assertEqual(len([call for call in calls if call[0] == "create"]), 1)


if __name__ == "__main__":
    unittest.main()
