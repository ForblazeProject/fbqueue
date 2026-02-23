use std::env;
use std::fs;
use std::process;
use crate::utils;
use crate::job;
use crate::daemon;

pub fn handle_sub(args: &[String]) {
    utils::init_dirs();
    let mut start_idx = if args.len() > 1 && args[1] == "sub" { 2 } else { 1 };
    
    let mut range: Option<(i32, i32)> = None;
    let mut list: Vec<String> = Vec::new();
    let mut cli_cost: Option<usize> = None;
    let mut cli_name: Option<String> = None;
    let mut cli_out: Option<String> = None;
    let mut cli_err: Option<String> = None;
    let mut cli_queue: Option<String> = None;
    let mut cli_walltime: Option<u64> = None;
    let mut cli_depend: Option<String> = None;
    let mut cli_start_after: Option<u64> = None;

    while start_idx < args.len() {
        let arg = &args[start_idx];
        if (arg == "-c" || arg == "--cost") && start_idx + 1 < args.len() {
            cli_cost = Some(args[start_idx+1].parse().unwrap_or(1));
            start_idx += 2;
        } else if (arg == "-q" || arg == "--queue") && start_idx + 1 < args.len() {
            cli_queue = Some(args[start_idx+1].to_string());
            start_idx += 2;
        } else if (arg == "-N" || arg == "-J") && start_idx + 1 < args.len() {
            cli_name = Some(args[start_idx+1].to_string());
            start_idx += 2;
        } else if arg == "-W" && start_idx + 1 < args.len() {
            cli_walltime = Some(utils::parse_duration(&args[start_idx+1]));
            start_idx += 2;
        } else if arg == "-hold_jid" && start_idx + 1 < args.len() {
            cli_depend = Some(args[start_idx+1].to_string());
            start_idx += 2;
        } else if arg == "-a" && start_idx + 1 < args.len() {
            cli_start_after = Some(utils::parse_time(&args[start_idx+1]));
            start_idx += 2;
        } else if arg == "-o" && start_idx + 1 < args.len() {
            cli_out = Some(args[start_idx+1].to_string());
            start_idx += 2;
        } else if arg == "-e" && start_idx + 1 < args.len() {
            cli_err = Some(args[start_idx+1].to_string());
            start_idx += 2;
        } else if arg == "--range" && start_idx + 1 < args.len() {
            let parts: Vec<_> = args[start_idx+1].split('-').collect();
            if parts.len() == 2 {
                if let (Ok(s), Ok(e)) = (parts[0].parse::<i32>(), parts[1].parse::<i32>()) { range = Some((s, e)); }
            }
            start_idx += 2;
        } else if arg == "--list" && start_idx + 1 < args.len() {
            list = args[start_idx+1].split(',').map(|s| s.to_string()).collect();
            start_idx += 2;
        } else { break; }
    }

    if args.len() <= start_idx {
        eprintln!("Error: No command specified.");
        return;
    }

    let cmd_tmpl = &args[start_idx];
    let args_tmpl = &args[start_idx+1..];
    let cwd = env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    
    let values: Vec<Option<String>> = if let Some((s, e)) = range { (s..=e).map(|i| Some(i.to_string())).collect() }
                                      else if !list.is_empty() { list.into_iter().map(Some).collect() }
                                      else { vec![None] };

    for val in values {
        job::submit_job(cmd_tmpl, args_tmpl, &cwd, val.as_deref(), cli_cost, cli_name.clone(), cli_out.clone(), cli_err.clone(), cli_queue.clone(), cli_walltime, cli_depend.clone(), cli_start_after);
    }
    daemon::ensure_daemon();
}

pub fn handle_del(args: &[String]) {
    let job_id = if args.len() > 2 && args[1] == "del" { &args[2] } else if args.len() > 1 { &args[1] } else { return; };
    let fbq_dir = utils::get_fbq_dir();
    let new_path = fbq_dir.join("queue/new").join(format!("{}.job", job_id));
    let running_path = fbq_dir.join("queue/running").join(format!("{}.job", job_id));
    let cancel_path = fbq_dir.join("queue/cancel").join(format!("{}.job", job_id));

    if new_path.exists() { fs::rename(new_path, cancel_path).ok(); println!("Job {} cancelled.", job_id); }
    else if running_path.exists() { fs::write(cancel_path, "").ok(); println!("Job {} marked for cancellation.", job_id); }
    else { println!("Job {} not found.", job_id); }
    daemon::ensure_daemon();
}

pub fn handle_daemon(args: &[String]) {
    if args.len() < 3 { return; }
    match args[2].as_str() {
        "start" => {
            if args.len() > 3 {
                env::set_var("FBQUEUE_DIR", &args[3]);
            }
            daemon::run_daemon();
        },
        "stop" => {
            let lock_file = utils::get_fbq_dir().join("run/daemon.pid");
            if let Ok(pid_str) = fs::read_to_string(&lock_file) {
                if let Ok(pid) = pid_str.trim().parse::<u32>() {
                    #[cfg(unix)] { process::Command::new("kill").arg(pid.to_string()).status().ok(); }
                    #[cfg(windows)] { process::Command::new("taskkill").arg("/PID").arg(pid.to_string()).arg("/F").status().ok(); }
                }
            }
            let _ = fs::remove_file(lock_file);
        }
        "status" => {
            let lock_file = utils::get_fbq_dir().join("run/daemon.pid");
            if let Ok(pid) = fs::read_to_string(&lock_file) { println!("Daemon running PID: {}", pid); }
            else { println!("Daemon not running."); }
        }
        _ => {}
    }
}