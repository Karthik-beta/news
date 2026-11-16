//! Thin Win32 helpers for the spike. In the real build these live in one platform crate that is
//! allowed `unsafe` (the main workspace forbids it), with these exact calls behind safe functions.

use std::ffi::c_void;
use std::io;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{
    CloseHandle, HANDLE, INVALID_HANDLE_VALUE, LocalFree, UNICODE_STRING,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSecurityDescriptorToStringSecurityDescriptorW, ConvertSidToStringSidW,
    ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo, SDDL_REVISION_1,
    SE_KERNEL_OBJECT,
};
use windows_sys::Win32::Security::{
    DACL_SECURITY_INFORMATION, GetTokenInformation, LABEL_SECURITY_INFORMATION,
    OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, RevertToSelf, SECURITY_ATTRIBUTES,
    TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, IsProcessInJob,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectExtendedLimitInformation, SetInformationJobObject,
};
use windows_sys::Win32::System::Pipes::{
    GetNamedPipeClientProcessId, GetNamedPipeServerProcessId, ImpersonateNamedPipeClient,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentThread, OpenProcess, OpenProcessToken, OpenThread,
    OpenThreadToken, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW, ResumeThread,
    THREAD_SUSPEND_RESUME,
};
use windows_sys::Win32::UI::Shell::CommandLineToArgvW;

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn last() -> io::Error {
    io::Error::last_os_error()
}

unsafe fn from_wide_ptr(p: *const u16) -> String {
    let mut n = 0;
    while unsafe { *p.add(n) } != 0 {
        n += 1;
    }
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(p, n) })
}

/// An owned kernel handle.
pub struct Handle(pub HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
            unsafe { CloseHandle(self.0) };
        }
    }
}
// Kernel handles may be used from any thread.
unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}

/// SID string (S-1-5-21-…) of a token's user.
pub fn token_user_sid(token: HANDLE) -> io::Result<String> {
    unsafe {
        let mut len = 0u32;
        GetTokenInformation(token, TokenUser, null_mut(), 0, &mut len);
        let mut buf = vec![0u8; len as usize];
        if GetTokenInformation(token, TokenUser, buf.as_mut_ptr().cast(), len, &mut len) == 0 {
            return Err(last());
        }
        let tu = &*(buf.as_ptr() as *const TOKEN_USER);
        let mut s: *mut u16 = null_mut();
        if ConvertSidToStringSidW(tu.User.Sid, &mut s) == 0 {
            return Err(last());
        }
        let out = from_wide_ptr(s);
        LocalFree(s.cast());
        Ok(out)
    }
}

pub fn current_user_sid() -> io::Result<String> {
    unsafe {
        let mut tok = null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut tok) == 0 {
            return Err(last());
        }
        let tok = Handle(tok);
        token_user_sid(tok.0)
    }
}

/// A security descriptor built from SDDL, freed on drop.
pub struct Sd(PSECURITY_DESCRIPTOR);
impl Sd {
    pub fn from_sddl(sddl: &str) -> io::Result<Sd> {
        let w = wide(sddl);
        let mut sd: PSECURITY_DESCRIPTOR = null_mut();
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                w.as_ptr(),
                SDDL_REVISION_1,
                &mut sd,
                null_mut(),
            )
        };
        if ok == 0 {
            return Err(last());
        }
        Ok(Sd(sd))
    }
    pub fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.0,
            bInheritHandle: 0,
        }
    }
}
impl Drop for Sd {
    fn drop(&mut self) {
        unsafe { LocalFree(self.0) };
    }
}

/// The owner, DACL and integrity label of a kernel object (a pipe handle), as SDDL.
pub fn object_sddl(h: HANDLE) -> io::Result<String> {
    unsafe {
        let info = OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION | LABEL_SECURITY_INFORMATION;
        let mut sd: PSECURITY_DESCRIPTOR = null_mut();
        let rc = GetSecurityInfo(
            h,
            SE_KERNEL_OBJECT,
            info,
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
            &mut sd,
        );
        if rc != 0 {
            return Err(io::Error::from_raw_os_error(rc as i32));
        }
        let mut s: *mut u16 = null_mut();
        let ok = ConvertSecurityDescriptorToStringSecurityDescriptorW(
            sd,
            SDDL_REVISION_1,
            info,
            &mut s,
            null_mut(),
        );
        LocalFree(sd);
        if ok == 0 {
            return Err(last());
        }
        let out = from_wide_ptr(s);
        LocalFree(s.cast());
        Ok(out)
    }
}

pub fn pipe_client_pid(pipe: HANDLE) -> io::Result<u32> {
    let mut pid = 0;
    if unsafe { GetNamedPipeClientProcessId(pipe, &mut pid) } == 0 {
        return Err(last());
    }
    Ok(pid)
}

pub fn pipe_server_pid(pipe: HANDLE) -> io::Result<u32> {
    let mut pid = 0;
    if unsafe { GetNamedPipeServerProcessId(pipe, &mut pid) } == 0 {
        return Err(last());
    }
    Ok(pid)
}

/// The SID of the pipe client, read by impersonating it on this thread and reverting at once.
/// Works with an identification-level client (SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION).
pub fn pipe_client_sid(pipe: HANDLE) -> io::Result<String> {
    unsafe {
        if ImpersonateNamedPipeClient(pipe) == 0 {
            return Err(last());
        }
        let mut tok = null_mut();
        let opened = OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &mut tok);
        let err = last();
        RevertToSelf();
        if opened == 0 {
            return Err(err);
        }
        let tok = Handle(tok);
        token_user_sid(tok.0)
    }
}

pub fn open_process_query(pid: u32) -> io::Result<Handle> {
    let h = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if h.is_null() {
        return Err(last());
    }
    Ok(Handle(h))
}

/// Full Win32 image path of a process, read from the kernel (not spoofable by the process).
pub fn image_path(process: HANDLE) -> io::Result<String> {
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    if unsafe { QueryFullProcessImageNameW(process, 0, buf.as_mut_ptr(), &mut len) } == 0 {
        return Err(last());
    }
    Ok(String::from_utf16_lossy(&buf[..len as usize]))
}

/// The process's command line (ProcessCommandLineInformation = 60, Windows 8.1+). Read from the
/// target's own memory, so the target could rewrite it: a hint, not a proof.
pub fn command_line(process: HANDLE) -> io::Result<String> {
    use windows_sys::Wdk::System::Threading::NtQueryInformationProcess;
    unsafe {
        let mut len = 0u32;
        NtQueryInformationProcess(process, 60, null_mut(), 0, &mut len);
        if len == 0 {
            return Err(io::Error::other("NtQueryInformationProcess returned no length"));
        }
        let mut buf = vec![0u8; len as usize];
        let st = NtQueryInformationProcess(process, 60, buf.as_mut_ptr().cast(), len, &mut len);
        if st < 0 {
            return Err(io::Error::other(format!("NTSTATUS {st:#x}")));
        }
        let us = &*(buf.as_ptr() as *const UNICODE_STRING);
        let s = std::slice::from_raw_parts(us.Buffer, (us.Length / 2) as usize);
        Ok(String::from_utf16_lossy(s))
    }
}

/// Split a command line the way the C runtime does.
pub fn argv(cmdline: &str) -> Vec<String> {
    let w = wide(cmdline);
    let mut n = 0;
    unsafe {
        let p = CommandLineToArgvW(w.as_ptr(), &mut n);
        if p.is_null() {
            return vec![];
        }
        let out = (0..n as usize).map(|i| from_wide_ptr(*p.add(i))).collect();
        LocalFree(p.cast());
        out
    }
}

/// Is `process` in `job`? With `job` null: is it in any job at all?
pub fn in_job(process: HANDLE, job: HANDLE) -> io::Result<bool> {
    let mut r = 0;
    if unsafe { IsProcessInJob(process, job, &mut r) } == 0 {
        return Err(last());
    }
    Ok(r != 0)
}

pub fn current_process() -> HANDLE {
    unsafe { GetCurrentProcess() }
}

/// A Job Object for AI calls: kill-on-close, and *no* breakaway flags, so nothing started inside
/// it can leave it.
pub fn create_ai_job() -> io::Result<Handle> {
    unsafe {
        let job = CreateJobObjectW(null(), null());
        if job.is_null() {
            return Err(last());
        }
        let job = Handle(job);
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if SetInformationJobObject(
            job.0,
            JobObjectExtendedLimitInformation,
            (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast::<c_void>(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        ) == 0
        {
            return Err(last());
        }
        Ok(job)
    }
}

pub fn assign_to_job(job: HANDLE, process: HANDLE) -> io::Result<()> {
    if unsafe { AssignProcessToJobObject(job, process) } == 0 {
        return Err(last());
    }
    Ok(())
}

/// Resume every thread of a process created with CREATE_SUSPENDED. Lets the caller use
/// std/tokio `Command` (which hides the main thread handle) and still get suspend → assign → resume.
pub fn resume_process(pid: u32) -> io::Result<u32> {
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
        if snap == INVALID_HANDLE_VALUE {
            return Err(last());
        }
        let snap = Handle(snap);
        let mut te: THREADENTRY32 = std::mem::zeroed();
        te.dwSize = size_of::<THREADENTRY32>() as u32;
        let mut resumed = 0;
        let mut ok = Thread32First(snap.0, &mut te);
        while ok != 0 {
            if te.th32OwnerProcessID == pid {
                let th = OpenThread(THREAD_SUSPEND_RESUME, 0, te.th32ThreadID);
                if !th.is_null() {
                    let th = Handle(th);
                    if ResumeThread(th.0) != u32::MAX {
                        resumed += 1;
                    }
                }
            }
            ok = Thread32Next(snap.0, &mut te);
        }
        if resumed == 0 {
            return Err(io::Error::other("no thread resumed"));
        }
        Ok(resumed)
    }
}

/// The environment Windows builds for a fresh log-on of the current user (system + user
/// variables, profile folders), without anything inherited from this process. This is what a
/// service running as the user starts from (see the spike report on how the SCM builds it).
pub fn fresh_user_environment() -> io::Result<Vec<(String, String)>> {
    use windows_sys::Win32::System::Environment::{CreateEnvironmentBlock, DestroyEnvironmentBlock};
    unsafe {
        let mut tok = null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY | 0x0002 /* DUPLICATE */, &mut tok) == 0 {
            return Err(last());
        }
        let tok = Handle(tok);
        let mut block: *mut c_void = null_mut();
        if CreateEnvironmentBlock(&mut block, tok.0, 0) == 0 {
            return Err(last());
        }
        let mut out = vec![];
        let mut p = block as *const u16;
        loop {
            let s = from_wide_ptr(p);
            if s.is_empty() {
                break;
            }
            p = p.add(s.encode_utf16().count() + 1);
            if let Some((k, v)) = s.split_once('=')
                && !k.is_empty()
            {
                out.push((k.to_string(), v.to_string()));
            }
        }
        DestroyEnvironmentBlock(block);
        Ok(out)
    }
}

/// Start `cmdline` with a copy of our token lowered to Low integrity (S-1-16-4096), suspended,
/// in `job` if given, then resume and wait. Returns the exit code. A Low process cannot write to
/// Medium objects (the Hotline pipe, the user's files) and UIPI stops it sending input to Board.
pub fn run_low_integrity(
    cmdline: &str,
    env: &[(String, String)],
    cwd: &str,
    job: Option<HANDLE>,
) -> io::Result<u32> {
    use windows_sys::Win32::Security::Authorization::ConvertStringSidToSidW;
    use windows_sys::Win32::Security::{
        DuplicateTokenEx, GetLengthSid, SID_AND_ATTRIBUTES, SecurityImpersonation,
        SetTokenInformation, TOKEN_ADJUST_DEFAULT, TOKEN_ASSIGN_PRIMARY, TOKEN_DUPLICATE,
        TOKEN_MANDATORY_LABEL, TokenIntegrityLevel, TokenPrimary,
    };
    use windows_sys::Win32::System::Threading::{
        CREATE_NO_WINDOW, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessAsUserW,
        GetExitCodeProcess, INFINITE, PROCESS_INFORMATION, STARTUPINFOW, WaitForSingleObject,
    };
    unsafe {
        let mut tok = null_mut();
        if OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_DUPLICATE | TOKEN_QUERY | TOKEN_ADJUST_DEFAULT | TOKEN_ASSIGN_PRIMARY,
            &mut tok,
        ) == 0
        {
            return Err(last());
        }
        let tok = Handle(tok);
        let mut low = null_mut();
        if DuplicateTokenEx(tok.0, 0, null(), SecurityImpersonation, TokenPrimary, &mut low) == 0 {
            return Err(last());
        }
        let low = Handle(low);
        let mut sid = null_mut();
        let s = wide("S-1-16-4096");
        if ConvertStringSidToSidW(s.as_ptr(), &mut sid) == 0 {
            return Err(last());
        }
        let tml = TOKEN_MANDATORY_LABEL { Label: SID_AND_ATTRIBUTES { Sid: sid, Attributes: 0x20 } };
        let ok = SetTokenInformation(
            low.0,
            TokenIntegrityLevel,
            (&tml as *const TOKEN_MANDATORY_LABEL).cast(),
            (size_of::<TOKEN_MANDATORY_LABEL>() as u32) + GetLengthSid(sid),
        );
        LocalFree(sid);
        if ok == 0 {
            return Err(last());
        }
        let mut block: Vec<u16> = vec![];
        for (k, v) in env {
            block.extend(format!("{k}={v}").encode_utf16());
            block.push(0);
        }
        block.push(0);
        let mut cl = wide(cmdline);
        let cwdw = wide(cwd);
        let mut si: STARTUPINFOW = std::mem::zeroed();
        si.cb = size_of::<STARTUPINFOW>() as u32;
        let mut pi: PROCESS_INFORMATION = std::mem::zeroed();
        if CreateProcessAsUserW(
            low.0,
            null(),
            cl.as_mut_ptr(),
            null(),
            null(),
            0,
            CREATE_SUSPENDED | CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT,
            block.as_ptr().cast(),
            cwdw.as_ptr(),
            &si,
            &mut pi,
        ) == 0
        {
            return Err(last());
        }
        let proc_ = Handle(pi.hProcess);
        let thread = Handle(pi.hThread);
        if let Some(j) = job {
            assign_to_job(j, proc_.0)?;
        }
        ResumeThread(thread.0);
        WaitForSingleObject(proc_.0, INFINITE);
        let mut code = 0u32;
        GetExitCodeProcess(proc_.0, &mut code);
        Ok(code)
    }
}

#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtResumeProcess(process: HANDLE) -> i32;
}

/// Resume a suspended process by its handle (ntdll; undocumented but stable since XP). Far
/// cheaper than a system-wide Toolhelp thread snapshot.
pub fn resume_process_nt(process: HANDLE) -> io::Result<()> {
    let st = unsafe { NtResumeProcess(process) };
    if st < 0 {
        return Err(io::Error::other(format!("NtResumeProcess NTSTATUS {st:#x}")));
    }
    Ok(())
}
