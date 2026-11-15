//! Dispatch against PostgreSQL: concurrency, crash recovery, snoozes, unique keys, retries,
//! pool pauses and schedules (Bureau §5.4, §5.6).

use super::fixture::Fixture;
use news_core::job::{ErrorClass, JobFailure};
use news_dispatch::queue::{self, NewJob};
use news_dispatch::schedule;
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const LEASE: Duration = Duration::from_secs(60);

async fn enqueue(f: &Fixture, kind: &str, pool: &str) -> i64 {
    queue::enqueue(&f.pool, &NewJob::new(kind).pool(pool))
        .await
        .unwrap()
        .job_id
}

async fn state_of(f: &Fixture, job_id: i64) -> queue::Job {
    queue::get(&f.pool, job_id).await.unwrap()
}

/// Make every waiting job ready now (tests do not wait out backoffs or snoozes).
async fn make_ready(f: &Fixture) {
    sqlx::query("update jobs set run_after = now() - interval '1 second' where state <> 'running'")
        .execute(&f.pool)
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn eight_workers_and_200_jobs_never_exceed_max_running_and_never_double_claim() {
    let f = Fixture::migrated().await;
    // 200 jobs across the three work pools (max_running 6, 2, 1).
    let pools = [("net", 6usize), ("cpu", 2), ("docling", 1)];
    for i in 0..200 {
        let (pool, _) = pools[i % 3];
        queue::enqueue(
            &f.pool,
            &NewJob::new("test.job").pool(pool).args(json!({"i": i})),
        )
        .await
        .unwrap();
    }
    let limits: HashMap<&str, usize> = pools.into_iter().collect();
    let claimed: Arc<Mutex<HashSet<i64>>> = Arc::default();
    let holding: Arc<HashMap<&str, AtomicUsize>> = Arc::new(
        pools
            .iter()
            .map(|(p, _)| (*p, AtomicUsize::new(0)))
            .collect(),
    );
    let peak: Arc<Mutex<HashMap<String, usize>>> = Arc::default();
    let done = Arc::new(AtomicUsize::new(0));
    let stop = Arc::new(AtomicBool::new(false));

    // A monitor samples the database's own running counts while the workers run.
    let monitor = {
        let pool = f.pool(1).await;
        let stop = stop.clone();
        let peak = peak.clone();
        tokio::spawn(async move {
            while !stop.load(Ordering::SeqCst) {
                let rows: Vec<(String, i64)> = sqlx::query_as(
                    "select pool, count(*) from jobs where state = 'running' group by pool",
                )
                .fetch_all(&pool)
                .await
                .unwrap();
                {
                    let mut p = peak.lock().unwrap();
                    for (pool, n) in rows {
                        let e = p.entry(pool).or_default();
                        *e = (*e).max(usize::try_from(n).unwrap());
                    }
                }
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
    };

    let mut workers = Vec::new();
    for w in 0..8 {
        let db = f.pool(2).await;
        let claimed = claimed.clone();
        let holding = holding.clone();
        let limits = limits.clone();
        let done = done.clone();
        workers.push(tokio::spawn(async move {
            let name = format!("w{w}");
            let mut idle_rounds = 0;
            while done.load(Ordering::SeqCst) < 200 && idle_rounds < 2000 {
                let mut got_any = false;
                for (pool, _) in [("net", 6), ("cpu", 2), ("docling", 1)]
                    .iter()
                    .cycle()
                    .skip(w % 3)
                    .take(3)
                {
                    let c = queue::claim(&db, pool, 4, LEASE, &name).await.unwrap();
                    for job in c.jobs {
                        got_any = true;
                        assert!(
                            claimed.lock().unwrap().insert(job.job_id),
                            "job {} claimed twice",
                            job.job_id
                        );
                        assert_eq!(job.attempt, 1);
                        let now = holding[pool].fetch_add(1, Ordering::SeqCst) + 1;
                        assert!(
                            now <= limits[pool],
                            "{pool}: {now} running > {}",
                            limits[pool]
                        );
                        tokio::time::sleep(Duration::from_millis(1 + (job.job_id as u64 % 4)))
                            .await;
                        holding[pool].fetch_sub(1, Ordering::SeqCst);
                        queue::complete(&db, job.job_id, Some(&name), &json!({"ok": true}))
                            .await
                            .unwrap();
                        done.fetch_add(1, Ordering::SeqCst);
                    }
                }
                if !got_any {
                    idle_rounds += 1;
                    tokio::task::yield_now().await;
                }
            }
        }));
    }
    for w in workers {
        w.await.unwrap();
    }
    stop.store(true, Ordering::SeqCst);
    monitor.await.unwrap();

    assert_eq!(done.load(Ordering::SeqCst), 200);
    assert_eq!(claimed.lock().unwrap().len(), 200);
    assert_eq!(
        f.scalar_i64("select count(*) from jobs where state = 'completed' and attempt = 1")
            .await,
        200
    );
    for (pool, n) in peak.lock().unwrap().iter() {
        assert!(
            *n <= limits[pool.as_str()],
            "{pool}: database saw {n} running"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn a_killed_workers_job_returns_to_retryable_through_the_rescuer() {
    let f = Fixture::migrated().await;
    let id = enqueue(&f, "doc.parse_docling", "docling").await;
    // A worker process claims with a 1-second lease and dies without a heartbeat or an outcome.
    let (env, code, _, _) = f.call(&[
        "jobs",
        "claim",
        "--pool",
        "docling",
        "--worker",
        "doomed",
        "--lease-seconds",
        "1",
    ]);
    assert_eq!(code, 0, "{env}");
    assert_eq!(env["data"]["items"][0]["job_id"], id);
    assert_eq!(state_of(&f, id).await.state, "running");
    // While the lease holds, the rescuer leaves it alone and nobody else can claim it.
    assert!(queue::rescue(&f.pool).await.unwrap().retryable.is_empty());
    assert!(
        queue::claim(&f.pool, "docling", 1, LEASE, "other")
            .await
            .unwrap()
            .jobs
            .is_empty()
    );

    tokio::time::sleep(Duration::from_millis(1500)).await;
    // A second worker starts: its first maintenance pass runs the rescuer. It runs only `net`, so it
    // does not pick the rescued docling job up again.
    let (env, code, _, _) = f.call(&["work", "--pools", "net", "--once"]);
    assert_eq!(code, 0, "{env}");
    assert_eq!(env["data"]["report"]["rescued"], 1);
    let job = state_of(&f, id).await;
    assert_eq!(job.state, "retryable");
    assert_eq!(job.attempt, 1);
    assert_eq!(job.locked_by, None);
    assert_eq!(job.lease_until, None);
    assert_eq!(job.errors[0]["class"], "transient");
    assert!(
        job.errors[0]["message"]
            .as_str()
            .unwrap()
            .contains("lease expired (worker doomed")
    );
    // It can be claimed again, as attempt 2.
    let again = queue::claim(&f.pool, "docling", 1, LEASE, "next")
        .await
        .unwrap();
    assert_eq!(again.jobs[0].job_id, id);
    assert_eq!(again.jobs[0].attempt, 2);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn the_rescuer_discards_a_stuck_job_with_no_attempts_left_and_runs_once_at_a_time() {
    let f = Fixture::migrated().await;
    let id = queue::enqueue(&f.pool, &NewJob::new("doc.fetch").max_attempts(1))
        .await
        .unwrap()
        .job_id;
    queue::claim(&f.pool, "net", 1, Duration::from_millis(10), "w")
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    // Another session holding advisory lock 42 keeps this rescuer out.
    let mut holder = f.pool.acquire().await.unwrap();
    sqlx::query("select pg_advisory_lock(42)")
        .execute(&mut *holder)
        .await
        .unwrap();
    assert!(!queue::rescue(&f.pool).await.unwrap().ran);
    sqlx::query("select pg_advisory_unlock(42)")
        .execute(&mut *holder)
        .await
        .unwrap();
    drop(holder);
    let r = queue::rescue(&f.pool).await.unwrap();
    assert!(r.ran);
    assert_eq!(r.discarded, vec![id]);
    let job = state_of(&f, id).await;
    assert_eq!(job.state, "discarded");
    assert!(job.finished_at.is_some());
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn a_rate_limited_snooze_leaves_the_attempt_unchanged() {
    let f = Fixture::migrated().await;
    let id = enqueue(&f, "doc.fetch", "net").await;
    assert_eq!(state_of(&f, id).await.attempt, 0);
    let (_, code, _, _) = f.call(&["jobs", "claim", "--pool", "net", "--worker", "w1"]);
    assert_eq!(code, 0);
    assert_eq!(state_of(&f, id).await.attempt, 1);

    let id_s = id.to_string();
    let (env, code, _, _) = f.call(&[
        "jobs",
        "fail",
        &id_s,
        "--class",
        "rate_limited",
        "--message",
        "429 Too Many Requests",
        "--retry-after",
        "30",
        "--worker",
        "w1",
    ]);
    assert_eq!(code, 0, "{env}");
    assert_eq!(env["data"]["state"], "snoozed");
    let job = state_of(&f, id).await;
    assert_eq!(job.state, "snoozed");
    assert_eq!(job.attempt, 0, "a snooze must not spend an attempt");
    assert_eq!(job.snoozes, 1);
    let wait = job.run_after - chrono::Utc::now();
    assert!(
        wait > chrono::Duration::seconds(25) && wait <= chrono::Duration::seconds(31),
        "{wait}"
    );
    // Not claimable before its time; claimable after, as attempt 1 again.
    assert!(
        queue::claim(&f.pool, "net", 1, LEASE, "w2")
            .await
            .unwrap()
            .jobs
            .is_empty()
    );
    make_ready(&f).await;
    let again = queue::claim(&f.pool, "net", 1, LEASE, "w2").await.unwrap();
    assert_eq!(again.jobs[0].attempt, 1);
    // Snoozing many times never discards the job.
    for _ in 0..10 {
        queue::fail(
            &f.pool,
            id,
            None,
            &JobFailure::new(ErrorClass::RateLimited, "429"),
        )
        .await
        .unwrap();
        make_ready(&f).await;
        queue::claim(&f.pool, "net", 1, LEASE, "w2").await.unwrap();
    }
    let job = state_of(&f, id).await;
    assert_eq!(
        (job.state.as_str(), job.attempt, job.snoozes),
        ("running", 1, 11)
    );
    assert_eq!(
        job.errors.as_array().unwrap().len(),
        5,
        "only the last five errors are kept"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn re_enqueueing_a_live_unique_key_returns_the_existing_job_id() {
    let f = Fixture::migrated().await;
    let job = NewJob::new("doc.parse").unique_key("doc.parse:abc123:sec@1");
    let first = queue::enqueue(&f.pool, &job).await.unwrap();
    assert!(!first.existing);
    let second = queue::enqueue(&f.pool, &job).await.unwrap();
    assert_eq!(second.job_id, first.job_id);
    assert!(second.existing);

    // Concurrent enqueues of one key all get the same id.
    let mut handles = Vec::new();
    for _ in 0..8 {
        let pool = f.pool(1).await;
        let job = NewJob::new("doc.parse").unique_key("doc.parse:race:1");
        handles.push(tokio::spawn(async move {
            queue::enqueue(&pool, &job).await.unwrap().job_id
        }));
    }
    let mut ids = HashSet::new();
    for h in handles {
        ids.insert(h.await.unwrap());
    }
    assert_eq!(ids.len(), 1);

    // Running is still live; once completed, the key is free again.
    let c = queue::claim(&f.pool, "cpu", 1, LEASE, "w").await.unwrap();
    assert_eq!(c.jobs[0].job_id, first.job_id);
    assert_eq!(
        queue::enqueue(&f.pool, &job).await.unwrap().job_id,
        first.job_id
    );
    queue::complete(&f.pool, first.job_id, Some("w"), &json!({}))
        .await
        .unwrap();
    let third = queue::enqueue(&f.pool, &job).await.unwrap();
    assert!(!third.existing);
    assert_ne!(third.job_id, first.job_id);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn transient_failures_back_off_then_discard_when_attempts_run_out() {
    let f = Fixture::migrated().await;
    let id = queue::enqueue(&f.pool, &NewJob::new("doc.fetch").max_attempts(3))
        .await
        .unwrap()
        .job_id;
    for attempt in 1..=3 {
        make_ready(&f).await;
        let c = queue::claim(&f.pool, "net", 1, LEASE, "w").await.unwrap();
        assert_eq!(c.jobs[0].attempt, attempt);
        let failed = queue::fail(
            &f.pool,
            id,
            Some("w"),
            &JobFailure::new(ErrorClass::Transient, "timeout"),
        )
        .await
        .unwrap();
        if attempt < 3 {
            assert_eq!(failed.state, "retryable");
            let ceiling = news_dispatch::backoff::ceiling(attempt);
            let wait = (failed.run_after - chrono::Utc::now())
                .to_std()
                .unwrap_or_default();
            assert!(
                wait <= ceiling,
                "attempt {attempt}: waited {wait:?} > {ceiling:?}"
            );
        } else {
            assert_eq!(failed.state, "discarded");
        }
    }
    let job = state_of(&f, id).await;
    assert_eq!(job.error_class.as_deref(), Some("transient"));
    assert_eq!(job.errors.as_array().unwrap().len(), 3);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn permanent_classes_go_straight_to_the_dead_list_and_retry_brings_them_back() {
    let f = Fixture::migrated().await;
    let mut ids = Vec::new();
    for i in 0..3 {
        let id = enqueue(&f, "doc.parse", "cpu").await;
        queue::claim(&f.pool, "cpu", 1, LEASE, "w").await.unwrap();
        queue::fail(
            &f.pool,
            id,
            None,
            &JobFailure::new(ErrorClass::InvalidData, format!("unknown unit in row {i}")),
        )
        .await
        .unwrap();
        ids.push(id);
    }
    let (env, code, _, _) = f.call(&["jobs", "dead"]);
    assert_eq!(code, 0, "{env}");
    let groups = env["data"]["items"].as_array().unwrap();
    assert_eq!(groups.len(), 1, "one fingerprint: digits are masked");
    assert_eq!(groups[0]["error_class"], "invalid_data");
    assert_eq!(groups[0]["count"], 3);
    assert_eq!(groups[0]["fingerprint"], "unknown unit in row #");

    let (env, code, _, _) = f.call(&["jobs", "retry", "--class", "invalid_data"]);
    assert_eq!(code, 0, "{env}");
    assert_eq!(env["data"]["requeued"], json!(ids));
    for id in ids {
        let job = state_of(&f, id).await;
        assert_eq!((job.state.as_str(), job.attempt), ("available", 0));
    }
    // A running job cannot be "retried".
    let c = queue::claim(&f.pool, "cpu", 1, LEASE, "w").await.unwrap();
    let id_s = c.jobs[0].job_id.to_string();
    let (env, code, _, _) = f.call(&["jobs", "retry", &id_s]);
    assert_eq!((code, env["error"]["code"].as_str()), (4, Some("conflict")));
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn retrying_more_than_50_dead_jobs_is_karthiks_call() {
    let f = Fixture::migrated().await;
    sqlx::query(
        "insert into jobs (kind, pool, state, error_class, finished_at)
         select 'doc.fetch', 'net', 'discarded', 'upstream_changed', now() from generate_series(1, 51)",
    )
    .execute(&f.pool)
    .await
    .unwrap();
    let mut agent = f.news();
    agent
        .args(["jobs", "retry", "--class", "upstream_changed"])
        .env("CLAUDECODE", "1");
    let (env, code, _, _) = super::fixture::call(&mut agent);
    assert_eq!(
        (code, env["error"]["code"].as_str()),
        (7, Some("gated")),
        "{env}"
    );
    assert_eq!(env["error"]["details"]["matched"], 51);
    assert_eq!(
        f.scalar_i64("select count(*) from jobs where state = 'available'")
            .await,
        0
    );
    let (env, code, _, _) = f.call(&["jobs", "retry", "--class", "upstream_changed"]);
    assert_eq!(code, 0, "{env}");
    assert_eq!(env["data"]["requeued"].as_array().unwrap().len(), 51);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn a_usage_limit_pauses_the_ai_pool_without_spending_the_attempt() {
    let f = Fixture::migrated().await;
    let a = enqueue(&f, "ai.write", "ai").await;
    enqueue(&f, "ai.write", "ai").await;
    let c = queue::claim(&f.pool, "ai", 1, LEASE, "ghost")
        .await
        .unwrap();
    assert_eq!(c.jobs[0].job_id, a);
    let failed = queue::fail(
        &f.pool,
        a,
        Some("ghost"),
        &JobFailure::new(ErrorClass::AiUsageLimit, "plan limit reached")
            .with_retry_after(Duration::from_secs(600)),
    )
    .await
    .unwrap();
    assert!(failed.pool_paused_until.is_some());
    let job = state_of(&f, a).await;
    assert_eq!((job.state.as_str(), job.attempt), ("snoozed", 0));
    // The paused pool hands out nothing, and says until when.
    let c = queue::claim(&f.pool, "ai", 3, LEASE, "ghost")
        .await
        .unwrap();
    assert!(c.jobs.is_empty());
    assert!(c.paused_until.is_some());
    // Unpausing (Karthik, from a terminal) releases it.
    let (env, code, _, _) = f.call(&["jobs", "unpause", "ai"]);
    assert_eq!(code, 0, "{env}");
    assert_eq!(
        queue::claim(&f.pool, "ai", 3, LEASE, "ghost")
            .await
            .unwrap()
            .jobs
            .len(),
        1
    );
    let (env, code, _, _) = f.call(&["jobs", "pause", "cpu", "--until", "+2h"]);
    assert_eq!(code, 0, "{env}");
    assert!(
        queue::claim(&f.pool, "cpu", 1, LEASE, "w")
            .await
            .unwrap()
            .paused_until
            .is_some()
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn heartbeats_extend_the_lease_and_only_the_holder_finishes_the_job() {
    let f = Fixture::migrated().await;
    let id = enqueue(&f, "doc.fetch", "net").await;
    let c = queue::claim(&f.pool, "net", 1, Duration::from_secs(5), "holder")
        .await
        .unwrap();
    let before = c.jobs[0].lease_until.unwrap();
    let id_s = id.to_string();
    let (env, code, _, _) = f.call(&[
        "jobs",
        "heartbeat",
        &id_s,
        "--worker",
        "holder",
        "--lease-seconds",
        "300",
    ]);
    assert_eq!(code, 0, "{env}");
    assert!(state_of(&f, id).await.lease_until.unwrap() > before);

    let result = std::env::temp_dir().join(format!("news-result-{}-{id}.json", std::process::id()));
    std::fs::write(&result, r#"{"snapshots": 2}"#).unwrap();
    let result_s = result.to_string_lossy().into_owned();
    let (env, code, _, _) = f.call(&[
        "jobs", "complete", &id_s, "--result", &result_s, "--worker", "intruder",
    ]);
    assert_eq!(
        (code, env["error"]["code"].as_str()),
        (4, Some("conflict")),
        "{env}"
    );
    let (env, code, _, _) = f.call(&[
        "jobs", "complete", &id_s, "--result", &result_s, "--worker", "holder",
    ]);
    assert_eq!(code, 0, "{env}");
    let job = state_of(&f, id).await;
    assert_eq!(job.state, "completed");
    assert_eq!(job.result, Some(json!({"snapshots": 2})));
    // Finished jobs take no heartbeat.
    let (env, code, _, _) = f.call(&["jobs", "heartbeat", &id_s]);
    assert_eq!((code, env["error"]["code"].as_str()), (4, Some("conflict")));
    let (env, code, _, _) = f.call(&["jobs", "heartbeat", "999999"]);
    assert_eq!(
        (code, env["error"]["code"].as_str()),
        (3, Some("not_found"))
    );
    let _ = std::fs::remove_file(result);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn a_schedule_that_slept_through_several_runs_fires_once() {
    let f = Fixture::migrated().await;
    sqlx::query(
        "update schedules set enabled = true, next_at = now() - interval '3 days' where schedule_id = 'calendar.sync'",
    )
    .execute(&f.pool)
    .await
    .unwrap();
    let fired = schedule::tick(&f.pool).await.unwrap().unwrap();
    assert_eq!(fired.len(), 1);
    assert!(!fired[0].existing);
    assert!(fired[0].next_at > chrono::Utc::now());
    assert_eq!(
        fired[0].next_at.format("%H:%M").to_string(),
        "06:00",
        "cron is UTC"
    );
    assert_eq!(
        f.scalar_i64("select count(*) from jobs where kind = 'calendar.sync'")
            .await,
        1
    );
    // Not due again: nothing fires.
    assert!(schedule::tick(&f.pool).await.unwrap().unwrap().is_empty());
    // Due again while the last run is still waiting: still one job (latest due run only).
    sqlx::query("update schedules set next_at = now() - interval '1 minute'")
        .execute(&f.pool)
        .await
        .unwrap();
    let fired = schedule::tick(&f.pool).await.unwrap().unwrap();
    assert!(fired.iter().all(|x| x.existing));
    assert_eq!(
        f.scalar_i64("select count(*) from jobs where kind = 'calendar.sync'")
            .await,
        1
    );
    // Disabled schedules never fire.
    assert_eq!(
        f.scalar_i64("select count(*) from jobs where kind in ('work.prune', 'traffic.pull')")
            .await,
        0
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn news_work_never_reads_the_write_api_key() {
    // The sentinel test: a live `news work` round over every work pool, with jobs whose handlers run,
    // and the write-API key set to a sentinel in its environment.
    let f = Fixture::migrated().await;
    for (kind, pool) in [
        ("doc.fetch", "net"),
        ("doc.parse", "cpu"),
        ("doc.parse_docling", "docling"),
        ("page.publish", "publish"),
    ] {
        enqueue(&f, kind, pool).await;
    }
    let sentinel = "sentinel-write-key-9d2e41";
    let mut cmd = f.news();
    cmd.args(["work", "--pools", "net,cpu,docling", "--once"])
        .env("NEWS_WRITE_API_KEY", sentinel);
    let (env, code, stdout, stderr) = super::fixture::call(&mut cmd);
    assert_eq!(code, 0, "{env}");
    assert_eq!(env["data"]["write_api_key_reads"], 0);
    assert_eq!(env["data"]["report"]["claimed"], 3, "{env}");
    assert!(!stdout.contains(sentinel) && !stderr.contains(sentinel));
    let errors = f
        .scalar_str("select coalesce(string_agg(errors::text, ' '), '') from jobs")
        .await;
    assert!(!errors.contains(sentinel));
    // The publish job was never touched.
    assert_eq!(
        f.scalar_str("select state from jobs where kind = 'page.publish'")
            .await,
        "available"
    );
    // The stub handlers fail as not-built bugs: discarded, never retried blindly.
    assert_eq!(
        f.scalar_i64("select count(*) from jobs where state = 'discarded' and error_class = 'bug'")
            .await,
        3
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn the_worker_runs_handlers_completes_jobs_and_turns_panics_into_bugs() {
    use news_core::job::{Handler, JobFuture, JobInput};
    fn ok(input: JobInput) -> JobFuture {
        Box::pin(async move { Ok(json!({"echo": input.args})) })
    }
    fn boom(_: JobInput) -> JobFuture {
        Box::pin(async move { panic!("handler exploded") })
    }
    fn resolve(kind: &str) -> Option<Handler> {
        match kind {
            "test.ok" => Some(ok),
            "test.boom" => Some(boom),
            _ => None,
        }
    }
    let f = Fixture::migrated().await;
    let a = queue::enqueue(
        &f.pool,
        &NewJob::new("test.ok").pool("cpu").args(json!({"x": 1})),
    )
    .await
    .unwrap()
    .job_id;
    let b = enqueue(&f, "test.boom", "cpu").await;
    let c = enqueue(&f, "test.unknown", "net").await;
    let mut opts = news_dispatch::worker::WorkOptions::new(vec!["net".into(), "cpu".into()], "t");
    opts.once = true;
    let report = news_dispatch::worker::run(f.pool.clone(), opts, resolve, std::future::pending())
        .await
        .unwrap();
    assert_eq!(
        (report.claimed, report.completed, report.failed),
        (3, 1, 2),
        "{report:?}"
    );
    assert_eq!(
        state_of(&f, a).await.result,
        Some(json!({"echo": {"x": 1}}))
    );
    let b = state_of(&f, b).await;
    assert_eq!(
        (b.state.as_str(), b.error_class.as_deref()),
        ("discarded", Some("bug"))
    );
    assert!(
        b.errors[0]["message"]
            .as_str()
            .unwrap()
            .contains("panicked")
    );
    let c = state_of(&f, c).await;
    assert!(
        c.errors[0]["message"]
            .as_str()
            .unwrap()
            .contains("no handler")
    );
}
