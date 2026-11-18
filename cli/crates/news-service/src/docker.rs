//! `docker.status`, `docker.start` and `docker.stop` (ADR 0007, `contracts/hotline/commands/docker.*`).
//!
//! The News containers (PostgreSQL, the OTel collector, DynamoDB local) can be started and stopped
//! from Board; the shared ClickHouse is health-checked only. Everything goes through the `docker`
//! command line, run by a [`Runner`] so tests can answer instead of Docker. No container name from a
//! request reaches a command line unless it is one of the managed names.

use news_core::NewsError;
use serde_json::{Value, json};
use std::sync::Arc;

/// The News containers Board may start and stop (`infra/docker-compose.yml`), with their roles.
pub const MANAGED: &[(&str, &str)] = &[
    ("news-postgres", "postgres"),
    ("news-otel", "otel"),
    ("news-dynamodb", "dynamodb"),
];

/// Runs `docker <args>`; `Err` carries the reason it could not run or failed.
pub trait Runner: Send + Sync {
    fn run(&self, args: &[&str]) -> Result<String, String>;
}

/// The real Docker command line.
pub struct DockerCli;

impl Runner for DockerCli {
    fn run(&self, args: &[&str]) -> Result<String, String> {
        let out = std::process::Command::new("docker")
            .args(args)
            .output()
            .map_err(|e| format!("cannot run docker: {e}"))?;
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).into_owned())
        } else {
            let err = String::from_utf8_lossy(&out.stderr);
            Err(err
                .lines()
                .next()
                .unwrap_or("docker failed")
                .trim()
                .to_owned())
        }
    }
}

pub fn default_runner() -> Arc<dyn Runner> {
    Arc::new(DockerCli)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Row {
    name: String,
    image: String,
    state: &'static str,
    health: &'static str,
}

/// One `docker ps -a --format '{{json .}}'` line.
fn parse_row(line: &str) -> Option<Row> {
    let v: Value = serde_json::from_str(line).ok()?;
    let status = v["Status"].as_str().unwrap_or_default();
    let health = if status.contains("(healthy)") {
        "healthy"
    } else if status.contains("(unhealthy)") {
        "unhealthy"
    } else if status.contains("health: starting") {
        "starting"
    } else {
        "none"
    };
    let state = match v["State"].as_str().unwrap_or_default() {
        "running" if health == "starting" => "starting",
        "running" => "running",
        "created" | "restarting" => "starting",
        "exited" | "dead" | "paused" | "removing" => "exited",
        _ => "unknown",
    };
    Some(Row {
        name: v["Names"].as_str()?.split(',').next()?.to_owned(),
        image: v["Image"].as_str().unwrap_or_default().to_owned(),
        state,
        health,
    })
}

fn role_of(name: &str, image: &str) -> Option<&'static str> {
    MANAGED
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, r)| *r)
        .or_else(|| {
            (name.contains("clickhouse") || image.contains("clickhouse")).then_some("clickhouse")
        })
}

fn rows(runner: &dyn Runner) -> Result<Vec<Row>, String> {
    let out = runner.run(&["ps", "-a", "--format", "{{json .}}"])?;
    Ok(out.lines().filter_map(parse_row).collect())
}

/// `docker.status`: the engine, every managed container (`missing` when it was never created) and the
/// shared ClickHouse.
pub fn status(runner: &dyn Runner) -> Value {
    let all = match rows(runner) {
        Ok(r) => r,
        Err(reason) => {
            return json!({"engine": {"reachable": false, "reason": reason}, "containers": [],
                          "text": format!("Docker is not reachable: {reason}")});
        }
    };
    let mut containers = Vec::new();
    for (name, role) in MANAGED {
        let found = all.iter().find(|r| r.name == *name);
        containers.push(json!({"name": name, "role": role, "managed": true,
            "state": found.map_or("missing", |r| r.state),
            "health": found.map_or("none", |r| r.health), "started_at": null}));
    }
    for r in all
        .iter()
        .filter(|r| role_of(&r.name, &r.image) == Some("clickhouse"))
    {
        containers.push(
            json!({"name": r.name, "role": "clickhouse", "managed": false,
            "state": r.state, "health": r.health, "started_at": null}),
        );
    }
    let lines: Vec<String> = containers
        .iter()
        .map(|c| {
            let health = match c["health"].as_str() {
                Some("none") | None => String::new(),
                Some(h) => format!(" ({h})"),
            };
            format!(
                "{} {}{}",
                c["name"].as_str().unwrap_or_default(),
                c["state"].as_str().unwrap_or_default(),
                health
            )
        })
        .collect();
    json!({"engine": {"reachable": true, "reason": null}, "containers": containers,
           "text": format!("Docker is running. {}.", lines.join("; "))})
}

fn check_managed(name: &str) -> Result<(), NewsError> {
    if MANAGED.iter().any(|(n, _)| *n == name) {
        return Ok(());
    }
    if name.contains("clickhouse") {
        return Err(NewsError::precondition(
            "the shared ClickHouse is health-checked only: Board never starts or stops it",
        ));
    }
    Err(NewsError::not_found(format!(
        "{name} is not a News container Board manages"
    )))
}

/// `docker.start` and `docker.stop` (`start` false). Only the managed names reach `docker`.
pub fn control(runner: &dyn Runner, name: &str, start: bool) -> Result<Value, NewsError> {
    check_managed(name)?;
    let verb = if start { "start" } else { "stop" };
    runner
        .run(&[verb, name])
        .map_err(|e| NewsError::upstream(format!("docker {verb} {name} failed: {e}")))?;
    let state = rows(runner)
        .ok()
        .and_then(|r| r.into_iter().find(|r| r.name == name))
        .map_or("unknown", |r| r.state);
    Ok(json!({"name": name, "state": state,
              "text": format!("{name} {}; it is now {state}.", if start { "started" } else { "stopped" })}))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use news_core::ErrorCode;
    use std::sync::Mutex;

    struct Fake {
        ps: Result<String, String>,
        calls: Mutex<Vec<String>>,
    }

    impl Runner for Fake {
        fn run(&self, args: &[&str]) -> Result<String, String> {
            self.calls.lock().unwrap().push(args.join(" "));
            if args[0] == "ps" {
                self.ps.clone()
            } else {
                Ok(String::new())
            }
        }
    }

    const PS: &str = r#"{"Names":"news-postgres","Image":"pgvector/pgvector:0.8.1-pg17","State":"running","Status":"Up 2 hours (healthy)"}
{"Names":"news-otel","Image":"otel/opentelemetry-collector-contrib:0.139.0","State":"exited","Status":"Exited (0) 3 minutes ago"}
{"Names":"shared-clickhouse","Image":"clickhouse/clickhouse-server:25.8","State":"running","Status":"Up 5 days (unhealthy)"}
{"Names":"someone-elses-neo4j","Image":"neo4j:5","State":"running","Status":"Up 1 hour"}"#;

    fn fake(ps: Result<String, String>) -> Fake {
        Fake {
            ps,
            calls: Mutex::default(),
        }
    }

    #[test]
    fn status_lists_managed_containers_and_the_shared_clickhouse_and_ignores_the_rest() {
        let v = status(&fake(Ok(PS.into())));
        news_hotline::contracts()
            .check_output("docker.status", &v)
            .unwrap();
        let c = v["containers"].as_array().unwrap();
        let by = |n: &str| c.iter().find(|x| x["name"] == n).unwrap().clone();
        assert_eq!(by("news-postgres")["health"], "healthy");
        assert_eq!(by("news-otel")["state"], "exited");
        assert_eq!(by("news-dynamodb")["state"], "missing");
        assert_eq!(by("shared-clickhouse")["managed"], false);
        assert_eq!(by("shared-clickhouse")["health"], "unhealthy");
        assert!(c.iter().all(|x| x["name"] != "someone-elses-neo4j"));
    }

    #[test]
    fn an_unreachable_engine_is_reported_with_its_reason() {
        let v = status(&fake(Err("Cannot connect to the Docker daemon".into())));
        news_hotline::contracts()
            .check_output("docker.status", &v)
            .unwrap();
        assert_eq!(v["engine"]["reachable"], false);
        assert!(v["text"].as_str().unwrap().contains("Cannot connect"));
    }

    #[test]
    fn start_and_stop_reach_docker_only_for_managed_names() {
        let f = fake(Ok(PS.into()));
        let v = control(&f, "news-otel", true).unwrap();
        news_hotline::contracts()
            .check_output("docker.start", &v)
            .unwrap();
        assert!(
            f.calls
                .lock()
                .unwrap()
                .iter()
                .any(|c| c == "start news-otel")
        );
        for bad in ["shared-clickhouse", "news-postgres; rm -rf", "other"] {
            let before = f.calls.lock().unwrap().len();
            let e = control(&f, bad, false).unwrap_err();
            assert!(matches!(
                e.code,
                ErrorCode::Precondition | ErrorCode::NotFound
            ));
            assert_eq!(
                f.calls.lock().unwrap().len(),
                before,
                "{bad} reached docker"
            );
        }
    }
}
