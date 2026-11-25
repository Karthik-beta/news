//! A stand-in for `claude -p`, for Ghost's tests only. It writes what it was given (argv, environment,
//! stdin, pid) to `fake-claude-<pid>.json` in its current directory, then replies as its input asks:
//! `{"fake": {"mode": "ok" | "usage_limit" | "fail" | "spawn", "reset": <unix s>, "output": {…}}}` at
//! the top level or under `brief` (a repair input). `spawn` starts a copy of itself first, and both
//! wait for a `go` file, so a test can look at both processes while they live. `"probe": <path>` makes
//! it try to write that file first and record whether it could (`probe_written`).

use serde_json::{Value, json};
use std::io::Read;
use std::path::Path;
use std::time::Duration;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--fake-sleep") {
        // It inherits its parent's stdout pipe (Windows inherits handles), so it lives only until the
        // test's `go`, or Ghost would wait for it.
        wait_for_go();
        return;
    }
    let mut stdin = String::new();
    let _ = std::io::stdin().read_to_string(&mut stdin);
    let input: Value = serde_json::from_str(&stdin).unwrap_or(Value::Null);
    let fake = if input["fake"].is_object() {
        input["fake"].clone()
    } else {
        input["brief"]["fake"].clone()
    };
    let mode = fake["mode"].as_str().unwrap_or("ok");
    let mut grandchild = None;
    if mode == "spawn"
        && let Ok(exe) = std::env::current_exe()
    {
        grandchild = std::process::Command::new(exe)
            .arg("--fake-sleep")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .ok()
            .map(|c| c.id());
    }
    let probe_written = fake["probe"]
        .as_str()
        .map(|p| std::fs::write(p, "written at Low integrity?").is_ok());
    let env: Vec<(String, String)> = std::env::vars().collect();
    let record = json!({"pid": std::process::id(), "argv": args, "env": env, "stdin": stdin,
                        "grandchild": grandchild, "probe_written": probe_written});
    let _ = std::fs::write(
        format!("fake-claude-{}.json", std::process::id()),
        record.to_string(),
    );
    if mode == "spawn" {
        wait_for_go();
    }
    let usage = json!({"input_tokens": 1000, "output_tokens": 500, "cache_read_input_tokens": 8000,
                       "cache_creation_input_tokens": 200,
                       "cache_creation": {"ephemeral_5m_input_tokens": 0, "ephemeral_1h_input_tokens": 200}});
    match mode {
        "usage_limit" => {
            let reset = fake["reset"].as_i64().unwrap_or(0);
            println!(
                "{}",
                json!({"type": "result", "subtype": "success", "is_error": true, "num_turns": 1,
                       "result": format!("Claude AI usage limit reached|{reset}"),
                       "usage": {"input_tokens": 0, "output_tokens": 0}})
            );
            std::process::exit(1);
        }
        "fail" => {
            eprintln!("fake claude failed");
            std::process::exit(2);
        }
        _ => {
            let output = if fake["output"].is_object() {
                fake["output"].clone()
            } else {
                json!({"headline": "Annual inflation in America fell to {dp_7kq2} in October"})
            };
            println!(
                "{}",
                json!({"type": "result", "subtype": "success", "is_error": false, "num_turns": 1,
                       "total_cost_usd": 0.05, "result": "", "structured_output": output,
                       "usage": usage})
            );
        }
    }
}

/// Wait up to two minutes for a `go` file in the current directory.
fn wait_for_go() {
    for _ in 0..2400 {
        if Path::new("go").exists() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
