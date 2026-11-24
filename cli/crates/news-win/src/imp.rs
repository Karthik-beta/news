//! The Windows implementation. Calls as proven in the service spike (`cli/spikes/bureau-spike`, branch
//! `spike/bureau-service`), behind safe functions.

use crate::PeerFacts;
use std::ffi::c_void;
use std::io;
use std::os::windows::io::AsRawHandle;
use std::ptr::{null, null_mut};
use tokio::net::windows::named_pipe::{
    ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions,
};
use windows_sys::Win32::Foundation::{
    CloseHandle, HANDLE, INVALID_HANDLE_VALUE, LocalFree, UNICODE_STRING,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{
    GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, PSECURITY_DESCRIPTOR, PSID,
    SECURITY_ATTRIBUTES, TOKEN_MANDATORY_LABEL, TOKEN_QUERY, TOKEN_USER, TokenIntegrityLevel,
    TokenUser,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, IsProcessInJob, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectBasicAccountingInformation, JobObjectExtendedLimitInformation,
    QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
};
use windows_sys::Win32::System::Pipes::{GetNamedPipeClientProcessId, GetNamedPipeServerProcessId};
use windows_sys::Win32::System::Threading::{
    CREATE_NO_WINDOW, CREATE_SUSPENDED, GetCurrentProcess, OpenProcess, OpenProcessToken,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_QUOTA, PROCESS_SUSPEND_RESUME,
    PROCESS_TERMINATE, QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::Shell::CommandLineToArgvW;

/// Creation flags for a process the spawn marker adopts: suspended, with no console window.
pub const SPAWN_SUSPENDED: u32 = CREATE_SUSPENDED | CREATE_NO_WINDOW;

#[allow(unsafe_code)]
#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtResumeProcess(process: HANDLE) -> i32;
}

fn last() -> io::Error {
    io::Error::last_os_error()
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// A NUL-terminated UTF-16 string.
#[allow(unsafe_code)]
pub(crate) unsafe fn from_wide_ptr(p: *const u16) -> String {
    let mut n = 0;
    // SAFETY: the caller passes a valid NUL-terminated UTF-16 string; we stop at the NUL.
    while unsafe { *p.add(n) } != 0 {
        n += 1;
    }
    // SAFETY: `n` elements were just read from `p`.
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(p, n) })
}

/// An owned kernel handle, closed on drop.
#[derive(Debug)]
pub struct Handle(HANDLE);

impl Drop for Handle {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
            // SAFETY: we own the handle and close it once.
            unsafe { CloseHandle(self.0) };
        }
    }
}

// SAFETY: kernel handles may be used from any thread.
#[allow(unsafe_code)]
unsafe impl Send for Handle {}
// SAFETY: as above; the handle is only passed to thread-safe kernel calls.
#[allow(unsafe_code)]
unsafe impl Sync for Handle {}

/// The SID string of a token's user.
#[allow(unsafe_code)]
fn token_user_sid(token: HANDLE) -> io::Result<String> {
    let mut len = 0u32;
    // SAFETY: a size query with a null buffer; `len` receives the size.
    unsafe { GetTokenInformation(token, TokenUser, null_mut(), 0, &mut len) };
    let mut buf = vec![0u8; len as usize];
    // SAFETY: `buf` has `len` bytes.
    if unsafe { GetTokenInformation(token, TokenUser, buf.as_mut_ptr().cast(), len, &mut len) } == 0
    {
        return Err(last());
    }
    // SAFETY: the buffer now holds a TOKEN_USER; read unaligned in case the Vec<u8> is not aligned.
    let tu: TOKEN_USER = unsafe { std::ptr::read_unaligned(buf.as_ptr().cast()) };
    sid_string(tu.User.Sid)
}

#[allow(unsafe_code)]
pub(crate) fn sid_string(sid: PSID) -> io::Result<String> {
    let mut s: *mut u16 = null_mut();
    // SAFETY: `sid` points into a live token buffer; `s` receives a LocalAlloc'd string we free.
    if unsafe { ConvertSidToStringSidW(sid, &mut s) } == 0 {
        return Err(last());
    }
    // SAFETY: `s` is a valid NUL-terminated string from the call above.
    let out = unsafe { from_wide_ptr(s) };
    // SAFETY: freeing the LocalAlloc'd string once.
    unsafe { LocalFree(s.cast()) };
    Ok(out)
}

/// The integrity RID of a token (0x2000 is Medium).
#[allow(unsafe_code)]
fn token_integrity(token: HANDLE) -> io::Result<u32> {
    let mut len = 0u32;
    // SAFETY: size query.
    unsafe { GetTokenInformation(token, TokenIntegrityLevel, null_mut(), 0, &mut len) };
    let mut buf = vec![0u8; len as usize];
    // SAFETY: `buf` has `len` bytes.
    if unsafe {
        GetTokenInformation(
            token,
            TokenIntegrityLevel,
            buf.as_mut_ptr().cast(),
            len,
            &mut len,
        )
    } == 0
    {
        return Err(last());
    }
    // SAFETY: the buffer holds a TOKEN_MANDATORY_LABEL whose SID points into the same buffer.
    let label: TOKEN_MANDATORY_LABEL = unsafe { std::ptr::read_unaligned(buf.as_ptr().cast()) };
    let sid = label.Label.Sid;
    // SAFETY: `sid` is valid while `buf` lives; the count and sub-authority pointers point into it.
    let rid = unsafe {
        let n = *GetSidSubAuthorityCount(sid);
        *GetSidSubAuthority(sid, u32::from(n.saturating_sub(1)))
    };
    Ok(rid)
}

/// The user SID of this process.
#[allow(unsafe_code)]
pub fn current_user_sid() -> io::Result<String> {
    let mut tok = null_mut();
    // SAFETY: the pseudo-handle of the current process; `tok` receives a handle we own.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut tok) } == 0 {
        return Err(last());
    }
    let tok = Handle(tok);
    token_user_sid(tok.0)
}

/// Create a Hotline pipe instance with the security descriptor `sddl`; the first instance with
/// `first` (a second server on the same name then fails with error 5), remote clients rejected.
#[allow(unsafe_code)]
pub fn create_pipe(name: &str, sddl: &str, first: bool) -> io::Result<NamedPipeServer> {
    let w = wide(sddl);
    let mut sd: PSECURITY_DESCRIPTOR = null_mut();
    // SAFETY: `w` is NUL-terminated; `sd` receives a LocalAlloc'd descriptor freed below.
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            w.as_ptr(),
            SDDL_REVISION_1,
            &mut sd,
            null_mut(),
        )
    } == 0
    {
        return Err(last());
    }
    let mut sa = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: sd,
        bInheritHandle: 0,
    };
    let mut opts = ServerOptions::new();
    opts.first_pipe_instance(first)
        .reject_remote_clients(true)
        .max_instances(16);
    // SAFETY: `sa` and the descriptor it points to outlive the call; the kernel copies them.
    let r = unsafe {
        opts.create_with_security_attributes_raw(
            name,
            (&mut sa as *mut SECURITY_ATTRIBUTES).cast::<c_void>(),
        )
    };
    // SAFETY: freeing the descriptor once, after the kernel has copied it.
    unsafe { LocalFree(sd) };
    r
}

#[allow(unsafe_code)]
fn image_path(process: HANDLE) -> io::Result<String> {
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    // SAFETY: `buf` holds `len` u16s.
    if unsafe { QueryFullProcessImageNameW(process, 0, buf.as_mut_ptr(), &mut len) } == 0 {
        return Err(last());
    }
    Ok(String::from_utf16_lossy(&buf[..len as usize]))
}

/// The process's command line (ProcessCommandLineInformation = 60). The process could rewrite it, so
/// argv is a hint, not a proof.
#[allow(unsafe_code)]
fn argv(process: HANDLE) -> io::Result<Vec<String>> {
    use windows_sys::Wdk::System::Threading::NtQueryInformationProcess;
    let mut len = 0u32;
    // SAFETY: size query with a null buffer.
    unsafe { NtQueryInformationProcess(process, 60, null_mut(), 0, &mut len) };
    if len == 0 {
        return Err(io::Error::other("no command line length"));
    }
    let mut buf = vec![0u8; len as usize];
    // SAFETY: `buf` has `len` bytes.
    let st =
        unsafe { NtQueryInformationProcess(process, 60, buf.as_mut_ptr().cast(), len, &mut len) };
    if st < 0 {
        return Err(io::Error::other(format!("NTSTATUS {st:#x}")));
    }
    // SAFETY: the buffer starts with a UNICODE_STRING whose Buffer points into the same buffer.
    let line = unsafe {
        let us: UNICODE_STRING = std::ptr::read_unaligned(buf.as_ptr().cast());
        String::from_utf16_lossy(std::slice::from_raw_parts(
            us.Buffer,
            (us.Length / 2) as usize,
        ))
    };
    let w = wide(&line);
    let mut n = 0;
    // SAFETY: `w` is NUL-terminated; the returned array is LocalAlloc'd and freed below.
    let p = unsafe { CommandLineToArgvW(w.as_ptr(), &mut n) };
    if p.is_null() {
        return Ok(Vec::new());
    }
    // SAFETY: `p` holds `n` valid NUL-terminated strings.
    let out = (0..n as usize)
        .map(|i| unsafe { from_wide_ptr(*p.add(i)) })
        .collect();
    // SAFETY: freeing the array once.
    unsafe { LocalFree(p.cast()) };
    Ok(out)
}

#[allow(unsafe_code)]
fn open_process(pid: u32, access: u32) -> io::Result<Handle> {
    // SAFETY: plain OpenProcess; a null result is an error.
    let h = unsafe { OpenProcess(access, 0, pid) };
    if h.is_null() {
        return Err(last());
    }
    Ok(Handle(h))
}

/// The facts about a connected client. The returned handle keeps the client's process open for the
/// life of the connection, so its pid cannot be reused under it (ADR 0008 §8.2).
#[allow(unsafe_code)]
pub fn peer_facts(pipe: &NamedPipeServer, marker: Option<&Job>) -> io::Result<(PeerFacts, Handle)> {
    let mut pid = 0u32;
    // SAFETY: a valid connected pipe handle.
    if unsafe { GetNamedPipeClientProcessId(pipe.as_raw_handle() as HANDLE, &mut pid) } == 0 {
        return Err(last());
    }
    let process = open_process(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
    let mut tok = null_mut();
    // SAFETY: `tok` receives a token handle we own; failure (another user's process) is not an error.
    let (sid, integrity_rid) = if unsafe { OpenProcessToken(process.0, TOKEN_QUERY, &mut tok) } != 0
    {
        let tok = Handle(tok);
        (
            token_user_sid(tok.0).ok(),
            token_integrity(tok.0).unwrap_or(0),
        )
    } else {
        (None, 0)
    };
    let in_marker_job = match marker {
        Some(j) => {
            let mut r = 0;
            // SAFETY: both handles are valid.
            if unsafe { IsProcessInJob(process.0, j.0.0, &mut r) } == 0 {
                return Err(last());
            }
            r != 0
        }
        None => false,
    };
    let facts = PeerFacts {
        pid,
        sid,
        integrity_rid,
        image: image_path(process.0).unwrap_or_default(),
        argv: argv(process.0).unwrap_or_default(),
        in_marker_job,
    };
    Ok((facts, process))
}

/// Open the Hotline as a client (identification-level impersonation only).
pub fn connect_client(name: &str) -> io::Result<NamedPipeClient> {
    ClientOptions::new().open(name)
}

/// The image path of the server at the other end of a client pipe: the client sends nothing unless it
/// is the service's own `news.exe` (ADR 0008 §8.2).
#[allow(unsafe_code)]
pub fn server_image(client: &NamedPipeClient) -> io::Result<String> {
    let mut pid = 0u32;
    // SAFETY: a valid connected pipe handle.
    if unsafe { GetNamedPipeServerProcessId(client.as_raw_handle() as HANDLE, &mut pid) } == 0 {
        return Err(last());
    }
    let p = open_process(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
    image_path(p.0)
}

/// A Job Object.
#[derive(Debug)]
pub struct Job(Handle);

impl Job {
    /// The root marker job: kill-on-close and no breakaway flags, so nothing started inside it can
    /// leave it, and every process in it dies with the service.
    #[allow(unsafe_code)]
    pub fn root() -> io::Result<Job> {
        // SAFETY: an unnamed job with default security.
        let h = unsafe { CreateJobObjectW(null(), null()) };
        if h.is_null() {
            return Err(last());
        }
        let job = Job(Handle(h));
        // SAFETY: a zeroed limit struct is valid; we set one flag.
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: `info` is the struct the class names, with its size.
        let ok = unsafe {
            SetInformationJobObject(
                job.0.0,
                JobObjectExtendedLimitInformation,
                (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if ok == 0 {
            return Err(last());
        }
        Ok(job)
    }

    /// A run's job. A process is put in the root first and then here, which nests this job in the
    /// root (Windows 8+), so one `IsProcessInJob(p, root)` covers every run.
    pub fn nested(&self) -> io::Result<Job> {
        Job::root()
    }

    /// Put process `pid` in this job (and, for a run job, call it on the root first).
    #[allow(unsafe_code)]
    pub fn assign_pid(&self, pid: u32) -> io::Result<()> {
        let p = open_process(
            pid,
            PROCESS_SET_QUOTA | PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION,
        )?;
        // SAFETY: both handles are valid.
        if unsafe { AssignProcessToJobObject(self.0.0, p.0) } == 0 {
            return Err(last());
        }
        Ok(())
    }

    /// The spawn marker (Bureau §3.9): put a process created with `CREATE_SUSPENDED` in this job, then
    /// resume it, so it runs no instruction outside the job and everything it starts stays inside.
    /// The caller holds the child's handle, so `pid` cannot be reused meanwhile; on an error the
    /// caller kills the still-suspended child.
    #[allow(unsafe_code)]
    pub fn adopt_suspended(&self, pid: u32) -> io::Result<()> {
        self.assign_pid(pid)?;
        let p = open_process(pid, PROCESS_SUSPEND_RESUME)?;
        // SAFETY: a valid process handle with PROCESS_SUSPEND_RESUME (ntdll; stable since XP, as the
        // service spike used it). A negative NTSTATUS is an error.
        let st = unsafe { NtResumeProcess(p.0) };
        if st < 0 {
            return Err(io::Error::other(format!(
                "NtResumeProcess NTSTATUS {st:#x}"
            )));
        }
        Ok(())
    }

    /// Whether process `pid` is in this job.
    #[allow(unsafe_code)]
    pub fn contains_pid(&self, pid: u32) -> io::Result<bool> {
        let p = open_process(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
        let mut r = 0;
        // SAFETY: both handles are valid.
        if unsafe { IsProcessInJob(p.0, self.0.0, &mut r) } == 0 {
            return Err(last());
        }
        Ok(r != 0)
    }

    /// Terminate every process in the job.
    #[allow(unsafe_code)]
    pub fn terminate(&self) -> io::Result<()> {
        // SAFETY: a valid job handle.
        if unsafe { TerminateJobObject(self.0.0, 1) } == 0 {
            return Err(last());
        }
        Ok(())
    }

    /// Processes still in the job.
    #[allow(unsafe_code)]
    pub fn active_processes(&self) -> io::Result<u32> {
        // SAFETY: a zeroed accounting struct is valid output space.
        let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { std::mem::zeroed() };
        // SAFETY: `info` is the struct the class names, with its size.
        let ok = unsafe {
            QueryInformationJobObject(
                self.0.0,
                JobObjectBasicAccountingInformation,
                (&mut info as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                null_mut(),
            )
        };
        if ok == 0 {
            return Err(last());
        }
        Ok(info.ActiveProcesses)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_pipe_admits_our_user_and_refuses_a_second_server() {
        let sid = current_user_sid().unwrap();
        assert!(sid.starts_with("S-1-5-"), "{sid}");
        let name = format!(r"\\.\pipe\news-win-test-{}", std::process::id());
        let server = create_pipe(&name, &crate::sddl_for(&sid), true).unwrap();
        // A second first-instance server on the same name fails.
        assert!(create_pipe(&name, &crate::sddl_for(&sid), true).is_err());
        let client = connect_client(&name).unwrap();
        server.connect().await.unwrap();
        let (facts, _h) = peer_facts(&server, None).unwrap();
        assert_eq!(facts.pid, std::process::id());
        assert_eq!(facts.sid.as_deref(), Some(sid.as_str()));
        assert!(facts.integrity_rid >= 0x2000, "{:#x}", facts.integrity_rid);
        let me = std::env::current_exe().unwrap();
        assert!(
            crate::same_image(&facts.image, &me.to_string_lossy()),
            "{}",
            facts.image
        );
        assert!(crate::same_image(
            &server_image(&client).unwrap(),
            &me.to_string_lossy()
        ));
        assert!(!facts.in_marker_job);
    }

    #[tokio::test]
    async fn a_pipe_for_another_sid_refuses_us() {
        // The spike's way to model another user without a password: a DACL that names someone else.
        let name = format!(r"\\.\pipe\news-win-test-other-{}", std::process::id());
        let _server = create_pipe(&name, &crate::sddl_for("S-1-5-21-1-2-3-4242"), true).unwrap();
        let e = connect_client(&name).unwrap_err();
        assert_eq!(e.raw_os_error(), Some(5), "{e}");
    }

    #[test]
    fn a_root_job_starts_empty() {
        let j = Job::root().unwrap();
        assert_eq!(j.active_processes().unwrap(), 0);
        assert!(!j.contains_pid(std::process::id()).unwrap());
        j.terminate().unwrap();
    }
}
