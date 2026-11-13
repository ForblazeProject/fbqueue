#!/usr/bin/env python3
"""Regression: optimized Python must not erase negative-control verdicts."""
import os
from pathlib import Path
import subprocess
import sys
import unittest


class OptimizedVerdictTests(unittest.TestCase):
    def test_multiple_subtest_failures_count_one_failed_case(self):
        source = r'''
import sys
import linux_integration as integration
def failures(self):
    for index in range(2):
        with self.subTest(index=index):
            self.fail("intentional subtest failure")
integration.SafeCase.test_basic_explicit_cli_arguments_outputs_and_state = failures
sys.argv = ["linux_integration.py", "--only", "basic_explicit_cli_arguments_outputs_and_state"]
raise SystemExit(integration.main())
'''
        proc = subprocess.run(
            [sys.executable, "-B", "-c", source],
            cwd=Path(__file__).resolve().parent,
            env=os.environ.copy(), capture_output=True, text=True, timeout=15,
        )
        self.assertEqual(proc.returncode, 1, proc.stdout + proc.stderr)
        self.assertIn("1 cases; 0 passed", proc.stdout)
        self.assertIn("1 failed cases", proc.stdout)
        self.assertIn("2 failure records", proc.stdout)

    def test_unexpected_success_is_rejected_before_the_next_control(self):
        # Only the control's child-run result is substituted. The unrelated
        # sentinel is real and main's finally block must stop/reap it normally.
        source = r'''
import sys
from types import SimpleNamespace
from unittest.mock import patch
import test_harness
calls = 0
def unexpected_success(*args, **kwargs):
    global calls
    calls += 1
    if calls > 1:
        raise RuntimeError("advanced past an invalid negative control")
    return SimpleNamespace(returncode=0, stdout="23 != 0", stderr="")
sys.argv = ["test_harness.py"]
with patch.object(test_harness.subprocess, "run", unexpected_success):
    try:
        test_harness.main()
    except AssertionError as error:
        if "broken binary" not in str(error):
            raise
        print("invalid negative control rejected under optimized Python")
    else:
        raise RuntimeError("negative control accepted unexpected success")
'''
        proc = subprocess.run(
            [sys.executable, "-B", "-O", "-c", source],
            cwd=Path(__file__).resolve().parent,
            env=os.environ.copy(), capture_output=True, text=True, timeout=15,
        )
        self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
        self.assertIn("invalid negative control rejected", proc.stdout)


if __name__ == "__main__":
    unittest.main(verbosity=2)
