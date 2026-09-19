#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#
# test-ecosystem-coherence-issue.py — drive the REAL board-publisher CLI
# (scripts/ci/ecosystem-coherence-issue.py) against an isolated fake `gh`
# executable on PATH. Zero network, zero real mutation: the fake keeps an
# issue/comment state JSON per case and logs every invocation, so each test
# asserts both the CLI's exit contract and the exact gh operations it did
# (and did NOT) perform.
#
# The decision table pins the 2026-09 doctrine and its review:
#   T1  findings land on the retitled board by IDENTITY — a human retitle
#       never forks a second issue
#   T2  a closed board + live findings → WARNING + reopen + update, still
#       never a create
#   T3  GREEN reports pin health and NEVER closes the accepted scope
#   T4  a missing report fails closed before ANY gh call
#   T5  empty/malformed reports fail closed before ANY gh call — including
#       near-GREEN text that is not the producer's exact verdict
#   T6  a failed GitHub read (issue or comments) fails closed with no
#       mutation after it
#   T7  human body/title/comments are preserved; the marker comment updates
#       in place (idempotent across runs)
#   T8  GREEN on a closed board is a quiet no-op — a machine never reopens
#       and never closes accepted human scope
#   T9  the workflow itself declares the stable identity and delegates to
#       the publisher (no title search, no create, no close)
#   T10 a human comment QUOTING the marker is never patched — ownership is
#       marker AND bot author
#   T11 the comment search paginates: a bot marker beyond page 1 is found
#       and updated, never duplicated
#   T12 an unreadable issue state fails closed — it must never read as
#       "closed, reopen it"
#   T13 a hung gh call is a failed gh call (bounded timeout)

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CLI = Path(__file__).resolve().parent / "ecosystem-coherence-issue.py"
MARKER = "<!-- nika-coherence-bot:board v1 -->"
BOT = {"login": "github-actions[bot]", "type": "Bot"}
HUMAN = {"login": "a-human", "type": "User"}
FINDINGS_REPORT = (
    "latest release: v0.120.2 · 50.0h old · tag-pin severity: FAIL\n"
    "FAIL  tap             formula 0.120.1 != latest release 0.120.2\n"
    "WARN  docs            status snapshot 0.120.1 != main workspace 0.120.2\n"
)
GREEN_REPORT = "latest release: v0.120.2 · 50.0h old · tag-pin severity: FAIL\nGREEN — every pin holds\n"
COMMENTS_PAGE = "repos/supernovae-st/nika/issues/841/comments?per_page=100&page="

# The fake gh: a stateful stand-in. argv is logged as one JSON line per call;
# reads/writes go to a state file; comments carry GitHub's author shape and
# are served paginated like the real API; the operations the doctrine
# forbids die loudly so a regression surfaces as a red CLI, not as a
# quietly green one.
FAKE_GH = r'''#!/usr/bin/env python3
import json
import os
import sys
import time

LOG = os.environ["FAKE_GH_LOG"]
STATE = os.environ["FAKE_GH_STATE"]
args = sys.argv[1:]
with open(LOG, "a", encoding="utf-8") as fh:
    fh.write(json.dumps(args) + "\n")

with open(STATE, encoding="utf-8") as fh:
    st = json.load(fh)

if st.get("sleep_secs"):
    time.sleep(st["sleep_secs"])


def die(msg, rc=1):
    print(f"gh: {msg}", file=sys.stderr)
    sys.exit(rc)


def store():
    with open(STATE, "w", encoding="utf-8") as fh:
        json.dump(st, fh)


def field(flag):
    return args[args.index(flag) + 1]


if args[:2] == ["issue", "view"]:
    if st.get("fail_issue_read"):
        die("simulated issue-read outage")
    issue = st["issue"]
    out = {"number": issue["number"], "title": issue["title"]}
    if "state" in issue:
        out["state"] = issue["state"]
    print(json.dumps(out))
elif args[:2] == ["issue", "reopen"]:
    st["issue"]["state"] = "open"
    store()
elif args[:2] == ["issue", "comment"]:
    with open(field("--body-file"), encoding="utf-8") as fh:
        body = fh.read()
    st["comments"].append({"id": st["next_id"], "body": body,
                           "user": {"login": "github-actions[bot]", "type": "Bot"}})
    st["next_id"] += 1
    store()
elif args[:2] in (["issue", "create"], ["issue", "close"], ["issue", "edit"]):
    die(f"forbidden op: {' '.join(args)}", 2)
elif args[0] == "api":
    path = next((a for a in args if a.startswith("repos/")), "")
    if "/issues/comments/" in path and "-X" in args:
        cid = int(path.rsplit("/", 1)[1])
        with open(field("--input"), encoding="utf-8") as fh:
            payload = json.load(fh)
        for c in st["comments"]:
            if c["id"] == cid:
                c["body"] = payload["body"]
                break
        else:
            die(f"no comment {cid}")
        store()
        print("{}")
    elif path.startswith("repos/") and "/comments?" in path:
        if st.get("fail_comments_read"):
            die("simulated comments-read outage")
        query = path.split("?", 1)[1]
        params = dict(p.split("=", 1) for p in query.split("&"))
        per_page = int(params.get("per_page", "30"))
        page = int(params.get("page", "1"))
        print(json.dumps(st["comments"][(page - 1) * per_page:page * per_page]))
    else:
        die(f"unhandled api call: {' '.join(args)}", 2)
else:
    die(f"unhandled: {' '.join(args)}", 2)
'''


def ops(calls):
    """Compact the logged argv into comparable operation strings."""
    out = []
    for c in calls:
        if c[0] == "api":
            method = c[c.index("-X") + 1] if "-X" in c else "GET"
            path = next(a for a in c if a.startswith("repos/"))
            out.append(f"api {method} {path}")
        else:
            out.append(" ".join(c[:2]))
    return out


class BoardPublisher(unittest.TestCase):
    maxDiff = None
    DEFAULT_TITLE = ("ecosystem coherence — Project lifecycle, "
                     "SDK/editor coverage and release pins")

    def run_cli(self, *, report=MARKER, issue=None, comments=None,
                fail_issue_read=False, fail_comments_read=False, extra_env=None,
                sleep_secs=0):
        """Run the CLI once. report=MARKER (default) uses FINDINGS_REPORT;
        report=None points the CLI at a file that does not exist."""
        with tempfile.TemporaryDirectory(prefix="nika-coherence-issue-") as tmp:
            root = Path(tmp)
            fake = root / "gh"
            fake.write_text(FAKE_GH)
            fake.chmod(0o755)
            state = {
                "issue": issue or {"number": 841, "title": self.DEFAULT_TITLE,
                                   "state": "open"},
                "comments": list(comments or []),
                "next_id": 100,
                "fail_issue_read": fail_issue_read,
                "fail_comments_read": fail_comments_read,
                "sleep_secs": sleep_secs,
            }
            (root / "state.json").write_text(json.dumps(state))
            argv = [sys.executable, str(CLI)]
            if report is None:
                argv.append(str(root / "missing.out"))
            else:
                (root / "bot.out").write_text(FINDINGS_REPORT if report is MARKER else report)
                argv.append(str(root / "bot.out"))
            env = dict(os.environ)
            env.update(PATH=f"{root}{os.pathsep}{env.get('PATH', '')}",
                       FAKE_GH_LOG=str(root / "gh.log"),
                       FAKE_GH_STATE=str(root / "state.json"),
                       BOARD_REPO="supernovae-st/nika",
                       BOARD_ISSUE="841",
                       GITHUB_RUN_ID="test-run",
                       GH_TOKEN="fake")
            env.update(extra_env or {})
            result = subprocess.run(argv, env=env, capture_output=True, text=True,
                                    timeout=60, check=False)
            log = root / "gh.log"
            calls = ([json.loads(line) for line in log.read_text().splitlines()]
                     if log.exists() else [])
            final = json.loads((root / "state.json").read_text())
            return result, calls, final

    def assert_never_forbidden(self, calls):
        for op in ops(calls):
            self.assertNotIn(op.split()[0:2], [["issue", "create"], ["issue", "close"],
                                               ["issue", "edit"]])

    # T1 · findings land on the retitled board by identity — a human retitle
    # must not fork a second issue or rename the board back.
    def test_findings_update_the_retitled_board_without_a_new_issue(self):
        retitled = {"number": 841, "title": "ecosystem coherence — widened by a human",
                    "state": "open"}
        result, calls, final = self.run_cli(issue=retitled)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(ops(calls), [
            "issue view",
            f"api GET {COMMENTS_PAGE}1",
            "issue comment",
        ])
        self.assert_never_forbidden(calls)
        self.assertEqual(final["issue"]["title"], retitled["title"])
        self.assertEqual(len(final["comments"]), 1)
        body = final["comments"][0]["body"]
        self.assertIn(MARKER, body)
        self.assertIn("FAIL  tap", body)

    # T2 · a closed board + live findings: warn, reopen, update — no create.
    def test_closed_board_warns_and_reopens_on_findings(self):
        closed = {"number": 841, "title": self.DEFAULT_TITLE, "state": "closed"}
        result, calls, final = self.run_cli(issue=closed)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("WARNING", result.stderr)
        self.assertIn("CLOSED", result.stderr)
        self.assertEqual(ops(calls), [
            "issue view",
            f"api GET {COMMENTS_PAGE}1",
            "issue reopen",
            "issue comment",
        ])
        self.assert_never_forbidden(calls)
        self.assertEqual(final["issue"]["state"], "open")
        self.assertEqual(len(final["comments"]), 1)

    # T3 · GREEN reports pin health on the open board and NEVER closes it.
    def test_green_reports_but_never_closes(self):
        result, calls, final = self.run_cli(report=GREEN_REPORT)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("pins GREEN", result.stdout)
        self.assertEqual(final["issue"]["state"], "open")
        self.assertNotIn("issue close", ops(calls))
        self.assert_never_forbidden(calls)
        self.assertEqual(len(final["comments"]), 1)
        body = final["comments"][0]["body"]
        self.assertIn(MARKER, body)
        self.assertIn("GREEN — every pin holds", body)
        self.assertIn("stays open", body)

    # T4 · a missing report fails closed before ANY gh call.
    def test_missing_report_fails_closed(self):
        result, calls, _ = self.run_cli(report=None)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(calls, [])
        self.assertIn("unreadable", result.stderr)

    # T5 · empty and malformed reports fail closed before ANY gh call —
    # including near-GREEN text that is not the producer's exact verdict.
    def test_empty_and_malformed_reports_fail_closed(self):
        for raw in ["", "   \n", "the bot crashed before a verdict\n",
                    "GREEN\n", "green — every pin holds\n",
                    "GREEN — every pin hold\n",
                    "GREEN — every pin holds extra words\n",
                    "GREEN — pins fine\n"]:
            with self.subTest(raw=raw):
                result, calls, _ = self.run_cli(report=raw)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(calls, [])

    # T6 · failed GitHub reads fail closed with no mutation after them.
    def test_failed_issue_read_fails_closed(self):
        result, calls, _ = self.run_cli(fail_issue_read=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(ops(calls), ["issue view"])
        self.assertIn("failed", result.stderr)

    def test_failed_comments_read_fails_closed(self):
        result, calls, _ = self.run_cli(fail_comments_read=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(ops(calls), ["issue view", f"api GET {COMMENTS_PAGE}1"])
        self.assertIn("failed", result.stderr)

    # T7 · human body/title/comments are preserved; the marker comment is
    # updated in place, so a second run adds nothing.
    def test_human_content_preserved_and_updates_idempotent(self):
        issue = {"number": 841, "title": self.DEFAULT_TITLE, "state": "open",
                 "body": "HUMAN: accepted lifecycle/SDK/editor criteria live here"}
        humans = [{"id": 1, "body": "human triage note — do not touch", "user": HUMAN}]
        first, _, state = self.run_cli(issue=issue, comments=humans)
        self.assertEqual(first.returncode, 0, first.stderr)
        self.assertEqual(state["issue"]["body"], issue["body"])
        self.assertEqual(state["issue"]["title"], issue["title"])
        self.assertEqual(state["comments"][0], humans[0])
        self.assertEqual(len(state["comments"]), 2)
        marker_id = state["comments"][1]["id"]

        # A second run (the state persists via the same fixtures) must PATCH
        # the marker comment, not stack another one.
        second, calls2, state2 = self.run_cli(issue=issue, comments=state["comments"])
        self.assertEqual(second.returncode, 0, second.stderr)
        self.assertEqual(ops(calls2), [
            "issue view",
            f"api GET {COMMENTS_PAGE}1",
            f"api PATCH repos/supernovae-st/nika/issues/comments/{marker_id}",
        ])
        self.assert_never_forbidden(calls2)
        self.assertEqual(len(state2["comments"]), 2)
        self.assertEqual(state2["comments"][0], humans[0])
        self.assertEqual(state2["issue"]["body"], issue["body"])

    # T8 · GREEN on a closed board is a quiet no-op: no reopen, no close,
    # no comment — accepted human scope is left exactly as it was left.
    def test_green_on_a_closed_board_is_a_quiet_noop(self):
        closed = {"number": 841, "title": self.DEFAULT_TITLE, "state": "closed"}
        humans = [{"id": 1, "body": "human triage note — do not touch", "user": HUMAN}]
        result, calls, final = self.run_cli(report=GREEN_REPORT, issue=closed,
                                            comments=humans)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(ops(calls), ["issue view", f"api GET {COMMENTS_PAGE}1"])
        self.assert_never_forbidden(calls)
        self.assertEqual(final["issue"]["state"], "closed")
        self.assertEqual(final["comments"], humans)

    # T9 · the workflow declares the stable identity and delegates every
    # side effect to the publisher — no title search, no create, no close.
    def test_workflow_declares_identity_and_delegates(self):
        workflow = (ROOT / ".github/workflows/ecosystem-coherence.yml").read_text()
        self.assertIn("BOARD_REPO: supernovae-st/nika", workflow)
        self.assertIn("BOARD_ISSUE: '841'", workflow)
        self.assertIn("scripts/ci/ecosystem-coherence-issue.py bot.out", workflow)
        self.assertIn("scripts/ci/test-ecosystem-coherence-issue.py", workflow)
        self.assertNotIn("in:title", workflow)
        self.assertNotIn("gh issue create", workflow)
        self.assertNotIn("gh issue close", workflow)

    # T10 · a human comment QUOTING the marker is never patched — ownership
    # is marker AND bot author, so the bot opens its own comment instead.
    def test_human_marker_collision_is_never_patched(self):
        quoted = {"id": 7, "body": f"why not drop {MARKER} into the update?",
                  "user": HUMAN}
        result, calls, final = self.run_cli(comments=[quoted])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(ops(calls), [
            "issue view",
            f"api GET {COMMENTS_PAGE}1",
            "issue comment",
        ])
        self.assert_never_forbidden(calls)
        self.assertEqual(len(final["comments"]), 2)
        self.assertEqual(final["comments"][0], quoted)  # untouched, verbatim
        self.assertIn(MARKER, final["comments"][1]["body"])

    # T11 · the comment search paginates: a bot marker beyond page 1 is
    # found and updated in place — never duplicated by a blind first page.
    def test_marker_on_the_second_page_is_updated_not_duplicated(self):
        humans = [{"id": i, "body": f"human note {i}", "user": HUMAN} for i in range(1, 106)]
        marker = {"id": 500, "body": f"{MARKER}\nold board state", "user": BOT}
        result, calls, final = self.run_cli(comments=humans + [marker])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(ops(calls), [
            "issue view",
            f"api GET {COMMENTS_PAGE}1",
            f"api GET {COMMENTS_PAGE}2",
            "api PATCH repos/supernovae-st/nika/issues/comments/500",
        ])
        self.assert_never_forbidden(calls)
        self.assertEqual(len(final["comments"]), 106)
        self.assertIn("FAIL  tap", final["comments"][-1]["body"])

    # T12 · an unreadable issue state fails closed — never "closed, reopen".
    def test_unreadable_issue_state_fails_closed(self):
        for weird in [{"state": "pending"}, {}]:
            with self.subTest(weird=weird):
                issue = {"number": 841, "title": self.DEFAULT_TITLE, **weird}
                result, calls, final = self.run_cli(issue=issue)
                self.assertNotEqual(result.returncode, 0)
                self.assertEqual(ops(calls), ["issue view"])
                self.assertIn("not OPEN/CLOSED", result.stderr)
                self.assertNotEqual(final["issue"].get("state"), "open")

    # T13 · a hung gh call is a failed gh call: the timeout fires, the run
    # dies red, and nothing after the hang executes.
    def test_hung_gh_call_fails_closed(self):
        result, calls, _ = self.run_cli(sleep_secs=5,
                                        extra_env={"BOARD_GH_TIMEOUT_SECS": "1"})
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(ops(calls), ["issue view"])
        self.assertIn("exceeded 1s", result.stderr)


if __name__ == "__main__":
    unittest.main()
