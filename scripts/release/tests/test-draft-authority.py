#!/usr/bin/env python3
"""Draft reads need push access; verifiers must not gain that authority.

These are workflow-routing regressions, not live GitHub permission proofs.
The original 0.118.2 run supplies the measured 403 counterexample.
"""

from pathlib import Path
import os
import re
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[3]
WORKFLOW = ROOT / ".github/workflows/release.yml"


def job(name):
    match = re.search(
        rf"^  {re.escape(name)}:\n(.*?)(?=^  [\w-]+:\n|\Z)",
        WORKFLOW.read_text(),
        re.MULTILINE | re.DOTALL,
    )
    if match is None:
        raise AssertionError(f"missing job {name}")
    return match.group(1)


class DraftAuthority(unittest.TestCase):
    def assert_read_only_without_draft_access(self, name):
        source = job(name)
        self.assertRegex(source, r"(?m)^      contents: read(?:\s|$)")
        self.assertNotRegex(source, r"(?m)^      contents: write")
        for draft_read in (
            "read-release-state.sh",
            "release-digest-marker.sh",
            "read-release-provenance.sh",
            "gh release download",
            'gh api "repos/${REPO}/releases/',
        ):
            self.assertNotIn(draft_read, source, f"{name} requires draft access")
        return source

    def test_push_provenance_reads_public_tag_not_private_draft(self):
        source = self.assert_read_only_without_draft_access("provenance-publish")
        self.assertIn("verify-slsa-provenance.sh", source)
        self.assertIn("resolve-release-tag.sh", source)

    def test_replay_provenance_is_fetched_by_the_existing_draft_owner(self):
        source = self.assert_read_only_without_draft_access("provenance-replay-check")
        self.assertIn("name: release-provenance-source", source)
        self.assertIn("verify-slsa-provenance.sh", source)
        owner = job("release-draft")
        self.assertIn("read-release-provenance.sh", owner)
        self.assertIn("name: release-provenance-source", owner)

    def test_docker_receives_marker_decision_without_draft_authority(self):
        source = self.assert_read_only_without_draft_access("docker")
        self.assertIn("needs.release-draft.outputs.oci-build", source)
        self.assertIn("needs.release-draft.outputs.oci-digest", source)
        owner = job("release-draft")
        self.assertIn("release-digest-marker.sh", owner)
        self.assertIn('[ "$state" -eq 44 ] || exit "$state"', owner)

    def test_final_registry_proof_leaves_fresh_draft_check_to_finalizer(self):
        source = self.assert_read_only_without_draft_access("release-final-proof")
        self.assertIn("needs.oci-version.outputs.digest", source)
        self.assertIn("verify-oci-payload.sh", source)
        finalizer = (ROOT / "scripts/release/finalize-release.sh").read_text()
        self.assertIn("release-digest-marker.sh", finalizer)
        self.assertIn('[ "$persisted" = "$digest" ]', finalizer)
        self.assertIn("REFUSED persisted digest drift", finalizer)


class ReplayInput(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="release-input-")
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name)
        self.output = self.root / "multiple.intoto.jsonl"
        bin_dir = self.root / "bin"
        bin_dir.mkdir()
        git = bin_dir / "git"
        git.write_text(
            "#!/bin/sh\n[ \"$1\" = ls-remote ] || exit 90\n"
            "printf '%s\\trefs/tags/v9.9.9\\n' 2222222222222222222222222222222222222222\n"
        )
        gh = bin_dir / "gh"
        gh.write_text('''#!/usr/bin/env python3
from pathlib import Path
import os, sys
root = Path(os.environ["FIXTURE_ROOT"])
case = os.environ["FIXTURE_CASE"]
args = sys.argv[1:]
with (root / "calls").open("a") as log:
    log.write(" ".join(args) + "\\n")
if not args or args[0] != "api" or "--method" in args:
    sys.exit(90)
endpoint = args[1]
if case == "forbidden":
    print("Resource not accessible by integration (HTTP 403)", file=sys.stderr)
    sys.exit(1)
if endpoint == "repos/example/nika/releases/123":
    calls = (root / "calls").read_text().count(endpoint + " --jq")
    tag = "v8.8.8" if case == "state-drift" and calls > 1 else "v9.9.9"
    print("123\\t" + tag + "\\ttrue\\tfalse")
elif endpoint == "repos/example/nika/releases/123/assets":
    if "--paginate" not in args:
        sys.exit(90)
    calls = (root / "calls").read_text().count(endpoint + " --paginate")
    ids = {"absent": "", "duplicate": "456\\n789", "invalid": "invalid"}
    print("789" if case == "asset-drift" and calls > 1 else ids.get(case, "456"))
elif endpoint == "repos/example/nika/releases/assets/456":
    if "Accept: application/octet-stream" not in args:
        sys.exit(90)
    if case != "empty":
        print("unverified-source-statement")
    if case == "partial":
        sys.exit(1)
else:
    sys.exit(90)
''')
        git.chmod(0o755)
        gh.chmod(0o755)
        self.env = {
            "PATH": str(bin_dir) + os.pathsep + os.environ["PATH"],
            "HOME": str(self.root),
            "TMPDIR": str(self.root),
            "GH_TOKEN": "synthetic-fixture",
            "FIXTURE_ROOT": str(self.root),
        }

    def invoke(self, case="ok"):
        return subprocess.run(
            ["bash", str(ROOT / "scripts/release/read-release-provenance.sh"),
             "example/nika", "123", "v9.9.9", "2" * 40, str(self.output)],
            env={**self.env, "FIXTURE_CASE": case},
            capture_output=True, text=True, timeout=10, check=False,
        )

    def test_download_is_by_asset_id_and_is_not_a_verification_claim(self):
        result = self.invoke()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.output.read_text(), "unverified-source-statement\n")
        calls = (self.root / "calls").read_text()
        self.assertIn("releases/assets/456", calls)
        self.assertNotIn("releases/tags/", calls)
        self.assertEqual(result.stdout, "")

    def test_refusals_leave_no_output_or_partial_scratch(self):
        for case in ("forbidden", "absent", "duplicate", "invalid", "empty",
                     "partial", "state-drift", "asset-drift"):
            with self.subTest(case=case):
                (self.root / "calls").write_text("")
                result = self.invoke(case)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(self.output.exists())
                self.assertEqual(sorted(p.name for p in self.root.iterdir()), ["bin", "calls"])

    def test_existing_output_is_never_overwritten(self):
        self.output.write_text("existing-evidence")
        result = self.invoke()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.output.read_text(), "existing-evidence")
        self.assertFalse((self.root / "calls").exists())


if __name__ == "__main__":
    unittest.main()
