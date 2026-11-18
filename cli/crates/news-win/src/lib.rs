//! Every Win32 call of the Bureau service, behind safe functions (ADR 0008 §2, §8.2). The only crate
//! allowed `unsafe`: each unsafe function carries `#[allow(unsafe_code)]` and each block a `// SAFETY:`
//! comment. Off Windows every function returns `Unsupported`, so the workspace builds and tests on
//! Linux CI.
//!
//! What is here: the Hotline pipe (`D:P(A;;GA;;;<sid>)`, first instance only, remote clients
//! rejected), the facts about a connected client (pid, user, integrity, image path, command line,
//! membership of the root marker job), the client's check of the server's image, and Job Objects (the
//! root marker job with kill-on-close and no breakaway, nested per-run jobs, terminate, emptiness).
//! The Low-integrity spawner Ghost needs lands with Ghost (#18).

#[cfg(not(windows))]
use std::io;

/// Facts about a connected Hotline client, read from the kernel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerFacts {
    pub pid: u32,
    /// The client's user SID, or `None` when its token cannot be opened (another user).
    pub sid: Option<String>,
    /// Mandatory integrity RID: 0x1000 Low, 0x2000 Medium, 0x3000 High, 0x4000 System.
    pub integrity_rid: u32,
    pub image: String,
    pub argv: Vec<String>,
    pub in_marker_job: bool,
}

/// SDDL for the Hotline: a protected DACL with one ACE, full access for one SID.
pub fn sddl_for(sid: &str) -> String {
    format!("D:P(A;;GA;;;{sid})")
}

/// Case-insensitive comparison of two image paths (Windows paths are case-insensitive).
pub fn same_image(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// The service's name in the SCM.
pub const SERVICE_NAME: &str = "NewsBureau";

/// The installed binary and its staging files: one folder under Program Files that only
/// administrators (and SYSTEM) can write, so no build step or agent running as Karthik can change the
/// binary the service runs (ADR 0007 decision 12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallPaths {
    /// `%ProgramFiles%\News`
    pub dir: std::path::PathBuf,
    /// `news.exe`: what the SCM, Board's log-on entry and the Hotline's image check use.
    pub exe: std::path::PathBuf,
    /// `news.exe.new`: a build staged by `news service stage`.
    pub staged: std::path::PathBuf,
    /// `news.exe.new.sha256`: the staged build's SHA-256, written by the same elevated step.
    pub staged_hash: std::path::PathBuf,
}

impl InstallPaths {
    pub fn under(program_files: &std::path::Path) -> Self {
        let dir = program_files.join("News");
        Self {
            exe: dir.join("news.exe"),
            staged: dir.join("news.exe.new"),
            staged_hash: dir.join("news.exe.new.sha256"),
            dir,
        }
    }
}

/// A user account's SID string, `^S-1-5-21(-\d{1,10}){4}$`: a local, Microsoft or domain account.
/// Anything else (a well-known SID, an SDDL fragment) is refused before it reaches an access list.
pub fn is_user_sid(s: &str) -> bool {
    let Some(rest) = s.strip_prefix("S-1-5-21-") else {
        return false;
    };
    let parts: Vec<&str> = rest.split('-').collect();
    parts.len() == 4
        && parts
            .iter()
            .all(|p| (1..=10).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_digit()))
}

/// An account name that can go on the elevated command line and to `LookupAccountNameW`: not empty,
/// at most 256 characters, no `"` and no control characters.
pub fn is_clean_account(s: &str) -> bool {
    !s.is_empty() && s.chars().count() <= 256 && !s.chars().any(|c| c == '"' || c.is_control())
}

/// What `news service install` registers with the SCM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceSpec {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub exe: std::path::PathBuf,
    /// Arguments after the executable path (`service run`).
    pub args: Vec<String>,
    /// `.\user` or `DOMAIN\user`: Karthik's own account, because `claude -p` uses his login.
    pub account: String,
}

/// The SCM's view of the service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScmStatus {
    /// `stopped`, `startpending`, `running`, ...
    pub state: String,
    pub running: bool,
    /// The Win32 or service-specific exit code of the last stop, when it was not clean (1069 is a
    /// log-on failure).
    pub exit_code: Option<u32>,
    pub pid: Option<u32>,
}

/// A readable reason for the exit codes Board's offline banner is likely to meet.
pub fn explain_exit_code(code: u32) -> String {
    match code {
        1069 => "Windows could not log the service on: the account password is wrong or changed. \
                 Run `news service install` again to enter it."
            .to_owned(),
        1326 => "Windows rejected the account name or password. Run `news service install` again."
            .to_owned(),
        1053 => "The service did not respond to the start request in time.".to_owned(),
        5 => "Access denied.".to_owned(),
        other => format!("Windows error {other}."),
    }
}

#[cfg(windows)]
mod admin;
#[cfg(windows)]
mod imp;
#[cfg(windows)]
#[allow(unsafe_code)]
pub mod scm;
#[cfg(windows)]
pub use admin::*;
#[cfg(windows)]
pub use imp::*;

/// Off Windows there is no SCM: every call is `Unsupported`.
#[cfg(not(windows))]
pub mod scm {
    use super::*;

    fn unsupported<T>() -> io::Result<T> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "Windows only"))
    }
    pub fn install(_spec: &ServiceSpec, _password: Option<&[u16]>) -> io::Result<()> {
        unsupported()
    }
    pub fn uninstall(_name: &str) -> io::Result<()> {
        unsupported()
    }
    pub fn start(_name: &str) -> io::Result<()> {
        unsupported()
    }
    pub fn stop(_name: &str) -> io::Result<()> {
        unsupported()
    }
    pub fn status(_name: &str) -> io::Result<Option<ScmStatus>> {
        unsupported()
    }
    pub fn dispatch(
        _name: &str,
        _body: fn(tokio::sync::watch::Receiver<bool>) -> u32,
    ) -> io::Result<()> {
        unsupported()
    }
    pub const NOT_STARTED_BY_SCM: i32 = 1063;
}

#[cfg(not(windows))]
mod stub {
    use super::*;

    fn unsupported<T>() -> io::Result<T> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "Windows only"))
    }

    /// A Job Object (stub).
    #[derive(Debug)]
    pub struct Job;

    impl Job {
        pub fn root() -> io::Result<Job> {
            unsupported()
        }
        pub fn nested(&self) -> io::Result<Job> {
            unsupported()
        }
        pub fn terminate(&self) -> io::Result<()> {
            unsupported()
        }
        pub fn active_processes(&self) -> io::Result<u32> {
            unsupported()
        }
    }

    pub fn current_user_sid() -> io::Result<String> {
        unsupported()
    }

    pub fn cred_write(_target: &str, _secret: &str) -> io::Result<()> {
        unsupported()
    }
    pub fn cred_read(_target: &str) -> Option<String> {
        None
    }
    pub fn cred_exists(_target: &str) -> bool {
        false
    }
    pub fn cred_delete(_target: &str) -> io::Result<()> {
        unsupported()
    }
    pub fn read_password(_prompt: &str) -> io::Result<zeroize::Zeroizing<Vec<u16>>> {
        unsupported()
    }
    pub fn install_paths() -> io::Result<crate::InstallPaths> {
        unsupported()
    }
    pub fn make_admin_only(_path: &std::path::Path, _dir: bool) -> io::Result<()> {
        unsupported()
    }
    pub fn account_sid(_account: &str) -> io::Result<String> {
        unsupported()
    }
    pub fn is_elevated() -> bool {
        false
    }
    pub fn relaunch_elevated(_exe: &std::path::Path, _args: &str) -> io::Result<u32> {
        unsupported()
    }
    pub fn grant_service_logon(_sid: &str) -> io::Result<()> {
        unsupported()
    }
    pub fn set_logon_entry(_command: Option<&str>) -> io::Result<()> {
        unsupported()
    }
    pub fn logon_entry_exists() -> bool {
        false
    }
    pub fn allow_user_to_control(_service: &str, _sid: &str) -> io::Result<()> {
        unsupported()
    }
    pub fn detach_console_if_alone() {}
}
#[cfg(not(windows))]
pub use stub::*;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn the_descriptor_names_one_sid() {
        assert_eq!(
            sddl_for("S-1-5-21-1-2-3-1001"),
            "D:P(A;;GA;;;S-1-5-21-1-2-3-1001)"
        );
        assert!(same_image(r"C:\Bin\news.exe", r"c:\bin\NEWS.EXE"));
    }

    #[test]
    fn only_a_user_account_sid_is_accepted() {
        for good in [
            "S-1-5-21-1-2-3-1001",
            "S-1-5-21-3623811015-3361044348-30300820-1013",
            "S-1-5-21-0-0-0-0",
        ] {
            assert!(is_user_sid(good), "{good}");
        }
        for bad in [
            "",
            "S-1-5-18",
            "S-1-5-32-544",
            "S-1-12-1-1-2-3-4",
            "S-1-5-21-1-2-3",
            "S-1-5-21-1-2-3-4-5",
            "S-1-5-21-1-2-3-12345678901",
            "S-1-5-21-1-2-3-",
            "S-1-5-21-1-2-3-+1",
            "S-1-5-21-1-2-3-1001)(A;;GA;;;WD)",
            "S-1-5-21-1-2-3-1001\n",
            "s-1-5-21-1-2-3-1001",
            " S-1-5-21-1-2-3-1001",
            "S-1-5-21-\u{661}-2-3-4",
        ] {
            assert!(!is_user_sid(bad), "{bad:?}");
        }
    }

    #[test]
    fn an_account_with_a_quote_or_a_control_character_is_refused() {
        assert!(is_clean_account(r".\Admin"));
        assert!(is_clean_account(r"CORP\Karthik P"));
        for bad in [
            "",
            "a\"b",
            "Admin\" --sid \"S-1-5-18",
            "a\nb",
            "a\u{0}",
            "a\tb",
            "a\u{7f}",
        ] {
            assert!(!is_clean_account(bad), "{bad:?}");
        }
        assert!(!is_clean_account(&"a".repeat(257)));
    }

    #[test]
    fn the_installed_files_share_one_folder() {
        let pf = std::path::Path::new(r"C:\Program Files");
        let p = InstallPaths::under(pf);
        assert_eq!(p.dir, pf.join("News"));
        assert_eq!(p.exe, p.dir.join("news.exe"));
        assert_eq!(p.staged, p.dir.join("news.exe.new"));
        assert_eq!(p.staged_hash, p.dir.join("news.exe.new.sha256"));
    }

    #[cfg(not(windows))]
    #[test]
    fn off_windows_everything_is_unsupported() {
        assert_eq!(
            current_user_sid().map_err(|e| e.kind()),
            Err(io::ErrorKind::Unsupported)
        );
        assert!(Job::root().is_err());
    }
}
