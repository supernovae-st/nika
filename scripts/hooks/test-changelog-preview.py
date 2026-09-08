#!/usr/bin/env python3
"""Hermetic post-commit preview tests; only local mock Git/git-cliff run.

--hook selects an older source for the same red-before-green suite. The outer
watchdog is test infrastructure, not evidence that the hook enforced its bound.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import selectors
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import unittest

HOOK = Path(__file__).with_name("changelog-preview.sh")
SCRATCH_ROOT = None
MEASUREMENTS = []
BASH = shutil.which("bash")
HEAD = shutil.which("head")
PS = shutil.which("ps")
WATCHDOG_SECONDS = 4.5
HOOK_BOUND_SECONDS = 4.0  # 3s work + 0.5s reap, with 0.5s scheduling margin.

MOCK = r'''
import json, os, pathlib, signal, subprocess, sys, time
root = pathlib.Path(os.environ["MOCK_ROOT"])
kind = pathlib.Path(sys.argv[0]).name
mode = os.environ.get("MOCK_MODE", "ordinary")
child = "--child" in sys.argv
record = {"pid": os.getpid(), "pgid": os.getpgrp(), "kind": "child" if child else kind,
          "args": sys.argv[1:]}
fd = os.open(root / "calls.jsonl", os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o600)
os.write(fd, (json.dumps(record) + "\n").encode())
os.close(fd)
if child:
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
    (root / "child-ready").write_text("ready")
    while True:
        time.sleep(60)
if kind == "git":
    if mode == "git-hang":
        signal.signal(signal.SIGTERM, signal.SIG_IGN)
        while True:
            time.sleep(60)
    if mode == "git-error":
        sys.exit(17)
    print("v1.2.3")
    sys.exit(0)
if mode in ("cliff-hang", "exit-open-child", "exit-closed-child"):
    kwargs = {}
    if mode == "exit-closed-child":
        kwargs = dict(stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    subprocess.Popen([sys.executable, __file__, "--child"], **kwargs)
    until = time.monotonic() + 1
    while not (root / "child-ready").exists():
        if time.monotonic() > until:
            raise RuntimeError("mock child never reached its ready marker")
        time.sleep(0.005)
if mode == "cliff-hang":
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
    while True:
        time.sleep(60)
if mode == "exit-open-child":
    sys.exit(0)
if mode == "cliff-error":
    print("partial preview")
    print("hidden tool diagnostic", file=sys.stderr)
    sys.exit(23)
for i in range(1, 36):
    print(f"preview {i:02d}")
print("hidden tool diagnostic", file=sys.stderr)
'''


def running(pid):
    result = subprocess.run(
        [PS, "-o", "stat=", "-p", str(pid)], capture_output=True, text=True, timeout=1,
    )
    state = result.stdout.strip()
    return bool(state) and not state.startswith("Z")


class PreviewTests(unittest.TestCase):
    def setUp(self):
        for program in (BASH, HEAD, PS):
            self.assertIsNotNone(program, "Bash, head and ps are required for these POSIX fixtures")
        self.scratch = tempfile.TemporaryDirectory(prefix="preview-test-", dir=SCRATCH_ROOT)
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name).resolve()
        self.bin = self.root / "bin"
        self.bin.mkdir()
        for name, target in (("head", HEAD), ("python3", sys.executable)):
            (self.bin / name).symlink_to(target)
        for name in ("git", "git-cliff"):
            file = self.bin / name
            file.write_text(f"#!{sys.executable}\n" + MOCK)
            file.chmod(0o755)
        (self.root / "cliff.toml").write_text("# mock configuration\n")
        self.env = {
            "PATH": str(self.bin), "MOCK_ROOT": str(self.root),
            "HOME": str(self.root), "TMPDIR": str(self.root),
            "LC_ALL": "C", "PYTHONDONTWRITEBYTECODE": "1",
        }

    def invoke(self, mode="ordinary"):
        env = dict(self.env, MOCK_MODE=mode)
        started = time.monotonic()
        process = subprocess.Popen(
            [BASH, str(HOOK)], cwd=self.root, env=env,
            stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            start_new_session=True,
        )
        streams = selectors.DefaultSelector()
        streams.register(process.stdout, selectors.EVENT_READ, "stdout")
        streams.register(process.stderr, selectors.EVENT_READ, "stderr")
        output = {"stdout": bytearray(), "stderr": bytearray()}
        watchdog = False
        try:
            # Do not poll/wait: the direct child reserves its process-group ID
            # until all observations and fixture cleanup signals are complete.
            while streams.get_map():
                remaining = WATCHDOG_SECONDS - (time.monotonic() - started)
                if remaining <= 0:
                    watchdog = True
                    break
                for key, _ in streams.select(min(remaining, 0.1)):
                    data = os.read(key.fd, 8192)
                    if data:
                        output[key.data].extend(data)
                    else:
                        streams.unregister(key.fileobj)
            elapsed = time.monotonic() - started
            calls_file = self.root / "calls.jsonl"
            calls = [json.loads(line) for line in calls_file.read_text().splitlines()] if calls_file.exists() else []
            until = time.monotonic() + 0.5
            alive = [call for call in calls if running(call["pid"])]
            while alive and time.monotonic() < until and not watchdog:
                time.sleep(0.01)
                alive = [call for call in calls if running(call["pid"])]
            # Snapshot BEFORE the independent watchdog/fixture cleanup can
            # mask a leaked child in the old hook.
            measurement = {
                "test": self.id().split(".")[-1], "mode": mode,
                "elapsed_seconds": round(elapsed, 6), "outer_watchdog_fired": watchdog,
                "calls": calls, "alive_before_fixture_cleanup": alive,
            }
            MEASUREMENTS.append(measurement)
        finally:
            streams.close()
            # Every test group is led by our unreaped direct child. On a
            # regression timeout, give the preview supervisor its TERM cleanup
            # opportunity before the fixture's final, same-group SIGKILL.
            if watchdog:
                try:
                    os.killpg(process.pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
                time.sleep(0.15)
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            except PermissionError:
                # Darwin may report EPERM for a group containing only its
                # zombie leader. Never mask an actually running mock child.
                if any(running(call["pid"]) for call in calls):
                    raise
            process.wait(timeout=1)
            process.stdout.close()
            process.stderr.close()
        measurement["exit_code"] = process.returncode
        return measurement, {key: value.decode(errors="replace") for key, value in output.items()}

    def assert_finished(self, result):
        self.assertFalse(result["outer_watchdog_fired"], "hook required the independent test watchdog")
        self.assertLess(result["elapsed_seconds"], HOOK_BOUND_SECONDS)
        self.assertEqual(result["exit_code"], 0)
        self.assertEqual(result["alive_before_fixture_cleanup"], [], "hook left mock children running")

    def test_ordinary_preview_is_stderr_only_and_thirty_lines(self):
        result, output = self.invoke()
        self.assert_finished(result)
        self.assertEqual(output["stdout"], "")
        self.assertEqual(output["stderr"], "\n[changelog-preview] unreleased since v1.2.3:\n" + "".join(f"preview {i:02d}\n" for i in range(1, 31)))
        cliff = next(call for call in result["calls"] if call["kind"] == "git-cliff")
        self.assertEqual(cliff["args"], ["--unreleased", "--tag", "v1.2.3", "--strip", "all"])

    def test_tool_error_keeps_partial_output_and_success(self):
        result, output = self.invoke("cliff-error")
        self.assert_finished(result)
        self.assertIn("partial preview\n", output["stderr"])
        self.assertNotIn("hidden tool diagnostic", output["stderr"])

    def test_git_error_preserves_fallback_tag(self):
        result, output = self.invoke("git-error")
        self.assert_finished(result)
        self.assertIn("unreleased since v0.80.0:", output["stderr"])
        cliff = next(call for call in result["calls"] if call["kind"] == "git-cliff")
        self.assertIn("v0.80.0", cliff["args"])

    def test_missing_git_cliff_skips_silently(self):
        (self.bin / "git-cliff").unlink()
        result, output = self.invoke()
        self.assert_finished(result)
        self.assertEqual(result["calls"], [])
        self.assertEqual(output, {"stdout": "", "stderr": ""})

    def test_missing_config_skips_silently(self):
        (self.root / "cliff.toml").unlink()
        result, output = self.invoke()
        self.assert_finished(result)
        self.assertEqual(result["calls"], [])
        self.assertEqual(output, {"stdout": "", "stderr": ""})

    def test_missing_python_skips_instead_of_running_unbounded(self):
        (self.bin / "python3").unlink()
        result, output = self.invoke()
        self.assert_finished(result)
        self.assertEqual(result["calls"], [])
        self.assertEqual(output, {"stdout": "", "stderr": ""})

    def test_hanging_git_describe_is_in_the_same_budget(self):
        result, output = self.invoke("git-hang")
        self.assertIn("3s budget reached; preview stopped", output["stderr"])
        self.assert_finished(result)
        self.assertEqual([call["kind"] for call in result["calls"]], ["git"])

    def test_hanging_cliff_and_child_are_killed_without_touching_another_group(self):
        sentinel = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)"], start_new_session=True)
        try:
            result, output = self.invoke("cliff-hang")
            self.assertIn("3s budget reached; preview stopped", output["stderr"])
            self.assertIsNone(sentinel.poll(), "preview killed an unrelated process group")
            self.assertTrue(any(call["kind"] == "child" for call in result["calls"]))
            self.assert_finished(result)
        finally:
            if sentinel.poll() is None:
                sentinel.terminate()
            sentinel.wait(timeout=1)

    def test_exited_cliff_with_child_holding_output_is_bounded(self):
        result, output = self.invoke("exit-open-child")
        self.assertIn("3s budget reached; preview stopped", output["stderr"])
        self.assertTrue(any(call["kind"] == "child" for call in result["calls"]))
        self.assert_finished(result)

    def test_success_still_cleans_child_with_closed_output(self):
        result, output = self.invoke("exit-closed-child")
        self.assertTrue(any(call["kind"] == "child" for call in result["calls"]))
        self.assert_finished(result)
        self.assertIn("preview 30\n", output["stderr"])


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--hook", type=Path, default=HOOK)
    parser.add_argument("--scratch-root", type=Path)
    parser.add_argument("--results", type=Path)
    options = parser.parse_args()
    HOOK = options.hook.resolve()
    SCRATCH_ROOT = options.scratch_root
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(PreviewTests)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    if options.results:
        options.results.write_text(json.dumps({"hook": str(HOOK), "hook_sha256": hashlib.sha256(HOOK.read_bytes()).hexdigest(), "test_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(), "measurement": "wall clock before Popen through output EOF or independent watchdog", "tests": result.testsRun, "failures": len(result.failures), "errors": len(result.errors), "measurements": MEASUREMENTS}, indent=2) + "\n")
    sys.exit(0 if result.wasSuccessful() else 1)
