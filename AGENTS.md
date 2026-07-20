# FBQueue (Forblaze Queue)

Lightweight local job scheduler written in Rust (zero-dependency except `libc` on Unix).
PBS/HPC-compatible command-line interface for researchers and developers.

## Build, Lint, Test

```bash
cargo build --release          # build optimized binary
cargo clippy --all-targets     # lint
cargo test                     # unit tests (none currently; tests are shell-based)
bash tests/run_tests.sh        # full integration test suite (Linux, ~60s)
pwsh tests/run_tests_core.ps1  # Windows test suite
```

### Verification flow after code changes

1. `cargo build --release` — must compile clean
2. `bash tests/run_tests.sh` — all 22 assertions must pass
3. If MPI/PBS_NODEFILE touched: submit a job with `-c 4` and verify
   `mpiexec -n 4 lmp -in <script>` runs without `--oversubscribe`, and that
   `$PBS_NODEFILE` has exactly 4 lines

## Project Structure

```
fbqueue/
├── src/
│   ├── main.rs        # Entry point; detects invocation name (qsub/qstat/qdel/fbqueue)
│   ├── handlers.rs    # CLI command handlers: handle_sub, handle_del, handle_daemon
│   ├── job.rs         # Job struct, .job file parser, submit_job (writes job files)
│   ├── daemon.rs      # Transient daemon: scheduling loop, spawn/kill, nodefile, archiving
│   ├── stat.rs        # qstat/stat output (PBS style + default style), filtering
│   ├── config.rs      # Config struct, YAML-like config parser
│   └── utils.rs       # FBQUEUE_DIR resolution, time parsing, ID generation, shebang parser
├── tests/
│   ├── run_tests.sh        # Linux integration tests (13 cases, 22 assertions)
│   ├── run_tests_core.ps1  # Windows tests
│   └── test_id_filter.sh   # standalone qstat filter test
├── examples/{linux,windows}/  # sample job scripts
├── MANUAL.md              # User manual (architecture, CLI reference, env vars)
├── PBS_COMPATIBILITY.md   # PBS migration guide
└── Cargo.toml             # package = "fbqueue", edition 2021
```

## Architecture & Conventions

### Binary invocation detection

`main.rs` detects how the binary was called (`args[0]` file stem) and dispatches:
- `qsub` → `sub` command, outputs `<id>.master` only
- `qstat` → `stat` command with `--style pbs` default
- `qdel` → `del` command
- `fbqueue <subcommand>` → explicit subcommand

Symlinks (`qsub`, `qstat`, `qdel` → `fbqueue`) are the recommended install method.

### File-based state (no DB, no network)

All state lives under `$FBQUEUE_DIR` (default `~/.fbqueue/`):
```
queue/{new,running,done,failed,cancel}/<id>.job   # job state machine via rename()
run/{daemon.pid, last_id, id.lock, nodefile.<id>}  # daemon control + per-job nodefiles
archive/{pending, archive_*.tar.gz}                # idle-time bundling
config                                               # YAML-like, hand-parsed
```

Jobs move between states by **atomic `fs::rename`**, not locks. The `.job` file
format is simple `key: value` lines (see `job.rs:parse_job_file`).

### Daemon lifecycle

- **Auto-start**: `ensure_daemon()` spawns a detached daemon (via `setsid`) on
  any `sub`/`stat`/`del` if not running.
- **Auto-shutdown**: daemon exits after `inactivity_timeout` (default 300s) when
  both `queue/new/` and `queue/running/` are empty.
- **Recovery**: on restart, jobs left in `running/` are resumed (daemon re-spawns them).

### Resource model

- `capacity` (config): total resource slots for the system or a queue
- `cost` (per job, `-c N` or `#PBS -l ncpus=N`): resource weight
- Scheduling: job starts only when `used + cost <= capacity` (per-queue and global)

### PBS compatibility (core design principle)

- **`PBS_NODEFILE`**: must contain one line per requested slot (= `cost`).
  OpenMPI reads line count as slot count. Writing only 1 line breaks
  `mpiexec -n N` (requires `--oversubscribe` workaround). See `daemon.rs` nodefile
  generation.
- Script directives: `#PBS`, `#$` (SGE), `#SBATCH` parsed from first 100 lines.
- PBS env vars injected: `PBS_JOBID`, `PBS_JOBNAME`, `PBS_QUEUE`, `PBS_NODEFILE`,
  `PBS_ENVIRONMENT`, `PBS_O_WORKDIR`, `PBS_O_HOST`, `PBS_O_LOGNAME`.
- All submitter env vars are captured into the `.job` file and replayed at spawn.

### Process management

- Unix: `setsid()` in `pre_exec` creates a new session; `kill(-pgid, SIGKILL)`
  terminates the whole process tree on `qdel`.
- Windows: `taskkill /T /F /PID` for tree termination.

### stat.rs output modes

- PBS style (`qstat`): tabular, `R`/`Q`/`F`/`E` status chars
- Default style (`fbqueue stat`): capacity summary + job lists
- `qstat <jobid>`: shows job in ANY state (running, pending, finished)
- `qstat -H`: history-only mode
- `qstat -u <user>`: user filter

## Git & Release Workflow

- Commit message style: `Release vX.Y.Z: <short description>` for releases,
  conventional commits for features/fixes between releases.
- Version bumps: update `Cargo.toml` AND `Cargo.lock` together.
- README.md changelog: add `### vX.Y.Z` entry with bullet points.
- README.md download URL: update to current version in Quick Start section.
- Tags: `git tag vX.Y.Z` → triggers `.github/workflows/release.yml` (builds
  linux-x64, linux-arm64, windows-x64 and uploads to GitHub Releases).
- MANUAL.md and PBS_COMPATIBILITY.md may also need updates when env vars or
  directives change.

## Gotchas

- **MANUAL.md vs PBS_COMPATIBILITY.md**: both document env vars/directives.
  Keep them in sync when adding PBS features.
- **Test timing**: the daemon polls every 1s. Tests use `sleep` to wait for job
  completion; too-short sleeps cause flaky failures (see
  `test_pbs_user_filter_and_history` which needs `sleep 3`).
- **`cost=0` guard**: nodefile generation defaults to 1 slot if cost is 0.
- **Config parser**: hand-rolled YAML-like parser in `config.rs`. Queue entries
  use indented `capacity:`/`priority:` lines under `queue: <name>`. Not real
  YAML — indentation matters but is simple.
- **Global capacity default**: `min(available_parallelism, 8)` if not in config.

## Existing Documentation

- `GEMINI.md` — earlier dev notes (Windows guidelines, roadmap); superseded by this file
- `MANUAL.md` — comprehensive user manual
- `PBS_COMPATIBILITY.md` — PBS/SGE migration guide
- `README.md` — project overview + changelog
