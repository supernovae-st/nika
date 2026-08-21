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
import stat
import subprocess
import sys
import time
from typing import Any

LANE = pathlib.Path("docs/testimonials/arm-w7-ledger-salvage")
PROTOCOL_PATH = pathlib.Path(__file__).with_name("protocol.json")
PROTOCOL = json.loads(PROTOCOL_PATH.read_text(encoding="utf-8"))
TESTED_COMMIT = PROTOCOL["tested_commit"]
TESTED_TREE = PROTOCOL["tested_tree"]
RUN_ID = PROTOCOL["run_id"]
RUN_WORKTREE = PROTOCOL["worktree"]
RUN_OUTPUT = PROTOCOL["output"]
RUN_TARGET = PROTOCOL["target"]
RAW_RUN_RECEIPT = pathlib.Path(RUN_OUTPUT) / "run-receipt.raw.json"
MONITOR_INTERVAL_SECONDS = 1.0
RELEVANT_ENV = PROTOCOL["relevant_env"]
EXCLUDED_NAME = PROTOCOL["excluded_name"]
EXCLUDED_RE = PROTOCOL["excluded_regex"]
RUN_ARGV = PROTOCOL["run_argv"]
LIST_ARGV = PROTOCOL["list_argv"]
RUNNER_ARGV = PROTOCOL["runner_argv"]
SAME_SPAN_CONTROLS = PROTOCOL["same_span_controls"]

PRIOR_ANOMALIES = [
    (item["prior_summary"], item["prior_name"], item["current_name"])
    for item in PROTOCOL["prior_anomalies"]
]
GUARD_MUTANTS = PROTOCOL["guard_mutants"]

START_ALLOWED = {
    str(LANE / "census.txt"),
    str(LANE / "start.json"),
    str(LANE / "validate.py"),
    str(LANE / "selftest.py"),
    str(LANE / "protocol.json"),
    "scripts/estate_rules.py",
    "scripts/hygiene/tests/estate-testimonial.test.sh",
    "estate.yaml",
}
FINAL_ALLOWED = {
    str(LANE / "manifest.json"),
    str(LANE / "mutants.json"),
    str(LANE / "outcomes.json"),
    str(LANE / "run-receipt.json"),
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
        if path in {"Cargo.lock", "Cargo.toml", "rust-toolchain.toml", "crates/nika-cadence/Cargo.toml"} or path.startswith(".cargo/") and path.endswith(".toml") or path == "crates/nika-cadence/build.rs" or path.startswith("crates/nika-cadence/src/"):
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


def require_detached_head(returncode: int, symbolic_ref: str) -> None:
    require(
        returncode == 1 and symbolic_ref == "",
        f"run worktree HEAD is symbolic: {symbolic_ref or '<probe-error>'}",
    )


def probe_run_worktree() -> dict[str, Any]:
    run = pathlib.Path(RUN_WORKTREE)
    require(run.is_dir(), f"detached run worktree is absent: {run}")
    head = git(run, "rev-parse", "HEAD").strip()
    tree = git(run, "rev-parse", "HEAD^{tree}").strip()
    index_tree = git(run, "write-tree").strip()
    status = git(run, "status", "--porcelain=v2", "--untracked-files=all")
    index_clean = subprocess.run(
        ["git", "diff", "--cached", "--quiet"], cwd=run, check=False
    ).returncode == 0
    worktree_clean = subprocess.run(
        ["git", "diff", "--quiet"], cwd=run, check=False
    ).returncode == 0
    symbolic = subprocess.run(
        ["git", "symbolic-ref", "-q", "HEAD"],
        cwd=run,
        check=False,
        capture_output=True,
        text=True,
    )
    symbolic_ref = symbolic.stdout.strip()
    require_detached_head(symbolic.returncode, symbolic_ref)
    require(head == TESTED_COMMIT, f"detached worktree HEAD drift: {head}")
    require(tree == TESTED_TREE, f"detached worktree tree drift: {tree}")
    require(index_tree == TESTED_TREE, f"detached worktree index drift: {index_tree}")
    require(index_clean, "detached worktree index is dirty")
    require(worktree_clean, "detached worktree tracked files are dirty")
    require(status == "", "detached worktree has tracked or untracked changes")
    return {
        "head": head,
        "tree": tree,
        "index_tree": index_tree,
        "index_clean": index_clean,
        "worktree_clean": worktree_clean,
        "porcelain_v2": status,
        "porcelain_v2_sha256": sha256_bytes(status.encode()),
        "detached_head": True,
        "symbolic_ref": None,
    }


def require_run_identity(run_id: str, output: str, target: str) -> None:
    require(
        re.fullmatch(r"arm-w7-[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}", run_id) is not None,
        "run_id is not a unique UUIDv4 ARM W7 identity",
    )
    require(run_id == RUN_ID, "run_id drift")
    require(output == RUN_OUTPUT and run_id in output, "output path/run_id mismatch")
    require(target == RUN_TARGET and run_id in target, "target path/run_id mismatch")
    require(output != target, "output and target paths collide")


def require_run_id_absent_from_tested(repo: pathlib.Path) -> None:
    probe = subprocess.run(
        ["git", "grep", "-F", "--quiet", RUN_ID, TESTED_COMMIT],
        cwd=repo,
        check=False,
    )
    require(probe.returncode == 1, "run_id already exists in the tested tree or grep failed")


def probe_paths_absent() -> dict[str, Any]:
    checked_at = dt.datetime.now(dt.timezone.utc).isoformat().replace("+00:00", "Z")
    result = {
        "checked_at": checked_at,
        "output": {"path": RUN_OUTPUT, "exists": os.path.lexists(RUN_OUTPUT)},
        "target": {"path": RUN_TARGET, "exists": os.path.lexists(RUN_TARGET)},
    }
    require(result["output"]["exists"] is False, "reserved output path already exists")
    require(result["target"]["exists"] is False, "reserved target path already exists")
    return result


def capture_start(repo: pathlib.Path, raw_census: pathlib.Path) -> None:
    require_run_id_absent_from_tested(repo)
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
        "run_id": RUN_ID,
        "scope": "non-runtime pre-run evidence infrastructure child: testimonial files, validator, estate classification rule and self-test, and derived estate projection; no crates/**, Rust tests, Cargo manifests/lock/config, .cargo, or rust-toolchain blobs",
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
            "wrapper_argv": RUNNER_ARGV,
            "detached_probe": probe_run_worktree(),
            "path_absence_probe": probe_paths_absent(),
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
        "selftest": {
            "path": str(LANE / "selftest.py"),
            "sha256": sha256_file(lane / "selftest.py"),
        },
        "protocol": {
            "path": str(LANE / "protocol.json"),
            "sha256": sha256_file(lane / "protocol.json"),
        },
    }
    json_write(lane / "start.json", start)


def read_names(path: pathlib.Path) -> list[str]:
    return path.read_text(encoding="utf-8").splitlines()


def verify_privacy(paths: list[pathlib.Path]) -> None:
    needles = [
        "/" + "Users" + "/",
        "/" + "home" + "/",
        "/" + "private" + "/",
    ]
    windows_user_path = re.compile(
        r"[A-Za-z]:[\\/](?:Users|Documents and Settings)[\\/]"
    )
    credential_patterns = [
        re.compile(r"(?i)\b(?:api[_-]?key|access[_-]?token|secret)[\"'\s:=]+[A-Za-z0-9_./+=-]{12,}"),
        re.compile(r"\b" + "s" + "k-" + r"[A-Za-z0-9_-]{12,}"),
    ]
    user = os.environ.get("USER", "")
    for evidence_path in paths:
        text = evidence_path.read_text(encoding="utf-8", errors="replace")
        for needle in needles:
            require(needle not in text, f"private path leaked in {evidence_path}")
        require(
            windows_user_path.search(text) is None,
            f"Windows user path leaked in {evidence_path}",
        )
        for pattern in credential_patterns:
            require(pattern.search(text) is None, f"credential-shaped value leaked in {evidence_path}")
        if user and len(user) > 2:
            require(
                user.casefold() not in text.casefold(),
                f"user identity leaked in {evidence_path}",
            )


def changed_paths(repo: pathlib.Path, older: str, newer: str) -> set[str]:
    return set(git(repo, "diff", "--name-only", older, newer).splitlines())


def verify_start(
    repo: pathlib.Path, receipt_commit: str, *, require_paths_absent: bool
) -> dict[str, Any]:
    start_path = repo / LANE / "start.json"
    start = json.loads(start_path.read_text(encoding="utf-8"))
    require(start["schema"] == "nika.mutation-testimonial.start.v2", "wrong start schema")
    require(start["tested_commit"] == TESTED_COMMIT, "wrong tested commit")
    require(start["tested_tree"] == TESTED_TREE, "wrong tested tree")
    require(git(repo, "rev-parse", f"{TESTED_COMMIT}^{{tree}}").strip() == TESTED_TREE, "tested tree drift")
    require(git(repo, "rev-parse", f"{receipt_commit}^").strip() == TESTED_COMMIT, "start receipt is not a direct child of tested commit")
    delta = changed_paths(repo, TESTED_COMMIT, receipt_commit)
    require(delta <= START_ALLOWED, f"start commit contains non-evidence paths: {sorted(delta - START_ALLOWED)}")
    require_run_identity(
        start["run_id"],
        start["run"]["output_dir"],
        start["run"]["environment"]["CARGO_TARGET_DIR"],
    )
    require_run_id_absent_from_tested(repo)
    require(start["run"]["argv"] == RUN_ARGV, "run argv drift")
    require(start["run"]["wrapper_argv"] == RUNNER_ARGV, "atomic wrapper argv drift")
    require(start["run"]["environment"] == {"CARGO_TARGET_DIR": RUN_TARGET}, "target dir drift")
    require(start["run"]["worktree"] == RUN_WORKTREE, "run worktree drift")
    require(start["run"]["detached_probe"] == probe_run_worktree(), "detached worktree probe drift")
    absence = start["run"]["path_absence_probe"]
    require(absence["output"] == {"path": RUN_OUTPUT, "exists": False}, "recorded output absence drift")
    require(absence["target"] == {"path": RUN_TARGET, "exists": False}, "recorded target absence drift")
    parse_timestamp(absence["checked_at"])
    if require_paths_absent:
        probe_paths_absent()
    require(start["inputs"] == input_records(repo, TESTED_COMMIT), "tested input blob binding drift")
    require(start["validator"]["sha256"] == sha256_file(repo / LANE / "validate.py"), "validator hash drift")
    require(start["selftest"]["sha256"] == sha256_file(repo / LANE / "selftest.py"), "selftest hash drift")
    require(start["protocol"]["sha256"] == sha256_file(repo / LANE / "protocol.json"), "protocol hash drift")

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
    verify_privacy([start_path, census_path, repo / LANE / "validate.py", repo / LANE / "selftest.py", repo / LANE / "protocol.json"])
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


def parse_timestamp(value: str) -> dt.datetime:
    parsed = dt.datetime.fromisoformat(value.replace("Z", "+00:00"))
    require(parsed.tzinfo is not None, f"timestamp has no timezone: {value}")
    return parsed


def require_run_after_receipt(receipt_time: str, run_start: str) -> None:
    require(
        parse_timestamp(run_start) > parse_timestamp(receipt_time),
        f"run did not start after committed receipt: {run_start} <= {receipt_time}",
    )


def require_outcome_time_authority(
    manifest_run: dict[str, Any], outcomes: dict[str, Any], receipt_time: str
) -> None:
    require(
        manifest_run["start_time"] == outcomes["start_time"],
        "manifest start_time is not outcomes.json start_time",
    )
    require(
        manifest_run["end_time"] == outcomes["end_time"],
        "manifest end_time is not outcomes.json end_time",
    )
    require_run_after_receipt(receipt_time, outcomes["start_time"])
    require(
        parse_timestamp(outcomes["end_time"]) >= parse_timestamp(outcomes["start_time"]),
        "outcomes end_time precedes start_time",
    )


def now_utc() -> str:
    return dt.datetime.now(dt.timezone.utc).isoformat().replace("+00:00", "Z")


def cargo_mutants_binary() -> dict[str, Any]:
    invoked = shutil.which("cargo-mutants")
    require(invoked is not None, "cargo-mutants binary is not on PATH")
    real = pathlib.Path(invoked).resolve(strict=True)
    require(stat.S_ISREG(os.lstat(real).st_mode), "cargo-mutants realpath is not regular")
    return {
        "invoked_path": invoked,
        "realpath": str(real),
        "sha256": sha256_file(real),
        "version": version(["cargo", "mutants", "--version"]),
    }


def relevant_environment(environment: dict[str, str]) -> list[dict[str, Any]]:
    records = []
    for name in RELEVANT_ENV:
        value = environment.get(name)
        records.append(
            {
                "name": name,
                "present": value is not None,
                "value_sha256": sha256_bytes(value.encode()) if value is not None else None,
            }
        )
    return records


def quick_worktree_probe() -> dict[str, Any]:
    run = pathlib.Path(RUN_WORKTREE)
    revisions = git(run, "rev-parse", "HEAD", "HEAD^{tree}").splitlines()
    status = git(run, "status", "--porcelain=v2", "--untracked-files=all")
    symbolic = subprocess.run(
        ["git", "symbolic-ref", "-q", "HEAD"],
        cwd=run,
        check=False,
        capture_output=True,
        text=True,
    )
    clean = (
        revisions == [TESTED_COMMIT, TESTED_TREE]
        and status == ""
        and symbolic.returncode == 1
        and symbolic.stdout == ""
    )
    return {
        "clean": clean,
        "head": revisions[0] if revisions else None,
        "tree": revisions[1] if len(revisions) > 1 else None,
        "status_sha256": sha256_bytes(status.encode()),
        "symbolic_ref": symbolic.stdout.strip() or None,
    }


def require_monitoring(monitoring: dict[str, Any]) -> None:
    require(monitoring["interval_ms"] == 1000, "monitor interval drift")
    require(monitoring["probe_count"] >= 1, "runner recorded no during-run probe")
    require(monitoring["violation"] is None, "detached worktree changed during run")


def run_process(environment: dict[str, str]) -> tuple[int, dict[str, Any]]:
    process = subprocess.Popen(RUN_ARGV, cwd=RUN_WORKTREE, env=environment)
    probe_count = 0
    violation = None
    try:
        while True:
            returncode = process.poll()
            probe = quick_worktree_probe()
            probe_count += 1
            if not probe["clean"]:
                violation = probe
                if returncode is None:
                    process.terminate()
                    try:
                        returncode = process.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        returncode = process.wait()
                break
            if returncode is not None:
                break
            time.sleep(MONITOR_INTERVAL_SECONDS)
    except BaseException:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        raise
    monitoring = {
        "interval_ms": int(MONITOR_INTERVAL_SECONDS * 1000),
        "probe_count": probe_count,
        "violation": violation,
    }
    return int(returncode), monitoring


def artifact_record(output: pathlib.Path, artifact: pathlib.Path) -> dict[str, Any]:
    require(output.is_dir(), f"artifact output directory is absent: {output}")
    require(os.path.lexists(artifact), f"artifact is absent: {artifact}")
    output_real = output.resolve(strict=True)
    metadata = os.lstat(artifact)
    require(not stat.S_ISLNK(metadata.st_mode), f"artifact is a symlink: {artifact}")
    require(stat.S_ISREG(metadata.st_mode), f"artifact is not regular: {artifact}")
    real = artifact.resolve(strict=True)
    require(real.is_relative_to(output_real), f"artifact escapes output directory: {artifact}")
    return {
        "relative_path": str(real.relative_to(output_real)),
        "realpath": str(real),
        "regular": True,
        "symlink": False,
        "size": metadata.st_size,
        "sha256": sha256_file(real),
    }


def raw_counts(outcomes: dict[str, Any], mutants: list[dict[str, Any]]) -> dict[str, int]:
    return {
        "mutants_json": len(mutants),
        "outcome_mutants": len(mutant_outcomes(outcomes)),
        "total_mutants": outcomes["total_mutants"],
        "caught": outcomes["caught"],
        "unviable": outcomes["unviable"],
        "missed": outcomes["missed"],
        "timeout": outcomes["timeout"],
    }


def run_mutation(repo: pathlib.Path, receipt_commit: str) -> None:
    receipt = git(repo, "rev-parse", receipt_commit).strip()
    require(git(repo, "rev-parse", "HEAD").strip() == receipt, "runner must execute from receipt HEAD")
    require(
        git(repo, "status", "--porcelain=v2", "--untracked-files=all") == "",
        "receipt worktree is dirty",
    )
    start = verify_start(repo, receipt, require_paths_absent=True)
    pre_probe = probe_run_worktree()
    environment = os.environ.copy()
    environment["CARGO_TARGET_DIR"] = RUN_TARGET
    binary = cargo_mutants_binary()
    wall_start = now_utc()
    monotonic_start = time.monotonic_ns()
    returncode, monitoring = run_process(environment)
    monotonic_end = time.monotonic_ns()
    wall_end = now_utc()
    post_probe = probe_run_worktree()
    raw_outcomes_path, raw_mutants_path = raw_artifacts(pathlib.Path(RUN_OUTPUT))
    raw_outcomes = json.loads(raw_outcomes_path.read_text(encoding="utf-8"))
    raw_mutants = json.loads(raw_mutants_path.read_text(encoding="utf-8"))
    run_receipt = {
        "schema": "nika.mutation-testimonial.run.raw.v1",
        "guarantee": "Reproducible local testimonial from the committed wrapper; not remote attestation and not a claim against a malicious local operator.",
        "receipt_commit": receipt,
        "tested_commit": TESTED_COMMIT,
        "tested_tree": TESTED_TREE,
        "run_id": RUN_ID,
        "wrapper": {
            "argv": RUNNER_ARGV,
            "validator_sha256": start["validator"]["sha256"],
            "protocol_sha256": start["protocol"]["sha256"],
        },
        "process": {
            "argv": RUN_ARGV,
            "cwd": RUN_WORKTREE,
            "returncode": returncode,
            "wall_start": wall_start,
            "wall_end": wall_end,
            "monotonic_start_ns": monotonic_start,
            "monotonic_end_ns": monotonic_end,
        },
        "cargo_mutants_binary": binary,
        "environment": relevant_environment(environment),
        "pre_probe": pre_probe,
        "monitoring": monitoring,
        "post_probe": post_probe,
        "artifacts": {
            "outcomes.json": artifact_record(pathlib.Path(RUN_OUTPUT), raw_outcomes_path),
            "mutants.json": artifact_record(pathlib.Path(RUN_OUTPUT), raw_mutants_path),
        },
        "counts": raw_counts(raw_outcomes, raw_mutants),
        "written_at": now_utc(),
    }
    json_write(RAW_RUN_RECEIPT, run_receipt)
    require(returncode == 0, f"cargo-mutants exited {returncode}")
    require_monitoring(monitoring)
    require(monotonic_end > monotonic_start, "non-positive monotonic run duration")
    require(pre_probe == post_probe == start["run"]["detached_probe"], "pre/post worktree probe drift")
    print("OK: runner emitted a bound raw receipt after process completion")


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


def known_replacements() -> dict[str, str]:
    cargo = shutil.which("cargo") or ""
    cargo_real = str(pathlib.Path(cargo).resolve(strict=True)) if cargo else ""
    mutants = cargo_mutants_binary()
    return {
        cargo: "<CARGO_BIN>",
        cargo_real: "<CARGO_BIN>",
        mutants["invoked_path"]: "<CARGO_MUTANTS_BIN>",
        mutants["realpath"]: "<CARGO_MUTANTS_BIN>",
    }


def sanitize_value(
    value: Any, replacements: dict[str, str], counts: dict[str, int]
) -> Any:
    if isinstance(value, str):
        sanitized = value
        for source, replacement in sorted(
            replacements.items(), key=lambda item: len(item[0]), reverse=True
        ):
            if not source:
                continue
            occurrences = sanitized.count(source)
            if occurrences:
                counts[replacement] = counts.get(replacement, 0) + occurrences
                sanitized = sanitized.replace(source, replacement)
        return sanitized
    if isinstance(value, list):
        return [sanitize_value(item, replacements, counts) for item in value]
    if isinstance(value, dict):
        return {
            key: sanitize_value(item, replacements, counts)
            for key, item in value.items()
        }
    return value


def raw_artifacts(output: pathlib.Path) -> tuple[pathlib.Path, pathlib.Path]:
    candidates = [output / "mutants.out", output]
    for root in candidates:
        outcomes = root / "outcomes.json"
        mutants = root / "mutants.json"
        if outcomes.is_file() and mutants.is_file():
            return outcomes, mutants
    raise EvidenceError(f"cargo-mutants artifacts not found beneath {output}")


def require_receipt_identity(run_receipt: dict[str, Any], receipt: str) -> None:
    require(run_receipt["schema"] == "nika.mutation-testimonial.run.raw.v1", "wrong raw run receipt schema")
    require(run_receipt["receipt_commit"] == receipt, "raw run receipt commit mismatch")
    require(run_receipt["tested_commit"] == TESTED_COMMIT, "raw run receipt tested commit mismatch")
    require(run_receipt["tested_tree"] == TESTED_TREE, "raw run receipt tested tree mismatch")
    require(run_receipt["run_id"] == RUN_ID, "raw run receipt run_id mismatch")


def validate_raw_run_receipt(
    repo: pathlib.Path, receipt: str, output: pathlib.Path
) -> tuple[dict[str, Any], dict[str, Any], list[dict[str, Any]]]:
    require(output.resolve(strict=True) == pathlib.Path(RUN_OUTPUT), "raw output path mismatch")
    artifact_record(output, RAW_RUN_RECEIPT)
    run_receipt = json.loads(RAW_RUN_RECEIPT.read_text(encoding="utf-8"))
    require_receipt_identity(run_receipt, receipt)
    require(git(repo, "rev-parse", f"{receipt}^").strip() == TESTED_COMMIT, "receipt parent drift")
    ancestor = subprocess.run(
        ["git", "merge-base", "--is-ancestor", TESTED_COMMIT, receipt],
        cwd=repo,
        check=False,
    )
    require(ancestor.returncode == 0, "tested anchor is not receipt ancestry")
    require(run_receipt["wrapper"] == {"argv": RUNNER_ARGV, "validator_sha256": sha256_file(repo / LANE / "validate.py"), "protocol_sha256": sha256_file(repo / LANE / "protocol.json")}, "raw wrapper binding drift")
    process = run_receipt["process"]
    require(process["argv"] == RUN_ARGV and process["cwd"] == RUN_WORKTREE, "raw process invocation drift")
    require(process["returncode"] == 0, "raw process did not exit zero")
    require(process["monotonic_end_ns"] > process["monotonic_start_ns"], "raw monotonic duration is not positive")
    require(run_receipt["cargo_mutants_binary"] == cargo_mutants_binary(), "cargo-mutants binary binding drift")
    environment = os.environ.copy()
    environment["CARGO_TARGET_DIR"] = RUN_TARGET
    require(run_receipt["environment"] == relevant_environment(environment), "relevant environment hash drift")
    start = json.loads((repo / LANE / "start.json").read_text(encoding="utf-8"))
    require(run_receipt["pre_probe"] == start["run"]["detached_probe"], "raw pre-probe drift")
    require(run_receipt["post_probe"] == probe_run_worktree(), "raw post-probe drift")
    require_monitoring(run_receipt["monitoring"])
    raw_outcomes_path, raw_mutants_path = raw_artifacts(output)
    expected_artifacts = {
        "outcomes.json": artifact_record(output, raw_outcomes_path),
        "mutants.json": artifact_record(output, raw_mutants_path),
    }
    require(run_receipt["artifacts"] == expected_artifacts, "raw artifact lstat/hash binding drift")
    outcomes = json.loads(raw_outcomes_path.read_text(encoding="utf-8"))
    mutants = json.loads(raw_mutants_path.read_text(encoding="utf-8"))
    require(run_receipt["counts"] == raw_counts(outcomes, mutants), "raw artifact counts drift")
    receipt_time = git(repo, "show", "-s", "--format=%cI", receipt).strip()
    require_run_after_receipt(receipt_time, outcomes["start_time"])
    require(parse_timestamp(process["wall_start"]) <= parse_timestamp(outcomes["start_time"]), "outcomes started before wrapper")
    require(parse_timestamp(outcomes["end_time"]) <= parse_timestamp(process["wall_end"]), "outcomes ended after wrapper")
    require(parse_timestamp(process["wall_end"]) <= parse_timestamp(run_receipt["written_at"]), "raw receipt predates process end")
    return run_receipt, outcomes, mutants


def sanitize(repo: pathlib.Path, receipt_commit: str, output: pathlib.Path) -> None:
    receipt_commit = git(repo, "rev-parse", receipt_commit).strip()
    start = verify_start(repo, receipt_commit, require_paths_absent=False)
    require(git(repo, "rev-parse", "HEAD").strip() == receipt_commit, "sanitize must run from receipt HEAD")
    require(output.resolve() == pathlib.Path(start["run"]["output_dir"]), "raw output path/run_id mismatch")
    raw_receipt, raw_outcomes, raw_mutants = validate_raw_run_receipt(
        repo, receipt_commit, output
    )
    raw_outcomes_path, raw_mutants_path = raw_artifacts(output)
    replacements = known_replacements()
    replacement_counts: dict[str, int] = {}
    safe_outcomes = sanitize_value(raw_outcomes, replacements, replacement_counts)
    safe_mutants = sanitize_value(raw_mutants, replacements, replacement_counts)
    safe_receipt = sanitize_value(raw_receipt, replacements, replacement_counts)
    require(replacement_counts.get("<CARGO_BIN>", 0) > 0, "cargo path sanitization matched nothing")
    require(replacement_counts.get("<CARGO_MUTANTS_BIN>", 0) > 0, "cargo-mutants path sanitization matched nothing")
    lane = repo / LANE
    json_write(lane / "outcomes.json", safe_outcomes)
    json_write(lane / "mutants.json", safe_mutants)
    json_write(lane / "run-receipt.json", safe_receipt)
    verify_privacy(
        [
            lane / "start.json",
            lane / "census.txt",
            lane / "validate.py",
            lane / "outcomes.json",
            lane / "mutants.json",
            lane / "run-receipt.json",
        ]
    )
    census = read_names(lane / "census.txt")
    summary = accounting(start, census, safe_mutants, safe_outcomes)
    manifest = {
        "schema": "nika.mutation-testimonial.final.v2",
        "claim": "Reproducible local testimonial of a complete serial cargo-mutants run for the ARM W7 firing and ledger surfaces; not remote attestation",
        "tested_commit": TESTED_COMMIT,
        "tested_tree": TESTED_TREE,
        "run_id": RUN_ID,
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
            "run-receipt.json": {"raw_sha256": sha256_file(RAW_RUN_RECEIPT), "sanitized_sha256": sha256_file(lane / "run-receipt.json")},
            "census.txt": {"sha256": sha256_file(lane / "census.txt")},
        },
        "sanitization": {
            "algorithm": "recursive replacement of only the resolved cargo and cargo-mutants executable paths, followed by Python 3 indent=2 UTF-8 serialization and one trailing newline",
            "replacements": replacement_counts,
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
    verify_privacy(list(lane.iterdir()))


def verify_final(repo: pathlib.Path, final_commit: str, raw_output: pathlib.Path | None) -> dict[str, Any]:
    final_commit = git(repo, "rev-parse", final_commit).strip()
    lane = repo / LANE
    manifest = json.loads((lane / "manifest.json").read_text(encoding="utf-8"))
    run_receipt = json.loads((lane / "run-receipt.json").read_text(encoding="utf-8"))
    require(manifest["schema"] == "nika.mutation-testimonial.final.v2", "wrong final schema")
    receipt_commit = git(repo, "rev-parse", f"{final_commit}^").strip()
    require(manifest["pre_run_receipt_commit"] == receipt_commit, "manifest receipt is not final parent")
    require_receipt_identity(run_receipt, receipt_commit)
    start = verify_start(repo, receipt_commit, require_paths_absent=False)
    delta = changed_paths(repo, receipt_commit, final_commit)
    require(delta <= FINAL_ALLOWED, f"final commit contains non-evidence paths: {sorted(delta - FINAL_ALLOWED)}")
    require(manifest["tested_commit"] == TESTED_COMMIT and manifest["tested_tree"] == TESTED_TREE, "final tested binding drift")
    require(manifest["run_id"] == start["run_id"] == RUN_ID, "final run_id drift")
    require(manifest["start_receipt_sha256"] == sha256_file(lane / "start.json"), "start receipt hash drift")
    for name in ["outcomes.json", "mutants.json", "run-receipt.json"]:
        require(manifest["artifacts"][name]["sanitized_sha256"] == sha256_file(lane / name), f"{name} sanitized hash drift")
    require(manifest["artifacts"]["census.txt"]["sha256"] == sha256_file(lane / "census.txt"), "census manifest hash drift")
    outcomes = json.loads((lane / "outcomes.json").read_text(encoding="utf-8"))
    mutants = json.loads((lane / "mutants.json").read_text(encoding="utf-8"))
    receipt_time = git(repo, "show", "-s", "--format=%cI", receipt_commit).strip()
    require_outcome_time_authority(manifest["run"], outcomes, receipt_time)
    process = run_receipt["process"]
    require(process["argv"] == RUN_ARGV and process["cwd"] == RUN_WORKTREE, "committed run invocation drift")
    require(process["returncode"] == 0, "committed run receipt is nonzero")
    require(parse_timestamp(process["wall_start"]) <= parse_timestamp(outcomes["start_time"]), "committed outcomes predate wrapper")
    require(parse_timestamp(outcomes["end_time"]) <= parse_timestamp(process["wall_end"]), "committed outcomes outlive wrapper")
    require(process["monotonic_end_ns"] > process["monotonic_start_ns"], "committed monotonic duration invalid")
    require_monitoring(run_receipt["monitoring"])
    require(run_receipt["pre_probe"] == start["run"]["detached_probe"], "committed pre-probe drift")
    require(run_receipt["post_probe"] == probe_run_worktree(), "committed post-probe drift")
    expected_binary = sanitize_value(cargo_mutants_binary(), known_replacements(), {})
    require(run_receipt["cargo_mutants_binary"] == expected_binary, "committed binary binding drift")
    environment = os.environ.copy()
    environment["CARGO_TARGET_DIR"] = RUN_TARGET
    require(run_receipt["environment"] == relevant_environment(environment), "committed environment hash drift")
    require(run_receipt["counts"] == raw_counts(outcomes, mutants), "committed raw counts drift")
    for name in ["outcomes.json", "mutants.json"]:
        artifact = run_receipt["artifacts"][name]
        require(artifact["regular"] is True and artifact["symlink"] is False, f"committed {name} lstat drift")
        require(artifact["sha256"] == manifest["artifacts"][name]["raw_sha256"], f"committed {name} raw hash drift")
    computed = accounting(start, read_names(lane / "census.txt"), mutants, outcomes)
    require(manifest["accounting"] == computed, "manifest accounting drift")
    if raw_output is not None:
        require(raw_output.resolve() == pathlib.Path(start["run"]["output_dir"]), "raw output path/run_id mismatch")
        raw_receipt, raw_outcomes, raw_mutants = validate_raw_run_receipt(repo, receipt_commit, raw_output)
        replacements = known_replacements()
        counts: dict[str, int] = {}
        require(sanitize_value(raw_outcomes, replacements, counts) == outcomes, "outcomes sanitization is not reproducible")
        require(sanitize_value(raw_mutants, replacements, counts) == mutants, "mutants sanitization is not reproducible")
        require(sanitize_value(raw_receipt, replacements, counts) == run_receipt, "run receipt sanitization is not reproducible")
        require(counts == manifest["sanitization"]["replacements"], "sanitization replacement counts drift")
        require(manifest["artifacts"]["run-receipt.json"]["raw_sha256"] == sha256_file(RAW_RUN_RECEIPT), "raw run receipt hash drift")
    verify_privacy(list(lane.iterdir()))
    return computed


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=["capture-start", "verify-start", "run", "sanitize", "verify-final"])
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
            verify_start(
                args.repo.resolve(), args.receipt_commit, require_paths_absent=True
            )
            print("OK: pre-run receipt, causal parent, input blobs, census, and privacy verified")
        elif args.command == "run":
            require(args.receipt_commit is not None, "--receipt-commit is required")
            run_mutation(args.repo.resolve(), args.receipt_commit)
        elif args.command == "sanitize":
            require(args.receipt_commit is not None and args.raw_output is not None, "--receipt-commit and --raw-output are required")
            sanitize(args.repo.resolve(), args.receipt_commit, args.raw_output)
        elif args.command == "verify-final":
            require(args.final_commit is not None, "--final-commit is required")
            result = verify_final(args.repo.resolve(), args.final_commit, args.raw_output)
            print(f"OK: final evidence verified · {result['caught']}/{result['viable_denominator']} viable caught · {result['viable_score_percent']:.2f}%")
        return 0
    except (EvidenceError, KeyError, json.JSONDecodeError, subprocess.CalledProcessError) as error:
        print(f"ERROR: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
