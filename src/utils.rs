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

pub fn format_time(ts: u64) -> String {
    let mut rem = ts;
    let ss = rem % 60; rem /= 60;
    let mm = rem % 60; rem /= 60;
    let hh = rem % 24; rem /= 24;
    let mut days = rem;
    let mut year = 1970;
    loop {
        let leap = (year % 4 == 0 && (year % 100 != 0 || year % 400 == 0));
        let y_days = if leap { 366 } else { 365 };
        if days < y_days { break; }
        days -= y_days;
        year += 1;
    }
    let leap = (year % 4 == 0 && (year % 100 != 0 || year % 400 == 0));
    let mut m_days = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    if leap { m_days[1] = 29; }
    let mut month = 1;
    for &d in &m_days {
        if days < d { break; }
        days -= d;
        month += 1;
    }
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", year, month, days + 1, hh, mm, ss)
}

pub fn parse_time(s: &str) -> u64 {
    if s.starts_with('+') {
        let now = get_now();
        let unit = s.chars().last().unwrap_or(' ');
        let val_str = if unit.is_ascii_digit() { &s[1..] } else { &s[1..s.len()-1] };
        let val = val_str.parse::<u64>().unwrap_or(0);
        return match unit {
            'm' => now + val * 60,
            'h' => now + val * 3600,
            'd' => now + val * 86400,
            's' => now + val,
            _ => if unit.is_ascii_digit() { now + val } else { now }
        };
    }
    if s.contains('-') || s.contains(':') {
        // Simple YYYY-MM-DD HH:MM:SS or HH:MM:SS
        let parts: Vec<&str> = s.split(|c| c == ' ' || c == '-' || c == ':').collect();
        let now = get_now();
        let mut rem = now; rem /= 60; rem /= 60; rem /= 24;
        let mut days = rem;
        let mut year = 1970;
        loop {
            let leap = (year % 4 == 0 && (year % 100 != 0 || year % 400 == 0));
            let y_days = if leap { 366 } else { 365 };
            if days < y_days { break; }
            days -= y_days;
            year += 1;
        }
        let leap = (year % 4 == 0 && (year % 100 != 0 || year % 400 == 0));
        let mut m_days = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
        if leap { m_days[1] = 29; }
        let mut month = 1;
        let mut d_rem = days;
        for &d in &m_days {
            if d_rem < d { break; }
            d_rem -= d;
            month += 1;
        }
        let day = d_rem + 1;

        let (mut y, mut mo, mut d, mut h, mut mi, mut se) = (year as i32, month as u32, day as u32, 0u32, 0u32, 0u32);
        
        if parts.len() >= 5 { // YYYY MM DD HH MM [SS]
            y = parts[0].parse().unwrap_or(y);
            mo = parts[1].parse().unwrap_or(mo);
            d = parts[2].parse().unwrap_or(d);
            h = parts[3].parse().unwrap_or(0);
            mi = parts[4].parse().unwrap_or(0);
            if parts.len() >= 6 { se = parts[5].parse().unwrap_or(0); }
        } else if parts.len() >= 2 { // HH MM [SS]
            h = parts[0].parse().unwrap_or(0);
            mi = parts[1].parse().unwrap_or(0);
            if parts.len() >= 3 { se = parts[2].parse().unwrap_or(0); }
        }

        // Convert back to timestamp
        let mut total_days = 0u64;
        for yr in 1970..y {
            let leap = (yr % 4 == 0 && (yr % 100 != 0 || yr % 400 == 0));
            total_days += if leap { 366 } else { 365 };
        }
        let leap = (y % 4 == 0 && (y % 100 != 0 || y % 400 == 0));
        let mut m_days = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
        if leap { m_days[1] = 29; }
        for m in 1..mo { total_days += m_days[m as usize - 1] as u64; }
        total_days += (d - 1) as u64;
        
        let ts = total_days * 86400 + (h as u64) * 3600 + (mi as u64) * 60 + (se as u64);
        if ts < now && parts.len() < 5 { // If HH:MM is in the past, assume tomorrow
            return ts + 86400;
        }
        return ts;
    }
    s.parse::<u64>().unwrap_or(0)
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
