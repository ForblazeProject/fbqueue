use std::fs;
use std::collections::HashMap;
use crate::utils;
use crate::job;
use crate::config;
use crate::daemon;

pub fn handle_stat(args: &[String], default_style: &str) {
    utils::init_dirs();
    let fbq_dir = utils::get_fbq_dir();
    let conf = config::get_config();
    
    let mut style = default_style;
    let mut show_history = None;
    let mut filter_user = None;
    let mut filter_job_id = None;

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "stat" || arg.ends_with("fbqueue") || arg.ends_with("qstat") {
            i += 1;
            continue;
        }
        if arg == "--style" && i + 1 < args.len() {
            style = &args[i+1];
            i += 2;
        } else if arg == "-H" || arg == "--history" {
            if i + 1 < args.len() && args[i+1].parse::<usize>().is_ok() {
                show_history = Some(args[i+1].parse::<usize>().unwrap_or(conf.history_limit));
                i += 2;
            } else {
                show_history = Some(conf.history_limit);
                i += 1;
            }
        } else if arg == "-u" && i + 1 < args.len() {
            filter_user = Some(args[i+1].clone());
            i += 2;
        } else if !arg.starts_with('-') {
            // Positional argument: Job ID (handle both "123" and "123.master")
            let id_part = arg.split('.').next().unwrap_or(arg);
            if id_part.chars().next().map_or(false, |c| c.is_ascii_digit()) {
                filter_job_id = Some(id_part.to_string());
            }
            i += 1;
        } else {
            i += 1;
        }
    }
    // eprintln!("DEBUG: filter_job_id={:?}, filter_user={:?}, style={}", filter_job_id, filter_user, style);

    let new_entries: Vec<_> = fs::read_dir(fbq_dir.join("queue/new")).map(|d| d.filter_map(|e| e.ok()).collect()).unwrap_or_default();
    let running_entries: Vec<_> = fs::read_dir(fbq_dir.join("queue/running")).map(|d| d.filter_map(|e| e.ok()).collect()).unwrap_or_default();

    let mut used_caps = HashMap::new();
    let mut running_jobs = Vec::new();
    let mut pending_jobs = Vec::new();
    let mut total_used = 0;

    for entry in &running_entries {
        if let Ok(j) = job::parse_job_file(&entry.path()) {
            if let Some(ref fid) = filter_job_id { 
                let jid_norm = j.id.split('.').next().unwrap_or(&j.id);
                if jid_norm != fid { continue; } 
            }
            if let Some(ref u) = filter_user { if &j.user != u { continue; } }
            *used_caps.entry(j.queue.clone()).or_insert(0) += j.cost;
            total_used += j.cost;
            running_jobs.push(j);
        }
    }
    
    let mut sorted_new = new_entries;
    sorted_new.sort_by_key(|e| e.file_name().to_str().unwrap_or("0").trim_end_matches(".job").parse::<usize>().unwrap_or(0));
    for entry in sorted_new {
        if let Ok(j) = job::parse_job_file(&entry.path()) {
            if let Some(ref fid) = filter_job_id { 
                let jid_norm = j.id.split('.').next().unwrap_or(&j.id);
                if jid_norm != fid { continue; } 
            }
            if let Some(ref u) = filter_user { if &j.user != u { continue; } }
            pending_jobs.push(j);
        }
    }

    let mut history_jobs = Vec::new();
    if show_history.is_some() || filter_job_id.is_some() {
        let limit = show_history.unwrap_or(conf.history_limit);
        for dir in &["queue/done", "queue/failed"] {
            if let Ok(entries) = fs::read_dir(fbq_dir.join(dir)) {
                for entry in entries.filter_map(|e| e.ok()) {
                    if let Ok(j) = job::parse_job_file(&entry.path()) {
                        if let Some(ref fid) = filter_job_id { 
                            let jid_norm = j.id.split('.').next().unwrap_or(&j.id);
                            if jid_norm != fid { continue; } 
                        }
                        if let Some(ref u) = filter_user { if &j.user != u { continue; } }
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

    if style == "pbs" {
        print_pbs_style(running_jobs, pending_jobs, history_jobs, show_history.is_some() || filter_job_id.is_some());
    } else {
        if let Some(limit) = show_history {
            println!("Recent Job History (Last {}):", limit);
            for j in history_jobs {
                let status = j.status.as_deref().unwrap_or("DONE");
                println!("  ID: {:>4} | NAME: {:<15} | USER: {:<10} | QUEUE: {:<10} | STATUS: {}", j.id, j.name, j.user, j.queue, status);
            }
        } else {
            println!("FBQueue Status (Global Capacity: {}/{}):", total_used, conf.global_capacity);
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
                    println!("  ID: {:>4} | NAME: {:<15} | USER: {:<10} | QUEUE: {:<10} | COST: {} | TIME: {}{}s", j.id, j.name, j.user, j.queue, j.cost, elapsed, walltime_str);
                }
            }
        }
    }
    
    // Ensure daemon is running if there are pending or running jobs
    if has_pending || has_running {
        daemon::ensure_daemon();
    }
}

fn print_pbs_style(mut running: Vec<job::Job>, mut pending: Vec<job::Job>, history: Vec<job::Job>, is_history_mode: bool) {
    println!("{:<16}  {:<16} {:<16}  {:<8} S {:<5}", "Job id", "Name", "User", "Time Use", "Queue");
    println!("{:-<16}  {:-<16} {:-<16}  {:-<8} - {:-<5}", "", "", "", "", "");

    let now = utils::get_now();
    
    if !is_history_mode {
        running.sort_by_key(|j| j.id.parse::<usize>().unwrap_or(0));
        for j in running {
            let elapsed = if let Some(start) = j.start_at { now - start } else { 0 };
            let time_use = format!("{:02}:{:02}:{:02}", elapsed / 3600, (elapsed % 3600) / 60, elapsed % 60);
            println!("{:<16}  {:<16} {:<16}  {:<8} R {:<5}", format!("{}.master", j.id), truncate(&j.name, 16), truncate(&j.user, 16), time_use, j.queue);
        }
        pending.sort_by_key(|j| j.id.parse::<usize>().unwrap_or(0));
        for j in pending {
            println!("{:<16}  {:<16} {:<16}  {:<8} Q {:<5}", format!("{}.master", j.id), truncate(&j.name, 16), truncate(&j.user, 16), "0", j.queue);
        }
    } else {
        for j in history {
            let status_char = match j.status.as_deref() {
                Some("DONE") => "F",
                Some("FAILED") | Some("CANCELLED") | Some("TIMEOUT") => "E",
                _ => "F",
            };
            println!("{:<16}  {:<16} {:<16}  {:<8} {} {:<5}", format!("{}.master", j.id), truncate(&j.name, 16), truncate(&j.user, 16), "0", status_char, j.queue);
        }
    }
}

fn truncate(s: &str, len: usize) -> String {
    if s.len() > len { s[..len].to_string() } else { s.to_string() }
}
