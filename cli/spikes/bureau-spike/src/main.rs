//! bureau-spike: proofs for ADR 0007 (docs/spikes/bureau-service.md).
//!
//!   console [--pipe P] [--grant-sid SID] [--for SECS] [--spawn job-demo|wmi-escape|claude-ping|claude-tool]
//!   service install|uninstall|start|stop|status [--name N] [--grant-sid SID]
//!   service run                       (entry point the SCM calls)
//!   client --cmd C [--pipe P]         (the terminal CLI as a Hotline client)
//!   board  --cmd C [--pipe P] [--out F]   (simulates Board: argv[1] == "board")
//!   job-child [--pipe P] [--wmi-escape]   (runs inside the AI job)
//!   job-check [--pipe P]              (prints job membership, then tries R and H commands)
//!   probe --dotenv PATH               (fresh-logon env, PostgreSQL, ClickHouse, Docker)

mod hotline;
mod job;
mod probe;
mod service;
mod win;

use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::json;

pub fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}
pub fn flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let pipe = arg(&args, "--pipe").unwrap_or_else(|| hotline::DEFAULT_PIPE.to_string());
    let code = match args.get(1).map(String::as_str) {
        Some("console") => console(&args, &pipe),
        Some("service") => service::cli(&args),
        Some("client") | Some("board") => client(&args, &pipe),
        Some("job-child") => job_child(&args, &pipe),
        Some("job-check") => job_check(&pipe),
        Some("probe") => probe::run(&args),
        Some("low-check") => low_check(&args, &pipe),
        Some("claude-run") => claude_run(&args, &pipe),
        _ => {
            eprintln!("usage: see the top of src/main.rs");
            2
        }
    };
    std::process::exit(code);
}

fn client(args: &[String], pipe: &str) -> i32 {
    let cmd = arg(args, "--cmd").unwrap_or_else(|| "status".into());
    let out = match hotline::request(pipe, &cmd) {
        Ok(v) => v.to_string(),
        Err(e) => json!({"client_error": e.to_string(), "os_error": e.raw_os_error()}).to_string(),
    };
    println!("{out}");
    if let Some(f) = arg(args, "--out") {
        let _ = std::fs::write(f, &out);
    }
    0
}

pub fn server_config(pipe: &str, grant_sid: Option<String>, ai_job: Option<Arc<win::Handle>>) -> hotline::Config {
    // In production the granted SID is the service's own (Karthik's). The override exists for the
    // refusal test and for the LocalService lifecycle test, where the client is not the service.
    let grant = grant_sid.unwrap_or_else(|| win::current_user_sid().unwrap_or_default());
    hotline::Config {
        pipe: pipe.to_string(),
        sddl: hotline::sddl_for(&grant),
        owner_sid: grant,
        expected_image: std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default(),
        ai_job,
    }
}

/// Console mode: the service body without the SCM.
fn console(args: &[String], pipe: &str) -> i32 {
    let job = match win::create_ai_job() {
        Ok(j) => Arc::new(j),
        Err(e) => {
            eprintln!("create job: {e}");
            return 1;
        }
    };
    let cfg = Arc::new(server_config(pipe, arg(args, "--grant-sid"), Some(job.clone())));
    let secs: u64 = arg(args, "--for").and_then(|s| s.parse().ok()).unwrap_or(30);
    let spawn = arg(args, "--spawn");
    let rt = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let (tx, rx) = tokio::sync::watch::channel(false);
    let log: Arc<dyn Fn(String) + Send + Sync> = Arc::new(|l| println!("[server] {l}"));
    let pipe_s = pipe.to_string();
    let job2 = job.clone();
    let log2 = log.clone();
    rt.spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        if let Some(kind) = spawn {
            let r = tokio::task::spawn_blocking(move || run_scenario(&kind, &job2, &pipe_s, &*log2)).await;
            if let Err(e) = r {
                eprintln!("scenario panicked: {e}");
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
            let _ = tx.send(true);
        } else {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(secs)) => {},
                _ = tokio::signal::ctrl_c() => {},
            }
            let _ = tx.send(true);
        }
    });
    match rt.block_on(hotline::serve(cfg, rx, log)) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("serve: {e} (os error {:?})", e.raw_os_error());
            1
        }
    }
}

fn run_scenario(kind: &str, job: &win::Handle, pipe: &str, log: &(dyn Fn(String) + Send + Sync)) {
    let exe = std::env::current_exe().unwrap_or_default();
    match kind {
        "job-demo" | "wmi-escape" => {
            let mut c = Command::new(&exe);
            c.args(["job-child", "--pipe", pipe]);
            if kind == "wmi-escape" {
                c.arg("--wmi-escape");
            }
            c.stdout(std::process::Stdio::piped());
            match job::spawn_in_job(job, c) {
                Ok((child, dt)) => {
                    log(json!({"event":"spawned_in_job","pid":child.id(),"suspend_assign_resume_us":dt.as_micros() as u64}).to_string());
                    match child.wait_with_output() {
                        Ok(o) => print!("{}", String::from_utf8_lossy(&o.stdout)),
                        Err(e) => log(format!("wait: {e}")),
                    }
                }
                Err(e) => log(format!("spawn_in_job: {e}")),
            }
        }
        "claude-ping" | "claude-tool" => claude_scenario(kind == "claude-tool", job, pipe, log),
        "low-il" => low_scenario("low-check", job, pipe, log),
        "claude-ping-low" => low_scenario("claude-run", job, pipe, log),
        other => log(format!("unknown scenario {other}")),
    }
}

/// The `claude -p` command Ghost would run: allow-listed fresh-logon environment, absolute
/// claude.exe, no project context (cwd is a temp dir). `tool` lets the call use Bash once.
fn claude_command(tool: bool, pipe: &str, env: &[(String, String)], cwd: &std::path::Path) -> Result<Command, String> {
    let path = env.iter().find(|(k, _)| k.eq_ignore_ascii_case("path")).map(|(_, v)| v.clone()).unwrap_or_default();
    let claude = job::find_claude(&path).ok_or("claude.exe not found on the fresh-logon PATH")?;
    let _ = std::fs::create_dir_all(cwd);
    let exe = std::env::current_exe().unwrap_or_default();
    let mut c = Command::new(&claude);
    c.env_clear().envs(env.iter().map(|(k, v)| (k, v)));
    c.env("CLAUDE_CODE_DISABLE_AUTO_MEMORY", "1").env("NEWS_SPAWNED_BY", "spike:1");
    c.current_dir(cwd);
    c.args(["-p", "--model", "haiku", "--strict-mcp-config", "--mcp-config", r#"{"mcpServers":{}}"#,
        "--disable-slash-commands", "--no-session-persistence", "--output-format", "json"]);
    let prompt = if tool {
        c.args(["--tools", "Bash", "--allowedTools", "Bash", "--max-turns", "4"]);
        // No --pipe: Git Bash (MSYS) rewrites `\\.\pipe\…` arguments, so job-check uses the default.
        let _ = pipe;
        format!(
            "Use the Bash tool exactly once to run this command, then reply with its full output verbatim and nothing else:\n'{}' job-check",
            exe.display().to_string().replace('\\', "/"),
        )
    } else {
        c.args(["--tools", "", "--max-turns", "1"]);
        "Reply with exactly the word pong and nothing else.".to_string()
    };
    c.arg(prompt);
    c.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    Ok(c)
}

fn summarize(o: &std::process::Output, wall: Duration) -> serde_json::Value {
    let stdout = String::from_utf8_lossy(&o.stdout);
    let v: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap_or(json!({"raw": stdout.chars().take(2000).collect::<String>()}));
    json!({
        "exit": o.status.code(), "wall_ms": wall.as_millis() as u64,
        "is_error": v.get("is_error"), "subtype": v.get("subtype"), "result": v.get("result"),
        "num_turns": v.get("num_turns"), "duration_ms": v.get("duration_ms"),
        "duration_api_ms": v.get("duration_api_ms"), "usage": v.get("usage"),
        "total_cost_usd": v.get("total_cost_usd"), "models": v.get("modelUsage").and_then(|m| m.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>())),
        "raw_if_unparsed": v.get("raw"),
        "stderr": String::from_utf8_lossy(&o.stderr).chars().take(1500).collect::<String>(),
    })
}

fn claude_scenario(tool: bool, job: &win::Handle, pipe: &str, log: &(dyn Fn(String) + Send + Sync)) {
    let env = match job::allow_listed_env() {
        Ok(e) => e,
        Err(e) => return log(format!("env: {e}")),
    };
    let cwd = std::env::temp_dir().join("bureau-spike-claude");
    let c = match claude_command(tool, pipe, &env, &cwd) {
        Ok(c) => c,
        Err(e) => return log(e),
    };
    log(json!({"event":"claude_spawn","claude":c.get_program().to_string_lossy(),"env_names":env.iter().map(|(k,_)|k.clone()).collect::<Vec<_>>()}).to_string());
    let t = Instant::now();
    match job::spawn_in_job(job, c) {
        Ok((child, _)) => match child.wait_with_output() {
            Ok(o) => log(json!({"event":"claude_done","out":summarize(&o, t.elapsed())}).to_string()),
            Err(e) => log(format!("claude wait: {e}")),
        },
        Err(e) => log(format!("claude spawn: {e}")),
    }
}

fn low_dir() -> std::path::PathBuf {
    let base = std::env::var_os("USERPROFILE").unwrap_or_default();
    std::path::Path::new(&base).join("AppData").join("LocalLow").join("bureau-spike")
}

/// Start `<exe> <sub> ...` at Low integrity inside the AI job, then print what it wrote.
fn low_scenario(sub: &str, job: &win::Handle, pipe: &str, log: &(dyn Fn(String) + Send + Sync)) {
    let dir = low_dir();
    let _ = std::fs::create_dir_all(&dir);
    let out = dir.join(format!("{sub}.txt"));
    let _ = std::fs::remove_file(&out);
    let exe = std::env::current_exe().unwrap_or_default();
    let env = job::allow_listed_env().unwrap_or_default();
    let cl = format!("\"{}\" {sub} --pipe {pipe} --out \"{}\"", exe.display(), out.display());
    let t = Instant::now();
    let r = win::run_low_integrity(&cl, &env, &dir.display().to_string(), Some(job.0));
    log(json!({"event":"low_il_done","sub":sub,"exit":format!("{r:?}"),"wall_ms":t.elapsed().as_millis() as u64}).to_string());
    println!("{}", std::fs::read_to_string(&out).unwrap_or_else(|e| format!("<no output file: {e}>")));
}

/// Runs at Low integrity: can it reach the Hotline, escape through WMI, write the user's files?
fn low_check(args: &[String], pipe: &str) -> i32 {
    let out = arg(args, "--out").unwrap_or_default();
    let exe = std::env::current_exe().unwrap_or_default();
    let mut lines = vec![format!("[low-check] pid={}", std::process::id())];
    lines.push(format!("[low-check] hotline status -> {}", match hotline::request(pipe, "status") {
        Ok(v) => format!("ok={}", v["ok"]),
        Err(e) => format!("error {:?} {e}", e.raw_os_error()),
    }));
    let medium = std::env::temp_dir().join("bureau-spike-low-write-test.txt");
    lines.push(format!("[low-check] write {} -> {:?}", medium.display(), std::fs::write(&medium, "x").map_err(|e| e.raw_os_error())));
    let wmi_out = low_dir().join("wmi-from-low.json");
    let _ = std::fs::remove_file(&wmi_out);
    let cl = format!("\"{}\" board --cmd run.start --pipe {} --out {}", exe.display(), pipe, wmi_out.display());
    let ps = format!(
        "try {{ $r = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{{CommandLine='{}'}} -ErrorAction Stop; \"ReturnValue=$($r.ReturnValue) pid=$($r.ProcessId)\" }} catch {{ \"WMI error: $($_.Exception.Message)\" }}",
        cl.replace('\'', "''")
    );
    let o = Command::new("powershell.exe").args(["-NoProfile", "-NonInteractive", "-Command", &ps]).output();
    lines.push(format!("[low-check] WMI create -> {:?}", o.map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())));
    std::thread::sleep(Duration::from_secs(3));
    lines.push(format!("[low-check] WMI-created process reply: {}", std::fs::read_to_string(&wmi_out).unwrap_or_else(|_| "<none>".into())));
    let s = lines.join("\n");
    println!("{s}");
    let _ = std::fs::write(out, s);
    0
}

/// Runs at Low integrity: one `claude -p` ping, result written to --out.
fn claude_run(args: &[String], pipe: &str) -> i32 {
    let out = arg(args, "--out").unwrap_or_default();
    let env: Vec<(String, String)> = std::env::vars().collect();
    let cwd = low_dir().join("claude-cwd");
    let s = match claude_command(false, pipe, &env, &cwd) {
        Ok(mut c) => {
            let t = Instant::now();
            match c.output() {
                Ok(o) => summarize(&o, t.elapsed()).to_string(),
                Err(e) => format!("spawn: {e}"),
            }
        }
        Err(e) => e,
    };
    let _ = std::fs::write(out, s);
    0
}

/// Runs inside the job: shows membership, refused breakaway, and that grandchildren inherit.
fn job_child(args: &[String], pipe: &str) -> i32 {
    let exe = std::env::current_exe().unwrap_or_default();
    let in_job = win::in_job(win::current_process(), std::ptr::null_mut());
    println!("[job-child] pid={} in_any_job={:?}", std::process::id(), in_job);
    // 1. Breakaway must fail: the job has no BREAKAWAY_OK.
    {
        use std::os::windows::process::CommandExt;
        let r = Command::new("cmd.exe").args(["/c", "exit", "0"]).creation_flags(job::CREATE_BREAKAWAY_FROM_JOB).status();
        println!("[job-child] spawn with CREATE_BREAKAWAY_FROM_JOB -> {:?}", r.map_err(|e| (e.raw_os_error(), e.to_string())));
    }
    // 2. Grandchild via cmd.exe (three levels deep): job-check.
    let o = {
        use std::os::windows::process::CommandExt;
        Command::new("cmd.exe")
            .raw_arg(format!("/d /c \"\"{}\" job-check --pipe {}\"", exe.display(), pipe))
            .output()
    };
    match o {
        Ok(o) => print!("{}", String::from_utf8_lossy(&o.stdout)),
        Err(e) => println!("[job-child] grandchild: {e}"),
    }
    // 3. Optional: escape through WMI (the process is created by WmiPrvSE, outside our job).
    if flag(args, "--wmi-escape") {
        let out = std::env::temp_dir().join("bureau-spike-wmi.json");
        let _ = std::fs::remove_file(&out);
        let cl = format!("\"{}\" board --cmd run.start --pipe {} --out {}", exe.display(), pipe, out.display());
        let ps = format!(
            "$r = Invoke-CimMethod -ClassName Win32_Process -MethodName Create -Arguments @{{CommandLine='{}'}}; \"$($r.ReturnValue) $($r.ProcessId)\"",
            cl.replace('\'', "''")
        );
        let o = Command::new("powershell.exe").args(["-NoProfile", "-NonInteractive", "-Command", &ps]).output();
        println!("[job-child] WMI Win32_Process.Create -> {:?}", o.map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()));
        for _ in 0..40 {
            if out.is_file() {
                break;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        println!("[job-child] escaped process reply: {}", std::fs::read_to_string(&out).unwrap_or_else(|_| "<none>".into()));
    }
    0
}

fn job_check(pipe: &str) -> i32 {
    let exe = std::env::current_exe().unwrap_or_default();
    println!(
        "[job-check] pid={} in_any_job={:?}",
        std::process::id(),
        win::in_job(win::current_process(), std::ptr::null_mut())
    );
    for (sub, cmd) in [("client", "status"), ("board", "run.start")] {
        let o = Command::new(&exe).args([sub, "--cmd", cmd, "--pipe", pipe]).output();
        match o {
            Ok(o) => print!("[job-check] {sub} {cmd}: {}", String::from_utf8_lossy(&o.stdout)),
            Err(e) => println!("[job-check] {sub}: {e}"),
        }
    }
    0
}
