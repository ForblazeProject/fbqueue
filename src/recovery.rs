//! Linux-only, fail-closed daemon ownership and recovery. No job replay/adoption.
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{self, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use crate::{job, utils};

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

pub fn atomic_write(path: &Path, data: &[u8]) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| invalid("missing parent directory"))?;
    let temp = parent.join(format!(".{}.{}.tmp", path.file_name().unwrap().to_string_lossy(), process::id()));
    let mut file = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&temp)?;
    let result = (|| {
        file.write_all(data)?;
        file.sync_all()?;
        fs::rename(&temp, path)?;
        File::open(parent)?.sync_all()
    })();
    if result.is_err() { let _ = fs::remove_file(temp); }
    result
}

fn valid_boot_id(value: &str) -> bool {
    value.len() == 36 && value.bytes().enumerate().all(|(i, c)| {
        if matches!(i, 8 | 13 | 18 | 23) { c == b'-' } else { c.is_ascii_hexdigit() }
    })
}

pub fn boot_id() -> io::Result<String> {
    let value = fs::read_to_string("/proc/sys/kernel/random/boot_id")?.trim().to_string();
    if !valid_boot_id(&value) { return Err(invalid("invalid Linux boot identity")); }
    Ok(value)
}

fn start_time(pid: u32) -> io::Result<u64> {
    let stat = fs::read_to_string(format!("/proc/{}/stat", pid))?;
    let fields: Vec<_> = stat.rsplit_once(')').ok_or_else(|| invalid("invalid process stat"))?.1.split_whitespace().collect();
    if fields.first() == Some(&"Z") { return Err(invalid("process has exited")); }
    fields.get(19).ok_or_else(|| invalid("missing process start time"))?
        .parse().map_err(|_| invalid("invalid process start time"))
}

#[derive(Clone, Debug, PartialEq)]
struct Identity { pid: u32, boot: String, start: u64 }

impl Identity {
    fn current() -> io::Result<Self> {
        let pid = process::id();
        Ok(Self { pid, boot: boot_id()?, start: start_time(pid)? })
    }
    fn encode(&self) -> String {
        format!("version: 1\npid: {}\nboot_id: {}\nstart_time: {}\n", self.pid, self.boot, self.start)
    }
    fn read(root: &Path) -> io::Result<Self> {
        let text = fs::read_to_string(root.join("run/daemon.identity"))?;
        let value = |key: &str| text.lines().find_map(|line| line.strip_prefix(key));
        if value("version: ") != Some("1") { return Err(invalid("unsupported daemon identity")); }
        let pid: u32 = value("pid: ").ok_or_else(|| invalid("missing daemon PID"))?.parse().map_err(|_| invalid("invalid daemon PID"))?;
        if pid == 0 || pid > i32::MAX as u32 { return Err(invalid("invalid daemon PID")); }
        let boot = value("boot_id: ").ok_or_else(|| invalid("missing boot identity"))?.to_string();
        if !valid_boot_id(&boot) { return Err(invalid("invalid boot identity")); }
        let start = value("start_time: ").ok_or_else(|| invalid("missing process start time"))?.parse().map_err(|_| invalid("invalid process start time"))?;
        Ok(Self { pid, boot, start })
    }
    fn verified_pidfd(&self) -> io::Result<OwnedFd> {
        // Pin first: a later PID reuse must not redirect the signal.
        let raw = unsafe { libc::syscall(libc::SYS_pidfd_open, self.pid, 0) };
        if raw < 0 { return Err(io::Error::last_os_error()); }
        let fd = unsafe { OwnedFd::from_raw_fd(raw as RawFd) };
        if self.boot != boot_id()? || self.start != start_time(self.pid)? {
            return Err(invalid("daemon identity mismatch; refusing to signal"));
        }
        let mut pollfd = libc::pollfd { fd: fd.as_raw_fd(), events: libc::POLLIN, revents: 0 };
        if unsafe { libc::poll(&mut pollfd, 1, 0) } != 0 {
            return Err(invalid("daemon identity is no longer live"));
        }
        Ok(fd)
    }
}

fn open_lock(root: &Path, create: bool) -> io::Result<File> {
    OpenOptions::new().read(true).write(true).create(create).mode(0o600)
        .custom_flags(libc::O_CLOEXEC).open(root.join("run/daemon.lock"))
}

pub fn lock_held(root: &Path) -> io::Result<bool> {
    let file = match open_lock(root, false) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
    };
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
        return Ok(false); // Closing this temporary fd releases its probe lock.
    }
    let error = io::Error::last_os_error();
    if error.kind() == io::ErrorKind::WouldBlock { Ok(true) } else { Err(error) }
}

pub struct DaemonGuard { lock: File, root: PathBuf, identity: Identity }

impl DaemonGuard {
    pub fn acquire(root: &Path) -> io::Result<Option<Self>> {
        let lock = open_lock(root, true)?;
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            let error = io::Error::last_os_error();
            return if error.kind() == io::ErrorKind::WouldBlock { Ok(None) } else { Err(error) };
        }
        let identity = Identity::current()?;
        // Refuse startup before any dispatch if this kernel/sandbox cannot pin
        // the daemon identity; never silently fall back to bare-PID control.
        let _self_pidfd = identity.verified_pidfd()?;
        // Never unlink daemon.lock: all contenders must lock the same inode.
        let _ = fs::remove_file(root.join("run/daemon.retiring"));
        atomic_write(&root.join("run/daemon.identity"), identity.encode().as_bytes())?;
        atomic_write(&root.join("run/daemon.pid"), identity.pid.to_string().as_bytes())?;
        Ok(Some(Self { lock, root: root.to_path_buf(), identity }))
    }
    pub fn fd(&self) -> RawFd { self.lock.as_raw_fd() }
    pub fn boot(&self) -> &str { &self.identity.boot }
    pub fn retire(&self) -> io::Result<()> {
        atomic_write(&self.root.join("run/daemon.retiring"), b"retiring\n")
    }
    pub fn stay(&self) -> io::Result<()> {
        fs::remove_file(self.root.join("run/daemon.retiring"))
    }
}

impl Drop for DaemonGuard {
    fn drop(&mut self) {
        // The lifetime lock remains held throughout metadata cleanup.
        if Identity::read(&self.root).ok().as_ref() == Some(&self.identity) {
            let _ = fs::remove_file(self.root.join("run/daemon.pid"));
            let _ = fs::remove_file(self.root.join("run/daemon.identity"));
            let _ = fs::remove_file(self.root.join("run/daemon.retiring"));
        }
    }
}

fn ready(root: &Path) -> bool {
    Identity::read(root).and_then(|identity| {
        let _fd = identity.verified_pidfd()?;
        let displayed = fs::read_to_string(root.join("run/daemon.pid"))?;
        if displayed.trim() != identity.pid.to_string() { return Err(invalid("daemon metadata not ready")); }
        Ok(())
    }).is_ok()
}

pub fn ensure_daemon() -> io::Result<()> {
    let root = utils::get_fbq_dir();
    // Retiring is published before the final work check. A submitter that sees
    // retiring waits for release and retries instead of losing its wake-up.
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        if lock_held(&root)? {
            if !root.join("run/daemon.retiring").exists() && ready(&root) { return Ok(()); }
        } else {
            use std::os::unix::process::CommandExt;
            let mut cmd = Command::new(std::env::current_exe()?);
            cmd.arg("daemon").arg("start").arg(&root)
                .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
            unsafe { cmd.pre_exec(|| {
                if libc::setsid() < 0 { return Err(io::Error::last_os_error()); }
                Ok(())
            }); }
            let mut child = cmd.spawn()?;
            loop {
                if lock_held(&root)? && ready(&root) { return Ok(()); }
                if let Some(status) = child.try_wait()? {
                    if !status.success() { return Err(invalid("daemon failed to start")); }
                    break; // Another contender may own (or have released) it.
                }
                if Instant::now() >= deadline { return Err(invalid("daemon startup timed out")); }
                thread::sleep(Duration::from_millis(10));
            }
        }
        if Instant::now() >= deadline { return Err(invalid("daemon retirement timed out")); }
        thread::sleep(Duration::from_millis(10));
    }
}

pub fn status(root: &Path) -> io::Result<String> {
    if !lock_held(root)? { return Ok("Daemon not running.".to_string()); }
    let identity = Identity::read(root)?;
    let _fd = identity.verified_pidfd()?;
    Ok(format!("Daemon running PID: {}", identity.pid))
}

pub fn stop(root: &Path) -> io::Result<()> {
    if !lock_held(root)? { return Ok(()); }
    let identity = Identity::read(root)?;
    let fd = identity.verified_pidfd()?;
    if unsafe { libc::syscall(libc::SYS_pidfd_send_signal, fd.as_raw_fd(), libc::SIGTERM, std::ptr::null::<libc::siginfo_t>(), 0) } < 0 {
        return Err(io::Error::last_os_error());
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    while lock_held(root)? {
        // Do not wait for a successor or delete any daemon's metadata.
        if Identity::read(root).ok().as_ref() != Some(&identity) { return Ok(()); }
        if Instant::now() >= deadline { return Err(invalid("daemon did not stop before deadline")); }
        thread::sleep(Duration::from_millis(10));
    }
    // SIGTERM does not execute Rust destructors. Clean stale display metadata
    // only after acquiring the released lock, never under a successor daemon.
    let file = open_lock(root, false)?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0
        && Identity::read(root).ok().as_ref() == Some(&identity) {
        fs::remove_file(root.join("run/daemon.pid"))?;
        fs::remove_file(root.join("run/daemon.identity"))?;
        let _ = fs::remove_file(root.join("run/daemon.retiring"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latched_transition_error_cannot_be_cleared_by_an_empty_scan() {
        let root = std::env::temp_dir().join(format!("fbqueue-latch-test-{}-{}", process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        for dir in ["run", "queue/new", "queue/running", "queue/cancel"] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        latch_error(&root, "directory sync failed after rename");
        let persisted = fs::read_to_string(root.join("run/recovery.error")).unwrap();
        let held = scan(&root, &HashSet::new(), &boot_id().unwrap());
        let refused_clear = clear_hold(&root).is_err();
        fs::remove_file(root.join("run/recovery.error")).unwrap();
        fs::remove_file(root.join("run/recovery.hold")).unwrap();
        let sync_failed = sync_transition(&root, &root.join("missing-directory"), &root).is_err();
        let fresh_error = root.join("run/recovery.error").exists();
        let held_again = scan(&root, &HashSet::new(), &boot_id().unwrap());
        fs::remove_dir_all(root).unwrap();
        assert_eq!(persisted, "directory sync failed after rename");
        assert!(sync_failed && fresh_error && held_again, "post-transition I/O failures must produce a persistent hold");
        assert!(held, "an empty running queue is not proof that a failed durable transition was repaired");
        assert!(refused_clear, "a latched transition error must not be cleared automatically");
    }

    #[test]
    fn cancellation_records_prevent_idle_retirement() {
        let root = std::env::temp_dir().join(format!("fbqueue-retire-test-{}-{}", process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        for dir in ["queue/new", "queue/running", "queue/cancel"] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        fs::write(root.join("queue/cancel/1.job"), "id: 1\nname: pending\n").unwrap();
        let result = has_work(&root);
        fs::remove_dir_all(root).unwrap();
        assert!(result.unwrap(), "cancellation history must be finalized before retirement");
    }
}

pub fn has_work(root: &Path) -> io::Result<bool> {
    for dir in ["queue/new", "queue/running", "queue/cancel"] {
        if fs::read_dir(root.join(dir))?.next().transpose()?.is_some() { return Ok(true); }
    }
    Ok(false)
}

pub fn scan(root: &Path, tracked: &HashSet<&str>, boot: &str) -> bool {
    // Absence from running cannot prove a failed durable transition repaired.
    match fs::symlink_metadata(root.join("run/recovery.error")) {
        Ok(_) => {
            let reason = fs::read_to_string(root.join("run/recovery.error"))
                .unwrap_or_else(|_| "latched recovery error is unreadable; intervention required".to_string());
            hold(root, &reason);
            return true;
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {},
        Err(error) => { hold(root, &format!("cannot verify recovery error state: {}", error)); return true; }
    }
    let (held, reason) = match recover_untracked(root, tracked, boot) {
        Ok(false) => (false, String::new()),
        Ok(true) => (true, "UNKNOWN execution; automatic replay disabled".to_string()),
        Err(error) => (true, format!("recovery I/O error; execution is unverified: {}", error)),
    };
    if held {
        hold(root, &reason);
    }
    held
}

pub fn clear_hold(root: &Path) -> io::Result<()> {
    match fs::symlink_metadata(root.join("run/recovery.error")) {
        Ok(_) => return Err(invalid("latched transition error requires intervention")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {},
        Err(error) => return Err(error),
    }
    match fs::remove_file(root.join("run/recovery.hold")) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

pub fn hold(root: &Path, reason: &str) {
    let flag = root.join("run/recovery.hold");
    if fs::read_to_string(&flag).ok().as_deref() != Some(reason) {
        if let Err(error) = atomic_write(&flag, reason.as_bytes()) {
            eprintln!("Recovery blocked dispatch; could not persist warning: {}", error);
        }
    }
}

pub fn latch_error(root: &Path, reason: &str) {
    if let Err(error) = atomic_write(&root.join("run/recovery.error"), reason.as_bytes()) {
        eprintln!("Cannot persist latched transition error: {}", error);
    }
    hold(root, reason);
}

pub fn sync_transition(root: &Path, source_dir: &Path, destination_dir: &Path) -> io::Result<()> {
    let result = (|| -> io::Result<()> {
        File::open(source_dir)?.sync_all()?;
        File::open(destination_dir)?.sync_all()
    })();
    if let Err(error) = result {
        latch_error(root, &format!("terminal directory sync failed after rename; intervention required: {}", error));
        return Err(error);
    }
    Ok(())
}

pub fn history_conflict(root: &Path, name: &std::ffi::OsStr) -> io::Result<bool> {
    for dir in ["queue/done", "queue/failed"] {
        match fs::symlink_metadata(root.join(dir).join(name)) {
            Ok(_) => return Ok(true),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {},
            Err(error) => return Err(error),
        }
    }
    Ok(false)
}

fn recover_untracked(root: &Path, tracked: &HashSet<&str>, boot: &str) -> io::Result<bool> {
    let mut held = false;
    for entry in fs::read_dir(root.join("queue/running"))? {
        let entry = entry?;
        if entry.path().extension().and_then(|s| s.to_str()) != Some("job") { continue; }
        let id = entry.path().file_stem().unwrap().to_string_lossy().into_owned();
        if tracked.contains(id.as_str()) { continue; }
        let path = entry.path();
        let mut text = fs::read_to_string(&path)?;
        let stored_boot = text.lines().rev().find_map(|line| line.strip_prefix("execution_boot_id: "));
        let after_reboot = stored_boot.map(|old| valid_boot_id(old) && old != boot).unwrap_or(false);
        let parsed = job::parse_job_file(&path)?;
        if after_reboot {
            if parsed.status.as_deref() != Some("INTERRUPTED") {
                text.push_str(&format!("\nstatus: INTERRUPTED\nexit_reason: recovery_after_reboot\nend_at: {}\n", utils::get_now()));
                atomic_write(&path, text.as_bytes())?;
            }
            let destination = root.join("queue/failed").join(entry.file_name());
            if history_conflict(root, &entry.file_name())? {
                return Err(invalid("interrupted job conflicts with existing history; retaining both records"));
            }
            fs::rename(&path, destination)?;
            sync_transition(root, &root.join("queue/running"), &root.join("queue/failed"))?;
            let _ = fs::remove_file(root.join("run").join(format!("nodefile.{}", id)));
        } else {
            held = true;
            if parsed.status.as_deref() != Some("UNKNOWN") {
                text.push_str(&format!("\nstatus: UNKNOWN\nrecovery_reason: execution outcome unknown; automatic replay disabled\nrecovery_observed_at: {}\n", utils::get_now()));
                atomic_write(&path, text.as_bytes())?;
            }
        }
    }
    Ok(held)
}
