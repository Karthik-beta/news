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

#[cfg(windows)]
mod imp;
#[cfg(windows)]
pub use imp::*;

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
