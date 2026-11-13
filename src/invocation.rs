/// Normalize the entrypoint before command dispatch; no filesystem side effects.
pub fn program_name(name: &str, windows: bool) -> Result<&str, &'static str> {
    if !windows {
        return Ok(name);
    }
    if name.eq_ignore_ascii_case("fbqueue") {
        Ok("fbqueue")
    } else {
        Err("Windows aliases are not supported. Use fbqueue.exe sub, fbqueue.exe stat, or fbqueue.exe del; do not rename fbqueue.exe.")
    }
}

#[cfg(test)]
mod tests {
    use super::program_name;

    #[test]
    fn windows_rejects_aliases_before_dispatch() {
        for name in ["qsub", "qstat", "qdel", "QSUB", "other"] {
            assert!(program_name(name, true).is_err(), "Windows accepted {}", name);
        }
    }

    #[test]
    fn windows_accepts_canonical_name_case_insensitively() {
        assert_eq!(program_name("fbqueue", true), Ok("fbqueue"));
        assert_eq!(program_name("FBQUEUE", true), Ok("fbqueue"));
    }

    #[test]
    fn unix_preserves_existing_invocation_names() {
        for name in ["fbqueue", "qsub", "qstat", "qdel", "other"] {
            assert_eq!(program_name(name, false), Ok(name));
        }
    }

    #[test]
    fn windows_uses_actual_executable_name_not_forged_argv() {
        assert!(super::entrypoint_name("fbqueue", Some("qsub"), true).is_err());
        assert!(super::entrypoint_name("fbqueue", None, true).is_err());
        assert_eq!(super::entrypoint_name("qsub", Some("FBQUEUE"), true), Ok("fbqueue"));
        assert_eq!(super::entrypoint_name("qsub", Some("fbqueue"), false), Ok("qsub"));
    }
}

pub fn entrypoint_name<'a>(argv_name: &'a str, actual_name: Option<&'a str>, windows: bool) -> Result<&'a str, &'static str> {
    let name = if windows {
        actual_name.ok_or("Cannot identify the Windows executable. Use fbqueue.exe with an explicit subcommand.")?
    } else {
        argv_name
    };
    program_name(name, windows)
}
