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
    println!("FBQueue (Forblaze Queue) v{} - Local background job scheduler", env!("CARGO_PKG_VERSION"));
    println!("\nRun commands and scripts in the background, queue resource-intensive work,");
    println!("and track jobs across separate terminal or AI-agent sessions.");
    println!("Use it for simulations, batch processing, and other long-running tasks.");
    println!("\nUsage:");
    let frontend = if cfg!(windows) { "fbqueue.exe" } else { "fbqueue" };
    let suffix = |name: &str| if cfg!(windows) { String::new() } else { format!(" (alias: {})", name) };
    println!("  {} sub [options] <command> [args...]{}", frontend, suffix("qsub"));
    println!("  {} stat [job_id] [--style pbs|default] [-H [N]]{}", frontend, suffix("qstat"));
    println!("  {} del <job_id>{}", frontend, suffix("qdel"));
    println!("  {} daemon <start|stop|status>", frontend);
    println!("  {} help", frontend);
    println!("\nQuick start:");
    let script = if cfg!(windows) { ".\\analysis.ps1" } else { "./analysis.sh" };
    println!("  {} sub -N analysis -o analysis.out -e analysis.err {}", frontend, script);
    println!("  {} stat <job_id>", frontend);
    println!("  {} stat -H", frontend);
    println!("  {} del <job_id>", frontend);
    println!("\nWorkflow for users and AI agents:");
    println!("  1. Submit a job and save the returned job ID.");
    println!("  2. Check that job ID to distinguish queued, running, and finished work.");
    println!("  3. Inspect the final status and output files before reporting success.");
    println!("  4. Reuse the job ID in later sessions instead of submitting the work again.");
    println!("\nSubmission runs asynchronously: acceptance does not mean completion.");
    println!("Normal use does not require manually starting the daemon.");
    println!("\nOptions for sub:");
    println!("  -q QUEUE        Queue name");
    println!("  -c COST         Resource cost: positive integer (default: 1)");
    println!("  -N NAME         Job name");
    println!("  -W HH:MM:SS     Walltime limit");
    println!("  -hold_jid ID    Wait for the specified job to finish successfully");
    println!("  -a TIME         Start after TIME (UNIX ts, +1h, 18:00, or YYYY-MM-DD HH:MM)");
    println!("  -o OUT          Standard output file");
    println!("  -e ERR          Standard error file");
    println!("  --range N-M     Batch range expansion");
    println!("  --list A,B      Batch list expansion");
    println!("\nImportant:");
    println!("  COST is scheduling accounting, not a CPU, memory, or GPU usage limit.");
    println!("  Configure the command's own parallelism to match its declared cost.");
    println!("\n  Do not automatically resubmit jobs with unknown outcomes or clear");
    println!("  recovery holds. Check surviving processes and external effects first.");
    println!("\n  Stopping the daemon is not a safe way to pause jobs.");
    println!("  On Windows, stopping or restarting it can kill or duplicate work.");
    println!("\nState directory:");
    println!("  FBQUEUE_DIR selects the queue state directory (default: ~/.fbqueue).");
    println!("  Use the same directory when submitting and checking jobs.");
}
