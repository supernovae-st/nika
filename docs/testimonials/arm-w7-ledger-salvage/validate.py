#!/usr/bin/env python3
"""Capture and verify the local ARM W7 focused mutation testimonial.

The protocol detects accidental, stale, or mismatched evidence.  It is not a
cryptographic attestation and does not defend against a malicious local author.
"""

from __future__ import annotations

import argparse
import datetime as dt
import fcntl
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
RUN_LEASE = pathlib.Path(PROTOCOL["lease"])
RAW_RUN_RECEIPT = pathlib.Path(RUN_OUTPUT) / "run-receipt.raw.json"
MONITOR_INTERVAL_SECONDS = 1.0
SUPPORTED_PLATFORMS = {"darwin", "linux"}
HANDOFF_SIGNALS = {signal.SIGHUP, signal.SIGINT, signal.SIGTERM}
HANDOFF_SIGNAL_NAMES = sorted(item.name for item in HANDOFF_SIGNALS)
SIGNAL_GUARD_POLICY = PROTOCOL["signal_guard"]
SETTLEMENT_POLICY = PROTOCOL["settlement"]
SETTLEMENT_ORDER = PROTOCOL["settlement_order"]
RUN_ENV_ALLOW = PROTOCOL["run_env_allow"]
RUN_ENV_FORBIDDEN = PROTOCOL["run_env_forbidden"]
EQUIVALENT_EXCLUSIONS = PROTOCOL["equivalent_exclusions"]
RUN_ARGV = PROTOCOL["run_argv"]
LIST_ARGV = PROTOCOL["list_argv"]
RUNNER_ARGV = PROTOCOL["runner_argv"]

APPROVED_EQUIVALENT_NAMES = [
    "crates/nika-cadence/src/ledger.rs:406:27: replace > with >= in unsettled",
    "crates/nika-cadence/src/ledger.rs:1108:21: replace && with || in LifecycleValidator::accept",
    "crates/nika-cadence/src/ledger.rs:1109:21: replace && with || in LifecycleValidator::accept",
]
EXPECTED_EQUIVALENCE_DOMAINS = {
    APPROVED_EQUIVALENT_NAMES[0]: (
        "P=current claim position; L=receipt position",
        "L != P",
        "L > P iff L >= P",
    ),
    APPROVED_EQUIVALENT_NAMES[1]: (
        "A=!seen; B=seq == 1; C=prev_hash == null",
        "rotated reaches accept only at (A,B,C)=(true,true,true)",
        "(A && B) && C equals A || (B && C)",
    ),
    APPROVED_EQUIVALENT_NAMES[2]: (
        "A=!seen; B=seq == 1; C=prev_hash == null",
        "rotated reaches accept only at (A,B,C)=(true,true,true)",
        "(A && B) && C equals (A && B) || C",
    ),
}
EXPECTED_EQUIVALENCE_PREMISES = {
    APPROVED_EQUIVALENT_NAMES[0]: [
        "the claimed branch continues before receipt collection",
        "enumerate assigns one unique position to each immutable line",
    ],
    APPROVED_EQUIVALENT_NAMES[1]: [
        "the only production callers are Walker::fold_chain and scan_chain",
        "both callers run verify_line on the same immutable line before accept",
        "verify_line admits rotated only for sequence 1 with a null predecessor",
        "LifecycleValidator starts unseen and marks every accepted line seen",
    ],
    APPROVED_EQUIVALENT_NAMES[2]: [
        "the only production callers are Walker::fold_chain and scan_chain",
        "both callers run verify_line on the same immutable line before accept",
        "verify_line admits rotated only for sequence 1 with a null predecessor",
        "LifecycleValidator starts unseen and marks every accepted line seen",
    ],
}
EXPECTED_SAME_SPAN_CONTROLS = {
    APPROVED_EQUIVALENT_NAMES[0]: [
        "crates/nika-cadence/src/ledger.rs:406:27: replace > with == in unsettled",
        "crates/nika-cadence/src/ledger.rs:406:27: replace > with < in unsettled",
    ],
    APPROVED_EQUIVALENT_NAMES[1]: [],
    APPROVED_EQUIVALENT_NAMES[2]: [],
}
SAME_SPAN_CONTROLS = [
    control
    for item in EQUIVALENT_EXCLUSIONS
    for control in item["same_span_controls"]
]

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


class LeaseBusy(EvidenceError):
    """A prior execution still owns the durable advisory lease."""


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


def anchored_literal_regex(name: str) -> str:
    """Return the Rust/Python-compatible exact regex for one mutant name."""
    escaped = re.sub(r"([\\.^$*+?{}\[\]|()])", r"\\\1", name)
    return f"^{escaped}$"


def validate_equivalent_exclusions(
    exclusions: Any, census_names: list[str]
) -> None:
    """Reject any exclusion that is not one of the three proved identities."""
    require(isinstance(exclusions, list), "equivalent exclusions must be a list")
    require(
        [item.get("name") for item in exclusions if isinstance(item, dict)]
        == APPROVED_EQUIVALENT_NAMES,
        "equivalent exclusion set or location drift",
    )
    for item in exclusions:
        require(
            set(item) == {"name", "anchored_regex", "proof", "same_span_controls"},
            "equivalent exclusion shape drift",
        )
        name = item["name"]
        require(
            item["anchored_regex"] == anchored_literal_regex(name),
            f"exclusion regex is not exact literal identity: {name}",
        )
        require(census_names.count(name) == 1, f"exclusion cardinality drift: {name}")
        require(
            sum(
                re.fullmatch(item["anchored_regex"], candidate) is not None
                for candidate in census_names
            )
            == 1,
            f"anchored exclusion does not match exactly once: {name}",
        )
        variables, reachable_domain, conclusion = EXPECTED_EQUIVALENCE_DOMAINS[name]
        proof = item["proof"]
        require(
            proof.get("method") == "reachable-domain equivalence"
            and proof.get("variables") == variables
            and proof.get("reachable_domain") == reachable_domain
            and proof.get("conclusion") == conclusion,
            f"production equivalence proof drift: {name}",
        )
        require(
            proof.get("premises") == EXPECTED_EQUIVALENCE_PREMISES[name],
            f"equivalence premises drift: {name}",
        )
        require(
            item["same_span_controls"] == EXPECTED_SAME_SPAN_CONTROLS[name],
            f"same-span controls drift: {name}",
        )


def validate_exclusion_argv(exclusions: list[dict[str, Any]], argv: list[str]) -> None:
    patterns = [argv[index + 1] for index, arg in enumerate(argv[:-1]) if arg == "-E"]
    require(
        patterns == [item["anchored_regex"] for item in exclusions],
        "run argv exclusion set or order drift",
    )


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


def require_direct_scratch_path(
    path: pathlib.Path, *, must_exist: bool, directory: bool = True
) -> None:
    resolved = canonical_bound_path(
        path, must_exist=must_exist, directory=must_exist and directory
    )
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


def require_run_identity(
    run_id: str, output: str, target: str, temp_root: str, lease: str
) -> None:
    require(
        re.fullmatch(r"arm-w7-[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}", run_id) is not None,
        "run_id is not a unique UUIDv4 ARM W7 identity",
    )
    require(run_id == RUN_ID, "run_id drift")
    require(output == RUN_OUTPUT and run_id in output, "output path/run_id mismatch")
    require(target == RUN_TARGET and run_id in target, "target path/run_id mismatch")
    require(temp_root == RUN_TEMP and run_id in temp_root, "temp path/run_id mismatch")
    require(lease == str(RUN_LEASE) and run_id in lease, "lease path/run_id mismatch")
    require(len({output, target, temp_root, lease}) == 4, "run scratch paths collide")


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
    for path in (output, target, temp_root, RUN_LEASE):
        require_direct_scratch_path(path, must_exist=False)
        require_same_bound_path(path, path, must_exist=False)
    result = {
        "checked_at": checked_at,
        "output": {"path": RUN_OUTPUT, "exists": os.path.lexists(output)},
        "target": {"path": RUN_TARGET, "exists": os.path.lexists(target)},
        "temp_root": {"path": RUN_TEMP, "exists": os.path.lexists(temp_root)},
        "lease": {"path": str(RUN_LEASE), "exists": os.path.lexists(RUN_LEASE)},
    }
    require(result["output"]["exists"] is False, "reserved output path already exists")
    require(result["target"]["exists"] is False, "reserved target path already exists")
    require(result["temp_root"]["exists"] is False, "reserved temp root already exists")
    require(result["lease"]["exists"] is False, "reserved execution lease already exists")
    return result


def capture_run_record(repo: pathlib.Path, environment: dict[str, str]) -> dict[str, Any]:
    return {
        "worktree": RUN_WORKTREE,
        "cwd": RUN_WORKTREE,
        "output_dir": RUN_OUTPUT,
        "temp_root": RUN_TEMP,
        "lease": str(RUN_LEASE),
        "platform_policy": sorted(SUPPORTED_PLATFORMS),
        "signal_policy": HANDOFF_SIGNAL_NAMES,
        "signal_guard": SIGNAL_GUARD_POLICY,
        "settlement": SETTLEMENT_POLICY,
        "startup_recovery": PROTOCOL["startup_recovery"],
        "execution_lease": PROTOCOL["execution_lease"],
        "environment": {"CARGO_TARGET_DIR": RUN_TARGET, "TMPDIR": RUN_TEMP},
        "environment_hashes": relevant_environment(environment),
        "cargo_config": cargo_config_binding(repo, TESTED_COMMIT, environment),
        "argv": RUN_ARGV,
        "wrapper_argv": RUNNER_ARGV,
        "detached_probe": probe_run_worktree(),
        "path_absence_probe": probe_paths_absent(),
    }


def equivalence_source_binding(repo: pathlib.Path) -> dict[str, Any]:
    """Bind the three exclusions to the exact production dataflow they preserve."""
    source_path = "crates/nika-cadence/src/ledger.rs"
    source = git(repo, "show", f"{TESTED_COMMIT}:{source_path}")
    fold_pipeline = """match verify_line(line, seq + 1, prev.as_deref()) {
                Some(hash) => {
                    if let Ok(doc) = serde_json::from_str::<serde_json::Value>(line) {
                        if !lifecycle.accept(&doc)"""
    scan_pipeline = """match verify_line(line, seq + 1, prev_hash.as_deref()) {
            Some(hash) => {
                let Ok(doc) = serde_json::from_str::<serde_json::Value>(line) else {
                    break;
                };
                if !lifecycle.accept(&doc)"""
    rotated_guard = """if kind == "rotated" && (expected_seq != 1 || expected_prev.is_some()) {
        return None;
    }"""
    rotated_authority = """"rotated" => {
                !self.seen
                    && doc.get("seq").and_then(serde_json::Value::as_u64) == Some(1)
                    && doc.get("prev_hash").is_some_and(serde_json::Value::is_null)
            }"""
    claim_separation = """claims.push((
                position,
                Unsettled {
                    seq,
                    slot_id,
                    deadline,
                    claimed_at,
                },
            ));
            continue;
        }
        let receipt = ("""
    later_comparison = "later > position && (slot_id, *fencing) == (&claim.slot_id, claim.seq)"
    seen_transition = """if accepted {
            self.seen = true;
        }"""
    for label, excerpt in {
        "fold pipeline": fold_pipeline,
        "scan pipeline": scan_pipeline,
        "rotated envelope guard": rotated_guard,
        "rotated lifecycle authority": rotated_authority,
        "claim/receipt separation": claim_separation,
        "later receipt comparison": later_comparison,
        "accepted lifecycle seen transition": seen_transition,
    }.items():
        require(excerpt in source, f"equivalence source binding drift: {label}")
    all_callers = git(
        repo,
        "grep",
        "-F",
        "-n",
        "lifecycle.accept(",
        TESTED_COMMIT,
        "--",
        "crates/nika-cadence/src",
    ).splitlines()
    callers = [
        line for line in all_callers if line.startswith(f"{TESTED_COMMIT}:{source_path}:")
    ]
    require(len(callers) == 2, "LifecycleValidator production caller set drift")
    require(
        source.count("let mut lifecycle = LifecycleValidator::default();") == 2,
        "LifecycleValidator initialization set drift",
    )
    require(
        all(
            line.startswith(
                (
                    f"{TESTED_COMMIT}:{source_path}:",
                    f"{TESTED_COMMIT}:crates/nika-cadence/src/ledger/tests.rs:",
                )
            )
            for line in all_callers
        ),
        "LifecycleValidator caller escaped production or adjacent tests",
    )
    return {
        "source": source_path,
        "git_blob": git(repo, "rev-parse", f"{TESTED_COMMIT}:{source_path}").strip(),
        "sha256": sha256_bytes(source.encode()),
        "production_callers": ["Walker::fold_chain", "scan_chain"],
        "caller_count": len(callers),
        "rotated_reachable_predicate_tuple": [True, True, True],
        "claim_receipt_position_relation": "not equal",
    }


def capture_start(repo: pathlib.Path, raw_census: pathlib.Path) -> None:
    require_supported_platform()
    require_run_id_absent_from_tested(repo)
    lane = repo / LANE
    raw = json.loads(raw_census.read_text(encoding="utf-8"))
    names = [item["name"] for item in raw]
    require(len(names) == len(set(names)), "census names are not unique")
    validate_equivalent_exclusions(EQUIVALENT_EXCLUSIONS, names)
    validate_exclusion_argv(EQUIVALENT_EXCLUSIONS, RUN_ARGV)
    required = SAME_SPAN_CONTROLS + [item[2] for item in PRIOR_ANOMALIES] + GUARD_MUTANTS
    missing = sorted(set(required) - set(names))
    require(not missing, f"required census identities missing: {missing}")
    census = "".join(f"{name}\n" for name in names)
    (lane / "census.txt").write_text(census, encoding="utf-8")

    now = dt.datetime.now(dt.timezone.utc).isoformat().replace("+00:00", "Z")
    environment = build_run_environment(dict(os.environ))
    tools = tool_binaries(environment, pathlib.Path(RUN_WORKTREE))
    start = {
        "schema": "nika.mutation-testimonial.start.v3",
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
        "run": capture_run_record(repo, environment),
        "census": {
            "argv": LIST_ARGV,
            "artifact": str(LANE / "census.txt"),
            "sha256": sha256_bytes(census.encode()),
            "unfiltered_count": len(names),
            "excluded_count": len(EQUIVALENT_EXCLUSIONS),
            "expected_executed_count": len(names) - len(EQUIVALENT_EXCLUSIONS),
        },
        "equivalent_exclusions": EQUIVALENT_EXCLUSIONS,
        "equivalence_source_binding": equivalence_source_binding(repo),
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


def verify_start_census(repo: pathlib.Path, start: dict[str, Any]) -> None:
    census_path = repo / LANE / "census.txt"
    names = read_names(census_path)
    require(len(names) == len(set(names)), "duplicate census names")
    require(sha256_file(census_path) == start["census"]["sha256"], "census hash drift")
    require(len(names) == start["census"]["unfiltered_count"], "census count drift")
    validate_equivalent_exclusions(start["equivalent_exclusions"], names)
    validate_exclusion_argv(
        start["equivalent_exclusions"], start["run"]["argv"]
    )
    require(
        start["census"]["excluded_count"] == len(EQUIVALENT_EXCLUSIONS),
        "excluded census count drift",
    )
    require(
        start["census"]["expected_executed_count"]
        == len(names) - len(EQUIVALENT_EXCLUSIONS),
        "executed census arithmetic drift",
    )
    required = SAME_SPAN_CONTROLS + [item[2] for item in PRIOR_ANOMALIES] + GUARD_MUTANTS
    require(
        not (set(required) - set(names)),
        "a required reconciliation identity left the census",
    )


def verify_start(
    repo: pathlib.Path, receipt_commit: str, *, require_paths_absent: bool
) -> dict[str, Any]:
    start_path = repo / LANE / "start.json"
    start = json.loads(start_path.read_text(encoding="utf-8"))
    require(start["schema"] == "nika.mutation-testimonial.start.v3", "wrong start schema")
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
        start["run"]["lease"],
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
    require(
        start["run"]["settlement"] == SETTLEMENT_POLICY,
        "runner settlement policy drift",
    )
    require(
        start["run"]["startup_recovery"] == PROTOCOL["startup_recovery"],
        "runner startup recovery policy drift",
    )
    require(
        start["run"]["execution_lease"] == PROTOCOL["execution_lease"],
        "runner execution lease policy drift",
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
    require(
        absence["lease"] == {"path": str(RUN_LEASE), "exists": False},
        "recorded execution lease absence drift",
    )
    parse_timestamp(absence["checked_at"])
    if require_paths_absent:
        require(
            start["run"]["detached_probe"] == probe_run_worktree(),
            "detached worktree probe drift",
        )
        probe_paths_absent()
    require(start["inputs"] == input_records(repo, TESTED_COMMIT), "tested input blob binding drift")
    require(
        start["equivalence_source_binding"] == equivalence_source_binding(repo),
        "equivalence production-source binding drift",
    )
    require(start["validator"]["sha256"] == sha256_file(repo / LANE / "validate.py"), "validator hash drift")
    require(start["selftest"]["sha256"] == sha256_file(repo / LANE / "selftest.py"), "selftest hash drift")
    require(start["protocol"]["sha256"] == sha256_file(repo / LANE / "protocol.json"), "protocol hash drift")

    census_path = repo / LANE / "census.txt"
    verify_start_census(repo, start)
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


def reserve_owned_scratch_roots() -> dict[str, dict[str, Any]]:
    roots = {
        "output": pathlib.Path(RUN_OUTPUT),
        "target": pathlib.Path(RUN_TARGET),
        "temp": pathlib.Path(RUN_TEMP),
    }
    reservations = {}
    canonical = {}
    for kind, root in roots.items():
        require(
            root.name == f"nika-pr1079-{RUN_ID}-{'tmp' if kind == 'temp' else kind}",
            f"{kind} scratch identity drift",
        )
        require_direct_scratch_path(root, must_exist=False)
        resolved = canonical_bound_path(root, must_exist=False)
        canonical[kind] = resolved
        reservations[kind] = {
            "path": str(root),
            "canonical_path_sha256": sha256_bytes(str(resolved).encode()),
            "canonical_parent_sha256": sha256_bytes(str(resolved.parent).encode()),
        }
    require(len(set(canonical.values())) == 3, "scratch reservations overlap")
    return reservations


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


def remove_owned_scratch_root(kind: str) -> dict[str, Any]:
    paths = {
        "output": pathlib.Path(RUN_OUTPUT),
        "target": pathlib.Path(RUN_TARGET),
        "temp": pathlib.Path(RUN_TEMP),
    }
    root = paths[kind]
    require(
        root.name == f"nika-pr1079-{RUN_ID}-{'tmp' if kind == 'temp' else kind}",
        f"{kind} cleanup identity drift",
    )
    require_direct_scratch_path(root, must_exist=True)
    canonical = require_same_bound_path(
        root, paths[kind], must_exist=True, directory=True
    )
    metadata = os.lstat(root)
    require(stat.S_ISDIR(metadata.st_mode), f"{kind} root is not a real directory")
    require(metadata.st_uid == os.getuid(), f"{kind} root owner changed")
    os.chmod(root, 0o700, follow_symlinks=False)
    require(shutil.rmtree.avoids_symlink_attacks, "fd-safe recursive cleanup unavailable")
    for other_kind, other in paths.items():
        if other_kind == kind:
            continue
        destination = canonical_bound_path(
            other, must_exist=os.path.lexists(other), directory=os.path.lexists(other)
        )
        require(
            destination != canonical
            and not destination.is_relative_to(canonical)
            and not canonical.is_relative_to(destination),
            f"{kind} cleanup overlaps {other_kind}",
        )
    removed_entries = sum(len(directories) + len(files) for _, directories, files in os.walk(root, followlinks=False))
    shutil.rmtree(root)
    require(not os.path.lexists(root), f"{kind} root cleanup failed")
    return {
        "removed_entries": removed_entries,
        "fd_safe_no_symlink_follow": True,
        "removed": True,
    }


def lease_payload(receipt: str) -> bytes:
    value = {
        "schema": "nika.mutation-testimonial.lease.v1",
        "run_id": RUN_ID,
        "receipt_commit": receipt,
        "tested_commit": TESTED_COMMIT,
    }
    return (json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n").encode()


def open_lease_candidate() -> tuple[int, dict[str, Any]]:
    require_direct_scratch_path(RUN_LEASE, must_exist=True, directory=False)
    metadata = os.lstat(RUN_LEASE)
    require(not stat.S_ISLNK(metadata.st_mode), "execution lease is a symlink")
    require(stat.S_ISREG(metadata.st_mode), "execution lease is not regular")
    require(metadata.st_uid == os.getuid(), "execution lease owner drift")
    mode = stat.S_IMODE(metadata.st_mode)
    require(mode & 0o077 == 0, "execution lease permissions are ambiguous")
    descriptor = os.open(
        RUN_LEASE,
        os.O_RDWR | getattr(os, "O_NOFOLLOW", 0),
    )
    current = os.fstat(descriptor)
    require(
        (current.st_dev, current.st_ino) == (metadata.st_dev, metadata.st_ino),
        "execution lease identity changed during open",
    )
    os.set_inheritable(descriptor, True)
    os.lseek(descriptor, 0, os.SEEK_SET)
    content = os.read(descriptor, current.st_size + 1)
    return descriptor, {
        "path": str(RUN_LEASE),
        "canonical_path_sha256": sha256_bytes(
            str(RUN_LEASE.resolve(strict=True)).encode()
        ),
        "mode": f"{mode:04o}",
        "uid_owned": True,
        "regular": True,
        "sha256": sha256_bytes(content),
        "size": len(content),
    }


def acquire_lease(descriptor: int) -> None:
    try:
        fcntl.flock(descriptor, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError as error:
        raise LeaseBusy("execution lease is held by a surviving process") from error


def require_lease_content(descriptor: int, receipt: str) -> None:
    expected = lease_payload(receipt)
    os.lseek(descriptor, 0, os.SEEK_SET)
    actual = os.read(descriptor, len(expected) + 1)
    require(actual == expected, "execution lease content or identity drift")


def create_execution_lease(receipt: str) -> tuple[int, dict[str, Any]]:
    require_direct_scratch_path(RUN_LEASE, must_exist=False, directory=False)
    descriptor = os.open(
        RUN_LEASE,
        os.O_RDWR
        | os.O_CREAT
        | os.O_EXCL
        | getattr(os, "O_NOFOLLOW", 0),
        0o600,
    )
    try:
        os.fchmod(descriptor, 0o600)
        payload = lease_payload(receipt)
        written = os.write(descriptor, payload)
        require(written == len(payload), "execution lease write was partial")
        os.fsync(descriptor)
        os.set_inheritable(descriptor, True)
        acquire_lease(descriptor)
        parent = fsync_scratch_parent("lease-created")
        current, record = open_lease_candidate()
        try:
            require_lease_content(current, receipt)
        finally:
            os.close(current)
        return descriptor, record | {
            "schema": "nika.mutation-testimonial.lease.v1",
            "receipt_commit": receipt,
            "run_id": RUN_ID,
            "tested_commit": TESTED_COMMIT,
            "file_fsync": True,
            "scratch_parent": parent,
            "inheritable": os.get_inheritable(descriptor),
            "exclusive_lock": True,
        }
    except BaseException:
        os.close(descriptor)
        raise


def unlink_locked_lease(descriptor: int, stage: str) -> dict[str, Any]:
    metadata = os.lstat(RUN_LEASE)
    current = os.fstat(descriptor)
    require(
        stat.S_ISREG(metadata.st_mode)
        and (current.st_dev, current.st_ino) == (metadata.st_dev, metadata.st_ino),
        "execution lease changed before retirement",
    )
    acquire_lease(descriptor)
    authority = fsync_scratch_parent(f"{stage}-authority-before-lease")
    RUN_LEASE.unlink()
    lease_unlinked = fsync_scratch_parent(f"{stage}-lease-unlinked")
    return {
        "removed": True,
        "authority_before_lease": authority,
        "lease_unlinked": lease_unlinked,
    }


def retire_execution_lease(descriptor: int, receipt: str, stage: str) -> dict[str, Any]:
    require_lease_content(descriptor, receipt)
    return unlink_locked_lease(descriptor, stage)


def startup_root_record(kind: str) -> dict[str, Any]:
    path = {
        "output": pathlib.Path(RUN_OUTPUT),
        "target": pathlib.Path(RUN_TARGET),
        "temp": pathlib.Path(RUN_TEMP),
    }[kind]
    if not os.path.lexists(path):
        return {"kind": kind, "exists": False}
    require_direct_scratch_path(path, must_exist=True)
    canonical = require_same_bound_path(
        path, path, must_exist=True, directory=True
    )
    metadata = os.lstat(path)
    require(not stat.S_ISLNK(metadata.st_mode), f"{kind} startup root is a symlink")
    require(stat.S_ISDIR(metadata.st_mode), f"{kind} startup root is not a directory")
    require(metadata.st_uid == os.getuid(), f"{kind} startup root owner drift")
    mode = stat.S_IMODE(metadata.st_mode)
    require(mode & 0o077 == 0, f"{kind} startup root permissions are ambiguous")
    return {
        "kind": kind,
        "exists": True,
        "canonical_path_sha256": sha256_bytes(str(canonical).encode()),
        "mode": f"{mode:04o}",
        "uid_owned": True,
        "real_directory": True,
    }


def resume_startup_receipt(
    repo: pathlib.Path,
    receipt: str,
    roots: list[dict[str, Any]],
    resume_validator: Any = None,
) -> dict[str, Any]:
    validate_resume = resume_validator or (
        lambda target_repo, target_receipt, target_output: validate_raw_run_receipt(
            target_repo,
            target_receipt,
            target_output,
            allow_stale_lease=True,
        )
    )
    lease_descriptor = None
    try:
        if os.path.lexists(RUN_LEASE):
            lease_descriptor, _lease = open_lease_candidate()
            acquire_lease(lease_descriptor)
            require_lease_content(lease_descriptor, receipt)
        validate_resume(repo, receipt, pathlib.Path(RUN_OUTPUT))
    except (
        EvidenceError,
        KeyError,
        json.JSONDecodeError,
        OSError,
        subprocess.CalledProcessError,
    ) as error:
        if lease_descriptor is not None:
            os.close(lease_descriptor)
        raise EvidenceError(
            f"invalid startup receipt preserved; manual review required: {error}"
        ) from error
    retired = None
    if lease_descriptor is not None:
        try:
            retired = retire_execution_lease(
                lease_descriptor, receipt, "settled-lease-recovery"
            )
        finally:
            os.close(lease_descriptor)
    return {
        "action": "resume",
        "roots": roots,
        "raw_receipt": True,
        "lease_retired": retired,
    }


def recover_receiptless_scratch(
    receipt: str, roots: list[dict[str, Any]]
) -> dict[str, Any]:
    existing = [record for record in roots if record["exists"]]
    if not os.path.lexists(RUN_LEASE):
        require(
            not existing,
            "receipt-less scratch has no execution lease; preserving as ambiguous",
        )
        return {
            "action": "clean",
            "roots": roots,
            "raw_receipt": False,
            "path_absence_probe": probe_paths_absent(),
        }
    lease_descriptor = None
    invalid_aborted_creation = False
    try:
        lease_descriptor, _lease = open_lease_candidate()
        acquire_lease(lease_descriptor)
        try:
            require_lease_content(lease_descriptor, receipt)
        except EvidenceError:
            require(
                not existing,
                "invalid execution lease with scratch was preserved",
            )
            invalid_aborted_creation = True
    except (EvidenceError, OSError) as error:
        if lease_descriptor is not None:
            os.close(lease_descriptor)
        raise EvidenceError(
            f"ambiguous or busy execution lease preserved: {error}"
        ) from error
    removed = []
    try:
        for record in existing:
            removed.append(
                {"kind": record["kind"]}
                | remove_owned_scratch_root(record["kind"])
            )
        durability = unlink_locked_lease(
            lease_descriptor, "startup-recovery-removal"
        )
    finally:
        os.close(lease_descriptor)
    absence = probe_paths_absent()
    return {
        "action": "recovered",
        "roots": roots,
        "removed": removed,
        "lease_retirement": durability,
        "aborted_creation": invalid_aborted_creation,
        "path_absence_probe": absence,
        "raw_receipt": False,
    }


def classify_startup_recovery(
    repo: pathlib.Path,
    receipt: str,
    resume_validator: Any = None,
) -> dict[str, Any]:
    """Classify exact reserved scratch before admitting a fresh process."""
    try:
        roots = [startup_root_record(kind) for kind in ("output", "target", "temp")]
    except (EvidenceError, OSError) as error:
        raise EvidenceError(
            f"ambiguous startup scratch preserved; manual review required: {error}"
        ) from error
    if os.path.lexists(RAW_RUN_RECEIPT):
        return resume_startup_receipt(repo, receipt, roots, resume_validator)
    return recover_receiptless_scratch(receipt, roots)


def fsync_scratch_parent(stage: str) -> dict[str, Any]:
    parent = pathlib.Path("/tmp").resolve(strict=True)
    metadata = os.lstat(parent)
    require(stat.S_ISDIR(metadata.st_mode), "canonical scratch parent is not real")
    descriptor = os.open(
        parent,
        os.O_RDONLY
        | getattr(os, "O_DIRECTORY", 0)
        | getattr(os, "O_NOFOLLOW", 0),
    )
    try:
        require(stat.S_ISDIR(os.fstat(descriptor).st_mode), "scratch parent fd drift")
        os.fsync(descriptor)
    finally:
        os.close(descriptor)
    return {
        "stage": stage,
        "canonical_path_sha256": sha256_bytes(str(parent).encode()),
        "directory_fsync": True,
    }


def record_settlement_step(guard: dict[str, Any], step: str) -> None:
    observed = guard["settlement_order"]
    require(
        len(observed) < len(SETTLEMENT_ORDER)
        and SETTLEMENT_ORDER[len(observed)] == step,
        f"settlement order drift at {step}",
    )
    observed.append(step)


def cleanup_run_temp_root(reservation: dict[str, Any]) -> dict[str, Any]:
    root = pathlib.Path(RUN_TEMP)
    canonical = require_same_bound_path(
        root, pathlib.Path(RUN_TEMP), must_exist=True, directory=True
    )
    require(
        sha256_bytes(str(canonical).encode())
        == reservation["canonical_path_sha256"],
        "temporary root identity changed before cleanup",
    )
    mode = stat.S_IMODE(os.lstat(root).st_mode)
    require(mode & 0o077 == 0, "temporary root gained group or other permissions")
    hierarchy = reject_temp_cargo_hierarchy(must_exist=True)
    removed = remove_owned_scratch_root("temp")
    return removed | {
        "cargo_hierarchy_checked": hierarchy,
        "scratch_parent": fsync_scratch_parent("temp-removal"),
    }


def cleanup_interrupted_scratch() -> dict[str, Any]:
    removed = []
    for kind in ("output", "target"):
        if os.path.lexists({"output": RUN_OUTPUT, "target": RUN_TARGET}[kind]):
            removed.append({"kind": kind} | remove_owned_scratch_root(kind))
    return {
        "removed": removed,
        "scratch_parent": fsync_scratch_parent("interrupted-removal"),
    }


def remember_cleanup_error(
    primary: BaseException | None,
    secondary: BaseException | None,
    label: str,
    error: BaseException,
) -> BaseException | None:
    if primary is not None:
        note_secondary(primary, label, error)
        return secondary
    if secondary is not None:
        note_secondary(secondary, label, error)
        return secondary
    return error


def finalize_scratch_transaction(
    reservation: dict[str, Any],
    primary: BaseException | None,
    before_cleanup: Any,
    transaction: dict[str, Any],
) -> tuple[dict[str, Any] | None, BaseException | None]:
    post_process = transaction.get("temp_post")
    secondary = None
    if before_cleanup is not None:
        try:
            before_cleanup()
        except BaseException as error:  # noqa: BLE001
            secondary = remember_cleanup_error(
                primary, secondary, "pre-cleanup signal block failed", error
            )
    if os.path.lexists(RUN_TEMP):
        try:
            post_process = cleanup_run_temp_root(reservation)
        except BaseException as error:  # noqa: BLE001
            secondary = remember_cleanup_error(
                primary, secondary, "reserved temp cleanup failed", error
            )
    if primary is not None and not transaction["settled"]:
        try:
            cleanup_interrupted_scratch()
        except BaseException as error:  # noqa: BLE001
            note_secondary(primary, "interrupted scratch cleanup failed", error)
    return post_process, secondary


def guarded_temp_root(
    action: Any,
    after_mkdir: Any = None,
    before_cleanup: Any = None,
    transaction: dict[str, Any] | None = None,
) -> tuple[Any, dict[str, Any], dict[str, Any]]:
    require_supported_platform()
    transaction = transaction or {"settled": False, "temp_post": None}
    reservations = reserve_owned_scratch_roots()
    reservation = reservations["temp"]
    pre_spawn = None
    post_process = None
    primary = None
    try:
        pre_spawn = create_run_temp_root(reservation, after_mkdir)
        transaction["temp_pre"] = pre_spawn
        transaction["temp_reservation"] = reservation
        result = action()
    except BaseException as error:
        primary = error
        raise
    finally:
        post_process, secondary = finalize_scratch_transaction(
            reservation, primary, before_cleanup, transaction
        )
        if primary is None and secondary is not None:
            raise secondary
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


def controlled_handoff_signal(
    guard: dict[str, Any], received: int, _frame: Any
) -> None:
    name = signal.Signals(received).name
    guard["received_signals"].append(name)
    if guard["interrupt_raised"]:
        return
    guard["interrupt_raised"] = True
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
    guard = {
        "old_mask": old_mask,
        "old_handlers": old_handlers,
        "received_signals": [],
        "interrupt_raised": False,
        "settled": False,
        "settlement_order": [],
        "temp_post": None,
        "spawn_attempted": False,
        "execution_group_reaped": False,
    }

    def controlled(received: int, frame: Any) -> None:
        controlled_handoff_signal(guard, received, frame)

    try:
        for item in sorted(HANDOFF_SIGNALS):
            old_handlers[item] = signal.getsignal(item)
            signal.signal(item, controlled)
    except BaseException:
        for item, previous in old_handlers.items():
            signal.signal(item, previous)
        signal.pthread_sigmask(signal.SIG_SETMASK, old_mask)
        raise
    return guard


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
            guard,
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


def lease_pass_fds(descriptor: int | None) -> tuple[int, ...]:
    if descriptor is None:
        return ()
    require(
        os.get_inheritable(descriptor),
        "execution lease descriptor is not inheritable",
    )
    return (descriptor,)


def owned_process(
    argv: list[str],
    cwd: pathlib.Path,
    environment: dict[str, str],
    workspace_config: dict[str, Any],
    guard: dict[str, Any],
    lease_descriptor: int | None = None,
    post_spawn: Any = None,
) -> tuple[int, dict[str, Any], int]:
    process = None
    primary = None
    monitoring = None
    secondary = None
    child_mask = guard["old_mask"] - HANDOFF_SIGNALS

    def restore_child_mask() -> None:
        os.umask(0o077)
        signal.pthread_sigmask(signal.SIG_SETMASK, child_mask)

    try:
        block_guard_signals(guard)
        pass_fds = lease_pass_fds(lease_descriptor)
        # POSIX-only, single-threaded runner: undo the inherited block before exec.
        guard["spawn_attempted"] = True
        process = subprocess.Popen(
            argv,
            cwd=cwd,
            env=environment,
            start_new_session=True,
            pass_fds=pass_fds,
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
        if stopped is None:
            guard["execution_group_reaped"] = True
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
    lease_descriptor: int,
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
        lease_descriptor,
    )
    return returncode, monitoring, argv, process_group


def timed_mutation_process(
    environment: dict[str, str],
    tools: dict[str, dict[str, Any]],
    workspace_config: dict[str, Any],
    guard: dict[str, Any],
    lease_descriptor: int,
) -> tuple[str, int, int, dict[str, Any], list[str], int, int, str]:
    wall_start = now_utc()
    monotonic_start = time.monotonic_ns()
    returncode, monitoring, process_argv, process_group = run_process(
        environment, tools, workspace_config, guard, lease_descriptor
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


def fsync_bound_artifacts(
    output: pathlib.Path, artifacts: dict[str, dict[str, Any]]
) -> dict[str, Any]:
    output_real = require_same_bound_path(
        output, pathlib.Path(RUN_OUTPUT), must_exist=True, directory=True
    )
    files = []
    directories = {output_real}
    for name, record in sorted(artifacts.items()):
        path = output_real / record["relative_path"]
        current = artifact_record(output_real, path)
        require(current["sha256"] == record["sha256"], f"{name} changed before fsync")
        flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0)
        descriptor = os.open(path, flags)
        try:
            require(stat.S_ISREG(os.fstat(descriptor).st_mode), f"{name} is not regular")
            os.fsync(descriptor)
        finally:
            os.close(descriptor)
        require(sha256_file(path) == record["sha256"], f"{name} changed during fsync")
        files.append(
            {
                "name": name,
                "relative_path": record["relative_path"],
                "sha256": record["sha256"],
                "file_fsync": True,
            }
        )
        directories.add(path.parent)
    synced_directories = []
    for directory in sorted(directories):
        relative = directory.relative_to(output_real)
        descriptor = os.open(
            directory,
            os.O_RDONLY
            | getattr(os, "O_DIRECTORY", 0)
            | getattr(os, "O_NOFOLLOW", 0),
        )
        try:
            require(stat.S_ISDIR(os.fstat(descriptor).st_mode), "artifact parent is not a directory")
            os.fsync(descriptor)
        finally:
            os.close(descriptor)
        synced_directories.append(
            {
                "relative_path": str(relative) or ".",
                "canonical_path_sha256": sha256_bytes(str(directory).encode()),
                "directory_fsync": True,
            }
        )
    return {"files": files, "directories": synced_directories}


def atomic_json_write(path: pathlib.Path, value: Any) -> dict[str, Any]:
    parent = require_same_bound_path(
        path.parent, pathlib.Path(RUN_OUTPUT), must_exist=True, directory=True
    )
    require(not os.path.lexists(path), "durable receipt already exists")
    temporary = parent / f".{path.name}.{RUN_ID}.tmp"
    require(not os.path.lexists(temporary), "durable receipt temp file exists")
    payload = (
        json.dumps(value, indent=2, ensure_ascii=False, sort_keys=False) + "\n"
    ).encode()
    descriptor = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    try:
        with os.fdopen(descriptor, "wb") as handle:
            handle.write(payload)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
        directory = os.open(parent, os.O_RDONLY | getattr(os, "O_DIRECTORY", 0))
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
    finally:
        if os.path.lexists(temporary):
            temporary.unlink()
    record = artifact_record(parent, path, allowed={"run-receipt.raw.json"})
    return record | {
        "atomic_replace": True,
        "file_fsync": True,
        "parent_directory_fsync": True,
    }


def settle_raw_receipt(
    guard: dict[str, Any], run_receipt: dict[str, Any], after_fsync: Any = None
) -> dict[str, Any]:
    require(guard["settled"] is False, "transaction already settled")
    block_guard_signals(guard)
    durable = atomic_json_write(RAW_RUN_RECEIPT, run_receipt)
    require(
        durable["atomic_replace"]
        and durable["file_fsync"]
        and durable["parent_directory_fsync"],
        "raw receipt did not reach durable settlement",
    )
    record_settlement_step(
        guard, "receipt-atomic-file-output-directory-fsync"
    )
    settlement_parent = fsync_scratch_parent("after-receipt-before-settled")
    require(
        settlement_parent
        == run_receipt["scratch_durability"]["settlement_parent"],
        "settlement scratch-parent fsync drift",
    )
    record_settlement_step(guard, "settlement-parent-fsync")
    pending = HANDOFF_SIGNALS & signal.sigpending()
    if pending:
        unblock_guard_signals(guard)
        raise EvidenceError("pending supported signal did not interrupt settlement")
    guard["durable_receipt"] = durable
    guard["settled"] = True
    record_settlement_step(guard, "settled")
    require(guard["settlement_order"] == SETTLEMENT_ORDER, "settlement order incomplete")
    if after_fsync is not None:
        after_fsync()
    unblock_guard_signals(guard)
    return durable


def build_raw_run_receipt(
    receipt: str,
    start: dict[str, Any],
    process: tuple[Any, ...],
    runtime: dict[str, Any],
) -> dict[str, Any]:
    (
        wall_start,
        monotonic_start,
        returncode,
        monitoring,
        process_argv,
        process_group,
        monotonic_end,
        wall_end,
    ) = process
    return {
        "schema": "nika.mutation-testimonial.run.raw.v1",
        "guarantee": "Reproducible local testimonial from the committed wrapper; not remote attestation and not a claim against a malicious local operator.",
        "receipt_commit": receipt,
        "tested_commit": TESTED_COMMIT,
        "tested_tree": TESTED_TREE,
        "run_id": RUN_ID,
        "settlement": SETTLEMENT_POLICY,
        "settlement_order": SETTLEMENT_ORDER,
        "execution_lease": runtime["lease_record"],
        "scratch_durability": runtime["scratch_durability"],
        "artifact_durability": runtime["artifact_durability"],
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
        "tool_binaries": runtime["tools"],
        "environment": runtime["environment_records"],
        "cargo_config": runtime["cargo_config"],
        "temp_root": {
            "pre_spawn": runtime["guard"]["temp_pre"],
            "post_process": runtime["guard"]["temp_post"],
        },
        "pre_probe": runtime["pre_probe"],
        "monitoring": monitoring,
        "post_probe": runtime["post_probe"],
        "artifacts": runtime["artifacts"],
        "counts": runtime["counts"],
        "written_at": now_utc(),
    }


def execute_mutation_transaction(
    guard: dict[str, Any],
    receipt: str,
    start: dict[str, Any],
    runtime: dict[str, Any],
    after_process: Any = None,
    after_fsync: Any = None,
) -> tuple[tuple[Any, ...], dict[str, Any]]:
    process = timed_mutation_process(
        runtime["environment"],
        runtime["tools"],
        start["run"]["cargo_config"]["workspace"],
        guard,
        runtime["lease_descriptor"],
    )
    if after_process is not None:
        after_process(guard)
    unblock_guard_signals(guard)
    runtime["post_probe"] = probe_run_worktree()
    output = pathlib.Path(RUN_OUTPUT)
    target = pathlib.Path(RUN_TARGET)
    require_direct_scratch_path(output, must_exist=True)
    require_direct_scratch_path(target, must_exist=True)
    require_same_bound_path(output, pathlib.Path(RUN_OUTPUT), must_exist=True, directory=True)
    require_same_bound_path(target, pathlib.Path(RUN_TARGET), must_exist=True, directory=True)
    require(os.lstat(output).st_uid == os.getuid(), "output root owner drift")
    require(os.lstat(target).st_uid == os.getuid(), "target root owner drift")
    require(
        stat.S_IMODE(os.lstat(output).st_mode) & 0o077 == 0,
        "output root permissions drift",
    )
    require(
        stat.S_IMODE(os.lstat(target).st_mode) & 0o077 == 0,
        "target root permissions drift",
    )
    block_guard_signals(guard)
    materialized_parent = fsync_scratch_parent("materialized-output-target")
    unblock_guard_signals(guard)
    record_settlement_step(guard, "scratch-materialized-parent-fsync")
    raw_outcomes_path, raw_mutants_path = raw_artifacts(output)
    raw_outcomes = json.loads(raw_outcomes_path.read_text(encoding="utf-8"))
    raw_mutants = json.loads(raw_mutants_path.read_text(encoding="utf-8"))
    runtime["artifacts"] = raw_artifact_records(
        output, raw_outcomes_path, raw_mutants_path
    )
    runtime["counts"] = raw_counts(raw_outcomes, raw_mutants)
    block_guard_signals(guard)
    runtime["artifact_durability"] = fsync_bound_artifacts(
        output, runtime["artifacts"]
    )
    unblock_guard_signals(guard)
    record_settlement_step(guard, "artifact-files-and-directories-fsync")
    block_guard_signals(guard)
    target_cleanup = remove_owned_scratch_root("target")
    guard["temp_post"] = cleanup_run_temp_root(guard["temp_reservation"])
    removal_parent = fsync_scratch_parent("pre-settlement-removal")
    unblock_guard_signals(guard)
    record_settlement_step(guard, "scratch-removal-parent-fsync")
    runtime["scratch_durability"] = {
        "materialized_parent": materialized_parent,
        "target": {
            "evidence_authority": False,
            "cleanup": target_cleanup,
        },
        "removal_parent": removal_parent,
        "settlement_parent": {
            "stage": "after-receipt-before-settled",
            "canonical_path_sha256": materialized_parent[
                "canonical_path_sha256"
            ],
            "directory_fsync": True,
        },
    }
    run_receipt = build_raw_run_receipt(receipt, start, process, runtime | {"guard": guard})
    settle_raw_receipt(guard, run_receipt, after_fsync)
    return process, run_receipt


def run_mutation(repo: pathlib.Path, receipt_commit: str) -> None:
    require_supported_platform()
    receipt = git(repo, "rev-parse", receipt_commit).strip()
    require(git(repo, "rev-parse", "HEAD").strip() == receipt, "runner must execute from receipt HEAD")
    require(git(repo, "status", "--porcelain=v2", "--untracked-files=all") == "", "receipt worktree is dirty")
    startup = classify_startup_recovery(repo, receipt)
    if startup["action"] == "resume":
        print("RESUME: valid durable raw receipt exists; sanitize without rerunning")
        return
    start = verify_start(repo, receipt, require_paths_absent=True)
    environment = build_run_environment(dict(os.environ))
    tools = tool_binaries(environment, pathlib.Path(RUN_WORKTREE))
    require(tool_fingerprints(tools) == {name: value for name, value in start["tools"].items() if name != "python"}, "runner tool binaries drifted after capture")
    environment_records = relevant_environment(environment)
    require(environment_records == start["run"]["environment_hashes"], "runner environment drifted after capture")
    cargo_config = cargo_config_binding(repo, TESTED_COMMIT, environment)
    require(cargo_config == start["run"]["cargo_config"], "runner Cargo config drifted after capture")
    lease_descriptor = None
    primary = None
    runtime: dict[str, Any] = {}
    try:
        lease_descriptor, lease_record = create_execution_lease(receipt)
        runtime = {
            "environment": environment,
            "environment_records": environment_records,
            "tools": tools,
            "cargo_config": cargo_config,
            "pre_probe": probe_run_worktree(),
            "lease_descriptor": lease_descriptor,
            "lease_record": lease_record,
        }

        def execute(guard: dict[str, Any]) -> tuple[tuple[Any, ...], dict[str, Any]]:
            runtime["guard"] = guard
            return execute_mutation_transaction(guard, receipt, start, runtime)

        settled, _, _ = guarded_signal_temp_root(execute)
        retire_execution_lease(
            lease_descriptor, receipt, "settled-lease-retirement"
        )
        process, _run_receipt = settled
        returncode = process[2]
        monitoring = process[3]
        monotonic_start = process[1]
        monotonic_end = process[6]
        require(returncode == 0, f"cargo-mutants exited {returncode}")
        require_monitoring(monitoring)
        require(monotonic_end > monotonic_start, "non-positive monotonic run duration")
        require(runtime["pre_probe"] == runtime["post_probe"] == start["run"]["detached_probe"], "pre/post worktree probe drift")
        print("OK: runner fsynced a settled raw receipt after process completion")
    except BaseException as error:
        primary = error
        raise
    finally:
        if lease_descriptor is not None:
            guard = runtime.get("guard", {})
            roots_absent = all(
                not os.path.lexists(path)
                for path in (RUN_OUTPUT, RUN_TARGET, RUN_TEMP)
            )
            may_retire = (
                not guard.get("spawn_attempted", False)
                or guard.get("execution_group_reaped", False)
            ) and roots_absent
            try:
                if os.path.lexists(RUN_LEASE) and may_retire:
                    retire_execution_lease(
                        lease_descriptor, receipt, "failed-run-lease-retirement"
                    )
            except (EvidenceError, OSError) as error:
                if primary is not None:
                    note_secondary(primary, "execution lease retirement failed", error)
                else:
                    raise
            finally:
                os.close(lease_descriptor)


def accounting(start: dict[str, Any], census: list[str], mutants: list[dict[str, Any]], outcomes: dict[str, Any]) -> dict[str, Any]:
    exclusions = start["equivalent_exclusions"]
    validate_equivalent_exclusions(exclusions, census)
    excluded_names = {item["name"] for item in exclusions}
    expected = set(census) - excluded_names
    mutant_names = [item["name"] for item in mutants]
    require(len(mutant_names) == len(set(mutant_names)), "duplicate mutants.json identities")
    require(set(mutant_names) == expected, "mutants.json is not census minus exact exclusions")
    by_name = mutant_outcomes(outcomes)
    require(set(by_name) == expected, "outcomes set is not census minus exact exclusions")
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
    required_caught = [
        control for item in exclusions for control in item["same_span_controls"]
    ] + [item["current_name"] for item in start["prior_anomalies"]] + start["guard_mutants"]
    wrong = {name: by_name.get(name) for name in required_caught if by_name.get(name) != "CaughtMutant"}
    require(not wrong, f"anomaly/control reconciliation failed: {wrong}")
    return {
        "unfiltered_census": len(census),
        "excluded_equivalent": len(excluded_names),
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
            "settlement",
            "settlement_order",
            "execution_lease",
            "scratch_durability",
            "artifact_durability",
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
    require(
        run_receipt["settlement"] == SETTLEMENT_POLICY,
        "raw run receipt settlement policy mismatch",
    )
    require(
        run_receipt["settlement_order"] == SETTLEMENT_ORDER,
        "raw run receipt settlement order mismatch",
    )


def require_execution_lease_record(record: dict[str, Any], receipt: str) -> None:
    require(record["schema"] == "nika.mutation-testimonial.lease.v1", "lease schema drift")
    require(record["path"] == str(RUN_LEASE), "lease path drift")
    require_sha256(record["canonical_path_sha256"], "lease path hash drift")
    require(record["mode"] == "0600", "lease mode drift")
    require(record["uid_owned"] is True and record["regular"] is True, "lease file type drift")
    require_sha256(record["sha256"], "lease content hash drift")
    require(
        record["sha256"] == sha256_bytes(lease_payload(receipt)),
        "lease content binding drift",
    )
    require(record["size"] == len(lease_payload(receipt)), "lease size drift")
    require(record["receipt_commit"] == receipt, "lease receipt drift")
    require(record["run_id"] == RUN_ID, "lease run_id drift")
    require(record["tested_commit"] == TESTED_COMMIT, "lease tested commit drift")
    require(record["file_fsync"] is True, "lease file was not fsynced")
    require(record["inheritable"] is True, "lease descriptor was not inheritable")
    require(record["exclusive_lock"] is True, "lease was not exclusively locked")
    require(
        record["scratch_parent"]["stage"] == "lease-created"
        and record["scratch_parent"]["directory_fsync"] is True,
        "lease scratch-parent durability drift",
    )


def require_scratch_durability(
    record: dict[str, Any], *, portable: bool = False
) -> None:
    require(
        set(record)
        == {"materialized_parent", "target", "removal_parent", "settlement_parent"},
        "scratch durability field drift",
    )
    hashes = []
    for name in ("materialized_parent", "removal_parent", "settlement_parent"):
        parent = record[name]
        require_sha256(parent["canonical_path_sha256"], f"{name} hash drift")
        hashes.append(parent["canonical_path_sha256"])
        require(parent["directory_fsync"] is True, f"{name} lacks fsync")
    require(len(set(hashes)) == 1, "scratch parent hash changed during settlement")
    if not portable:
        expected_hash = sha256_bytes(
            str(pathlib.Path("/tmp").resolve(strict=True)).encode()
        )
        require(hashes[0] == expected_hash, "scratch parent host binding drift")
    require(
        record["materialized_parent"]["stage"] == "materialized-output-target"
        and record["removal_parent"]["stage"] == "pre-settlement-removal"
        and record["settlement_parent"]["stage"] == "after-receipt-before-settled",
        "scratch durability stage drift",
    )
    target = record["target"]
    require(target["evidence_authority"] is False, "target became evidence authority")
    require(
        target["cleanup"]["removed"] is True
        and target["cleanup"]["fd_safe_no_symlink_follow"] is True,
        "target cleanup proof drift",
    )


def load_raw_run_receipt(output: pathlib.Path, receipt: str) -> dict[str, Any]:
    artifact_record(output, RAW_RUN_RECEIPT, allowed={"run-receipt.raw.json"})
    run_receipt = json.loads(RAW_RUN_RECEIPT.read_text(encoding="utf-8"))
    require_receipt_identity(run_receipt, receipt)
    return run_receipt


def require_committed_artifact_durability(
    durability: dict[str, Any], artifacts: dict[str, dict[str, Any]]
) -> None:
    files = durability["files"]
    require(
        {item["name"] for item in files} == set(artifacts),
        "committed fsynced artifact set drift",
    )
    for item in files:
        artifact = artifacts[item["name"]]
        require(item["relative_path"] == artifact["relative_path"], "fsynced path drift")
        require(item["sha256"] == artifact["sha256"], "fsynced artifact hash drift")
        require(item["file_fsync"] is True, "artifact lacks file fsync")
    directories = durability["directories"]
    require(directories and len({item["relative_path"] for item in directories}) == len(directories), "fsynced directory set drift")
    require(any(item["relative_path"] == "." for item in directories), "output root was not fsynced")
    for item in directories:
        require_sha256(item["canonical_path_sha256"], "fsynced directory hash drift")
        require(item["directory_fsync"] is True, "artifact parent lacks directory fsync")


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
    require(post["scratch_parent"]["directory_fsync"] is True, "raw temp removal parent was not fsynced")


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
    require(post["scratch_parent"]["directory_fsync"] is True, "committed temp removal parent lacks fsync")


def validate_raw_run_receipt(
    repo: pathlib.Path,
    receipt: str,
    output: pathlib.Path,
    *,
    allow_stale_lease: bool = False,
) -> tuple[dict[str, Any], dict[str, Any], list[dict[str, Any]]]:
    require_same_bound_path(
        output, pathlib.Path(RUN_OUTPUT), must_exist=True, directory=True
    )
    require(not os.path.lexists(RUN_TARGET), "build target remains after settlement")
    if not allow_stale_lease:
        require(not os.path.lexists(RUN_LEASE), "execution lease remains after settlement")
    require_direct_scratch_path(output, must_exist=True)
    require(not os.path.lexists(RUN_TEMP), "temporary root remains after run")
    run_receipt = load_raw_run_receipt(output, receipt)
    require_execution_lease_record(run_receipt["execution_lease"], receipt)
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
    require_scratch_durability(run_receipt["scratch_durability"])
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
    require(
        run_receipt["artifact_durability"]
        == fsync_bound_artifacts(output, expected_artifacts),
        "raw artifact fsync binding drift",
    )
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
        "schema": "nika.mutation-testimonial.final.v3",
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
        "equivalent_exclusions": start["equivalent_exclusions"],
        "equivalence_source_binding": start["equivalence_source_binding"],
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


def verify_raw_final_reproduction(
    repo: pathlib.Path,
    receipt_commit: str,
    raw_output: pathlib.Path,
    start: dict[str, Any],
    manifest: dict[str, Any],
    outcomes: dict[str, Any],
    mutants: list[dict[str, Any]],
    run_receipt: dict[str, Any],
) -> None:
    require_same_bound_path(
        raw_output,
        pathlib.Path(start["run"]["output_dir"]),
        must_exist=True,
        directory=True,
    )
    raw_receipt, raw_outcomes, raw_mutants = validate_raw_run_receipt(
        repo, receipt_commit, raw_output
    )
    replacements = known_replacements(raw_receipt["tool_binaries"])
    counts: dict[str, int] = {}
    require(
        sanitize_value(raw_outcomes, replacements, counts) == outcomes,
        "outcomes sanitization is not reproducible",
    )
    require(
        sanitize_value(raw_mutants, replacements, counts) == mutants,
        "mutants sanitization is not reproducible",
    )
    require(
        sanitize_value(raw_receipt, replacements, counts) == run_receipt,
        "run receipt sanitization is not reproducible",
    )
    require(
        counts == manifest["sanitization"]["replacements"],
        "sanitization replacement counts drift",
    )
    require(
        manifest["artifacts"]["run-receipt.json"]["raw_sha256"]
        == sha256_file(RAW_RUN_RECEIPT),
        "raw run receipt hash drift",
    )


def verify_final(repo: pathlib.Path, final_commit: str, raw_output: pathlib.Path | None) -> dict[str, Any]:
    final_commit = git(repo, "rev-parse", final_commit).strip()
    lane = repo / LANE
    manifest = json.loads((lane / "manifest.json").read_text(encoding="utf-8"))
    run_receipt = json.loads((lane / "run-receipt.json").read_text(encoding="utf-8"))
    require(manifest["schema"] == "nika.mutation-testimonial.final.v3", "wrong final schema")
    receipt_commit = git(repo, "rev-parse", f"{final_commit}^").strip()
    require(manifest["pre_run_receipt_commit"] == receipt_commit, "manifest receipt is not final parent")
    require_receipt_identity(run_receipt, receipt_commit)
    require_execution_lease_record(run_receipt["execution_lease"], receipt_commit)
    start = verify_start(repo, receipt_commit, require_paths_absent=False)
    delta = changed_paths(repo, receipt_commit, final_commit)
    require(delta <= FINAL_ALLOWED, f"final commit contains non-evidence paths: {sorted(delta - FINAL_ALLOWED)}")
    require(manifest["tested_commit"] == TESTED_COMMIT and manifest["tested_tree"] == TESTED_TREE, "final tested binding drift")
    require(
        manifest["equivalent_exclusions"] == start["equivalent_exclusions"]
        and manifest["equivalence_source_binding"]
        == start["equivalence_source_binding"],
        "final equivalence binding drift",
    )
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
    require_scratch_durability(run_receipt["scratch_durability"], portable=True)
    require(run_receipt["counts"] == raw_counts(outcomes, mutants), "committed raw counts drift")
    require_committed_artifact_durability(
        run_receipt["artifact_durability"], run_receipt["artifacts"]
    )
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
        verify_raw_final_reproduction(
            repo,
            receipt_commit,
            raw_output,
            start,
            manifest,
            outcomes,
            mutants,
            run_receipt,
        )
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
