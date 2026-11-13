# FBQueue (Forblaze Queue)

FBQueue is a lightweight, robust local job scheduler written in Rust. It is designed for researchers and developers who need efficient job management on shared computing resources without the complexity of system-wide installations.

## 💡 Why FBQueue?

FBQueue addresses the gap between manual script execution and heavy enterprise schedulers like Slurm or PBS.

- **Polite Resource Sharing**: Manage your own jobs "politely" on multi-user servers. Limit your personal resource consumption (CPU/GPU) to ensure fair access for everyone.
- **Personal Scheduler in Restricted Environments**: Get advanced scheduling (dependencies, priorities, walltime) on any server, even without root access or a system-wide scheduler.
- **Enterprise-Grade Security & Simplicity**:
    - **No Network Ports**: Operates entirely via the file system. No firewall rules or port exposures required.
    - **No Database**: Uses a transparent file-based state management system.
    - **Auto-Shutdown Daemon**: The daemon automatically terminates after a configurable period of inactivity (default 300s). It only shuts down when both the running and pending queues are completely empty.
- **Zero-Config & Zero Management**: A single binary that runs in user-space, built with the Rust standard library plus the `libc` crate on Unix. The daemon starts automatically on any command (`sub`, `stat`, `del`) and stops when idle—no manual daemon management required.

## 🎨 Key Features

- **Resource-Aware Scheduling**: Flexible control using abstract `capacity` and job `cost`.
- **Multi-Queue & Priority Support**: Configure multiple queues with different priorities and limits.
- **PBS/HPC Compatibility**: Supports PBS-style commands and a subset of embedded script directives (`#PBS`, `#SBATCH`, `#$`), not full scheduler compatibility. Detailed usage and limitations are available in the [MANUAL](MANUAL.md).
- **Batch Processing**: Simplified submission for parameter studies using `--range` and `--list`.
- **Conservative Linux Recovery (v0.9.6)**: Running records with an unknown outcome are held, not automatically replayed. New dispatch pauses until ambiguous work is reconciled. In contrast, v0.9.5 uses replay and can duplicate surviving workers.

## Changes in v0.9.6

v0.9.6 introduces the following behavior changes compared with v0.9.5:

- Submission cost must be a positive integer (default 1), within both the global
  and target queue capacity. Requests that only lack currently free slots wait.
- Submission usage/validation errors return status 2 on stderr without a job ID;
  explicit submission help returns 0 on stdout.
- Cancelling a pending job retains `CANCELLED` history without inventing an exit code.
- Linux daemon ownership uses a permanent lifetime lock and boot/PID/starttime
  identity checked through pidfds. Stale PID files are not signaling authority.
- Unknown running work stays `UNKNOWN` (`H` in PBS output), blocking new dispatch.
  Records proven to belong to another boot become `INTERRUPTED` history (`E`),
  without a fabricated exit code. Legacy records without boot evidence stay held.
- Recovery/cancellation I/O errors and conflicting history retain evidence and
  stop new dispatch. No automatic retry, worker adoption, or supervisor receipts
  are implemented. See [the staged recovery design](https://github.com/ForblazeProject/fbqueue/blob/v0.9.6/docs/DAEMON_RECOVERY_DESIGN.md)
  for the operational limitations and deferred reconciliation interface.

## 🚀 Quick Start

### Installation

#### Linux
Download the v0.9.6 pre-built static binary and place it in your `$PATH`:

```bash
# 1. Download
wget https://github.com/ForblazeProject/fbqueue/releases/download/v0.9.6/fbqueue-linux-x64.tar.gz

# 2. Extract and move
tar -xzvf fbqueue-linux-x64.tar.gz
mkdir -p ~/bin
mv fbqueue/fbqueue ~/bin/

# 3. (Optional) Create PBS-style symbolic links
ln -s ~/bin/fbqueue ~/bin/qsub
ln -s ~/bin/fbqueue ~/bin/qstat
ln -s ~/bin/fbqueue ~/bin/qdel
```

#### Windows
1. Download [fbqueue-windows-x64.zip for v0.9.6](https://github.com/ForblazeProject/fbqueue/releases/download/v0.9.6/fbqueue-windows-x64.zip).
2. Extract the ZIP and move `fbqueue.exe` to a folder in your `Path`.

**Windows CLI in v0.9.6:** use `fbqueue.exe sub`, `fbqueue.exe stat`,
`fbqueue.exe del` and `fbqueue.exe daemon`. Renamed/copied aliases such as
`qsub.exe`, `qstat.exe` and `qdel.exe` are rejected with status 2 before state
initialization. PBS-style output remains available with `fbqueue.exe stat --style pbs`.
Linux aliases are unchanged; this Windows CLI policy starts with v0.9.6.

Windows has native job execution, but not the Linux recovery protocol introduced in v0.9.6.
It still replays unresolved running records on daemon restart; duplicate work is
possible. Windows `daemon stop` uses `taskkill /F /T` and can also kill workers,
unlike the Linux v0.9.6 command. See [Windows support and limitations](MANUAL.md#7-windows-implementation-and-guidelines).
The implementation prepared for v0.9.6 was rebuilt and tested on Windows AMD64
with Windows PowerShell 5.1 on 2026-11-13: all 19 cases passed, with no skips or
errors. This includes PBS-style history/ID filtering and rejection of renamed
aliases. See [Windows verification evidence](https://github.com/ForblazeProject/fbqueue/blob/v0.9.6/docs/WINDOWS_VERIFICATION.md) for
the exact pre-version-bump snapshot and limits, rather than assuming every
release artifact or environment has been independently tested.

### Job Submission (`sub`)
FBQueue automatically handles path prefixes and shell selection.

```bash
# Linux
fbqueue sub my_script.sh

# Windows
fbqueue.exe sub my_script.ps1
```

### Status & Management
```bash
fbqueue stat          # Check active jobs and resource usage
fbqueue stat -H       # Show recent job history (last 100 jobs)
fbqueue del <job_id>  # Delete/Cancel a job
fbqueue sub -a +1h ./task.sh  # Schedule a job to start in 1 hour
```

## 📂 Environment & Project Isolation

By default, FBQueue stores its data in `~/.fbqueue/` (the user's home directory), keeping your queue private. 

For team collaboration on a single machine, you can point multiple users to a **local shared directory** using the `FBQUEUE_DIR` environment variable. Note that using network-mounted drives (NFS/SMB) is discouraged due to potential file-locking latency:

```bash
# Example: Shared local project directory
export FBQUEUE_DIR=/var/lib/fbqueue/project_a
fbqueue sub ./calc.sh
```

## Windows testing

Windows real-machine tests and returnable diagnostic ZIPs are described in
[the Windows test kit guide](https://github.com/ForblazeProject/fbqueue/blob/v0.9.6/tests/windows/README-JA.md). The kit can build a source
snapshot or test an existing `.exe`. Select `Development` for the v0.9.6 behavior
and `Released095` only for the older v0.9.5 release. These are test-profile names,
not automatic version detection: the pre-release snapshot used for Windows
verification still reported v0.9.5 before the version bump.

## 📜 Change Log

### v0.9.6

Reliability-focused update: stricter submission and resource validation, safer
Linux recovery, more reliable job history, and an explicit Windows CLI.

#### Fixes and improvements

- **Strict submission validation**: Cost must be a positive integer, defaults to
  1, and must fit both global and target-queue capacity. Invalid costs, unknown
  queues, malformed range/list expansion, missing option values and missing
  commands are rejected. Usage/validation errors return status 2 on stderr without
  issuing a job ID. Submission help returns 0 on stdout. Expanded submissions are
  validated before registration; this does not make registration transactional
  across storage failures. Explicit CLI settings retain precedence over script
  directives, including a valid CLI cost overriding an invalid script cost.
- **Correct resource scheduling**: Fix ignored indented queue `capacity` and
  `priority` settings. Enforce queue and global limits even when idle, protect
  capacity arithmetic from overflow, and account for legacy zero-cost records as
  one slot. Requests that fit configured limits but lack currently free slots wait.
- **Safer Linux daemon ownership and recovery**: Use a permanent lifetime lock
  and boot/PID/starttime identity verified through pidfds. Do not automatically
  replay unresolved running jobs after daemon failure. Hold ambiguous work as
  `UNKNOWN` and pause new dispatch; finalize records proven to belong to another
  boot as `INTERRUPTED`, without inventing exit codes. Native status reports holds;
  PBS output uses `H` for held work and `E` for interrupted history.
- **Reliable cancellation and terminal history**: Retain pending cancellations as
  `CANCELLED` history with a cancellation time but no fabricated start time or
  exit code. Accept `.master` job IDs for cancellation and preserve records across
  repeated/late cancellation requests. Record spawn failures explicitly as
  `FAILED` with a reason. Sync terminal metadata before moving records; on Linux,
  conflicting history or persistence failures retain evidence and hold dispatch.
  Unconfirmed post-rename directory-sync errors remain latched for intervention.
- **Explicit Windows CLI**: Use `fbqueue.exe sub/stat/del/daemon`. Reject copied
  or renamed aliases such as `qsub.exe`, `qstat.exe` and `qdel.exe` with status 2
  before state initialization, preventing recursive daemon-command submissions.
  Keep `fbqueue.exe stat --style pbs` and Linux aliases.
- **Regression coverage and documentation**: Add Rust unit tests, rebuild the
  isolated Linux integration suite, add CI failure/cleanup checks, and provide a
  Windows test kit with returnable diagnostic ZIPs. The implementation was checked
  with 10 Rust unit tests, 52 Linux integration cases and 19 native Windows cases;
  see [Windows verification evidence](https://github.com/ForblazeProject/fbqueue/blob/v0.9.6/docs/WINDOWS_VERIFICATION.md) for the exact
  pre-release snapshot and environment. Clarify partial PBS/SGE/Slurm compatibility,
  platform differences and recovery limits. Nodefile slot generation was already
  fixed in v0.9.5 and is covered by regression tests here.

#### Compatibility and upgrade notes

- **Stricter inputs and limits**: Previously tolerated invalid or oversized
  requests now fail. Correctly applying queue limits/priorities can change
  scheduling compared with v0.9.5.
- **Linux recovery favors avoiding duplicate execution over automatic progress**:
  `UNKNOWN` remains held even after an orphan exits. Worker adoption, lost-result
  recovery, a reconciliation/retry CLI and exactly-once execution are not provided.
- **Linux requirements**: Usable pidfds and readable process/boot identity under
  `/proc` are required; there is no unsafe bare-PID fallback. The recovery scope is
  one host/PID namespace on persistent local storage, not a distributed scheduler.
- **Windows aliases are no longer supported**. Windows still has its older
  restart/replay behavior, not Linux recovery parity. Windows `daemon stop` can
  kill workers through `taskkill /F /T`; Linux `daemon stop` leaves workers alive.
- **Controlled upgrade required**: Let running jobs finish, stop the old daemon,
  then switch binaries. Do not run old/new writers against the same state directory
  or delete `daemon.lock` to force startup. Legacy running records without valid
  boot evidence are held, not automatically retried. Confirm surviving processes
  and external effects before any administrative repair or resubmission.

### v0.9.5
- **MPI Parallel Execution Fix**: `$PBS_NODEFILE` now contains one line per requested slot (`cost`), matching the standard PBS specification. This allows `mpiexec -n N` to work without `--oversubscribe` when the job's cost is set appropriately (e.g., `#PBS -l ncpus=4` or `qsub -c 4`).
- **qstat Job ID Filtering Fix**: `qstat <jobid>` now correctly displays jobs in any state (running, pending, or finished) instead of only showing history. Previously, active jobs were hidden when filtering by job ID.

### v0.9.4
- **Shebang Support**: Unix scripts now respect the interpreter defined in their shebang line (e.g., `#!/bin/bash`), resolving issues with `conda` environments and shell-specific syntax.

### v0.9.3
- **PBS Environment Variables**: Jobs now automatically define selected standard PBS variables like `$PBS_JOBID`, `$PBS_O_WORKDIR`, `$PBS_O_HOST`, and `$PBS_O_LOGNAME`.
- **Nodefile Support**: Dynamically generates and cleans up `$PBS_NODEFILE` for compatibility with scripts expecting a list of execution nodes.
- **Improved Metadata Accuracy**: Enhanced hostname and user detection across both Linux and Windows for more reliable job logging.

### v0.9.2
- **Fixed Process Leak**: Improved job cancellation logic using process groups (`setsid`) on Unix and tree termination (`/T`) on Windows. Subprocesses spawned by job scripts are now correctly terminated.
- **Robust Job Filtering**: `qstat <jobID>` (and `stat <jobID>`) now works reliably even if the job has already finished and moved to history. It also correctly handles the `.master` suffix.
- **Dependency Update**: Added `libc` dependency for Unix systems to support advanced process management.

### v0.9.1
- **Enhanced PBS Compatibility**: Added support for `qstat -u <user>` filtering.
- **Improved History Display**: `qstat -H` now correctly shows job history in PBS-style format.
- **PBS-Style Submission**: `qsub` now outputs only the job ID (e.g., `1.master`) upon success, matching traditional HPC behavior.

### v0.9.0
- **International Release**: Documentation translated to English and terminology standardized.
- **Delayed Start**: Added human-readable time support for `-a` option (e.g., `+1h`, `18:00`).
- **Resource Efficiency**: Added configurable `inactivity_timeout` and automatic background archiving of old jobs.
- **Zero Management**: Daemon starts automatically on any command and shuts down when idle.

---
### Author
**Forblaze Project**  
Website: [https://forblaze-works.com/en/](https://forblaze-works.com/en/)

### License
This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.