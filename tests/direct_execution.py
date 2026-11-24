#!/usr/bin/env python3
"""Direct submission regressions, using the private Linux scheduler harness."""

import os
import shutil
import sys

import linux_integration as harness


class DirectExecutionCases(harness.SafeCase):
    def copy_echo(self, path):
        echo = shutil.which("echo")
        if echo is None:
            self.fail("external echo executable required")
        shutil.copy2(echo, path)

    def test_direct_local_and_path_executables(self):
        # Copy a native binary into the working directory. The legacy mode
        # would attempt to interpret this local file as a script.
        local = self.work / "native echo"
        self.copy_echo(local)
        for program in (str(local), "./native echo", "native echo", "echo"):
            with self.subTest(program=program):
                jid = self.submit("--exec", "--", program, "two words", "$HOME;*", explicit=True)
                meta = self.metadata(jid)
                self.assertEqual(meta["exit_code"], "0")
                self.assertEqual(self.read(meta["name"] + ".o" + jid), "two words $HOME;*\n")

    def test_direct_shell_is_explicit_and_preserves_argv(self):
        jid = self.submit("-o", "argv.out", "--exec", "sh", "-c",
                          'printf "<%s>\\n" "$@"; exit 7', "argv0",
                          "", "  padded  ", "one\ntwo", "line\r", "--help", "$(echo nope)")
        self.assertEqual(self.metadata(jid, "failed")["exit_code"], "7")
        self.assertEqual((self.work / "argv.out").read_bytes(),
                         b"<>\n<  padded  >\n<one\ntwo>\n<line\r>\n<--help>\n<$(echo nope)>\n")

    def test_direct_does_not_read_directives_or_fall_back_to_shell(self):
        script = self.script("plain.sh", "#PBS -c 0\nprintf 'unexpected\\n' > ran\n")
        (self.work / script).chmod(0o755)
        jid = self.submit("--exec", script)
        self.assertEqual(self.metadata(jid, "failed")["exit_reason"], "spawn_failed")
        self.assertFalse((self.work / "ran").exists())

    def test_direct_uses_submitter_path(self):
        jid = self.submit("-o", "first.out", "--exec", "echo", "first")
        self.metadata(jid)
        env = dict(self.env, PATH="bin" + os.pathsep + self.env["PATH"])
        local_bin = self.work / "bin"
        local_bin.mkdir()
        program = local_bin / "private-echo"
        self.copy_echo(program)
        jid = self.submit("-o", "second.out", "--exec", program.name, "second", env=env)
        self.metadata(jid)
        self.assertEqual(self.read("second.out"), "second\n")


if __name__ == "__main__":
    sys.exit(harness.main(DirectExecutionCases, method_prefix="test_direct_"))
