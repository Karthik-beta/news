//! The Hotline: a named pipe locked to one user, local clients only, with per-connection client
//! identification (SID by impersonation, image path, command line, AI-job membership).
//! Framing as ADR 0007: little-endian u32 length, then UTF-8 JSON, at most 1 MiB.

use std::ffi::c_void;
use std::io::{self, Read, Write};
use std::os::windows::io::AsRawHandle;
use std::sync::Arc;
use std::time::Instant;

use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use tokio::sync::watch;

use crate::win;

pub const DEFAULT_PIPE: &str = r"\\.\pipe\news-hotline-spike";
const MAX_MSG: u32 = 1 << 20;

pub struct Config {
    pub pipe: String,
    /// SDDL for the pipe. `D:P(A;;GA;;;<sid>)`: protected DACL, one ACE, full access to one SID.
    pub sddl: String,
    pub owner_sid: String,
    pub expected_image: String,
    /// The AI Job Object (spawn marker). Null when no job exists yet.
    pub ai_job: Option<Arc<win::Handle>>,
}

pub fn sddl_for(sid: &str) -> String {
    format!("D:P(A;;GA;;;{sid})")
}

fn create_instance(cfg: &Config, first: bool) -> io::Result<NamedPipeServer> {
    let sd = win::Sd::from_sddl(&cfg.sddl)?;
    let mut sa = sd.attributes();
    let mut opts = ServerOptions::new();
    opts.first_pipe_instance(first).reject_remote_clients(true);
    // SAFETY: `sa` and the descriptor it points to outlive the call; the kernel copies them.
    unsafe {
        let p: *mut windows_sys::Win32::Security::SECURITY_ATTRIBUTES = &mut sa;
        opts.create_with_security_attributes_raw(&cfg.pipe, p.cast::<c_void>())
    }
}

/// Serve until `shutdown` turns true. Logs one JSON line per decision on stdout (or the log file
/// in service mode, via `log`).
pub async fn serve(
    cfg: Arc<Config>,
    mut shutdown: watch::Receiver<bool>,
    log: Arc<dyn Fn(String) + Send + Sync>,
) -> io::Result<()> {
    let mut server = create_instance(&cfg, true)?;
    let sddl = win::object_sddl(server.as_raw_handle() as _)
        .unwrap_or_else(|e| format!("<GetSecurityInfo failed: {e}>"));
    log(json!({"event": "listening", "pipe": cfg.pipe, "pid": std::process::id(), "sddl_requested": cfg.sddl, "sddl_actual": sddl}).to_string());
    loop {
        tokio::select! {
            r = server.connect() => r?,
            _ = shutdown.changed() => { log(json!({"event":"stopping"}).to_string()); return Ok(()); }
        }
        let conn = server;
        server = create_instance(&cfg, false)?;
        let cfg = cfg.clone();
        let log = log.clone();
        tokio::spawn(async move {
            if let Err(e) = handle(conn, &cfg, &*log).await {
                log(json!({"event": "conn_error", "error": e.to_string()}).to_string());
            }
        });
    }
}

struct Client {
    pid: u32,
    image: String,
    cmdline: String,
    in_ai_job: bool,
    in_any_job: bool,
}

fn identify(pipe: *mut c_void, cfg: &Config) -> io::Result<Client> {
    let pid = win::pipe_client_pid(pipe)?;
    let p = win::open_process_query(pid)?;
    let image = win::image_path(p.0)?;
    let cmdline = win::command_line(p.0).unwrap_or_else(|e| format!("<{e}>"));
    let in_ai_job = match &cfg.ai_job {
        Some(j) => win::in_job(p.0, j.0)?,
        None => false,
    };
    let in_any_job = win::in_job(p.0, std::ptr::null_mut())?;
    Ok(Client { pid, image, cmdline, in_ai_job, in_any_job })
}

fn tier(cmd: &str) -> &'static str {
    match cmd {
        "status" | "run.show" | "lineup.list" | "standing.get" | "service.log" | "docker.status" => "R",
        _ => "H",
    }
}

async fn handle(mut conn: NamedPipeServer, cfg: &Config, log: &(dyn Fn(String) + Send + Sync)) -> io::Result<()> {
    let t0 = Instant::now();
    let raw = conn.as_raw_handle() as usize; // a handle value, so the future stays Send
    let client = identify(raw as *mut c_void, cfg)?;
    let len = conn.read_u32_le().await?;
    if len > MAX_MSG {
        return Err(io::Error::other("message too large"));
    }
    let mut body = vec![0u8; len as usize];
    conn.read_exact(&mut body).await?;
    // Impersonation is per thread: do it synchronously, with no await in between.
    let client_sid = win::pipe_client_sid(raw as *mut c_void);
    let req: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let id = req.get("id").cloned().unwrap_or(Value::Null);
    let cmd = req.get("cmd").and_then(Value::as_str).unwrap_or("").to_string();
    let argv = win::argv(&client.cmdline);
    let is_board = argv.get(1).map(String::as_str) == Some("board");
    let image_ok = client.image.eq_ignore_ascii_case(&cfg.expected_image);
    let t = tier(&cmd);
    let (sid_str, same_user) = match &client_sid {
        Ok(s) => (s.clone(), *s == cfg.owner_sid),
        Err(e) => (format!("<impersonation failed: {e}>"), false),
    };
    let decision = if !same_user {
        "refused:not_owner"
    } else if t == "R" {
        "ok"
    } else if client.in_ai_job {
        "gated:ai_job"
    } else if !image_ok {
        "gated:not_installed_image"
    } else if !is_board {
        "gated:not_board"
    } else {
        "ok"
    };
    let facts = json!({
        "client_pid": client.pid, "client_image": client.image, "client_cmdline": client.cmdline,
        "client_sid": sid_str, "same_user": same_user, "in_ai_job": client.in_ai_job,
        "in_any_job": client.in_any_job, "image_ok": image_ok, "is_board": is_board,
        "cmd": cmd, "tier": t, "decision": decision, "check_us": t0.elapsed().as_micros() as u64,
    });
    log(json!({"event": "request", "facts": facts}).to_string());
    let reply = if decision == "ok" {
        json!({"id": id, "ok": true, "data": {"cmd": cmd, "facts": facts}})
    } else {
        json!({"id": id, "ok": false, "error": {"code": "gated", "reason": decision}, "data": {"facts": facts}})
    };
    let out = serde_json::to_vec(&reply).map_err(io::Error::other)?;
    conn.write_u32_le(out.len() as u32).await?;
    conn.write_all(&out).await?;
    conn.flush().await?;
    if decision.starts_with("refused") {
        conn.disconnect()?;
    }
    Ok(())
}

/// A blocking Hotline client, as Board and the CLI would use. Opens the pipe at identification
/// level only (the server can read our SID but cannot act as us), then checks who the server is.
pub fn request(pipe: &str, cmd: &str) -> io::Result<Value> {
    use std::os::windows::fs::OpenOptionsExt;
    const SECURITY_IDENTIFICATION: u32 = 1 << 16; // SecurityIdentification << 16
    let t0 = Instant::now();
    let mut f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .security_qos_flags(SECURITY_IDENTIFICATION)
        .open(pipe)?;
    let server_pid = win::pipe_server_pid(f.as_raw_handle() as _)?;
    let server_image = win::open_process_query(server_pid)
        .and_then(|p| win::image_path(p.0))
        .unwrap_or_else(|e| format!("<{e}>"));
    let req = json!({"v": 1, "id": format!("c{}", std::process::id()), "cmd": cmd, "args": {}});
    let body = serde_json::to_vec(&req).map_err(io::Error::other)?;
    f.write_all(&(body.len() as u32).to_le_bytes())?;
    f.write_all(&body)?;
    let mut lb = [0u8; 4];
    f.read_exact(&mut lb)?;
    let mut rb = vec![0u8; u32::from_le_bytes(lb) as usize];
    f.read_exact(&mut rb)?;
    let mut reply: Value = serde_json::from_slice(&rb).map_err(io::Error::other)?;
    reply["client_side"] = json!({"server_pid": server_pid, "server_image": server_image, "round_trip_us": t0.elapsed().as_micros() as u64});
    Ok(reply)
}
