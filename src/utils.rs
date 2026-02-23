use std::env;
use std::fs;
use std::path::PathBuf;

pub fn get_fbq_dir() -> PathBuf {
    if let Ok(dir) = env::var("FBQUEUE_DIR") {
        return PathBuf::from(dir);
    }
    let home = env::var("HOME").or_else(|_| env::var("USERPROFILE"))
        .expect("Could not find HOME, USERPROFILE, or FBQUEUE_DIR");
    PathBuf::from(home).join(".fbqueue")
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
