//! `news work --pools net,cpu,docling`: the optional background worker (Bureau §5.6, decision D8).
//!
//! It runs code-only pools. `ai`, `render` and `publish` are refused (usage, exit 2), and the process
//! forbids itself from reading the write-API key before doing anything else (§9.3).

use super::{Reply, database};
use crate::cli::WorkArgs;
use crate::registry;
use news_core::{NewsError, NewsResult, secrets};
use news_dispatch::WORK_POOLS;
use news_dispatch::worker::{self, WorkOptions};
use serde_json::json;
use std::time::Duration;

pub async fn run(args: WorkArgs) -> NewsResult<Reply> {
    secrets::forbid_write_api_key();
    let pools = parse_pools(&args.pools)?;
    let worker_id = args.worker.unwrap_or_else(default_worker_id);
    let (db, _) = database(12).await?;

    let mut opts = WorkOptions::new(pools.clone(), worker_id.clone());
    opts.once = args.once;
    opts.max_duration = args.max_seconds.map(Duration::from_secs);
    let shutdown = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    tracing::info!(worker = %worker_id, pools = ?pools, "news work started");
    let report = worker::run(db, opts, registry::resolve, shutdown).await?;
    Ok(Reply::new(json!({
        "worker": worker_id,
        "pools": pools,
        "report": report,
        "write_api_key_reads": secrets::write_api_key_reads(),
    })))
}

/// Comma-separated pools, each one of `net`, `cpu`, `docling`.
pub fn parse_pools(raw: &str) -> NewsResult<Vec<String>> {
    let mut pools: Vec<String> = Vec::new();
    for p in raw.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        if !WORK_POOLS.contains(&p) {
            let why = if news_dispatch::is_pool(p) {
                format!(
                    "pool {p:?} runs only inside preset runs (Ghost, the renderer or Press), never in news work"
                )
            } else {
                format!("unknown pool {p:?}")
            };
            return Err(NewsError::usage(why).with_hint("news work --pools net,cpu,docling"));
        }
        if !pools.iter().any(|q| q == p) {
            pools.push(p.to_owned());
        }
    }
    if pools.is_empty() {
        return Err(NewsError::usage(
            "--pools needs at least one of net, cpu, docling",
        ));
    }
    Ok(pools)
}

fn default_worker_id() -> String {
    let host = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "local".into());
    format!("work-{}-{}", host.to_lowercase(), std::process::id())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn only_code_pools_are_accepted() {
        assert_eq!(
            parse_pools("net,cpu,docling").unwrap(),
            ["net", "cpu", "docling"]
        );
        assert_eq!(parse_pools(" net , net ").unwrap(), ["net"]);
        for bad in ["ai", "render", "publish", "net,ai", "gpu", "", ","] {
            assert_eq!(parse_pools(bad).unwrap_err().exit_code(), 2, "{bad}");
        }
    }
}
