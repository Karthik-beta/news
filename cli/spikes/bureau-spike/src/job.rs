//! The spawn marker: start a child suspended, put it in the AI Job Object, resume it.

use std::io;
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use crate::win;

pub const CREATE_SUSPENDED: u32 = 0x0000_0004;
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;
pub const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;

/// Spawn `cmd` suspended, assign it to `job`, resume it. Returns the child and the time from
/// spawn to resume. If the assignment fails the child is killed before it ever ran.
pub fn spawn_in_job(job: &win::Handle, mut cmd: Command) -> io::Result<(Child, Duration)> {
    cmd.creation_flags(CREATE_SUSPENDED | CREATE_NO_WINDOW);
    let t = Instant::now();
    let mut child = cmd.spawn()?;
    let t_spawn = t.elapsed();
    if let Err(e) = win::assign_to_job(job.0, child.as_raw_handle() as _) {
        let _ = child.kill();
        return Err(e);
    }
    let t_assign = t.elapsed();
    if std::env::var_os("SPIKE_RESUME_TOOLHELP").is_some() {
        win::resume_process(child.id())?;
    } else {
        win::resume_process_nt(child.as_raw_handle() as _)?;
    }
    let total = t.elapsed();
    eprintln!(
        "[spawn_in_job] spawn={}us assign={}us resume={}us",
        t_spawn.as_micros(),
        (t_assign - t_spawn).as_micros(),
        (total - t_assign).as_micros()
    );
    Ok((child, total))
}

/// The allow-listed child environment (Bureau Design §9.3), taken from a fresh log-on
/// environment of the current user, so nothing from this process's own environment leaks in.
pub fn allow_listed_env() -> io::Result<Vec<(String, String)>> {
    const KEEP: &[&str] = &[
        "path", "pathext", "systemroot", "systemdrive", "windir", "comspec", "userprofile",
        "username", "userdomain", "homedrive", "homepath", "appdata", "localappdata", "temp",
        "tmp", "programdata", "programfiles", "programfiles(x86)", "programw6432",
        "commonprogramfiles", "computername", "number_of_processors", "processor_architecture",
        "os", "public", "allusersprofile",
    ];
    Ok(win::fresh_user_environment()?
        .into_iter()
        .filter(|(k, _)| KEEP.contains(&k.to_ascii_lowercase().as_str()))
        .collect())
}

/// Find claude.exe on a PATH value: either a direct claude.exe, or the npm shim's target.
pub fn find_claude(path: &str) -> Option<std::path::PathBuf> {
    for dir in path.split(';').filter(|d| !d.is_empty()) {
        let d = std::path::Path::new(dir);
        for cand in [
            d.join("claude.exe"),
            d.join(r"node_modules\@anthropic-ai\claude-code\bin\claude.exe"),
        ] {
            if cand.is_file() {
                return Some(cand);
            }
        }
    }
    None
}
