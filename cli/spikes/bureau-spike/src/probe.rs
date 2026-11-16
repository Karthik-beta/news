//! What the service would see as Karthik's user: the fresh-logon environment, PostgreSQL,
//! the shared ClickHouse and Docker. Reads connection values from a .env file and never prints them.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::Command;
use std::time::{Duration, Instant};

use serde_json::json;

use crate::{arg, job, win};

fn dotenv(path: &str) -> std::collections::HashMap<String, String> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().trim_matches('"').to_string()))
        .collect()
}

pub fn run(args: &[String]) -> i32 {
    let Some(path) = arg(args, "--dotenv") else {
        eprintln!("probe --dotenv PATH");
        return 2;
    };
    let env = dotenv(&path);

    // 1. Fresh-logon environment vs this process's environment (names only).
    let fresh = win::fresh_user_environment().unwrap_or_default();
    let fresh_names: std::collections::BTreeSet<String> = fresh.iter().map(|(k, _)| k.to_ascii_uppercase()).collect();
    let mine: std::collections::BTreeSet<String> = std::env::vars_os().map(|(k, _)| k.to_string_lossy().to_ascii_uppercase()).collect();
    let fresh_path = fresh.iter().find(|(k, _)| k.eq_ignore_ascii_case("path")).map(|(_, v)| v.clone()).unwrap_or_default();
    let get = |k: &str| fresh.iter().find(|(n, _)| n.eq_ignore_ascii_case(k)).map(|(_, v)| v.clone());
    println!("{}", json!({
        "probe": "environment",
        "fresh_count": fresh_names.len(), "process_count": mine.len(),
        "only_in_process": mine.difference(&fresh_names).cloned().collect::<Vec<_>>(),
        "only_in_fresh": fresh_names.difference(&mine).cloned().collect::<Vec<_>>(),
        "fresh_userprofile": get("USERPROFILE"), "fresh_has_claude_config_dir": get("CLAUDE_CONFIG_DIR").is_some(),
        "fresh_has_anthropic_api_key": get("ANTHROPIC_API_KEY").is_some(),
        "claude_on_fresh_path": job::find_claude(&fresh_path).map(|p| p.display().to_string()),
        "fresh_path_dirs": fresh_path.split(';').filter(|d| !d.is_empty()).count(),
        "fresh_path_has": {
            "nvm4w": fresh_path.to_ascii_lowercase().contains("nvm4w"),
            "docker": fresh_path.to_ascii_lowercase().contains("docker"),
            "git": fresh_path.to_ascii_lowercase().contains("\\git\\"),
            "cargo": fresh_path.to_ascii_lowercase().contains(".cargo"),
            "bun": fresh_path.to_ascii_lowercase().contains(".bun"),
        },
        "credentials_file": get("USERPROFILE").map(|u| std::path::Path::new(&u).join(".claude").join(".credentials.json").is_file()),
    }));

    // 2. PostgreSQL.
    let t = Instant::now();
    let pg = env.get("DATABASE_URL").map(|url| {
        postgres::Client::connect(url, postgres::NoTls).and_then(|mut c| {
            c.query_one("select current_user::text, split_part(version(), ' ', 2), inet_server_addr()::text", &[])
                .map(|r| (r.get::<_, String>(0), r.get::<_, String>(1), r.get::<_, Option<String>>(2)))
        })
    });
    println!("{}", json!({"probe": "postgres", "ms": t.elapsed().as_millis() as u64,
        "result": match pg { None => json!("DATABASE_URL missing"), Some(Ok((u, v, a))) => json!({"ok": true, "user": u, "version": v, "server_addr": a}), Some(Err(e)) => json!({"ok": false, "error": e.to_string()}) }}));

    // 3. ClickHouse over HTTP (no TLS; loopback), credentials in headers, never printed.
    let t = Instant::now();
    let ch = clickhouse(
        env.get("NEWS_CH_HTTP").map(String::as_str).unwrap_or("http://127.0.0.1:8123"),
        env.get("NEWS_CH_USER").map(String::as_str).unwrap_or("default"),
        env.get("NEWS_CH_PASSWORD").map(String::as_str).unwrap_or(""),
        "SELECT version(), currentUser() FORMAT TabSeparated",
    );
    println!("{}", json!({"probe": "clickhouse", "ms": t.elapsed().as_millis() as u64, "result": match ch { Ok(s) => json!({"ok": true, "body": s}), Err(e) => json!({"ok": false, "error": e}) }}));

    // 4. Docker with only the allow-listed fresh-logon environment.
    let allow = job::allow_listed_env().unwrap_or_default();
    let t = Instant::now();
    let d = Command::new("docker")
        .env_clear()
        .envs(allow.iter().map(|(k, v)| (k, v)))
        .args(["ps", "--format", "{{.Names}} {{.Status}}"])
        .output();
    println!("{}", json!({"probe": "docker", "ms": t.elapsed().as_millis() as u64, "result": match d {
        Ok(o) => json!({"exit": o.status.code(), "stdout": String::from_utf8_lossy(&o.stdout), "stderr": String::from_utf8_lossy(&o.stderr).chars().take(400).collect::<String>()}),
        Err(e) => json!({"error": e.to_string()}) }}));
    0
}

fn clickhouse(base: &str, user: &str, key: &str, sql: &str) -> Result<String, String> {
    let hostport = base.trim_start_matches("http://").trim_end_matches('/').split('/').next().unwrap_or("127.0.0.1:8123");
    let mut s = TcpStream::connect(hostport).map_err(|e| e.to_string())?;
    s.set_read_timeout(Some(Duration::from_secs(10))).map_err(|e| e.to_string())?;
    let req = format!(
        "POST / HTTP/1.1\r\nHost: {hostport}\r\nX-ClickHouse-User: {user}\r\nX-ClickHouse-Key: {key}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{sql}",
        sql.len()
    );
    s.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    let mut buf = String::new();
    s.read_to_string(&mut buf).map_err(|e| e.to_string())?;
    let status = buf.lines().next().unwrap_or("").to_string();
    let body = buf.split("\r\n\r\n").nth(1).unwrap_or("").trim().to_string();
    if status.contains(" 200 ") { Ok(body) } else { Err(format!("{status}: {}", body.chars().take(200).collect::<String>())) }
}
