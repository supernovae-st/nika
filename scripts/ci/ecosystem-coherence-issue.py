#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#
# ecosystem-coherence-issue.py — publish the nightly coherence report to its
# durable board: ONE accepted-scope issue, supernovae-st/nika#841.
#
# The board outgrew the old publication logic (2026-09): "ecosystem
# coherence — nightly board" became "ecosystem coherence — Project
# lifecycle, SDK/editor coverage and release pins", an accepted tracking
# issue whose scope is WIDER than release pins. The old step searched the
# OLD exact title and CLOSED the board on a green pin report — a retitle
# forks the search into a second issue, and a green pin ladder erases
# lifecycle/SDK/editor scope that only a human may accept.
#
# Doctrine (operator lock 2026-09-19):
#   · identity is EXPLICIT — BOARD_REPO + BOARD_ISSUE (declared in
#     .github/workflows/ecosystem-coherence.yml), never a title search;
#     a human retitle must never fork the board
#   · FAIL/WARN → update the board's bot-owned marker comment in place;
#     a closed board is reopened with a warning; NEVER a second issue
#   · GREEN → pin health is REPORTED on the board; the accepted scope is
#     never closed by a machine — closing stays a human act
#   · the issue title, body and human comments are read-only here; the bot
#     owns exactly one marker comment — marker AND bot authorship both
#     required, so a human quoting the marker is never rewritten — and the
#     comment search paginates, so a long board cannot hide it
#   · FAIL CLOSED: a missing/empty/malformed report, an unreadable issue
#     state, or a failed GitHub read exits non-zero BEFORE any mutation —
#     silence that reads as success is the defect class this exists to kill

import datetime
import json
import os
import re
import subprocess
import sys
import tempfile

MARKER = "<!-- nika-coherence-bot:board v1 -->"
# The workflow publishes with secrets.GITHUB_TOKEN, whose author is always
# github-actions[bot]. Ownership = marker AND this author AND type Bot.
EXPECTED_BOT = "github-actions[bot]"
GREEN_VERDICT = "GREEN — every pin holds"  # the producer's exact line
FINDING = re.compile(r"^(FAIL|WARN)\s")
BODY_LIMIT = 60_000   # GitHub's ceiling is 65_536 — headroom for the wrapper
PAGE_SIZE = 100
MAX_PAGES = 20        # a board past 2,000 comments is itself a finding
GH_TIMEOUT = int(os.environ.get("BOARD_GH_TIMEOUT_SECS", "30"))


def die(msg):
    print(f"ecosystem-coherence-issue: {msg}", file=sys.stderr)
    sys.exit(1)


def gh(args):
    """One bounded gh call; any failure is fatal. A half-published board
    that reads as success is worse than a loud red step."""
    try:
        res = subprocess.run(["gh", *args], capture_output=True, text=True,
                             timeout=GH_TIMEOUT)
    except subprocess.TimeoutExpired:
        die(f"`gh {' '.join(args[:2])}` exceeded {GH_TIMEOUT}s — a hung read/write "
            "is a failed one (fail closed)")
    if res.returncode != 0:
        lines = (res.stderr or res.stdout).strip().splitlines()
        die(f"`gh {' '.join(args[:2])}` failed (rc {res.returncode}): "
            f"{lines[-1] if lines else 'no output'}")
    return res.stdout


def gh_json(args, what):
    try:
        return json.loads(gh(args))
    except json.JSONDecodeError:
        die(f"`gh {' '.join(args[:2])}` returned no usable JSON for {what} — refusing to guess")


def read_report(path):
    """Classify the bot's stdout. The board state must be PROVEN by the
    report: a FAIL/WARN line, or the producer's exact GREEN verdict.
    Anything else — empty, truncated mid-crash, near-GREEN junk — is not a
    board state."""
    try:
        with open(path, encoding="utf-8") as fh:
            text = fh.read()
    except OSError as exc:
        die(f"report unreadable: {path} ({exc.strerror or exc}) — nothing published, nothing closed")
    if not text.strip():
        die(f"report {path} is empty — an empty board proves nothing (fail closed)")
    if any(FINDING.match(line) for line in text.splitlines()):
        return "findings", text
    if any(line == GREEN_VERDICT for line in text.splitlines()):
        return "green", text
    die(f"report {path} carries neither a FAIL/WARN finding nor the exact GREEN "
        f"verdict '{GREEN_VERDICT}' — malformed (fail closed)")


def read_comments(repo, number):
    """Every comment on the board, paged — a marker beyond page 1 must still
    be found, or the next run posts a duplicate."""
    out = []
    for page in range(1, MAX_PAGES + 1):
        batch = gh_json(["api", f"repos/{repo}/issues/{number}/comments"
                                 f"?per_page={PAGE_SIZE}&page={page}"],
                        f"comments page {page} of {repo}#{number}")
        if not isinstance(batch, list):
            die(f"comments page {page} for {repo}#{number} is not a list — refusing to guess")
        out.extend(batch)
        if len(batch) < PAGE_SIZE:
            return out
    die(f"{repo}#{number} carries more than {MAX_PAGES * PAGE_SIZE} comments — "
        "refusing to guess which marker comment is the live one")


def owned(comment):
    """The bot's own board comment: the marker AND the expected bot author.
    A human quoting the marker in prose keeps their comment untouched."""
    user = comment.get("user") if isinstance(comment, dict) else None
    return (isinstance(comment, dict)
            and isinstance(comment.get("id"), int)
            and MARKER in str(comment.get("body", ""))
            and isinstance(user, dict)
            and user.get("login") == EXPECTED_BOT
            and user.get("type") == "Bot")


def board_body(verdict, report, run_id):
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%d %H:%M UTC")
    if verdict == "findings":
        lead = ("**FAIL/WARN findings are live.** Pin drift is one input to this issue's "
                "accepted scope — project lifecycle, SDK/editor coverage and release pins. "
                "Lifecycle and SDK/editor acceptance stays with its human reviewers.")
    else:
        lead = ("**Pins GREEN — release-pin health only.** The nightly pin board reporting; "
                "the accepted lifecycle/SDK/editor scope of this issue is unaffected "
                "and stays open.")
    tail = report.rstrip()
    if len(tail) > BODY_LIMIT:
        tail = (f"… truncated to the last {BODY_LIMIT} chars — the full log is run {run_id} …\n"
                + tail[-BODY_LIMIT:])
    return f"{MARKER}\n_Coherence board · run {run_id} · {stamp}_\n\n{lead}\n\n```text\n{tail}\n```\n"


def upsert_comment(repo, number, mine, body):
    """The bot owns exactly one marker comment: updated in place when it
    exists (idempotent), created once when it does not. Human comments are
    never touched."""
    fd, path = tempfile.mkstemp(suffix=".board", text=True)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as fh:
            fh.write(json.dumps({"body": body}) if mine else body)
        if mine:
            target = mine[-1]["id"]
            gh(["api", "-X", "PATCH", f"repos/{repo}/issues/comments/{target}", "--input", path])
            return f"board comment {target} updated in place"
        gh(["issue", "comment", number, "--repo", repo, "--body-file", path])
        return "board comment opened"
    finally:
        os.unlink(path)


def main(argv):
    if len(argv) != 2:
        die("usage: ecosystem-coherence-issue.py <bot-report-file>")
    repo = os.environ.get("BOARD_REPO", "supernovae-st/nika")
    number = os.environ.get("BOARD_ISSUE", "841")
    run_id = os.environ.get("GITHUB_RUN_ID", "local")
    verdict, report = read_report(argv[1])

    # Every read happens BEFORE any mutation, and the issue must answer in a
    # shape the doctrine understands: an object, the number asked for, a
    # state that is exactly OPEN or CLOSED. Anything less refuses the run —
    # an unreadable state must never read as "closed, reopen it".
    issue = gh_json(["issue", "view", number, "--repo", repo, "--json", "number,title,state"],
                    f"issue {repo}#{number}")
    if not isinstance(issue, dict):
        die(f"the issue read for {repo}#{number} is not an object — refusing to guess")
    if str(issue.get("number")) != str(number):
        die(f"gh answered for issue {issue.get('number')}, asked {repo}#{number} — refusing to touch it")
    state = str(issue.get("state", "")).upper()
    if state not in ("OPEN", "CLOSED"):
        die(f"{repo}#{number} state is {issue.get('state')!r}, not OPEN/CLOSED — "
            "refusing to touch the board")
    title = issue.get("title") if isinstance(issue.get("title"), str) else ""
    open_issue = state == "OPEN"

    mine = [c for c in read_comments(repo, number) if owned(c)]

    if verdict == "green" and not open_issue:
        print(f"{repo}#{number} is closed and pins are GREEN — "
              "accepted scope stays as the human left it, nothing reported")
        return 0

    if verdict == "findings" and not open_issue:
        print(f"WARNING: {repo}#{number} is {state} while FAIL/WARN findings are "
              "live — reopening the accepted-scope board (only a human closes it)",
              file=sys.stderr)
        gh(["issue", "reopen", number, "--repo", repo])

    action = upsert_comment(repo, number, mine, board_body(verdict, report, run_id))
    if verdict == "findings":
        print(f"{action}: {repo}#{number} '{title}' — findings live (run {run_id})"
              + ("" if open_issue else " · board reopened"))
    else:
        print(f"{action}: {repo}#{number} '{title}' — pins GREEN, accepted scope untouched "
              f"(run {run_id})")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv))
    except SystemExit:
        raise
    except Exception as exc:  # noqa: BLE001 — a crash must not read as success
        print(f"ecosystem-coherence-issue: crashed: {exc.__class__.__name__}: {exc}", file=sys.stderr)
        sys.exit(1)
