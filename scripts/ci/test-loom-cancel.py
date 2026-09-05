#!/usr/bin/env python3
"""Exercise the real Loom gate against missing models and Cargo failures."""

import os
from pathlib import Path
import subprocess
import tempfile
import unittest


class LoomGateTests(unittest.TestCase):
    def test_discovery_and_execution_failures_are_not_green(self):
        gate = Path(__file__).with_name("check-loom-cancel.sh").resolve()
        for case, expected, calls in [
            ("missing", 1, 1),
            ("list-failed", 43, 1),
            ("test-failed", 7, 2),
            ("success", 0, 2),
        ]:
            with self.subTest(case=case), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                cargo = root / "cargo"
                cargo.write_text(
                    "#!/usr/bin/env python3\n"
                    "import os, pathlib, sys\n"
                    "assert sys.argv[1:7] == ['test', '--locked', '-p', 'nika-types', '--lib', 'loom_cancel']\n"
                    "assert '--cfg loom' in os.environ['RUSTFLAGS']\n"
                    "assert not any(k in os.environ for k in ['LOOM_MAX_PERMUTATIONS', 'LOOM_MAX_DURATION', 'LOOM_MAX_PREEMPTIONS', 'LOOM_CHECKPOINT_FILE'])\n"
                    "with pathlib.Path(os.environ['CALLS']).open('a') as f: f.write('call\\n')\n"
                    "case = os.environ['CASE']\n"
                    "if '--list' in sys.argv:\n"
                    "    if case == 'list-failed': sys.exit(43)\n"
                    "    if case != 'missing': print('cancel::loom_cancel::cancellation_publishes_preceding_payload: test')\n"
                    "else:\n"
                    "    assert sys.argv[7:] == ['--', '--include-ignored'], 'ignored models must execute'\n"
                    "    if case == 'test-failed': sys.exit(7)\n"
                )
                cargo.chmod(0o755)
                env = dict(os.environ, PATH=f"{root}:{os.environ['PATH']}",
                           CASE=case, CALLS=str(root / "calls"),
                           LOOM_MAX_PERMUTATIONS="1", LOOM_MAX_DURATION="0",
                           LOOM_MAX_PREEMPTIONS="0", LOOM_CHECKPOINT_FILE="missing")
                result = subprocess.run(["bash", str(gate)], env=env,
                                        text=True, capture_output=True, check=False)
                self.assertEqual(result.returncode, expected, result.stderr)
                self.assertEqual(len((root / "calls").read_text().splitlines()), calls)


if __name__ == "__main__":
    unittest.main()
