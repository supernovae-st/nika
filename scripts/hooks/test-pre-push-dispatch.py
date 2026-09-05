#!/usr/bin/env python3
"""Exercise the real Lefthook dispatcher against disposable local Git remotes.

The Cargo fixture always fails: these tests prove gate reachability and refusal,
not Rust correctness. No real repository ref, hook configuration or gate lease
is changed, and no gate bypass environment variable is set.
"""

import os
from pathlib import Path
import shutil
import signal
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
LEFTHOOK = shutil.which("lefthook")
ZERO = "0" * 40


class PushDispatch(unittest.TestCase):
    def setUp(self):
        self.assertIsNotNone(LEFTHOOK, "install Lefthook; dispatch coverage cannot be skipped")
        self.scratch = tempfile.TemporaryDirectory(prefix="nika-push-dispatch-")
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name)
        self.repo = self.root / "repo"
        self.remote = self.root / "remote.git"
        self.repo.mkdir()
        home = self.root / "home"
        home.mkdir()
        fake_bin = self.root / "bin"
        fake_bin.mkdir()
        cargo = fake_bin / "cargo"
        cargo.write_text("#!/bin/sh\nprintf 'cargo reached\\n' > gate-observed\nexit 17\n")
        cargo.chmod(0o755)
        # An allowlist prevents inherited Git configuration, credentials or gate
        # opt-outs from changing the fixture's authority or expected verdict.
        self.env = {
            "PATH": str(fake_bin) + os.pathsep + os.environ.get("PATH", ""),
            "HOME": str(home),
            "TMPDIR": str(self.root),
            "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_CONFIG_GLOBAL": os.devnull,
            "GIT_TERMINAL_PROMPT": "0",
            "NO_COLOR": "1",
        }
        self.run_command("git", "init", "--bare", str(self.remote))
        self.run_command("git", "init", "-b", "main")
        self.run_command("git", "config", "user.name", "Nika Hook Test")
        self.run_command("git", "config", "user.email", "fixture@example.invalid")
        shutil.copyfile(ROOT / "lefthook.yml", self.repo / "lefthook.yml")
        shutil.copytree(ROOT / "scripts/hooks", self.repo / "scripts/hooks")
        if (ROOT / "scripts/pre-push").is_dir():
            shutil.copytree(ROOT / "scripts/pre-push", self.repo / "scripts/pre-push")
        self.run_command("git", "add", "lefthook.yml", "scripts")
        self.run_command("git", "commit", "-m", "fixture initial tree")
        self.run_command("git", "remote", "add", "origin", str(self.remote))
        self.run_command("git", "push", "-u", "origin", "main")
        self.sha = self.run_command("git", "rev-parse", "HEAD").stdout.strip()
        self.run_command(LEFTHOOK, "install")

    def run_command(self, *args, success=True, stdin=None):
        process = subprocess.Popen(
            args, cwd=self.repo, env=self.env, text=True,
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            start_new_session=True,
        )
        try:
            stdout, stderr = process.communicate(stdin, timeout=30)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.communicate(timeout=5)
            self.fail(f"fixture command exceeded its deadline: {args}")
        result = subprocess.CompletedProcess(args, process.returncode, stdout, stderr)
        if success:
            self.assertEqual(result.returncode, 0, stdout + stderr)
        return result

    def assert_gate_refused(self, result):
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual((self.repo / "gate-observed").read_text(), "cargo reached\n")
        self.assertFalse((self.repo / ".git/nika-pre-push.lock").exists())

    def test_tag_only_push_reaches_gate_and_refuses_failed_cargo(self):
        self.run_command("git", "tag", "test-candidate")
        result = self.run_command("git", "push", "origin", "refs/tags/test-candidate", success=False)
        self.assert_gate_refused(result)
        self.assertEqual(self.run_command("git", "ls-remote", "origin", "refs/tags/test-candidate").stdout, "")

    def test_manual_empty_ref_input_still_reaches_gate(self):
        result = self.run_command(LEFTHOOK, "run", "pre-push", success=False, stdin="")
        self.assert_gate_refused(result)

    def test_deletion_only_is_decided_by_the_ref_reader(self):
        self.run_command("git", "--git-dir", str(self.remote), "update-ref", "refs/heads/obsolete", self.sha)
        result = self.run_command("git", "push", "origin", ":refs/heads/obsolete")
        self.assertIn("deletion-only push", result.stdout + result.stderr)
        self.assertFalse((self.repo / "gate-observed").exists())
        self.assertEqual(self.run_command("git", "ls-remote", "origin", "refs/heads/obsolete").stdout, "")

    def test_mixed_tag_and_deletion_is_not_a_deletion_only_skip(self):
        self.run_command("git", "--git-dir", str(self.remote), "update-ref", "refs/heads/obsolete", self.sha)
        self.run_command("git", "tag", "test-mixed")
        result = self.run_command("git", "push", "origin", ":refs/heads/obsolete", "refs/tags/test-mixed", success=False)
        self.assert_gate_refused(result)
        self.assertEqual(self.run_command("git", "ls-remote", "origin", "refs/tags/test-mixed").stdout, "")
        self.assertIn(self.sha, self.run_command("git", "ls-remote", "origin", "refs/heads/obsolete").stdout)

    def test_force_guard_sees_refs_even_when_tree_has_no_difference(self):
        # A synthetic ref proposal to the installed hook, not a real force push.
        # The all-zero local SHA proposes deletion of protected main.
        hook = str(self.repo / ".git/hooks/pre-push")
        result = self.run_command(
            hook, "origin", str(self.remote), success=False,
            stdin=f"(delete) {ZERO} refs/heads/main {self.sha}\n",
        )
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("BLOCKED", result.stdout + result.stderr)
        self.assertFalse((self.repo / "gate-observed").exists())


if __name__ == "__main__":
    unittest.main()
