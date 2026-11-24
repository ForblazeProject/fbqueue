use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        loop {
            let root = std::env::temp_dir().join(format!(
                "fbqueue-stat-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::create_dir(&root) {
                Ok(()) => return Self(root),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("{e}"),
            }
        }
    }

    fn stat(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_fbqueue"))
            .arg("stat")
            .args(args)
            .current_dir(&self.0)
            .env("HOME", &self.0)
            .env("USERPROFILE", &self.0)
            .env("FBQUEUE_DIR", self.0.join("state"))
            .output()
            .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn missing_id_is_an_error() {
    let fixture = Fixture::new();
    for args in [&["42.master"][..], &["42", "--json"][..]] {
        let output = fixture.stat(args);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("42"));
    }
}

#[test]
fn invalid_arguments_fail_without_initializing_state() {
    let fixture = Fixture::new();
    for args in [
        &["--bogus"][..],
        &["42", "--bogus"],
        &["--style", "--bogus"],
        &["--style", "invalid"],
        &["-u", "--bogus"],
        &["--json"],
        &["42abc"],
        &["42", "43"],
    ] {
        let output = fixture.stat(args);
        assert!(!output.status.success(), "unexpected success: {args:?}");
        assert!(!output.stderr.is_empty(), "missing diagnostic: {args:?}");
        assert!(!fixture.0.join("state").exists());
    }
}

#[test]
fn help_succeeds_without_initializing_state() {
    let fixture = Fixture::new();
    for flag in ["--help", "-h"] {
        let output = fixture.stat(&[flag]);
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains("Usage:"));
        assert!(text.contains("--json"));
        assert!(!fixture.0.join("state").exists());
    }
}

#[test]
fn no_args_keeps_capacity_listing() {
    let fixture = Fixture::new();
    let output = fixture.stat(&[]);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("FBQueue Status (Global Capacity:"));
}

#[test]
fn user_filter_can_hide_an_existing_id_in_text_output() {
    let fixture = Fixture::new();
    let dir = fixture.0.join("state/queue/done");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("42.job"), "id: 42\nname: sample\nuser: owner\n").unwrap();
    let output = fixture.stat(&["--style", "pbs", "-u", "other-user", "42"]);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("Job id"));
    assert!(!text.contains("sample"));
}

#[test]
fn json_reports_terminal_state_and_paths() {
    let fixture = Fixture::new();
    for (id, state, metadata, code) in [("42", "FAILED", "exit_code: 7\n", "7"), ("43", "CANCELLED", "", "null")] {
        let dir = fixture.0.join("state/queue/failed");
        std::fs::create_dir_all(&dir).unwrap();
        let cwd = fixture.0.display();
        std::fs::write(dir.join(format!("{id}.job")), format!(
            "id: {id}\nname: sample\ncwd: {cwd}\nstdout: out\t雪.log\nstatus: {state}\n{metadata}"
        )).unwrap();
        let output = fixture.stat(&[&format!("{id}.master"), "--json"]);
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        assert_eq!(String::from_utf8(output.stdout).unwrap(), format!(
            "{{\"id\":\"{id}\",\"state\":\"{state}\",\"exit_code\":{code},\"stdout\":{:?},\"stderr\":{:?}}}\n",
            fixture.0.join("out\t雪.log").to_str().unwrap(),
            fixture.0.join(format!("sample.e{id}")).to_str().unwrap(),
        ));
    }
}
