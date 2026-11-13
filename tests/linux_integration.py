#!/usr/bin/env python3
"""Safe real-binary Linux integration tests; Python standard library only.

Every case gets a fresh HOME, FBQUEUE_DIR and ownership token. Cleanup signals
only processes whose /proc environment contains that exact token, using pidfds
(to avoid PID reuse), and reaps adopted descendants as a Linux subreaper.
"""
import argparse
import concurrent.futures
import ctypes
import json
import os
from pathlib import Path
import re
import select
import shutil
import signal
import subprocess
import sys
import tarfile
import tempfile
import time
import unittest
import uuid

REPO = Path(__file__).resolve().parents[1]
SCRATCH = Path(os.environ.get("FBQUEUE_TEST_SCRATCH", tempfile.gettempdir()))
TOKEN_KEY = "FBQUEUE_INTEGRATION_TOKEN"
BINARY = None
ROOT = None
KEEP = False
INJECT = False


def poll(predicate, description, timeout=12):
    deadline = time.monotonic() + timeout
    while True:
        value = predicate()
        if value:
            return value
        if time.monotonic() >= deadline:
            raise AssertionError("Timed out after %ss: %s" % (timeout, description))
        time.sleep(min(0.05, max(0, deadline - time.monotonic())))


def reap():
    while True:
        try:
            if os.waitpid(-1, os.WNOHANG)[0] == 0:
                break
        except ChildProcessError:
            break


def owned(token):
    """Read-only discovery; never select processes by global command patterns."""
    marker = (TOKEN_KEY + "=" + token).encode()
    found = []
    for path in Path("/proc").iterdir():
        if not path.name.isdigit():
            continue
        try:
            if marker in (path / "environ").read_bytes().split(b"\0"):
                stat = (path / "stat").read_text().rsplit(")", 1)[1].split()
                if stat[0] != "Z":
                    found.append((int(path.name), stat[19]))
        except (FileNotFoundError, PermissionError, ProcessLookupError):
            pass
    return found


def signal_owned(pid, token, sig):
    """Open pidfd first, then verify exact ownership; signal pinned process."""
    try:
        fd = os.pidfd_open(pid)
    except ProcessLookupError:
        return
    try:
        marker = (TOKEN_KEY + "=" + token).encode()
        try:
            env = Path("/proc/%s/environ" % pid).read_bytes().split(b"\0")
        except FileNotFoundError:
            return
        if marker not in env:
            if select.select([fd], [], [], 0)[0]:
                return  # The pinned process exited between discovery and verification.
            raise RuntimeError("Refusing to signal unowned PID %s" % pid)
        try:
            signal.pidfd_send_signal(fd, sig)
        except ProcessLookupError:
            pass
    finally:
        os.close(fd)


class SafeCase(unittest.TestCase):
    def setUp(self):
        assert ROOT is not None and BINARY is not None
        self.case = ROOT / self._testMethodName
        self.case.mkdir()
        self.work = self.case / "work"
        self.work.mkdir()
        self.state = self.case / "state"
        self.state.mkdir()
        home = self.case / "home"
        home.mkdir()
        self.token = uuid.uuid4().hex
        (self.case / "ownership-token").write_text(self.token)
        self.env = {k: v for k, v in os.environ.items() if not k.startswith("PBS_")}
        self.env.update(HOME=str(home), FBQUEUE_DIR=str(self.state),
                        FBQUEUE_INTEGRATION_TOKEN=self.token, USER="fbqueue_test_user",
                        FBQUEUE_TEST_VALUE="spaces = preserved", LC_ALL="C")
        self.bin = self.case / "bin"
        self.bin.mkdir()
        for name in ("fbqueue", "qsub", "qstat", "qdel"):
            (self.bin / name).symlink_to(BINARY)
        self.env["PATH"] = str(self.bin) + os.pathsep + os.environ.get("PATH", "/usr/bin:/bin")
        self.log = []
        self.configure()
        self.addCleanup(self.cleanup)

    def configure(self, capacity=8, extra="", idle=30, history=100):
        (self.state / "config").write_text(
            "capacity: %d\ndefault_queue: batch\ninactivity_timeout: %d\n"
            "history_limit: %d\narchive_interval_days: 30\nqueue: batch\n"
            "  capacity: %d\n  priority: 10\n%s" %
            (capacity, idle, history, capacity, extra))

    def cli(self, *args, tool="fbqueue", rc=0, env=None):
        command = [str(self.bin / tool), *map(str, args)]
        proc = subprocess.run(command, cwd=self.work, env=env or self.env,
                              capture_output=True, text=True, timeout=8)
        self.log.append({"argv": command, "rc": proc.returncode,
                         "stdout": proc.stdout, "stderr": proc.stderr})
        self.assertEqual(proc.returncode, rc, self.log[-1])
        return proc.stdout

    def submit(self, *args, explicit=False, env=None):
        output = self.cli(*(("sub",) + args if explicit else args),
                          tool="fbqueue" if explicit else "qsub", env=env)
        self.assertRegex(output, r"^\d+\.master\n$", "submission must print only ID")
        return output.strip().split(".")[0]

    def script(self, name, text):
        path = self.work / name
        path.write_text(text)
        path.chmod(0o644)
        return name

    def path(self, state, jid):
        return self.state / "queue" / state / (jid + ".job")

    def wait_state(self, jid, state, timeout=12):
        path = self.path(state, jid)
        poll(path.exists, "%s -> %s" % (jid, state), timeout)
        return path.read_text()

    def metadata(self, jid, state="done"):
        text = self.wait_state(jid, state)
        return dict(line.split(": ", 1) for line in text.splitlines() if ": " in line)

    def read(self, name):
        return (self.work / name).read_text()

    def gate(self, name="gate.sh"):
        return self.script(name, '#!/bin/bash\nwhile [ ! -f release ]; do sleep 0.05; done\n')

    def release(self):
        (self.work / "release").touch()

    def daemon_pid(self):
        path = self.state / "run/daemon.pid"
        poll(lambda: path.exists() and path.read_text().strip().isdigit(), "daemon PID")
        pid = int(path.read_text())
        self.assertIn(pid, [p for p, _ in owned(self.token)])
        return pid

    def live(self, pid):
        return pid in [p for p, _ in owned(self.token)]

    def cleanup(self):
        # Stop scheduler before workers so it cannot launch replacements.
        pidfile = self.state / "run/daemon.pid"
        try:
            if pidfile.exists() and pidfile.read_text().strip().isdigit():
                pid = int(pidfile.read_text())
                if pid in [p for p, _ in owned(self.token)]:
                    signal_owned(pid, self.token, signal.SIGKILL)
            deadline = time.monotonic() + 5
            while True:
                processes = owned(self.token)
                for pid, _ in processes:
                    signal_owned(pid, self.token, signal.SIGKILL)
                reap()
                if not owned(self.token):
                    break
                if time.monotonic() > deadline:
                    raise RuntimeError("Owned processes survived cleanup: %r" % owned(self.token))
                time.sleep(0.02)
            reap()
        finally:
            # Job files capture environment, so diagnostics redact it.
            diagnostic = {"commands": self.log, "files": {}}
            for path in self.state.rglob("*.job"):
                diagnostic["files"][str(path.relative_to(self.case))] = "\n".join(
                    line for line in path.read_text().splitlines() if not line.startswith("env: "))
            (self.case / "diagnostics.json").write_text(json.dumps(diagnostic, indent=2))

    def test_cli_help_unknown_and_missing_command(self):
        self.assertIn("Usage:", self.cli("--help"))
        self.cli("not-a-command", rc=1)
        for tool, args, usage in (("fbqueue", ("sub",), "fbqueue sub"), ("qsub", (), "qsub")):
            self.assertEqual(self.cli(*args, tool=tool, rc=2), "")
            self.assertIn("No command specified", self.log[-1]["stderr"])
            self.assertIn("Usage: " + usage, self.log[-1]["stderr"])
            self.assertIn("--help", self.log[-1]["stderr"])
            self.assertIn("Usage: " + usage, self.cli(*args, "--help", tool=tool))
        self.assertFalse((self.state / "run/last_id").exists())
        self.assertFalse((self.state / "run/daemon.pid").exists())
        self.assertEqual(list(self.state.glob("queue/new/*.job")), [])

    def test_submission_usage_errors_do_not_register_jobs(self):
        for option in ("-c", "--cost", "-q", "--queue", "-N", "-J", "-W",
                       "-hold_jid", "-a", "-o", "-e", "--range", "--list",
                       "--not-an-option"):
            with self.subTest(option=option):
                self.assertEqual(self.cli(option, tool="qsub", rc=2), "")
                self.assertIn("Error:", self.log[-1]["stderr"])
        for args in (("",), ("--",), ("--range", "3-1", "true"),
                     ("--range", "bad", "true"), ("--list", "", "true")):
            with self.subTest(args=args):
                self.assertEqual(self.cli(*args, tool="qsub", rc=2), "")
        self.assertFalse((self.state / "run/last_id").exists())
        self.assertFalse((self.state / "run/daemon.pid").exists())
        self.assertEqual(list(self.state.glob("queue/*/*.job")), [])

    def test_first_submission_initializes_default_batch_queue(self):
        (self.state / "config").unlink()
        jid = self.submit("-N", "bootstrap", "echo", "first run")
        self.assertEqual(jid, "1")
        meta = self.metadata(jid)
        self.assertEqual(meta["queue"], "batch")
        self.assertEqual(meta["cost"], "1")
        self.assertEqual(self.read("bootstrap.o" + jid), "first run\n")
        self.assertIn("default_queue: batch", (self.state / "config").read_text())

    def test_first_submission_accepts_explicit_batch_queue(self):
        (self.state / "config").unlink()
        jid = self.submit("-q", "batch", "-N", "bootstrap", "true")
        self.assertEqual(jid, "1")
        self.assertEqual(self.metadata(jid)["queue"], "batch")

    def test_basic_explicit_cli_arguments_outputs_and_state(self):
        jid = self.submit("-N", "basic", "echo", "hello", "two words", explicit=True)
        meta = self.metadata(jid)
        self.assertEqual(self.read("basic.o" + jid), "hello two words\n")
        self.assertEqual(self.read("basic.e" + jid), "")
        self.assertEqual(meta["status"], "DONE")
        self.assertEqual(meta["exit_code"], "0")
        self.assertLessEqual(int(meta["start_at"]), int(meta["end_at"]))
        self.assertFalse(self.path("running", jid).exists())
        self.assertFalse(self.path("new", jid).exists())

    def test_nonexecutable_bash_shebang_with_arguments(self):
        script = self.script("bash.sh", '#!/bin/bash -eu\na=(bash interpreter)\nprintf "%s:%s\\n" "${a[*]}" "$1"\n')
        jid = self.submit("-N", "bash", script, "argument with spaces")
        self.wait_state(jid, "done")
        self.assertEqual(self.read("bash.o" + jid), "bash interpreter:argument with spaces\n")
        self.assertEqual((self.work / script).stat().st_mode & 0o777, 0o644)

    def test_no_shebang_shell_fallback(self):
        script = self.script("plain.sh", 'printf "fallback\\n"\n')
        jid = self.submit("-N", "plain", script)
        self.wait_state(jid, "done")
        self.assertEqual(self.read("plain.o" + jid), "fallback\n")

    def test_pbs_directives_environment_and_four_line_nodefile(self):
        body = '''#!/bin/bash
#PBS -N PbsName
#PBS -q batch
#PBS -l ncpus=4
#PBS -o pbs.out
#PBS -e pbs.err
python3 - <<'PY'
import json, os
from pathlib import Path
keys = ['PBS_JOBID','PBS_JOBNAME','PBS_QUEUE','PBS_ENVIRONMENT','PBS_O_WORKDIR','PBS_O_HOST','PBS_O_LOGNAME','PBS_NODEFILE','FBQUEUE_TEST_VALUE']
data = {k: os.environ.get(k) for k in keys}
data['node_lines'] = Path(os.environ['PBS_NODEFILE']).read_text().splitlines()
data['cwd'] = os.getcwd()
Path('environment.json').write_text(json.dumps(data))
PY
printf 'stdout\\n'
printf 'stderr\\n' >&2
'''
        jid = self.submit(self.script("pbs.sh", body))
        self.wait_state(jid, "done")
        data = json.loads(self.read("environment.json"))
        expected = dict(PBS_JOBID=jid, PBS_JOBNAME="PbsName", PBS_QUEUE="batch",
                        PBS_ENVIRONMENT="PBS_BATCH", PBS_O_WORKDIR=str(self.work),
                        PBS_O_LOGNAME="fbqueue_test_user", FBQUEUE_TEST_VALUE="spaces = preserved",
                        cwd=str(self.work), PBS_O_HOST=subprocess.check_output(["hostname"], text=True).strip())
        for key, value in expected.items():
            self.assertEqual(data[key], value, key)
        self.assertEqual(data["node_lines"], [expected["PBS_O_HOST"]] * 4)
        self.assertEqual(self.metadata(jid)["cost"], "4")
        self.assertFalse(Path(data["PBS_NODEFILE"]).exists())
        self.assertEqual(self.read("pbs.out"), "stdout\n")
        self.assertEqual(self.read("pbs.err"), "stderr\n")

    def test_submission_environment_replayed_into_existing_daemon(self):
        initial = self.submit("-N", "initial", "true")
        self.wait_state(initial, "done")
        daemon = self.daemon_pid()
        script = self.script("env.sh", '#!/bin/sh\nprintf "%s\\n" "$FBQUEUE_TEST_VALUE"\n')
        env = self.env.copy()
        env["FBQUEUE_TEST_VALUE"] = "captured after daemon startup = different value"
        jid = self.submit("-N", "replay", script, env=env)
        self.wait_state(jid, "done")
        self.assertEqual(self.daemon_pid(), daemon)
        self.assertEqual(self.read("replay.o" + jid), env["FBQUEUE_TEST_VALUE"] + "\n")

    def test_cli_overrides_script_directives(self):
        script = self.script("directives.sh", '#!/bin/sh\n#PBS -N scriptname\n#PBS -l nodes=1:ppn=2\n#PBS -o ignored.out\necho overridden\n')
        jid = self.submit("-N", "cli", "-c", "4", "-o", "chosen.out", script)
        meta = self.metadata(jid)
        self.assertEqual((meta["name"], meta["cost"]), ("cli", "4"))
        self.assertEqual(self.read("chosen.out"), "overridden\n")
        self.assertFalse((self.work / "ignored.out").exists())

    def test_valid_cli_cost_overrides_invalid_script_cost(self):
        directives = ("#PBS -l ncpus=0", "#PBS -l nodes=1:ppn=-2",
                      "#$ -pe smp wrong", "#SBATCH -c not-a-number")
        for index, directive in enumerate(directives):
            with self.subTest(directive=directive):
                script = self.script("override%d.sh" % index,
                                     "#!/bin/sh\n" + directive + "\ntrue\n")
                jid = self.submit("-N", "override", "-c", "1", script)
                self.assertEqual(self.metadata(jid)["cost"], "1")

    def test_sge_and_slurm_supported_directive_subset(self):
        for name, directive in (("sge", "#$ -pe smp 2"), ("slurm", "#SBATCH -c 3")):
            script = self.script(name + ".sh", "#!/bin/sh\n" + directive + "\necho ok\n")
            jid = self.submit("-N", name, script)
            self.assertEqual(self.metadata(jid)["cost"], "2" if name == "sge" else "3")

    def test_exit_code_failure_and_pbs_error_history(self):
        script = self.script("failure.sh", '#!/bin/sh\necho error >&2\nexit 7\n')
        jid = self.submit("-N", "failed", script)
        meta = self.metadata(jid, "failed")
        self.assertEqual((meta["status"], meta["exit_code"]), ("FAILED", "7"))
        self.assertEqual(self.read("failed.e" + jid), "error\n")
        self.assertRegex(self.cli(jid, tool="qstat"), r"\sE\s+batch")
        self.assertFalse((self.state / "run" / ("nodefile." + jid)).exists())

    def test_spawn_failure_is_failed_history_not_success(self):
        jid = self.submit("-N", "spawn_error", "fbqueue-nonexistent-test-executable-86f1")
        meta = self.metadata(jid, "failed")
        self.assertEqual(meta.get("status"), "FAILED")
        self.assertEqual(meta.get("exit_reason"), "spawn_failed")
        self.assertNotIn("exit_code", meta, "no process ran, so no process exit code exists")
        self.assertRegex(self.cli(jid, tool="qstat"), r"\sE\s+batch")
        self.assertFalse((self.state / "run" / ("nodefile." + jid)).exists())

    def test_id_filter(self):
        self.configure(capacity=1)
        script = self.gate()
        first = self.submit("-N", "FirstJob", script)
        self.wait_state(first, "running")
        second = self.submit("-N", "SecondJob", script)
        self.wait_state(second, "new")
        for jid, name, other, status in ((first, "FirstJob", "SecondJob", "R"),
                                          (second, "SecondJob", "FirstJob", "Q")):
            for token in (jid, jid + ".master"):
                output = self.cli(token, tool="qstat")
                self.assertIn(name, output)
                self.assertNotIn(other, output)
                self.assertRegex(output, r"\s" + status + r"\s+batch")
        output = self.cli("-u", "fbqueue_test_user", first, tool="qstat")
        self.assertIn("FirstJob", output)
        self.assertNotIn("FirstJob", self.cli("-u", "other-user", first, tool="qstat"))
        self.release()
        for jid in (first, second):
            self.wait_state(jid, "done")
        for token, name, other in ((first, "FirstJob", "SecondJob"),
                                   (second + ".master", "SecondJob", "FirstJob")):
            output = self.cli(token, tool="qstat")
            self.assertIn(name, output)
            self.assertNotIn(other, output)
            self.assertRegex(output, r"\sF\s+batch")
        self.assertNotIn("FirstJob", self.cli("99999", tool="qstat"))
        self.assertIn("FirstJob", self.cli("stat", first))

    def test_history_limit_user_filter_and_active_exclusion(self):
        ids = [self.submit("-N", "history" + str(i), "true") for i in range(3)]
        for jid in ids:
            self.wait_state(jid, "done")
        active = self.submit("-N", "active", self.gate())
        self.wait_state(active, "running")
        output = self.cli("-H", "2", tool="qstat")
        self.assertNotIn("history0", output)
        self.assertNotIn("active", output)
        self.assertEqual(len(re.findall(r"^\d+\.master", output, re.M)), 2)
        self.assertNotIn("history", self.cli("-H", "-u", "wrong", tool="qstat"))
        self.assertIn("history2", self.cli("-H", "-u", "fbqueue_test_user", tool="qstat"))
        self.release()

    def test_global_capacity_and_fifo_same_priority(self):
        self.configure(capacity=3, extra="queue: other\n  capacity: 3\n  priority: 10\n")
        first = self.submit("-q", "other", "-c", "2", "-N", "first", self.gate())
        self.wait_state(first, "running")
        record = self.script("record.sh", '#!/bin/sh\nprintf "%s\\n" "$1" >> order\n')
        second = self.submit("-c", "2", "-N", "second", record, "second")
        third = self.submit("-c", "2", "-N", "third", record, "third")
        self.wait_state(second, "new")
        self.wait_state(third, "new")
        observer = self.submit("-q", "other", "-N", "pass", "true")
        self.wait_state(observer, "done")
        self.assertTrue(self.path("new", second).exists())
        self.assertTrue(self.path("new", third).exists())
        self.assertIn("Capacity", self.cli("stat"))
        self.assertEqual(len(list((self.state / "queue/running").glob("*.job"))), 1)
        self.release()
        for jid in (first, second, third):
            self.wait_state(jid, "done")
        self.assertEqual(self.read("order").splitlines(), ["second", "third"])

    def test_documented_queue_capacity(self):
        self.configure(capacity=4, extra="queue: narrow\n  capacity: 1\n  priority: 10\n")
        gate = self.gate()
        first = self.submit("-q", "narrow", "-N", "first", gate)
        self.wait_state(first, "running")
        second = self.submit("-q", "narrow", "-N", "second", gate)
        # A separate queue's observer establishes that a scheduling pass occurred.
        observer = self.submit("-N", "observer", "true")
        self.wait_state(observer, "done")
        self.assertTrue(self.path("new", second).exists(),
                        "documented narrow queue capacity=1 must prevent a second concurrent job")

    def test_documented_queue_priority(self):
        self.configure(capacity=1, extra="queue: express\n  capacity: 1\n  priority: 100\n")
        blocker = self.submit("-N", "blocker", self.gate())
        self.wait_state(blocker, "running")
        low = self.submit("-N", "low", self.script("low.sh", "#!/bin/sh\necho low >> order\n"))
        high = self.submit("-q", "express", "-N", "high", self.script("high.sh", "#!/bin/sh\necho high >> order\n"))
        self.wait_state(low, "new")
        self.wait_state(high, "new")
        self.release()
        for jid in (low, high):
            self.wait_state(jid, "done")
        self.assertEqual(self.read("order").splitlines(), ["high", "low"],
                         "documented express priority=100 must precede older batch priority=10")

    def test_invalid_costs_and_capacity_reject_without_ids(self):
        self.configure(capacity=4, extra="queue: narrow\n  capacity: 1\n")
        self.cli("-c", tool="qsub", rc=2)
        for value in ("0", "-1", "abc", "1.5", "184467440737095516160", "5"):
            self.assertEqual(self.cli("-c", value, "true", tool="qsub", rc=2), "")
            self.assertIn("Error:", self.log[-1]["stderr"])
            self.assertRegex(self.log[-1]["stderr"], r"[Cc]ost|capacity")
        self.cli("-q", "narrow", "-c", "2", "true", tool="qsub", rc=2)
        self.cli("-q", "unknown", "true", tool="qsub", rc=2)
        self.assertFalse((self.state / "run/last_id").exists())
        self.assertFalse((self.state / "run/daemon.pid").exists())
        self.assertEqual(list(self.state.glob("queue/*/*.job")), [])
        jid = self.submit("-c", "4", "true")
        self.assertEqual(jid, "1")
        self.assertEqual(self.metadata(jid)["cost"], "4")

    def test_invalid_script_cost_and_batch_preflight(self):
        for index, directive in enumerate(("#PBS -l ncpus=0", "#PBS -l nodes=1:ppn=-2", "#$ -pe smp wrong", "#SBATCH -c 184467440737095516160", "#SBATCH -n 0", "#SBATCH -c", "#$ -pe smp")):
            script = self.script("invalid%d.sh" % index, "#!/bin/sh\n" + directive + "\ntrue\n")
            self.assertEqual(self.cli(script, tool="qsub", rc=2), "")
            self.assertIn("cost", self.log[-1]["stderr"].lower())
        self.script("batch1.sh", "#!/bin/sh\ntrue\n")
        self.script("batch2.sh", "#!/bin/sh\n#PBS -l ncpus=0\ntrue\n")
        for option, value in (("--range", "1-2"), ("--list", "1,2")):
            self.assertEqual(self.cli(option, value, "batch{}.sh", tool="qsub", rc=2), "")
            self.assertIn("cost", self.log[-1]["stderr"].lower())
        self.assertFalse((self.state / "run/last_id").exists())
        self.assertFalse((self.state / "run/daemon.pid").exists())
        self.assertEqual(list(self.state.glob("queue/*/*.job")), [])

    def test_successful_dependency_waits_then_runs(self):
        gate = self.gate()
        predecessor = self.submit("-N", "predecessor", gate)
        self.wait_state(predecessor, "running")
        successor = self.submit("-N", "successor", "-hold_jid", predecessor, "echo", "after")
        observer = self.submit("-N", "observer", "true")
        self.wait_state(observer, "done")
        self.assertTrue(self.path("new", successor).exists())
        self.assertIn("Dependency", self.cli("stat"))
        self.release()
        self.wait_state(successor, "done")
        self.assertLessEqual(int(self.metadata(predecessor)["end_at"]), int(self.metadata(successor)["start_at"]))
        self.assertEqual(self.read("successor.o" + successor), "after\n")

    def test_failed_dependency_remains_pending(self):
        predecessor = self.submit("-N", "bad", "false")
        self.wait_state(predecessor, "failed")
        successor = self.submit("-N", "held", "-hold_jid", predecessor, "true")
        observer = self.submit("-N", "observer", "true")
        self.wait_state(observer, "done")
        self.assertTrue(self.path("new", successor).exists())
        self.assertFalse((self.work / ("held.o" + successor)).exists())

    def test_pending_cancel_retains_history_and_master_id(self):
        jid = self.submit("-N", "pending", "-a", "+60s", "true")
        original = self.wait_state(jid, "new")
        self.cli(jid + ".master", tool="qdel")
        meta = self.metadata(jid, "failed")
        self.assertTrue(self.path("failed", jid).read_text().startswith(original))
        self.assertEqual(meta["status"], "CANCELLED")
        self.assertIn("end_at", meta)
        self.assertNotIn("start_at", meta)
        self.assertNotIn("exit_code", meta)
        self.assertRegex(self.cli(jid, tool="qstat"), r"\sE\s+batch")
        self.assertFalse(self.path("new", jid).exists())
        self.cli("../escape", tool="qdel", rc=2)

    def test_repeated_and_late_cancellation_preserve_history(self):
        pending = self.submit("-N", "pending", "-a", "+60s", "true")
        original = self.wait_state(pending, "new")
        daemon = self.daemon_pid()
        signal_owned(daemon, self.token, signal.SIGSTOP)
        poll(lambda: Path("/proc/%s/stat" % daemon).read_text().rsplit(")", 1)[1].split()[0] in ("T", "t"),
             "owned daemon stopped for cancellation interleaving")
        try:
            self.cli(pending, tool="qdel")
            marker = self.path("cancel", pending)
            self.assertEqual(marker.read_text(), original)
            self.cli(pending, tool="qdel")
            self.assertEqual(marker.read_text(), original,
                             "a repeated cancellation must not truncate queued metadata")
        finally:
            signal_owned(daemon, self.token, signal.SIGCONT)
        self.assertEqual(self.metadata(pending, "failed")["status"], "CANCELLED")

        running = self.submit("-N", "running", self.gate())
        self.wait_state(running, "running")
        signal_owned(daemon, self.token, signal.SIGSTOP)
        poll(lambda: Path("/proc/%s/stat" % daemon).read_text().rsplit(")", 1)[1].split()[0] in ("T", "t"),
             "owned daemon stopped for cancellation interleaving")
        try:
            # Exercise the create-new interleaving deterministically: a complete
            # record is already in cancel while another caller sees running.
            marker = self.path("cancel", running)
            running_record = self.path("running", running).read_text()
            marker.write_text(running_record)
            self.cli(running, tool="qdel")
            self.assertEqual(marker.read_text(), running_record)
        finally:
            signal_owned(daemon, self.token, signal.SIGCONT)
        self.assertEqual(self.metadata(running, "failed")["status"], "CANCELLED")

        completed = self.submit("-N", "completed", "true")
        terminal = self.wait_state(completed, "done")
        # A request written after completion won the race must not replace it.
        self.path("cancel", completed).write_text("")
        observer = self.submit("-N", "observer", "true")
        self.wait_state(observer, "done")
        self.assertEqual(self.path("done", completed).read_text(), terminal)
        self.assertFalse(self.path("failed", completed).exists())
        self.assertFalse(self.path("cancel", completed).exists())

    def test_live_capacity_reduction_and_legacy_zero_cost(self):
        for reduction in ("global", "queue", "both"):
            with self.subTest(reduction=reduction):
                (self.work / "release").unlink(missing_ok=True)
                self.configure(capacity=4)
                initial = self.submit("-c", "2", self.gate())
                self.wait_state(initial, "running")
                pending = self.submit("-c", "2", "-a", "+60s", "true")
                pending_text = self.wait_state(pending, "new")
                if reduction == "global":
                    self.configure(capacity=1, extra="queue: batch\n  capacity: 4\n")
                elif reduction == "queue":
                    self.configure(capacity=4, extra="queue: batch\n  capacity: 1\n")
                else:
                    self.configure(capacity=1)
                self.path("new", pending).write_text("\n".join(
                    line for line in pending_text.splitlines() if not line.startswith("start_after:")) + "\n")
                self.release()
                self.wait_state(initial, "done")
                observer = self.submit("true")
                self.wait_state(observer, "done")
                self.assertTrue(self.path("new", pending).exists())
                self.configure(capacity=4)
                self.wait_state(pending, "done")
        # Simulate a historical zero-cost record. It must consume one slot.
        held = self.submit("-a", "+60s", self.gate("legacy.sh"))
        text = self.wait_state(held, "new").replace("cost: 1", "cost: 0")
        text = "\n".join(l for l in text.splitlines() if not l.startswith("start_after:")) + "\n"
        (self.work / "release").unlink()
        self.configure(capacity=2, extra="queue: observer\n  capacity: 1\n  priority: 100\n")
        self.path("new", held).write_text(text)
        self.wait_state(held, "running")
        other = self.submit("-c", "2", "true")
        obs = self.submit("-q", "observer", "true")
        self.wait_state(obs, "done")
        self.assertTrue(self.path("new", other).exists(), "legacy zero cost must consume one slot")
        self.release()
        self.wait_state(other, "done")

    def test_running_cancel_terminates_owned_process_tree(self):
        script = self.script("tree.sh", '#!/bin/bash\nsleep 120 &\nchild=$!\nprintf "%s %s\\n" "$$" "$child" > pids\nwait "$child"\n')
        jid = self.submit("-N", "tree", script)
        poll(lambda: (self.work / "pids").exists() and len(self.read("pids").split()) == 2, "tree PIDs")
        pids = list(map(int, self.read("pids").split()))
        for pid in pids:
            self.assertTrue(self.live(pid), "tree PID must carry ownership token")
        if INJECT:
            self.fail("deliberately injected assertion failure with live owned process tree")
        self.cli(jid, tool="qdel")
        self.assertEqual(self.metadata(jid, "failed")["status"], "CANCELLED")
        poll(lambda: not any(self.live(pid) for pid in pids), "cancelled parent and child exit")
        self.assertFalse((self.state / "run" / ("nodefile." + jid)).exists())
        self.assertRegex(self.cli(jid, tool="qstat"), r"\sE\s+batch")

    def test_walltime_timeout_state_and_nodefile_cleanup(self):
        jid = self.submit("-N", "timeout", "-W", "00:00:01", "sleep", "120")
        meta = self.metadata(jid, "failed")
        self.assertEqual(meta["status"], "TIMEOUT")
        self.assertEqual(meta["exit_reason"], "walltime_exceeded")
        self.assertGreater(int(meta["end_at"]) - int(meta["start_at"]), 1)
        self.assertFalse((self.state / "run" / ("nodefile." + jid)).exists())

    def test_delay_keeps_daemon_alive_then_idle_shutdown(self):
        self.configure(idle=1)
        jid = self.submit("-N", "delayed", "-a", "+4s", "echo", "delayed")
        pid = self.daemon_pid()
        self.assertIn("Wait until", self.cli("stat"))
        meta = self.metadata(jid)
        self.assertGreaterEqual(int(meta["start_at"]), int(meta["start_after"]))
        self.assertEqual(self.read("delayed.o" + jid), "delayed\n")
        poll(lambda: not (self.state / "run/daemon.pid").exists() and not self.live(pid), "idle shutdown")
        self.assertIn("not running", self.cli("daemon", "status"))

    def test_daemon_respects_existing_lifetime_lock(self):
        import fcntl
        (self.state / "run").mkdir(exist_ok=True)
        lock = self.state / "run/daemon.lock"
        with lock.open("a+") as held:
            fcntl.flock(held, fcntl.LOCK_EX | fcntl.LOCK_NB)
            self.cli("daemon", "start", self.state)
            self.assertFalse((self.state / "run/daemon.pid").exists())
            self.assertEqual(owned(self.token), [])

    def test_daemon_autostart_stop_and_restart(self):
        jid = self.submit("-N", "first", "true")
        self.wait_state(jid, "done")
        pid = self.daemon_pid()
        self.assertIn(str(pid), self.cli("daemon", "status"))
        lock_inode = (self.state / "run/daemon.lock").stat().st_ino
        # CLI stop only targets an identity already checked against our token.
        self.cli("daemon", "stop")
        poll(lambda: not self.live(pid), "daemon stop")
        self.assertFalse((self.state / "run/daemon.pid").exists())
        next_id = self.submit("-N", "next", "true")
        self.wait_state(next_id, "done")
        self.assertNotEqual(self.daemon_pid(), pid)
        self.assertEqual((self.state / "run/daemon.lock").stat().st_ino, lock_inode)

    def test_recovery_never_replays_surviving_worker_and_holds_dispatch(self):
        script = self.script("recovery.sh", '#!/bin/bash\necho "$$" >> launches\nwhile [ ! -f release ]; do sleep 0.05; done\n')
        jid = self.submit("-N", "recovery", script)
        poll(lambda: (self.work / "launches").exists() and self.read("launches").strip(), "first launch")
        worker = int(self.read("launches").strip())
        pid = self.daemon_pid()
        signal_owned(pid, self.token, signal.SIGKILL)
        poll(lambda: not self.live(pid), "old daemon gone")
        reap()
        self.assertTrue(self.live(worker), "worker must survive daemon failure")
        self.cli(jid, tool="qstat")
        poll(lambda: "status: UNKNOWN" in self.path("running", jid).read_text(), "recovery hold")
        self.assertNotEqual(self.daemon_pid(), pid)
        pending = self.submit("-N", "must_wait", "true")
        deadline = time.monotonic() + 2.2
        while time.monotonic() < deadline:
            self.assertEqual(len(self.read("launches").splitlines()), 1, "must never replay")
            self.assertTrue(self.path("new", pending).exists(), "unknown work must pause dispatch")
            time.sleep(0.05)
        self.assertIn("UNKNOWN", self.cli("stat", jid))
        self.assertRegex(self.cli(jid, tool="qstat"), r"\sH\s+batch")
        self.cli(jid, tool="qdel")
        self.assertTrue(self.live(worker), "unverified recovery must not signal old workers")
        self.release()
        poll(lambda: not self.live(worker), "original worker finished")
        self.assertFalse(self.path("done", jid).exists(), "lost exit status must not become success")
        self.assertTrue(self.path("running", jid).exists())

    def test_concurrent_cold_start_elects_one_daemon(self):
        with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
            ids = list(pool.map(lambda i: self.submit("-N", "cold" + str(i), "-a", "+60s", "true"), range(8)))
        self.assertEqual(len(set(ids)), 8)
        owner = self.daemon_pid()
        def daemons():
            result = []
            for pid, _ in owned(self.token):
                try:
                    args = Path("/proc/%s/cmdline" % pid).read_bytes().split(b"\0")
                    if args[1:3] == [b"daemon", b"start"]:
                        result.append(pid)
                except FileNotFoundError:
                    pass
            return result
        poll(lambda: daemons() == [owner], "one elected daemon after cold-start contenders exit")
        self.assertEqual(daemons(), [owner])
        self.assertTrue(all(self.path("new", jid).exists() for jid in ids))

    def test_stale_pidfile_cannot_signal_an_unrelated_process(self):
        sentinel = subprocess.Popen(["sleep", "120"], env=self.env, start_new_session=True)
        (self.state / "run").mkdir(exist_ok=True)
        (self.state / "run/daemon.pid").write_text(str(sentinel.pid))
        self.assertIn("not running", self.cli("daemon", "status"))
        self.cli("daemon", "stop")
        self.assertIsNone(sentinel.poll())
        jid = self.submit("-N", "fresh", "true")
        self.wait_state(jid, "done")
        self.assertNotEqual(self.daemon_pid(), sentinel.pid)
        self.assertIsNone(sentinel.poll())

    def test_stale_or_corrupt_identity_refuses_stop_without_signaling(self):
        jid = self.submit("-N", "pending", "-a", "+60s", "true")
        self.wait_state(jid, "new")
        daemon = self.daemon_pid()
        path = self.state / "run/daemon.identity"
        original = path.read_text()
        fields = dict(line.split(": ", 1) for line in original.splitlines())
        sentinel = subprocess.Popen(["sleep", "120"], env=self.env, start_new_session=True)
        variants = ("invalid identity\n",
                    original.replace("pid: " + str(daemon), "pid: " + str(sentinel.pid)).replace("start_time: " + fields["start_time"], "start_time: 0"),
                    original.replace("start_time: " + fields["start_time"],
                                     "start_time: " + str(int(fields["start_time"]) + 1)),
                    original.replace(fields["boot_id"], "00000000-0000-0000-0000-000000000000"))
        try:
            for damaged in variants:
                with self.subTest(identity=damaged):
                    path.write_text(damaged)
                    self.cli("daemon", "stop", rc=1)
                    self.assertIn("safely", self.log[-1]["stderr"])
                    self.cli("daemon", "status", rc=1)
                    self.assertTrue(self.live(daemon))
                    self.assertIsNone(sentinel.poll())
                    self.assertEqual(path.read_text(), damaged)
        finally:
            path.write_text(original)
        self.cli("daemon", "stop")
        poll(lambda: not self.live(daemon), "verified daemon stopped")
        self.assertIsNone(sentinel.poll())

    def test_old_boot_record_becomes_interrupted_without_replay(self):
        actual = Path("/proc/sys/kernel/random/boot_id").read_text().strip()
        previous = "00000000-0000-0000-0000-000000000000"
        if previous == actual:
            previous = "11111111-1111-1111-1111-111111111111"
        path = self.path("running", "42")
        path.parent.mkdir(parents=True, exist_ok=True)
        original = "id: 42\nname: previous_boot\ncmd: true\ncost: 1\nqueue: batch\nexecution_boot_id: " + previous + "\n"
        path.write_text(original)
        self.cli("stat")
        meta = self.metadata("42", "failed")
        self.assertEqual(meta["status"], "INTERRUPTED")
        self.assertNotIn("exit_code", meta)
        self.assertTrue(self.path("failed", "42").read_text().startswith(original))
        self.assertFalse(path.exists())
        self.assertRegex(self.cli("42", tool="qstat"), r"\sE\s+batch")
        dependent = self.submit("-N", "dependent", "-hold_jid", "42", "true")
        observer = self.submit("-N", "observer", "true")
        self.wait_state(observer, "done")
        self.assertTrue(self.path("new", dependent).exists())

    def test_legacy_and_corrupt_boot_records_remain_held_not_pruned(self):
        self.configure(idle=1, history=0)
        for jid, extra in (("40", ""), ("41", "execution_boot_id: malformed\n")):
            path = self.path("running", jid)
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("id: " + jid + "\nname: legacy\ncmd: true\ncost: 2\nqueue: batch\n" + extra)
        self.cli("stat")
        for jid in ("40", "41"):
            poll(lambda: "status: UNKNOWN" in self.path("running", jid).read_text(), "legacy/corrupt record held")
            self.assertRegex(self.cli(jid, tool="qstat"), r"\sH\s+batch")
        pending = self.submit("-N", "held", "true")
        deadline = time.monotonic() + 2.2
        while time.monotonic() < deadline:
            self.assertTrue(self.path("new", pending).exists())
            self.assertTrue(all(self.path("running", jid).exists() for jid in ("40", "41")))
            time.sleep(0.05)
        self.assertIn("Recovery hold", self.cli("stat"))
        self.assertIn("Global Capacity: 4/8", self.log[-1]["stdout"])
        self.assertTrue(self.live(self.daemon_pid()))

    def test_recovery_write_error_pauses_dispatch_and_reports_hold(self):
        pending = self.submit("-N", "pending", "-a", "+60s", "true")
        self.wait_state(pending, "new")
        daemon = self.daemon_pid()
        signal_owned(daemon, self.token, signal.SIGSTOP)
        poll(lambda: Path("/proc/%s/stat" % daemon).read_text().rsplit(")", 1)[1].split()[0] == "T",
             "daemon paused before recovery fault")
        running = self.path("running", "99")
        collision = running.parent / (".99.job.%s.tmp" % daemon)
        try:
            running.write_text("id: 99\nname: legacy\ncmd: true\ncost: 1\nqueue: batch\n")
            collision.write_text("owned fault fixture; must not overwrite")
            text = self.path("new", pending).read_text()
            self.path("new", pending).write_text("\n".join(
                line for line in text.splitlines() if not line.startswith("start_after:")) + "\n")
        finally:
            signal_owned(daemon, self.token, signal.SIGCONT)
        poll(lambda: (self.state / "run/recovery.hold").exists(), "persistent recovery error warning")
        self.assertIn("Recovery hold", self.cli("stat"))
        self.assertTrue(self.path("new", pending).exists())
        self.assertEqual(collision.read_text(), "owned fault fixture; must not overwrite")
        collision.unlink()
        poll(lambda: "status: UNKNOWN" in running.read_text(), "hold record persisted after fault removed")
        self.assertTrue(self.path("new", pending).exists())

    def test_reboot_recovery_preserves_conflicting_history(self):
        actual = Path("/proc/sys/kernel/random/boot_id").read_text().strip()
        old = "00000000-0000-0000-0000-000000000000"
        if old == actual:
            old = "11111111-1111-1111-1111-111111111111"
        running = self.path("running", "42")
        previous = self.path("failed", "42")
        running.parent.mkdir(parents=True, exist_ok=True)
        previous.parent.mkdir(parents=True, exist_ok=True)
        previous.write_text("id: 42\nname: previous\nstatus: FAILED\nexit_code: 7\n")
        saved = previous.read_text()
        running.write_text("id: 42\nname: conflicting\ncmd: true\ncost: 1\nqueue: batch\nexecution_boot_id: " + old + "\n")
        self.cli("stat")
        poll(lambda: (self.state / "run/recovery.hold").exists(), "conflicting history blocks dispatch")
        self.assertEqual(previous.read_text(), saved)
        self.assertTrue(running.exists())
        self.assertIn("existing history", (self.state / "run/recovery.hold").read_text())
        pending = self.submit("true")
        self.assertTrue(self.path("new", pending).exists())

    def test_reboot_recovery_preserves_conflicting_success_history(self):
        actual = Path("/proc/sys/kernel/random/boot_id").read_text().strip()
        old = "00000000-0000-0000-0000-000000000000"
        if old == actual:
            old = "11111111-1111-1111-1111-111111111111"
        running = self.path("running", "42")
        previous = self.path("done", "42")
        running.parent.mkdir(parents=True, exist_ok=True)
        previous.parent.mkdir(parents=True, exist_ok=True)
        previous.write_text("id: 42\nname: previous_success\nstatus: DONE\nexit_code: 0\n")
        saved = previous.read_text()
        running.write_text("id: 42\nname: conflicting\ncmd: true\ncost: 1\nqueue: batch\nexecution_boot_id: " + old + "\n")
        self.cli("stat")
        poll(lambda: (self.state / "run/recovery.hold").exists(), "conflicting success blocks dispatch")
        self.assertEqual(previous.read_text(), saved)
        self.assertTrue(running.exists())
        dependent = self.submit("-hold_jid", "42", "true")
        deadline = time.monotonic() + 1.2
        while time.monotonic() < deadline:
            self.assertTrue(self.path("new", dependent).exists(), "ambiguous success must not release dependencies")
            time.sleep(0.05)

    def test_cancellation_conflicts_preserve_both_records(self):
        for history_dir in ("failed", "done"):
            with self.subTest(history=history_dir):
                jid = self.submit("-N", "cancel_conflict", "-a", "+60s", "true")
                original = self.wait_state(jid, "new")
                daemon = self.daemon_pid()
                signal_owned(daemon, self.token, signal.SIGSTOP)
                poll(lambda: Path("/proc/%s/stat" % daemon).read_text().rsplit(")", 1)[1].split()[0] == "T", "daemon paused")
                history = self.path(history_dir, jid)
                saved = "id: " + jid + "\nname: existing_history\nstatus: DONE\nexit_code: 0\n"
                try:
                    history.write_text(saved)
                    self.cli(jid, tool="qdel")
                finally:
                    signal_owned(daemon, self.token, signal.SIGCONT)
                poll(lambda: (self.state / "run/recovery.hold").exists(), "cancellation conflict hold")
                self.assertEqual(history.read_text(), saved)
                self.assertEqual(self.path("cancel", jid).read_text(), original)
                # Remove only this synthetic conflicting fixture, not real data.
                history.unlink()
                self.assertEqual(self.metadata(jid, "failed")["status"], "CANCELLED")
                poll(lambda: not (self.state / "run/recovery.hold").exists(), "conflict cleared")

    def test_cancellation_metadata_error_retains_request(self):
        jid = self.submit("-N", "metadata_fault", "-a", "+60s", "true")
        self.wait_state(jid, "new")
        daemon = self.daemon_pid()
        signal_owned(daemon, self.token, signal.SIGSTOP)
        poll(lambda: Path("/proc/%s/stat" % daemon).read_text().rsplit(")", 1)[1].split()[0] == "T", "daemon paused")
        marker = self.path("cancel", jid)
        saved = self.work / "saved-request.job"
        try:
            self.cli(jid, tool="qdel")
            marker.rename(saved)
            marker.symlink_to(self.work / "missing-target")
        finally:
            signal_owned(daemon, self.token, signal.SIGCONT)
        try:
            poll(lambda: (self.state / "run/recovery.hold").exists(), "metadata error hold")
            self.assertTrue(marker.is_symlink(), "metadata errors must not delete a request")
            self.assertTrue(saved.exists())
        finally:
            if marker.is_symlink():
                marker.unlink()
            if saved.exists():
                saved.rename(marker)
        self.assertEqual(self.metadata(jid, "failed")["status"], "CANCELLED")

    def test_dangling_running_record_does_not_erase_cancellation(self):
        running = self.path("running", "42")
        marker = self.path("cancel", "42")
        running.parent.mkdir(parents=True, exist_ok=True)
        marker.parent.mkdir(parents=True, exist_ok=True)
        running.symlink_to(self.work / "missing-running-record")
        marker.write_text("")
        try:
            self.cli("stat")
            poll(lambda: (self.state / "run/recovery.hold").exists(), "dangling running record holds recovery")
            self.assertTrue(marker.exists(), "unverified cancellation must not be consumed")
            self.assertTrue(running.is_symlink())
        finally:
            running.unlink(missing_ok=True)
            marker.unlink(missing_ok=True)

    def test_completion_metadata_error_holds_instead_of_reporting_success(self):
        if os.geteuid() == 0:
            self.skipTest("append-permission regression requires an unprivileged user")
        script = self.script("fail_after_release.sh", '#!/bin/bash\nwhile [ ! -f release ]; do sleep 0.05; done\nexit 7\n')
        jid = self.submit("-N", "cannot_append", script)
        self.wait_state(jid, "running")
        daemon = self.daemon_pid()
        signal_owned(daemon, self.token, signal.SIGSTOP)
        poll(lambda: Path("/proc/%s/stat" % daemon).read_text().rsplit(")", 1)[1].split()[0] == "T", "daemon paused")
        path = self.path("running", jid)
        try:
            path.chmod(0o400)
            self.release()
        finally:
            signal_owned(daemon, self.token, signal.SIGCONT)
        try:
            poll(lambda: self.path("done", jid).exists() or self.path("failed", jid).exists()
                 or (self.state / "run/recovery.hold").exists(), "completion handled")
            self.assertTrue(path.exists(), "unrecorded completion must stay held, not become terminal success")
            self.assertIn("Recovery hold", self.cli("stat"))
            self.assertFalse(self.path("done", jid).exists())
            self.assertFalse(self.path("failed", jid).exists())
        finally:
            for state in ("running", "done", "failed"):
                candidate = self.path(state, jid)
                if candidate.exists():
                    candidate.chmod(0o600)

    def test_history_pruning_and_archive_contents(self):
        self.configure(history=2)
        with (self.state / "config").open("a") as f:
            f.write("archive_interval_days: 0\n")
        ids = [self.submit("-N", "arch" + str(i), "true") for i in range(4)]
        for jid in ids:
            self.wait_state(jid, "done")
        archives = poll(lambda: list((self.state / "archive").glob("archive_*.tar.gz")), "idle archive", 22)
        # Wait for last_archive: archive existence precedes successful tar close.
        poll(lambda: (self.state / "run/last_archive").exists(), "archive completed")
        with tarfile.open(archives[0], "r:gz") as archive:
            members = {Path(m.name).name: archive.extractfile(m).read().decode()
                       for m in archive.getmembers() if m.isfile()}
        self.assertEqual(set(members), {jid + ".job" for jid in ids[:2]})
        for jid in ids[:2]:
            self.assertIn("status: DONE", members[jid + ".job"])
        self.assertEqual({p.stem for p in (self.state / "queue/done").glob("*.job")}, set(ids[2:]))
        poll(lambda: not list((self.state / "archive/pending").iterdir()), "archive pending cleared")
        output = self.cli("-H", tool="qstat")
        self.assertEqual(len(re.findall(r"^\d+\.master", output, re.M)), 2)

    def test_range_and_list_substitution(self):
        output = self.cli("--range", "2-4", "-N", "range", "echo", "value{}", tool="qsub")
        ids = [line.split(".")[0] for line in output.splitlines()]
        self.assertEqual(len(ids), 3)
        for jid, value in zip(ids, (2, 3, 4)):
            self.wait_state(jid, "done")
            self.assertEqual(self.read("range.o%s" % jid), "value%s\n" % value)
        output = self.cli("--list", "A,B", "-N", "list", "echo", "{}", tool="qsub")
        ids = [line.split(".")[0] for line in output.splitlines()]
        self.assertEqual(len(ids), 2)
        for jid, value in zip(ids, ("A", "B")):
            self.wait_state(jid, "done")
            self.assertEqual(self.read("list.o%s" % jid), value + "\n")

    def test_default_cost_is_one(self):
        jid = self.submit("true")
        self.assertEqual(self.metadata(jid)["cost"], "1")

    def test_multiple_dependencies_all_must_finish(self):
        gate = self.gate()
        first = self.submit("-N", "first", "true")
        self.wait_state(first, "done")
        second = self.submit("-N", "second", gate)
        self.wait_state(second, "running")
        successor = self.submit("-N", "both", "-hold_jid", first + "," + second, "true")
        observer = self.submit("-N", "observer", "true")
        self.wait_state(observer, "done")
        self.assertTrue(self.path("new", successor).exists())
        self.release()
        self.wait_state(successor, "done")
        self.assertGreaterEqual(int(self.metadata(successor)["start_at"]),
                                int(self.metadata(second)["end_at"]))

    def test_shared_output_and_append_preserves_existing_data(self):
        (self.work / "combined").write_text("existing\n")
        script = self.script("streams.sh", '#!/bin/sh\necho out\necho err >&2\n')
        jid = self.submit("-N", "streams", "-o", str(self.work / "combined"), "-e", "combined", script)
        self.wait_state(jid, "done")
        self.assertEqual(self.read("combined"), "existing\nout\nerr\n")

    def test_stat_restarts_daemon_for_pending_jobs(self):
        jid = self.submit("-N", "pending", "-a", "+60s", "true")
        self.wait_state(jid, "new")
        old = self.daemon_pid()
        self.cli("daemon", "stop")
        poll(lambda: not self.live(old), "old daemon stopped")
        self.assertIn("pending", self.cli(jid, tool="qstat"))
        self.assertNotEqual(self.daemon_pid(), old)
        self.assertTrue(self.path("new", jid).exists())

    def test_absolute_timestamp_delayed_start(self):
        start = int(time.time()) + 3
        jid = self.submit("-N", "timestamp", "-a", str(start), "true")
        meta = self.metadata(jid)
        self.assertEqual(int(meta["start_after"]), start)
        self.assertGreaterEqual(int(meta["start_at"]), start)

    def test_concurrent_submission_unique_ids(self):
        # Start one daemon first: testing ID allocation, not daemon-election races.
        initial = self.submit("-N", "initial", "true")
        self.wait_state(initial, "done")
        with concurrent.futures.ThreadPoolExecutor(max_workers=6) as pool:
            ids = list(pool.map(lambda i: self.submit("-N", "parallel" + str(i), "true"), range(12)))
        self.assertEqual(len(ids), len(set(ids)))
        self.assertEqual(set(map(int, ids)), set(range(2, 14)))
        for jid in ids:
            self.wait_state(jid, "done")


def main():
    global BINARY, ROOT, KEEP, INJECT
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", default=os.environ.get("FBQUEUE_TEST_BINARY"))
    parser.add_argument("--only", action="append", default=[])
    parser.add_argument("--keep", action="store_true", help="retain successful scratch artifacts too")
    parser.add_argument("--inject-failure", action="store_true", help="safe assertion failure test of runner")
    args = parser.parse_args()
    if sys.platform != "linux":
        parser.error("Linux only; Windows suite is separate")
    for feature in ((hasattr(os, "pidfd_open")), hasattr(signal, "pidfd_send_signal")):
        if not feature:
            parser.error("Python 3.9+ with Linux pidfd support is required for safe cleanup")
    if ctypes.CDLL(None, use_errno=True).prctl(36, 1, 0, 0, 0) != 0:
        parser.error("cannot enable Linux child subreaper")
    candidates = [Path(args.binary)] if args.binary else [REPO / "target/release/fbqueue", REPO / "target/debug/fbqueue"]
    BINARY = next((p.resolve() for p in candidates if p.is_file() and os.access(p, os.X_OK)), None)
    if BINARY is None:
        parser.error("no executable binary; run cargo build --release or set FBQUEUE_TEST_BINARY")
    KEEP, INJECT = args.keep, args.inject_failure
    SCRATCH.mkdir(parents=True, exist_ok=True)
    ROOT = Path(tempfile.mkdtemp(prefix="fbqueue-linux-", dir=SCRATCH))
    os.chmod(ROOT, 0o700)
    def interrupted(signum, frame):
        raise KeyboardInterrupt("signal %s" % signum)
    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    names = unittest.defaultTestLoader.getTestCaseNames(SafeCase)
    if args.only:
        names = [name for name in names if any(name == x or name == "test_" + x for x in args.only)]
        if len(names) != len(set(args.only)):
            parser.error("unknown or duplicate --only selection")
    print("Binary: %s\nScratch: %s" % (BINARY, ROOT), flush=True)
    result = None
    try:
        result = unittest.TextTestRunner(verbosity=2).run(
            unittest.TestSuite(SafeCase(name) for name in names))
        for test, details in result.expectedFailures:
            print("XFAIL %s: %s" % (test.id(), details.strip().splitlines()[-1]))
        # unittest records each failing subtest separately; count parent cases
        # for the case summary so several failures never produce negative passes.
        failed = {getattr(test, "test_case", test).id() for test, _ in result.failures}
        errored = {getattr(test, "test_case", test).id() for test, _ in result.errors}
        expected = {test.id() for test, _ in result.expectedFailures}
        skipped = {getattr(test, "test_case", test).id() for test, _ in result.skipped}
        unexpected = {test.id() for test in result.unexpectedSuccesses}
        passed = result.testsRun - len(failed | errored | expected | skipped | unexpected)
        print("SUMMARY: %d cases; %d passed; %d expected failures; %d failed cases; "
              "%d errored cases; %d skipped; %d unexpected successes "
              "(%d failure records; %d error records)" %
              (result.testsRun, passed, len(expected), len(failed), len(errored),
               len(skipped), len(unexpected), len(result.failures), len(result.errors)), flush=True)
        return 0 if result.wasSuccessful() else 1
    finally:
        # Finish bounded cleanup even if a second interrupt arrives.
        signal.signal(signal.SIGTERM, signal.SIG_IGN)
        signal.signal(signal.SIGINT, signal.SIG_IGN)
        # Belt-and-suspenders cleanup on interruption, including the current case.
        cleanup_errors = []
        for case in ROOT.iterdir():
            # Registry exists before any command, even if no job was submitted.
            token_file = case / "ownership-token"
            tokens = {token_file.read_text()} if token_file.exists() else set()
            for token in tokens:
                deadline = time.monotonic() + 5
                pidfile = case / "state/run/daemon.pid"
                if pidfile.exists() and pidfile.read_text().strip().isdigit():
                    pid = int(pidfile.read_text())
                    if pid in [p for p, _ in owned(token)]:
                        signal_owned(pid, token, signal.SIGKILL)
                while owned(token):
                    for pid, _ in owned(token):
                        signal_owned(pid, token, signal.SIGKILL)
                    reap()
                    if time.monotonic() > deadline:
                        cleanup_errors.append(case.name)
                        break
                    time.sleep(0.02)
                reap()
        if cleanup_errors:
            raise RuntimeError("Cleanup verification failed: %r" % cleanup_errors)
        if KEEP or result is None or not result.wasSuccessful():
            print("Artifacts retained: %s (may contain captured environment; private mode 0700)" % ROOT, flush=True)
        else:
            shutil.rmtree(ROOT)
            print("Scratch removed; no owned live processes remain.", flush=True)


if __name__ == "__main__":
    sys.exit(main())
