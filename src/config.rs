use std::fs;
use crate::utils;

pub struct QueueConfig {
    pub name: String,
    pub capacity: usize,
    pub priority: i32,
}

pub struct Config {
    pub default_queue: String,
    pub global_capacity: usize,
    pub inactivity_timeout: u64,
    pub history_limit: usize,
    pub archive_interval_days: u64,
    pub queues: Vec<QueueConfig>,
}

pub fn get_config() -> Config {
    let fbq_dir = utils::get_fbq_dir();
    let config_path = fbq_dir.join("config");
    
    let mut default_queue = "default".to_string();
    let mut global_capacity = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(8).min(8);
    let mut inactivity_timeout = 300;
    let mut history_limit = 100;
    let mut archive_interval_days = 30;
    let mut queues = Vec::new();

    if let Ok(content) = fs::read_to_string(config_path) {
        let mut current_queue: Option<QueueConfig> = None;
        for line in content.lines() {
            let is_indented = line.starts_with(' ') || line.starts_with('\t');
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') { continue; }
            
            if trimmed.starts_with("default_queue: ") {
                default_queue = trimmed[15..].trim().to_string();
            } else if trimmed.starts_with("inactivity_timeout: ") {
                inactivity_timeout = trimmed[20..].trim().parse().unwrap_or(300);
            } else if trimmed.starts_with("history_limit: ") {
                history_limit = trimmed[15..].trim().parse().unwrap_or(100);
            } else if trimmed.starts_with("archive_interval_days: ") {
                archive_interval_days = trimmed[23..].trim().parse().unwrap_or(30);
            } else if trimmed.starts_with("queue: ") {
                if let Some(q) = current_queue.take() {
                    add_or_replace_queue(&mut queues, q);
                }
                current_queue = Some(QueueConfig {
                    name: line[7..].trim().to_string(),
                    capacity: global_capacity,
                    priority: 10,
                });
            } else if line.starts_with("capacity: ") {
                let val = line[10..].trim().parse().unwrap_or(1);
                if is_indented && current_queue.is_some() {
                    if let Some(ref mut q) = current_queue { q.capacity = val; }
                } else {
                    global_capacity = val;
                }
            } else if line.starts_with("priority: ") {
                let val = line[10..].trim().parse().unwrap_or(10);
                if let Some(ref mut q) = current_queue { q.priority = val; }
            }
        }
        if let Some(q) = current_queue {
            add_or_replace_queue(&mut queues, q);
        }
    }
    
    if !queues.iter().any(|q| q.name == default_queue) {
        queues.push(QueueConfig {
            name: default_queue.clone(),
            capacity: global_capacity,
            priority: 10,
        });
    }

    Config { default_queue, global_capacity, inactivity_timeout, history_limit, archive_interval_days, queues }
}

fn add_or_replace_queue(queues: &mut Vec<QueueConfig>, q: QueueConfig) {
    if let Some(pos) = queues.iter().position(|xq| xq.name == q.name) {
        queues[pos] = q;
    } else {
        queues.push(q);
    }
}