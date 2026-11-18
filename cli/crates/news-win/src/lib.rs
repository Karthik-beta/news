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
    pub fn install(_spec: &ServiceSpec, _password: Option<&str>) -> io::Result<()> {
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
    pub fn read_password(_prompt: &str) -> io::Result<String> {
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
