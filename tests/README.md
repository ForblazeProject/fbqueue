# Linux integration tests (v0.9.6)

```sh
cargo build --release
bash tests/run_tests.sh                 # 52 real-binary cases
bash tests/test_id_filter.sh            # focused active/pending/history ID filtering
python3 tests/test_harness.py           # negative controls for the harness itself
python3 -B tests/test_harness_verdicts.py # optimized-Python false-success regression
```

Requirements: Linux with readable own `/proc` entries and pidfds, Python 3.9+
(stdlib only), bash, sh, hostname, sleep, wc, and tar. Windows tests remain
separate and are not invoked by the Linux suite. No actual MPI runtime is required:
the PBS test verifies exactly four hostname lines for a four-slot job, not a
claim that an MPI solver was executed.
The append-permission regression needs an unprivileged user and is explicitly
skipped when running as root; this is reported as a skip, not a pass.

## Windows real-machine kit (separate scope)

See `windows/README-JA.md` and `../docs/WINDOWS_SUPPORT.md`. The old process-name
kill suite has been replaced by a safe compatibility entrypoint:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File tests/run_tests_core.ps1 -Binary C:\path\fbqueue.exe
```

The Windows 10/11 kit uses Windows PowerShell 5.1 and compiled C# native helpers,
not Python or Pester. It provides 19 cases with explicit Development/Released095
profiles and per-case Job Objects. It returns `FBQueue-Windows-Results.zip`,
including binary SHA-256, case verdicts and cleanup accounting, but no raw job
environment dumps. The pre-version-bump implementation prepared for v0.9.6 was rebuilt on Windows
AMD64/PowerShell 5.1 and passed 19/19 native cases on 2026-11-13, including alias
rejection and explicit PBS history/filter checks. No cases were skipped or left
unexecuted. See `../docs/WINDOWS_VERIFICATION.md` for evidence and scope.
Parser/compiler/protocol checks on Linux do not count as Windows product passes. Test-only Job Objects do
not add Linux-equivalent recovery or containment to FBQueue itself.

## Linux binary selection

Both shell entrypoints are location-independent. By default the harness selects
an existing executable `target/release/fbqueue`, falling back to
`target/debug/fbqueue`; it does not rebuild implicitly. Override with
`FBQUEUE_TEST_BINARY=/absolute/path/to/fbqueue` or `--binary PATH`. An invalid
explicit path is an error rather than a silent fallback. Select cases using
repeatable `--only test_name` (the `test_` prefix is optional).

## Isolation and failure handling

- Every invocation uses `mkdtemp` beneath Python's temporary directory (`TMPDIR`
  when set; override the **parent** with `FBQUEUE_TEST_SCRATCH`). Every case has fresh state, work, HOME, aliases, and a
  random ownership token. Caller `FBQUEUE_DIR` is ignored. Nothing uses the real
  `~/.fbqueue`; nothing writes job output into the checkout.
- Scheduler state, output, and executable alias paths can contain spaces.
- There are no global `pgrep`, `pkill`, command-pattern kills, or process-group
  broadcasts. Cleanup discovers only exact per-case environment tokens, pins
  processes with pidfds, rechecks ownership, and signals those specific PIDs.
  The daemon is stopped before its workers to prevent replacement jobs.
- Linux subreaper mode adopts/reaps orphan descendants. Zombie processes are not
  mistaken for live work. A daemon's PID is ownership-checked before exercising
  the product's `daemon stop` command. Recovery deliberately leaves the owned
  original worker alive to prove that a new daemon neither replays it nor inherits
  an execution lock through it. Final cleanup still owns and stops all test work.
- State transitions use bounded 50ms predicate polling, not guessed multi-second
  sleeps. CLI calls have eight-second subprocess timeouts. Cleanup is bounded and
  runs on failures, SIGINT, and SIGTERM as well as success. SIGKILL/power loss
  cannot be trapped.
- Failures return nonzero. Failure diagnostics and scratch are retained; `--keep`
  also retains successful runs. Success otherwise removes scratch after checking
  that no owned live processes remain. `diagnostics.json` records CLI arguments,
  return codes, stdout/stderr, and job metadata with environment lines redacted.
  Raw job files still capture the submission environment, so the scratch root is
  mode 0700 and retained artifacts should be treated as sensitive.

`test_harness.py` verifies a deliberately broken executable returns failure, an
injected assertion with a live job process tree returns failure, and SIGTERM
cleans up an active scheduler. All controls check an unrelated sentinel survives
and all private ownership tokens have zero live processes. Negative-control logs
are retained in their own private scratch directory.

`test_harness_verdicts.py` injects an unexpected successful child result under
`python -O` and verifies it is rejected, rather than printed as PASS. The verdict
checks in `test_harness.py` use explicit exceptions, not removable `assert`s.
It also checks that multiple failing subtests count as one failed parent case,
so the case summary cannot report a negative number of passes.

## Coverage and interpretation

The suite counts **test cases**, not hand-maintained shell assertion counters.
It verifies native and PBS aliases, argument passing, non-executable scripts,
shebang arguments and shell fallback, directive resource/name/output handling,
CLI override precedence, PBS variables, per-submission environment replay into an
already-running daemon, four-line nodefiles and their cleanup, success/failure
exit metadata, R/Q/H/F/E status, active/pending/completed ID filters and `.master`
suffixes, user/history filters, global and documented indented queue capacity,
actual queue-priority execution order, dependencies (successful, failed, multiple),
relative and absolute delay, walltime, queued cancellation, running process-tree
cancellation, daemon autostart/stop/restart/recovery/idle shutdown, history pruning,
archive member contents, batch argument substitution, and concurrent ID issuance.

The queue capacity/priority regressions assert the documented indented config
syntax as normal passing behavior; they must not become expected failures or
silently use unindented workarounds.

Approved contract regressions additionally verify first-run default-config
initialization; missing commands/option arguments, invalid options and malformed
batch requests; positive/default resource cost and capacity equality; rejection
before any IDs are allocated; preflight of all expanded scripts; valid CLI cost
overriding malformed script cost; independent global limits across queues; FIFO
execution order; live capacity reduction; and pending cancellation history without
fabricated start/exit fields. Unknown target queues are rejected. Legacy zero-cost
records reserve one slot rather than running for free, and native usage reporting
uses that same defensive effective cost.
Repeated cancellation requests are exercised with an ownership-verified paused
daemon to check preservation of complete records. Late cancellation markers use
a completed-job barrier and an observer job to verify successful completion
history remains intact.

Linux stage-1 recovery coverage includes an externally held lifetime lock,
concurrent cold starts, permanent lock inode reuse, stale PID files, stale/corrupt
identity refusal without signaling unrelated work, live-worker daemon failure
without replay, legacy/invalid boot records held as UNKNOWN, previous-boot fixtures
finalized as INTERRUPTED without fake exit codes, dispatch/dependency holds,
recovery-write faults and conflicting successful/failed history. Cancellation
conflicts, metadata failures and dangling running records retain requests.
Completion metadata must be retained before terminal moves; spawn failures are
failed history, not successful completion. Rust unit regressions additionally
cover cancellation work at idle retirement and latched post-transition I/O errors
that an empty running queue must not clear. Boot mismatch is simulated with private
records, not a real reboot or power-loss test.

Limits remain explicit: stage 2 (worker adoption, completion receipts, explicit
reconciliation/retry CLI) in `docs/DAEMON_RECOVERY_DESIGN.md` is not implemented.
An UNKNOWN job stays held even after its old worker disappears. Process groups do
not contain arbitrary escaping descendants. Unsupported PBS forms, walltime
precision, every malformed time/duration input, and real storage/power-loss failure
boundaries are not claimed as comprehensively covered. Range/list expansion is
tested for command arguments, not CLI job-name/output option substitution.
