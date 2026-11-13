use std::env;
use std::fs;
use std::process;
use crate::utils;
use crate::job;
use crate::daemon;

pub fn handle_sub(args: &[String]) {
    let alias = !cfg!(windows) && std::path::Path::new(&args[0]).file_stem().and_then(|s| s.to_str()) == Some("qsub");
    let usage = if alias { "qsub" } else if cfg!(windows) { "fbqueue.exe sub" } else { "fbqueue sub" };
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
        if arg == "--help" || arg == "-h" {
            println!("Usage: {} [options] <command> [args...]\n  -c COST  Resource cost (positive integer; default: 1)\n  -q QUEUE  Queue name\n  --range N-M  Batch expansion\n  --list A,B  List expansion", usage);
            return;
        }
        if arg == "--" {
            start_idx += 1;
            break;
        }
        if matches!(arg.as_str(), "-c" | "--cost" | "-q" | "--queue" | "-N" | "-J" | "-W" | "-hold_jid" | "-a" | "-o" | "-e" | "--range" | "--list") && start_idx + 1 == args.len() {
            submission_error(&format!("Missing value for option '{}'", arg));
        }
        if (arg == "-c" || arg == "--cost") && start_idx + 1 < args.len() {
            cli_cost = Some(job::parse_cost(&args[start_idx+1]).unwrap_or_else(|e| submission_error(&e)));
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
            range = None;
            let parts: Vec<_> = args[start_idx+1].split('-').collect();
            if parts.len() == 2 {
                if let (Ok(s), Ok(e)) = (parts[0].parse::<i32>(), parts[1].parse::<i32>()) {
                    if s <= e { range = Some((s, e)); }
                }
            }
            if range.is_none() {
                submission_error("Invalid range: expected N-M with N <= M");
            }
            start_idx += 2;
        } else if arg == "--list" && start_idx + 1 < args.len() {
            list = args[start_idx+1].split(',').map(|s| s.to_string()).collect();
            if list.iter().any(|value| value.is_empty()) {
                submission_error("Invalid list: values must not be empty");
            }
            start_idx += 2;
        } else if arg.starts_with('-') {
            submission_error(&format!("Unknown option '{}'", arg));
        } else { break; }
    }

    if args.len() <= start_idx || args[start_idx].is_empty() {
        eprintln!("Error: No command specified.\nUsage: {} [options] <command> [args...]\nTry '{} --help' for help.", usage, usage);
        process::exit(2);
    }

    let cmd_tmpl = &args[start_idx];
    let args_tmpl = &args[start_idx+1..];
    let cwd = env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    
    let values: Vec<Option<String>> = if let Some((s, e)) = range { (s..=e).map(|i| Some(i.to_string())).collect() }
                                      else if !list.is_empty() { list.into_iter().map(Some).collect() }
                                      else { vec![None] };

    // Validate against the same generated defaults that the daemon will read.
    // Directory/config initialization does not allocate IDs or register jobs.
    utils::init_dirs();
    let mut prepared = Vec::new();
    for val in values {
        prepared.push(job::prepare_job(cmd_tmpl, args_tmpl, &cwd, val.as_deref(), cli_cost, cli_name.clone(), cli_out.clone(), cli_err.clone(), cli_queue.clone(), cli_walltime, cli_depend.clone(), cli_start_after).unwrap_or_else(|e| submission_error(&e)));
    }
    for content in prepared {
        job::register_job(&content).unwrap_or_else(|e| submission_error(&e.to_string()));
    }
    daemon::ensure_daemon();
}

pub fn handle_del(args: &[String]) {
    let job_id = if args.len() > 2 && args[1] == "del" { &args[2] } else if args.len() > 1 { &args[1] } else { return; };
    let job_id = job_id.strip_suffix(".master").unwrap_or(job_id);
    if job_id.is_empty() || !job_id.bytes().all(|b| b.is_ascii_digit()) {
        submission_error("Invalid job ID");
    }
    let fbq_dir = utils::get_fbq_dir();
    let new_path = fbq_dir.join("queue/new").join(format!("{}.job", job_id));
    let running_path = fbq_dir.join("queue/running").join(format!("{}.job", job_id));
    let cancel_path = fbq_dir.join("queue/cancel").join(format!("{}.job", job_id));

    let moved = match fs::rename(&new_path, &cancel_path) {
        Ok(()) => true,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => submission_error(&format!("Cannot cancel job {}: {}", job_id, e)),
    };
    if moved { println!("Job {} cancelled.", job_id); }
    else if running_path.exists() {
        use std::io::ErrorKind;
        match fs::OpenOptions::new().write(true).create_new(true).open(&cancel_path) {
            Ok(_) => println!("Job {} marked for cancellation.", job_id),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => println!("Job {} already marked for cancellation.", job_id),
            Err(e) => submission_error(&format!("Cannot cancel job {}: {}", job_id, e)),
        }
    }
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
            #[cfg(target_os = "linux")]
            if let Err(error) = crate::recovery::stop(&utils::get_fbq_dir()) {
                eprintln!("Error: cannot stop daemon safely: {}", error);
                process::exit(1);
            }
            #[cfg(not(target_os = "linux"))]
            {
            let lock_file = utils::get_fbq_dir().join("run/daemon.pid");
            if let Ok(pid_str) = fs::read_to_string(&lock_file) {
                if let Ok(pid) = pid_str.trim().parse::<u32>() {
                    #[cfg(unix)] { process::Command::new("kill").arg(pid.to_string()).status().ok(); }
                    #[cfg(windows)] { process::Command::new("taskkill").arg("/PID").arg(pid.to_string()).arg("/F").arg("/T").status().ok(); }
                }
            }
            let _ = fs::remove_file(lock_file);
            }
        }
        "status" => {
            #[cfg(target_os = "linux")]
            match crate::recovery::status(&utils::get_fbq_dir()) {
                Ok(status) => println!("{}", status),
                Err(error) => { eprintln!("Error: daemon identity unverified: {}", error); process::exit(1); }
            }
            #[cfg(not(target_os = "linux"))]
            {
            let lock_file = utils::get_fbq_dir().join("run/daemon.pid");
            if let Ok(pid) = fs::read_to_string(&lock_file) { println!("Daemon running PID: {}", pid); }
            else { println!("Daemon not running."); }
            }
        }
        _ => {}
    }
}
fn submission_error(message: &str) -> ! {
    eprintln!("Error: {}", message);
    process::exit(2)
}
