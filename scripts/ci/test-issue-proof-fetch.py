#!/usr/bin/env python3
"""Exercise the actual issue-proof workflow shell against an isolated fake gh."""
import json
import os
import subprocess
import tempfile
import textwrap
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = (ROOT / ".github/workflows/issue-proof.yml").read_text()
MARKER = "\n        run: |\n"
# This workflow has one final literal shell block. Reject a changed layout
# rather than silently testing a different script; no YAML package is needed
# by the standalone issue-proof workflow's self-test.
if WORKFLOW.count(MARKER) != 1:
    raise RuntimeError("expected one final workflow shell block")
RUN = textwrap.dedent(WORKFLOW.split(MARKER)[1])


class IssueProofFetch(unittest.TestCase):
    def check_case(self, *, snapshot=None, raw=None, event="workflow_dispatch",
                   body="", fetch_rc="0", number="999999", expected=0, calls=None):
        with tempfile.TemporaryDirectory(prefix="nika-issue-proof-fetch-") as tmp:
            root = Path(tmp)
            fake = root / "gh"
            fake.write_text('''#!/usr/bin/env bash
printf '%s %s\\n' "$1" "$2" >> "$PROOF_CALLS"
if [ "$1 $2" = "issue view" ]; then
  cat "$PROOF_SNAPSHOT"
  exit "$PROOF_FETCH_RC"
fi
exit 0
''')
            fake.chmod(0o755)
            snapshot_file = root / "snapshot.json"
            snapshot_file.write_text(raw if raw is not None else json.dumps(snapshot))
            env = dict(os.environ)
            env.update(PATH=f"{root}{os.pathsep}{env.get('PATH', '')}",
                       PROOF_CALLS=str(root / "calls"), PROOF_SNAPSHOT=str(snapshot_file),
                       PROOF_INJECTED=str(root / "injected"),
                       PROOF_FETCH_RC=fetch_rc, NIKA_SKIP_ISSUE_PROOF_SELFTEST="1",
                       ISSUE_EVENT=event, ISSUE_NUM=number, ISSUE_BODY=body,
                       ISSUE_LABELS="", ISSUE_STATE_REASON="completed",
                       GH_REPO="example/fixture")
            result = subprocess.run(["bash", "-c", RUN], cwd=ROOT, env=env,
                                    capture_output=True, text=True, timeout=15, check=False)
            self.assertEqual(result.returncode, expected, result.stdout + result.stderr)
            observed = (root / "calls").read_text().splitlines() if (root / "calls").exists() else []
            self.assertEqual(observed, calls or [], result.stdout + result.stderr)
            self.assertFalse((root / "injected").exists())
            return result.stdout

    @staticmethod
    def snapshot(**changes):
        value = {"number": 999999, "body": "proven_by: rust", "state": "CLOSED",
                 "stateReason": "COMPLETED", "labels": []}
        value.update(changes)
        return value

    def test_manual_dispatch_reads_the_real_body(self):
        body = 'untrusted "quotes" and `commands` and $(touch "$PROOF_INJECTED")\nproven_by: rust'
        out = self.check_case(snapshot=self.snapshot(body=body), calls=["issue view"])
        self.assertIn("HOLD", out)

    def test_manual_dispatch_preserves_the_closer_waivers(self):
        for change in [{"stateReason": "NOT_PLANNED"}, {"labels": [{"name": "question"}]}]:
            with self.subTest(change=change):
                out = self.check_case(snapshot=self.snapshot(body="", **change), calls=["issue view"])
                self.assertIn("WAIVED", out)

    def test_unreadable_or_wrong_snapshot_never_mutates(self):
        for snapshot in [self.snapshot(number=1), self.snapshot(state="OPEN"),
                         self.snapshot(labels=None), self.snapshot(body={})]:
            with self.subTest(snapshot=snapshot):
                self.check_case(snapshot=snapshot, expected=2, calls=["issue view"])
        self.check_case(raw="not json", expected=2, calls=["issue view"])
        self.check_case(snapshot=self.snapshot(), fetch_rc="1", expected=2, calls=["issue view"])

    def test_a_real_missing_proof_still_reopens(self):
        self.check_case(snapshot=self.snapshot(body="no proof here"), expected=1,
                        calls=["issue view", "issue reopen", "issue comment"])

    def test_closed_events_use_the_event_snapshot(self):
        self.check_case(event="issues", body="proven_by: rust")

    def test_invalid_number_never_reaches_github(self):
        self.check_case(number="../pulls/1", expected=2)


if __name__ == "__main__":
    unittest.main()
