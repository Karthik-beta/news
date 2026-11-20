//! Windows administration for `news service install` and Board (ADR 0007 decisions 11 and 12): the
//! Credential Manager, the no-echo password prompt, elevation, the "Log on as a service" right and
//! the per-user log-on entry for Board, the admin-only install folder, and the service's access list.
//! Behind safe functions, like the rest of this crate.

use std::io;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
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
use zeroize::Zeroizing;

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
    // Zeroed when dropped, on every path out of this function.
    let mut blob = Zeroizing::new(secret.as_bytes().to_vec());
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

/// The longest password the prompt takes, in UTF-16 units (a Windows password is at most 256).
const PASSWORD_MAX: usize = 512;

/// Read one line from the console without echoing it (the service account's password), as
/// NUL-terminated UTF-16 ready for the SCM. `ReadConsoleW` reads straight into one buffer allocated at
/// full size up front, so the password is never copied, reallocated, decoded to UTF-8 or left in the
/// standard input's buffer; the buffer is zeroed when dropped. (The console host's own input buffer
/// belongs to Windows.)
#[allow(unsafe_code)]
pub fn read_password(prompt: &str) -> io::Result<Zeroizing<Vec<u16>>> {
    use std::io::Write;
    use windows_sys::Win32::System::Console::{FlushConsoleInputBuffer, ReadConsoleW};
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
    // Room for the longest password and its CR LF.
    let mut buf = Zeroizing::new(vec![0u16; PASSWORD_MAX + 2]);
    let mut read = 0u32;
    // SAFETY: sets the mode with echo off; restored below.
    unsafe { SetConsoleMode(h, mode & !ENABLE_ECHO_INPUT) };
    // SAFETY: `buf` holds the number of UTF-16 units passed; `read` receives how many were written.
    let ok = unsafe {
        ReadConsoleW(
            h,
            buf.as_mut_ptr().cast(),
            buf.len() as u32,
            &mut read,
            null(),
        )
    };
    let failed = (ok == 0).then(last);
    // SAFETY: restores the mode read above.
    unsafe { SetConsoleMode(h, mode) };
    println!();
    if let Some(e) = failed {
        return Err(e);
    }
    let n = (read as usize).min(buf.len());
    let end = buf[..n]
        .iter()
        .position(|&c| c == u16::from(b'\r') || c == u16::from(b'\n'));
    let Some(end) = end else {
        // SAFETY: drops the rest of an over-long line from the console's input.
        unsafe { FlushConsoleInputBuffer(h) };
        return Err(io::Error::other(if n == 0 {
            "no password was read"
        } else {
            "the password is longer than 512 characters"
        }));
    };
    // Keep the password and one NUL; the line end and the rest are zero. Truncating never reallocates.
    buf[end..].fill(0);
    buf.truncate(end + 1);
    Ok(buf)
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
    check_user_sid(sid)?;
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

// ---- Security descriptors --------------------------------------------------------------------------

/// A security descriptor from a constant SDDL string, freed on drop. Never built from input: a SID
/// from outside goes through `ConvertStringSidToSidW` and `SetEntriesInAclW` instead.
struct Descriptor(windows_sys::Win32::Security::PSECURITY_DESCRIPTOR);

impl Descriptor {
    #[allow(unsafe_code)]
    fn from_sddl(sddl: &'static str) -> io::Result<Self> {
        use windows_sys::Win32::Security::Authorization::{
            ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
        };
        let w = wide(sddl);
        let mut sd = null_mut();
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

    /// The descriptor's DACL (null when it has none).
    #[allow(unsafe_code)]
    fn dacl(&self) -> io::Result<*mut windows_sys::Win32::Security::ACL> {
        use windows_sys::Win32::Security::GetSecurityDescriptorDacl;
        let (mut present, mut defaulted, mut acl) = (0, 0, null_mut());
        // SAFETY: a valid descriptor; the outputs are plain locals.
        if unsafe { GetSecurityDescriptorDacl(self.0, &mut present, &mut acl, &mut defaulted) } == 0
        {
            return Err(last());
        }
        Ok(acl)
    }

    /// The descriptor's owner SID.
    #[allow(unsafe_code)]
    fn owner(&self) -> io::Result<windows_sys::Win32::Security::PSID> {
        use windows_sys::Win32::Security::GetSecurityDescriptorOwner;
        let (mut owner, mut defaulted) = (null_mut(), 0);
        // SAFETY: a valid descriptor; the outputs are plain locals.
        if unsafe { GetSecurityDescriptorOwner(self.0, &mut owner, &mut defaulted) } == 0 {
            return Err(last());
        }
        Ok(owner)
    }
}

impl Drop for Descriptor {
    #[allow(unsafe_code)]
    fn drop(&mut self) {
        // SAFETY: frees the descriptor ConvertStringSecurityDescriptorToSecurityDescriptorW allocated.
        unsafe { LocalFree(self.0) };
    }
}

fn check_user_sid(sid: &str) -> io::Result<()> {
    if crate::is_user_sid(sid) {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "not a user account SID (S-1-5-21-...)",
        ))
    }
}

/// The SID string of an account name (`.\name`, `name` or `DOMAIN\name`), from `LookupAccountNameW`.
#[allow(unsafe_code)]
pub fn account_sid(account: &str) -> io::Result<String> {
    use windows_sys::Win32::Security::LookupAccountNameW;
    if !crate::is_clean_account(account) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the account name is empty, too long, or has a quote or a control character",
        ));
    }
    // `.\name` means this computer; the bare name is looked up on this computer first.
    let w = wide(account.strip_prefix(r".\").unwrap_or(account));
    let (mut sid_len, mut dom_len, mut kind) = (0u32, 0u32, 0);
    // SAFETY: a size query with null buffers; the lengths receive the sizes needed.
    unsafe {
        LookupAccountNameW(
            null(),
            w.as_ptr(),
            null_mut(),
            &mut sid_len,
            null_mut(),
            &mut dom_len,
            &mut kind,
        )
    };
    if sid_len == 0 {
        return Err(last());
    }
    let mut sid = vec![0u8; sid_len as usize];
    let mut dom = vec![0u16; dom_len.max(1) as usize];
    // SAFETY: the buffers have the sizes passed.
    if unsafe {
        LookupAccountNameW(
            null(),
            w.as_ptr(),
            sid.as_mut_ptr().cast(),
            &mut sid_len,
            dom.as_mut_ptr(),
            &mut dom_len,
            &mut kind,
        )
    } == 0
    {
        return Err(last());
    }
    crate::imp::sid_string(sid.as_mut_ptr().cast())
}

// ---- The install folder ----------------------------------------------------------------------------

/// `%ProgramFiles%\News` and its files, from the known-folder API (not the environment, which the
/// user controls).
#[allow(unsafe_code)]
pub fn install_paths() -> io::Result<crate::InstallPaths> {
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{
        FOLDERID_ProgramFiles, KF_FLAG_DEFAULT, SHGetKnownFolderPath,
    };
    let mut p: *mut u16 = null_mut();
    // SAFETY: a known folder id; `p` receives a CoTaskMemAlloc'd string freed below.
    let hr = unsafe {
        SHGetKnownFolderPath(
            &FOLDERID_ProgramFiles,
            KF_FLAG_DEFAULT as u32,
            null_mut(),
            &mut p,
        )
    };
    let dir = if hr >= 0 && !p.is_null() {
        let mut n = 0;
        // SAFETY: `p` is a NUL-terminated string from the call above; we stop at the NUL.
        while unsafe { *p.add(n) } != 0 {
            n += 1;
        }
        // SAFETY: `n` units were just read from `p`.
        let s = unsafe { std::slice::from_raw_parts(p, n) };
        Ok(PathBuf::from(std::ffi::OsString::from_wide(s)))
    } else {
        Err(io::Error::from_raw_os_error(hr))
    };
    // SAFETY: frees the string (the API asks for this even on failure; null is allowed).
    unsafe { CoTaskMemFree(p.cast()) };
    dir.map(|d| crate::InstallPaths::under(&d))
}

/// Give `path` an access list only administrators can write: owner Administrators and a protected
/// DACL (nothing inherited from above) of SYSTEM and Administrators full control, Users read and
/// execute. On the folder the entries are inherited by its files. Needs an elevated process.
#[allow(unsafe_code)]
pub fn make_admin_only(path: &Path, dir: bool) -> io::Result<()> {
    use windows_sys::Win32::Security::Authorization::{SE_FILE_OBJECT, SetNamedSecurityInfoW};
    use windows_sys::Win32::Security::{
        DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
    };
    let sd = Descriptor::from_sddl(if dir {
        "O:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;0x1200a9;;;BU)"
    } else {
        "O:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;0x1200a9;;;BU)"
    })?;
    let (owner, dacl) = (sd.owner()?, sd.dacl()?);
    let name: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: `name` is NUL-terminated; the owner and DACL point into `sd`, alive for the call.
    let st = unsafe {
        SetNamedSecurityInfoW(
            name.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION
                | DACL_SECURITY_INFORMATION
                | PROTECTED_DACL_SECURITY_INFORMATION,
            owner,
            null_mut(),
            dacl,
            null(),
        )
    };
    if st != 0 {
        return Err(io::Error::from_raw_os_error(st as i32));
    }
    Ok(())
}

// ---- The service's access list ---------------------------------------------------------------------

/// Let `sid` query, start and stop the service without elevation, so Board's "Start service" works.
/// Replaces the service's DACL with the usual one (SYSTEM, administrators, interactive read) plus one
/// entry for this user, added with `SetEntriesInAclW` (the SID is never put into an SDDL string).
/// Needs an elevated process.
#[allow(unsafe_code)]
pub fn allow_user_to_control(service: &str, sid: &str) -> io::Result<()> {
    use windows_sys::Win32::Security::Authorization::{
        EXPLICIT_ACCESS_W, NO_MULTIPLE_TRUSTEE, SET_ACCESS, SetEntriesInAclW, TRUSTEE_IS_SID,
        TRUSTEE_IS_USER, TRUSTEE_W,
    };
    use windows_sys::Win32::Security::{
        InitializeSecurityDescriptor, NO_INHERITANCE, SECURITY_DESCRIPTOR,
        SetSecurityDescriptorDacl,
    };
    use windows_sys::Win32::System::Services::{
        CloseServiceHandle, OpenSCManagerW, OpenServiceW, SetServiceObjectSecurity,
    };
    const WRITE_DAC: u32 = 0x0004_0000;
    const SC_MANAGER_CONNECT: u32 = 0x0001;
    const DACL_SECURITY_INFORMATION: u32 = 4;
    // QUERY_STATUS, START, STOP, PAUSE_CONTINUE, INTERROGATE and READ_CONTROL (`LCRPWPDTLORC`).
    const USER_RIGHTS: u32 = 0x0004 | 0x0010 | 0x0020 | 0x0040 | 0x0080 | 0x0002_0000;
    check_user_sid(sid)?;
    let base = Descriptor::from_sddl(
        "D:(A;;CCLCSWRPWPDTLOCRRC;;;SY)(A;;CCDCLCSWRPWPDTLOCRSDRCWDWO;;;BA)(A;;CCLCSWLOCRRC;;;IU)",
    )?;
    let base_dacl = base.dacl()?;
    let w = wide(sid);
    let mut psid = null_mut();
    // SAFETY: `w` is NUL-terminated; `psid` receives a LocalAlloc'd SID freed below.
    if unsafe { ConvertStringSidToSidW(w.as_ptr(), &mut psid) } == 0 {
        return Err(last());
    }
    let entry = EXPLICIT_ACCESS_W {
        grfAccessPermissions: USER_RIGHTS,
        grfAccessMode: SET_ACCESS,
        grfInheritance: NO_INHERITANCE,
        Trustee: TRUSTEE_W {
            pMultipleTrustee: null_mut(),
            MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_USER,
            ptstrName: psid.cast(),
        },
    };
    let mut acl = null_mut();
    // SAFETY: one entry whose SID is alive; `base_dacl` belongs to `base`; `acl` receives a
    // LocalAlloc'd ACL freed below.
    let st = unsafe { SetEntriesInAclW(1, &entry, base_dacl, &mut acl) };
    let result = if st != 0 {
        Err(io::Error::from_raw_os_error(st as i32))
    } else {
        // SAFETY: a zeroed SECURITY_DESCRIPTOR is initialised by the call below.
        let mut sd: SECURITY_DESCRIPTOR = unsafe { std::mem::zeroed() };
        let psd = std::ptr::from_mut(&mut sd).cast();
        // SAFETY: `psd` points at `sd`, alive for the rest of this block; `acl` is valid.
        let ok = unsafe {
            InitializeSecurityDescriptor(psd, 1) != 0
                && SetSecurityDescriptorDacl(psd, 1, acl, 0) != 0
        };
        if !ok {
            Err(last())
        } else {
            // SAFETY: local machine, connect access.
            let scm = unsafe { OpenSCManagerW(null(), null(), SC_MANAGER_CONNECT) };
            if scm.is_null() {
                Err(last())
            } else {
                let name = wide(service);
                // SAFETY: `scm` is open; `name` is NUL-terminated.
                let svc = unsafe { OpenServiceW(scm, name.as_ptr(), WRITE_DAC) };
                let r = if svc.is_null() {
                    Err(last())
                } else {
                    // SAFETY: `svc` is open with WRITE_DAC; `psd` is a valid descriptor.
                    let ok =
                        unsafe { SetServiceObjectSecurity(svc, DACL_SECURITY_INFORMATION, psd) };
                    let r = if ok == 0 { Err(last()) } else { Ok(()) };
                    // SAFETY: closes the service handle opened above.
                    unsafe { CloseServiceHandle(svc) };
                    r
                };
                // SAFETY: closes the manager handle opened above.
                unsafe { CloseServiceHandle(scm) };
                r
            }
        }
    };
    // SAFETY: frees the ACL (null when SetEntriesInAclW failed) and the SID allocated above, once.
    unsafe {
        LocalFree(acl.cast());
        LocalFree(psid);
    }
    result
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn our_own_account_resolves_to_our_own_sid() {
        let me = crate::current_user_sid().unwrap();
        let user = std::env::var("USERNAME").unwrap();
        assert_eq!(account_sid(&user).unwrap(), me);
        assert_eq!(account_sid(&format!(r".\{user}")).unwrap(), me);
        assert!(account_sid("a\"b").is_err());
        assert!(account_sid("no-such-account-7f3a9c").is_err());
    }

    #[test]
    fn a_sid_that_is_not_a_user_account_never_reaches_an_access_list() {
        for bad in ["S-1-1-0", "S-1-5-21-1-2-3-4)(A;;GA;;;WD)", "WD"] {
            let e = allow_user_to_control(crate::SERVICE_NAME, bad).unwrap_err();
            assert_eq!(e.kind(), io::ErrorKind::InvalidInput, "{bad}");
            assert_eq!(
                grant_service_logon(bad).unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
        }
    }

    #[test]
    fn the_install_folder_is_under_program_files() {
        let p = install_paths().unwrap();
        assert!(p.dir.ends_with("News"), "{}", p.dir.display());
        assert!(p.dir.parent().unwrap().exists());
    }
}
