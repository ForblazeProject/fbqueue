# FBQueue - PBS Compatibility Guide

FBQueue is designed with deep respect for the established workflows of High-Performance Computing (HPC) environments. While it is a lightweight tool for local and personal use, it maintains strong compatibility with the conventions of the **Portable Batch System (PBS)** and **Sun Grid Engine (SGE)**.

This guide is for users who are accustomed to these traditional schedulers and wish to apply their existing scripts and habits to FBQueue.

---

## 1. Setting up PBS-style Aliases

On Linux, you can interact with FBQueue using the familiar `qsub`, `qstat`, and `qdel` commands by creating symbolic links to the `fbqueue` binary.

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

---

## 2. Using PBS Directives in Scripts

FBQueue respects your existing job scripts. You don't need to rewrite them. It automatically parses the `#PBS` (and `#$`, `#SBATCH`) directives embedded in the first 100 lines of your script.

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
| `#PBS -hold_jid <id>` | Sets job dependency (wait for ID to finish) |
| `#PBS -l h_rt=HH:MM:SS` | Sets the Walltime execution limit |

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
- **Time Use**: Displays the elapsed walltime for running jobs.

---

## 4. Standard PBS Environment Variables

FBQueue injects standard PBS environment variables into your job's execution context. This allows your scripts to remain portable across different clusters.

| Variable | Description |
| :--- | :--- |
| **`$PBS_JOBID`** | The unique job ID assigned by FBQueue. |
| **`$PBS_O_WORKDIR`** | The directory from which the job was submitted. |
| **`$PBS_NODEFILE`** | Path to a file containing the hostname of the execution node. |
| **`$PBS_O_HOST`** | The hostname of the submission machine. |
| **`$PBS_O_LOGNAME`** | The username of the user who submitted the job. |
| **`$PBS_ENVIRONMENT`** | Set to `PBS_BATCH` to indicate a batch job environment. |

---

## 5. Why use FBQueue for PBS Workflows?

- **Personal Sandbox**: Run your PBS scripts on your local workstation or a shared server where you don't have administrative rights to a full PBS cluster.
- **Portability**: Move your research scripts between a massive supercomputer and your local laptop without changing a single line of the `#PBS` directives.
- **Lightweight**: Get the benefits of a robust queue manager without the overhead of complex system-wide installations.

---
### Author
**Forblaze Project**  
Website: [https://forblaze-works.com/en/](https://forblaze-works.com/en/)

### License
This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

---
*FBQueue: Honoring the heritage of HPC scheduling while providing modern, lightweight local execution.*
