use std::fs;
use std::collections::HashMap;
use crate::utils;
use crate::job;
use crate::config;
use crate::daemon;

pub fn handle_stat(args: &[String], default_style: &str) -> Result<(), (i32, String)> {
    let mut style = default_style;
    let mut history = false;
    let mut history_limit = None;
    let mut filter_user = None;
    let mut filter_job_id = None;
    let mut json = false;
    let usage_error = |message| (2, message);

    if matches!(args, [flag] if flag == "--help" || flag == "-h") {
        let frontend = if cfg!(windows) { "fbqueue.exe" } else { "fbqueue" };
        println!("Usage: {frontend} stat [job_id] [--style pbs|default] [-H [N]] [-u USER]");
        println!("       {frontend} stat <job_id> --json");
        println!("  --json             Print id, state, exit_code (null if unavailable), stdout and stderr paths");
        println!("  -H, --history [N]  Show recent job history");
        println!("  -h, --help         Show this help");
        return Ok(());
    }

    let mut args = args.iter().peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--style" => match args.next().map(String::as_str) {
                Some(value @ ("pbs" | "default")) => style = value,
                _ => return Err(usage_error("--style expects pbs or default".into())),
            },
            "-H" | "--history" => {
                history = true;
                history_limit = args.peek().and_then(|value| value.parse::<usize>().ok());
                if history_limit.is_some() {
                    args.next();
                }
            }
            "-u" => {
                filter_user = Some(args.next().filter(|s| !s.is_empty() && !s.starts_with('-'))
                    .ok_or_else(|| usage_error("-u expects a user name".into()))?.clone());
            }
            "--json" => json = true,
            _ if arg.starts_with('-') => return Err(usage_error(format!("unknown option: {arg}"))),
            _ => {
                let id = arg.strip_suffix(".master").unwrap_or(arg);
                if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(usage_error(format!("invalid job ID: {arg}")));
                }
                if filter_job_id.replace(id.to_owned()).is_some() {
                    return Err(usage_error("expected at most one job ID".into()));
                }
            }
        }
    }

    if json && filter_job_id.is_none() {
        return Err(usage_error("--json requires a job ID".into()));
    }
    utils::init_dirs();
    let fbq_dir = utils::get_fbq_dir();
    let conf = config::get_config();
    let show_history = history.then(|| history_limit.unwrap_or(conf.history_limit));
    let mut id_exists = false;
    let mut matches_filters = |j: &job::Job| {
        if let Some(id) = &filter_job_id {
            if j.id.split('.').next() != Some(id.as_str()) {
                return false;
            }
            id_exists = true;
        }
        filter_user.as_ref().is_none_or(|user| j.user == *user)
    };

    let new_entries: Vec<_> = fs::read_dir(fbq_dir.join("queue/new")).map(|d| d.filter_map(|e| e.ok()).collect()).unwrap_or_default();
    let mut used_caps = HashMap::<String, usize>::new();
    let mut running_jobs = Vec::new();
    let mut pending_jobs = Vec::new();
    let mut total_used = 0usize;
    let mut sorted_new = new_entries;
    sorted_new.sort_by_key(|e| e.file_name().to_str().unwrap_or("0").trim_end_matches(".job").parse::<usize>().unwrap_or(0));
    // Read in transition order. If a pending path vanishes on dispatch,
    // the subsequent running or history scan can still find the record.
    for entry in sorted_new {
        if let Ok(j) = job::parse_job_file(&entry.path()) {
            if !matches_filters(&j) { continue; }
            pending_jobs.push(j);
        }
    }

    let running_entries: Vec<_> = fs::read_dir(fbq_dir.join("queue/running")).map(|d| d.filter_map(|e| e.ok()).collect()).unwrap_or_default();
    let hold_reason = fs::read_to_string(fbq_dir.join("run/recovery.hold")).ok();
    let recovery_held = hold_reason.is_some() || running_entries.iter().any(|entry| {
        job::parse_job_file(&entry.path()).map(|j| j.status.as_deref() == Some("UNKNOWN")).unwrap_or(true)
    });

    for entry in &running_entries {
        if let Ok(j) = job::parse_job_file(&entry.path()) {
            if !matches_filters(&j) { continue; }
            let used = used_caps.entry(j.queue.clone()).or_insert(0);
            *used = used.saturating_add(j.cost.max(1));
            total_used = total_used.saturating_add(j.cost.max(1));
            running_jobs.push(j);
        }
    }
    
    // A record successfully read before dispatch may also appear as running.
    if !pending_jobs.is_empty() && !running_jobs.is_empty() {
        let running_ids: std::collections::HashSet<_> = running_jobs.iter().map(|j| &j.id).collect();
        pending_jobs.retain(|j| !running_ids.contains(&j.id));
    }

    let mut history_jobs = Vec::new();
    if show_history.is_some() || filter_job_id.is_some() {
        let limit = show_history.unwrap_or(conf.history_limit);
        for dir in &["queue/done", "queue/failed"] {
            if let Ok(entries) = fs::read_dir(fbq_dir.join(dir)) {
                for entry in entries.filter_map(|e| e.ok()) {
                    if let Ok(mut j) = job::parse_job_file(&entry.path()) {
                        if !matches_filters(&j) { continue; }
                        if json && j.status.is_none() && *dir == "queue/failed" {
                            j.status = Some("FAILED".into());
                        }
                        history_jobs.push(j);
                    }
                }
            }
        }
        history_jobs.sort_by(|a, b| b.id.parse::<usize>().unwrap_or(0).cmp(&a.id.parse::<usize>().unwrap_or(0)));
        if show_history.is_some() {
            history_jobs.truncate(limit);
        }
    }

    let has_pending = !pending_jobs.is_empty();
    let has_running = !running_entries.is_empty();

    if let Some(id) = &filter_job_id {
        let not_found = || (1, format!("job {id} not found"));
        if !id_exists {
            return Err(not_found());
        }
        if json {
            let active = if history { None } else {
                running_jobs.first().map(|j| (j, "RUNNING"))
                    .or_else(|| pending_jobs.first().map(|j| (j, "PENDING")))
            };
            let (job, fallback_state) = active.or_else(|| history_jobs.first().map(|j| (j, "DONE")))
                .ok_or_else(not_found)?;
            print_json(job, fallback_state);
            return Ok(());
        }
    }

    if style == "pbs" {
        // history_only: show only finished jobs (qstat -H)
        // include_history: also show finished jobs alongside active ones (qstat <jobid>)
        let history_only = show_history.is_some();
        let include_history = filter_job_id.is_some();
        print_pbs_style(running_jobs, pending_jobs, history_jobs, history_only, include_history);
    } else {
        if let Some(limit) = show_history {
            println!("Recent Job History (Last {}):", limit);
            for j in history_jobs {
                let status = j.status.as_deref().unwrap_or("DONE");
                println!("  ID: {:>4} | NAME: {:<15} | USER: {:<10} | QUEUE: {:<10} | STATUS: {}", j.id, j.name, j.user, j.queue, status);
            }
        } else {
            println!("FBQueue Status (Global Capacity: {}/{}):", total_used, conf.global_capacity);
            if recovery_held { println!("Recovery hold: {}. New dispatch is paused. No automatic retry.", hold_reason.as_deref().unwrap_or("UNKNOWN execution")); }
            for q in &conf.queues {
                let used = used_caps.get(&q.name).unwrap_or(&0);
                println!("  Queue: {:<10} | Capacity: {:>2}/{:<2} | Priority: {:>3}", q.name, used, q.capacity, q.priority);
            }

            let now = utils::get_now();
            if has_pending {
                println!("\nPending Jobs:");
                for j in &pending_jobs {
                    let wait_reason = if let Some(sa) = j.start_after {
                        if now < sa { format!("Wait until {}", utils::format_time(sa)) } else { "Capacity".to_string() }
                    } else if !j.depend.is_empty() { "Dependency".to_string() }
                    else { "Capacity".to_string() };
                    println!("  ID: {:>4} | NAME: {:<15} | USER: {:<10} | QUEUE: {:<10} | COST: {} | STATUS: Pending ({})", j.id, j.name, j.user, j.queue, j.cost, wait_reason);
                }
            }
            if has_running {
                println!("\nRunning Jobs:");
                running_jobs.sort_by_key(|j| j.id.parse::<usize>().unwrap_or(0));
                for j in running_jobs {
                    let elapsed = if let Some(start) = j.start_at { now - start } else { 0 };
                    let walltime_str = if let Some(wt) = j.walltime { format!("/{}", wt) } else { "".to_string() };
                    println!("  ID: {:>4} | NAME: {:<15} | USER: {:<10} | QUEUE: {:<10} | COST: {} | TIME: {}{}s | STATUS: {}", j.id, j.name, j.user, j.queue, j.cost.max(1), elapsed, walltime_str, j.status.as_deref().unwrap_or("RUNNING"));
                }
            }

            // When filtering by job ID, also show finished jobs from history
            if filter_job_id.is_some() && !history_jobs.is_empty() {
                println!("\nFinished Jobs:");
                let mut sorted_history = history_jobs;
                sorted_history.sort_by_key(|j| j.id.parse::<usize>().unwrap_or(0));
                for j in sorted_history {
                    let status = j.status.as_deref().unwrap_or("DONE");
                    println!("  ID: {:>4} | NAME: {:<15} | USER: {:<10} | QUEUE: {:<10} | STATUS: {}", j.id, j.name, j.user, j.queue, status);
                }
            }
        }
    }
    
    // Ensure daemon is running if there are pending or running jobs
    if has_pending || has_running {
        daemon::ensure_daemon();
    }
    Ok(())
}

fn print_json(job: &job::Job, fallback_state: &str) {
    let state = job.status.as_deref().unwrap_or(fallback_state);
    let exit_code = job._exit_code.map_or_else(|| "null".into(), |code| code.to_string());
    // Use the same explicit/relative/default path rules as the daemon. The
    // paths describe the job's destinations, not whether the files exist yet.
    let output_path = |configured: Option<&str>, suffix: &str| {
        let path = match configured {
            Some(path) if std::path::Path::new(path).is_absolute() => path.into(),
            Some(path) => job.cwd.join(path),
            None => job.cwd.join(format!("{}.{suffix}{}", job.name, job.id)),
        };
        json_string(&path.to_string_lossy())
    };
    println!(
        "{{\"id\":{},\"state\":{},\"exit_code\":{exit_code},\"stdout\":{},\"stderr\":{}}}",
        json_string(&job.id), json_string(state),
        output_path(job.stdout.as_deref(), "o"), output_path(job.stderr.as_deref(), "e"),
    );
}

fn json_string(value: &str) -> String {
    use std::fmt::Write;

    let mut encoded = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => encoded.push_str("\\\""),
            '\\' => encoded.push_str("\\\\"),
            '\n' => encoded.push_str("\\n"),
            '\r' => encoded.push_str("\\r"),
            '\t' => encoded.push_str("\\t"),
            '\u{0}'..='\u{1f}' => write!(encoded, "\\u{:04x}", ch as u32).unwrap(),
            _ => encoded.push(ch),
        }
    }
    encoded.push('"');
    encoded
}

fn print_pbs_style(mut running: Vec<job::Job>, mut pending: Vec<job::Job>, history: Vec<job::Job>, history_only: bool, include_history: bool) {
    println!("{:<16}  {:<16} {:<16}  {:<8} S {:<5}", "Job id", "Name", "User", "Time Use", "Queue");
    println!("{:-<16}  {:-<16} {:-<16}  {:-<8} - {:-<5}", "", "", "", "", "");

    let now = utils::get_now();
    
    if !history_only {
        running.sort_by_key(|j| j.id.parse::<usize>().unwrap_or(0));
        for j in running {
            let elapsed = if let Some(start) = j.start_at { now - start } else { 0 };
            let time_use = format!("{:02}:{:02}:{:02}", elapsed / 3600, (elapsed % 3600) / 60, elapsed % 60);
            let state = if matches!(j.status.as_deref(), Some("UNKNOWN" | "INTERRUPTED")) { "H" } else { "R" };
            println!("{:<16}  {:<16} {:<16}  {:<8} {} {:<5}", format!("{}.master", j.id), truncate(&j.name, 16), truncate(&j.user, 16), time_use, state, j.queue);
        }
        pending.sort_by_key(|j| j.id.parse::<usize>().unwrap_or(0));
        for j in pending {
            println!("{:<16}  {:<16} {:<16}  {:<8} Q {:<5}", format!("{}.master", j.id), truncate(&j.name, 16), truncate(&j.user, 16), "0", j.queue);
        }
    }

    // Show finished jobs when in history-only mode (qstat -H) or when filtering by job ID
    if history_only || include_history {
        let mut sorted_history = history;
        sorted_history.sort_by_key(|j| j.id.parse::<usize>().unwrap_or(0));
        for j in sorted_history {
            let status_char = match j.status.as_deref() {
                Some("DONE") => "F",
                None => "F", // Legacy completed records may not carry status.
                _ => "E", // Unknown outcomes, including INTERRUPTED, are not success.
            };
            println!("{:<16}  {:<16} {:<16}  {:<8} {} {:<5}", format!("{}.master", j.id), truncate(&j.name, 16), truncate(&j.user, 16), "0", status_char, j.queue);
        }
    }
}

fn truncate(s: &str, len: usize) -> String {
    if s.len() > len { s[..len].to_string() } else { s.to_string() }
}
