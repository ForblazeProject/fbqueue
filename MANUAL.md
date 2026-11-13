# FBQueue (Forblaze Queue) - Development Manual

This document provides a detailed description of the design, implementation, and future roadmap of FBQueue. It serves both as internal documentation for developers and a comprehensive manual for users.

---

## Table of Contents

1.  [Core Philosophy](#1-core-philosophy)
2.  [Directory Structure](#2-directory-structure)
3.  [Job File Specification (.job)](#3-job-file-specification-job)
4.  [Resource Management (Capacity & Cost)](#4-resource-management-capacity--cost)
5.  [Multi-Queue and Priority Support](#5-multi-queue-and-priority-support)
6.  [Advanced Scheduling Features](#6-advanced-scheduling-features)
7.  [Windows Implementation and Guidelines](#7-windows-implementation-and-guidelines)
8.  [CLI & Directive Reference](#8-cli--directive-reference)
9.  [Job Environment Variables](#9-job-environment-variables-pbs-compatibility)
10. [Roadmap](#10-roadmap)

---

## 1. Core Philosophy

FBQueue is designed to provide robust job scheduling without the administrative overhead or security risks associated with traditional enterprise software.

*   **Minimal Dependencies & Zero Config**: Built with the Rust standard library plus the `libc` crate on Unix. A single binary that runs entirely in user-space without root access.
*   **Security-First Architecture**:
    - **No Network Ports**: Operates strictly via the file system. It avoids exposing any network surface, making it ideal for restricted corporate or research environments.
    - **No Database Engine**: Uses a transparent, file-based state management system. No complex database setup or maintenance is required.
*   **Resource Efficiency (Auto-Shutdown)**: The daemon is a transient process designed to conserve system resources. It automatically terminates after a configurable period of inactivity (`inactivity_timeout`, default: 300s).
    - **Trigger Conditions**: The inactivity countdown begins **only** when both of the following conditions are met:
        1.  No jobs are currently executing (the `running/` directory is empty).
        2.  No jobs are waiting in the queue (the `new/` directory is empty).
    - **Behavior**: If a new job is submitted or a scheduled job is waiting for its start time (remaining in the `new/` directory), the daemon stays active. This ensures that scheduled tasks are never missed due to premature daemon termination.
*   **Automatic Daemon Management**: The daemon is launched on-demand by CLI commands (`sub`, `stat`, `del`) if it is not already running and shuts down when idle. Normal operation needs no manual startup; recovery holds can require operator verification and intervention.
*   **Transparency**: All job states are visible and manageable directly through the file system, allowing for easy inspection and debugging.
*   **Conservative Linux Recovery (v0.9.6)**: Unknown execution outcomes are held rather than replayed. The older v0.9.5 release uses replay; do not mix old and new daemon processes against one state directory.

### Linux recovery safety (v0.9.6)

The Linux daemon holds an exclusive lifetime lock on the permanent inode
`run/daemon.lock`. Never delete this file to force a restart. `daemon.pid` is
display metadata only; stop/status validate boot ID, PID and process starttime
through a pidfd. An unverified identity is an error, not permission to signal a PID.
The Linux v0.9.6 executable requires usable pidfds and readable own `/proc`
identity information; there is no unsafe bare-PID fallback.

On restart, untracked `running` records are never returned to `new`. Same-boot,
legacy or invalid-identity records become `UNKNOWN`; all new dispatch pauses and
their cancellation requests remain pending. Already tracked jobs can still finish
or be cancelled. A valid stored boot ID different from the current boot permits
`INTERRUPTED` finalization into failed history, without inventing an exit code.
Unreadable/unwritable recovery records or conflicting terminal history also block
dispatch; `stat` reports the reason using `run/recovery.hold` when storage permits.
If a terminal rename succeeds but directory synchronization fails, the error is
latched in `run/recovery.error`; an empty queue does not automatically clear it.
Storage and history must be checked before controlled administrative repair.

`daemon stop` stops only the verified daemon, not existing workers. Those workers
are not adopted on restart. Even after an orphan exits, its lost exit status is
not guessed: `UNKNOWN` remains held. Automatic worker supervision, completion
receipts and a reconciliation/retry CLI are deferred. Do not delete held evidence
or retry a simulation until its surviving processes and external effects have
been checked. See `docs/DAEMON_RECOVERY_DESIGN.md` for scope and migration notes.

## 2. Directory Structure

FBQueue operates within the directory specified by the `FBQUEUE_DIR` environment variable. If not set, it defaults to the user's home directory (`~/.fbqueue/`).

### Personal vs. Shared Usage
- **Personal Mode (Default)**: Using `~/.fbqueue/` ensures your job queue is private and doesn't interfere with other users.
- **Shared Mode**: Multiple users on a single machine can share a common queue by setting `FBQUEUE_DIR` to a **local shared directory**. Note that using network-mounted drives (NFS, SMB, etc.) is highly discouraged as it may impact performance and result in file-locking latency.

```
.fbqueue/
├── config                     # Main configuration file (YAML-like syntax)
├── queue/                     # Job queue root
│   ├── new/                   # Pending jobs
│   ├── running/               # Active jobs
│   ├── done/                  # Successfully completed jobs
│   ├── failed/                # Jobs that failed or were forcibly terminated
│   └── cancel/                # Signals for job cancellation
├── logs/                      # (Legacy) Output log location
└── run/                       # Daemon management files
    ├── daemon.pid             # Current daemon process ID
    ├── daemon.lock            # Linux lifetime lock; never unlink while in use
    ├── daemon.identity        # Linux boot ID / PID / process starttime
    ├── recovery.hold          # Present while recovery needs intervention
    ├── recovery.error         # Latched unconfirmed terminal transition
    ├── last_id                # Counter for the next job ID
    └── id.lock/               # Lock for ID issuance (concurrency control)
```

## 3. Job File Specification (.job)

Internal `.job` files use a simple key-value format to store job metadata:

```text
id: 1
name: MyJob
cmd: ./script.sh
cost: 1
user: username
queue: batch
cwd: /home/user/work
env: KEY=VALUE
stdout: /path/to/out
stderr: /path/to/err
walltime: 3600
depend: 10,11
start_after: 1771766817
```

## 4. Resource Management (Capacity & Cost)

*   **`capacity`**: The total resource allocation limit (e.g., CPU cores or GPU units) for the environment, defined in `config`.
*   **`cost`**: The resource "weight" or consumption of an individual job, specified at submission with `-c`.
*   **Scheduling Logic**: A job is started only when `currently_used_capacity + job_cost <= total_capacity`.

### Submission validation (v0.9.6)

The effective `cost` must be an integer of at least 1. If neither a CLI option nor
a supported script directive specifies it, the default is 1. CLI settings override
script settings. Invalid values and requests exceeding either the global capacity
or the selected queue's capacity are rejected before a job ID is allocated.
A valid request that fits the configured limits but not the currently available
slots is accepted and waits. Both limits are checked again before launch; reducing
capacity does not kill already-running jobs, but can leave accepted jobs waiting.

Submission usage/validation errors go to stderr and return exit status 2; stdout
is empty and no daemon is started by the rejected submission. An omitted command
also prints a short usage line and a help hint. Explicit `fbqueue sub --help` or
`qsub --help` prints help to stdout and returns 0 without submitting anything.

## 5. Multi-Queue and Priority Support

Queues are defined in the `config` file to control resource distribution and execution order.

```text
# Global maximum capacity
capacity: 16
default_queue: batch
# Seconds before idle shutdown
inactivity_timeout: 300
# Number of terminal jobs retained in direct history
history_limit: 100
# Days between archive bundles
archive_interval_days: 30

queue: batch
  capacity: 8
  priority: 10

queue: express
  capacity: 4
  priority: 100
```

This is a small YAML-like parser, not a full YAML implementation. Put comments on
their own lines, not after values. Higher priority values are processed first.

## 6. Advanced Scheduling Features

### Walltime (`-W`)
Automatically terminates jobs that exceed the specified execution time limit.
```bash
fbqueue sub -W 01:30:00 ./long_task.sh
```

### Dependency Management (`-hold_jid`)
Holds job execution until the specified job ID(s) finish successfully (`Done`). This allows for complex task-graph (DAG) execution.
```bash
fbqueue sub -hold_jid 123 ./next_task.sh
```

### Delayed Start (`-a`)
Schedules a job to start after a specific time. Supports relative time (`+1h`, `+30m`), time today (`18:00`), absolute date (`2026-02-23 18:00`), or UNIX timestamp.

```bash
fbqueue sub -a +1h ./future_task.sh
fbqueue sub -a 18:00 ./tonight_task.sh
fbqueue sub -a "2026-02-24 09:00:00" ./tomorrow_task.sh
```

## 7. Windows Implementation and Guidelines

The pre-version-bump implementation prepared for v0.9.6 was rebuilt on Windows AMD64 with
PowerShell 5.1 and passed all 19 native test cases on 2026-11-13, with no skips,
errors or unexecuted cases. This includes alias rejection and PBS history/filter
checks. See `docs/WINDOWS_VERIFICATION.md` for the tested snapshot and limits.
Linux recovery parity is not a Windows requirement. Published v0.9.5 does not
include the validation, pending-cancellation or alias-policy changes introduced in v0.9.6.

### Interpreter Selection
The appropriate interpreter is automatically selected based on the file extension:
- **`.bat`, `.cmd`**: Executed via `cmd /c`.
- **`.ps1`**: Executed via `powershell -ExecutionPolicy Bypass -File`.

`powershell` means Windows PowerShell, not automatic selection of PowerShell 7's
`pwsh`. Unix `.sh`/shebang behavior is not emulated. The application does not add
`-NoProfile`; user profiles can affect job execution. Execution-policy bypass is
not a safety guarantee or a system-wide policy change.

### Feature Scope and Operational Limits

| Area | Windows implementation / limitation |
| :--- | :--- |
| Scheduling | Shared capacity/cost, queue, priority, dependency, delay, expansion and walltime implementation. Selected Windows execution cases passed in the recorded native run; this is not exhaustive Windows coverage. Capacity is declared accounting, not hardware isolation, on both platforms. |
| PBS subset | Selected directives, variables and nodefile generation exist. Real Windows MPI/solver execution is not validated by nodefile tests. |
| CLI names | Use `fbqueue.exe sub/stat/del/daemon`; do not rename the executable. Windows aliases are rejected with status 2 before state initialization. Linux aliases are unchanged. PBS output remains `fbqueue.exe stat --style pbs`. |
| Cancellation / walltime | Uses `taskkill /F /T`. No graceful shutdown or arbitrary escaped-process containment guarantee. |
| `daemon stop` | Uses the PID file and `taskkill /PID ... /F /T`; can terminate workers too. This differs from Linux development behavior. |
| Ownership / status | Legacy PID-file / tasklist checks, not Linux lifetime locks or boot/starttime/pidfd verification. Status reads the PID file; it is not proof of process identity. Concurrent cold-start safety is not equivalent to Linux. |
| Restart | Moves running records back to pending and re-executes them. Surviving workers can be duplicated; avoid restarting during active work. |
| Fail-closed recovery | Linux UNKNOWN/INTERRUPTED reconciliation, recovery holds and persistent `recovery.error` latches are not implemented on Windows. |
| Archiving | Requires external `tar` on PATH; the optional archive test is SKIP if `tar.exe` is unavailable. |

Use a single-user, single-host local state directory for the test baseline. Shared
multi-user permissions, network storage, long paths, every quoting/encoding form,
and arbitrary external effects are not covered by these tests. Do not treat
filesystem-based state or unchanged file permissions as a security sandbox.

See `docs/WINDOWS_SUPPORT.md` for the detailed support matrix. The real-machine
test kit is `tests/windows/Run-Tests.ps1` (Windows 10/11, Windows PowerShell 5.1;
no Python/Pester/admin required). It uses per-case private state and test-only Job
Objects, records binary SHA-256 and explicit Development/Released095 expectations,
and produces `FBQueue-Windows-Results.zip` for review. Test containment is not a
new feature of the FBQueue product. The native 19-case run confirms the tested
execution, environment/nodefile, scheduling, cancellation, walltime, archiving,
explicit history/filter and alias-rejection cases. Every case's cleanup reached
zero active processes and the separate sentinel survived case cleanup. These
results do not extend to untested recovery, MPI or multi-user scenarios.

## 8. CLI & Directive Reference

### Monitoring & Management

| Command | Option | Description |
| :--- | :--- | :--- |
| `stat` | `[jobID]` | Filter by a specific job ID (supports `.master` suffix) |
| `stat` | `--style pbs` | Use PBS-compatible tabular output |
| `stat` | `-u <user>` | Filter jobs by specific username |
| `stat` | `-H`, `--history [N]` | Show recent job history (last N jobs) |
| `del` | `<job_id>` | Cancel a pending or running job |

In v0.9.6, cancelling a pending job preserves its
submission metadata in `queue/failed` with `status: CANCELLED` and `end_at` set to
the cancellation time. No start time or exit code is invented for an unstarted
job. View cancelled jobs using `stat -H` or a job-ID filter; PBS-style output uses
`E`, as for running-job cancellation. History retention and archiving still apply.

### Job Submission Options (`fbqueue sub`)

FBQueue supports many options common in HPC schedulers for easy migration.

| Option | Parameter | Description | Default |
| :--- | :--- | :--- | :--- |
| `-c`, `--cost` | `cost` | Resource weight per job | `1` |
| `-N`, `-J` | `name` | Job name (for display) | Command name |
| `-q`, `--queue` | `queue` | Target queue name | `default_queue` |
| `-W` | `walltime` | Execution time limit (`HH:MM:SS`) | Unlimited |
| `-hold_jid` | `depend` | Wait for completion of job ID(s) | - |
| `-a` | `start_after` | Scheduled start time (ts, +1h, 18:00) | Immediate |
| `-o` | `stdout` | Redirect path for stdout | `<cwd>/<name>.o<id>` |
| `-e` | `stderr` | Redirect path for stderr | `<cwd>/<name>.e<id>` |
| `--range` | - | Submit range of jobs (`N-M`) | - |
| `--list` | - | Submit list of jobs (`A,B,C`) | - |

### Supported Script Directives

FBQueue maps a subset of embedded script directives from the first 100 lines to internal job parameters. Only the supported forms below are recognized; existing PBS, SGE, or Slurm scripts may need changes. CLI options override the corresponding script settings.

| Feature | External Notation (Example) | Internal Parameter |
| :--- | :--- | :--- |
| **Job Name** | `#PBS -N name`, `#SBATCH -J name` | **`name`** |
| **Resource/Cost** | `#$ -pe smp 4`, `#SBATCH -c 4`, `#PBS -l ncpus=4` | **`cost`** |
| **Target Queue** | `#PBS -q express` | **`queue`** |
| **Stdout Path** | `#$ -o /path/to/log`, `#SBATCH -o ...` | **`stdout`** |
| **Stderr Path** | `#PBS -e /path/to/err`, `#SBATCH -e ...` | **`stderr`** |
| **Dependencies** | `#$ -hold_jid 123` | **`depend`** |
| **Walltime** | `#$ -l h_rt=01:30:00` | **`walltime`** |

The parser uses whitespace-separated tokens, not full scheduler syntax. Slurm's `#SBATCH -p`/`--partition`, long options such as `--job-name`, and `--option=value` forms are not recognized. Use `fbqueue sub -q express script.sh` to select a queue for a Slurm-style script. PBS resource forms such as `select=...` and `walltime=...` are not recognized; the supported walltime directive is `-l h_rt=HH:MM:SS`.

## 9. Job Environment Variables (PBS Compatibility)

FBQueue automatically injects environment variables into the job execution context to ensure compatibility with scripts designed for traditional HPC schedulers like PBS.

| Variable | Description |
| :--- | :--- |
| **`$PBS_JOBID`** | The unique job ID assigned by FBQueue (e.g., `147`). |
| **`$PBS_JOBNAME`** | The name of the job (defaults to the script path if not specified). |
| **`$PBS_O_WORKDIR`** | The absolute path to the directory where the job was submitted. |
| **`$PBS_QUEUE`** | The name of the queue where the job is executing. |
| **`$PBS_NODEFILE`** | Path to a temporary file containing the execution hostname repeated once per requested slot (`cost`). OpenMPI/`mpiexec` reads this to determine the number of available slots, so `mpiexec -n N lmp` works without `--oversubscribe` when `cost >= N`. Deleted after job completion. |
| **`$PBS_ENVIRONMENT`** | Set to `PBS_BATCH` to indicate the job is running in a batch environment. |
| **`$PBS_O_HOST`** | The hostname of the machine where the job was submitted. |
| **`$PBS_O_LOGNAME`** | The username of the user who submitted the job. |

## 10. Roadmap

*   [x] **Background Archiving**: Automatic `tar.gz` compression of old job records during idle periods.
*   [x] **PBS-style Compatibility Subset**: Support for selected environment variables and script directives listed above.
*   [x] **Windows Native Execution**: Support for `.ps1` and `.bat` scripts with automatic path handling.
*   [ ] **Detailed Analytics**: Advanced statistics and status summaries via `fbqueue stat`.

---
### Author
**Forblaze Project**  
Website: [https://forblaze-works.com/en/](https://forblaze-works.com/en/)

### License
This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
