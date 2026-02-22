use std::env;
use std::fs;
use std::process;
use std::thread;
use std::time::{Duration};
use crate::utils;
use crate::job;
use std::path::{Path, PathBuf};

#[cfg(unix)]
unsafe extern "C" { fn setsid() -> i32; }

pub fn ensure_daemon() {
    let lock_file = utils::get_fbq_dir().join("run/daemon.pid");
    let is_running = if let Ok(pid_str) = fs::read_to_string(&lock_file) {
        if let Ok(pid) = pid_str.trim().parse::<u32>() {
            #[cfg(unix)] { process::Command::new("kill").arg("-0").arg(pid.to_string()).output().map(|o| o.status.success()).unwrap_or(false) }
            #[cfg(windows)] { process::Command::new("tasklist").arg("/FI").arg(format!("PID eq {}", pid)).output().map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string())).unwrap_or(false) }
        } else { false }
    } else { false };

    if !is_running {
        let exe = env::current_exe().expect("Failed to get exe path");
        let mut cmd = process::Command::new(exe);
        cmd.arg("daemon").arg("start");
        #[cfg(unix)] {
            use std::os::unix::process::CommandExt;
            cmd.stdin(process::Stdio::null()).stdout(process::Stdio::null()).stderr(process::Stdio::null());
            unsafe { cmd.pre_exec(|| { setsid(); Ok(()) }); }
        }
        #[cfg(windows)] { use std::os::windows::process::CommandExt; cmd.creation_flags(0x08000000); }
        cmd.spawn().ok();
    }
}

pub fn run_daemon() {
    let fbq_dir = utils::get_fbq_dir();
    let lock_file = fbq_dir.join("run/daemon.pid");
    if let Ok(pid_str) = fs::read_to_string(&lock_file) {
        if let Ok(pid) = pid_str.trim().parse::<u32>() {
            let is_alive = {
                #[cfg(unix)] { process::Command::new("kill").arg("-0").arg(pid.to_string()).output().map(|o| o.status.success()).unwrap_or(false) }
                #[cfg(windows)] { process::Command::new("tasklist").arg("/FI").arg(format!("PID eq {}", pid)).output().map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string())).unwrap_or(false) }
            };
            if is_alive { return; }
        }
    }
    fs::write(&lock_file, process::id().to_string()).ok();

    if let Ok(entries) = fs::read_dir(fbq_dir.join("queue/running")) {
        for entry in entries.filter_map(|e| e.ok()) {
            let dest = fbq_dir.join("queue/new").join(entry.file_name());
            fs::rename(entry.path(), dest).ok();
        }
    }
    
    // (JobID, ChildProcess, CostUsed, QueueName)
    let mut running_jobs: Vec<(String, process::Child, usize, String)> = Vec::new();
    let mut idle_seconds = 0;

    loop {
        let config = utils::get_config();
        
        if let Ok(entries) = fs::read_dir(fbq_dir.join("queue/cancel")) {
            for entry in entries.filter_map(|e| e.ok()) {
                let id = entry.file_name().to_str().unwrap().trim_end_matches(".job").to_string();
                if let Some(pos) = running_jobs.iter().position(|(rid, _, _, _)| rid == &id) {
                    let (_, mut child, _, _) = running_jobs.remove(pos);
                    let _ = child.kill();
                    let _ = child.wait();
                    fs::rename(fbq_dir.join("queue/running").join(entry.file_name()), fbq_dir.join("queue/failed").join(entry.file_name())).ok();
                }
                let _ = fs::remove_file(entry.path());
            }
        }

        let mut i = 0;
        while i < running_jobs.len() {
            if let Ok(Some(status)) = running_jobs[i].1.try_wait() {
                let (id, _, _, _) = running_jobs.remove(i);
                let fname = format!("{}.job", id);
                let dest = if status.success() { "queue/done" } else { "queue/failed" };
                fs::rename(fbq_dir.join("queue/running").join(&fname), fbq_dir.join(dest).join(&fname)).ok();
            } else { i += 1; }
        }

        if let Ok(entries) = fs::read_dir(fbq_dir.join("queue/new")) {
            let mut job_list = Vec::new();
            for entry in entries.filter_map(|e| e.ok()) {
                if let Ok(j) = job::parse_job_file(&entry.path()) {
                    let q_prio = config.queues.iter().find(|q| q.name == j.queue).map(|q| q.priority).unwrap_or(0);
                    let numeric_id = j.id.parse::<usize>().unwrap_or(0);
                    job_list.push((entry, j, q_prio, numeric_id));
                }
            }

            // Sort: Priority (desc), then ID (asc)
            job_list.sort_by(|a, b| {
                b.2.cmp(&a.2).then_with(|| a.3.cmp(&b.3))
            });

            for (entry, j, _, _) in job_list {
                let global_used: usize = running_jobs.iter().map(|(_, _, c, _)| *c).sum();
                let q_limit = config.queues.iter().find(|q| q.name == j.queue).map(|q| q.capacity).unwrap_or(1);
                let q_used: usize = running_jobs.iter().filter(|(_, _, _, qn)| qn == &j.queue).map(|(_, _, c, _)| *c).sum();

                // Check both global and queue limits
                let can_run_global = global_used == 0 || (global_used + j.cost <= config.global_capacity);
                let can_run_queue = q_used == 0 || (q_used + j.cost <= q_limit);

                if can_run_global && can_run_queue {
                    let rpath = fbq_dir.join("queue/running").join(entry.file_name());
                    if fs::rename(entry.path(), &rpath).is_ok() {
                        let resolve_path = |p: Option<String>, suffix: &str| {
                            p.map(|s| {
                                let path = Path::new(&s);
                                if path.is_absolute() { PathBuf::from(&s) } else { j.cwd.join(path) }
                            }).unwrap_or_else(|| {
                                j.cwd.join(format!("{}.{}{}", j.name, suffix, j.id))
                            }).display().to_string()
                        };
                        let stdout_path = resolve_path(j.stdout.clone(), "o");
                        let stderr_path = resolve_path(j.stderr.clone(), "e");

                        if let Ok(out_f) = fs::OpenOptions::new().create(true).append(true).open(&stdout_path) {
                            let err_f = if stdout_path == stderr_path { out_f.try_clone().unwrap() }
                                        else { fs::OpenOptions::new().create(true).append(true).open(&stderr_path).unwrap_or(out_f.try_clone().unwrap()) };

                            if let Ok(child) = process::Command::new(&j.cmd).args(&j.args).current_dir(&j.cwd).envs(j.envs).stdout(out_f).stderr(err_f).spawn() {
                                running_jobs.push((j.id, child, j.cost, j.queue));
                            } else { fs::rename(rpath, fbq_dir.join("queue/failed").join(entry.file_name())).ok(); }
                        }
                    }
                }
            }
        }

        if running_jobs.is_empty() && fs::read_dir(fbq_dir.join("queue/new")).map(|d| d.count()).unwrap_or(0) == 0 { idle_seconds += 1; }
        else { idle_seconds = 0; }
        if idle_seconds > 300 { let _ = fs::remove_file(&lock_file); break; }
        thread::sleep(Duration::from_secs(1));
    }
}
