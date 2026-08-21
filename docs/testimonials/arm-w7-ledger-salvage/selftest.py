#!/usr/bin/env python3
"""Negative fixtures for the W7 mutation testimonial validator."""

import json
import pathlib
import runpy
import tempfile
from typing import Any

validator = runpy.run_path(pathlib.Path(__file__).with_name("validate.py"))
EvidenceError = validator["EvidenceError"]
RUN_ID = validator["RUN_ID"]
RUN_TARGET = validator["RUN_TARGET"]
TESTED_COMMIT = validator["TESTED_COMMIT"]
TESTED_TREE = validator["TESTED_TREE"]

failures = 0


def expect_red(action: Any) -> None:
    global failures
    try:
        action()
    except (EvidenceError, KeyError):
        failures += 1
        return
    raise EvidenceError("negative self-test unexpectedly passed")


start = {
    "equivalent_exclusion": {"name": "m:eq", "same_span_controls": []},
    "prior_anomalies": [{"current_name": "m:caught"}],
    "guard_mutants": [],
}
census = ["m:eq", "m:caught", "m:unviable"]
mutants = [{"name": "m:caught"}, {"name": "m:unviable"}]
outcomes = {
    "total_mutants": 2,
    "caught": 1,
    "unviable": 1,
    "missed": 0,
    "timeout": 0,
    "outcomes": [
        {"scenario": {"Mutant": {"name": "m:caught"}}, "summary": "CaughtMutant"},
        {"scenario": {"Mutant": {"name": "m:unviable"}}, "summary": "Unviable"},
    ],
}
summary = validator["accounting"](start, census, mutants, outcomes)
validator["require"](summary["viable_score_percent"] == 100.0, "green fixture failed")
broken = json.loads(json.dumps(outcomes))
broken["outcomes"][0]["summary"] = "Timeout"
expect_red(lambda: validator["accounting"](start, census, mutants[:-1], outcomes))
expect_red(lambda: validator["accounting"](start, census, mutants, broken))

with tempfile.TemporaryDirectory() as tmp:
    root = pathlib.Path(tmp)
    leaked = root / "leak.txt"
    privacy_cases = [
        "/" + "Users" + "/someone/source",
        "/" + "home" + "/someone/source",
        "C:\\Users\\someone\\source",
        "s" + "k-" + "a" * 20,
    ]
    for content in privacy_cases:
        leaked.write_text(content)
        expect_red(lambda: validator["verify_privacy"]([leaked]))
    expect_red(lambda: validator["artifact_record"](root, root / "missing.json"))
    target = root / "outcomes.json"
    target.write_text("{}")
    link = root / "mutants.json"
    link.symlink_to(target.name)
    expect_red(lambda: validator["artifact_record"](root, link))

expect_red(lambda: validator["require_run_after_receipt"]("2026-08-21T04:00:00Z", "2026-08-21T03:59:59Z"))
expect_red(lambda: validator["require_run_identity"](RUN_ID, "/tmp/wrong", RUN_TARGET))
expect_red(lambda: validator["require_detached_head"](0, "refs/heads/not-detached"))
expect_red(lambda: validator["require_monitoring"]({"interval_ms": 1000, "probe_count": 2, "violation": {"clean": False}}))
expect_red(lambda: validator["require_receipt_identity"]({"schema": "nika.mutation-testimonial.run.raw.v1", "receipt_commit": "wrong", "tested_commit": TESTED_COMMIT, "tested_tree": TESTED_TREE, "run_id": RUN_ID}, "expected"))
expect_red(lambda: validator["require_receipt_identity"]({"schema": "synthetic-without-process", "receipt_commit": "expected", "tested_commit": TESTED_COMMIT, "tested_tree": TESTED_TREE, "run_id": RUN_ID}, "expected"))
expect_red(lambda: validator["require_outcome_time_authority"]({"start_time": "2026-08-21T04:00:01Z", "end_time": "2026-08-21T04:00:02Z"}, {"start_time": "2026-08-21T04:00:00Z", "end_time": "2026-08-21T04:00:02Z"}, "2026-08-21T03:59:59Z"))

validator["require"](failures == 15, "one of 15 negative fixtures survived")
print("OK: validator green fixtures passed; 15/15 negative mutations rejected")
