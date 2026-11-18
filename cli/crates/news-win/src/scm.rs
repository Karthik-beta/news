//! The Service Control Manager side of the Bureau service (ADR 0007 decision 12, spike item 1):
//! install under the user's own account, restart on failure, start and stop, the entry point the SCM
//! calls, and the service's access list so Karthik can start it from Board without elevation.

use crate::ServiceSpec;
use std::ffi::OsString;
use std::io;
use std::sync::OnceLock;
use std::time::Duration;
use windows_service::service::{
    ServiceAccess, ServiceAction, ServiceActionType, ServiceControl, ServiceControlAccept,
    ServiceErrorControl, ServiceExitCode, ServiceFailureActions, ServiceFailureResetPeriod,
    ServiceInfo, ServiceStartType, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
use windows_service::{define_windows_service, service_dispatcher};

fn io_err(e: windows_service::Error) -> io::Error {
    match e {
        windows_service::Error::Winapi(io) => io,
        other => io::Error::other(other.to_string()),
    }
}

fn manager(access: ServiceManagerAccess) -> io::Result<ServiceManager> {
    ServiceManager::local_computer(None::<&str>, access).map_err(io_err)
}

/// Create the service, or, if it exists, update its configuration (so `install` also changes the
/// password after Karthik changes his Windows password). Delayed automatic start; restart after 5 s,
/// then 60 s, also when the service exits with an error code of its own (the update restart).
pub fn install(spec: &ServiceSpec, password: Option<&str>) -> io::Result<()> {
    let m = manager(ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE)?;
    let info = ServiceInfo {
        name: spec.name.clone().into(),
        display_name: spec.display_name.clone().into(),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: spec.exe.clone(),
        launch_arguments: spec.args.iter().map(OsString::from).collect(),
        dependencies: vec![],
        account_name: Some(spec.account.clone().into()),
        account_password: password.map(OsString::from),
    };
    let access = ServiceAccess::CHANGE_CONFIG | ServiceAccess::QUERY_STATUS;
    let svc = match m.open_service(&spec.name, access) {
        Ok(s) => {
            s.change_config(&info).map_err(io_err)?;
            s
        }
        Err(_) => m.create_service(&info, access).map_err(io_err)?,
    };
    svc.set_description(&spec.description).map_err(io_err)?;
    svc.set_delayed_auto_start(true).map_err(io_err)?;
    svc.update_failure_actions(ServiceFailureActions {
        reset_period: ServiceFailureResetPeriod::After(Duration::from_secs(24 * 3600)),
        reboot_msg: None,
        command: None,
        actions: Some(vec![
            restart(Duration::from_secs(5)),
            restart(Duration::from_secs(60)),
            restart(Duration::from_secs(60)),
        ]),
    })
    .map_err(io_err)?;
    svc.set_failure_actions_on_non_crash_failures(true)
        .map_err(io_err)?;
    Ok(())
}

fn restart(delay: Duration) -> ServiceAction {
    ServiceAction {
        action_type: ServiceActionType::Restart,
        delay,
    }
}

/// Stop the service if it runs, wait for it, and delete it.
pub fn uninstall(name: &str) -> io::Result<()> {
    let m = manager(ServiceManagerAccess::CONNECT)?;
    let s = m
        .open_service(
            name,
            ServiceAccess::QUERY_STATUS | ServiceAccess::STOP | ServiceAccess::DELETE,
        )
        .map_err(io_err)?;
    if s.query_status().map_err(io_err)?.current_state != ServiceState::Stopped {
        let _ = s.stop();
        for _ in 0..100 {
            if s.query_status().map_err(io_err)?.current_state == ServiceState::Stopped {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    s.delete().map_err(io_err)
}

pub fn start(name: &str) -> io::Result<()> {
    let m = manager(ServiceManagerAccess::CONNECT)?;
    let s = m.open_service(name, ServiceAccess::START).map_err(io_err)?;
    s.start(&[] as &[&std::ffi::OsStr]).map_err(io_err)
}

pub fn stop(name: &str) -> io::Result<()> {
    let m = manager(ServiceManagerAccess::CONNECT)?;
    let s = m.open_service(name, ServiceAccess::STOP).map_err(io_err)?;
    s.stop().map(|_| ()).map_err(io_err)
}

/// The service's state, or `None` when it is not installed.
pub fn status(name: &str) -> io::Result<Option<crate::ScmStatus>> {
    let m = manager(ServiceManagerAccess::CONNECT)?;
    let s = match m.open_service(name, ServiceAccess::QUERY_STATUS) {
        Ok(s) => s,
        Err(windows_service::Error::Winapi(e)) if e.raw_os_error() == Some(1060) => {
            return Ok(None);
        }
        Err(e) => return Err(io_err(e)),
    };
    let st = s.query_status().map_err(io_err)?;
    let exit_code = match st.exit_code {
        ServiceExitCode::Win32(0) | ServiceExitCode::ServiceSpecific(0) => None,
        ServiceExitCode::Win32(c) => Some(c),
        ServiceExitCode::ServiceSpecific(c) => Some(c),
    };
    Ok(Some(crate::ScmStatus {
        state: format!("{:?}", st.current_state).to_lowercase(),
        running: st.current_state == ServiceState::Running,
        exit_code,
        pid: st.process_id,
    }))
}

// ---- The entry point the SCM calls ---------------------------------------------------------------

type Body = fn(tokio::sync::watch::Receiver<bool>) -> u32;

static BODY: OnceLock<(String, Body)> = OnceLock::new();

define_windows_service!(ffi_service_main, service_main);

fn service_main(_args: Vec<OsString>) {
    let Some((name, body)) = BODY.get() else {
        return;
    };
    let (tx, rx) = tokio::sync::watch::channel(false);
    let handler = move |c| match c {
        ServiceControl::Stop | ServiceControl::Shutdown => {
            let _ = tx.send(true);
            ServiceControlHandlerResult::NoError
        }
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    };
    let Ok(h) = service_control_handler::register(name, handler) else {
        return;
    };
    let status = |state, accept, code| ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: state,
        controls_accepted: accept,
        exit_code: code,
        checkpoint: 0,
        wait_hint: Duration::from_secs(20),
        process_id: None,
    };
    let _ = h.set_service_status(status(
        ServiceState::Running,
        ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
        ServiceExitCode::Win32(0),
    ));
    let code = body(rx);
    let exit = if code == 0 {
        ServiceExitCode::Win32(0)
    } else {
        ServiceExitCode::ServiceSpecific(code)
    };
    let _ = h.set_service_status(status(
        ServiceState::Stopped,
        ServiceControlAccept::empty(),
        exit,
    ));
}

/// Hand this process to the SCM. `body` runs the service until the receiver turns `true` and returns
/// the exit code (0 for a clean stop, non-zero to be restarted by the failure actions). Fails with
/// error 1063 when the process was not started by the SCM (a terminal): the caller then runs the
/// service in console mode.
pub fn dispatch(name: &str, body: Body) -> io::Result<()> {
    let _ = BODY.set((name.to_owned(), body));
    service_dispatcher::start(name, ffi_service_main).map_err(io_err)
}

/// Error 1063: this process was not started by the SCM.
pub const NOT_STARTED_BY_SCM: i32 = 1063;
