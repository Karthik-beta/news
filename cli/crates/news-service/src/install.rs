//! `news service install|uninstall|start|stop|status` (ADR 0007 decision 12, spike item 1).
//!
//! Karthik runs `install` in his own terminal. Windows needs an administrator token to create a
//! service, so a terminal without one re-launches `news.exe` through the UAC prompt; the elevated copy
//! asks for his Windows password at a no-echo prompt (the agent never handles it), grants his account
//! the "Log on as a service" right, registers the service under his own account with a delayed
//! automatic start and restart on failure, lets his account start and stop it (so Board's "Start
//! service" needs no elevation), and adds Board's log-on entry. `--dry-run` prints the plan and
//! changes nothing. Running `install` again updates the registration, which is how he changes the
//! stored password after changing his Windows password.

use news_core::NewsError;
use news_win::ServiceSpec;
use serde_json::{Value, json};
use std::path::Path;

/// The service's name in the SCM.
pub const SERVICE_NAME: &str = news_win::SERVICE_NAME;

pub const DISPLAY_NAME: &str = "News Bureau";

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
pub fn plan(spec: &ServiceSpec) -> Value {
    json!({
        "dry_run": true,
        "service": spec.name,
        "display_name": spec.display_name,
        "binary": spec.exe,
        "arguments": spec.args.join(" "),
        "account": spec.account,
        "start": "automatic (delayed)",
        "on_failure": "restart after 5 s, then 60 s, then 60 s; reset after a day",
        "also_restarts_after": "an update (the service exits with a code of its own)",
        "steps": [
            "re-launch elevated (Windows asks for permission) unless already elevated",
            "ask for the account's Windows password at a prompt that does not echo; it goes to the SCM only",
            "grant the account the 'Log on as a service' right",
            "create the service, or update it when it exists (this is how the password is changed)",
            "let the account query, start and stop the service without elevation",
            "add Board's log-on entry for the account: board --minimized",
        ],
        "text": format!(
            "Would register service {} under {} (delayed automatic start, restart on failure) and add \
             Board's log-on entry. Nothing was changed.",
            spec.name, spec.account
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
    /// The invoking user's SID, passed to the elevated copy (which may run under another token).
    pub sid: Option<String>,
}

fn io(what: &str, e: std::io::Error) -> NewsError {
    let hint = match e.raw_os_error() {
        Some(5) => " (access denied: run it again and accept the administrator prompt)",
        Some(1069 | 1326) => " (the account name or password was rejected)",
        _ => "",
    };
    NewsError::precondition(format!("{what}: {e}{hint}"))
}

pub fn install(a: &InstallArgs) -> Result<Value, NewsError> {
    let exe = std::env::current_exe()
        .map_err(|e| NewsError::internal(format!("cannot find news.exe: {e}")))?;
    let account = a
        .account
        .clone()
        .or_else(default_account)
        .ok_or_else(|| NewsError::usage("cannot tell the account name; pass --account"))?;
    let spec = spec(&exe, &account);
    if a.dry_run {
        return Ok(plan(&spec));
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
        println!("Windows will ask for permission to continue as administrator.");
        let args = format!("service install --elevated --account \"{account}\" --sid \"{sid}\"");
        let code = news_win::relaunch_elevated(&exe, &args)
            .map_err(|e| io("could not start the elevated install", e))?;
        if code != 0 {
            return Err(NewsError::precondition(format!(
                "the elevated install ended with code {code}; its window showed why"
            )));
        }
        return Ok(
            json!({"installed": true, "service": SERVICE_NAME, "account": account,
            "text": format!("{SERVICE_NAME} is installed under {account}. Start it from Board, or with `news service start`.")}),
        );
    }
    let result = install_elevated(&spec, &sid, &exe);
    if a.elevated {
        match &result {
            Ok(_) => println!("\nDone. Press Enter to close this window."),
            Err(e) => println!("\nFailed: {}\nPress Enter to close this window.", e.message),
        }
        let mut s = String::new();
        let _ = std::io::stdin().read_line(&mut s);
    }
    result
}

fn install_elevated(spec: &ServiceSpec, sid: &str, exe: &Path) -> Result<Value, NewsError> {
    let mut password = news_win::read_password(&format!(
        "Windows password for {} (typed here; News never stores it): ",
        spec.account
    ))
    .map_err(|e| io("cannot read the password", e))?;
    if password.is_empty() {
        return Err(NewsError::usage(
            "the password is empty; a service cannot log on without one (turn off \"only allow \
             Windows Hello sign-in\" in Settings, Accounts, Sign-in options, for the install)",
        ));
    }
    let done = (|| {
        news_win::grant_service_logon(sid)
            .map_err(|e| io("cannot grant 'Log on as a service'", e))?;
        news_win::scm::install(spec, Some(&password))
            .map_err(|e| io("cannot register the service", e))?;
        news_win::allow_user_to_control(&spec.name, sid)
            .map_err(|e| io("cannot set the service's access list", e))?;
        news_win::set_logon_entry(Some(&board_command(exe)))
            .map_err(|e| io("cannot add Board's log-on entry", e))
    })();
    // Best effort: overwrite the buffer before it is freed.
    wipe(&mut password);
    done?;
    Ok(
        json!({"installed": true, "service": spec.name, "account": spec.account,
        "text": format!("{} is installed under {}. Start it from Board, or with `news service start`.",
                        spec.name, spec.account)}),
    )
}

/// Overwrite a string's bytes with zeros (no `unsafe` here, so go through a Vec).
fn wipe(s: &mut String) {
    let mut bytes = std::mem::take(s).into_bytes();
    bytes.fill(0);
    drop(bytes);
}

pub fn uninstall(elevated: bool) -> Result<Value, NewsError> {
    if !cfg!(windows) {
        return Err(NewsError::precondition(
            "the Bureau service is a Windows service",
        ));
    }
    if !news_win::is_elevated() {
        let exe = std::env::current_exe()
            .map_err(|e| NewsError::internal(format!("cannot find news.exe: {e}")))?;
        let code = news_win::relaunch_elevated(&exe, "service uninstall --elevated")
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
        });
    if elevated {
        println!(
            "\n{}\nPress Enter to close this window.",
            match &r {
                Ok(()) => "Removed.".to_owned(),
                Err(e) => format!("Failed: {}", e.message),
            }
        );
        let mut s = String::new();
        let _ = std::io::stdin().read_line(&mut s);
    }
    r?;
    Ok(json!({"uninstalled": true, "text": format!("{SERVICE_NAME} is removed.")}))
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
        let v = plan(&s);
        assert_eq!(s.args, ["service", "run"]);
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
    }
}
