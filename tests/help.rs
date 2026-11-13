use std::process::Command;

#[test]
fn top_level_help_guides_users_and_agents_without_creating_state() {
    let root = std::env::temp_dir().join(format!("fbqueue-help-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let state = root.join("state");
    let mut baseline = None;
    for args in [vec![], vec!["help"], vec!["--help"], vec!["-h"]] {
        let output = Command::new(env!("CARGO_BIN_EXE_fbqueue"))
            .args(&args)
            .env("FBQUEUE_DIR", &state)
            .env("HOME", &root)
            .env("USERPROFILE", &root)
            .output()
            .unwrap();
        assert!(output.status.success(), "help failed: {args:?}");
        assert!(output.stderr.is_empty(), "help wrote to stderr: {args:?}");
        assert!(!state.exists(), "help initialized queue state: {args:?}");
        let text = String::from_utf8(output.stdout).unwrap();
        for expected in [
            "Local background job scheduler",
            "Workflow for users and AI agents:",
            "save the returned job ID",
            "acceptance does not mean completion",
            "Inspect the final status and output files",
            "not a CPU, memory, or GPU usage limit",
            "Do not automatically resubmit jobs with unknown outcomes",
            "stopping or restarting it can kill or duplicate work",
            "FBQUEUE_DIR",
            "--list A,B",
        ] {
            assert!(text.contains(expected), "missing help guidance: {expected}");
        }
        let frontend = if cfg!(windows) { "fbqueue.exe" } else { "fbqueue" };
        assert!(text.contains(&format!("{frontend} stat <job_id>")));
        assert!(text.contains(&format!("{frontend} sub -N analysis")));
        if let Some(previous) = &baseline {
            assert_eq!(&text, previous, "help variants differ");
        } else {
            baseline = Some(text);
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}
