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
    for i in 0..args.len() {
        if args[i] == "--style" && i + 1 < args.len() {
            style = &args[i+1];
        }
    }

    let new_entries: Vec<_> = fs::read_dir(fbq_dir.join("queue/new")).map(|d| d.filter_map(|e| e.ok()).collect()).unwrap_or_default();
    let running_entries: Vec<_> = fs::read_dir(fbq_dir.join("queue/running")).map(|d| d.filter_map(|e| e.ok()).collect()).unwrap_or_default();
    let done_count = fs::read_dir(fbq_dir.join("queue/done")).map(|d| d.count()).unwrap_or(0);
    let failed_count = fs::read_dir(fbq_dir.join("queue/failed")).map(|d| d.count()).unwrap_or(0);

    let mut used_caps = HashMap::new();
    let mut running_jobs = Vec::new();
    let mut pending_jobs = Vec::new();
    let mut total_used = 0;

    for entry in &running_entries {
        if let Ok(j) = job::parse_job_file(&entry.path()) {
            *used_caps.entry(j.queue.clone()).or_insert(0) += j.cost;
            total_used += j.cost;
            running_jobs.push(j);
        }
    }
    let mut sorted_new = new_entries;
    sorted_new.sort_by_key(|e| e.file_name().to_str().unwrap_or("0").trim_end_matches(".job").parse::<usize>().unwrap_or(0));
    for entry in sorted_new {
        if let Ok(j) = job::parse_job_file(&entry.path()) { pending_jobs.push(j); }
    }

    let has_pending = !pending_jobs.is_empty();
    let has_running = !running_entries.is_empty();

    if style == "pbs" {
        print_pbs_style(running_jobs, pending_jobs);
    } else {
        println!("FBQueue Status (Global Capacity: {}/{}):", total_used, conf.global_capacity);
        for q in &conf.queues {
            let used = used_caps.get(&q.name).unwrap_or(&0);
            println!("  Queue: {:<10} | Capacity: {:>2}/{:<2} | Priority: {:>3}", q.name, used, q.capacity, q.priority);
        }
        println!("  Done: {}, Failed: {}", done_count, failed_count);

        let now = utils::get_now();
        if has_pending {
            println!("\nPending Jobs:");
            for j in &pending_jobs {
                let wait_reason = if let Some(sa) = j.start_after {
                    if now < sa { format!("Wait until {}", sa) } else { "Capacity".to_string() }
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
    
    // Ensure daemon is running if there are pending or running jobs
    if has_pending || has_running {
        daemon::ensure_daemon();
    }
}

fn print_pbs_style(mut running: Vec<job::Job>, mut pending: Vec<job::Job>) {
    println!("{:<16}  {:<16} {:<16}  {:<8} S {:<5}", "Job id", "Name", "User", "Time Use", "Queue");
    println!("{:-<16}  {:-<16} {:-<16}  {:-<8} - {:-<5}", "", "", "", "", "");

    let now = utils::get_now();
    running.sort_by_key(|j| j.id.parse::<usize>().unwrap_or(0));
    pending.sort_by_key(|j| j.id.parse::<usize>().unwrap_or(0));

    for j in running {
        let elapsed = if let Some(start) = j.start_at { now - start } else { 0 };
        let time_use = format!("{:02}:{:02}:{:02}", elapsed / 3600, (elapsed % 3600) / 60, elapsed % 60);
        println!("{:<16}  {:<16} {:<16}  {:<8} R {:<5}", format!("{}.master", j.id), truncate(&j.name, 16), truncate(&j.user, 16), time_use, j.queue);
    }
    for j in pending {
        println!("{:<16}  {:<16} {:<16}  {:<8} Q {:<5}", format!("{}.master", j.id), truncate(&j.name, 16), truncate(&j.user, 16), "0", j.queue);
    }
}

fn truncate(s: &str, len: usize) -> String {
    if s.len() > len { s[..len].to_string() } else { s.to_string() }
}
