# FBQueue (Forblaze Queue)

![FBQueue Logo Placeholder](https://via.placeholder.com/150/0000FF/FFFFFF?text=FBQueue)

FBQueue is a lightweight, robust local job scheduler written in Rust. It is designed for researchers and developers who need efficient job management on shared computing resources without the complexity of system-wide installations.

## 💡 Why FBQueue?

FBQueue addresses the gap between manual script execution and heavy enterprise schedulers like Slurm or PBS.

- **Polite Resource Sharing**: Manage your own jobs "politely" on multi-user servers. Limit your personal resource consumption (CPU/GPU) to ensure fair access for everyone.
- **Personal Scheduler in Restricted Environments**: Get advanced scheduling (dependencies, priorities, walltime) on any server, even without root access or a system-wide scheduler.
- **Enterprise-Grade Security & Simplicity**:
    - **No Network Ports**: Operates entirely via the file system. No firewall rules or port exposures required.
    - **No Database**: Uses a transparent file-based state management system.
    - **Auto-Shutdown Daemon**: The daemon process automatically terminates after 5 minutes of inactivity, ensuring it doesn't linger as a persistent background process.
- **Zero-Config Deployment**: A single, dependency-free binary that runs in user-space.

## 🎨 Key Features

- **Resource-Aware Scheduling**: Flexible control using abstract `capacity` and job `cost`.
- **Multi-Queue & Priority Support**: Configure multiple queues with different priorities and limits.
- **PBS/HPC Compatibility**: Supports PBS-style commands and parses embedded script directives (`#PBS`, `#SBATCH`, etc.). Detailed usage is available in the [MANUAL](MANUAL.md).
- **Batch Processing**: Simplified submission for parameter studies using `--range` and `--list`.
- **Resilience**: Automatically recovers and resumes interrupted jobs after a system reboot or daemon restart.

## 🚀 Quick Start

### Job Submission (`sub`)
FBQueue automatically handles path prefixes and shell selection.

```bash
# Linux
fbqueue sub my_script.sh

# Windows
fbqueue sub my_script.ps1
```

### Status & Management
```bash
fbqueue stat          # Check job status and resource usage
fbqueue del <job_id>  # Delete/Cancel a job
```

## 📂 Environment & Project Isolation

By default, FBQueue stores its data in `~/.fbqueue/` (the user's home directory), keeping your queue private. 

For team collaboration or project-specific management, you can point multiple users to the same directory using the `FBQUEUE_DIR` environment variable, enabling a shared lightweight scheduler for a specific workgroup:

```bash
export FBQUEUE_DIR=/path/to/shared_project/.fbqueue
fbqueue sub ./calc.sh
```

---
*Monday, February 23, 2026 - Documentation updated for international release.*