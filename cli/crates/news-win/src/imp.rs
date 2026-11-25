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

/// A process created suspended with a Low-integrity copy of this process's token, its stdout and
/// stderr piped to us. It runs nothing until a job adopts it ([`Job::adopt_suspended`]).
#[derive(Debug)]
pub struct LowChild {
    process: Handle,
    pub pid: u32,
    pub stdout: Option<std::fs::File>,
    pub stderr: Option<std::fs::File>,
}

impl LowChild {
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

/// Start `exe` suspended at Low integrity (ADR 0008 §8.2): `DuplicateTokenEx` of this process's token,
/// the integrity SID `S-1-16-4096` set with `SetTokenInformation`, then `CreateProcessAsUserW`. The
/// child gets exactly `env`, `cwd`, `stdin` and two new pipes, and inherits no other handle. The caller
/// puts it in a job, which resumes it ([`Job::adopt_suspended`]), or kills it.
#[allow(unsafe_code)]
pub fn spawn_low_suspended(
    exe: &std::path::Path,
    args: &[String],
    env: &[(String, String)],
    cwd: &std::path::Path,
    stdin: std::fs::File,
) -> io::Result<LowChild> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::FromRawHandle;
    use windows_sys::Win32::Foundation::{HANDLE_FLAG_INHERIT, SetHandleInformation};
    use windows_sys::Win32::Security::Authorization::ConvertStringSidToSidW;
    use windows_sys::Win32::Security::{
        DuplicateTokenEx, GetLengthSid, SID_AND_ATTRIBUTES, SecurityImpersonation,
        SetTokenInformation, TOKEN_ADJUST_DEFAULT, TOKEN_ASSIGN_PRIMARY, TOKEN_DUPLICATE,
        TokenPrimary,
    };
    use windows_sys::Win32::System::Pipes::CreatePipe;
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
    let mut low = null_mut();
    // SAFETY: a valid token opened with TOKEN_DUPLICATE; `low` receives a primary token we own, with
    // the same access as `own`.
    if unsafe {
        DuplicateTokenEx(
            own.0,
            0,
            null(),
            SecurityImpersonation,
            TokenPrimary,
            &mut low,
        )
    } == 0
    {
        return Err(last());
    }
    let low = Handle(low);
    let sid_text = wide("S-1-16-4096");
    let mut sid: PSID = null_mut();
    // SAFETY: `sid_text` is NUL-terminated; `sid` receives a LocalAlloc'd SID freed below.
    if unsafe { ConvertStringSidToSidW(sid_text.as_ptr(), &mut sid) } == 0 {
        return Err(last());
    }
    let label = TOKEN_MANDATORY_LABEL {
        Label: SID_AND_ATTRIBUTES {
            Sid: sid,
            Attributes: SE_GROUP_INTEGRITY,
        },
    };
    // SAFETY: `label` and the SID it points to are alive for the call; the length covers both.
    let ok = unsafe {
        SetTokenInformation(
            low.0,
            TokenIntegrityLevel,
            (&label as *const TOKEN_MANDATORY_LABEL).cast(),
            size_of::<TOKEN_MANDATORY_LABEL>() as u32 + GetLengthSid(sid),
        )
    };
    let err = last();
    // SAFETY: freeing the SID once; the kernel has copied it into the token.
    unsafe { LocalFree(sid) };
    if ok == 0 {
        return Err(err);
    }

    let pipe = || -> io::Result<(std::fs::File, Handle)> {
        let (mut r, mut w) = (null_mut(), null_mut());
        // SAFETY: plain outputs; both new handles are ours and not inheritable.
        if unsafe { CreatePipe(&mut r, &mut w, null(), 0) } == 0 {
            return Err(last());
        }
        // SAFETY: `r` is a fresh pipe handle owned by nothing else.
        Ok((unsafe { std::fs::File::from_raw_handle(r) }, Handle(w)))
    };
    let (out_r, out_w) = pipe()?;
    let (err_r, err_w) = pipe()?;
    // The child's three handles must be inheritable; the handle list below keeps every other
    // inheritable handle of this process away from it. The write ends close when this returns.
    let inherit = [stdin.as_raw_handle() as HANDLE, out_w.0, err_w.0];
    for h in inherit {
        // SAFETY: a valid handle we own.
        if unsafe { SetHandleInformation(h, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) } == 0 {
            return Err(last());
        }
    }

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
    let dir: Vec<u16> = cwd.as_os_str().encode_wide().chain(Some(0)).collect();
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
                SPAWN_SUSPENDED | CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT,
                block.as_ptr().cast(),
                dir.as_ptr(),
                &si.StartupInfo,
                &mut pi,
            ) != 0
    };
    let err = last();
    // SAFETY: the list was initialised above and is deleted once, after its last use.
    unsafe { DeleteProcThreadAttributeList(list) };
    if !ok {
        return Err(err);
    }
    // Resumed through the process handle (NtResumeProcess), so the thread handle is closed now.
    drop(Handle(pi.hThread));
    Ok(LowChild {
        process: Handle(pi.hProcess),
        pid: pi.dwProcessId,
        stdout: Some(out_r),
        stderr: Some(err_r),
    })
}

/// The integrity RID of process `pid` (0x1000 Low, 0x2000 Medium).
#[allow(unsafe_code)]
pub fn process_integrity(pid: u32) -> io::Result<u32> {
    let p = open_process(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
    let mut tok = null_mut();
    // SAFETY: a valid process handle; `tok` receives a token handle we own.
    if unsafe { OpenProcessToken(p.0, TOKEN_QUERY, &mut tok) } == 0 {
        return Err(last());
    }
    let tok = Handle(tok);
    token_integrity(tok.0)
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
