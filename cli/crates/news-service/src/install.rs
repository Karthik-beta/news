//! `news service install|uninstall|stage|start|stop|status` (ADR 0007 decision 12, spike item 1).
//!
//! Karthik runs `install` in his own terminal. Windows needs an administrator token to create a
//! service, so a terminal without one re-launches `news.exe` through the UAC prompt. The elevated copy
//! checks that the account resolves to his own SID, asks for his Windows password at a no-echo prompt
//! (the agent never handles it), copies the binary to `%ProgramFiles%\News\news.exe` in a folder only
//! administrators can write, grants his account the "Log on as a service" right, registers that path
//! under his own account with a delayed automatic start, restart on failure and only the privileges
//! the service needs, lets his account start and stop it (so Board's "Start service" needs no
//! elevation), and adds Board's log-on entry for the installed binary. `--dry-run` prints the plan and
//! changes nothing. Running `install` again updates the registration and the installed binary, which
//! is also how he changes the stored password after changing his Windows password.
//!
//! `stage <path>` is the other elevated step: it copies a new build to `news.exe.new` in the same
//! folder and records its SHA-256 beside it, for `service.update` to verify before the swap.

use crate::update;
use news_core::NewsError;
use news_win::{InstallPaths, ServiceSpec};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// The service's name in the SCM.
pub const SERVICE_NAME: &str = news_win::SERVICE_NAME;

pub const DISPLAY_NAME: &str = "News Bureau";

/// The file in the install folder that holds the repository root, for the service to read.
const ROOT_FILE: &str = "repo-root";

pub const DESCRIPTION: &str =
    "News Bureau: Chief, Rota and the Dispatch workers. Controlled from Board.";

/// The service account: `.\user` for a local account (a Microsoft account is local too), or
/// `DOMAIN\user` on a domain.
pub fn account_for(user: &str, domain: &str, computer: &str) -> String {
    if domain.is_empty() || domain.eq_ignore_ascii_case(computer) {
        format!(r".\{user}")
    } else {
        format!(r"{domain}\{user}")
    }
}

/// The invoking user's account from the environment.
pub fn default_account() -> Option<String> {
    let user = std::env::var("USERNAME").ok().filter(|u| !u.is_empty())?;
    let domain = std::env::var("USERDOMAIN").unwrap_or_default();
    let computer = std::env::var("COMPUTERNAME").unwrap_or_default();
    Some(account_for(&user, &domain, &computer))
}

/// The install folder: `%ProgramFiles%\News` (off Windows, the usual path, for `--dry-run`).
pub fn install_paths() -> InstallPaths {
    news_win::install_paths()
        .unwrap_or_else(|_| InstallPaths::under(Path::new(r"C:\Program Files")))
}

/// The repository root `install` recorded (the service starts in `C:\Windows\System32`, so it cannot
/// find one from its working directory), if any.
pub fn recorded_root() -> Option<PathBuf> {
    let text = std::fs::read_to_string(install_paths().dir.join(ROOT_FILE)).ok()?;
    Some(PathBuf::from(text.trim())).filter(|p| !p.as_os_str().is_empty())
}

/// `exe` is the installed binary, never the one `install` was run from.
pub fn spec(exe: &Path, account: &str) -> ServiceSpec {
    ServiceSpec {
        name: SERVICE_NAME.to_owned(),
        display_name: DISPLAY_NAME.to_owned(),
        description: DESCRIPTION.to_owned(),
        exe: exe.to_path_buf(),
        args: vec!["service".to_owned(), "run".to_owned()],
        account: account.to_owned(),
    }
}

/// Board's log-on command line.
pub fn board_command(exe: &Path) -> String {
    format!("\"{}\" board --minimized", exe.display())
}

/// What `install` would do; never includes a password.
pub fn plan(spec: &ServiceSpec, source: &Path, root: Option<&Path>) -> Value {
    json!({
        "dry_run": true,
        "service": spec.name,
        "display_name": spec.display_name,
        "copy_from": source,
        "binary": spec.exe,
        "repo_root": root,
        "tools": found_tools().into_iter().map(|(n, p)| (n.to_owned(), json!(p))).collect::<serde_json::Map<_, _>>(),
        "arguments": spec.args.join(" "),
        "account": spec.account,
        "start": "automatic (delayed)",
        "on_failure": "restart after 5 s, then 60 s, then 60 s; reset after a day",
        "also_restarts_after": "an update (the service exits with a code of its own)",
        "privileges": "SeChangeNotifyPrivilege only",
        "steps": [
            "re-launch elevated (Windows asks for permission) unless already elevated",
            "check that the account resolves to the SID of the user who ran install",
            "ask for the account's Windows password at a prompt that does not echo; it goes to the SCM only",
            "copy this news.exe to the binary path, in a folder only administrators can write",
            "grant the account the 'Log on as a service' right",
            "create the service, or update it when it exists (this is how the password is changed)",
            "keep only SeChangeNotifyPrivilege in the service's token",
            "let the account query, start and stop the service without elevation",
            "record the repository root (repo_root) in the install folder, for Audit to read",
            "record where docker and bun are (absolute paths found on PATH now) in the install \
             folder: the service runs those, never a PATH lookup",
            "add Board's log-on entry for the account: the installed binary, board --minimized",
        ],
        "text": format!(
            "Would copy {} to {} and register service {} under {} (delayed automatic start, restart \
             on failure) and add Board's log-on entry. Nothing was changed.",
            source.display(),
            spec.exe.display(),
            spec.name,
            spec.account
        ),
    })
}

/// What Karthik passes on the command line.
#[derive(Debug, Clone, Default)]
pub struct InstallArgs {
    pub dry_run: bool,
    /// Set by the elevated re-launch: pause at the end so its window can be read.
    pub elevated: bool,
    pub account: Option<String>,
    /// The News repository Audit reads, recorded in the install folder (the caller defaults it to the
    /// repository `install` was run from). Absolute, with `pipeline/eval/`.
    pub root: Option<PathBuf>,
    /// The invoking user's SID, passed to the elevated copy (which may run under another token).
    pub sid: Option<String>,
}

fn io(what: &str, e: std::io::Error) -> NewsError {
    let hint = match e.raw_os_error() {
        Some(5) => " (access denied: run it again and accept the administrator prompt)",
        Some(1069 | 1326) => " (the account name or password was rejected)",
        Some(1332) => " (no account has that name)",
        _ => "",
    };
    NewsError::precondition(format!("{what}: {e}{hint}"))
}

fn this_exe() -> Result<PathBuf, NewsError> {
    std::env::current_exe().map_err(|e| NewsError::internal(format!("cannot find news.exe: {e}")))
}

/// The elevated window stays open until Enter, so its result can be read.
fn pause_if(elevated: bool, result: &Result<Value, NewsError>) {
    if elevated {
        match result {
            Ok(_) => println!("\nDone. Press Enter to close this window."),
            Err(e) => println!("\nFailed: {}\nPress Enter to close this window.", e.message),
        }
        let mut s = String::new();
        let _ = std::io::stdin().read_line(&mut s);
    }
}

pub fn install(a: &InstallArgs) -> Result<Value, NewsError> {
    let source = this_exe()?;
    let account = a
        .account
        .clone()
        .or_else(default_account)
        .ok_or_else(|| NewsError::usage("cannot tell the account name; pass --account"))?;
    // It goes on the elevated command line and to LookupAccountNameW.
    if !news_win::is_clean_account(&account) {
        return Err(NewsError::usage(
            "the account name is empty, too long, or has a quote or a control character",
        ));
    }
    let root = check_root(a.root.as_deref())?;
    let paths = install_paths();
    let spec = spec(&paths.exe, &account);
    if a.dry_run {
        return Ok(plan(&spec, &source, root));
    }
    if !cfg!(windows) {
        return Err(NewsError::precondition(
            "the Bureau service is a Windows service",
        ));
    }
    let sid = match &a.sid {
        Some(s) => s.clone(),
        None => news_win::current_user_sid().map_err(|e| io("cannot read your account", e))?,
    };
    if !news_win::is_elevated() {
        if !news_win::is_user_sid(&sid) {
            return Err(NewsError::precondition(format!(
                "{sid} is not a local, Microsoft or domain account (S-1-5-21-...)"
            )));
        }
        println!("Windows will ask for permission to continue as administrator.");
        let mut args =
            format!("service install --elevated --account \"{account}\" --sid \"{sid}\"");
        if let Some(r) = root {
            args += &format!(" --root \"{}\"", r.display());
        }
        let code = news_win::relaunch_elevated(&source, &args)
            .map_err(|e| io("could not start the elevated install", e))?;
        if code != 0 {
            return Err(NewsError::precondition(format!(
                "the elevated install ended with code {code}; its window showed why"
            )));
        }
        return Ok(
            json!({"installed": true, "service": SERVICE_NAME, "account": account,
            "binary": paths.exe,
            "text": format!("{SERVICE_NAME} is installed under {account}, running {}. Start it from \
                             Board, or with `news service start`.", paths.exe.display())}),
        );
    }
    let result = install_elevated(&spec, &sid, &source, &paths, root);
    pause_if(a.elevated, &result);
    result
}

/// `--root` must be an absolute folder holding `pipeline/eval/`, and fit in the elevated command line.
fn check_root(root: Option<&Path>) -> Result<Option<&Path>, NewsError> {
    let Some(r) = root else { return Ok(None) };
    if !r.is_absolute() || !r.join("pipeline/eval").is_dir() || r.to_string_lossy().contains('"') {
        return Err(NewsError::usage(format!(
            "{} is not an absolute path to the News repository (no pipeline/eval/ in it)",
            r.display()
        )));
    }
    Ok(Some(r))
}

/// Write the root into the admin-only install folder, so only an administrator can change it.
fn record_root(root: &Path, p: &InstallPaths) -> Result<(), NewsError> {
    let f = p.dir.join(ROOT_FILE);
    std::fs::write(&f, root.to_string_lossy().as_bytes())
        .map_err(|e| io(&format!("cannot write {}", f.display()), e))?;
    news_win::make_admin_only(&f, false)
        .map_err(|e| io(&format!("cannot restrict {}", f.display()), e))
}

/// The absolute paths of the tools the service runs (docker, bun), found on PATH now.
pub fn found_tools() -> Vec<(&'static str, PathBuf)> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    news_win::TOOLS
        .iter()
        .filter_map(|t| Some((*t, news_win::find_on_path(t, &path)?)))
        .collect()
}

/// Write the tools' paths into the admin-only install folder (security review on #10, finding 1):
/// the service never looks a tool up on PATH, which any process running as Karthik can change.
fn record_tools(p: &InstallPaths) -> Result<Value, NewsError> {
    let found = found_tools();
    let f = p.dir.join(news_win::TOOLS_FILE);
    std::fs::write(&f, news_win::tools_text(&found))
        .map_err(|e| io(&format!("cannot write {}", f.display()), e))?;
    news_win::make_admin_only(&f, false)
        .map_err(|e| io(&format!("cannot restrict {}", f.display()), e))?;
    Ok(found
        .into_iter()
        .map(|(n, p)| (n.to_owned(), json!(p)))
        .collect())
}

/// The elevated install's check of what the command line said: the SID is a user account's, and the
/// account name resolves (`LookupAccountNameW`) to that same SID.
pub fn check_account(account: &str, sid: &str) -> Result<(), NewsError> {
    if !news_win::is_user_sid(sid) {
        return Err(NewsError::usage(format!(
            "{sid:?} is not a local, Microsoft or domain account SID (S-1-5-21-...)"
        )));
    }
    if !news_win::is_clean_account(account) {
        return Err(NewsError::usage(
            "the account name is empty, too long, or has a quote or a control character",
        ));
    }
    let resolved = news_win::account_sid(account)
        .map_err(|e| io(&format!("cannot look up the account {account}"), e))?;
    if resolved != sid {
        return Err(NewsError::precondition(format!(
            "the account {account} is {resolved}, not the user who ran install ({sid})"
        ))
        .with_hint("pass --account with your own account, or leave it out"));
    }
    Ok(())
}

/// Copy `source` to the installed path, in a folder whose access list is set to administrators-only
/// first. A build staged before this install is dropped: only `news service stage` puts one there.
fn install_binary(source: &Path, p: &InstallPaths) -> Result<(), NewsError> {
    std::fs::create_dir_all(&p.dir)
        .map_err(|e| io(&format!("cannot create {}", p.dir.display()), e))?;
    news_win::make_admin_only(&p.dir, true)
        .map_err(|e| io(&format!("cannot restrict {}", p.dir.display()), e))?;
    for f in [&p.staged, &p.staged_hash] {
        match std::fs::remove_file(f) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                return Err(io(&format!("cannot remove {}", f.display()), e));
            }
            _ => {}
        }
    }
    if !news_win::same_image(&source.to_string_lossy(), &p.exe.to_string_lossy()) {
        if p.exe.exists() {
            // A running service's image cannot be overwritten, but it can be renamed aside.
            let old = p.dir.join("news.exe.old");
            let _ = std::fs::remove_file(&old);
            std::fs::rename(&p.exe, &old)
                .map_err(|e| io(&format!("cannot move the old {} aside", p.exe.display()), e))?;
        }
        std::fs::copy(source, &p.exe)
            .map_err(|e| io(&format!("cannot copy news.exe to {}", p.exe.display()), e))?;
    }
    news_win::make_admin_only(&p.exe, false)
        .map_err(|e| io(&format!("cannot restrict {}", p.exe.display()), e))
}

fn install_elevated(
    spec: &ServiceSpec,
    sid: &str,
    source: &Path,
    paths: &InstallPaths,
    root: Option<&Path>,
) -> Result<Value, NewsError> {
    check_account(&spec.account, sid)?;
    // NUL-terminated UTF-16 in one buffer, zeroed when it is dropped (on every path out of here).
    let password = news_win::read_password(&format!(
        "Windows password for {} (typed here; News never stores it): ",
        spec.account
    ))
    .map_err(|e| io("cannot read the password", e))?;
    if password.first().is_none_or(|&c| c == 0) {
        return Err(NewsError::usage(
            "the password is empty; a service cannot log on without one (turn off \"only allow \
             Windows Hello sign-in\" in Settings, Accounts, Sign-in options, for the install)",
        ));
    }
    install_binary(source, paths)?;
    if let Some(r) = root {
        record_root(r, paths)?;
    }
    let tools = record_tools(paths)?;
    news_win::grant_service_logon(sid).map_err(|e| io("cannot grant 'Log on as a service'", e))?;
    news_win::scm::install(spec, Some(&password))
        .map_err(|e| io("cannot register the service", e))?;
    drop(password);
    news_win::allow_user_to_control(&spec.name, sid)
        .map_err(|e| io("cannot set the service's access list", e))?;
    news_win::set_logon_entry(Some(&board_command(&paths.exe)))
        .map_err(|e| io("cannot add Board's log-on entry", e))?;
    Ok(
        json!({"installed": true, "service": spec.name, "account": spec.account, "binary": spec.exe,
        "tools": tools,
        "text": format!("{} is installed under {}, running {}. Start it from Board, or with \
                         `news service start` (restart it if it was running).",
                        spec.name, spec.account, spec.exe.display())}),
    )
}

/// `news service stage <path>`: copy a new build into the install folder as `news.exe.new` and
/// record its SHA-256 beside it, elevated, for `service.update` to verify before the swap.
pub fn stage(path: &Path, elevated: bool) -> Result<Value, NewsError> {
    if !cfg!(windows) {
        return Err(NewsError::precondition(
            "the Bureau service is a Windows service",
        ));
    }
    // Absolute, because the elevated copy starts in another folder.
    let source = std::fs::canonicalize(path)
        .map_err(|e| NewsError::usage(format!("cannot read {}: {e}", path.display())))?;
    if !source.is_file() {
        return Err(NewsError::usage(format!(
            "{} is not a file",
            source.display()
        )));
    }
    if !news_win::is_elevated() {
        let shown = source.to_string_lossy();
        if shown.contains('"') {
            return Err(NewsError::usage("the path has a quote"));
        }
        println!("Windows will ask for permission to continue as administrator.");
        let code = news_win::relaunch_elevated(
            &this_exe()?,
            &format!("service stage --elevated \"{shown}\""),
        )
        .map_err(|e| io("could not start the elevated stage", e))?;
        if code != 0 {
            return Err(NewsError::precondition(format!(
                "the elevated stage ended with code {code}; its window showed why"
            )));
        }
        let p = install_paths();
        let hash = std::fs::read_to_string(&p.staged_hash)
            .map(|s| s.trim().to_owned())
            .unwrap_or_default();
        return Ok(staged_reply(&p, &hash));
    }
    let result = stage_elevated(&source);
    pause_if(elevated, &result);
    result
}

fn staged_reply(p: &InstallPaths, hash: &str) -> Value {
    json!({"staged": p.staged, "sha256": hash,
        "text": format!("Staged {} (SHA-256 {hash}). Update the service from Board.", p.staged.display())})
}

fn stage_elevated(source: &Path) -> Result<Value, NewsError> {
    let p = news_win::install_paths().map_err(|e| io("cannot find the install folder", e))?;
    if !p.exe.is_file() {
        return Err(
            NewsError::precondition(format!("{} is not installed", p.exe.display()))
                .with_hint("run `news service install` first"),
        );
    }
    // Check the folder is still administrators-only by setting it again.
    news_win::make_admin_only(&p.dir, true)
        .map_err(|e| io(&format!("cannot restrict {}", p.dir.display()), e))?;
    match std::fs::remove_file(&p.staged_hash) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            return Err(io("cannot remove the old hash", e));
        }
        _ => {}
    }
    std::fs::copy(source, &p.staged)
        .map_err(|e| io(&format!("cannot copy to {}", p.staged.display()), e))?;
    news_win::make_admin_only(&p.staged, false)
        .map_err(|e| io(&format!("cannot restrict {}", p.staged.display()), e))?;
    // Hash the copy in the admin-only folder, not the source, which the user can still change.
    let hash = update::record_staged_hash(&p.staged)
        .map_err(|e| io("cannot record the staged build's hash", e))?;
    news_win::make_admin_only(&p.staged_hash, false)
        .map_err(|e| io(&format!("cannot restrict {}", p.staged_hash.display()), e))?;
    Ok(staged_reply(&p, &hash))
}

pub fn uninstall(elevated: bool) -> Result<Value, NewsError> {
    if !cfg!(windows) {
        return Err(NewsError::precondition(
            "the Bureau service is a Windows service",
        ));
    }
    if !news_win::is_elevated() {
        let code = news_win::relaunch_elevated(&this_exe()?, "service uninstall --elevated")
            .map_err(|e| io("could not start the elevated uninstall", e))?;
        if code != 0 {
            return Err(NewsError::precondition(format!(
                "the elevated uninstall ended with code {code}"
            )));
        }
        return Ok(json!({"uninstalled": true, "text": format!("{SERVICE_NAME} is removed.")}));
    }
    let r = news_win::scm::uninstall(SERVICE_NAME)
        .map_err(|e| io("cannot remove the service", e))
        .and_then(|()| {
            news_win::set_logon_entry(None).map_err(|e| io("cannot remove Board's log-on entry", e))
        })
        .map(|()| {
            // Best effort: a binary still running (this one, say) stays until it exits.
            let dir = install_paths().dir;
            let text = match std::fs::remove_dir_all(&dir) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => format!(
                    "{SERVICE_NAME} is removed. {} could not be deleted ({e}); delete it once \
                     nothing runs from it.",
                    dir.display()
                ),
                _ => format!("{SERVICE_NAME} is removed."),
            };
            json!({"uninstalled": true, "text": text})
        });
    pause_if(elevated, &r);
    r
}

pub fn start() -> Result<Value, NewsError> {
    news_win::scm::start(SERVICE_NAME).map_err(|e| io("cannot start the service", e))?;
    Ok(json!({"text": format!("{SERVICE_NAME} is starting.")}))
}

pub fn stop() -> Result<Value, NewsError> {
    news_win::scm::stop(SERVICE_NAME).map_err(|e| io("cannot stop the service", e))?;
    Ok(json!({"text": format!("{SERVICE_NAME} is stopping.")}))
}

/// The SCM's view, with a readable reason when the last stop was an error.
pub fn status() -> Result<Value, NewsError> {
    let st = news_win::scm::status(SERVICE_NAME).map_err(|e| io("cannot query the service", e))?;
    Ok(match st {
        None => {
            json!({"installed": false, "text": format!("{SERVICE_NAME} is not installed. Run `news service install`.")})
        }
        Some(s) => {
            let reason = s.exit_code.map(news_win::explain_exit_code);
            json!({"installed": true, "state": s.state, "running": s.running, "pid": s.pid,
            "last_error": reason,
            "text": match &reason {
                Some(r) if !s.running => format!("{SERVICE_NAME} is {}. Last error: {r}", s.state),
                _ => format!("{SERVICE_NAME} is {}.", s.state),
            }})
        }
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn the_account_is_local_unless_the_domain_differs_from_the_computer() {
        assert_eq!(account_for("Admin", "LAPTOP", "laptop"), r".\Admin");
        assert_eq!(account_for("Admin", "", "LAPTOP"), r".\Admin");
        assert_eq!(account_for("kp", "CORP", "LAPTOP"), r"CORP\kp");
    }

    #[test]
    fn the_dry_run_names_the_plan_and_never_a_password() {
        let s = spec(Path::new(r"C:\news\news.exe"), r".\Admin");
        let v = plan(
            &s,
            Path::new(r"C:\src\news\cli\target\release\news.exe"),
            None,
        );
        assert_eq!(s.args, ["service", "run"]);
        assert!(v["steps"].to_string().contains("repository root"));
        assert_eq!(v["account"], r".\Admin");
        assert_eq!(v["dry_run"], true);
        let text = v.to_string().to_lowercase();
        assert!(text.contains("log on as a service"));
        assert!(!text.contains("\"password\""));
        assert_eq!(
            board_command(Path::new(r"C:\news\news.exe")),
            "\"C:\\news\\news.exe\" board --minimized"
        );
    }

    #[test]
    fn a_dry_run_changes_nothing_and_needs_no_elevation() {
        let v = install(&InstallArgs {
            dry_run: true,
            account: Some(r".\Test".into()),
            ..InstallArgs::default()
        })
        .unwrap();
        assert!(v["text"].as_str().unwrap().contains("Nothing was changed"));
        // The service is registered at the admin-only install path, not where `install` ran from.
        let installed = install_paths().exe;
        assert!(installed.ends_with(Path::new("News").join("news.exe")));
        assert_eq!(v["binary"], json!(installed));
        assert_eq!(v["copy_from"], json!(std::env::current_exe().unwrap()));
        assert_ne!(v["binary"], v["copy_from"]);
    }

    #[test]
    fn an_account_that_could_break_the_command_line_is_refused_before_anything_runs() {
        for bad in ["Admin\" --sid \"S-1-5-18", "a\nb", ""] {
            let e = install(&InstallArgs {
                dry_run: true,
                account: Some(bad.into()),
                ..InstallArgs::default()
            })
            .unwrap_err();
            assert_eq!(e.code, news_core::ErrorCode::Usage, "{bad:?}");
        }
    }

    #[test]
    fn the_elevated_install_takes_only_a_user_sid_that_the_account_resolves_to() {
        for bad in ["S-1-5-18", "S-1-5-21-1-2-3-4)(A;;GA;;;WD)", "S-1-1-0"] {
            assert_eq!(
                check_account(r".\Admin", bad).unwrap_err().code,
                news_core::ErrorCode::Usage,
                "{bad}"
            );
        }
        assert!(check_account("a\"b", "S-1-5-21-1-2-3-1001").is_err());
        #[cfg(windows)]
        {
            let me = news_win::current_user_sid().unwrap();
            let user = std::env::var("USERNAME").unwrap();
            check_account(&format!(r".\{user}"), &me).unwrap();
            // Our own name with someone else's SID: refused.
            let e = check_account(&user, "S-1-5-21-1-2-3-1001").unwrap_err();
            assert_eq!(e.code, news_core::ErrorCode::Precondition, "{}", e.message);
        }
    }

    #[test]
    fn stage_needs_a_file() {
        if cfg!(windows) {
            let e = stage(Path::new("no-such-build-7f3a9c.exe"), false).unwrap_err();
            assert_eq!(e.code, news_core::ErrorCode::Usage);
        } else {
            assert!(stage(Path::new("x"), false).is_err());
        }
    }
}
