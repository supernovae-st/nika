#!/usr/bin/env python3
"""Capture and verify the ARM W7 focused mutation testimonial.

The start receipt is committed before the run.  The final receipt can therefore
prove that its tested tree, input blobs, census, invocation, and validator were
fixed before cargo-mutants produced any result.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
from typing import Any


LANE = pathlib.Path("docs/testimonials/arm-w7-ledger-salvage")
TESTED_COMMIT = "55df3d29150eb28508c945c85c7802bf4c6d851f"
TESTED_TREE = "bb7f629da770435208edd8e9c6a48cf24cad873b"
RUN_WORKTREE = "/tmp/nika-pr1079-mutants-55df"
RUN_OUTPUT = "/tmp/nika-pr1079-final-mutants-output"
RUN_TARGET = "/tmp/nika-pr1079-final-mutants-target"
EXCLUDED_NAME = (
    "crates/nika-cadence/src/ledger.rs:406:27: "
    "replace > with >= in unsettled"
)
EXCLUDED_RE = (
    r"^crates/nika-cadence/src/ledger\.rs:406:27: "
    r"replace > with >= in unsettled$"
)
RUN_ARGV = [
    "cargo",
    "mutants",
    "-p",
    "nika-cadence",
    "-f",
    "crates/nika-cadence/src/firing.rs",
    "-f",
    "crates/nika-cadence/src/ledger.rs",
    "-E",
    EXCLUDED_RE,
    "-o",
    RUN_OUTPUT,
    "-j",
    "1",
    "--baseline",
    "run",
    "--timeout",
    "300",
    "--build-timeout",
    "300",
    "--",
    "--lib",
]
LIST_ARGV = [
    "cargo",
    "mutants",
    "--list",
    "--json",
    "-p",
    "nika-cadence",
    "-f",
    "crates/nika-cadence/src/firing.rs",
    "-f",
    "crates/nika-cadence/src/ledger.rs",
]
SAME_SPAN_CONTROLS = [
    "crates/nika-cadence/src/ledger.rs:406:27: replace > with == in unsettled",
    "crates/nika-cadence/src/ledger.rs:406:27: replace > with < in unsettled",
]

# These twenty identities were anomalous in the discarded parallel map.  Their
# current exact identities are fixed before the serial run and must all finish
# as CaughtMutant before the testimonial can pass.
PRIOR_ANOMALIES = [
    ("MissedMutant", "crates/nika-cadence/src/ledger.rs:353:52: replace + with * in unsettled", "crates/nika-cadence/src/ledger.rs:353:52: replace + with * in unsettled"),
    ("MissedMutant", "crates/nika-cadence/src/ledger.rs:356:17: replace += with *= in unsettled", "crates/nika-cadence/src/ledger.rs:356:17: replace += with *= in unsettled"),
    ("MissedMutant", "crates/nika-cadence/src/ledger.rs:356:17: replace += with -= in unsettled", "crates/nika-cadence/src/ledger.rs:356:17: replace += with -= in unsettled"),
    ("MissedMutant", "crates/nika-cadence/src/ledger.rs:362:64: replace == with != in unsettled", "crates/nika-cadence/src/ledger.rs:362:64: replace == with != in unsettled"),
    ("MissedMutant", "crates/nika-cadence/src/ledger.rs:405:17: delete ! in unsettled", "crates/nika-cadence/src/ledger.rs:405:17: delete ! in unsettled"),
    ("Timeout", "crates/nika-cadence/src/ledger.rs:754:28: replace == with != in first_line_is_versioned", "crates/nika-cadence/src/ledger.rs:757:28: replace == with != in first_line_is_versioned"),
    ("Timeout", "crates/nika-cadence/src/ledger.rs:758:5: replace has_ledger_marker -> bool with false", "crates/nika-cadence/src/ledger.rs:761:5: replace has_ledger_marker -> bool with false"),
    ("Timeout", "crates/nika-cadence/src/ledger.rs:758:5: replace has_ledger_marker -> bool with true", "crates/nika-cadence/src/ledger.rs:761:5: replace has_ledger_marker -> bool with true"),
    ("Timeout", "crates/nika-cadence/src/ledger.rs:773:5: replace legacy_line_valid -> bool with false", "crates/nika-cadence/src/ledger.rs:776:5: replace legacy_line_valid -> bool with false"),
    ("Timeout", "crates/nika-cadence/src/ledger.rs:773:5: replace legacy_line_valid -> bool with true", "crates/nika-cadence/src/ledger.rs:776:5: replace legacy_line_valid -> bool with true"),
    ("Timeout", "crates/nika-cadence/src/ledger.rs:786:9: replace || with && in legacy_line_valid", "crates/nika-cadence/src/ledger.rs:789:9: replace || with && in legacy_line_valid"),
    ("Timeout", "crates/nika-cadence/src/ledger.rs:791:9: replace || with && in legacy_line_valid", "crates/nika-cadence/src/ledger.rs:794:9: replace || with && in legacy_line_valid"),
    ("Timeout", "crates/nika-cadence/src/ledger.rs:796:9: replace || with && in legacy_line_valid", "crates/nika-cadence/src/ledger.rs:799:9: replace || with && in legacy_line_valid"),
    ("Timeout", "crates/nika-cadence/src/ledger.rs:920:50: replace || with && in verify_payload", "crates/nika-cadence/src/ledger.rs:927:50: replace || with && in verify_payload"),
    ("Timeout", "crates/nika-cadence/src/ledger.rs:921:57: replace == with != in verify_payload", "crates/nika-cadence/src/ledger.rs:928:57: replace == with != in verify_payload"),
    ("Timeout", "crates/nika-cadence/src/ledger.rs:923:17: replace || with && in verify_payload", "crates/nika-cadence/src/ledger.rs:930:17: replace || with && in verify_payload"),
    ("Timeout", "crates/nika-cadence/src/ledger.rs:924:21: replace && with || in verify_payload", "crates/nika-cadence/src/ledger.rs:931:21: replace && with || in verify_payload"),
    ("Timeout", "crates/nika-cadence/src/ledger.rs:925:21: replace && with || in verify_payload", "crates/nika-cadence/src/ledger.rs:932:21: replace && with || in verify_payload"),
    ("Timeout", "crates/nika-cadence/src/ledger.rs:932:17: replace && with || in verify_payload", "crates/nika-cadence/src/ledger.rs:939:17: replace && with || in verify_payload"),
    ("Timeout", "crates/nika-cadence/src/ledger.rs:933:17: replace && with || in verify_payload", "crates/nika-cadence/src/ledger.rs:940:17: replace && with || in verify_payload"),
]

GUARD_MUTANTS = [
    "crates/nika-cadence/src/firing.rs:619:34: replace > with >= in decide",
    "crates/nika-cadence/src/ledger.rs:406:38: replace && with || in unsettled",
    "crates/nika-cadence/src/ledger.rs:543:35: replace && with || in Walker::fold_versioned",
    "crates/nika-cadence/src/ledger.rs:734:25: replace || with && in classify_journal",
    "crates/nika-cadence/src/ledger.rs:889:18: replace == with != in verify_payload",
    "crates/nika-cadence/src/ledger.rs:1107:17: delete ! in LifecycleValidator::accept",
    "crates/nika-cadence/src/ledger.rs:1108:75: replace == with != in LifecycleValidator::accept",
]

START_ALLOWED = {
    str(LANE / "census.txt"),
    str(LANE / "start.json"),
    str(LANE / "validate.py"),
    "scripts/estate_rules.py",
    "scripts/hygiene/tests/estate-testimonial.test.sh",
    "estate.yaml",
}
FINAL_ALLOWED = {
    str(LANE / "manifest.json"),
    str(LANE / "mutants.json"),
    str(LANE / "outcomes.json"),
    "docs/crate-specs/nika-cadence.md",
    "estate.yaml",
}


class EvidenceError(RuntimeError):
    """A testimonial invariant failed."""


def require(condition: bool, message: str) -> None:
    if not condition:
        raise EvidenceError(message)


def git(repo: pathlib.Path, *args: str, text: bool = True) -> str | bytes:
    return subprocess.check_output(
        ["git", *args], cwd=repo, text=text, stderr=subprocess.STDOUT
    )


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: pathlib.Path) -> str:
    return sha256_bytes(path.read_bytes())


def json_write(path: pathlib.Path, value: Any) -> None:
    path.write_text(
        json.dumps(value, indent=2, ensure_ascii=False, sort_keys=False) + "\n",
        encoding="utf-8",
    )


def input_paths(repo: pathlib.Path, commit: str) -> list[str]:
    tracked = git(repo, "ls-tree", "-r", "--name-only", commit).splitlines()
    selected = []
    for path in tracked:
        if path in {"Cargo.lock", "Cargo.toml", "rust-toolchain.toml", "crates/nika-cadence/Cargo.toml"}:
            selected.append(path)
        elif path.startswith(".cargo/") and path.endswith(".toml"):
            selected.append(path)
        elif path == "crates/nika-cadence/build.rs":
            selected.append(path)
        elif path.startswith("crates/nika-cadence/src/"):
            selected.append(path)
    return sorted(selected)


def input_records(repo: pathlib.Path, commit: str) -> list[dict[str, str]]:
    records = []
    for path in input_paths(repo, commit):
        blob = git(repo, "rev-parse", f"{commit}:{path}").strip()
        data = git(repo, "show", f"{commit}:{path}", text=False)
        records.append({"path": path, "git_blob": blob, "sha256": sha256_bytes(data)})
    return records


def version(argv: list[str]) -> str:
    return subprocess.check_output(argv, text=True).strip()


def capture_start(repo: pathlib.Path, raw_census: pathlib.Path) -> None:
    lane = repo / LANE
    raw = json.loads(raw_census.read_text(encoding="utf-8"))
    names = [item["name"] for item in raw]
    require(len(names) == len(set(names)), "census names are not unique")
    require(names.count(EXCLUDED_NAME) == 1, "equivalent identity is not unique")
    require(sum(re.fullmatch(EXCLUDED_RE, name) is not None for name in names) == 1, "anchored exclusion does not match exactly once")
    required = SAME_SPAN_CONTROLS + [item[2] for item in PRIOR_ANOMALIES] + GUARD_MUTANTS
    missing = sorted(set(required) - set(names))
    require(not missing, f"required census identities missing: {missing}")
    census = "".join(f"{name}\n" for name in names)
    (lane / "census.txt").write_text(census, encoding="utf-8")

    now = dt.datetime.now(dt.timezone.utc).isoformat().replace("+00:00", "Z")
    start = {
        "schema": "nika.mutation-testimonial.start.v2",
        "captured_at": now,
        "tested_commit": TESTED_COMMIT,
        "tested_tree": TESTED_TREE,
        "expected_receipt_parent": TESTED_COMMIT,
        "tested_worktree_clean": True,
        "scope": "non-runtime testimonial child; no Rust, test, Cargo, or configuration blob changes",
        "baseline": {
            "origin_main": git(repo, "rev-parse", "origin/main").strip(),
            "remote_pr_head": git(repo, "rev-parse", "origin/feat/arm-w7-ledger-salvage").strip(),
        },
        "tools": {
            "rustc": version(["rustc", "--version"]),
            "cargo": version(["cargo", "--version"]),
            "cargo_mutants": version(["cargo", "mutants", "--version"]),
            "python": version(["python3", "--version"]),
        },
        "run": {
            "worktree": RUN_WORKTREE,
            "cwd": RUN_WORKTREE,
            "output_dir": RUN_OUTPUT,
            "environment": {"CARGO_TARGET_DIR": RUN_TARGET},
            "argv": RUN_ARGV,
        },
        "census": {
            "argv": LIST_ARGV,
            "artifact": str(LANE / "census.txt"),
            "sha256": sha256_bytes(census.encode()),
            "unfiltered_count": len(names),
            "excluded_count": 1,
            "expected_executed_count": len(names) - 1,
        },
        "equivalent_exclusion": {
            "name": EXCLUDED_NAME,
            "anchored_regex": EXCLUDED_RE,
            "proof": "The claimed branch continues before receipt collection. Enumerate assigns one unique position per line, so no receipt tuple can share a claim tuple position. For every reachable pair, later > position and later >= position have the same truth value.",
            "same_span_controls": SAME_SPAN_CONTROLS,
        },
        "prior_anomalies": [
            {"prior_summary": summary, "prior_name": prior, "current_name": current}
            for summary, prior, current in PRIOR_ANOMALIES
        ],
        "guard_mutants": GUARD_MUTANTS,
        "input_selection": [
            "Cargo.lock",
            "Cargo.toml",
            "rust-toolchain.toml",
            ".cargo/*.toml",
            "crates/nika-cadence/Cargo.toml",
            "crates/nika-cadence/build.rs if tracked",
            "crates/nika-cadence/src/**",
        ],
        "inputs": input_records(repo, TESTED_COMMIT),
        "validator": {
            "path": str(LANE / "validate.py"),
            "sha256": sha256_file(lane / "validate.py"),
        },
    }
    json_write(lane / "start.json", start)


def read_names(path: pathlib.Path) -> list[str]:
    return path.read_text(encoding="utf-8").splitlines()


def verify_privacy(paths: list[pathlib.Path]) -> None:
    needles = ["/" + "Users" + "/", "/" + "private" + "/"]
    user = os.environ.get("USER", "")
    for path in paths:
        text = path.read_text(encoding="utf-8", errors="replace")
        for needle in needles:
            require(needle not in text, f"private path leaked in {path}")
        if user and len(user) > 2:
            require(user.casefold() not in text.casefold(), f"user identity leaked in {path}")


def changed_paths(repo: pathlib.Path, older: str, newer: str) -> set[str]:
    return set(git(repo, "diff", "--name-only", older, newer).splitlines())


def verify_start(repo: pathlib.Path, receipt_commit: str) -> dict[str, Any]:
    start_path = repo / LANE / "start.json"
    start = json.loads(start_path.read_text(encoding="utf-8"))
    require(start["schema"] == "nika.mutation-testimonial.start.v2", "wrong start schema")
    require(start["tested_commit"] == TESTED_COMMIT, "wrong tested commit")
    require(start["tested_tree"] == TESTED_TREE, "wrong tested tree")
    require(git(repo, "rev-parse", f"{TESTED_COMMIT}^{{tree}}").strip() == TESTED_TREE, "tested tree drift")
    require(git(repo, "rev-parse", f"{receipt_commit}^").strip() == TESTED_COMMIT, "start receipt is not a direct child of tested commit")
    delta = changed_paths(repo, TESTED_COMMIT, receipt_commit)
    require(delta <= START_ALLOWED, f"start commit contains non-evidence paths: {sorted(delta - START_ALLOWED)}")
    require(start["tested_worktree_clean"] is True, "start did not record a clean tested worktree")
    require(start["run"]["argv"] == RUN_ARGV, "run argv drift")
    require(start["run"]["environment"] == {"CARGO_TARGET_DIR": RUN_TARGET}, "target dir drift")
    require(start["run"]["worktree"] == RUN_WORKTREE, "run worktree drift")
    require(start["inputs"] == input_records(repo, TESTED_COMMIT), "tested input blob binding drift")
    require(start["validator"]["sha256"] == sha256_file(repo / LANE / "validate.py"), "validator hash drift")

    census_path = repo / LANE / "census.txt"
    names = read_names(census_path)
    require(len(names) == len(set(names)), "duplicate census names")
    require(sha256_file(census_path) == start["census"]["sha256"], "census hash drift")
    require(len(names) == start["census"]["unfiltered_count"], "census count drift")
    require(names.count(EXCLUDED_NAME) == 1, "excluded identity cardinality drift")
    require(sum(re.fullmatch(EXCLUDED_RE, name) is not None for name in names) == 1, "exclusion regex cardinality drift")
    require(start["census"]["expected_executed_count"] == len(names) - 1, "executed census arithmetic drift")
    required = SAME_SPAN_CONTROLS + [item[2] for item in PRIOR_ANOMALIES] + GUARD_MUTANTS
    require(not (set(required) - set(names)), "a required reconciliation identity left the census")
    verify_privacy([start_path, census_path, repo / LANE / "validate.py"])
    return start


def mutant_outcomes(outcomes: dict[str, Any]) -> dict[str, str]:
    result: dict[str, str] = {}
    for outcome in outcomes["outcomes"]:
        scenario = outcome["scenario"]
        if isinstance(scenario, dict) and "Mutant" in scenario:
            name = scenario["Mutant"]["name"]
            require(name not in result, f"duplicate outcome for {name}")
            result[name] = outcome["summary"]
    return result


def accounting(start: dict[str, Any], census: list[str], mutants: list[dict[str, Any]], outcomes: dict[str, Any]) -> dict[str, Any]:
    expected = set(census) - {start["equivalent_exclusion"]["name"]}
    mutant_names = [item["name"] for item in mutants]
    require(len(mutant_names) == len(set(mutant_names)), "duplicate mutants.json identities")
    require(set(mutant_names) == expected, "mutants.json is not census minus the one exclusion")
    by_name = mutant_outcomes(outcomes)
    require(set(by_name) == expected, "outcomes set is not census minus the one exclusion")
    counts = {summary: list(by_name.values()).count(summary) for summary in ["CaughtMutant", "Unviable", "MissedMutant", "Timeout"]}
    unknown = sorted(set(by_name.values()) - set(counts))
    require(not unknown, f"unknown outcome summaries: {unknown}")
    executed = len(expected)
    settled = sum(counts.values())
    require(settled == executed, f"incomplete bucket accounting: {settled} != {executed}")
    require(outcomes["total_mutants"] == executed, "top-level total_mutants drift")
    require(outcomes["caught"] == counts["CaughtMutant"], "top-level caught drift")
    require(outcomes["unviable"] == counts["Unviable"], "top-level unviable drift")
    require(outcomes["missed"] == counts["MissedMutant"], "top-level missed drift")
    require(outcomes["timeout"] == counts["Timeout"], "top-level timeout drift")
    require(counts["MissedMutant"] == 0, "a genuine mutant survived")
    require(counts["Timeout"] == 0, "a mutant timed out")
    viable = counts["CaughtMutant"] + counts["MissedMutant"]
    score = 100.0 * counts["CaughtMutant"] / viable if viable else 0.0
    require(score >= 90.0, f"viable score below floor: {score:.2f}")
    required_caught = start["equivalent_exclusion"]["same_span_controls"] + [item["current_name"] for item in start["prior_anomalies"]] + start["guard_mutants"]
    wrong = {name: by_name.get(name) for name in required_caught if by_name.get(name) != "CaughtMutant"}
    require(not wrong, f"anomaly/control reconciliation failed: {wrong}")
    return {
        "unfiltered_census": len(census),
        "excluded_equivalent": 1,
        "executed_mutants": executed,
        "caught": counts["CaughtMutant"],
        "unviable": counts["Unviable"],
        "missed": counts["MissedMutant"],
        "timeout": counts["Timeout"],
        "viable_denominator": viable,
        "viable_score_percent": round(score, 6),
        "complete_bucket_equation": f"{counts['CaughtMutant']} + {counts['Unviable']} + {counts['MissedMutant']} + {counts['Timeout']} = {executed}",
    }


def sanitize_value(value: Any, cargo_bin: str, counts: dict[str, int]) -> Any:
    if isinstance(value, str):
        occurrences = value.count(cargo_bin) if cargo_bin else 0
        if occurrences:
            counts["cargo_bin"] += occurrences
            return value.replace(cargo_bin, "<CARGO_BIN>")
        return value
    if isinstance(value, list):
        return [sanitize_value(item, cargo_bin, counts) for item in value]
    if isinstance(value, dict):
        return {key: sanitize_value(item, cargo_bin, counts) for key, item in value.items()}
    return value


def raw_artifacts(output: pathlib.Path) -> tuple[pathlib.Path, pathlib.Path]:
    candidates = [output / "mutants.out", output]
    for root in candidates:
        outcomes = root / "outcomes.json"
        mutants = root / "mutants.json"
        if outcomes.is_file() and mutants.is_file():
            return outcomes, mutants
    raise EvidenceError(f"cargo-mutants artifacts not found beneath {output}")


def sanitize(repo: pathlib.Path, receipt_commit: str, output: pathlib.Path) -> None:
    start = verify_start(repo, receipt_commit)
    raw_outcomes_path, raw_mutants_path = raw_artifacts(output)
    raw_outcomes = json.loads(raw_outcomes_path.read_text(encoding="utf-8"))
    raw_mutants = json.loads(raw_mutants_path.read_text(encoding="utf-8"))
    cargo_bin = shutil.which("cargo") or ""
    counts = {"cargo_bin": 0}
    safe_outcomes = sanitize_value(raw_outcomes, cargo_bin, counts)
    safe_mutants = sanitize_value(raw_mutants, cargo_bin, counts)
    require(counts["cargo_bin"] > 0, "cargo executable sanitization matched nothing")
    lane = repo / LANE
    json_write(lane / "outcomes.json", safe_outcomes)
    json_write(lane / "mutants.json", safe_mutants)
    census = read_names(lane / "census.txt")
    summary = accounting(start, census, safe_mutants, safe_outcomes)
    manifest = {
        "schema": "nika.mutation-testimonial.final.v2",
        "claim": "Complete serial cargo-mutants proof for the ARM W7 firing and ledger surfaces",
        "tested_commit": TESTED_COMMIT,
        "tested_tree": TESTED_TREE,
        "pre_run_receipt_commit": receipt_commit,
        "start_receipt_sha256": sha256_file(lane / "start.json"),
        "run": {
            "start_time": safe_outcomes["start_time"],
            "end_time": safe_outcomes["end_time"],
            "cargo_mutants_version": safe_outcomes["cargo_mutants_version"],
            "cwd": RUN_WORKTREE,
            "environment": {"CARGO_TARGET_DIR": RUN_TARGET},
            "argv": RUN_ARGV,
        },
        "artifacts": {
            "outcomes.json": {"raw_sha256": sha256_file(raw_outcomes_path), "sanitized_sha256": sha256_file(lane / "outcomes.json")},
            "mutants.json": {"raw_sha256": sha256_file(raw_mutants_path), "sanitized_sha256": sha256_file(lane / "mutants.json")},
            "census.txt": {"sha256": sha256_file(lane / "census.txt")},
        },
        "sanitization": {
            "algorithm": "recursive exact-string replacement in parsed JSON followed by Python 3 sorted-disabled indent=2 UTF-8 serialization and one trailing newline",
            "replacements": {"absolute_cargo_executable_to_<CARGO_BIN>": counts["cargo_bin"]},
            "omitted": ["lock.json", "debug.log", "per-mutant logs", "host identity", "user identity", "private absolute paths"],
        },
        "accounting": summary,
        "equivalent_exclusion": start["equivalent_exclusion"],
        "prior_anomaly_verification": [
            {**item, "final_summary": "CaughtMutant"} for item in start["prior_anomalies"]
        ],
        "guard_verification": [
            {"name": name, "final_summary": "CaughtMutant"} for name in start["guard_mutants"] + SAME_SPAN_CONTROLS
        ],
        "scope": "non-runtime testimonial child of the pre-run receipt; the tested Rust, tests, Cargo, and configuration blobs are those of tested_commit",
    }
    json_write(lane / "manifest.json", manifest)
    verify_privacy([lane / "start.json", lane / "census.txt", lane / "validate.py", lane / "outcomes.json", lane / "mutants.json", lane / "manifest.json"])


def verify_final(repo: pathlib.Path, final_commit: str, raw_output: pathlib.Path | None) -> dict[str, Any]:
    lane = repo / LANE
    manifest = json.loads((lane / "manifest.json").read_text(encoding="utf-8"))
    require(manifest["schema"] == "nika.mutation-testimonial.final.v2", "wrong final schema")
    receipt_commit = manifest["pre_run_receipt_commit"]
    start = verify_start(repo, receipt_commit)
    require(git(repo, "rev-parse", f"{final_commit}^").strip() == receipt_commit, "final evidence is not a direct child of pre-run receipt")
    delta = changed_paths(repo, receipt_commit, final_commit)
    require(delta <= FINAL_ALLOWED, f"final commit contains non-evidence paths: {sorted(delta - FINAL_ALLOWED)}")
    require(manifest["tested_commit"] == TESTED_COMMIT and manifest["tested_tree"] == TESTED_TREE, "final tested binding drift")
    require(manifest["start_receipt_sha256"] == sha256_file(lane / "start.json"), "start receipt hash drift")
    for name in ["outcomes.json", "mutants.json"]:
        require(manifest["artifacts"][name]["sanitized_sha256"] == sha256_file(lane / name), f"{name} sanitized hash drift")
    require(manifest["artifacts"]["census.txt"]["sha256"] == sha256_file(lane / "census.txt"), "census manifest hash drift")
    outcomes = json.loads((lane / "outcomes.json").read_text(encoding="utf-8"))
    mutants = json.loads((lane / "mutants.json").read_text(encoding="utf-8"))
    computed = accounting(start, read_names(lane / "census.txt"), mutants, outcomes)
    require(manifest["accounting"] == computed, "manifest accounting drift")
    if raw_output is not None:
        raw_outcomes_path, raw_mutants_path = raw_artifacts(raw_output)
        require(manifest["artifacts"]["outcomes.json"]["raw_sha256"] == sha256_file(raw_outcomes_path), "raw outcomes hash drift")
        require(manifest["artifacts"]["mutants.json"]["raw_sha256"] == sha256_file(raw_mutants_path), "raw mutants hash drift")
        cargo_bin = shutil.which("cargo") or ""
        counts = {"cargo_bin": 0}
        require(sanitize_value(json.loads(raw_outcomes_path.read_text()), cargo_bin, counts) == outcomes, "outcomes sanitization is not reproducible")
        require(sanitize_value(json.loads(raw_mutants_path.read_text()), cargo_bin, counts) == mutants, "mutants sanitization is not reproducible")
        require(counts["cargo_bin"] == manifest["sanitization"]["replacements"]["absolute_cargo_executable_to_<CARGO_BIN>"], "sanitization replacement count drift")
    verify_privacy(list(lane.iterdir()))
    return computed


def self_test() -> None:
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
    summary = accounting(start, census, mutants, outcomes)
    require(summary["viable_score_percent"] == 100.0, "green fixture failed")

    broken = json.loads(json.dumps(outcomes))
    broken["outcomes"][0]["summary"] = "Timeout"
    failures = 0
    for candidate_mutants, candidate_outcomes in [
        (mutants[:-1], outcomes),
        (mutants, broken),
    ]:
        try:
            accounting(start, census, candidate_mutants, candidate_outcomes)
        except EvidenceError:
            failures += 1
    require(failures == 2, "negative accounting mutations survived")
    with tempfile.TemporaryDirectory() as tmp:
        leaked = pathlib.Path(tmp) / "leak.txt"
        leaked.write_text("/" + "Users" + "/someone/source")
        try:
            verify_privacy([leaked])
        except EvidenceError:
            failures += 1
    require(failures == 3, "privacy mutation survived")
    print("OK: validator green fixture passed; 3/3 negative mutations rejected")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=["capture-start", "verify-start", "sanitize", "verify-final", "self-test"])
    parser.add_argument("--repo", type=pathlib.Path, default=pathlib.Path.cwd())
    parser.add_argument("--raw-census", type=pathlib.Path)
    parser.add_argument("--receipt-commit")
    parser.add_argument("--final-commit")
    parser.add_argument("--raw-output", type=pathlib.Path)
    args = parser.parse_args()
    try:
        if args.command == "capture-start":
            require(args.raw_census is not None, "--raw-census is required")
            capture_start(args.repo.resolve(), args.raw_census)
        elif args.command == "verify-start":
            require(args.receipt_commit is not None, "--receipt-commit is required")
            verify_start(args.repo.resolve(), args.receipt_commit)
            print("OK: pre-run receipt, causal parent, input blobs, census, and privacy verified")
        elif args.command == "sanitize":
            require(args.receipt_commit is not None and args.raw_output is not None, "--receipt-commit and --raw-output are required")
            sanitize(args.repo.resolve(), args.receipt_commit, args.raw_output)
        elif args.command == "verify-final":
            require(args.final_commit is not None, "--final-commit is required")
            result = verify_final(args.repo.resolve(), args.final_commit, args.raw_output)
            print(f"OK: final evidence verified · {result['caught']}/{result['viable_denominator']} viable caught · {result['viable_score_percent']:.2f}%")
        else:
            self_test()
        return 0
    except (EvidenceError, KeyError, json.JSONDecodeError, subprocess.CalledProcessError) as error:
        print(f"ERROR: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
