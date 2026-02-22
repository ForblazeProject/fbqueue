use std::env;
use std::fs;
use std::io::{self, BufRead};
use std::path::{Path, PathBuf};
use crate::utils;

pub struct Job {
    pub id: String,
    pub name: String,
    pub cmd: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub envs: Vec<(String, String)>,
    pub cost: usize,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub user: String,
    pub queue: String,
    pub walltime: Option<u64>,
    pub depend: Vec<String>,
    pub start_after: Option<u64>,
    pub start_at: Option<u64>,
    pub _end_at: Option<u64>,
    pub _exit_code: Option<i32>,
}

pub fn parse_job_file(path: &Path) -> io::Result<Job> {
    let file = fs::File::open(path)?;
    let reader = io::BufReader::new(file);
    let mut id = String::new();
    let mut name = String::new();
    let mut cmd = String::new();
    let mut args = Vec::new();
    let mut cwd = PathBuf::from(".");
    let mut envs = Vec::new();
    let mut cost = 1;
    let mut stdout = None;
    let mut stderr = None;
    let mut user = String::new();
    let mut queue = "default".to_string();
    let mut walltime = None;
    let mut depend = Vec::new();
    let mut start_after = None;
    let mut start_at = None;
    let mut end_at = None;
    let mut exit_code = None;

    for line in reader.lines() {
        let line = line?;
        if line.starts_with("id: ") { id = line[4..].to_string(); }
        else if line.starts_with("name: ") { name = line[6..].to_string(); }
        else if line.starts_with("cmd: ") { cmd = line[5..].to_string(); }
        else if line.starts_with("arg: ") { args.push(line[5..].to_string()); }
        else if line.starts_with("cwd: ") { cwd = PathBuf::from(&line[5..]); }
        else if line.starts_with("cost: ") { cost = line[6..].parse().unwrap_or(1); }
        else if line.starts_with("stdout: ") { stdout = Some(line[8..].to_string()); }
        else if line.starts_with("stderr: ") { stderr = Some(line[8..].to_string()); }
        else if line.starts_with("user: ") { user = line[6..].to_string(); }
        else if line.starts_with("queue: ") { queue = line[7..].to_string(); }
        else if line.starts_with("walltime: ") { walltime = Some(line[10..].parse().unwrap_or(0)); }
        else if line.starts_with("depend: ") { depend = line[8..].split(',').map(|s| s.trim().to_string()).collect(); }
        else if line.starts_with("start_after: ") { start_after = Some(line[13..].parse().unwrap_or(0)); }
        else if line.starts_with("start_at: ") { start_at = Some(line[10..].parse().unwrap_or(0)); }
        else if line.starts_with("end_at: ") { end_at = Some(line[8..].parse().unwrap_or(0)); }
        else if line.starts_with("exit_code: ") { exit_code = Some(line[11..].parse().unwrap_or(0)); }
        else if line.starts_with("env: ") {
            let part = &line[5..];
            if let Some(pos) = part.find('=') {
                envs.push((part[..pos].to_string(), part[pos+1..].to_string()));
            }
        }
    }
    if name.is_empty() { name = cmd.clone(); }
    Ok(Job { id, name, cmd, args, cwd, envs, cost, stdout, stderr, user, queue, walltime, depend, start_after, start_at, _end_at: end_at, _exit_code: exit_code })
}

pub fn submit_job(cmd_tmpl: &str, args_tmpl: &[String], cwd: &Path, val: Option<&str>, 
                  cli_cost: Option<usize>, cli_name: Option<String>, 
                  cli_out: Option<String>, cli_err: Option<String>,
                  cli_queue: Option<String>, cli_walltime: Option<u64>,
                  cli_depend: Option<String>, cli_start_after: Option<u64>) {
    let replace = |s: &str| if let Some(v) = val { s.replace("{}", v) } else { s.to_string() };
    
    let raw_cmd = replace(cmd_tmpl);
    let mut cmd = raw_cmd.clone();
    
    // Path auto-completion: only prefix with ./ if it exists in cwd and has no separators
    if !cmd.contains('/') && !cmd.contains('\\') {
        let p = cwd.join(&cmd);
        if p.is_file() {
            cmd = format!("./{}", cmd);
        }
    }

    let job_args: Vec<_> = args_tmpl.iter().map(|s| replace(s)).collect();
    let config = utils::get_config();
    let def_q = config.default_queue;
    let mut script_cost = 1;
    let mut script_name = String::new();
    let mut script_out = None;
    let mut script_err = None;
    let mut script_queue = None;
    let mut script_walltime = None;
    let mut script_depend = Vec::new();

    let script_path = if Path::new(&cmd).is_absolute() { PathBuf::from(&cmd) } else { cwd.join(&cmd) };
    if let Ok(file) = fs::File::open(&script_path) {
        let reader = io::BufReader::new(file);
        for line in reader.lines().take(100) {
            if let Ok(l) = line {
                let l = l.trim();
                if l.starts_with("#$") || l.starts_with("#PBS") || l.starts_with("#SBATCH") {
                    let parts: Vec<_> = l.split_whitespace().collect();
                    let mut i = 1;
                    while i < parts.len() {
                        match parts[i] {
                            "-N" | "-J" if i + 1 < parts.len() => { script_name = parts[i+1].to_string(); i += 2; }
                            "-c" | "-n" if i + 1 < parts.len() => { script_cost = parts[i+1].parse().unwrap_or(script_cost); i += 2; }
                            "-q" if i + 1 < parts.len() => { script_queue = Some(parts[i+1].to_string()); i += 2; }
                            "-o" if i + 1 < parts.len() => { script_out = Some(parts[i+1].to_string()); i += 2; }
                            "-e" if i + 1 < parts.len() => { script_err = Some(parts[i+1].to_string()); i += 2; }
                            "-hold_jid" if i + 1 < parts.len() => { script_depend.push(parts[i+1].to_string()); i += 2; }
                            "-l" if i + 1 < parts.len() => {
                                let arg = parts[i+1];
                                if arg.starts_with("h_rt=") {
                                    script_walltime = Some(utils::parse_duration(&arg[5..]));
                                } else if arg.starts_with("nodes=") {
                                    if let Some(pos) = arg.find("ppn=") {
                                        script_cost = arg[pos+4..].parse().unwrap_or(script_cost);
                                    }
                                }
                                i += 2;
                            }
                            "-pe" if i + 2 < parts.len() && parts[i+1] == "smp" => { 
                                script_cost = parts[i+2].parse().unwrap_or(script_cost); i += 3; 
                            }
                            _ => i += 1,
                        }
                    }
                }
            } else { break; }
        }
    }

    let final_cost = cli_cost.unwrap_or(script_cost);
    let final_name = cli_name.unwrap_or(if script_name.is_empty() { raw_cmd.clone() } else { script_name });
    let final_out = cli_out.or(script_out);
    let final_err = cli_err.or(script_err);
    let final_queue = cli_queue.or(script_queue).unwrap_or(def_q);
    let final_walltime = cli_walltime.or(script_walltime);
    let final_depend = cli_depend.map(|s| s.split(',').map(|v| v.trim().to_string()).collect()).unwrap_or(script_depend);

    let job_id = utils::get_next_id();
    let fbq_dir = utils::get_fbq_dir();
    let job_file_path = fbq_dir.join("queue/new").join(format!("{}.job", job_id));

    let current_user = env::var("USER").or_else(|_| env::var("USERNAME")).unwrap_or_else(|_| "unknown".to_string());

    let mut content = format!("id: {}\nname: {}\ncmd: {}\ncost: {}\nuser: {}\nqueue: {}\n", job_id, final_name, cmd, final_cost, current_user, final_queue);
    if let Some(o) = final_out { content.push_str(&format!("stdout: {}\n", o)); }
    if let Some(e) = final_err { content.push_str(&format!("stderr: {}\n", e)); }
    if let Some(w) = final_walltime { content.push_str(&format!("walltime: {}\n", w)); }
    if !final_depend.is_empty() { content.push_str(&format!("depend: {}\n", final_depend.join(","))); }
    if let Some(s) = cli_start_after { content.push_str(&format!("start_after: {}\n", s)); }
    
    for arg in job_args { content.push_str(&format!("arg: {}\n", arg)); }
    content.push_str(&format!("cwd: {}\n", cwd.display()));
    for (key, val) in env::vars() { content.push_str(&format!("env: {}={}\n", key, val)); }

    fs::write(&job_file_path, content).expect("Failed to write job file");
    println!("Job submitted: {} ({}) [queue: {}, cost: {}]", job_id, final_name, final_queue, final_cost);
}
