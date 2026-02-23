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
    pub queues: Vec<QueueConfig>,
}

pub fn get_config() -> Config {
    let fbq_dir = utils::get_fbq_dir();
    let config_path = fbq_dir.join("config");
    
    let mut default_queue = "default".to_string();
    let mut global_capacity = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(8).min(8);
    let mut queues = Vec::new();

    if let Ok(content) = fs::read_to_string(config_path) {
        let mut current_queue: Option<QueueConfig> = None;
        for line in content.lines() {
            let is_indented = line.starts_with(' ') || line.starts_with('\t');
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') { continue; }
            
            if line.starts_with("default_queue: ") {
                default_queue = line[15..].trim().to_string();
            } else if line.starts_with("queue: ") {
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

    Config { default_queue, global_capacity, queues }
}

fn add_or_replace_queue(queues: &mut Vec<QueueConfig>, q: QueueConfig) {
    if let Some(pos) = queues.iter().position(|xq| xq.name == q.name) {
        queues[pos] = q;
    } else {
        queues.push(q);
    }
}