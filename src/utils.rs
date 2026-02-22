use std::env;
use std::fs;
use std::path::PathBuf;

pub fn get_fbq_dir() -> PathBuf {
    let dir = if let Ok(d) = env::var("FBQUEUE_DIR") {
        PathBuf::from(d)
    } else {
        let home = env::var("HOME").or_else(|_| env::var("USERPROFILE"))
            .expect("Could not find HOME, USERPROFILE, or FBQUEUE_DIR");
        PathBuf::from(home).join(".fbqueue")
    };
    // Ensure the directory exists
    if !dir.exists() {
        let _ = fs::create_dir_all(&dir);
    }
    dir
}

pub fn init_dirs() {
    let fbq_dir = get_fbq_dir();
    let subdirs = ["queue/new", "queue/running", "queue/done", "queue/failed", "queue/cancel", "logs", "run"];
    for subdir in &subdirs {
        let path = fbq_dir.join(subdir);
        if !path.exists() {
            fs::create_dir_all(&path).ok();
        }
    }
}

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
    let fbq_dir = get_fbq_dir();
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

pub fn get_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()
}

pub fn parse_duration(s: &str) -> u64 {
    let parts: Vec<&str> = s.split(':').collect();
    match parts.len() {
        3 => {
            let h = parts[0].parse::<u64>().unwrap_or(0);
            let m = parts[1].parse::<u64>().unwrap_or(0);
            let s = parts[2].parse::<u64>().unwrap_or(0);
            h * 3600 + m * 60 + s
        }
        2 => {
            let m = parts[0].parse::<u64>().unwrap_or(0);
            let s = parts[1].parse::<u64>().unwrap_or(0);
            m * 60 + s
        }
        1 => parts[0].parse::<u64>().unwrap_or(0),
        _ => 0,
    }
}

pub fn get_next_id() -> String {
    let fbq_dir = get_fbq_dir();
    let lock_dir = fbq_dir.join("run/id.lock");
    let id_file = fbq_dir.join("run/last_id");

    let mut acquired = false;
    for _ in 0..100 {
        if fs::create_dir(&lock_dir).is_ok() { acquired = true; break; }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    let mut next_id: usize = 1;
    if let Ok(content) = fs::read_to_string(&id_file) {
        if let Ok(last) = content.trim().parse::<usize>() { next_id = last + 1; }
    } else {
        let subdirs = ["queue/new", "queue/running", "queue/done", "queue/failed", "queue/cancel"];
        for subdir in &subdirs {
            let path = fbq_dir.join(subdir);
            if let Ok(entries) = fs::read_dir(path) {
                for entry in entries.filter_map(|e| e.ok()) {
                    if let Some(name) = entry.file_name().to_str() {
                        if name.ends_with(".job") {
                            if let Ok(id) = name.trim_end_matches(".job").parse::<usize>() {
                                if id >= next_id { next_id = id + 1; }
                            }
                        }
                    }
                }
            }
        }
    }
    fs::write(&id_file, next_id.to_string()).ok();
    if acquired { let _ = fs::remove_dir(&lock_dir); }
    next_id.to_string()
}