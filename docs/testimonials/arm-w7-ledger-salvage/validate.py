#!/usr/bin/env python3
"""Capture and verify the local ARM W7 focused mutation testimonial.

The protocol detects accidental, stale, or mismatched evidence.  It is not a
cryptographic attestation and does not defend against a malicious local author.
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
import signal
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
RUN_TEMP = PROTOCOL["temp_root"]
RAW_RUN_RECEIPT = pathlib.Path(RUN_OUTPUT) / "run-receipt.raw.json"
MONITOR_INTERVAL_SECONDS = 1.0
SUPPORTED_PLATFORMS = {"darwin", "linux"}
HANDOFF_SIGNALS = {signal.SIGINT, signal.SIGTERM}
HANDOFF_SIGNAL_NAMES = sorted(item.name for item in HANDOFF_SIGNALS)
SIGNAL_GUARD_POLICY = PROTOCOL["signal_guard"]
RUN_ENV_ALLOW = PROTOCOL["run_env_allow"]
RUN_ENV_FORBIDDEN = PROTOCOL["run_env_forbidden"]
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


SIGNAL_GUARD_ERRORS = (
    EvidenceError,
    OSError,
    RuntimeError,
    ValueError,
    KeyboardInterrupt,
    SystemExit,
)


def require(condition: bool, message: str) -> None:
    if not condition:
        raise EvidenceError(message)


def require_supported_platform() -> None:
    require(
        os.name == "posix"
        and sys.platform in SUPPORTED_PLATFORMS
        and hasattr(signal, "pthread_sigmask"),
        "mutation runner requires macOS or Linux process-group semantics",
    )


def canonical_bound_path(
    path: pathlib.Path, *, must_exist: bool, directory: bool = False
) -> pathlib.Path:
    """Resolve aliases while rejecting a symlink at the controlled leaf."""
    if must_exist:
        require(os.path.lexists(path), f"bound path is absent: {path}")
        metadata = os.lstat(path)
        require(not stat.S_ISLNK(metadata.st_mode), f"bound path is a symlink: {path}")
        if directory:
            require(stat.S_ISDIR(metadata.st_mode), f"bound path is not a directory: {path}")
        return path.resolve(strict=True)
    require(not os.path.lexists(path), f"reserved path already exists: {path}")
    return path.parent.resolve(strict=True) / path.name


def require_same_bound_path(
    actual: pathlib.Path,
    expected: pathlib.Path,
    *,
    must_exist: bool,
    directory: bool = False,
) -> pathlib.Path:
    actual_real = canonical_bound_path(
        actual, must_exist=must_exist, directory=directory
    )
    expected_real = canonical_bound_path(
        expected, must_exist=must_exist, directory=directory
    )
    require(actual_real == expected_real, f"bound path mismatch: {actual}")
    return actual_real


def require_direct_scratch_path(path: pathlib.Path, *, must_exist: bool) -> None:
    resolved = canonical_bound_path(path, must_exist=must_exist, directory=must_exist)
    scratch = pathlib.Path("/tmp").resolve(strict=True)
    require(resolved.parent == scratch, f"scratch path is not a direct /tmp child: {path}")


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
        if (
            path
            in {
                "Cargo.lock",
                "Cargo.toml",
                "rust-toolchain.toml",
                "crates/nika-cadence/Cargo.toml",
            }
            or path == ".cargo/config"
            or path.startswith(".cargo/")
            and path.endswith(".toml")
            or path == "crates/nika-cadence/build.rs"
            or path.startswith("crates/nika-cadence/src/")
        ):
            selected.append(path)
    return sorted(selected)


def input_records(repo: pathlib.Path, commit: str) -> list[dict[str, str]]:
    records = []
    for path in input_paths(repo, commit):
        blob = git(repo, "rev-parse", f"{commit}:{path}").strip()
        data = git(repo, "show", f"{commit}:{path}", text=False)
        records.append({"path": path, "git_blob": blob, "sha256": sha256_bytes(data)})
    return records


def version(
    argv: list[str], environment: dict[str, str] | None, cwd: pathlib.Path
) -> str:
    return subprocess.check_output(
        argv, text=True, env=environment, cwd=cwd
    ).strip()


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


def require_run_identity(run_id: str, output: str, target: str, temp_root: str) -> None:
    require(
        re.fullmatch(r"arm-w7-[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}", run_id) is not None,
        "run_id is not a unique UUIDv4 ARM W7 identity",
    )
    require(run_id == RUN_ID, "run_id drift")
    require(output == RUN_OUTPUT and run_id in output, "output path/run_id mismatch")
    require(target == RUN_TARGET and run_id in target, "target path/run_id mismatch")
    require(temp_root == RUN_TEMP and run_id in temp_root, "temp path/run_id mismatch")
    require(len({output, target, temp_root}) == 3, "run scratch paths collide")


def require_run_id_absent_from_tested(repo: pathlib.Path) -> None:
    probe = subprocess.run(
        ["git", "grep", "-F", "--quiet", RUN_ID, TESTED_COMMIT],
        cwd=repo,
        check=False,
    )
    require(probe.returncode == 1, "run_id already exists in the tested tree or grep failed")


def probe_paths_absent() -> dict[str, Any]:
    checked_at = dt.datetime.now(dt.timezone.utc).isoformat().replace("+00:00", "Z")
    output = pathlib.Path(RUN_OUTPUT)
    target = pathlib.Path(RUN_TARGET)
    temp_root = pathlib.Path(RUN_TEMP)
    for path in (output, target, temp_root):
        require_direct_scratch_path(path, must_exist=False)
        require_same_bound_path(path, path, must_exist=False)
    result = {
        "checked_at": checked_at,
        "output": {"path": RUN_OUTPUT, "exists": os.path.lexists(output)},
        "target": {"path": RUN_TARGET, "exists": os.path.lexists(target)},
        "temp_root": {"path": RUN_TEMP, "exists": os.path.lexists(temp_root)},
    }
    require(result["output"]["exists"] is False, "reserved output path already exists")
    require(result["target"]["exists"] is False, "reserved target path already exists")
    require(result["temp_root"]["exists"] is False, "reserved temp root already exists")
    return result


def capture_start(repo: pathlib.Path, raw_census: pathlib.Path) -> None:
    require_supported_platform()
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
    environment = build_run_environment(dict(os.environ))
    tools = tool_binaries(environment, pathlib.Path(RUN_WORKTREE))
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
        "tools": tool_fingerprints(tools)
        | {
            "python": {
                "version": version(
                    ["python3", "--version"], environment, pathlib.Path(RUN_WORKTREE)
                )
            }
        },
        "run": {
            "worktree": RUN_WORKTREE,
            "cwd": RUN_WORKTREE,
            "output_dir": RUN_OUTPUT,
            "temp_root": RUN_TEMP,
            "platform_policy": sorted(SUPPORTED_PLATFORMS),
            "signal_policy": HANDOFF_SIGNAL_NAMES,
            "signal_guard": SIGNAL_GUARD_POLICY,
            "environment": {"CARGO_TARGET_DIR": RUN_TARGET, "TMPDIR": RUN_TEMP},
            "environment_hashes": relevant_environment(environment),
            "cargo_config": cargo_config_binding(repo, TESTED_COMMIT, environment),
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
            ".cargo/config",
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
        "/" + "Volumes" + "/",
        "/" + "private" + "/",
    ]
    windows_user_path = re.compile(
        r"[A-Za-z]:[\\/](?:Users|Documents and Settings)[\\/]"
    )
    credential_patterns = [
        re.compile(
            r"(?i)\b(?:token|api[_-]?key|secret)[\"'\s:=]+"
            r"[A-Za-z0-9_./+=-]{12,}"
        ),
        re.compile(r"\b" + "s" + "k-" + r"[A-Za-z0-9_-]{12,}"),
    ]
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
        start["run"]["temp_root"],
    )
    require_run_id_absent_from_tested(repo)
    require(start["run"]["argv"] == RUN_ARGV, "run argv drift")
    require(start["run"]["wrapper_argv"] == RUNNER_ARGV, "atomic wrapper argv drift")
    require(
        start["run"]["environment"]
        == {"CARGO_TARGET_DIR": RUN_TARGET, "TMPDIR": RUN_TEMP},
        "run scratch environment drift",
    )
    require(start["run"]["worktree"] == RUN_WORKTREE, "run worktree drift")
    require(
        start["run"]["platform_policy"] == sorted(SUPPORTED_PLATFORMS),
        "runner platform policy drift",
    )
    require(
        start["run"]["signal_policy"] == HANDOFF_SIGNAL_NAMES,
        "runner signal policy drift",
    )
    require(
        start["run"]["signal_guard"] == SIGNAL_GUARD_POLICY,
        "runner signal guard drift",
    )
    require_probe_binding(start["run"]["detached_probe"])
    require_environment_records(start["run"]["environment_hashes"])
    require_cargo_config_binding(start["run"]["cargo_config"])
    require_tool_fingerprints(start["tools"])
    absence = start["run"]["path_absence_probe"]
    require(absence["output"] == {"path": RUN_OUTPUT, "exists": False}, "recorded output absence drift")
    require(absence["target"] == {"path": RUN_TARGET, "exists": False}, "recorded target absence drift")
    require(
        absence["temp_root"] == {"path": RUN_TEMP, "exists": False},
        "recorded temp root absence drift",
    )
    parse_timestamp(absence["checked_at"])
    if require_paths_absent:
        require(
            start["run"]["detached_probe"] == probe_run_worktree(),
            "detached worktree probe drift",
        )
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


def build_run_environment(parent: dict[str, str]) -> dict[str, str]:
    require(len(RUN_ENV_ALLOW) == len(set(RUN_ENV_ALLOW)), "duplicate run env key")
    risky = re.compile(r"(?i)(?:token|api[_-]?key|secret|password|provider)")
    for name in RUN_ENV_ALLOW:
        require(name not in RUN_ENV_FORBIDDEN, f"forbidden run env key allowed: {name}")
        require(not name.startswith("SCCACHE"), f"sccache run env key allowed: {name}")
        require(risky.search(name) is None, f"secret-shaped run env key allowed: {name}")
    environment = {name: parent[name] for name in RUN_ENV_ALLOW if name in parent}
    environment["CARGO_TARGET_DIR"] = RUN_TARGET
    environment["TMPDIR"] = RUN_TEMP
    require("PATH" in environment, "normalized run environment has no PATH")
    return environment


def launcher_file_record(executable: str, environment: dict[str, str]) -> dict[str, Any]:
    invoked = shutil.which(executable, path=environment["PATH"])
    require(invoked is not None, f"{executable} binary is not on PATH")
    invoked_path = pathlib.Path(invoked)
    require(invoked_path.is_absolute(), f"{executable} PATH result is not absolute")
    invoked_metadata = os.lstat(invoked_path)
    is_symlink = stat.S_ISLNK(invoked_metadata.st_mode)
    require(
        is_symlink or stat.S_ISREG(invoked_metadata.st_mode),
        f"{executable} launcher is neither regular nor symlink",
    )
    proxy = invoked_path.resolve(strict=True)
    require(stat.S_ISREG(os.lstat(proxy).st_mode), f"{executable} proxy is not regular")
    return {
        "invoked_path": invoked,
        "invoked_path_sha256": sha256_bytes(invoked.encode()),
        "invoked_kind": "symlink" if is_symlink else "regular",
        "invoked_size": invoked_metadata.st_size,
        "symlink_target": os.readlink(invoked_path) if is_symlink else None,
        "symlink_target_sha256": sha256_bytes(os.readlink(invoked_path).encode())
        if is_symlink
        else None,
        "proxy_realpath": str(proxy),
        "proxy_sha256": sha256_file(proxy),
    }


def require_tool_command_cwd(cwd: pathlib.Path) -> tuple[pathlib.Path, dict[str, str]]:
    canonical = require_same_bound_path(
        cwd,
        pathlib.Path(RUN_WORKTREE),
        must_exist=True,
        directory=True,
    )
    return canonical, {
        "logical": RUN_WORKTREE,
        "canonical_sha256": sha256_bytes(str(canonical).encode()),
    }


def launcher_record(
    executable: str,
    environment: dict[str, str],
    command_cwd: pathlib.Path,
    cwd_binding: dict[str, str],
    rustup: dict[str, Any] | None = None,
) -> dict[str, Any]:
    record = launcher_file_record(executable, environment)
    invoked = record["invoked_path"]
    version_args = ["mutants", "--version"] if executable == "cargo-mutants" else ["--version"]
    record |= {
        "command_cwd": cwd_binding,
        "proxy_version": version(
            [invoked, *version_args], environment, command_cwd
        ),
        "selected_toolchain": None,
    }
    if executable in {"cargo", "rustc"}:
        require(rustup is not None, "rustup launcher binding is absent")
        require(
            record["proxy_realpath"] == rustup["proxy_realpath"]
            and record["proxy_sha256"] == rustup["proxy_sha256"],
            f"{executable} launcher is not the bound rustup proxy",
        )
        selected = pathlib.Path(
            version(
                [rustup["invoked_path"], "which", executable],
                environment,
                command_cwd,
            )
        ).resolve(strict=True)
        require(stat.S_ISREG(os.lstat(selected).st_mode), f"selected {executable} is not regular")
        record["selected_toolchain"] = {
            "path": str(selected),
            "sha256": sha256_file(selected),
            "version": version(
                [str(selected), "--version"], environment, command_cwd
            ),
            "selection_launcher_sha256": rustup["invoked_path_sha256"],
        }
    return record


def tool_binaries(
    environment: dict[str, str], cwd: pathlib.Path
) -> dict[str, dict[str, Any]]:
    command_cwd, cwd_binding = require_tool_command_cwd(cwd)
    rustup = launcher_record(
        "rustup", environment, command_cwd, cwd_binding
    )
    return {
        "cargo_mutants": launcher_record(
            "cargo-mutants", environment, command_cwd, cwd_binding
        ),
        "rustup": rustup,
        "cargo": launcher_record(
            "cargo", environment, command_cwd, cwd_binding, rustup
        ),
        "rustc": launcher_record(
            "rustc", environment, command_cwd, cwd_binding, rustup
        ),
    }


def tool_fingerprints(tools: dict[str, dict[str, Any]]) -> dict[str, Any]:
    fingerprints = {}
    for name, record in tools.items():
        selected = record["selected_toolchain"]
        fingerprints[name] = {
            "invoked_path_sha256": record["invoked_path_sha256"],
            "invoked_kind": record["invoked_kind"],
            "invoked_size": record["invoked_size"],
            "symlink_target_sha256": record["symlink_target_sha256"],
            "command_cwd": record["command_cwd"],
            "proxy_sha256": record["proxy_sha256"],
            "proxy_version": record["proxy_version"],
            "selected_sha256": selected["sha256"] if selected else None,
            "selected_version": selected["version"] if selected else None,
            "selection_launcher_sha256": selected["selection_launcher_sha256"]
            if selected
            else None,
        }
    return fingerprints


def require_sha256(value: Any, message: str) -> None:
    require(
        isinstance(value, str) and re.fullmatch(r"[0-9a-f]{64}", value) is not None,
        message,
    )


def require_tool_fingerprints(fingerprints: dict[str, Any]) -> None:
    require(
        set(fingerprints)
        == {"cargo_mutants", "rustup", "cargo", "rustc", "python"},
        "start tool fingerprint set drift",
    )
    for name in ["cargo_mutants", "rustup", "cargo", "rustc"]:
        record = fingerprints[name]
        require_sha256(record["invoked_path_sha256"], f"{name} path hash drift")
        require(record["invoked_kind"] in {"regular", "symlink"}, f"{name} kind drift")
        require(isinstance(record["invoked_size"], int), f"{name} size drift")
        if record["invoked_kind"] == "symlink":
            require_sha256(
                record["symlink_target_sha256"], f"{name} symlink target drift"
            )
        else:
            require(
                record["symlink_target_sha256"] is None,
                f"{name} regular launcher has symlink target",
            )
        require_sha256(record["proxy_sha256"], f"{name} proxy hash drift")
        require(bool(record["proxy_version"]), f"{name} proxy version is empty")
        require(
            record["command_cwd"]["logical"] == RUN_WORKTREE,
            f"{name} command cwd drift",
        )
        require_sha256(
            record["command_cwd"]["canonical_sha256"],
            f"{name} canonical command cwd drift",
        )
        if name in {"cargo_mutants", "rustup"}:
            require(record["selected_sha256"] is None, f"{name} selected tool drift")
            require(record["selected_version"] is None, f"{name} selected version drift")
            require(
                record["selection_launcher_sha256"] is None,
                f"{name} selection launcher drift",
            )
        else:
            require_sha256(record["selected_sha256"], f"{name} selected hash drift")
            require(bool(record["selected_version"]), f"{name} selected version empty")
            require_sha256(
                record["selection_launcher_sha256"],
                f"{name} selection launcher drift",
            )
    require(bool(fingerprints["python"]["version"]), "python version is empty")


def relevant_environment(environment: dict[str, str]) -> list[dict[str, Any]]:
    records = []
    for name in RUN_ENV_ALLOW:
        value = environment.get(name)
        records.append(
            {
                "name": name,
                "present": value is not None,
                "value_sha256": sha256_bytes(value.encode()) if value is not None else None,
            }
        )
    return records


def require_environment_records(records: list[dict[str, Any]]) -> None:
    require([item["name"] for item in records] == RUN_ENV_ALLOW, "environment key set drift")
    for item in records:
        require(isinstance(item["present"], bool), "environment presence flag drift")
        if item["present"]:
            require_sha256(item["value_sha256"], "environment value hash drift")
        else:
            require(item["value_sha256"] is None, "absent environment has a hash")


def select_cargo_config(root: pathlib.Path, *, nested: bool) -> tuple[pathlib.Path | None, bool]:
    base = root / ".cargo" if nested else root
    legacy = base / "config"
    toml = base / "config.toml"
    legacy_present = os.path.lexists(legacy)
    toml_present = os.path.lexists(toml)
    selected = legacy if legacy_present else toml if toml_present else None
    return selected, legacy_present and toml_present


def regular_config_hash(config: pathlib.Path) -> str:
    metadata = os.lstat(config)
    require(not stat.S_ISLNK(metadata.st_mode), f"Cargo config is a symlink: {config}")
    require(stat.S_ISREG(metadata.st_mode), f"Cargo config is not regular: {config}")
    return sha256_file(config)


def cargo_user_config_record(environment: dict[str, str]) -> dict[str, Any]:
    if "CARGO_HOME" in environment:
        cargo_home = pathlib.Path(environment["CARGO_HOME"])
        source = "CARGO_HOME"
    else:
        require("HOME" in environment, "normalized env has no Cargo home source")
        cargo_home = pathlib.Path(environment["HOME"]) / ".cargo"
        source = "HOME/.cargo"
    config, shadowed = select_cargo_config(cargo_home, nested=False)
    if config is None:
        return {"source": source, "name": None, "sha256": None, "shadowed": False}
    return {
        "source": source,
        "name": config.name,
        "sha256": regular_config_hash(config),
        "shadowed": shadowed,
    }


def cargo_workspace_config_record(repo: pathlib.Path, commit: str) -> dict[str, Any]:
    worktree = canonical_bound_path(
        pathlib.Path(RUN_WORKTREE), must_exist=True, directory=True
    )
    config, shadowed = select_cargo_config(worktree, nested=True)
    if config is None:
        return {"source": "tested_worktree", "path": None, "git_blob": None, "sha256": None, "shadowed": False}
    relative = str(config.relative_to(worktree))
    blob = git(repo, "rev-parse", f"{commit}:{relative}").strip()
    data = git(repo, "show", f"{commit}:{relative}", text=False)
    digest = sha256_bytes(data)
    require(regular_config_hash(config) == digest, "worktree Cargo config differs from tested blob")
    return {
        "source": "tested_worktree",
        "path": relative,
        "git_blob": blob,
        "sha256": digest,
        "shadowed": shadowed,
    }


def reject_external_cargo_configs(cwd: pathlib.Path) -> int:
    canonical = canonical_bound_path(cwd, must_exist=True, directory=True)
    ancestors = list(canonical.parents)
    for ancestor in ancestors:
        config, shadowed = select_cargo_config(ancestor, nested=True)
        require(
            config is None and not shadowed,
            f"external Cargo ancestor config exists: {ancestor}",
        )
    return len(ancestors)


def reject_cargo_hierarchy(root: pathlib.Path, *, must_exist: bool) -> int:
    temp_root = canonical_bound_path(
        root, must_exist=must_exist, directory=must_exist
    )
    scopes = [temp_root, *temp_root.parents]
    for scope in scopes:
        config, shadowed = select_cargo_config(scope, nested=True)
        require(
            config is None and not shadowed,
            f"temporary Cargo hierarchy config exists: {scope}",
        )
    return len(scopes)


def reject_temp_cargo_hierarchy(*, must_exist: bool) -> int:
    return reject_cargo_hierarchy(pathlib.Path(RUN_TEMP), must_exist=must_exist)


def cargo_config_binding(
    repo: pathlib.Path, commit: str, environment: dict[str, str]
) -> dict[str, Any]:
    return {
        "workspace": cargo_workspace_config_record(repo, commit),
        "user": cargo_user_config_record(environment),
        "external_ancestors_checked": reject_external_cargo_configs(
            pathlib.Path(RUN_WORKTREE)
        ),
        "temp_hierarchy_checked": reject_temp_cargo_hierarchy(
            must_exist=os.path.lexists(RUN_TEMP)
        ),
    }


def require_cargo_config_binding(binding: dict[str, Any]) -> None:
    workspace = binding["workspace"]
    require(workspace["source"] == "tested_worktree", "workspace config source drift")
    require(isinstance(workspace["shadowed"], bool), "workspace shadow flag drift")
    if workspace["path"] is None:
        require(workspace["git_blob"] is None, "absent workspace config has a blob")
        require(workspace["sha256"] is None, "absent workspace config has a hash")
    else:
        require(workspace["path"] in {".cargo/config", ".cargo/config.toml"}, "workspace config path drift")
        if workspace["shadowed"]:
            require(workspace["path"] == ".cargo/config", "workspace precedence drift")
        require(re.fullmatch(r"[0-9a-f]{40}", workspace["git_blob"]) is not None, "workspace config blob drift")
        require_sha256(workspace["sha256"], "workspace config hash drift")
    user = binding["user"]
    require(user["source"] in {"CARGO_HOME", "HOME/.cargo"}, "user config source drift")
    require(isinstance(user["shadowed"], bool), "user shadow flag drift")
    if user["name"] is None:
        require(user["sha256"] is None, "absent user config has a hash")
    else:
        require(user["name"] in {"config", "config.toml"}, "user config name drift")
        if user["shadowed"]:
            require(user["name"] == "config", "user config precedence drift")
        require_sha256(user["sha256"], "user config hash drift")
    require(isinstance(binding["external_ancestors_checked"], int), "ancestor count drift")
    require(isinstance(binding["temp_hierarchy_checked"], int), "temp hierarchy count drift")


def copied_workspace_config_probe(expected: dict[str, Any]) -> list[dict[str, str]]:
    root = canonical_bound_path(
        pathlib.Path(RUN_TEMP), must_exist=True, directory=True
    )
    observed = []
    for current, directories, files in os.walk(root, followlinks=False):
        current_path = pathlib.Path(current)
        if ".cargo" in directories:
            cargo_dir = current_path / ".cargo"
            metadata = os.lstat(cargo_dir)
            require(
                stat.S_ISDIR(metadata.st_mode) and not stat.S_ISLNK(metadata.st_mode),
                f"copied workspace Cargo directory is not real: {cargo_dir}",
            )
        if current_path.name != ".cargo":
            continue
        for name in ("config", "config.toml"):
            if name not in files:
                continue
            config = current_path / name
            digest = regular_config_hash(config)
            require(
                expected["path"] is not None
                and name == pathlib.Path(expected["path"]).name
                and digest == expected["sha256"],
                f"copied workspace Cargo config drift: {config}",
            )
            observed.append(
                {
                    "relative_path_sha256": sha256_bytes(
                        str(config.relative_to(root)).encode()
                    ),
                    "sha256": digest,
                }
            )
    return observed


def reserve_run_temp_root() -> dict[str, Any]:
    root = pathlib.Path(RUN_TEMP)
    require(RUN_ID in root.name, "temporary root lacks the exact run identity")
    require(len({RUN_TEMP, RUN_OUTPUT, RUN_TARGET}) == 3, "scratch paths collide")
    require_direct_scratch_path(root, must_exist=False)
    reserved = canonical_bound_path(root, must_exist=False)
    parent = reserved.parent
    require(
        stat.S_ISDIR(os.lstat(parent).st_mode),
        "canonical temporary parent is not a real directory",
    )
    return {
        "path": RUN_TEMP,
        "canonical_path_sha256": sha256_bytes(str(reserved).encode()),
        "canonical_parent_sha256": sha256_bytes(str(parent).encode()),
    }


def create_run_temp_root(
    reservation: dict[str, Any], after_mkdir: Any = None
) -> dict[str, Any]:
    root = pathlib.Path(RUN_TEMP)
    require(reservation == reserve_run_temp_root(), "temporary reservation drift")
    root.mkdir(mode=0o700)
    os.chmod(root, 0o700, follow_symlinks=False)
    if after_mkdir is not None:
        after_mkdir()
    canonical = require_same_bound_path(
        root, pathlib.Path(RUN_TEMP), must_exist=True, directory=True
    )
    require(
        sha256_bytes(str(canonical).encode())
        == reservation["canonical_path_sha256"],
        "created temporary root escaped its reservation",
    )
    mode = stat.S_IMODE(os.lstat(root).st_mode)
    require(mode == 0o700, "temporary root mode is not 0700")
    return reservation | {
        "mode": "0700",
        "cargo_hierarchy_checked": reject_temp_cargo_hierarchy(must_exist=True),
    }


def cleanup_run_temp_root(reservation: dict[str, Any]) -> dict[str, Any]:
    root = pathlib.Path(RUN_TEMP)
    require(root.name.endswith("-tmp") and RUN_ID in root.name, "temp cleanup identity drift")
    require_direct_scratch_path(root, must_exist=True)
    canonical = require_same_bound_path(
        root, pathlib.Path(RUN_TEMP), must_exist=True, directory=True
    )
    require(
        sha256_bytes(str(canonical).encode())
        == reservation["canonical_path_sha256"],
        "temporary root identity changed before cleanup",
    )
    metadata = os.lstat(root)
    require(stat.S_ISDIR(metadata.st_mode), "temporary root is not a real directory")
    require(metadata.st_uid == os.getuid(), "temporary root owner changed")
    mode = stat.S_IMODE(metadata.st_mode)
    require(mode & 0o077 == 0, "temporary root gained group or other permissions")
    os.chmod(root, 0o700, follow_symlinks=False)
    require(shutil.rmtree.avoids_symlink_attacks, "fd-safe recursive cleanup unavailable")
    for preserved in (pathlib.Path(RUN_OUTPUT), pathlib.Path(RUN_TARGET)):
        destination = preserved.parent.resolve(strict=True) / preserved.name
        require(
            destination != canonical and not destination.is_relative_to(canonical),
            "temp cleanup overlaps a preserved artifact path",
        )
    hierarchy = reject_temp_cargo_hierarchy(must_exist=True)
    removed_entries = sum(len(directories) + len(files) for _, directories, files in os.walk(root, followlinks=False))
    shutil.rmtree(root)
    require(not os.path.lexists(root), "temporary root cleanup failed")
    return {
        "cargo_hierarchy_checked": hierarchy,
        "removed_entries": removed_entries,
        "fd_safe_no_symlink_follow": True,
        "removed": True,
    }


def guarded_temp_root(
    action: Any, after_mkdir: Any = None, before_cleanup: Any = None
) -> tuple[Any, dict[str, Any], dict[str, Any]]:
    require_supported_platform()
    reservation = reserve_run_temp_root()
    pre_spawn = None
    post_process = None
    primary = None
    try:
        pre_spawn = create_run_temp_root(reservation, after_mkdir)
        result = action()
    except SIGNAL_GUARD_ERRORS as error:
        primary = error
        raise
    finally:
        if before_cleanup is not None:
            try:
                before_cleanup()
            except BaseException as cleanup_error:
                if primary is None:
                    raise
                note_secondary(primary, "pre-cleanup signal block failed", cleanup_error)
        if os.path.lexists(RUN_TEMP):
            try:
                post_process = cleanup_run_temp_root(reservation)
            except BaseException as cleanup_error:
                if primary is None:
                    raise
                primary.add_note(
                    f"reserved temp cleanup failed: {type(cleanup_error).__name__}: {cleanup_error}"
                )
    require(pre_spawn is not None and post_process is not None, "temp transaction incomplete")
    return result, pre_spawn, post_process


def require_probe_binding(probe: dict[str, Any]) -> None:
    require(probe["head"] == TESTED_COMMIT, "recorded worktree HEAD drift")
    require(probe["tree"] == TESTED_TREE, "recorded worktree tree drift")
    require(probe["index_tree"] == TESTED_TREE, "recorded worktree index drift")
    require(probe["index_clean"] is True, "recorded index is dirty")
    require(probe["worktree_clean"] is True, "recorded worktree is dirty")
    require(probe["porcelain_v2"] == "", "recorded worktree status is dirty")
    require(probe["detached_head"] is True, "recorded worktree is not detached")
    require(probe["symbolic_ref"] is None, "recorded symbolic ref is present")


def require_sanitized_tools(tools: dict[str, dict[str, Any]]) -> None:
    placeholders = {
        "cargo_mutants": ("<CARGO_MUTANTS_BIN>", "<CARGO_MUTANTS_BIN>", None),
        "rustup": ("<RUSTUP_PROXY_BIN>", "<RUSTUP_PROXY_BIN>", None),
        "cargo": ("<CARGO_BIN>", "<RUSTUP_PROXY_BIN>", "<CARGO_TOOLCHAIN_BIN>"),
        "rustc": ("<RUSTC_BIN>", "<RUSTUP_PROXY_BIN>", "<RUSTC_TOOLCHAIN_BIN>"),
    }
    require(set(tools) == set(placeholders), "committed tool binary set drift")
    for name, (invoked, proxy, selected_path) in placeholders.items():
        record = tools[name]
        require(
            record["invoked_path"] == invoked and record["proxy_realpath"] == proxy,
            f"committed {name} path is not sanitized",
        )
        require_sha256(record["invoked_path_sha256"], f"committed {name} path hash drift")
        require(record["invoked_kind"] in {"regular", "symlink"}, f"committed {name} kind drift")
        require_sha256(record["proxy_sha256"], f"committed {name} proxy hash drift")
        require(bool(record["proxy_version"]), f"committed {name} proxy version empty")
        require(
            record["command_cwd"]["logical"] == RUN_WORKTREE,
            f"committed {name} command cwd drift",
        )
        require_sha256(
            record["command_cwd"]["canonical_sha256"],
            f"committed {name} command cwd hash drift",
        )
        selected = record["selected_toolchain"]
        if name in {"cargo_mutants", "rustup"}:
            require(selected is None, f"{name} has a selected toolchain")
        else:
            require(selected["path"] == selected_path, f"committed {name} selected path drift")
            require_sha256(selected["sha256"], f"committed {name} selected hash drift")
            require(bool(selected["version"]), f"committed {name} selected version empty")
            require(
                selected["selection_launcher_sha256"]
                == tools["rustup"]["invoked_path_sha256"],
                f"committed {name} rustup selection binding drift",
            )
            require(
                record["proxy_realpath"] == tools["rustup"]["proxy_realpath"]
                and record["proxy_sha256"] == tools["rustup"]["proxy_sha256"],
                f"committed {name} rustup proxy relation drift",
            )


def require_committed_runtime_binding(
    start: dict[str, Any], run_receipt: dict[str, Any]
) -> None:
    """Check committed hashes without consulting the verifier's host."""
    tools = run_receipt["tool_binaries"]
    require_sanitized_tools(tools)
    require(
        tool_fingerprints(tools)
        == {name: value for name, value in start["tools"].items() if name != "python"},
        "committed tool fingerprints drift",
    )
    environment = run_receipt["environment"]
    require_environment_records(environment)
    require(
        environment == start["run"]["environment_hashes"],
        "committed environment binding drift",
    )
    config = run_receipt["cargo_config"]
    require_cargo_config_binding(config)
    require(
        config == start["run"]["cargo_config"],
        "committed Cargo config binding drift",
    )


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
    require(
        isinstance(monitoring["copied_workspace_configs"], list),
        "copied workspace config observations drift",
    )
    for config in monitoring["copied_workspace_configs"]:
        require_sha256(
            config["relative_path_sha256"], "copied Cargo config path hash drift"
        )
        require_sha256(config["sha256"], "copied Cargo config hash drift")


def bound_process_argv(tools: dict[str, dict[str, Any]]) -> list[str]:
    require(RUN_ARGV[0] == "<CARGO_BIN>", "run argv lacks Cargo placeholder")
    return [tools["cargo"]["invoked_path"], *RUN_ARGV[1:]]


def process_group_exists(process_group: int) -> bool:
    try:
        os.killpg(process_group, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return False
    return True


def signal_process_group(process_group: int, requested: signal.Signals) -> None:
    try:
        os.killpg(process_group, requested)
    except ProcessLookupError:
        pass
    except PermissionError:
        pass


def wait_process_group(process: subprocess.Popen[Any], timeout: float) -> bool:
    deadline = time.monotonic() + timeout
    while process_group_exists(process.pid) and time.monotonic() < deadline:
        process.poll()
        time.sleep(0.05)
    process.poll()
    return not process_group_exists(process.pid)


def stop_process(process: subprocess.Popen[Any]) -> int:
    if process_group_exists(process.pid):
        signal_process_group(process.pid, signal.SIGTERM)
        if not wait_process_group(process, 10):
            signal_process_group(process.pid, signal.SIGKILL)
            require(
                wait_process_group(process, 10),
                "cargo-mutants process group survived SIGKILL",
            )
    if process.returncode is None:
        process.wait(timeout=10)
    require(not process_group_exists(process.pid), "cargo-mutants descendants remain")
    return int(process.returncode)


def controlled_handoff_signal(received: int, _frame: Any) -> None:
    name = signal.Signals(received).name
    raise EvidenceError(f"received supported runner signal during handoff: {name}")


def note_secondary(primary: BaseException, label: str, error: BaseException) -> None:
    primary.add_note(f"{label}: {type(error).__name__}: {error}")


def stop_owned_process(
    process: subprocess.Popen[Any] | None, primary: BaseException | None
) -> BaseException | None:
    if process is None:
        return None
    try:
        stop_process(process)
    except (
        EvidenceError,
        subprocess.SubprocessError,
        OSError,
        KeyboardInterrupt,
        SystemExit,
    ) as error:
        if primary is not None:
            note_secondary(primary, "process-group stop failed", error)
            return None
        return error
    return None


def begin_signal_guard() -> dict[str, Any]:
    old_mask = signal.pthread_sigmask(signal.SIG_BLOCK, HANDOFF_SIGNALS)
    old_handlers: dict[signal.Signals, Any] = {}
    try:
        for item in sorted(HANDOFF_SIGNALS):
            old_handlers[item] = signal.getsignal(item)
            signal.signal(item, controlled_handoff_signal)
    except BaseException:
        for item, previous in old_handlers.items():
            signal.signal(item, previous)
        signal.pthread_sigmask(signal.SIG_SETMASK, old_mask)
        raise
    return {"old_mask": old_mask, "old_handlers": old_handlers}


def block_guard_signals(_guard: dict[str, Any]) -> None:
    signal.pthread_sigmask(signal.SIG_BLOCK, HANDOFF_SIGNALS)


def unblock_guard_signals(guard: dict[str, Any]) -> None:
    signal.pthread_sigmask(signal.SIG_SETMASK, guard["old_mask"])


def finish_signal_guard(
    guard: dict[str, Any], primary: BaseException | None
) -> None:
    secondary = None
    try:
        block_guard_signals(guard)
        for item, previous in guard["old_handlers"].items():
            signal.signal(item, previous)
    except SIGNAL_GUARD_ERRORS as error:
        if primary is not None:
            note_secondary(primary, "signal-state restore failed", error)
        else:
            secondary = error
    try:
        signal.pthread_sigmask(signal.SIG_SETMASK, guard["old_mask"])
    except SIGNAL_GUARD_ERRORS as error:
        if primary is not None:
            note_secondary(primary, "signal-mask restore failed", error)
        elif secondary is not None:
            note_secondary(secondary, "signal-mask restore failed", error)
        else:
            secondary = error
    if primary is None and secondary is not None:
        raise secondary


def guarded_signal_temp_root(
    action: Any, after_mkdir: Any = None
) -> tuple[Any, dict[str, Any], dict[str, Any]]:
    guard = begin_signal_guard()
    primary = None
    try:
        unblock_guard_signals(guard)
        return guarded_temp_root(
            lambda: action(guard),
            after_mkdir,
            lambda: block_guard_signals(guard),
        )
    except BaseException as error:
        primary = error
        raise
    finally:
        finish_signal_guard(guard, primary)


def monitor_process(
    process: subprocess.Popen[Any], workspace_config: dict[str, Any]
) -> dict[str, Any]:
    probe_count = 0
    violation = None
    copied_configs: dict[str, dict[str, str]] = {}
    while True:
        returncode = process.poll()
        probe = quick_worktree_probe()
        for config in copied_workspace_config_probe(workspace_config):
            copied_configs[config["relative_path_sha256"]] = config
        probe_count += 1
        if not probe["clean"]:
            violation = probe
            break
        if returncode is not None:
            break
        time.sleep(MONITOR_INTERVAL_SECONDS)
    monitoring = {
        "interval_ms": int(MONITOR_INTERVAL_SECONDS * 1000),
        "probe_count": probe_count,
        "violation": violation,
        "copied_workspace_configs": sorted(
            copied_configs.values(), key=lambda item: item["relative_path_sha256"]
        ),
    }
    if workspace_config["path"] is not None:
        require(copied_configs, "copied workspace Cargo config was never observed")
    return monitoring


def owned_process(
    argv: list[str],
    cwd: pathlib.Path,
    environment: dict[str, str],
    workspace_config: dict[str, Any],
    guard: dict[str, Any],
    post_spawn: Any = None,
) -> tuple[int, dict[str, Any], int]:
    process = None
    primary = None
    monitoring = None
    secondary = None
    child_mask = guard["old_mask"] - HANDOFF_SIGNALS

    def restore_child_mask() -> None:
        signal.pthread_sigmask(signal.SIG_SETMASK, child_mask)

    try:
        block_guard_signals(guard)
        # POSIX-only, single-threaded runner: undo the inherited block before exec.
        process = subprocess.Popen(
            argv,
            cwd=cwd,
            env=environment,
            start_new_session=True,
            preexec_fn=restore_child_mask,  # noqa: PLW1509
        )
        if post_spawn is not None:
            post_spawn(process)
        unblock_guard_signals(guard)
        monitoring = monitor_process(process, workspace_config)
    except BaseException as error:
        primary = error
        raise
    finally:
        try:
            block_guard_signals(guard)
        except SIGNAL_GUARD_ERRORS as error:
            if primary is not None:
                note_secondary(primary, "pre-stop signal block failed", error)
            else:
                secondary = error
        stopped = stop_owned_process(process, primary or secondary)
        if secondary is None:
            secondary = stopped
        elif stopped is not None:
            note_secondary(secondary, "process-group stop failed", stopped)
        if primary is None and secondary is not None:
            raise secondary
    require(process is not None and monitoring is not None, "process ownership incomplete")
    return int(process.returncode), monitoring, process.pid


def run_process(
    environment: dict[str, str],
    expected_tools: dict[str, dict[str, Any]],
    workspace_config: dict[str, Any],
    guard: dict[str, Any],
) -> tuple[int, dict[str, Any], list[str], int]:
    require_supported_platform()
    current_tools = tool_binaries(environment, pathlib.Path(RUN_WORKTREE))
    require(current_tools == expected_tools, "launcher changed immediately before spawn")
    require(
        reject_temp_cargo_hierarchy(must_exist=True)
        >= 1,
        "temporary Cargo hierarchy was not checked before spawn",
    )
    argv = bound_process_argv(current_tools)
    returncode, monitoring, process_group = owned_process(
        argv,
        require_tool_command_cwd(pathlib.Path(RUN_WORKTREE))[0],
        environment,
        workspace_config,
        guard,
    )
    return returncode, monitoring, argv, process_group


def timed_mutation_process(
    environment: dict[str, str],
    tools: dict[str, dict[str, Any]],
    workspace_config: dict[str, Any],
    guard: dict[str, Any],
) -> tuple[str, int, int, dict[str, Any], list[str], int, int, str]:
    wall_start = now_utc()
    monotonic_start = time.monotonic_ns()
    returncode, monitoring, process_argv, process_group = run_process(
        environment, tools, workspace_config, guard
    )
    return (
        wall_start,
        monotonic_start,
        returncode,
        monitoring,
        process_argv,
        process_group,
        time.monotonic_ns(),
        now_utc(),
    )


def artifact_record(
    output: pathlib.Path,
    artifact: pathlib.Path,
    *,
    allowed: set[str] | None = None,
) -> dict[str, Any]:
    output_real = require_same_bound_path(
        output,
        pathlib.Path(RUN_OUTPUT) if output == pathlib.Path(RUN_OUTPUT) else output,
        must_exist=True,
        directory=True,
    )
    require(os.path.lexists(artifact), f"artifact is absent: {artifact}")
    metadata = os.lstat(artifact)
    require(not stat.S_ISLNK(metadata.st_mode), f"artifact is a symlink: {artifact}")
    require(stat.S_ISREG(metadata.st_mode), f"artifact is not regular: {artifact}")
    real = artifact.resolve(strict=True)
    require(real.is_relative_to(output_real), f"artifact escapes output directory: {artifact}")
    relative = real.relative_to(output_real)
    if allowed is not None:
        require(str(relative) in allowed, f"unexpected artifact path: {relative}")
    cursor = output
    for component in relative.parts[:-1]:
        cursor /= component
        component_metadata = os.lstat(cursor)
        require(
            stat.S_ISDIR(component_metadata.st_mode)
            and not stat.S_ISLNK(component_metadata.st_mode),
            f"artifact parent is not a real directory: {cursor}",
        )
    return {
        "relative_path": str(relative),
        "canonical_path_sha256": sha256_bytes(str(real).encode()),
        "realpath_under_output": True,
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


def raw_artifact_records(
    output: pathlib.Path,
    outcomes: pathlib.Path,
    mutants: pathlib.Path,
) -> dict[str, dict[str, Any]]:
    return {
        "outcomes.json": artifact_record(
            output, outcomes, allowed={"outcomes.json", "mutants.out/outcomes.json"}
        ),
        "mutants.json": artifact_record(
            output, mutants, allowed={"mutants.json", "mutants.out/mutants.json"}
        ),
    }


def run_mutation(repo: pathlib.Path, receipt_commit: str) -> None:
    require_supported_platform()
    receipt = git(repo, "rev-parse", receipt_commit).strip()
    require(git(repo, "rev-parse", "HEAD").strip() == receipt, "runner must execute from receipt HEAD")
    require(
        git(repo, "status", "--porcelain=v2", "--untracked-files=all") == "",
        "receipt worktree is dirty",
    )
    start = verify_start(repo, receipt, require_paths_absent=True)
    pre_probe = probe_run_worktree()
    environment = build_run_environment(dict(os.environ))
    tools = tool_binaries(environment, pathlib.Path(RUN_WORKTREE))
    require(
        tool_fingerprints(tools)
        == {name: value for name, value in start["tools"].items() if name != "python"},
        "runner tool binaries drifted after capture",
    )
    environment_records = relevant_environment(environment)
    require(
        environment_records == start["run"]["environment_hashes"],
        "runner environment drifted after capture",
    )
    require(
        cargo_config_binding(repo, TESTED_COMMIT, environment)
        == start["run"]["cargo_config"],
        "runner Cargo config drifted after capture",
    )
    result, temp_pre, temp_post = guarded_signal_temp_root(
        lambda guard: timed_mutation_process(
            environment,
            tools,
            start["run"]["cargo_config"]["workspace"],
            guard,
        )
    )
    (
        wall_start,
        monotonic_start,
        returncode,
        monitoring,
        process_argv,
        process_group,
        monotonic_end,
        wall_end,
    ) = result
    post_probe = probe_run_worktree()
    output = pathlib.Path(RUN_OUTPUT)
    target = pathlib.Path(RUN_TARGET)
    require_direct_scratch_path(output, must_exist=True)
    require_direct_scratch_path(target, must_exist=True)
    require_same_bound_path(output, pathlib.Path(RUN_OUTPUT), must_exist=True, directory=True)
    require_same_bound_path(target, pathlib.Path(RUN_TARGET), must_exist=True, directory=True)
    raw_outcomes_path, raw_mutants_path = raw_artifacts(output)
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
            "argv": process_argv,
            "cwd": RUN_WORKTREE,
            "platform": sys.platform,
            "signal_policy": HANDOFF_SIGNAL_NAMES,
            "signal_guard": SIGNAL_GUARD_POLICY,
            "new_session": True,
            "process_group": process_group,
            "returncode": returncode,
            "wall_start": wall_start,
            "wall_end": wall_end,
            "monotonic_start_ns": monotonic_start,
            "monotonic_end_ns": monotonic_end,
        },
        "tool_binaries": tools,
        "environment": environment_records,
        "cargo_config": cargo_config_binding(repo, TESTED_COMMIT, environment),
        "temp_root": {"pre_spawn": temp_pre, "post_process": temp_post},
        "pre_probe": pre_probe,
        "monitoring": monitoring,
        "post_probe": post_probe,
        "artifacts": raw_artifact_records(
            output, raw_outcomes_path, raw_mutants_path
        ),
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


def known_replacements(tools: dict[str, dict[str, Any]]) -> dict[str, str]:
    placeholders = {
        "cargo_mutants": ("<CARGO_MUTANTS_BIN>", "<CARGO_MUTANTS_BIN>", None),
        "rustup": ("<RUSTUP_PROXY_BIN>", "<RUSTUP_PROXY_BIN>", None),
        "cargo": ("<CARGO_BIN>", "<RUSTUP_PROXY_BIN>", "<CARGO_TOOLCHAIN_BIN>"),
        "rustc": ("<RUSTC_BIN>", "<RUSTUP_PROXY_BIN>", "<RUSTC_TOOLCHAIN_BIN>"),
    }
    replacements = {}
    for name, (invoked, proxy, selected_path) in placeholders.items():
        record = tools[name]
        replacements[record["invoked_path"]] = invoked
        replacements[record["proxy_realpath"]] = proxy
        selected = record["selected_toolchain"]
        if selected is not None:
            replacements[selected["path"]] = selected_path
        target = record["symlink_target"]
        if target is not None and pathlib.Path(target).is_absolute():
            replacements[target] = invoked
    return replacements


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
    require(
        set(run_receipt)
        == {
            "schema",
            "guarantee",
            "receipt_commit",
            "tested_commit",
            "tested_tree",
            "run_id",
            "wrapper",
            "process",
            "tool_binaries",
            "environment",
            "cargo_config",
            "temp_root",
            "pre_probe",
            "monitoring",
            "post_probe",
            "artifacts",
            "counts",
            "written_at",
        },
        "unexpected raw run receipt field",
    )
    require(run_receipt["schema"] == "nika.mutation-testimonial.run.raw.v1", "wrong raw run receipt schema")
    require(run_receipt["receipt_commit"] == receipt, "raw run receipt commit mismatch")
    require(run_receipt["tested_commit"] == TESTED_COMMIT, "raw run receipt tested commit mismatch")
    require(run_receipt["tested_tree"] == TESTED_TREE, "raw run receipt tested tree mismatch")
    require(run_receipt["run_id"] == RUN_ID, "raw run receipt run_id mismatch")


def require_temp_root_receipt(record: dict[str, Any]) -> None:
    pre = record["pre_spawn"]
    require(pre["path"] == RUN_TEMP, "raw temp root path drift")
    expected = canonical_bound_path(
        pathlib.Path(RUN_TEMP), must_exist=False
    )
    require(
        pre["canonical_path_sha256"] == sha256_bytes(str(expected).encode()),
        "raw temp root hash drift",
    )
    require(
        pre["canonical_parent_sha256"]
        == sha256_bytes(str(expected.parent).encode()),
        "raw temp parent hash drift",
    )
    require(pre["mode"] == "0700", "raw temp root mode drift")
    require(pre["cargo_hierarchy_checked"] >= 1, "raw temp hierarchy unchecked")
    post = record["post_process"]
    require(post["cargo_hierarchy_checked"] >= 1, "raw post temp hierarchy unchecked")
    require(
        isinstance(post["removed_entries"], int) and post["removed_entries"] >= 0,
        "raw removed-entry count drift",
    )
    require(post["fd_safe_no_symlink_follow"] is True, "raw temp cleanup was not fd-safe")
    require(post["removed"] is True, "raw temp root was not removed")


def require_committed_temp_root_receipt(record: dict[str, Any]) -> None:
    pre = record["pre_spawn"]
    require(pre["path"] == RUN_TEMP, "committed temp root path drift")
    require_sha256(
        pre["canonical_path_sha256"], "committed temp root hash drift"
    )
    require_sha256(
        pre["canonical_parent_sha256"], "committed temp parent hash drift"
    )
    require(pre["mode"] == "0700", "committed temp root mode drift")
    require(pre["cargo_hierarchy_checked"] >= 1, "committed temp hierarchy unchecked")
    post = record["post_process"]
    require(post["cargo_hierarchy_checked"] >= 1, "committed post hierarchy unchecked")
    require(
        isinstance(post["removed_entries"], int) and post["removed_entries"] >= 0,
        "committed removed-entry count drift",
    )
    require(
        post["fd_safe_no_symlink_follow"] is True,
        "committed temp cleanup was not fd-safe",
    )
    require(post["removed"] is True, "committed temp root was not removed")


def validate_raw_run_receipt(
    repo: pathlib.Path, receipt: str, output: pathlib.Path
) -> tuple[dict[str, Any], dict[str, Any], list[dict[str, Any]]]:
    require_same_bound_path(
        output, pathlib.Path(RUN_OUTPUT), must_exist=True, directory=True
    )
    require_same_bound_path(
        pathlib.Path(RUN_TARGET),
        pathlib.Path(RUN_TARGET),
        must_exist=True,
        directory=True,
    )
    require_direct_scratch_path(output, must_exist=True)
    require_direct_scratch_path(pathlib.Path(RUN_TARGET), must_exist=True)
    require(not os.path.lexists(RUN_TEMP), "temporary root remains after run")
    artifact_record(
        output, RAW_RUN_RECEIPT, allowed={"run-receipt.raw.json"}
    )
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
    require(process["cwd"] == RUN_WORKTREE, "raw process cwd drift")
    require(process["platform"] in SUPPORTED_PLATFORMS, "raw process platform drift")
    require(process["signal_policy"] == HANDOFF_SIGNAL_NAMES, "raw signal policy drift")
    require(process["signal_guard"] == SIGNAL_GUARD_POLICY, "raw signal guard drift")
    require(process["new_session"] is True, "raw process lacks a new session")
    require(process["process_group"] > 0, "raw process-group identity drift")
    require(process["returncode"] == 0, "raw process did not exit zero")
    require(process["monotonic_end_ns"] > process["monotonic_start_ns"], "raw monotonic duration is not positive")
    environment = build_run_environment(dict(os.environ))
    current_tools = tool_binaries(environment, pathlib.Path(RUN_WORKTREE))
    require(run_receipt["tool_binaries"] == current_tools, "tool binary binding drift")
    require(
        process["argv"] == bound_process_argv(current_tools),
        "raw process launcher drift",
    )
    current_environment = relevant_environment(environment)
    require(
        run_receipt["environment"] == current_environment,
        "relevant environment hash drift",
    )
    start = json.loads((repo / LANE / "start.json").read_text(encoding="utf-8"))
    require(
        tool_fingerprints(current_tools)
        == {name: value for name, value in start["tools"].items() if name != "python"},
        "raw tool fingerprints drifted from start",
    )
    require(
        current_environment == start["run"]["environment_hashes"],
        "raw environment drifted from start",
    )
    current_config = cargo_config_binding(repo, TESTED_COMMIT, environment)
    require(
        run_receipt["cargo_config"] == current_config,
        "raw Cargo config binding drift",
    )
    require(
        current_config == start["run"]["cargo_config"],
        "raw Cargo config drifted from start",
    )
    require_temp_root_receipt(run_receipt["temp_root"])
    require(run_receipt["pre_probe"] == start["run"]["detached_probe"], "raw pre-probe drift")
    require(run_receipt["post_probe"] == probe_run_worktree(), "raw post-probe drift")
    require_monitoring(run_receipt["monitoring"])
    copied_configs = run_receipt["monitoring"]["copied_workspace_configs"]
    workspace_config = current_config["workspace"]
    if workspace_config["path"] is not None:
        require(copied_configs, "raw receipt lacks copied workspace Cargo config")
        require(
            all(item["sha256"] == workspace_config["sha256"] for item in copied_configs),
            "raw copied workspace Cargo config hash drift",
        )
    raw_outcomes_path, raw_mutants_path = raw_artifacts(output)
    expected_artifacts = raw_artifact_records(
        output, raw_outcomes_path, raw_mutants_path
    )
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
    require_same_bound_path(
        output,
        pathlib.Path(start["run"]["output_dir"]),
        must_exist=True,
        directory=True,
    )
    raw_receipt, raw_outcomes, raw_mutants = validate_raw_run_receipt(
        repo, receipt_commit, output
    )
    raw_outcomes_path, raw_mutants_path = raw_artifacts(output)
    replacements = known_replacements(raw_receipt["tool_binaries"])
    replacement_counts: dict[str, int] = {}
    safe_outcomes = sanitize_value(raw_outcomes, replacements, replacement_counts)
    safe_mutants = sanitize_value(raw_mutants, replacements, replacement_counts)
    safe_receipt = sanitize_value(raw_receipt, replacements, replacement_counts)
    require(replacement_counts.get("<CARGO_BIN>", 0) > 0, "cargo path sanitization matched nothing")
    require(replacement_counts.get("<CARGO_MUTANTS_BIN>", 0) > 0, "cargo-mutants path sanitization matched nothing")
    require(replacement_counts.get("<RUSTC_BIN>", 0) > 0, "rustc path sanitization matched nothing")
    for placeholder in [
        "<RUSTUP_PROXY_BIN>",
        "<CARGO_TOOLCHAIN_BIN>",
        "<RUSTC_TOOLCHAIN_BIN>",
    ]:
        require(
            replacement_counts.get(placeholder, 0) > 0,
            f"tool path sanitization matched nothing: {placeholder}",
        )
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
        "claim": "Local reproducible testimonial of a complete serial cargo-mutants run for the ARM W7 firing and ledger surfaces; detects accidental, stale, or mismatched evidence but is neither remote attestation nor protection from malicious local fabrication",
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
            "environment": {"CARGO_TARGET_DIR": RUN_TARGET, "TMPDIR": RUN_TEMP},
            "argv": RUN_ARGV,
        },
        "artifacts": {
            "outcomes.json": {"raw_sha256": sha256_file(raw_outcomes_path), "sanitized_sha256": sha256_file(lane / "outcomes.json")},
            "mutants.json": {"raw_sha256": sha256_file(raw_mutants_path), "sanitized_sha256": sha256_file(lane / "mutants.json")},
            "run-receipt.json": {"raw_sha256": sha256_file(RAW_RUN_RECEIPT), "sanitized_sha256": sha256_file(lane / "run-receipt.json")},
            "census.txt": {"sha256": sha256_file(lane / "census.txt")},
        },
        "sanitization": {
            "algorithm": "recursive replacement of only the bound cargo, cargo-mutants, rustup, rustc, and selected-toolchain executable paths, followed by Python 3 indent=2 UTF-8 serialization and one trailing newline",
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
    require(
        process["platform"] in SUPPORTED_PLATFORMS,
        "committed process platform drift",
    )
    require(
        process["signal_policy"] == HANDOFF_SIGNAL_NAMES,
        "committed signal policy drift",
    )
    require(
        process["signal_guard"] == SIGNAL_GUARD_POLICY,
        "committed signal guard drift",
    )
    require(process["new_session"] is True, "committed run lacks a new session")
    require(process["process_group"] > 0, "committed process-group identity drift")
    require(process["returncode"] == 0, "committed run receipt is nonzero")
    require(parse_timestamp(process["wall_start"]) <= parse_timestamp(outcomes["start_time"]), "committed outcomes predate wrapper")
    require(parse_timestamp(outcomes["end_time"]) <= parse_timestamp(process["wall_end"]), "committed outcomes outlive wrapper")
    require(process["monotonic_end_ns"] > process["monotonic_start_ns"], "committed monotonic duration invalid")
    require_monitoring(run_receipt["monitoring"])
    require_committed_temp_root_receipt(run_receipt["temp_root"])
    workspace_config = start["run"]["cargo_config"]["workspace"]
    copied_configs = run_receipt["monitoring"]["copied_workspace_configs"]
    if workspace_config["path"] is not None:
        require(copied_configs, "committed copied Cargo config proof is absent")
        require(
            all(item["sha256"] == workspace_config["sha256"] for item in copied_configs),
            "committed copied Cargo config hash drift",
        )
    require(run_receipt["pre_probe"] == start["run"]["detached_probe"], "committed pre-probe drift")
    require(
        run_receipt["post_probe"] == start["run"]["detached_probe"],
        "committed post-probe drift",
    )
    require_committed_runtime_binding(start, run_receipt)
    require(run_receipt["counts"] == raw_counts(outcomes, mutants), "committed raw counts drift")
    for name in ["outcomes.json", "mutants.json"]:
        artifact = run_receipt["artifacts"][name]
        require(artifact["regular"] is True and artifact["symlink"] is False, f"committed {name} lstat drift")
        require(
            artifact["realpath_under_output"] is True,
            f"committed {name} path containment drift",
        )
        require_sha256(
            artifact["canonical_path_sha256"],
            f"committed {name} canonical path hash drift",
        )
        require(artifact["sha256"] == manifest["artifacts"][name]["raw_sha256"], f"committed {name} raw hash drift")
    computed = accounting(start, read_names(lane / "census.txt"), mutants, outcomes)
    require(manifest["accounting"] == computed, "manifest accounting drift")
    if raw_output is not None:
        require_same_bound_path(
            raw_output,
            pathlib.Path(start["run"]["output_dir"]),
            must_exist=True,
            directory=True,
        )
        raw_receipt, raw_outcomes, raw_mutants = validate_raw_run_receipt(repo, receipt_commit, raw_output)
        replacements = known_replacements(raw_receipt["tool_binaries"])
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
