//! The read side (Bureau §8.1, ADR 0008 §11): run summaries, the Lineup, run lists, one run, and the
//! status board with its next-action rules. Every figure is read from PostgreSQL by code and every
//! `text` is rendered by code. The Hotline handlers and the terminal's `news run …` call the same
//! functions, so both give identical JSON.

use crate::presets;
use crate::row::{self, RUN_COLUMNS, RunRow};
use crate::state::Status;
use crate::{RunStopper, StartPolicy, gates, is_stalled};
use chrono::{DateTime, Utc};
use news_core::NewsError;
use news_store::{PgPool, map_err};
use serde_json::{Map, Value, json};
use sqlx::{PgConnection, Row};
use uuid::Uuid;

/// The run's current step, its place and its progress (jobs of that step).
async fn position(
    conn: &mut PgConnection,
    run: &RunRow,
) -> Result<(Value, i64, i64, Value), NewsError> {
    let p = presets::preset(&run.preset);
    let steps = p.map_or(1, |p| p.steps.len() as i64);
    let cur = match (p, run.current_step.as_deref()) {
        (Some(p), Some(id)) => p.step(id),
        _ => None,
    };
    let Some((seq, decl)) = cur else {
        return Ok((Value::Null, 1, steps, Value::Null));
    };
    let status: Option<String> =
        sqlx::query_scalar("select status from run_steps where run_id = $1 and seq = $2")
            .bind(run.run_id)
            .bind(seq)
            .fetch_optional(&mut *conn)
            .await
            .map_err(map_err)?;
    let step = json!({"seq": seq, "id": decl.id, "title": decl.title, "kind": decl.kind.as_str(),
                      "status": status.unwrap_or_else(|| "pending".into())});
    let (total, done, failed): (i64, i64, i64) = sqlx::query_as(
        "select count(*), count(*) filter (where state = 'completed'),
                count(*) filter (where state in ('discarded', 'cancelled'))
           from jobs where run_id = $1 and step_seq = $2",
    )
    .bind(run.run_id)
    .bind(seq)
    .fetch_one(&mut *conn)
    .await
    .map_err(map_err)?;
    let progress = if total == 0 {
        Value::Null
    } else {
        json!({"done": done, "total": total, "failed": failed})
    };
    Ok((step, i64::from(seq), steps, progress))
}

/// The contracts' `runSummary`.
pub async fn run_summary(
    conn: &mut PgConnection,
    run: &RunRow,
    stall_minutes: i64,
    now: DateTime<Utc>,
    stopper: Option<&dyn RunStopper>,
) -> Result<Value, NewsError> {
    let (step, _, steps, progress) = position(conn, run).await?;
    let stopping = match (run.status(), stopper) {
        (Status::Pausing, Some(s)) => {
            let (jobs, since) = s.stopping(run.run_id);
            match since {
                Some(since) if !jobs.is_empty() => {
                    json!({"tasks_left": jobs.len(), "oldest_since": since, "jobs": jobs})
                }
                _ => Value::Null,
            }
        }
        _ => Value::Null,
    };
    Ok(json!({
        "run_id": run.run_id,
        "short_id": run.short_id,
        "preset": run.preset,
        "preset_name": presets::preset(&run.preset).map_or(run.preset.as_str(), |p| p.name),
        "heavy": run.heavy,
        "origin": run.origin,
        "schedule_key": run.schedule_key,
        "status": run.status,
        "step": step,
        "steps": steps,
        "progress": progress,
        "queued_at": run.queued_at,
        "started_at": run.started_at,
        "last_seen_at": run.last_seen_at,
        "stalled": is_stalled(run, stall_minutes, now),
        "paused_reason": run.paused_reason,
        "stop_mode": run.stop_mode,
        "resume_at": run.resume_at,
        "stopping": stopping,
    }))
}

/// `details.blocking` of a `queue_offer`: the run holding the slot, or the rank-0 run queued for it.
pub(crate) async fn blocking(conn: &mut PgConnection, run_id: Uuid) -> Result<Value, NewsError> {
    let run = row::load(conn, run_id, false).await?;
    let (step, step_no, steps, progress) = position(conn, &run).await?;
    Ok(
        json!({"run_id": run.run_id, "short_id": run.short_id, "preset": run.preset, "status": run.status,
              "step": step, "step_no": step_no, "steps": steps, "progress": progress,
              "started_at": run.started_at}),
    )
}

/// The `run` event's data.
pub async fn run_event(
    db: &PgPool,
    run_id: Uuid,
    previous: Option<Status>,
    wrap: Option<Value>,
    now: DateTime<Utc>,
) -> Result<Value, NewsError> {
    let mut conn = db.acquire().await.map_err(map_err)?;
    let run = row::load(&mut conn, run_id, false).await?;
    let (step, _, steps, progress) = position(&mut conn, &run).await?;
    let wrap = wrap.or_else(|| run.summary.clone().filter(|_| run.status().is_terminal()));
    let text = match &wrap {
        Some(w) => w["text"].as_str().unwrap_or_default().to_owned(),
        None => headline(&run, now),
    };
    Ok(
        json!({"run_id": run.run_id, "short_id": run.short_id, "preset": run.preset, "status": run.status,
              "previous": previous.map(Status::as_str), "step": step, "steps": steps, "progress": progress,
              "paused_reason": run.paused_reason, "resume_at": run.resume_at, "wrap": wrap, "text": text}),
    )
}

/// One line about a run.
fn headline(run: &RunRow, _now: DateTime<Utc>) -> String {
    let name = presets::preset(&run.preset).map_or(run.preset.as_str(), |p| p.name);
    match (&run.summary, run.status()) {
        (Some(s), st) if st.is_terminal() => s["text"].as_str().map_or_else(
            || format!("{name} {} {}", run.short_id, run.status),
            str::to_owned,
        ),
        (_, Status::Paused) => format!(
            "{name} {} paused ({})",
            run.short_id,
            run.paused_reason.as_deref().unwrap_or("?")
        ),
        _ => format!(
            "{name} {} {}{}",
            run.short_id,
            run.status,
            run.current_step
                .as_deref()
                .map(|s| format!(" at {s}"))
                .unwrap_or_default()
        ),
    }
}

async fn heavy_slot(conn: &mut PgConnection) -> Result<Value, NewsError> {
    let row: Option<(Uuid, String, String, String)> = sqlx::query_as(
        "select run_id, short_id, preset, status from runs where heavy and status in ('running', 'waiting_ai', 'pausing')",
    )
    .fetch_optional(&mut *conn)
    .await
    .map_err(map_err)?;
    Ok(row.map_or(
        Value::Null,
        |(id, s, p, st)| json!({"run_id": id, "short_id": s, "preset": p, "status": st}),
    ))
}

/// The Lineup (`lineup.list`): queued runs in order, with why each waits.
pub async fn lineup(
    db: &PgPool,
    policy: Option<&dyn StartPolicy>,
    now: DateTime<Utc>,
) -> Result<Value, NewsError> {
    let mut conn = db.acquire().await.map_err(map_err)?;
    let slot = heavy_slot(&mut conn).await?;
    let items = lineup_items(&mut conn, &slot, policy, now).await?;
    let mut text = String::from("LINEUP\n");
    if items.is_empty() {
        text.push_str("  Nothing queued.");
    }
    for i in &items {
        text.push_str(&format!(
            "  {}. {} {} ({}, waits for the {})\n",
            i["position"],
            i["preset"].as_str().unwrap_or("?"),
            i["short_id"].as_str().unwrap_or("?"),
            i["origin"].as_str().unwrap_or("?"),
            i["held_by"].as_str().unwrap_or("slot").replace('_', " ")
        ));
    }
    Ok(json!({"heavy_slot": slot, "items": items, "text": text.trim_end()}))
}

async fn lineup_items(
    conn: &mut PgConnection,
    slot: &Value,
    policy: Option<&dyn StartPolicy>,
    now: DateTime<Utc>,
) -> Result<Vec<Value>, NewsError> {
    let runs: Vec<RunRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "select {RUN_COLUMNS} from runs where status = 'queued' order by lineup_rank, queued_at, run_id"
    )))
    .fetch_all(&mut *conn)
    .await
    .map_err(map_err)?;
    Ok(runs
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let held = if !slot.is_null() || i > 0 {
                "slot"
            } else {
                let origin = if r.origin == "schedule" {
                    crate::Origin::Schedule
                } else {
                    crate::Origin::Board
                };
                match (policy, presets::preset(&r.preset)) {
                    (Some(pol), Some(p)) => match pol.check(p, origin, now) {
                        crate::Policy::Hold { .. } => "quiet_hours",
                        crate::Policy::Refuse { .. } => "quota",
                        crate::Policy::Allow => "slot",
                    },
                    _ => "slot",
                }
            };
            json!({"position": i + 1, "run_id": r.run_id, "short_id": r.short_id, "preset": r.preset,
                   "origin": r.origin, "schedule_key": r.schedule_key, "queued_at": r.queued_at,
                   "start_by": r.start_by, "lineup_rank": r.lineup_rank, "rejoined": r.started_at.is_some(),
                   "held_by": held})
        })
        .collect())
}

/// `run.list`: newest first. `cursor` is the offset the previous page returned.
pub async fn run_list(
    db: &PgPool,
    active: bool,
    preset: Option<&str>,
    limit: i64,
    cursor: Option<&str>,
    now: DateTime<Utc>,
) -> Result<Value, NewsError> {
    let offset: i64 = match cursor {
        None => 0,
        Some(c) => c
            .parse()
            .map_err(|_| NewsError::usage("cursor is not one this command returned"))?,
    };
    let runs: Vec<RunRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "select {RUN_COLUMNS} from runs
          where ($1 = false or status = any($2)) and ($3::text is null or preset = $3)
          order by created_at desc, run_id desc limit $4 offset $5"
    )))
    .bind(active)
    .bind(Status::LIVE.map(Status::as_str).to_vec())
    .bind(preset)
    .bind(limit + 1)
    .bind(offset)
    .fetch_all(db)
    .await
    .map_err(map_err)?;
    let more = runs.len() as i64 > limit;
    let items: Vec<Value> = runs
        .iter()
        .take(limit as usize)
        .map(|r| {
            json!({"run_id": r.run_id, "short_id": r.short_id, "preset": r.preset, "status": r.status,
                   "origin": r.origin, "created_at": r.created_at, "started_at": r.started_at,
                   "finished_at": r.finished_at, "headline": headline(r, now)})
        })
        .collect();
    let mut out = json!({"items": items});
    if more {
        out["next_cursor"] = json!((offset + limit).to_string());
    }
    Ok(out)
}

/// `run.show`: one run with its steps, gates and Wrap.
pub async fn run_show(
    db: &PgPool,
    run_ref: &str,
    now: DateTime<Utc>,
    stopper: Option<&dyn RunStopper>,
) -> Result<Value, NewsError> {
    let mut conn = db.acquire().await.map_err(map_err)?;
    let id = row::resolve(&mut conn, run_ref).await?;
    let run = row::load(&mut conn, id, false).await?;
    let standing = news_standing::current_on(&mut conn).await?;
    let summary = run_summary(
        &mut conn,
        &run,
        standing.int("chief.stall_minutes"),
        now,
        stopper,
    )
    .await?;
    let p = presets::preset(&run.preset);
    let rows = sqlx::query(
        "select seq, step, kind, status, attempt, started_at, finished_at, output from run_steps
          where run_id = $1 order by seq",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await
    .map_err(map_err)?;
    let mut steps = Vec::new();
    for r in &rows {
        let seq: i16 = r.get("seq");
        let id_: String = r.get("step");
        let progress: (i64, i64, i64) = sqlx::query_as(
            "select count(*), count(*) filter (where state = 'completed'),
                    count(*) filter (where state in ('discarded', 'cancelled'))
               from jobs where run_id = $1 and step_seq = $2",
        )
        .bind(id)
        .bind(seq)
        .fetch_one(&mut *conn)
        .await
        .map_err(map_err)?;
        let output: Option<Value> = r.get("output");
        steps.push(json!({
            "seq": seq,
            "id": id_,
            "title": p.and_then(|p| p.step(&id_)).map_or(id_.as_str(), |(_, s)| s.title),
            "kind": r.get::<String, _>("kind"),
            "status": r.get::<String, _>("status"),
            "attempt": r.get::<i16, _>("attempt"),
            "started_at": r.get::<Option<DateTime<Utc>>, _>("started_at"),
            "finished_at": r.get::<Option<DateTime<Utc>>, _>("finished_at"),
            "progress": if progress.0 == 0 { Value::Null } else { json!({"done": progress.1, "total": progress.0, "failed": progress.2}) },
            "summary": output.and_then(|o| o.get("summary").cloned()).filter(|s| s.get("text").is_some()),
        }));
    }
    if steps.is_empty()
        && let Some(p) = p
    {
        for (i, s) in p.steps.iter().enumerate() {
            steps.push(json!({"seq": i + 1, "id": s.id, "title": s.title, "kind": s.kind.as_str(),
                              "status": "pending", "attempt": 0, "started_at": null, "finished_at": null,
                              "progress": null, "summary": null}));
        }
    }
    let gate_rows = sqlx::query(
        "select g.gate, g.round, s.step, g.opened_at, g.answered_at, g.subject_sha, g.summary, g.preview,
                a.decision, a.note
           from gates g join run_steps s on s.run_id = g.run_id and s.seq = g.step_seq
           left join approvals a on a.run_id = g.run_id and a.gate = g.gate and a.round = g.round
          where g.run_id = $1 order by g.opened_at, g.gate",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await
    .map_err(map_err)?;
    let gates: Vec<Value> = gate_rows
        .iter()
        .map(|g| {
            json!({"gate": g.get::<String, _>("gate"), "round": g.get::<i16, _>("round"),
                   "step": g.get::<String, _>("step"), "opened_at": g.get::<DateTime<Utc>, _>("opened_at"),
                   "answered_at": g.get::<Option<DateTime<Utc>>, _>("answered_at"),
                   "subject_sha": gates::hex(&g.get::<Vec<u8>, _>("subject_sha")),
                   "summary": g.get::<Value, _>("summary"), "preview": g.get::<Option<Value>, _>("preview"),
                   "decision": g.get::<Option<String>, _>("decision"), "note": g.get::<Option<String>, _>("note")})
        })
        .collect();
    let wrap = run.summary.clone().filter(|_| run.status().is_terminal());
    let text = headline(&run, now);
    Ok(
        json!({"run": summary, "args": run.args, "settings_version": run.settings_version, "budget": run.budget,
              "steps": steps, "gates": gates, "wrap": wrap, "text": text}),
    )
}

/// The service's state as the status board shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceState {
    Starting,
    WaitingForDatabase,
    Recovering,
    Up,
    Stopping,
    Down,
}

impl ServiceState {
    pub fn as_str(self) -> &'static str {
        match self {
            ServiceState::Starting => "starting",
            ServiceState::WaitingForDatabase => "waiting_for_database",
            ServiceState::Recovering => "recovering",
            ServiceState::Up => "up",
            ServiceState::Stopping => "stopping",
            ServiceState::Down => "down",
        }
    }
}

/// Facts the next-action rules read.
#[derive(Debug, Clone, Default)]
pub struct Facts {
    pub stalled_heavy: Option<(Uuid, String)>,
    pub open_gate: Option<(Uuid, String, String)>,
    pub form_waiting: Option<(Uuid, String)>,
    pub dead_since_salvage: i64,
    pub dead_jobs_min: i64,
    pub rollcall_due_unscheduled: bool,
    pub huddle_due_unscheduled: bool,
}

/// One next-action rule: its id (the contract's `rule`) and when it fires.
pub struct Rule {
    pub id: &'static str,
    pub fire: fn(&Facts) -> Option<Value>,
}

fn next(
    action: &str,
    preset: Option<&str>,
    run: Option<&(Uuid, String)>,
    rule: &str,
    reason: String,
) -> Value {
    json!({"action": action, "preset": preset, "run_id": run.map(|r| r.0), "short_id": run.map(|r| &r.1),
           "args": if action == "run.start" { json!({}) } else { Value::Null }, "rule": rule, "reason": reason})
}

/// Bureau §8.1's next-action rules, first match wins. Rules 2, 4, 5, 6 and 8 read stories, the Diary,
/// sources, the home curation and deferred revisions, which land with #9, #12, #15 and #20; until then
/// their facts are absent and they never fire.
pub const RULES: &[Rule] = &[
    Rule {
        id: "stalled_heavy",
        fire: |f| {
            f.stalled_heavy.as_ref().map(|r| {
                next(
                    "run.pause",
                    None,
                    Some(r),
                    "stalled_heavy",
                    format!("{} has stalled: pause it (at once), then resume it", r.1),
                )
            })
        },
    },
    Rule {
        id: "tier1_waiting",
        fire: |_| None,
    },
    Rule {
        id: "gate_open",
        fire: |f| {
            f.open_gate.as_ref().map(|(id, short, gate)| {
                next(
                    "open.run",
                    None,
                    Some(&(*id, short.clone())),
                    "gate_open",
                    format!("{short} waits for your answer at gate {gate}"),
                )
            })
        },
    },
    Rule {
        id: "diary_source_red",
        fire: |_| None,
    },
    Rule {
        id: "curation_stale",
        fire: |_| None,
    },
    Rule {
        id: "stories_waiting",
        fire: |_| None,
    },
    Rule {
        id: "dead_jobs",
        fire: |f| {
            (f.dead_since_salvage >= f.dead_jobs_min).then(|| {
                next(
                    "run.start",
                    Some("salvage"),
                    None,
                    "dead_jobs",
                    format!("{} dead jobs since the last Salvage", f.dead_since_salvage),
                )
            })
        },
    },
    Rule {
        id: "deferred_revisions",
        fire: |_| None,
    },
    Rule {
        id: "huddle_questions",
        fire: |f| {
            f.form_waiting.as_ref().map(|r| {
                next(
                    "open.form",
                    Some("huddle"),
                    Some(r),
                    "huddle_questions",
                    "Huddle questions are waiting for your answers".into(),
                )
            })
        },
    },
    Rule {
        id: "frontpage_due",
        fire: |_| None,
    },
    Rule {
        id: "rollcall_due",
        fire: |f| {
            f.rollcall_due_unscheduled.then(|| {
                next(
                    "run.start",
                    Some("rollcall"),
                    None,
                    "rollcall_due",
                    "Rollcall is due and not scheduled".into(),
                )
            })
        },
    },
];

/// The first rule that fires, else "nothing due".
pub fn next_action(f: &Facts) -> Value {
    RULES
        .iter()
        .find_map(|r| (r.fire)(f))
        .unwrap_or_else(|| next("none", None, None, "nothing_due", "Nothing is due.".into()))
}

/// Due state of a preset run every so many days, from its last successful run.
async fn cadence(
    conn: &mut PgConnection,
    preset: &str,
    every: i64,
    now: DateTime<Utc>,
) -> Result<Value, NewsError> {
    let last: Option<(String, DateTime<Utc>)> = sqlx::query_as(
        "select short_id, finished_at from runs where preset = $1 and status = 'succeeded'
          order by finished_at desc limit 1",
    )
    .bind(preset)
    .fetch_optional(&mut *conn)
    .await
    .map_err(map_err)?;
    let age = last
        .as_ref()
        .map(|l| (now - l.1).num_minutes() as f64 / 1440.0);
    let due = age.is_none_or(|a| a >= every as f64);
    Ok(
        json!({"last_run": last.map(|l| l.0), "age_days": age.map(|a| (a * 10.0).round() / 10.0),
              "every_days": every, "state": if due { "due" } else { "not_due" }}),
    )
}

fn local(t: DateTime<Utc>, tz: chrono_tz::Tz) -> String {
    t.with_timezone(&tz).format("%a %d %b %H:%M").to_string()
}

/// The status board (`status`, `news run status`).
pub async fn status(
    db: &PgPool,
    service: ServiceState,
    policy: Option<&dyn StartPolicy>,
    stopper: Option<&dyn RunStopper>,
    now: DateTime<Utc>,
) -> Result<Value, NewsError> {
    let mut conn = db.acquire().await.map_err(map_err)?;
    let standing = news_standing::current_on(&mut conn).await?;
    let tz = news_standing::time_zone(&standing);
    let stall = standing.int("chief.stall_minutes");
    let slot = heavy_slot(&mut conn).await?;
    let live: Vec<RunRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "select {RUN_COLUMNS} from runs where status in ('running', 'waiting_ai', 'pausing', 'waiting_approval', 'paused')
          order by started_at nulls last, run_id"
    )))
    .fetch_all(&mut *conn)
    .await
    .map_err(map_err)?;
    let mut facts = Facts {
        dead_jobs_min: standing.int("next.dead_jobs_min"),
        ..Facts::default()
    };
    let mut running = Vec::new();
    let mut waiting = Vec::new();
    let mut text_running = Vec::new();
    let mut text_waiting = Vec::new();
    for r in &live {
        match r.status() {
            Status::Running | Status::WaitingAi | Status::Pausing => {
                let s = run_summary(&mut conn, r, stall, now, stopper).await?;
                if r.heavy && s["stalled"] == true && facts.stalled_heavy.is_none() {
                    facts.stalled_heavy = Some((r.run_id, r.short_id.clone()));
                }
                text_running.push(format!(
                    "  {} {:<10} {} step {} of {}{}",
                    s["preset_name"].as_str().unwrap_or("?"),
                    r.short_id,
                    r.status,
                    s["step"]["seq"].as_i64().unwrap_or(1),
                    s["steps"],
                    if s["stalled"] == true {
                        " (stalled)"
                    } else {
                        ""
                    }
                ));
                running.push(s);
            }
            Status::WaitingApproval => {
                let gate = sqlx::query(
                    "select g.gate, g.round, s.step, g.opened_at, g.subject_sha, g.summary, g.preview
                       from gates g join run_steps s on s.run_id = g.run_id and s.seq = g.step_seq
                      where g.run_id = $1 and g.answered_at is null order by g.opened_at, g.gate limit 1",
                )
                .bind(r.run_id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(map_err)?;
                let (kind, gate_v, summary, since) = match gate {
                    Some(g) => {
                        let name: String = g.get("gate");
                        if facts.open_gate.is_none() {
                            facts.open_gate = Some((r.run_id, r.short_id.clone(), name.clone()));
                        }
                        let summary: Value = g.get("summary");
                        let since: DateTime<Utc> = g.get("opened_at");
                        let gv = json!({"run_id": r.run_id, "short_id": r.short_id, "preset": r.preset,
                            "gate": name, "round": g.get::<i16, _>("round"), "step": g.get::<String, _>("step"),
                            "since": since, "subject_sha": gates::hex(&g.get::<Vec<u8>, _>("subject_sha")),
                            "summary": summary, "preview": g.get::<Option<Value>, _>("preview"),
                            "actions": ["gate.approve", "gate.changes", "gate.reject"]});
                        ("gate", gv, summary, since)
                    }
                    None => {
                        if r.preset == "huddle" && facts.form_waiting.is_none() {
                            facts.form_waiting = Some((r.run_id, r.short_id.clone()));
                        }
                        let since = r.last_seen_at.or(r.started_at).unwrap_or(r.created_at);
                        (
                            "form",
                            Value::Null,
                            json!({"text": "A form waits for your answers"}),
                            since,
                        )
                    }
                };
                text_waiting.push(format!(
                    "  {} {:<10} {} since {}\n                      {}",
                    r.preset,
                    r.short_id,
                    if kind == "gate" {
                        format!("gate {}", gate_v["gate"].as_str().unwrap_or("?"))
                    } else {
                        "form".into()
                    },
                    local(since, tz),
                    summary["text"].as_str().unwrap_or("")
                ));
                waiting.push(json!({"kind": kind, "run_id": r.run_id, "short_id": r.short_id, "preset": r.preset,
                    "since": since, "summary": summary, "gate": gate_v, "paused_reason": null, "resume_at": null,
                    "actions": if kind == "gate" { json!(["gate.approve", "gate.changes", "gate.reject", "run.cancel"]) } else { json!(["run.cancel"]) }}));
            }
            Status::Paused => {
                let since = r.last_seen_at.or(r.started_at).unwrap_or(r.created_at);
                let reason = r.paused_reason.clone().unwrap_or_default();
                let text = match (&*reason, r.resume_at) {
                    ("usage_limit", Some(t)) => format!(
                        "Paused for the usage limit; resumes on its own at {}",
                        local(t, tz)
                    ),
                    ("quota", _) => {
                        "Paused at the quota reserve; resumes when usage falls".to_owned()
                    }
                    _ => "Paused by you".to_owned(),
                };
                text_waiting.push(format!("  {} {:<10} {}", r.preset, r.short_id, text));
                waiting.push(json!({"kind": "paused", "run_id": r.run_id, "short_id": r.short_id, "preset": r.preset,
                    "since": since, "summary": {"text": text}, "gate": null, "paused_reason": r.paused_reason,
                    "resume_at": r.resume_at, "actions": ["run.resume", "run.cancel"]}));
            }
            _ => {}
        }
    }
    let queued = lineup_items(&mut conn, &slot, policy, now).await?;

    // Due: schedules (next slot each), Huddle and Rollcall cadence.
    let mut schedules = Vec::new();
    for (key, s) in standing.schedules() {
        if !s.enabled {
            continue;
        }
        if let Ok(at) = news_standing::next_slot(&s.cron, tz, now) {
            schedules.push(json!({"schedule_key": key, "preset": s.preset, "next_at": at}));
        }
    }
    schedules.sort_by(|a, b| a["next_at"].as_str().cmp(&b["next_at"].as_str()));
    let scheduled = |preset: &str| schedules.iter().any(|s| s["preset"] == preset);
    let huddle = cadence(
        &mut conn,
        "huddle",
        standing.int("board.huddle_every_days"),
        now,
    )
    .await?;
    let rollcall = cadence(
        &mut conn,
        "rollcall",
        standing.int("board.rollcall_every_days"),
        now,
    )
    .await?;
    facts.rollcall_due_unscheduled = rollcall["state"] == "due" && !scheduled("rollcall");
    facts.huddle_due_unscheduled = huddle["state"] == "due" && !scheduled("huddle");

    // Problems: dead jobs since the last Salvage, failed or skipped runs in 7 days, refusals.
    let last_salvage: Option<(String, DateTime<Utc>)> = sqlx::query_as(
        "select short_id, finished_at from runs where preset = 'salvage' and status = 'succeeded'
          order by finished_at desc limit 1",
    )
    .fetch_optional(&mut *conn)
    .await
    .map_err(map_err)?;
    let dead: Vec<(String, i64)> = sqlx::query_as(
        "select coalesce(error_class, 'unknown'), count(*) from jobs
          where state = 'discarded' and ($1::timestamptz is null or finished_at > $1)
          group by 1 order by 1",
    )
    .bind(last_salvage.as_ref().map(|s| s.1))
    .fetch_all(&mut *conn)
    .await
    .map_err(map_err)?;
    facts.dead_since_salvage = dead.iter().map(|d| d.1).sum();
    let by_class: Map<String, Value> = dead.iter().map(|(c, n)| (c.clone(), json!(n))).collect();
    let failed: Vec<Value> = sqlx::query(
        "select run_id, short_id, preset, status, finished_at, summary ->> 'reason' as reason from runs
          where status in ('failed', 'skipped') and finished_at > $1 - interval '7 days'
          order by finished_at desc limit 20",
    )
    .bind(now)
    .fetch_all(&mut *conn)
    .await
    .map_err(map_err)?
    .iter()
    .map(|r| {
        json!({"run_id": r.get::<Uuid, _>("run_id"), "short_id": r.get::<String, _>("short_id"),
               "preset": r.get::<String, _>("preset"), "status": r.get::<String, _>("status"),
               "finished_at": r.get::<DateTime<Utc>, _>("finished_at"), "reason": r.get::<Option<String>, _>("reason")})
    })
    .collect();
    let refused: Vec<Value> = sqlx::query(
        "select at, cmd, refusal, pid, image, run_id from hotline_refusals
          where at > $1 - interval '7 days' order by at desc limit 20",
    )
    .bind(now)
    .fetch_all(&mut *conn)
    .await
    .map_err(map_err)?
    .iter()
    .map(|r| {
        json!({"at": r.get::<DateTime<Utc>, _>("at"), "cmd": r.get::<String, _>("cmd"),
               "refusal": r.get::<String, _>("refusal"), "pid": r.get::<Option<i32>, _>("pid"),
               "image": r.get::<Option<String>, _>("image"), "run_id": r.get::<Option<Uuid>, _>("run_id")})
    })
    .collect();
    let usage = policy.map_or_else(|| crate::AllowAll.usage(), |p| p.usage());
    let next_v = next_action(&facts);

    // Text.
    let mut t = format!(
        "Bureau · {} · service {} · settings v{}\n\nRUNNING\n",
        now.with_timezone(&tz).format("%a %d %b %Y %H:%M %Z"),
        service.as_str(),
        standing.version
    );
    if text_running.is_empty() {
        t.push_str(if slot.is_null() {
            "  Nothing running. Heavy slot free.\n"
        } else {
            "  Nothing running.\n"
        });
    } else {
        t.push_str(&(text_running.join("\n") + "\n"));
    }
    t.push_str("\nWAITING FOR YOU\n");
    t.push_str(&if text_waiting.is_empty() {
        "  Nothing waiting.\n".to_owned()
    } else {
        text_waiting.join("\n") + "\n"
    });
    t.push_str("\nQUEUED\n");
    if queued.is_empty() {
        t.push_str("  Nothing queued.\n");
    }
    for q in &queued {
        t.push_str(&format!(
            "  {}. {} {}{}\n",
            q["position"],
            q["preset"].as_str().unwrap_or("?"),
            q["short_id"].as_str().unwrap_or("?"),
            q["start_by"]
                .as_str()
                .map(|s| format!(" (start by {s})"))
                .unwrap_or_default()
        ));
    }
    t.push_str("\nDUE\n");
    let sched: Vec<String> = schedules
        .iter()
        .filter_map(|s| {
            let at: DateTime<Utc> = s["next_at"].as_str()?.parse().ok()?;
            Some(format!("{} {}", s["preset"].as_str()?, local(at, tz)))
        })
        .collect();
    t.push_str(&format!(
        "  Schedules           {}\n",
        if sched.is_empty() {
            "none set".to_owned()
        } else {
            sched.join(" · ")
        }
    ));
    for (name, c) in [("Huddle", &huddle), ("Rollcall", &rollcall)] {
        t.push_str(&format!(
            "  {name:<20}{}\n",
            match c["last_run"].as_str() {
                Some(l) => format!(
                    "last {l}, {} days ago: {}",
                    c["age_days"],
                    c["state"].as_str().unwrap_or("").replace('_', " ")
                ),
                None => "never run: due".to_owned(),
            }
        ));
    }
    t.push_str("\nPROBLEMS\n");
    let dead_text: Vec<String> = dead.iter().map(|(c, n)| format!("{c} {n}")).collect();
    t.push_str(&format!(
        "  Dead jobs           {}\n",
        if dead_text.is_empty() {
            "none".to_owned()
        } else {
            format!(
                "{} since {}: {}",
                facts.dead_since_salvage,
                last_salvage.as_ref().map_or("the start", |s| s.0.as_str()),
                dead_text.join(", ")
            )
        }
    ));
    t.push_str(&format!(
        "  Failed runs         {}\n",
        if failed.is_empty() {
            "none in 7 days".to_owned()
        } else {
            format!("{} in 7 days", failed.len())
        }
    ));
    if !refused.is_empty() {
        t.push_str(&format!(
            "  Refused control     {} in 7 days\n",
            refused.len()
        ));
    }
    t.push_str(&format!(
        "  Usage               {}\n",
        if usage["known"] == true {
            format!("Bureau today {} tokens", usage["bureau_today"])
        } else {
            "usage unknown".to_owned()
        }
    ));
    t.push_str(&format!(
        "\nNEXT\n  {}",
        next_v["reason"].as_str().unwrap_or("")
    ));

    Ok(json!({
        "as_of": now,
        "time_zone": tz.name(),
        "service": {"state": service.as_str(), "settings_version": standing.version},
        "heavy_slot": slot,
        "running": running,
        "waiting": waiting,
        "queued": queued,
        "due": {"schedules": schedules, "diary": [], "stories_waiting": null, "frontpage": null,
                "huddle": huddle, "rollcall": rollcall},
        "problems": {"sources": [], "dead_jobs": {"total": facts.dead_since_salvage, "by_class": by_class,
                     "since_salvage": last_salvage.map(|s| s.0)},
                     "failed_runs": failed, "refused_control": refused, "usage": usage},
        "next": next_v,
        "text": t,
    }))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn next_action_rules_take_the_first_match() {
        let id = Uuid::nil();
        let mut f = Facts {
            dead_jobs_min: 10,
            ..Facts::default()
        };
        assert_eq!(next_action(&f)["rule"], "nothing_due");
        f.rollcall_due_unscheduled = true;
        assert_eq!(next_action(&f)["rule"], "rollcall_due");
        f.dead_since_salvage = 12;
        assert_eq!(next_action(&f)["rule"], "dead_jobs");
        assert_eq!(next_action(&f)["preset"], "salvage");
        f.open_gate = Some((id, "r_abcd1234".into(), "plan".into()));
        assert_eq!(next_action(&f)["action"], "open.run");
        f.stalled_heavy = Some((id, "r_abcd1234".into()));
        let n = next_action(&f);
        assert_eq!(
            (n["action"].as_str(), n["rule"].as_str()),
            (Some("run.pause"), Some("stalled_heavy"))
        );
        let ids: Vec<&str> = RULES.iter().map(|r| r.id).collect();
        assert_eq!(ids.len(), 11);
    }
}
