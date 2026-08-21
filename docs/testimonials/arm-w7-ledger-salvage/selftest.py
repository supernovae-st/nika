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
    except (EvidenceError, KeyError):
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

with tempfile.TemporaryDirectory(dir="/tmp") as tmp:
    survivor = pathlib.Path(tmp) / "survivor"
    survivor.write_text("keep")
    process_ids = {}
    group_stopped = {}

    def process_tree_failure() -> None:
        copy = pathlib.Path(RUN_TEMP) / "copy"
        copy.mkdir()
        (copy / "build-state").write_text("temporary")
        (pathlib.Path(RUN_TEMP) / "outside-link").symlink_to(survivor)
        code = (
            "import subprocess,sys,time; "
            "child=subprocess.Popen([sys.executable,'-c','import time; time.sleep(60)']); "
            "print(child.pid,flush=True); time.sleep(60)"
        )
        process = subprocess.Popen(
            [sys.executable, "-c", code],
            stdout=subprocess.PIPE,
            text=True,
            start_new_session=True,
        )
        process_ids["leader"] = process.pid
        validator["require"](process.stdout is not None, "fixture stdout absent")
        process_ids["grandchild"] = int(process.stdout.readline().strip())
        monitor_globals = validator["monitor_process"].__globals__
        original_probe = monitor_globals["quick_worktree_probe"]
        original_stop = monitor_globals["stop_process"]

        def recording_stop(candidate: subprocess.Popen[object]) -> int:
            returncode = original_stop(candidate)
            group_stopped["value"] = not validator["process_group_exists"](
                candidate.pid
            )
            return returncode

        monitor_globals["quick_worktree_probe"] = injected_monitor_failure
        monitor_globals["stop_process"] = recording_stop
        try:
            validator["monitor_process"](process, {"path": None})
        finally:
            monitor_globals["quick_worktree_probe"] = original_probe
            monitor_globals["stop_process"] = original_stop

    expect_red(lambda: validator["guarded_temp_root"](process_tree_failure))
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

    for requested in (signal.SIGINT, signal.SIGTERM):
        process_ids = {}

        def pending_handoff_failure(
            requested: signal.Signals = requested,
            process_ids: dict[str, int] = process_ids,
        ) -> None:
            pid_path = pathlib.Path(RUN_TEMP) / "grandchild.pid"
            code = (
                "import pathlib,subprocess,sys,time; "
                "child=subprocess.Popen([sys.executable,'-c','import time; time.sleep(60)']); "
                f"pathlib.Path({str(pid_path)!r}).write_text(str(child.pid)); "
                "time.sleep(60)"
            )

            def post_spawn(
                process: subprocess.Popen[object],
                requested: signal.Signals = requested,
                process_ids: dict[str, int] = process_ids,
            ) -> None:
                process_ids["leader"] = process.pid
                deadline = time.monotonic() + 5
                while not pid_path.exists() and time.monotonic() < deadline:
                    time.sleep(0.01)
                validator["require"](pid_path.exists(), "grandchild pid was not published")
                process_ids["grandchild"] = int(pid_path.read_text())
                os.kill(os.getpid(), requested)

            validator["owned_process"](
                [sys.executable, "-c", code],
                pathlib.Path("/tmp").resolve(strict=True),
                dict(os.environ),
                {"path": None},
                post_spawn,
            )

        expect_red(lambda: validator["guarded_temp_root"](pending_handoff_failure))
        deadline = time.monotonic() + 2
        while (
            any(process_exists(pid) for pid in process_ids.values())
            and time.monotonic() < deadline
        ):
            time.sleep(0.05)
        validator["require"](
            not any(process_exists(pid) for pid in process_ids.values()),
            f"{requested.name} handoff left a descendant",
        )
        validator["require"](not os.path.lexists(RUN_TEMP), "handoff temp remains")
        validator["require"](survivor.read_text() == "keep", "handoff cleanup widened")

    validator["require"](
        {item: signal.getsignal(item) for item in validator["HANDOFF_SIGNALS"]}
        == original_handlers,
        "handoff signal handlers were not restored",
    )
    validator["require"](
        signal.pthread_sigmask(signal.SIG_BLOCK, set()) == original_mask,
        "handoff signal mask was not restored",
    )
    reservation = validator["reserve_run_temp_root"]()
    validator["create_run_temp_root"](reservation)
    validator["cleanup_run_temp_root"](reservation)

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

validator["require"](failures == 33, "one of 33 negative fixtures survived")
print("OK: validator green fixtures passed; 33/33 negative mutations rejected")
