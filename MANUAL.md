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
9.  [Roadmap](#9-roadmap)

---

## 1. Core Philosophy

FBQueue is designed to provide robust job scheduling without the administrative overhead or security risks associated with traditional enterprise software.

*   **Zero Dependency & Zero Config**: Built exclusively with the Rust standard library. A single binary that runs entirely in user-space without root access.
*   **Security-First Architecture**:
    - **No Network Ports**: Operates strictly via the file system. It avoids exposing any network surface, making it ideal for restricted corporate or research environments.
    - **No Database Engine**: Uses a transparent, file-based state management system. No complex database setup or maintenance is required.
*   **Resource Efficiency (Auto-Shutdown)**: The daemon is a transient process designed to conserve system resources. It automatically terminates after a configurable period of inactivity (`inactivity_timeout`, default: 300s).
    - **Trigger Conditions**: The inactivity countdown begins **only** when both of the following conditions are met:
        1.  No jobs are currently executing (the `running/` directory is empty).
        2.  No jobs are waiting in the queue (the `new/` directory is empty).
    - **Behavior**: If a new job is submitted or a scheduled job is waiting for its start time (remaining in the `new/` directory), the daemon stays active. This ensures that scheduled tasks are never missed due to premature daemon termination.
*   **Zero Management (Transparent Daemon)**: The daemon is launched on-demand by any CLI command (`sub`, `stat`, `del`) if it is not already running. Combined with the auto-shutdown feature, users never need to manually start or stop the background process; the tool behaves like a standard CLI utility.
*   **Transparency**: All job states are visible and manageable directly through the file system, allowing for easy inspection and debugging.
*   **Robustness & Resilience**: Includes automatic recovery for interrupted jobs following a daemon restart or system reboot.

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

## 5. Multi-Queue and Priority Support

Queues are defined in the `config` file to control resource distribution and execution order.

```text
capacity: 16          # Global maximum capacity
default_queue: batch
inactivity_timeout: 300 # Seconds to wait before daemon auto-shutdown (default: 300)
history_limit: 100      # Number of completed/failed jobs to keep in direct history (default: 100)
archive_interval_days: 30 # Days between bundling pending archives into a tar.gz (default: 30)

queue: batch
  capacity: 8         # Maximum capacity for this queue
  priority: 10        # Lower priority value

queue: express
  capacity: 4         # Maximum capacity for this queue
  priority: 100       # Higher priority value (processed first)
```

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

### Interpreter Selection
The appropriate interpreter is automatically selected based on the file extension:
- **`.bat`, `.cmd`**: Executed via `cmd /c`.
- **`.ps1`**: Executed via `powershell -ExecutionPolicy Bypass -File`.

### Secure Script Execution
Jobs are executed via their respective shells without modifying the original script's file permissions (`+x` on Linux). This ensures script integrity and avoids security issues in shared environments.

## 8. CLI & Directive Reference

### Monitoring & Management

| Command | Option | Description |
| :--- | :--- | :--- |
| `stat` | `[jobID]` | Filter by a specific job ID (supports `.master` suffix) |
| `stat` | `--style pbs` | Use PBS-compatible tabular output |
| `stat` | `-u <user>` | Filter jobs by specific username |
| `stat` | `-H`, `--history [N]` | Show recent job history (last N jobs) |
| `del` | `<job_id>` | Cancel a pending or running job |

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

FBQueue automatically maps embedded script directives to internal job parameters. This allows you to use your existing Slurm or PBS scripts without modification.

| Feature | External Notation (Example) | Internal Parameter |
| :--- | :--- | :--- |
| **Job Name** | `#PBS -N name`, `#SBATCH -J name` | **`name`** |
| **Resource/Cost** | `#$ -pe smp 4`, `#SBATCH -c 4`, `#PBS -l ncpus=4` | **`cost`** |
| **Target Queue** | `#PBS -q express`, `#SBATCH -p express` | **`queue`** |
| **Stdout Path** | `#$ -o /path/to/log`, `#SBATCH -o ...` | **`stdout`** |
| **Stderr Path** | `#PBS -e /path/to/err`, `#SBATCH -e ...` | **`stderr`** |
| **Dependencies** | `#$ -hold_jid 123` | **`depend`** |
| **Walltime** | `#$ -l h_rt=01:30:00` | **`walltime`** |

## 9. Roadmap

*   [x] **Background Archiving**: Automatic `tar.gz` compression of old job records during idle periods.
*   *   **Flexible Directory Discovery**: Support for recursive parent directory search for `.fbqueue` configuration.
*   *   **Detailed Analytics**: Advanced statistics and status summaries via `fbqueue stat`.

---
### Author
**Forblaze Project**  
Website: [https://forblaze-works.com/en/](https://forblaze-works.com/en/)

### License
This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
