//! Windows administration for `news service install` and Board (ADR 0007 decisions 11 and 12): the
//! Credential Manager, the no-echo password prompt, elevation, the "Log on as a service" right and
//! the per-user log-on entry for Board. Behind safe functions, like the rest of this crate.

use std::io;
use std::os::windows::ffi::OsStrExt;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, LocalFree};
use windows_sys::Win32::Security::Authentication::Identity::{
    LSA_OBJECT_ATTRIBUTES, LSA_UNICODE_STRING, LsaAddAccountRights, LsaClose,
    LsaNtStatusToWinError, LsaOpenPolicy, POLICY_CREATE_ACCOUNT, POLICY_LOOKUP_NAMES,
};
use windows_sys::Win32::Security::Authorization::ConvertStringSidToSidW;
use windows_sys::Win32::Security::Credentials::{
    CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree, CredReadW,
    CredWriteW,
};
use windows_sys::Win32::Security::{
    GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation,
};
use windows_sys::Win32::System::Console::{
    ENABLE_ECHO_INPUT, GetConsoleMode, GetStdHandle, STD_INPUT_HANDLE, SetConsoleMode,
};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegQueryValueExW, RegSetValueExW,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetExitCodeProcess, INFINITE, OpenProcessToken, WaitForSingleObject,
};
use windows_sys::Win32::UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};

fn last() -> io::Error {
    io::Error::last_os_error()
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

// ---- Credential Manager --------------------------------------------------------------------------

/// Write a generic credential under `target` for the current user. The secret is stored as UTF-8 and
/// is never logged or returned.
#[allow(unsafe_code)]
pub fn cred_write(target: &str, secret: &str) -> io::Result<()> {
    let t = wide(target);
    let mut blob = secret.as_bytes().to_vec();
    let user = wide("news");
    // SAFETY: a zeroed CREDENTIALW is valid; the fields set below are the ones CredWriteW reads.
    let mut c: CREDENTIALW = unsafe { std::mem::zeroed() };
    c.Type = CRED_TYPE_GENERIC;
    c.TargetName = t.as_ptr().cast_mut();
    c.CredentialBlobSize = u32::try_from(blob.len()).map_err(io::Error::other)?;
    c.CredentialBlob = blob.as_mut_ptr();
    c.Persist = CRED_PERSIST_LOCAL_MACHINE;
    c.UserName = user.as_ptr().cast_mut();
    // SAFETY: `c` points at buffers that outlive the call; CredWriteW copies them.
    let ok = unsafe { CredWriteW(&c, 0) };
    blob.fill(0);
    if ok == 0 { Err(last()) } else { Ok(()) }
}

/// Read a credential's secret, or `None` when there is none (or it is not UTF-8).
#[allow(unsafe_code)]
pub fn cred_read(target: &str) -> Option<String> {
    let t = wide(target);
    let mut out: *mut CREDENTIALW = null_mut();
    // SAFETY: `t` is NUL-terminated; `out` receives a credential we free with CredFree.
    if unsafe { CredReadW(t.as_ptr(), CRED_TYPE_GENERIC, 0, &mut out) } == 0 {
        return None;
    }
    // SAFETY: `out` is valid until CredFree; the blob pointer and size describe its bytes.
    let secret = unsafe {
        let c = &*out;
        let bytes = std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize);
        String::from_utf8(bytes.to_vec()).ok()
    };
    // SAFETY: frees the buffer CredReadW allocated, once.
    unsafe { CredFree(out.cast()) };
    secret
}

/// Whether a credential exists (without reading its secret into a `String`).
pub fn cred_exists(target: &str) -> bool {
    cred_read(target).is_some()
}

/// Delete a credential; a missing one is not an error.
#[allow(unsafe_code)]
pub fn cred_delete(target: &str) -> io::Result<()> {
    let t = wide(target);
    // SAFETY: `t` is NUL-terminated.
    if unsafe { CredDeleteW(t.as_ptr(), CRED_TYPE_GENERIC, 0) } == 0 {
        let e = last();
        // ERROR_NOT_FOUND
        return if e.raw_os_error() == Some(1168) {
            Ok(())
        } else {
            Err(e)
        };
    }
    Ok(())
}

// ---- Console -------------------------------------------------------------------------------------

/// Read one line from the console without echoing it (the service account's password).
#[allow(unsafe_code)]
pub fn read_password(prompt: &str) -> io::Result<String> {
    use std::io::Write;
    print!("{prompt}");
    io::stdout().flush()?;
    // SAFETY: standard handle query.
    let h = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
    let mut mode = 0u32;
    // SAFETY: `h` is the console input handle; `mode` receives its mode.
    if unsafe { GetConsoleMode(h, &mut mode) } == 0 {
        return Err(io::Error::other(
            "no console: run this in a terminal window",
        ));
    }
    // SAFETY: sets the mode with echo off; restored below.
    unsafe { SetConsoleMode(h, mode & !ENABLE_ECHO_INPUT) };
    let mut line = String::new();
    let r = io::stdin().read_line(&mut line);
    // SAFETY: restores the mode read above.
    unsafe { SetConsoleMode(h, mode) };
    println!();
    r?;
    while line.ends_with(['\r', '\n']) {
        line.pop();
    }
    Ok(line)
}

// ---- Elevation -----------------------------------------------------------------------------------

/// Whether this process has an elevated (administrator) token.
#[allow(unsafe_code)]
pub fn is_elevated() -> bool {
    let mut tok: HANDLE = null_mut();
    // SAFETY: the current process pseudo-handle; `tok` receives a handle we close.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut tok) } == 0 {
        return false;
    }
    let mut e = TOKEN_ELEVATION { TokenIsElevated: 0 };
    let mut len = 0u32;
    // SAFETY: `e` is a TOKEN_ELEVATION of the size passed.
    let ok = unsafe {
        GetTokenInformation(
            tok,
            TokenElevation,
            std::ptr::from_mut(&mut e).cast(),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        )
    };
    // SAFETY: closes the token handle opened above.
    unsafe { CloseHandle(tok) };
    ok != 0 && e.TokenIsElevated != 0
}

/// Run `exe args` elevated (Windows shows its consent prompt) and wait for it; returns its exit code.
#[allow(unsafe_code)]
pub fn relaunch_elevated(exe: &std::path::Path, args: &str) -> io::Result<u32> {
    let verb = wide("runas");
    let file: Vec<u16> = exe.as_os_str().encode_wide().chain(Some(0)).collect();
    let params = wide(args);
    // SAFETY: a zeroed SHELLEXECUTEINFOW is valid; the fields set below are the ones read.
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS;
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.lpParameters = params.as_ptr();
    info.nShow = 1; // SW_SHOWNORMAL
    // SAFETY: all pointers outlive the call.
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        return Err(last());
    }
    let mut code = 1u32;
    // SAFETY: `hProcess` is the process handle the call returned (NOCLOSEPROCESS); closed once.
    unsafe {
        WaitForSingleObject(info.hProcess, INFINITE);
        GetExitCodeProcess(info.hProcess, &mut code);
        CloseHandle(info.hProcess);
    }
    Ok(code)
}

// ---- "Log on as a service" -----------------------------------------------------------------------

/// Grant `SeServiceLogonRight` to the account with this SID string (`CreateServiceW` does not; without
/// it the first start fails with error 1069). Needs an elevated process.
#[allow(unsafe_code)]
pub fn grant_service_logon(sid: &str) -> io::Result<()> {
    let w = wide(sid);
    let mut psid = null_mut();
    // SAFETY: `w` is NUL-terminated; `psid` receives a LocalAlloc'd SID freed below.
    if unsafe { ConvertStringSidToSidW(w.as_ptr(), &mut psid) } == 0 {
        return Err(last());
    }
    // SAFETY: a zeroed attributes struct is what LsaOpenPolicy expects for defaults.
    let attrs: LSA_OBJECT_ATTRIBUTES = unsafe { std::mem::zeroed() };
    let mut policy: isize = 0;
    // SAFETY: local system (null name), valid attributes; `policy` receives a handle closed below.
    let st = unsafe {
        LsaOpenPolicy(
            null(),
            &attrs,
            (POLICY_CREATE_ACCOUNT | POLICY_LOOKUP_NAMES) as u32,
            &mut policy,
        )
    };
    if st != 0 {
        // SAFETY: frees the SID allocated above.
        unsafe { LocalFree(psid) };
        // SAFETY: plain status conversion.
        return Err(io::Error::from_raw_os_error(
            unsafe { LsaNtStatusToWinError(st) } as i32,
        ));
    }
    let right: Vec<u16> = "SeServiceLogonRight".encode_utf16().collect();
    let name = LSA_UNICODE_STRING {
        Length: (right.len() * 2) as u16,
        MaximumLength: (right.len() * 2) as u16,
        Buffer: right.as_ptr().cast_mut(),
    };
    // SAFETY: valid policy handle, SID and one right; the buffers outlive the call.
    let st = unsafe { LsaAddAccountRights(policy, psid, &name, 1) };
    // SAFETY: closes the policy and frees the SID, once each.
    unsafe {
        LsaClose(policy);
        LocalFree(psid);
    }
    if st != 0 {
        // SAFETY: plain status conversion.
        return Err(io::Error::from_raw_os_error(
            unsafe { LsaNtStatusToWinError(st) } as i32,
        ));
    }
    Ok(())
}

// ---- Board's log-on entry --------------------------------------------------------------------------

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "NewsBoard";

#[allow(unsafe_code)]
fn open_run_key(access: u32) -> io::Result<HKEY> {
    let k = wide(RUN_KEY);
    let mut h: HKEY = null_mut();
    // SAFETY: HKCU is a predefined key; `k` is NUL-terminated; `h` receives a key closed by the caller.
    let st = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            k.as_ptr(),
            0,
            null(),
            REG_OPTION_NON_VOLATILE,
            access,
            null(),
            &mut h,
            null_mut(),
        )
    };
    if st != 0 {
        return Err(io::Error::from_raw_os_error(st as i32));
    }
    Ok(h)
}

/// Set (or with `None`, remove) the current user's log-on entry; `command` is the whole command line.
#[allow(unsafe_code)]
pub fn set_logon_entry(command: Option<&str>) -> io::Result<()> {
    let h = open_run_key(KEY_SET_VALUE)?;
    let name = wide(RUN_VALUE);
    let st = match command {
        Some(c) => {
            let v = wide(c);
            // SAFETY: `h` is open; `v` is a NUL-terminated UTF-16 string of the byte length passed.
            unsafe {
                RegSetValueExW(
                    h,
                    name.as_ptr(),
                    0,
                    REG_SZ,
                    v.as_ptr().cast(),
                    (v.len() * 2) as u32,
                )
            }
        }
        // SAFETY: `h` is open; `name` is NUL-terminated.
        None => match unsafe { RegDeleteValueW(h, name.as_ptr()) } {
            2 => 0, // ERROR_FILE_NOT_FOUND: already gone
            other => other,
        },
    };
    // SAFETY: closes the key opened above.
    unsafe { RegCloseKey(h) };
    if st != 0 {
        return Err(io::Error::from_raw_os_error(st as i32));
    }
    Ok(())
}

/// Whether the log-on entry exists.
#[allow(unsafe_code)]
pub fn logon_entry_exists() -> bool {
    let Ok(h) = open_run_key(KEY_QUERY_VALUE) else {
        return false;
    };
    let name = wide(RUN_VALUE);
    let mut len = 0u32;
    // SAFETY: a size-only query on an open key.
    let st =
        unsafe { RegQueryValueExW(h, name.as_ptr(), null(), null_mut(), null_mut(), &mut len) };
    // SAFETY: closes the key opened above.
    unsafe { RegCloseKey(h) };
    st == 0
}

/// Drop this process's console when it is the only process attached to it (a log-on start gets a
/// console of its own), so Board shows no console window. A terminal shared with a shell is kept.
#[allow(unsafe_code)]
pub fn detach_console_if_alone() {
    use windows_sys::Win32::System::Console::{FreeConsole, GetConsoleProcessList};
    let mut pids = [0u32; 2];
    // SAFETY: `pids` holds the 2 entries passed; the call reports how many processes share the console.
    let n = unsafe { GetConsoleProcessList(pids.as_mut_ptr(), 2) };
    if n == 1 {
        // SAFETY: detaches this process from its console.
        unsafe { FreeConsole() };
    }
}

// ---- The service's access list ---------------------------------------------------------------------

/// Let `sid` query, start and stop the service without elevation, so Board's "Start service" works.
/// Replaces the service's DACL with the usual one (SYSTEM, administrators, interactive read) plus this
/// user. Needs an elevated process.
#[allow(unsafe_code)]
pub fn allow_user_to_control(service: &str, sid: &str) -> io::Result<()> {
    use windows_sys::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows_sys::Win32::System::Services::{
        CloseServiceHandle, OpenSCManagerW, OpenServiceW, SetServiceObjectSecurity,
    };
    const WRITE_DAC: u32 = 0x0004_0000;
    const SC_MANAGER_CONNECT: u32 = 0x0001;
    const DACL_SECURITY_INFORMATION: u32 = 4;
    let sddl = wide(&format!(
        "D:(A;;CCLCSWRPWPDTLOCRRC;;;SY)(A;;CCDCLCSWRPWPDTLOCRSDRCWDWO;;;BA)(A;;CCLCSWLOCRRC;;;IU)(A;;LCRPWPDTLORC;;;{sid})"
    ));
    let mut sd = null_mut();
    // SAFETY: `sddl` is NUL-terminated; `sd` receives a LocalAlloc'd descriptor freed below.
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut sd,
            null_mut(),
        )
    } == 0
    {
        return Err(last());
    }
    // SAFETY: local machine, connect access.
    let scm = unsafe { OpenSCManagerW(null(), null(), SC_MANAGER_CONNECT) };
    let result = if scm.is_null() {
        Err(last())
    } else {
        let name = wide(service);
        // SAFETY: `scm` is open; `name` is NUL-terminated.
        let svc = unsafe { OpenServiceW(scm, name.as_ptr(), WRITE_DAC) };
        let r = if svc.is_null() {
            Err(last())
        } else {
            // SAFETY: `svc` is open with WRITE_DAC; `sd` is a valid descriptor.
            let ok = unsafe { SetServiceObjectSecurity(svc, DACL_SECURITY_INFORMATION, sd) };
            let r = if ok == 0 { Err(last()) } else { Ok(()) };
            // SAFETY: closes the service handle opened above.
            unsafe { CloseServiceHandle(svc) };
            r
        };
        // SAFETY: closes the manager handle opened above.
        unsafe { CloseServiceHandle(scm) };
        r
    };
    // SAFETY: frees the descriptor allocated above.
    unsafe { LocalFree(sd) };
    result
}
