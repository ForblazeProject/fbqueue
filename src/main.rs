mod utils;
mod job;
mod daemon;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();
    let program_name = Path::new(&args[0])
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("fbqueue");

    let command = if program_name == "qsub" { "sub" }
    else if program_name == "qstat" { "stat" }
    else if program_name == "qdel" { "del" }
    else if args.len() < 2 { print_help(); return; }
    else { &args[1] };

    match command {
        "sub" => handle_sub(&args),
        "stat" => handle_stat(),
        "del" => handle_del(&args),
        "daemon" => handle_daemon(&args),
        "help" | "--help" | "-h" => print_help(),
        _ => {
            if program_name == "fbqueue" {
                eprintln!("Unknown command: {}", command);
                print_help();
                process::exit(1);
            } else {
                handle_sub(&args);
            }
        }
    }
}

fn print_help() {
    println!("FBQueue (Forblaze Queue) - Simple local job scheduler");
    println!("Usage:");
    println!("  fbqueue sub [options] <command> [args...] (alias: qsub)");
    println!("  fbqueue stat                              (alias: qstat)");
    println!("  fbqueue del <job_id>                      (alias: qdel)");
    println!("  fbqueue daemon <start|stop|status>");
    println!("\nOptions for sub:");
    println!("  -q QUEUE        Queue name");
    println!("  -c COST         Resource cost (default: 1)");
    println!("  -N NAME         Job name");
    println!("  -W HH:MM:SS     Walltime limit");
    println!("  -hold_jid ID    Wait for job ID to finish");
    println!("  -a TIMESTAMP    Start after UNIX timestamp");
    println!("  -o OUT          Standard output file");
    println!("  -e ERR          Standard error file");
    println!("  --range N-M     Batch range expansion");
}

fn handle_sub(args: &[String]) {
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
            cli_start_after = Some(args[start_idx+1].parse().unwrap_or(0));
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
        process::exit(1);
    }

    let cmd_tmpl = &args[start_idx];
    let args_tmpl = &args[start_idx+1..];
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    
    let values: Vec<Option<String>> = if let Some((s, e)) = range { (s..=e).map(|i| Some(i.to_string())).collect() }
                                      else if !list.is_empty() { list.into_iter().map(Some).collect() }
                                      else { vec![None] };

    for val in values {
        job::submit_job(cmd_tmpl, args_tmpl, &cwd, val.as_deref(), cli_cost, cli_name.clone(), cli_out.clone(), cli_err.clone(), cli_queue.clone(), cli_walltime, cli_depend.clone(), cli_start_after);
    }
    daemon::ensure_daemon();
}

fn handle_stat() {
    utils::init_dirs();
    let fbq_dir = utils::get_fbq_dir();
    let config = utils::get_config();

    let mut new_entries: Vec<_> = fs::read_dir(fbq_dir.join("queue/new")).map(|d| d.filter_map(|e| e.ok()).collect()).unwrap_or_default();
    let running_entries: Vec<_> = fs::read_dir(fbq_dir.join("queue/running")).map(|d| d.filter_map(|e| e.ok()).collect()).unwrap_or_default();
    let done_count = fs::read_dir(fbq_dir.join("queue/done")).map(|d| d.count()).unwrap_or(0);
    let failed_count = fs::read_dir(fbq_dir.join("queue/failed")).map(|d| d.count()).unwrap_or(0);

    let mut used_caps = std::collections::HashMap::new();
    let mut running_jobs = Vec::new();
    let mut total_used = 0;
    for entry in &running_entries {
        if let Ok(j) = job::parse_job_file(&entry.path()) {
            *used_caps.entry(j.queue.clone()).or_insert(0) += j.cost;
            total_used += j.cost;
            running_jobs.push(j);
        }
    }

    println!("FBQueue Status (Global Capacity: {}/{}):", total_used, config.global_capacity);
    for q in &config.queues {
        let used = used_caps.get(&q.name).unwrap_or(&0);
        println!("  Queue: {:<10} | Capacity: {:>2}/{:<2} | Priority: {:>3}", q.name, used, q.capacity, q.priority);
    }
    println!("  Done: {}, Failed: {}", done_count, failed_count);

    let now = utils::get_now();
    if !new_entries.is_empty() {
        println!("\nPending Jobs:");
        new_entries.sort_by_key(|e| e.file_name().to_str().unwrap_or("0").trim_end_matches(".job").parse::<usize>().unwrap_or(0));
        for entry in new_entries {
            if let Ok(j) = job::parse_job_file(&entry.path()) {
                let wait_reason = if let Some(sa) = j.start_after {
                    if now < sa { format!("Wait until {}", sa) } else { "Capacity".to_string() }
                } else if !j.depend.is_empty() {
                    "Dependency".to_string()
                } else { "Capacity".to_string() };
                println!("  ID: {:>4} | NAME: {:<15} | USER: {:<10} | QUEUE: {:<10} | COST: {} | STATUS: Pending ({})", j.id, j.name, j.user, j.queue, j.cost, wait_reason);
            }
        }
    }
    if !running_jobs.is_empty() {
        println!("\nRunning Jobs:");
        running_jobs.sort_by_key(|j| j.id.parse::<usize>().unwrap_or(0));
        for j in running_jobs {
            let elapsed = if let Some(start) = j.start_at { now - start } else { 0 };
            let walltime_str = if let Some(wt) = j.walltime { format!("/{}", wt) } else { "".to_string() };
            println!("  ID: {:>4} | NAME: {:<15} | USER: {:<10} | QUEUE: {:<10} | COST: {} | TIME: {}{}s", j.id, j.name, j.user, j.queue, j.cost, elapsed, walltime_str);
        }
    }
}

fn handle_del(args: &[String]) {
    let job_id = if args.len() > 2 && args[1] == "del" { &args[2] } else if args.len() > 1 { &args[1] } else { return; };
    let fbq_dir = utils::get_fbq_dir();
    let new_path = fbq_dir.join("queue/new").join(format!("{}.job", job_id));
    let running_path = fbq_dir.join("queue/running").join(format!("{}.job", job_id));
    let cancel_path = fbq_dir.join("queue/cancel").join(format!("{}.job", job_id));

    if new_path.exists() { fs::rename(new_path, cancel_path).ok(); println!("Job {} cancelled.", job_id); }
    else if running_path.exists() { fs::write(cancel_path, "").ok(); println!("Job {} marked for cancellation.", job_id); }
    else { println!("Job {} not found.", job_id); }
}

fn handle_daemon(args: &[String]) {
    if args.len() < 3 { return; }
    match args[2].as_str() {
        "start" => daemon::run_daemon(),
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
