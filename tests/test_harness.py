#!/usr/bin/env python3
"""Negative controls for the safe Linux harness; runs real subprocesses."""
import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile

sys.dont_write_bytecode = True
import linux_integration as integration


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary")
    args = parser.parse_args()
    integration.SCRATCH.mkdir(parents=True, exist_ok=True)
    root = Path(tempfile.mkdtemp(prefix="fbqueue-harness-controls-", dir=integration.SCRATCH))
    root.chmod(0o700)
    env = os.environ.copy()
    env["FBQUEUE_TEST_SCRATCH"] = str(root)
    command = [sys.executable, str(Path(__file__).with_name("linux_integration.py"))]
    if args.binary:
        command += ["--binary", args.binary]
    sentinel = subprocess.Popen(["sleep", "120"], start_new_session=True)
    checks = []
    try:
        broken = root / "broken-fbqueue"
        broken.write_text("#!/bin/sh\nexit 23\n")
        broken.chmod(0o700)
        controls = [
            ("broken binary", command + ["--binary", str(broken), "--only", "basic_explicit_cli_arguments_outputs_and_state"], "23 != 0"),
            ("failed assertion with active tree", command + ["--only", "running_cancel_terminates_owned_process_tree", "--inject-failure"], "deliberately injected assertion failure"),
        ]
        for name, argv, expected in controls:
            proc = subprocess.run(argv, env=env, capture_output=True, text=True, timeout=30)
            output = proc.stdout + proc.stderr
            (root / (name.replace(" ", "_") + ".log")).write_text(output)
            if proc.returncode != 1:
                raise AssertionError((name, proc.returncode, output))
            if expected not in output:
                raise AssertionError((name, output))
            if sentinel.poll() is not None:
                raise AssertionError("unrelated sentinel was killed")
            checks.append(name)
            print("PASS: %s returns 1; unrelated sentinel alive" % name, flush=True)
        # Interrupt while a real delayed job keeps its daemon alive.
        before = set(root.glob("fbqueue-linux-*"))
        with (root / "interruption.log").open("w") as log:
            proc = subprocess.Popen(command + ["--only", "delay_keeps_daemon_alive_then_idle_shutdown"],
                                    env=env, stdout=log, stderr=subprocess.STDOUT)
            try:
                def active_daemon():
                    for run in set(root.glob("fbqueue-linux-*")) - before:
                        for token_file in run.glob("*/ownership-token"):
                            state = token_file.parent / "state"
                            if (state / "run/daemon.pid").exists() and list((state / "queue/new").glob("*.job")):
                                return True
                    return False
                integration.poll(active_daemon, "negative-control daemon active", 10)
                proc.send_signal(signal.SIGTERM)
                if proc.wait(timeout=15) == 0:
                    raise AssertionError("interrupted harness incorrectly returned success")
            finally:
                if proc.poll() is None:
                    proc.kill()
                    proc.wait()
        if sentinel.poll() is not None:
            raise AssertionError("unrelated sentinel was killed during interruption")
        checks.append("SIGTERM cleanup")
        print("PASS: SIGTERM returns nonzero; unrelated sentinel alive", flush=True)
        tokens = [p.read_text() for p in root.glob("fbqueue-linux-*/*/ownership-token")]
        for token in tokens:
            integration.poll(lambda: not integration.owned(token), "negative-control owned processes cleaned", 5)
        checks.append("no leaked owned processes")
        print("PASS: all %d case ownership tokens have no live processes" % len(tokens), flush=True)
        (root / "result.json").write_text(json.dumps({"checks": checks, "passed": len(checks)}, indent=2))
        print("SUMMARY: %d negative-control checks passed; logs: %s" % (len(checks), root), flush=True)
        return 0
    finally:
        # Controls are tests too: never strand owned work if a control fails.
        for token_file in root.glob("fbqueue-linux-*/*/ownership-token"):
            token = token_file.read_text()
            pidfile = token_file.parent / "state/run/daemon.pid"
            if pidfile.exists() and pidfile.read_text().strip().isdigit():
                pid = int(pidfile.read_text())
                if pid in [p for p, _ in integration.owned(token)]:
                    integration.signal_owned(pid, token, signal.SIGKILL)
            def cleaned():
                for pid, _ in integration.owned(token):
                    integration.signal_owned(pid, token, signal.SIGKILL)
                integration.reap()
                return not integration.owned(token)
            integration.poll(cleaned, "control final cleanup", 5)
        sentinel.terminate()
        sentinel.wait(timeout=5)


if __name__ == "__main__":
    sys.exit(main())
