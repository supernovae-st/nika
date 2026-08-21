#!/usr/bin/env python3
"""Negative fixtures for the W7 mutation testimonial validator."""

import json
import os
import pathlib
import runpy
import signal
import subprocess
import sys
import tempfile
import time
from typing import Any

validator = runpy.run_path(pathlib.Path(__file__).with_name("validate.py"))
EvidenceError = validator["EvidenceError"]
RUN_ID = validator["RUN_ID"]
RUN_OUTPUT = validator["RUN_OUTPUT"]
RAW_RUN_RECEIPT = validator["RAW_RUN_RECEIPT"]
RUN_TARGET = validator["RUN_TARGET"]
RUN_TEMP = validator["RUN_TEMP"]
RUN_WORKTREE = validator["RUN_WORKTREE"]
TESTED_COMMIT = validator["TESTED_COMMIT"]
TESTED_TREE = validator["TESTED_TREE"]

failures = 0


def expect_red(action: Any) -> None:
    global failures
    try:
        action()
    except (EvidenceError, KeyError, OSError, TypeError):
        failures += 1
        return
    raise EvidenceError("negative self-test unexpectedly passed")


def injected_failure() -> None:
    raise EvidenceError("injected post-create interruption")


def injected_monitor_failure() -> None:
    raise EvidenceError("injected monitor interruption")


def process_exists(pid: int) -> bool:
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    return True


def scratch_alias(path: pathlib.Path) -> pathlib.Path:
    if sys.platform == "darwin":
        return pathlib.Path("/" + "private" + "/tmp") / path.relative_to("/tmp")
    return path


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
        "/" + "Volumes" + "/external/source",
        "/" + "private" + "/tmp/source",
        "C:\\Users\\someone\\source",
        "s" + "k-" + "a" * 20,
        "token" + "=" + "a" * 20,
        "api_key" + ": " + "b" * 20,
        "secret" + "='" + "c" * 20 + "'",
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

validator["require_supported_platform"]()

with tempfile.TemporaryDirectory(dir="/tmp") as tmp:
    logical = pathlib.Path(tmp)
    alias = scratch_alias(logical)
    validator["require_same_bound_path"](
        logical, alias, must_exist=True, directory=True
    )
    validator["require_same_bound_path"](
        logical / "future", alias / "future", must_exist=False
    )

safe_cwd = {"logical": RUN_WORKTREE, "canonical_sha256": "9" * 64}
safe_tools = {
    "cargo_mutants": {
        "invoked_path": "<CARGO_MUTANTS_BIN>",
        "invoked_path_sha256": "a" * 64,
        "invoked_kind": "regular",
        "invoked_size": 1,
        "symlink_target": None,
        "symlink_target_sha256": None,
        "command_cwd": safe_cwd,
        "proxy_realpath": "<CARGO_MUTANTS_BIN>",
        "proxy_sha256": "b" * 64,
        "proxy_version": "cargo-mutants test",
        "selected_toolchain": None,
    },
    "rustup": {
        "invoked_path": "<RUSTUP_PROXY_BIN>",
        "invoked_path_sha256": "5" * 64,
        "invoked_kind": "regular",
        "invoked_size": 1,
        "symlink_target": None,
        "symlink_target_sha256": None,
        "command_cwd": safe_cwd,
        "proxy_realpath": "<RUSTUP_PROXY_BIN>",
        "proxy_sha256": "e" * 64,
        "proxy_version": "rustup test",
        "selected_toolchain": None,
    },
    "cargo": {
        "invoked_path": "<CARGO_BIN>",
        "invoked_path_sha256": "c" * 64,
        "invoked_kind": "symlink",
        "invoked_size": 1,
        "symlink_target": "rustup",
        "symlink_target_sha256": "d" * 64,
        "command_cwd": safe_cwd,
        "proxy_realpath": "<RUSTUP_PROXY_BIN>",
        "proxy_sha256": "e" * 64,
        "proxy_version": "cargo proxy test",
        "selected_toolchain": {
            "path": "<CARGO_TOOLCHAIN_BIN>",
            "sha256": "f" * 64,
            "version": "cargo selected test",
            "selection_launcher_sha256": "5" * 64,
        },
    },
    "rustc": {
        "invoked_path": "<RUSTC_BIN>",
        "invoked_path_sha256": "1" * 64,
        "invoked_kind": "symlink",
        "invoked_size": 1,
        "symlink_target": "rustup",
        "symlink_target_sha256": "2" * 64,
        "command_cwd": safe_cwd,
        "proxy_realpath": "<RUSTUP_PROXY_BIN>",
        "proxy_sha256": "e" * 64,
        "proxy_version": "rustc proxy test",
        "selected_toolchain": {
            "path": "<RUSTC_TOOLCHAIN_BIN>",
            "sha256": "4" * 64,
            "version": "rustc selected test",
            "selection_launcher_sha256": "5" * 64,
        },
    },
}
environment = validator["relevant_environment"](
    {"PATH": "/portable/bin", "CARGO_TARGET_DIR": RUN_TARGET, "TMPDIR": RUN_TEMP}
)
config = {
    "workspace": {
        "source": "tested_worktree",
        "path": None,
        "git_blob": None,
        "sha256": None,
        "shadowed": False,
    },
    "user": {
        "source": "HOME/.cargo",
        "name": None,
        "sha256": None,
        "shadowed": False,
    },
    "external_ancestors_checked": 3,
    "temp_hierarchy_checked": 4,
}
portable_start = {
    "tools": validator["tool_fingerprints"](safe_tools)
    | {"python": {"version": "Python test"}},
    "run": {"environment_hashes": environment, "cargo_config": config},
}
portable_receipt = {
    "tool_binaries": safe_tools,
    "environment": environment,
    "cargo_config": config,
}
previous_path = os.environ.get("PATH")
os.environ["PATH"] = "/host-drift/bin"
try:
    validator["require_committed_runtime_binding"](portable_start, portable_receipt)
finally:
    if previous_path is None:
        del os.environ["PATH"]
    else:
        os.environ["PATH"] = previous_path
expect_red(lambda: validator["require_environment_records"](environment[:-1]))
wrong_tools = json.loads(json.dumps(safe_tools))
wrong_tools["rustc"]["proxy_realpath"] = "<WRONG_BIN>"
expect_red(lambda: validator["require_sanitized_tools"](wrong_tools))
wrong_selection = json.loads(json.dumps(safe_tools))
wrong_selection["cargo"]["selected_toolchain"]["selection_launcher_sha256"] = "0" * 64
expect_red(lambda: validator["require_sanitized_tools"](wrong_selection))

with tempfile.TemporaryDirectory(dir="/tmp") as tmp:
    reported_cwd = validator["version"](["pwd"], None, pathlib.Path(tmp))
    validator["require"](
        pathlib.Path(reported_cwd) == pathlib.Path(tmp).resolve(),
        "version command ignored its bound cwd",
    )
    expect_red(
        lambda: validator["require_tool_command_cwd"](pathlib.Path(tmp))
    )

with tempfile.TemporaryDirectory(dir="/tmp") as tmp:
    survivor = pathlib.Path(tmp) / "survivor"
    survivor.write_text("keep")
    expect_red(lambda: validator["guarded_temp_root"](injected_failure))
    expect_red(
        lambda: validator["guarded_temp_root"](
            lambda: None, injected_failure
        )
    )
    validator["require"](
        not os.path.lexists(RUN_TEMP),
        "injected interruption left the reserved temp root behind",
    )
    validator["require"](survivor.read_text() == "keep", "cleanup widened")
    reservation = validator["reserve_run_temp_root"]()
    validator["create_run_temp_root"](reservation)
    validator["cleanup_run_temp_root"](reservation)

block_globals = validator["guarded_signal_temp_root"].__globals__
original_block = block_globals["block_guard_signals"]
block_calls = 0


def fail_first_cleanup_block(guard: dict[str, Any]) -> None:
    global block_calls
    block_calls += 1
    if block_calls == 1:
        raise EvidenceError("injected first cleanup block failure")
    original_block(guard)


block_globals["block_guard_signals"] = fail_first_cleanup_block
try:
    expect_red(lambda: validator["guarded_signal_temp_root"](lambda _guard: None))
finally:
    block_globals["block_guard_signals"] = original_block
validator["require"](not os.path.lexists(RUN_TEMP), "block failure left temp root")
validator["guarded_signal_temp_root"](lambda _guard: "retry")


def create_settlement_roots() -> None:
    for root_name in (RUN_OUTPUT, RUN_TARGET):
        root = pathlib.Path(root_name)
        root.mkdir(mode=0o700)
        (root / "completed-state").write_text("preserve")


def materialize_fixture_scratch(guard: dict[str, Any]) -> dict[str, Any]:
    validator["block_guard_signals"](guard)
    record = validator["fsync_scratch_parent"]("materialized-output-target")
    validator["unblock_guard_signals"](guard)
    validator["record_settlement_step"](
        guard, "scratch-materialized-parent-fsync"
    )
    return record


def finish_fixture_scratch(
    guard: dict[str, Any], materialized: dict[str, Any]
) -> dict[str, Any]:
    validator["block_guard_signals"](guard)
    target_cleanup = validator["remove_owned_scratch_root"]("target")
    guard["temp_post"] = validator["cleanup_run_temp_root"](
        guard["temp_reservation"]
    )
    removal = validator["fsync_scratch_parent"]("pre-settlement-removal")
    validator["unblock_guard_signals"](guard)
    validator["record_settlement_step"](
        guard, "scratch-removal-parent-fsync"
    )
    return {
        "materialized_parent": materialized,
        "target": {"evidence_authority": False, "cleanup": target_cleanup},
        "removal_parent": removal,
        "settlement_parent": {
            "stage": "after-receipt-before-settled",
            "canonical_path_sha256": materialized["canonical_path_sha256"],
            "directory_fsync": True,
        },
    }


def fixture_artifact_durability(guard: dict[str, Any]) -> tuple[dict[str, Any], dict[str, Any]]:
    output = pathlib.Path(RUN_OUTPUT)
    (output / "outcomes.json").write_text("{}")
    (output / "mutants.json").write_text("[]")
    artifacts = validator["raw_artifact_records"](
        output, output / "outcomes.json", output / "mutants.json"
    )
    validator["block_guard_signals"](guard)
    durability = validator["fsync_bound_artifacts"](output, artifacts)
    validator["unblock_guard_signals"](guard)
    validator["record_settlement_step"](
        guard, "artifact-files-and-directories-fsync"
    )
    return artifacts, durability


def minimal_raw_receipt(
    artifacts: dict[str, Any] | None = None,
    durability: dict[str, Any] | None = None,
    scratch: dict[str, Any] | None = None,
) -> dict[str, Any]:
    return {
        "schema": "nika.mutation-testimonial.run.raw.v1",
        "guarantee": "self-test",
        "receipt_commit": "fixture-receipt",
        "tested_commit": TESTED_COMMIT,
        "tested_tree": TESTED_TREE,
        "run_id": RUN_ID,
        "settlement": validator["SETTLEMENT_POLICY"],
        "settlement_order": validator["SETTLEMENT_ORDER"],
        "scratch_durability": scratch or {},
        "artifact_durability": durability or {"files": [], "directories": []},
        "wrapper": {},
        "process": {},
        "tool_binaries": {},
        "environment": {},
        "cargo_config": {},
        "temp_root": {},
        "pre_probe": {},
        "monitoring": {},
        "post_probe": {},
        "artifacts": artifacts or {},
        "counts": {},
        "written_at": "2026-08-21T00:00:00Z",
    }


def fixture_process(guard: dict[str, Any], code: str = "") -> tuple[int, dict[str, Any], int]:
    return validator["owned_process"](
        [sys.executable, "-c", code],
        pathlib.Path("/tmp").resolve(strict=True),
        dict(os.environ),
        {"path": None},
        guard,
    )


def child_umask_process(guard: dict[str, Any]) -> None:
    child_dir = pathlib.Path(RUN_TEMP) / "child-created"
    fixture_process(
        guard,
        f"import os; os.mkdir({str(child_dir)!r}, 0o777)",
    )
    validator["require"](
        os.stat(child_dir).st_mode & 0o077 == 0,
        "child process did not inherit the normalized private umask",
    )


validator["guarded_signal_temp_root"](child_umask_process)


def completed_nonzero_process(guard: dict[str, Any]) -> tuple[int, dict[str, Any], int]:
    create_settlement_roots()
    process = fixture_process(guard, "raise SystemExit(7)")
    materialized = materialize_fixture_scratch(guard)
    artifacts, durability = fixture_artifact_durability(guard)
    scratch = finish_fixture_scratch(guard, materialized)
    validator["settle_raw_receipt"](
        guard, minimal_raw_receipt(artifacts, durability, scratch)
    )
    return process


completed, _, _ = validator["guarded_signal_temp_root"](completed_nonzero_process)
validator["require"](completed[0] == 7, "nonzero process status drift")
validator["require"](
    os.path.lexists(RUN_OUTPUT)
    and os.path.lexists(RAW_RUN_RECEIPT)
    and not os.path.lexists(RUN_TARGET),
    "settled nonzero process lost its artifacts",
)
validator["load_raw_run_receipt"](pathlib.Path(RUN_OUTPUT), "fixture-receipt")
validator["cleanup_interrupted_scratch"]()

hangup_handler = signal.getsignal(signal.SIGHUP)
hangup_mask = signal.pthread_sigmask(signal.SIG_BLOCK, set())
pre_settlement_pending = set()


def hangup_before_receipt(guard: dict[str, Any]) -> None:
    create_settlement_roots()
    fixture_process(guard)
    os.kill(os.getpid(), signal.SIGHUP)
    pre_settlement_pending.update(signal.sigpending())
    validator["unblock_guard_signals"](guard)


expect_red(lambda: validator["guarded_signal_temp_root"](hangup_before_receipt))
validator["require"](
    signal.SIGHUP in pre_settlement_pending,
    "pre-settlement SIGHUP was not pending",
)
validator["require"](
    all(not os.path.lexists(path) for path in (RUN_OUTPUT, RUN_TARGET, RUN_TEMP)),
    "pre-settlement SIGHUP left a scratch root",
)
validator["require"](
    signal.getsignal(signal.SIGHUP) == hangup_handler, "SIGHUP handler drift"
)
validator["require"](
    signal.pthread_sigmask(signal.SIG_BLOCK, set()) == hangup_mask,
    "SIGHUP mask drift",
)
validator["guarded_signal_temp_root"](lambda _guard: "same-receipt-retry")

post_settlement_pending = set()


def hangup_after_receipt(guard: dict[str, Any]) -> None:
    create_settlement_roots()
    fixture_process(guard)
    materialized = materialize_fixture_scratch(guard)
    artifacts, durability = fixture_artifact_durability(guard)
    scratch = finish_fixture_scratch(guard, materialized)

    def queue_hangup() -> None:
        os.kill(os.getpid(), signal.SIGHUP)
        post_settlement_pending.update(signal.sigpending())

    validator["settle_raw_receipt"](
        guard,
        minimal_raw_receipt(artifacts, durability, scratch),
        after_fsync=queue_hangup,
    )


expect_red(lambda: validator["guarded_signal_temp_root"](hangup_after_receipt))
validator["require"](
    signal.SIGHUP in post_settlement_pending,
    "post-settlement SIGHUP was not pending",
)
validator["require"](not os.path.lexists(RUN_TEMP), "settled SIGHUP left temp root")
validator["require"](
    os.path.lexists(RUN_OUTPUT)
    and os.path.lexists(RAW_RUN_RECEIPT)
    and not os.path.lexists(RUN_TARGET),
    "settled SIGHUP lost resumable evidence",
)
resumed = validator["load_raw_run_receipt"](
    pathlib.Path(RUN_OUTPUT), "fixture-receipt"
)
validator["require_committed_artifact_durability"](
    resumed["artifact_durability"], resumed["artifacts"]
)
validator["cleanup_interrupted_scratch"]()


def json_settlement_failure(guard: dict[str, Any]) -> None:
    create_settlement_roots()
    materialized = materialize_fixture_scratch(guard)
    fixture_artifact_durability(guard)
    finish_fixture_scratch(guard, materialized)
    validator["settle_raw_receipt"](guard, {"not_json": object()})


expect_red(lambda: validator["guarded_signal_temp_root"](json_settlement_failure))
validator["require"](
    all(not os.path.lexists(path) for path in (RUN_OUTPUT, RUN_TARGET, RUN_TEMP)),
    "JSON failure left a scratch root",
)

atomic_globals = validator["settle_raw_receipt"].__globals__
original_atomic_write = atomic_globals["atomic_json_write"]


def injected_receipt_fsync_failure(
    _path: pathlib.Path, _value: dict[str, Any]
) -> None:
    raise OSError("injected receipt fsync failure")


def fsync_settlement_failure(guard: dict[str, Any]) -> None:
    create_settlement_roots()
    materialized = materialize_fixture_scratch(guard)
    artifacts, durability = fixture_artifact_durability(guard)
    scratch = finish_fixture_scratch(guard, materialized)
    validator["settle_raw_receipt"](
        guard, minimal_raw_receipt(artifacts, durability, scratch)
    )


atomic_globals["atomic_json_write"] = injected_receipt_fsync_failure
try:
    expect_red(
        lambda: validator["guarded_signal_temp_root"](
            fsync_settlement_failure
        )
    )
finally:
    atomic_globals["atomic_json_write"] = original_atomic_write
validator["require"](
    all(not os.path.lexists(path) for path in (RUN_OUTPUT, RUN_TARGET, RUN_TEMP)),
    "fsync failure left a scratch root",
)

artifact_fsync_globals = validator["fsync_bound_artifacts"].__globals__
original_fsync = artifact_fsync_globals["os"].fsync


def injected_fsync_failure(_descriptor: int) -> None:
    raise OSError("injected fsync failure")


def artifact_fsync_failure(guard: dict[str, Any]) -> None:
    create_settlement_roots()
    output = pathlib.Path(RUN_OUTPUT)
    (output / "outcomes.json").write_text("{}")
    (output / "mutants.json").write_text("[]")
    artifacts = validator["raw_artifact_records"](
        output, output / "outcomes.json", output / "mutants.json"
    )
    validator["fsync_bound_artifacts"](output, artifacts)


artifact_fsync_globals["os"].fsync = injected_fsync_failure
try:
    expect_red(
        lambda: validator["guarded_signal_temp_root"](
            artifact_fsync_failure
        )
    )
finally:
    artifact_fsync_globals["os"].fsync = original_fsync
validator["require"](
    all(not os.path.lexists(path) for path in (RUN_OUTPUT, RUN_TARGET, RUN_TEMP)),
    "artifact fsync failure left a scratch root",
)

parent_globals = validator["fsync_scratch_parent"].__globals__
original_parent_fsync = parent_globals["fsync_scratch_parent"]
parent_fsync_calls = 0


def fail_first_parent_fsync(stage: str) -> dict[str, Any]:
    global parent_fsync_calls
    parent_fsync_calls += 1
    if parent_fsync_calls == 1:
        raise OSError("injected scratch-parent fsync failure")
    return original_parent_fsync(stage)


def parent_fsync_failure(_guard: dict[str, Any]) -> None:
    create_settlement_roots()
    parent_globals["fsync_scratch_parent"]("materialized-output-target")


parent_globals["fsync_scratch_parent"] = fail_first_parent_fsync
try:
    expect_red(
        lambda: validator["guarded_signal_temp_root"](parent_fsync_failure)
    )
finally:
    parent_globals["fsync_scratch_parent"] = original_parent_fsync
validator["require"](
    all(not os.path.lexists(path) for path in (RUN_OUTPUT, RUN_TARGET, RUN_TEMP)),
    "scratch-parent fsync failure left a root",
)
validator["guarded_signal_temp_root"](lambda _guard: "parent-fsync-retry")

settlement_parent_stages: list[str] = []


def fail_settlement_parent_fsync(stage: str) -> dict[str, Any]:
    settlement_parent_stages.append(stage)
    if stage == "after-receipt-before-settled":
        raise OSError("injected settlement-parent fsync failure")
    return original_parent_fsync(stage)


def settlement_parent_fsync_failure(guard: dict[str, Any]) -> None:
    create_settlement_roots()
    materialized = materialize_fixture_scratch(guard)
    artifacts, durability = fixture_artifact_durability(guard)
    scratch = finish_fixture_scratch(guard, materialized)
    validator["settle_raw_receipt"](
        guard, minimal_raw_receipt(artifacts, durability, scratch)
    )


parent_globals["fsync_scratch_parent"] = fail_settlement_parent_fsync
try:
    expect_red(
        lambda: validator["guarded_signal_temp_root"](
            settlement_parent_fsync_failure
        )
    )
finally:
    parent_globals["fsync_scratch_parent"] = original_parent_fsync
validator["require"](
    settlement_parent_stages[-2:]
    == ["after-receipt-before-settled", "interrupted-removal"],
    f"settlement-parent failure order drift: {settlement_parent_stages}",
)
validator["require"](
    all(not os.path.lexists(path) for path in (RUN_OUTPUT, RUN_TARGET, RUN_TEMP)),
    "settlement-parent fsync failure left a root",
)
validator["guarded_signal_temp_root"](
    lambda _guard: "settlement-parent-fsync-retry"
)


def create_fault_roots(*kinds: str) -> None:
    paths = {"output": RUN_OUTPUT, "target": RUN_TARGET, "temp": RUN_TEMP}
    for kind in kinds:
        root = pathlib.Path(paths[kind])
        root.mkdir(mode=0o700)
        os.chmod(root, 0o700)


def clear_fault_roots() -> None:
    paths = {"output": RUN_OUTPUT, "target": RUN_TARGET, "temp": RUN_TEMP}
    for kind, name in paths.items():
        root = pathlib.Path(name)
        if not os.path.lexists(root):
            continue
        if root.is_symlink():
            root.unlink()
            continue
        os.chmod(root, 0o700)
        validator["remove_owned_scratch_root"](kind)
    validator["fsync_scratch_parent"]("selftest-cleanup")


def classify_fixture(
    receipt: str = "fixture-receipt", resume_validator: Any = None
) -> dict[str, Any]:
    return validator["classify_startup_recovery"](
        pathlib.Path.cwd(), receipt, resume_validator
    )


def write_fault_artifacts() -> dict[str, dict[str, Any]]:
    output = pathlib.Path(RUN_OUTPUT)
    (output / "outcomes.json").write_text("{}")
    (output / "mutants.json").write_text("[]")
    return validator["raw_artifact_records"](
        output, output / "outcomes.json", output / "mutants.json"
    )


def fixture_resume_validator(
    _repo: pathlib.Path, receipt: str, output: pathlib.Path
) -> None:
    run_receipt = validator["load_raw_run_receipt"](output, receipt)
    outcomes_path, mutants_path = validator["raw_artifacts"](output)
    artifacts = validator["raw_artifact_records"](
        output, outcomes_path, mutants_path
    )
    validator["require"](
        run_receipt["artifacts"] == artifacts,
        "fixture resume artifacts drift",
    )


fault_stages = ("materialized", "artifacts", "removal", "atomic-temp")
for fault_stage in fault_stages:
    create_fault_roots(
        *("output",) if fault_stage in {"removal", "atomic-temp"} else ("output", "target", "temp")
    )
    if fault_stage in {"artifacts", "removal", "atomic-temp"}:
        write_fault_artifacts()
    if fault_stage == "atomic-temp":
        temporary = pathlib.Path(RUN_OUTPUT) / f".run-receipt.raw.json.{RUN_ID}.tmp"
        temporary.write_text("partial")
    recovered = classify_fixture()
    validator["require"](
        recovered["action"] == "recovered",
        f"{fault_stage} crash was not recovered",
    )
    validator["require"](
        all(not os.path.lexists(path) for path in (RUN_OUTPUT, RUN_TARGET, RUN_TEMP)),
        f"{fault_stage} crash cleanup left a root",
    )
    validator["require"](
        classify_fixture()["action"] == "clean",
        f"{fault_stage} crash did not become retryable",
    )

with tempfile.TemporaryDirectory(dir="/tmp") as startup_external:
    survivor = pathlib.Path(startup_external) / "survivor"
    survivor.write_text("keep")
    create_fault_roots("output", "target", "temp")
    (pathlib.Path(RUN_OUTPUT) / "outside-link").symlink_to(survivor)
    validator["require"](
        classify_fixture()["action"] == "recovered",
        "owned roots containing a symlink were not recovered",
    )
    validator["require"](
        survivor.read_text() == "keep",
        "startup recovery followed an external symlink",
    )

create_fault_roots("output")
resume_artifacts = write_fault_artifacts()
validator["json_write"](
    RAW_RUN_RECEIPT,
    minimal_raw_receipt(artifacts=resume_artifacts),
)
resume = classify_fixture(resume_validator=fixture_resume_validator)
validator["require"](
    resume["action"] == "resume"
    and os.path.lexists(RAW_RUN_RECEIPT)
    and os.path.lexists(RUN_OUTPUT),
    "valid durable receipt was not preserved for sanitize",
)
clear_fault_roots()

create_fault_roots("output")
RAW_RUN_RECEIPT.write_text("{")
expect_red(lambda: classify_fixture(resume_validator=fixture_resume_validator))
validator["require"](os.path.lexists(RUN_OUTPUT), "truncated receipt was deleted")
clear_fault_roots()

create_fault_roots("output")
wrong_artifacts = write_fault_artifacts()
validator["json_write"](
    RAW_RUN_RECEIPT,
    minimal_raw_receipt(artifacts=wrong_artifacts),
)
expect_red(
    lambda: classify_fixture(
        "wrong-receipt", resume_validator=fixture_resume_validator
    )
)
validator["require"](os.path.lexists(RUN_OUTPUT), "wrong receipt was deleted")
clear_fault_roots()

with tempfile.TemporaryDirectory(dir="/tmp") as raw_external:
    survivor = pathlib.Path(raw_external) / "raw-receipt"
    survivor.write_text("external")
    create_fault_roots("output")
    RAW_RUN_RECEIPT.symlink_to(survivor)
    expect_red(lambda: classify_fixture(resume_validator=fixture_resume_validator))
    validator["require"](
        RAW_RUN_RECEIPT.is_symlink() and survivor.read_text() == "external",
        "ambiguous raw receipt was not preserved",
    )
    clear_fault_roots()

create_fault_roots("output")
os.chmod(RUN_OUTPUT, 0o755)
expect_red(classify_fixture)
validator["require"](os.path.lexists(RUN_OUTPUT), "permissive root was deleted")
clear_fault_roots()

with tempfile.TemporaryDirectory(dir="/tmp") as root_external:
    survivor = pathlib.Path(root_external) / "survivor"
    survivor.write_text("keep")
    pathlib.Path(RUN_OUTPUT).symlink_to(root_external)
    expect_red(classify_fixture)
    validator["require"](
        pathlib.Path(RUN_OUTPUT).is_symlink() and survivor.read_text() == "keep",
        "ambiguous root symlink was deleted or followed",
    )
    clear_fault_roots()

create_fault_roots("output")
startup_globals = validator["startup_root_record"].__globals__
original_getuid = startup_globals["os"].getuid
actual_uid = original_getuid()
startup_globals["os"].getuid = lambda: actual_uid + 1
try:
    expect_red(classify_fixture)
finally:
    startup_globals["os"].getuid = original_getuid
validator["require"](os.path.lexists(RUN_OUTPUT), "non-owned root was deleted")
clear_fault_roots()

with tempfile.TemporaryDirectory(dir="/tmp") as tmp:
    survivor = pathlib.Path(tmp) / "survivor"
    survivor.write_text("keep")
    process_ids = {}
    group_stopped = {}

    def process_tree_failure(guard: dict[str, Any]) -> None:
        for root_name in (RUN_OUTPUT, RUN_TARGET):
            root = pathlib.Path(root_name)
            root.mkdir(mode=0o700)
            (root / "build-state").write_text("temporary")
            (root / "outside-link").symlink_to(survivor)
        copy = pathlib.Path(RUN_TEMP) / "copy"
        copy.mkdir()
        (copy / "build-state").write_text("temporary")
        (pathlib.Path(RUN_TEMP) / "outside-link").symlink_to(survivor)
        pid_path = pathlib.Path(RUN_TEMP) / "grandchild.pid"
        code = (
            "import pathlib,subprocess,sys,time; "
            "child=subprocess.Popen([sys.executable,'-c','import time; time.sleep(60)']); "
            f"pathlib.Path({str(pid_path)!r}).write_text(str(child.pid)); "
            "time.sleep(60)"
        )
        monitor_globals = validator["monitor_process"].__globals__
        original_probe = monitor_globals["quick_worktree_probe"]
        original_stop = monitor_globals["stop_process"]

        def post_spawn(process: subprocess.Popen[object]) -> None:
            process_ids["leader"] = process.pid
            deadline = time.monotonic() + 5
            while not pid_path.exists() and time.monotonic() < deadline:
                time.sleep(0.01)
            validator["require"](pid_path.exists(), "grandchild pid was not published")
            process_ids["grandchild"] = int(pid_path.read_text())

        def recording_stop(candidate: subprocess.Popen[object]) -> int:
            returncode = original_stop(candidate)
            group_stopped["value"] = not validator["process_group_exists"](
                candidate.pid
            )
            return returncode

        monitor_globals["quick_worktree_probe"] = injected_monitor_failure
        monitor_globals["stop_process"] = recording_stop
        try:
            validator["owned_process"](
                [sys.executable, "-c", code],
                pathlib.Path("/tmp").resolve(strict=True),
                dict(os.environ),
                {"path": None},
                guard,
                post_spawn,
            )
        finally:
            monitor_globals["quick_worktree_probe"] = original_probe
            monitor_globals["stop_process"] = original_stop

    expect_red(lambda: validator["guarded_signal_temp_root"](process_tree_failure))
    deadline = time.monotonic() + 2
    while any(process_exists(pid) for pid in process_ids.values()) and time.monotonic() < deadline:
        time.sleep(0.05)
    validator["require"](
        group_stopped.get("value") is True,
        "interrupted process group was not reaped",
    )
    validator["require"](
        not any(process_exists(pid) for pid in process_ids.values()),
        "interrupted child or grandchild remains",
    )
    validator["require"](not os.path.lexists(RUN_TEMP), "temp root remains")
    validator["require"](not os.path.lexists(RUN_OUTPUT), "output root remains")
    validator["require"](not os.path.lexists(RUN_TARGET), "target root remains")
    validator["require"](survivor.read_text() == "keep", "cleanup followed symlink")
    reservation = validator["reserve_run_temp_root"]()
    validator["create_run_temp_root"](reservation)
    validator["cleanup_run_temp_root"](reservation)

with tempfile.TemporaryDirectory(dir="/tmp") as tmp:
    survivor = pathlib.Path(tmp) / "survivor"
    survivor.write_text("keep")
    original_handlers = {
        item: signal.getsignal(item) for item in validator["HANDOFF_SIGNALS"]
    }
    original_mask = signal.pthread_sigmask(signal.SIG_BLOCK, set())
    process_ids = {}
    queued = set()

    def benign_handler(received: int, _frame: Any) -> None:
        validator["require"](
            signal.Signals(received) in validator["HANDOFF_SIGNALS"],
            "unexpected restored signal",
        )

    for item in validator["HANDOFF_SIGNALS"]:
        signal.signal(item, benign_handler)

    def pending_handoff_failure(guard: dict[str, Any]) -> None:
        create_settlement_roots()
        pid_path = pathlib.Path(RUN_TEMP) / "grandchild.pid"
        code = (
            "import pathlib,subprocess,sys,time; "
            "child=subprocess.Popen([sys.executable,'-c','import time; time.sleep(60)']); "
            f"pathlib.Path({str(pid_path)!r}).write_text(str(child.pid)); "
            "time.sleep(60)"
        )

        def post_spawn(process: subprocess.Popen[object]) -> None:
            process_ids["leader"] = process.pid
            deadline = time.monotonic() + 5
            while not pid_path.exists() and time.monotonic() < deadline:
                time.sleep(0.01)
            validator["require"](pid_path.exists(), "grandchild pid was not published")
            process_ids["grandchild"] = int(pid_path.read_text())
            for requested in validator["HANDOFF_SIGNALS"]:
                os.kill(os.getpid(), requested)
            queued.update(signal.sigpending())

        validator["owned_process"](
            [sys.executable, "-c", code],
            pathlib.Path("/tmp").resolve(strict=True),
            dict(os.environ),
            {"path": None},
            guard,
            post_spawn,
        )

    try:
        expect_red(
            lambda: validator["guarded_signal_temp_root"](
                pending_handoff_failure
            )
        )
        validator["require"](
            not validator["process_group_exists"](process_ids["leader"]),
            "supported-signal handoff left its process group alive",
        )
        validator["require"](
            validator["HANDOFF_SIGNALS"].issubset(queued),
            "SIGHUP, SIGINT, and SIGTERM were not all pending before handoff",
        )
        validator["require"](
            all(
                not os.path.lexists(path)
                for path in (RUN_OUTPUT, RUN_TARGET, RUN_TEMP)
            ),
            "post-spawn SIGHUP left a scratch root",
        )
        validator["require"](survivor.read_text() == "keep", "handoff cleanup widened")
        validator["require"](
            all(
                signal.getsignal(item) is benign_handler
                for item in validator["HANDOFF_SIGNALS"]
            ),
            "outer signal guard did not restore prior handlers",
        )
        validator["require"](
            signal.pthread_sigmask(signal.SIG_BLOCK, set()) == original_mask,
            "outer signal guard did not restore prior mask",
        )
    finally:
        signal.pthread_sigmask(signal.SIG_BLOCK, validator["HANDOFF_SIGNALS"])
        for item, previous in original_handlers.items():
            signal.signal(item, previous)
        signal.pthread_sigmask(signal.SIG_SETMASK, original_mask)

    reservation = validator["reserve_run_temp_root"]()
    validator["create_run_temp_root"](reservation)
    validator["cleanup_run_temp_root"](reservation)

with tempfile.TemporaryDirectory(dir="/tmp") as tmp:
    status_path = pathlib.Path(tmp) / "term-status.json"
    ready_path = pathlib.Path(tmp) / "ready"
    code = f"""
import json, pathlib, signal, time
status = pathlib.Path({str(status_path)!r})
ready = pathlib.Path({str(ready_path)!r})
blocked = signal.pthread_sigmask(signal.SIG_BLOCK, set())
payload = {{"hup_blocked": signal.SIGHUP in blocked, "int_blocked": signal.SIGINT in blocked, "term_blocked": signal.SIGTERM in blocked, "terminated": False}}
status.write_text(json.dumps(payload))
def terminate(_received, _frame):
    payload["terminated"] = True
    status.write_text(json.dumps(payload))
    raise SystemExit(0)
signal.signal(signal.SIGTERM, terminate)
ready.write_text("ready")
while True:
    time.sleep(1)
"""

    def graceful_term_failure(guard: dict[str, Any]) -> None:
        def post_spawn(_process: subprocess.Popen[object]) -> None:
            deadline = time.monotonic() + 5
            while not ready_path.exists() and time.monotonic() < deadline:
                time.sleep(0.01)
            validator["require"](ready_path.exists(), "TERM fixture was not ready")
            raise EvidenceError("inject graceful TERM teardown")

        validator["owned_process"](
            [sys.executable, "-c", code],
            pathlib.Path("/tmp").resolve(strict=True),
            dict(os.environ),
            {"path": None},
            guard,
            post_spawn,
        )

    expect_red(lambda: validator["guarded_signal_temp_root"](graceful_term_failure))
    payload = json.loads(status_path.read_text())
    validator["require"](
        payload == {
            "hup_blocked": False,
            "int_blocked": False,
            "term_blocked": False,
            "terminated": True,
        },
        "child did not inherit an unblocked mask or handle graceful TERM",
    )
    validator["require"](not os.path.lexists(RUN_TEMP), "TERM fixture temp remains")

reservation = validator["reserve_run_temp_root"]()


def primary_with_cleanup_failure() -> None:
    def fail_after_mode_drift() -> None:
        pathlib.Path(RUN_TEMP).chmod(0o755)
        raise EvidenceError("primary marker")

    try:
        validator["guarded_temp_root"](fail_after_mode_drift)
    except EvidenceError as error:
        validator["require"](str(error) == "primary marker", "primary error replaced")
        validator["require"](
            any("cleanup failed" in note for note in error.__notes__),
            "cleanup diagnostic note absent",
        )
        raise


expect_red(primary_with_cleanup_failure)
pathlib.Path(RUN_TEMP).chmod(0o700)
validator["cleanup_run_temp_root"](reservation)

with tempfile.TemporaryDirectory(dir="/tmp") as tmp:
    root = pathlib.Path(tmp)
    proxy = root / "cargo-proxy"
    proxy.write_text("#!/bin/sh\nexit 0\n")
    proxy.chmod(0o755)
    (root / "cargo").symlink_to(proxy.name)
    before = validator["launcher_file_record"]("cargo", {"PATH": str(root)})
    proxy.write_text("#!/bin/sh\nexit 1\n")
    after = validator["launcher_file_record"]("cargo", {"PATH": str(root)})
    expect_red(lambda: validator["require"](before == after, "shim hash drift"))

with tempfile.TemporaryDirectory(dir="/tmp") as tmp:
    root = pathlib.Path(tmp)
    nested = root / "outer" / "worktree"
    nested.mkdir(parents=True)
    cargo_dir = root / "outer" / ".cargo"
    cargo_dir.mkdir()
    (cargo_dir / "config.toml").write_text("[build]\njobs = 1\n")
    alias = scratch_alias(root / "outer" / "worktree")
    expect_red(lambda: validator["reject_external_cargo_configs"](alias))
    expect_red(
        lambda: validator["reject_cargo_hierarchy"](alias, must_exist=True)
    )

with tempfile.TemporaryDirectory(dir="/tmp") as tmp:
    cargo_dir = pathlib.Path(tmp) / ".cargo"
    cargo_dir.mkdir()
    (cargo_dir / "config.toml").write_text("[build]\njobs = 1\n")
    (cargo_dir / "config").write_text("[build]\njobs = 2\n")
    selected, shadowed = validator["select_cargo_config"](
        pathlib.Path(tmp), nested=True
    )
    validator["require"](
        selected == cargo_dir / "config" and shadowed,
        "Cargo extensionless precedence drift",
    )

parent = {"PATH": "/portable/bin", "HOME": "/portable/home", "LANG": "C"}
poisoned = parent | {
    "RUSTC": "/wrong/rustc",
    "RUSTC_WRAPPER": "/wrong/wrapper",
    "RUSTC_WORKSPACE_WRAPPER": "/wrong/workspace-wrapper",
    "CARGO_BUILD_RUSTC_WRAPPER": "/wrong/cargo-wrapper",
    "RUSTFLAGS": "--cfg wrong",
    "CARGO_ENCODED_RUSTFLAGS": "--cfg\x1fwrong",
    "SCCACHE_BUCKET": "wrong-cache",
    "OPENAI_" + "API_KEY": "x" * 20,
}
validator["require"](
    validator["build_run_environment"](parent)
    == validator["build_run_environment"](poisoned),
    "parent Cargo controls leaked into normalized run env",
)

expect_red(lambda: validator["require_run_after_receipt"]("2026-08-21T04:00:00Z", "2026-08-21T03:59:59Z"))
expect_red(
    lambda: validator["require_run_identity"](
        RUN_ID, "/tmp/wrong", RUN_TARGET, RUN_TEMP
    )
)
expect_red(lambda: validator["require_detached_head"](0, "refs/heads/not-detached"))
expect_red(lambda: validator["require_monitoring"]({"interval_ms": 1000, "probe_count": 2, "violation": {"clean": False}, "copied_workspace_configs": []}))
expect_red(lambda: validator["require_receipt_identity"]({"schema": "nika.mutation-testimonial.run.raw.v1", "receipt_commit": "wrong", "tested_commit": TESTED_COMMIT, "tested_tree": TESTED_TREE, "run_id": RUN_ID}, "expected"))
expect_red(lambda: validator["require_receipt_identity"]({"schema": "synthetic-without-process", "receipt_commit": "expected", "tested_commit": TESTED_COMMIT, "tested_tree": TESTED_TREE, "run_id": RUN_ID}, "expected"))
expect_red(lambda: validator["require_outcome_time_authority"]({"start_time": "2026-08-21T04:00:01Z", "end_time": "2026-08-21T04:00:02Z"}, {"start_time": "2026-08-21T04:00:00Z", "end_time": "2026-08-21T04:00:02Z"}, "2026-08-21T03:59:59Z"))

validator["require"](failures == 47, "one of 47 negative fixtures survived")
print("OK: validator green fixtures passed; 47/47 negative mutations rejected")
