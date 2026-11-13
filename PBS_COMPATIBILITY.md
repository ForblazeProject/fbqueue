# FBQueue - PBS Compatibility Guide

FBQueue is designed with deep respect for the established workflows of High-Performance Computing (HPC) environments. While it is a lightweight tool for local and personal use, it maintains strong compatibility with the conventions of the **Portable Batch System (PBS)** and **Sun Grid Engine (SGE)**.

This guide is for users who are accustomed to these traditional schedulers and wish to apply their existing scripts and habits to FBQueue.

---

## 1. Setting up PBS-style Aliases

On Linux, you can interact with FBQueue using the familiar `qsub`, `qstat`, and `qdel` commands by creating symbolic links to the `fbqueue` binary.

In Windows v0.9.6, aliases are intentionally disabled.
Keep the filename `fbqueue.exe` and use `sub`, `stat` and `del` explicitly;
`fbqueue.exe stat --style pbs` retains PBS-style formatting. Copied/renamed aliases
are rejected with status 2, not interpreted as commands or jobs. Linux aliases
below are unchanged. Published v0.9.5 does not include this Windows policy change.

### Installation via Symbolic Links
Run the following commands in a directory that is in your `$PATH` (e.g., `~/bin` or `/usr/local/bin`):

```bash
# Assuming 'fbqueue' binary is in the current directory
ln -s fbqueue qsub
ln -s fbqueue qstat
ln -s fbqueue qdel
```

Once linked, FBQueue automatically detects how it was invoked and adjusts its behavior:
- `qsub` behaves like `fbqueue sub` (Outputs only `<id>.master` upon success)
- `qstat` behaves like `fbqueue stat --style pbs` (Supports `qstat <jobID>`, `-u <user>`, and `-H`)
- `qdel` behaves like `fbqueue del`

In v0.9.6, submission requires a positive integer
`cost` (default 1 if unspecified) that does not exceed the global or target queue
capacity. Invalid requests and missing commands return status 2 with diagnostics
on stderr, no stdout job ID, and no registered job. `qsub --help` returns 0 and
prints usage to stdout. This CLI requires a command/script argument; it does not
read a job script from stdin when called without one.

Pending cancellations now retain `CANCELLED` history, including the original job
metadata and cancellation time but no fabricated process exit code. Query it with
`qstat -H` or `qstat <jobID>`; the PBS-style terminal status is `E`.

---

## 2. Using PBS Directives in Scripts

FBQueue parses a subset of `#PBS` (and `#$`, `#SBATCH`) directives embedded in the first 100 lines of your script. Existing scripts may need changes: this is not a full PBS, SGE, or Slurm implementation. CLI options override the corresponding script settings.

### Supported Directives Mapping

| PBS/SGE Directive | FBQueue Action |
| :--- | :--- |
| `#PBS -N <name>` | Sets the job display name |
| `#PBS -q <queue>` | Routes the job to a specific FBQueue queue |
| `#PBS -l nodes=1:ppn=N` | Maps `ppn` to the job's resource `cost` |
| `#PBS -l ncpus=N` | Maps `ncpus` to the job's resource `cost` |
| `#$ -pe smp N` | Maps `N` to the job's resource `cost` |
| `#PBS -o <path>` | Redirects standard output |
| `#PBS -e <path>` | Redirects standard error |
| `#PBS -hold_jid <id>` | Sets job dependency (wait for ID to finish successfully) |
| `#PBS -l h_rt=HH:MM:SS` | Sets the Walltime execution limit |

These are FBQueue-supported forms, not a complete reference for the original schedulers. Parsing uses whitespace-separated tokens. PBS `-l walltime=...` and `-l select=...`, Slurm `#SBATCH -p`/`--partition`, and long `--option=value` forms are not recognized. For a Slurm-style script, select the FBQueue queue with `fbqueue sub -q <queue> script.sh`.

### Example Script (`my_job.sh`)
```bash
#!/bin/bash
#PBS -N Simulation_v1
#PBS -q express
#PBS -l nodes=1:ppn=4
#PBS -l h_rt=00:30:00

echo "Running simulation on 4 cores..."
./my_solver
```

Submit it simply with:
```bash
qsub my_job.sh
```

---

## 3. PBS-style Status Monitoring

When invoked as `qstat`, FBQueue provides a tabular output format familiar to HPC users.

### Example `qstat` Output:
```text
Job id            Name             User              Time Use S Queue
----------------  ---------------- ----------------  -------- - -----
123.master        Simulation_v1    username          00:15:22 R express
124.master        Analysis_task    username          00:00:00 Q batch
```

- **S (Status)**:
    - `R`: Running
    - `Q`: Queued (Pending)
    - `H`: Held because execution outcome is unknown (Linux v0.9.6)
    - `F`: Finished successfully (history or job-ID lookup)
    - `E`: Failed, timed out, cancelled, or interrupted (history or job-ID lookup)
- **Time Use**: Displays the elapsed walltime for running jobs.

In Linux v0.9.6, `UNKNOWN` is not successful completion and
pauses new dispatch globally. A recovered unknown job is not automatically
re-executed or safely cancellable through an old PID. `INTERRUPTED` history does
not satisfy successful-completion dependencies. `H` is FBQueue's conservative
display choice, not a claim of full PBS hold/resume command support.

---

## 4. Standard PBS Environment Variables

FBQueue injects standard PBS environment variables into your job's execution context. This allows your scripts to remain portable across different clusters.

| Variable | Description |
| :--- | :--- |
| **`$PBS_JOBID`** | The unique job ID assigned by FBQueue. |
| **`$PBS_O_WORKDIR`** | The directory from which the job was submitted. |
| **`$PBS_NODEFILE`** | Path to a temporary file containing the local execution hostname repeated once per requested slot (`cost`). v0.9.6 rejects zero-cost submissions. |
| **`$PBS_O_HOST`** | The hostname of the submission machine. |
| **`$PBS_O_LOGNAME`** | The username of the user who submitted the job. |
| **`$PBS_ENVIRONMENT`** | Set to `PBS_BATCH` to indicate a batch job environment. |

---

## 5. Why use FBQueue for PBS Workflows?

- **Personal Sandbox**: Run your PBS scripts on your local workstation or a shared server where you don't have administrative rights to a full PBS cluster.
- **Portability**: Reuse scripts that use the supported directive subset; review unsupported scheduler options before moving scripts between a cluster and your local machine.
- **Lightweight**: Get the benefits of a robust queue manager without the overhead of complex system-wide installations.

---
### Author
**Forblaze Project**  
Website: [https://forblaze-works.com/en/](https://forblaze-works.com/en/)

### License
This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

---
*FBQueue: Honoring the heritage of HPC scheduling while providing modern, lightweight local execution.*
