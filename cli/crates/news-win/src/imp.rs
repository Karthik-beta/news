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

// ---- Low-integrity processes (ADR 0008 §8.2) -------------------------------------------------------

/// A process started with a restricted copy of this process's token: a Low-integrity `claude -p`
/// ([`spawn_low_suspended`], which runs nothing until a job adopts it) or a Medium-integrity tool
/// ([`spawn_medium`]).
#[derive(Debug)]
pub struct RestrictedChild {
    process: Handle,
    pub pid: u32,
}

pub type LowChild = RestrictedChild;

impl RestrictedChild {
    /// Block until the process exits; its exit code.
    #[allow(unsafe_code)]
    pub fn wait(&self) -> io::Result<u32> {
        use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
        use windows_sys::Win32::System::Threading::{
            GetExitCodeProcess, INFINITE, WaitForSingleObject,
        };
        // SAFETY: a valid process handle we own.
        if unsafe { WaitForSingleObject(self.process.0, INFINITE) } != WAIT_OBJECT_0 {
            return Err(last());
        }
        let mut code = 0u32;
        // SAFETY: as above; `code` is a plain local.
        if unsafe { GetExitCodeProcess(self.process.0, &mut code) } == 0 {
            return Err(last());
        }
        Ok(code)
    }

    /// Terminate the process (an error once it has exited).
    #[allow(unsafe_code)]
    pub fn kill(&self) -> io::Result<()> {
        use windows_sys::Win32::System::Threading::TerminateProcess;
        // SAFETY: a valid process handle we own, with the full access CreateProcessAsUserW gives.
        if unsafe { TerminateProcess(self.process.0, 1) } == 0 {
            return Err(last());
        }
        Ok(())
    }
}

/// A SID parsed from a string, freed on drop.
struct LocalSid(PSID);

impl LocalSid {
    #[allow(unsafe_code)]
    fn parse(text: &str) -> io::Result<Self> {
        use windows_sys::Win32::Security::Authorization::ConvertStringSidToSidW;
        let w = wide(text);
        let mut sid: PSID = null_mut();
        // SAFETY: `w` is NUL-terminated; `sid` receives a LocalAlloc'd SID freed on drop.
        if unsafe { ConvertStringSidToSidW(w.as_ptr(), &mut sid) } == 0 {
            return Err(last());
        }
        Ok(Self(sid))
    }
}

impl Drop for LocalSid {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        // SAFETY: frees the SID ConvertStringSidToSidW allocated, once.
        unsafe { LocalFree(self.0) };
    }
}

/// BUILTIN\Administrators.
const ADMINISTRATORS: &str = "S-1-5-32-544";

/// The SID strings of a token's groups whose attributes include all of `flags`.
#[allow(unsafe_code)]
fn group_sids(token: HANDLE, flags: u32) -> io::Result<Vec<String>> {
    use windows_sys::Win32::Security::{SID_AND_ATTRIBUTES, TOKEN_GROUPS, TokenGroups};
    let buf = token_info(token, TokenGroups)?;
    // SAFETY: the buffer holds a TOKEN_GROUPS followed by its GroupCount entries, each SID pointing
    // into the same buffer, which outlives the slice.
    let groups: &[SID_AND_ATTRIBUTES] = unsafe {
        let g = buf.as_ptr().cast::<TOKEN_GROUPS>();
        std::slice::from_raw_parts(
            std::ptr::addr_of!((*g).Groups).cast(),
            (*g).GroupCount as usize,
        )
    };
    groups
        .iter()
        .filter(|a| a.Attributes & flags == flags)
        .map(|a| sid_string(a.Sid))
        .collect()
}

/// A security descriptor from SDDL, freed on drop.
struct LocalSd(PSECURITY_DESCRIPTOR);

impl LocalSd {
    #[allow(unsafe_code)]
    fn parse(sddl: &str) -> io::Result<Self> {
        let w = wide(sddl);
        let mut sd: PSECURITY_DESCRIPTOR = null_mut();
        // SAFETY: `w` is NUL-terminated; `sd` receives a LocalAlloc'd descriptor freed on drop.
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
        Ok(Self(sd))
    }
}

impl Drop for LocalSd {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        // SAFETY: frees the descriptor the conversion allocated, once.
        unsafe { LocalFree(self.0) };
    }
}

/// Give a restricted token the user as owner and a default DACL of the user, SYSTEM and the logon
/// session (all `GA`). Under the SCM the service's token owner and default DACL name Administrators,
/// which the child holds deny-only, so without this it could not open the objects it creates itself.
#[allow(unsafe_code)]
fn set_owner_and_default_dacl(own: HANDLE, child: HANDLE) -> io::Result<()> {
    use windows_sys::Win32::Security::{
        ACL, GetSecurityDescriptorDacl, SetTokenInformation, TOKEN_DEFAULT_DACL, TOKEN_OWNER,
        TokenDefaultDacl, TokenOwner,
    };
    const SE_GROUP_LOGON_ID: u32 = 0xC000_0000;
    let user = token_user_sid(own)?;
    let mut sddl = format!("D:(A;;GA;;;{user})(A;;GA;;;SY)");
    for logon in group_sids(own, SE_GROUP_LOGON_ID)? {
        sddl += &format!("(A;;GA;;;{logon})");
    }
    let sd = LocalSd::parse(&sddl)?;
    let (mut present, mut defaulted, mut dacl) = (0, 0, null_mut::<ACL>());
    // SAFETY: `sd` is a valid descriptor; `dacl` receives a pointer into it, alive while `sd` is.
    if unsafe { GetSecurityDescriptorDacl(sd.0, &mut present, &mut dacl, &mut defaulted) } == 0 {
        return Err(last());
    }
    let info = TOKEN_DEFAULT_DACL { DefaultDacl: dacl };
    // SAFETY: `info` points at the ACL inside `sd`, alive for the call; the kernel copies it.
    if unsafe {
        SetTokenInformation(
            child,
            TokenDefaultDacl,
            (&info as *const TOKEN_DEFAULT_DACL).cast(),
            size_of::<TOKEN_DEFAULT_DACL>() as u32,
        )
    } == 0
    {
        return Err(last());
    }
    let user_sid = LocalSid::parse(&user)?;
    let owner = TOKEN_OWNER { Owner: user_sid.0 };
    // SAFETY: `owner` points at a SID alive for the call; the user SID is a valid owner of its token.
    if unsafe {
        SetTokenInformation(
            child,
            TokenOwner,
            (&owner as *const TOKEN_OWNER).cast(),
            size_of::<TOKEN_OWNER>() as u32,
        )
    } == 0
    {
        return Err(last());
    }
    Ok(())
}

/// Quote one argument the way the MSVC runtime (and Rust's `std`) splits a command line.
fn push_arg(line: &mut String, arg: &str) {
    let quote = arg.is_empty() || arg.contains([' ', '\t']);
    if quote {
        line.push('"');
    }
    let mut backslashes = 0;
    for c in arg.chars() {
        if c == '\\' {
            backslashes += 1;
        } else {
            if c == '"' {
                line.extend(std::iter::repeat_n('\\', backslashes + 1));
            }
            backslashes = 0;
        }
        line.push(c);
    }
    if quote {
        line.extend(std::iter::repeat_n('\\', backslashes));
        line.push('"');
    }
}

/// The command line for `exe` and `args`; the program is always quoted, as `std` does.
fn command_line(exe: &str, args: &[String]) -> String {
    let mut line = format!("\"{exe}\"");
    for a in args {
        line.push(' ');
        push_arg(&mut line, a);
    }
    line
}

/// A `CREATE_UNICODE_ENVIRONMENT` block: `K=V\0` per variable, sorted by name as Windows expects.
fn env_block(env: &[(String, String)]) -> Vec<u16> {
    let mut sorted: Vec<&(String, String)> = env.iter().collect();
    sorted.sort_by_cached_key(|(k, _)| k.to_uppercase());
    let mut b: Vec<u16> = Vec::new();
    for (k, v) in sorted {
        b.extend(format!("{k}={v}\0").encode_utf16());
    }
    if b.is_empty() {
        b.push(0);
    }
    b.push(0);
    b
}

/// Start `exe` suspended at Low integrity (ADR 0008 §8.2). The token is `CreateRestrictedToken` of this
/// process's token with `DISABLE_MAX_PRIVILEGE` (only SeChangeNotifyPrivilege left) and Administrators
/// deny-only (the SCM applies no UAC filtering), then the integrity SID `S-1-16-4096` set with
/// `SetTokenInformation`, then `CreateProcessAsUserW`. The child gets exactly `env`, `cwd` and the
/// three handles: inheritable copies passed only through `PROC_THREAD_ATTRIBUTE_HANDLE_LIST` and
/// closed straight after, so the caller's own handles are never inheritable. The caller puts it in a
/// job, which resumes it ([`Job::adopt_suspended`]), or kills it.
pub fn spawn_low_suspended(
    exe: &std::path::Path,
    args: &[String],
    env: &[(String, String)],
    cwd: &std::path::Path,
    stdin: &std::fs::File,
    stdout: &std::fs::File,
    stderr: &std::fs::File,
) -> io::Result<LowChild> {
    let stdio = [stdin, stdout, stderr];
    spawn_restricted(exe, args, env, Some(cwd), stdio, LOW, SPAWN_SUSPENDED)
}

/// Start a tool (docker, bun) with the same restricted token as [`spawn_low_suspended`] but at
/// Medium integrity, running at once (security review on #10, finding 1): the service runs from the
/// SCM with Karthik's unfiltered admin token, and no program it starts may hold that token. `cwd`
/// `None` keeps this process's working folder.
pub fn spawn_medium(
    exe: &std::path::Path,
    args: &[String],
    env: &[(String, String)],
    cwd: Option<&std::path::Path>,
    stdio: [&std::fs::File; 3],
) -> io::Result<RestrictedChild> {
    spawn_restricted(exe, args, env, cwd, stdio, MEDIUM, CREATE_NO_WINDOW)
}

/// Mandatory integrity SIDs.
const LOW: &str = "S-1-16-4096";
const MEDIUM: &str = "S-1-16-8192";

#[allow(unsafe_code)]
fn spawn_restricted(
    exe: &std::path::Path,
    args: &[String],
    env: &[(String, String)],
    cwd: Option<&std::path::Path>,
    stdio: [&std::fs::File; 3],
    integrity: &'static str,
    flags: u32,
) -> io::Result<RestrictedChild> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{DUPLICATE_SAME_ACCESS, DuplicateHandle};
    use windows_sys::Win32::Security::{
        CreateRestrictedToken, DISABLE_MAX_PRIVILEGE, GetLengthSid, SID_AND_ATTRIBUTES,
        SetTokenInformation, TOKEN_ADJUST_DEFAULT, TOKEN_ASSIGN_PRIMARY, TOKEN_DUPLICATE,
    };
    use windows_sys::Win32::System::Threading::{
        CREATE_UNICODE_ENVIRONMENT, CreateProcessAsUserW, DeleteProcThreadAttributeList,
        EXTENDED_STARTUPINFO_PRESENT, InitializeProcThreadAttributeList,
        LPPROC_THREAD_ATTRIBUTE_LIST, PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROCESS_INFORMATION,
        STARTF_USESTDHANDLES, STARTUPINFOEXW, UpdateProcThreadAttribute,
    };
    const SE_GROUP_INTEGRITY: u32 = 0x20;

    let mut own = null_mut();
    // SAFETY: the current-process pseudo-handle; `own` receives a token handle we own.
    if unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_DUPLICATE | TOKEN_QUERY | TOKEN_ADJUST_DEFAULT | TOKEN_ASSIGN_PRIMARY,
            &mut own,
        )
    } == 0
    {
        return Err(last());
    }
    let own = Handle(own);
    let admins = LocalSid::parse(ADMINISTRATORS)?;
    let deny = [SID_AND_ATTRIBUTES {
        Sid: admins.0,
        Attributes: 0,
    }];
    let mut low = null_mut();
    // SAFETY: a valid token opened with TOKEN_DUPLICATE; `deny` outlives the call; `low` receives a
    // primary token we own.
    if unsafe {
        CreateRestrictedToken(
            own.0,
            DISABLE_MAX_PRIVILEGE,
            1,
            deny.as_ptr(),
            0,
            null(),
            0,
            null(),
            &mut low,
        )
    } == 0
    {
        return Err(last());
    }
    let low = Handle(low);
    set_owner_and_default_dacl(own.0, low.0)?;
    let low_sid = LocalSid::parse(integrity)?;
    let label = TOKEN_MANDATORY_LABEL {
        Label: SID_AND_ATTRIBUTES {
            Sid: low_sid.0,
            Attributes: SE_GROUP_INTEGRITY,
        },
    };
    // SAFETY: `label` and the SID it points to are alive for the call; the length covers both.
    if unsafe {
        SetTokenInformation(
            low.0,
            TokenIntegrityLevel,
            (&label as *const TOKEN_MANDATORY_LABEL).cast(),
            size_of::<TOKEN_MANDATORY_LABEL>() as u32 + GetLengthSid(low_sid.0),
        )
    } == 0
    {
        return Err(last());
    }

    let mut copies = Vec::with_capacity(3);
    for f in stdio {
        let mut h = null_mut();
        // SAFETY: duplicates a valid handle of this process into an inheritable copy we own.
        if unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                f.as_raw_handle() as HANDLE,
                GetCurrentProcess(),
                &mut h,
                0,
                1,
                DUPLICATE_SAME_ACCESS,
            )
        } == 0
        {
            return Err(last());
        }
        copies.push(Handle(h));
    }
    let inherit = [copies[0].0, copies[1].0, copies[2].0];

    let mut size = 0usize;
    // SAFETY: a size query with a null list; it fails by design and sets `size`.
    unsafe { InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut size) };
    let mut buf = vec![0usize; size.div_ceil(size_of::<usize>())];
    let list: LPPROC_THREAD_ATTRIBUTE_LIST = buf.as_mut_ptr().cast();
    // SAFETY: `buf` holds at least `size` pointer-aligned bytes.
    if unsafe { InitializeProcThreadAttributeList(list, 1, 0, &mut size) } == 0 {
        return Err(last());
    }
    // SAFETY: a zeroed STARTUPINFOEXW is valid; the fields the call reads are set below.
    let mut si: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
    si.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    si.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    si.StartupInfo.hStdInput = inherit[0];
    si.StartupInfo.hStdOutput = inherit[1];
    si.StartupInfo.hStdError = inherit[2];
    si.lpAttributeList = list;
    let mut line = wide(&command_line(&exe.to_string_lossy(), args));
    let app: Vec<u16> = exe.as_os_str().encode_wide().chain(Some(0)).collect();
    let dir: Option<Vec<u16>> = cwd.map(|c| c.as_os_str().encode_wide().chain(Some(0)).collect());
    let block = env_block(env);
    // SAFETY: a zeroed PROCESS_INFORMATION is valid output space.
    let mut pi: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: `inherit`, `line`, `app`, `dir`, `block` and `si` outlive both calls; `line` is mutable
    // as CreateProcessAsUserW requires; `low` is a primary token with TOKEN_ASSIGN_PRIMARY.
    let ok = unsafe {
        UpdateProcThreadAttribute(
            list,
            0,
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
            inherit.as_ptr().cast(),
            size_of_val(&inherit),
            null_mut(),
            null(),
        ) != 0
            && CreateProcessAsUserW(
                low.0,
                app.as_ptr(),
                line.as_mut_ptr(),
                null(),
                null(),
                1,
                flags | CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT,
                block.as_ptr().cast(),
                dir.as_ref().map_or(null(), |d| d.as_ptr()),
                &si.StartupInfo,
                &mut pi,
            ) != 0
    };
    let err = last();
    // Close the inheritable copies at once, so no other process started from here can inherit them.
    drop(copies);
    // SAFETY: the list was initialised above and is deleted once, after its last use.
    unsafe { DeleteProcThreadAttributeList(list) };
    if !ok {
        return Err(err);
    }
    // Resumed through the process handle (NtResumeProcess), so the thread handle is closed now.
    drop(Handle(pi.hThread));
    Ok(RestrictedChild {
        process: Handle(pi.hProcess),
        pid: pi.dwProcessId,
    })
}

/// What a process's token holds, for the Low-integrity check (ADR 0008 §8.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenFacts {
    /// 0x1000 Low, 0x2000 Medium.
    pub integrity_rid: u32,
    /// BUILTIN\Administrators is in the token and enabled (not deny-only).
    pub admins_enabled: bool,
    /// The names of the enabled privileges.
    pub privileges_enabled: Vec<String>,
    /// The SID new objects are owned by.
    pub owner: String,
    /// The SIDs the default DACL's access-allowed entries name, in order.
    pub default_dacl: Vec<String>,
}

/// The token's owner SID and the SIDs of its default DACL's access-allowed entries.
#[allow(unsafe_code)]
fn owner_and_default_dacl(token: HANDLE) -> io::Result<(String, Vec<String>)> {
    use windows_sys::Win32::Security::{
        ACCESS_ALLOWED_ACE, ACL, GetAce, TOKEN_DEFAULT_DACL, TOKEN_OWNER, TokenDefaultDacl,
        TokenOwner,
    };
    let buf = token_info(token, TokenOwner)?;
    // SAFETY: the buffer holds a TOKEN_OWNER whose SID points into the same buffer.
    let owner = sid_string(unsafe { (*buf.as_ptr().cast::<TOKEN_OWNER>()).Owner })?;
    let buf = token_info(token, TokenDefaultDacl)?;
    // SAFETY: the buffer holds a TOKEN_DEFAULT_DACL whose ACL (or null) points into the same buffer.
    let acl: *mut ACL = unsafe { (*buf.as_ptr().cast::<TOKEN_DEFAULT_DACL>()).DefaultDacl };
    let mut sids = Vec::new();
    if !acl.is_null() {
        // SAFETY: a valid ACL inside `buf`.
        let count = unsafe { (*acl).AceCount };
        for i in 0..u32::from(count) {
            let mut ace = null_mut();
            // SAFETY: `i` is below the ACL's count; `ace` receives a pointer into the ACL.
            if unsafe { GetAce(acl, i, &mut ace) } == 0 {
                return Err(last());
            }
            let ace = ace.cast::<ACCESS_ALLOWED_ACE>();
            // SAFETY: every ACE starts with an ACE_HEADER; type 0 is ACCESS_ALLOWED, whose SID
            // starts at SidStart inside the ACE.
            unsafe {
                if (*ace).Header.AceType == 0 {
                    sids.push(sid_string(std::ptr::addr_of_mut!((*ace).SidStart).cast())?);
                }
            }
        }
    }
    Ok((owner, sids))
}

/// A token information class's buffer, 8-byte aligned.
#[allow(unsafe_code)]
fn token_info(
    token: HANDLE,
    class: windows_sys::Win32::Security::TOKEN_INFORMATION_CLASS,
) -> io::Result<Vec<u64>> {
    let mut len = 0u32;
    // SAFETY: a size query with a null buffer.
    unsafe { GetTokenInformation(token, class, null_mut(), 0, &mut len) };
    let mut buf = vec![0u64; (len as usize).div_ceil(8)];
    // SAFETY: `buf` has at least `len` bytes.
    if unsafe { GetTokenInformation(token, class, buf.as_mut_ptr().cast(), len, &mut len) } == 0 {
        return Err(last());
    }
    Ok(buf)
}

/// The token facts of process `pid`.
#[allow(unsafe_code)]
pub fn process_token(pid: u32) -> io::Result<TokenFacts> {
    use windows_sys::Win32::Security::{
        LUID_AND_ATTRIBUTES, LookupPrivilegeNameW, SE_PRIVILEGE_ENABLED, SID_AND_ATTRIBUTES,
        TOKEN_GROUPS, TOKEN_PRIVILEGES, TokenGroups, TokenPrivileges,
    };
    const SE_GROUP_ENABLED: u32 = 0x4;
    let p = open_process(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
    let mut tok = null_mut();
    // SAFETY: a valid process handle; `tok` receives a token handle we own.
    if unsafe { OpenProcessToken(p.0, TOKEN_QUERY, &mut tok) } == 0 {
        return Err(last());
    }
    let tok = Handle(tok);
    let groups = token_info(tok.0, TokenGroups)?;
    // SAFETY: the buffer holds a TOKEN_GROUPS followed by its GroupCount entries, each SID pointing
    // into the same buffer, which outlives the slice.
    let groups: &[SID_AND_ATTRIBUTES] = unsafe {
        let g = groups.as_ptr().cast::<TOKEN_GROUPS>();
        std::slice::from_raw_parts(
            std::ptr::addr_of!((*g).Groups).cast(),
            (*g).GroupCount as usize,
        )
    };
    let admins_enabled = groups.iter().any(|a| {
        a.Attributes & SE_GROUP_ENABLED != 0 && sid_string(a.Sid).is_ok_and(|s| s == ADMINISTRATORS)
    });
    let privs = token_info(tok.0, TokenPrivileges)?;
    // SAFETY: as above, a TOKEN_PRIVILEGES followed by its PrivilegeCount entries.
    let privs: &[LUID_AND_ATTRIBUTES] = unsafe {
        let t = privs.as_ptr().cast::<TOKEN_PRIVILEGES>();
        std::slice::from_raw_parts(
            std::ptr::addr_of!((*t).Privileges).cast(),
            (*t).PrivilegeCount as usize,
        )
    };
    let mut privileges_enabled = Vec::new();
    for p in privs
        .iter()
        .filter(|p| p.Attributes & SE_PRIVILEGE_ENABLED != 0)
    {
        let mut name = [0u16; 128];
        let mut n = name.len() as u32;
        // SAFETY: `name` holds `n` u16s; `p.Luid` is a plain value.
        if unsafe { LookupPrivilegeNameW(null(), &p.Luid, name.as_mut_ptr(), &mut n) } == 0 {
            return Err(last());
        }
        privileges_enabled.push(String::from_utf16_lossy(&name[..n as usize]));
    }
    let (owner, default_dacl) = owner_and_default_dacl(tok.0)?;
    Ok(TokenFacts {
        integrity_rid: token_integrity(tok.0)?,
        admins_enabled,
        privileges_enabled,
        owner,
        default_dacl,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn arguments_are_quoted_as_the_msvc_runtime_splits_them() {
        let a = ["", "a b", r#"{"x":1}"#, r"C:\dir\", r#"a\"b"#, "c d\\"].map(String::from);
        assert_eq!(
            command_line(r"C:\c.exe", &a),
            r#""C:\c.exe" "" "a b" {\"x\":1} C:\dir\ a\\\"b "c d\\""#
        );
        let b = env_block(&[("b".into(), "2".into()), ("A".into(), "1".into())]);
        assert_eq!(String::from_utf16_lossy(&b), "A=1\0b=2\0\0");
        assert_eq!(env_block(&[]), [0, 0]);
    }

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
