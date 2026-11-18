//! The service's log ring (the last 2,000 lines) behind `service.log` and the `service` topic.

use chrono::Utc;
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::sync::Mutex;
use uuid::Uuid;

const KEEP: usize = 2000;

#[derive(Default)]
pub struct LogRing {
    lines: Mutex<(u64, VecDeque<Value>)>,
}

impl LogRing {
    /// Add a line (`info`, `warn` or `error`) and return it as the contracts' `logLine`.
    pub fn push(&self, level: &str, target: &str, message: &str, run_id: Option<Uuid>) -> Value {
        let mut g = self.lines.lock().unwrap_or_else(|p| p.into_inner());
        g.0 += 1;
        let line = json!({"seq": g.0, "at": Utc::now(), "level": level, "target": target,
                          "message": message, "run_id": run_id});
        g.1.push_back(line.clone());
        while g.1.len() > KEEP {
            g.1.pop_front();
        }
        match level {
            "error" => tracing::error!(target: "news_service", "{message}"),
            "warn" => tracing::warn!(target: "news_service", "{message}"),
            _ => tracing::info!(target: "news_service", "{message}"),
        }
        line
    }

    /// `service.log`: lines after `after_seq`, at `level` or worse, at most `limit`.
    pub fn read(&self, after: u64, limit: usize, level: Option<&str>) -> Value {
        let rank = |l: &str| match l {
            "error" => 2,
            "warn" => 1,
            _ => 0,
        };
        let min = level.map_or(0, rank);
        let g = self.lines.lock().unwrap_or_else(|p| p.into_inner());
        let items: Vec<&Value> =
            g.1.iter()
                .filter(|l| l["seq"].as_u64().unwrap_or(0) > after)
                .filter(|l| rank(l["level"].as_str().unwrap_or("info")) >= min)
                .take(limit)
                .collect();
        json!({"items": items, "last_seq": g.0})
    }
}
