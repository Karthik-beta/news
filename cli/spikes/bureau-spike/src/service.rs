//! Windows service lifecycle with the `windows-service` crate. The service body is the Hotline
//! server from `hotline.rs`; it logs to %ProgramData%\news-bureau-spike\service.log.

use std::ffi::OsString;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

use windows_service::service::{
    ServiceAccess, ServiceControl, ServiceControlAccept, ServiceErrorControl, ServiceExitCode,
    ServiceInfo, ServiceStartType, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
use windows_service::{define_windows_service, service_dispatcher};

use crate::{arg, hotline, server_config, win};

const DEFAULT_NAME: &str = "NewsBureauSpike";

fn err(e: windows_service::Error) -> String {
    match &e {
        windows_service::Error::Winapi(io) => format!("{e} (os error {:?})", io.raw_os_error()),
        _ => format!("{e:?}"),
    }
}

pub fn cli(args: &[String]) -> i32 {
    let name = arg(args, "--name").unwrap_or_else(|| DEFAULT_NAME.into());
    let r: Result<String, String> = match args.get(2).map(String::as_str) {
        Some("install") => install(&name, arg(args, "--grant-sid")),
        Some("uninstall") => uninstall(&name),
        Some("start") => with_service(&name, ServiceAccess::START, |s| {
            s.start(&[] as &[&std::ffi::OsStr]).map(|_| "start requested".into())
        }),
        Some("stop") => with_service(&name, ServiceAccess::STOP, |s| {
            s.stop().map(|st| format!("{:?}", st.current_state))
        }),
        Some("status") => with_service(&name, ServiceAccess::QUERY_STATUS, |s| {
            s.query_status().map(|st| format!("{:?} pid={:?}", st.current_state, st.process_id))
        }),
        Some("run") => {
            // Fails with ERROR_FAILED_SERVICE_CONTROLLER_CONNECT (1063) when not started by the SCM:
            // that is how `news service run` can tell it was started from a terminal.
            match service_dispatcher::start(&name, ffi_service_main) {
                Ok(()) => Ok("dispatcher returned".into()),
                Err(e) => Err(err(e)),
            }
        }
        _ => Err("service install|uninstall|start|stop|status|run".into()),
    };
    match r {
        Ok(s) => {
            println!("{s}");
            0
        }
        Err(e) => {
            println!("error: {e}");
            1
        }
    }
}

fn with_service<T>(
    name: &str,
    access: ServiceAccess,
    f: impl FnOnce(&windows_service::service::Service) -> windows_service::Result<T>,
) -> Result<T, String> {
    let m = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT).map_err(err)?;
    let s = m.open_service(name, access).map_err(err)?;
    f(&s).map_err(err)
}

/// Throwaway install under LocalService. Installing as Karthik's account differs only in
/// `account_name` (".\\<user>") and `account_password` (typed by him), plus granting
/// SeServiceLogonRight to his account (see the report).
fn install(name: &str, grant_sid: Option<String>) -> Result<String, String> {
    let m = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
    )
    .map_err(err)?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut launch = vec![OsString::from("service"), OsString::from("run"), OsString::from("--name"), OsString::from(name)];
    if let Some(sid) = grant_sid {
        launch.push("--grant-sid".into());
        launch.push(sid.into());
    }
    let info = ServiceInfo {
        name: name.into(),
        display_name: "News Bureau (spike)".into(),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::OnDemand,
        error_control: ServiceErrorControl::Normal,
        executable_path: exe,
        launch_arguments: launch,
        dependencies: vec![],
        account_name: Some(r"NT AUTHORITY\LocalService".into()),
        account_password: None,
    };
    let s = m.create_service(&info, ServiceAccess::CHANGE_CONFIG).map_err(err)?;
    s.set_description("Throwaway spike service for ADR 0007; safe to delete.").map_err(err)?;
    Ok(format!("installed {name}"))
}

fn uninstall(name: &str) -> Result<String, String> {
    let m = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT).map_err(err)?;
    let s = m
        .open_service(name, ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE)
        .map_err(err)?;
    if s.query_status().map_err(err)?.current_state != ServiceState::Stopped {
        let _ = s.stop();
        for _ in 0..50 {
            if s.query_status().map_err(err)?.current_state == ServiceState::Stopped {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    s.delete().map_err(err)?;
    Ok(format!("deleted {name}"))
}

define_windows_service!(ffi_service_main, service_main);

fn log_path() -> std::path::PathBuf {
    let base = std::env::var_os("ProgramData").unwrap_or_else(|| r"C:\ProgramData".into());
    std::path::Path::new(&base).join("news-bureau-spike").join("service.log")
}

fn service_log(line: &str) {
    let p = log_path();
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
        let _ = writeln!(f, "{line}");
    }
}

fn service_main(_args: Vec<OsString>) {
    if let Err(e) = run_service() {
        service_log(&format!("service error: {e}"));
    }
}

fn run_service() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let (tx, rx) = tokio::sync::watch::channel(false);
    let handler = move |c| match c {
        ServiceControl::Stop | ServiceControl::Shutdown => {
            let _ = tx.send(true);
            ServiceControlHandlerResult::NoError
        }
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    };
    let name = arg(&args, "--name").unwrap_or_else(|| DEFAULT_NAME.into());
    let h = service_control_handler::register(&name, handler).map_err(err)?;
    let status = |state, accept| ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: state,
        controls_accepted: accept,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::from_secs(5),
        process_id: None,
    };
    h.set_service_status(status(ServiceState::Running, ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN))
        .map_err(err)?;
    // What a real install needs to know: who we are and which environment the SCM gave us.
    let env_names: Vec<String> = std::env::vars_os().map(|(k, _)| k.to_string_lossy().into_owned()).collect();
    service_log(&serde_json::json!({
        "event": "service_started", "pid": std::process::id(), "sid": win::current_user_sid().ok(),
        "userprofile": std::env::var("USERPROFILE").ok(), "env_names": env_names,
        "path": std::env::var("PATH").ok(),
    }).to_string());
    let cfg = Arc::new(server_config(hotline::DEFAULT_PIPE, arg(&args, "--grant-sid"), None));
    let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    let log: Arc<dyn Fn(String) + Send + Sync> = Arc::new(|l| service_log(&l));
    let r = rt.block_on(hotline::serve(cfg, rx, log));
    service_log(&format!("serve ended: {r:?}"));
    h.set_service_status(status(ServiceState::Stopped, ServiceControlAccept::empty())).map_err(err)?;
    Ok(())
}
