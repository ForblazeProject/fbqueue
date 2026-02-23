mod utils;
mod job;
mod daemon;
mod config;
mod stat;
mod handlers;

use std::env;
use std::path::Path;

fn main() {
    let args: Vec<String> = env::args().collect();
    let program_name = Path::new(&args[0])
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("fbqueue");

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
    println!("FBQueue (Forblaze Queue) - Simple local job scheduler");
    println!("Usage:");
    println!("  fbqueue sub [options] <command> [args...] (alias: qsub)");
    println!("  fbqueue stat [--style pbs|default]        (alias: qstat)");
    println!("  fbqueue del <job_id>                      (alias: qdel)");
    println!("  fbqueue daemon <start|stop|status>");
    println!("\nOptions for sub:");
    println!("  -q QUEUE        Queue name");
    println!("  -c COST         Resource cost (default: 1)");
    println!("  -N NAME         Job name");
    println!("  -W HH:MM:SS     Walltime limit");
    println!("  -hold_jid ID    Wait for job ID to finish");
    println!("  -a TIMESTAMP    Start after UNIX timestamp");
    println!("  -o OUT          Standard output file");
    println!("  -e ERR          Standard error file");
    println!("  --range N-M     Batch range expansion");
}
