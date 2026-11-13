mod utils;
mod job;
mod daemon;
mod config;
mod stat;
mod handlers;
mod invocation;
#[cfg(target_os = "linux")]
mod recovery;

use std::env;
use std::path::Path;

fn main() {
    let args: Vec<String> = env::args().collect();
    let program_name = Path::new(&args[0])
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("fbqueue");

    // Unix needs argv[0] for symlink aliases. Windows checks the actual file,
    // not an independently supplied argv[0], and never guesses on lookup error.
    let executable = if cfg!(windows) {
        Some(env::current_exe().unwrap_or_else(|error| {
            eprintln!("Error: cannot identify Windows executable: {}", error);
            std::process::exit(2);
        }))
    } else { None };
    let actual_name = executable.as_ref().and_then(|path| path.file_stem()).and_then(|stem| stem.to_str());
    let program_name = invocation::entrypoint_name(program_name, actual_name, cfg!(windows))
        .unwrap_or_else(|message| {
            eprintln!("Error: {}", message);
            std::process::exit(2);
        });

    let command = if program_name == "qsub" { "sub" }
    else if program_name == "qstat" { "stat" }
    else if program_name == "qdel" { "del" }
    else if args.len() < 2 { print_help(); return; }
    else { &args[1] };

    let default_style = if program_name == "qstat" { "pbs" } else { "default" };

    match command {
        "sub" => handlers::handle_sub(&args),
        "stat" => stat::handle_stat(&args, default_style),
        "del" => handlers::handle_del(&args),
        "daemon" => handlers::handle_daemon(&args),
        "help" | "--help" | "-h" => print_help(),
        _ => {
            if program_name == "fbqueue" {
                eprintln!("Unknown command: {}", command);
                print_help();
                std::process::exit(1);
            } else {
                handlers::handle_sub(&args);
            }
        }
    }
}

fn print_help() {
    println!("FBQueue (Forblaze Queue) v{} - Simple local job scheduler", env!("CARGO_PKG_VERSION"));
    println!("Usage:");
    let frontend = if cfg!(windows) { "fbqueue.exe" } else { "fbqueue" };
    let suffix = |name: &str| if cfg!(windows) { String::new() } else { format!(" (alias: {})", name) };
    println!("  {} sub [options] <command> [args...]{}", frontend, suffix("qsub"));
    println!("  {} stat [--style pbs|default] [-H [N]]{}", frontend, suffix("qstat"));
    println!("  {} del <job_id>{}", frontend, suffix("qdel"));
    println!("  {} daemon <start|stop|status>", frontend);
    println!("\nOptions for sub:");
    println!("  -q QUEUE        Queue name");
    println!("  -c COST         Resource cost (default: 1)");
    println!("  -N NAME         Job name");
    println!("  -W HH:MM:SS     Walltime limit");
    println!("  -hold_jid ID    Wait for job ID to finish");
    println!("  -a TIME         Start after TIME (UNIX ts, +1h, 18:00, or YYYY-MM-DD HH:MM)");
    println!("  -o OUT          Standard output file");
    println!("  -e ERR          Standard error file");
    println!("  --range N-M     Batch range expansion");
}
