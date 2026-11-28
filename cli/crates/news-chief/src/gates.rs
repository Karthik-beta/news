//! Gates, approvals and the G-tier rule (ADR 0008 §6.3).
//!
//! An approval step opens gates holding the SHA-256 of exactly what Board shows; `gate.approve`,
//! `gate.changes` and `gate.reject` must name that hash, and the approval's foreign key covers it, so
//! the database itself refuses an approval for a subject the gate does not hold. [`authorize`] is the
//! one check every G-tier command passes, inside the caller's transaction.

use crate::presets::{self, Cover, GCommand};
use crate::row::{self, RunRow};
use crate::state::Status;
use crate::{Chief, LINEUP_LOCK};
use chrono::{DateTime, Utc};
use news_core::NewsError;
use news_store::map_err;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, Row};
use uuid::Uuid;

/// A gate an approval step opens.
#[derive(Debug, Clone, PartialEq)]
pub struct GateOpen {
    /// `plan`, or a per-item gate `report:81`.
    pub gate: String,
    /// What Board shows and Karthik approves (plan, draft, diff). Its `items`, if any, are what the
    /// approval covers.
    pub subject: Value,
    /// `{"text": …, …figures}`, code-rendered.
    pub summary: Value,
    pub preview: Option<Value>,
}

/// SHA-256 of the subject's JSON bytes (keys sorted, as serde_json's map is).
pub fn subject_sha(subject: &Value) -> [u8; 32] {
    let bytes = serde_json::to_vec(subject).unwrap_or_default();
    Sha256::digest(&bytes).into()
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if s.len() != 64 {
        return None;
    }
    (0..32)
        .map(|i| u8::from_str_radix(s.get(i * 2..i * 2 + 2)?, 16).ok())
        .collect()
}

/// The latest round among the gates of a step (0 when none was opened).
pub(crate) async fn latest_round(
    conn: &mut PgConnection,
    run_id: Uuid,
    step_seq: i16,
) -> Result<i16, NewsError> {
    sqlx::query_scalar(
        "select coalesce(max(round), 0)::smallint from gates where run_id = $1 and step_seq = $2",
    )
    .bind(run_id)
    .bind(step_seq)
    .fetch_one(&mut *conn)
    .await
    .map_err(map_err)
}

/// Open `gates` at `round` (idempotent: re-opening after a restart is a no-op).
pub(crate) async fn open(
    conn: &mut PgConnection,
    run_id: Uuid,
    step_seq: i16,
    round: i16,
    gates: &[GateOpen],
    now: DateTime<Utc>,
) -> Result<(), NewsError> {
    for g in gates {
        let mut summary = g.summary.clone();
        if let (Some(m), Some(items)) = (summary.as_object_mut(), g.subject.get("items")) {
            m.insert("subject_items".into(), items.clone());
        }
        sqlx::query(
            "insert into gates (run_id, gate, round, step_seq, subject_sha, summary, preview, opened_at)
             values ($1, $2, $3, $4, $5, $6, $7, $8) on conflict do nothing",
        )
        .bind(run_id)
        .bind(&g.gate)
        .bind(round)
        .bind(step_seq)
        .bind(subject_sha(&g.subject).to_vec())
        .bind(summary)
        .bind(&g.preview)
        .bind(now)
        .execute(&mut *conn)
        .await
        .map_err(map_err)?;
    }
    Ok(())
}

/// Decisions on the gates of `step_seq` in `round`, or `None` while any of them is still open.
pub(crate) async fn decisions(
    conn: &mut PgConnection,
    run_id: Uuid,
    step_seq: i16,
    round: i16,
) -> Result<Option<Vec<(String, String, Option<String>)>>, NewsError> {
    let rows = sqlx::query(
        "select g.gate, a.decision, a.note from gates g
           left join approvals a on a.run_id = g.run_id and a.gate = g.gate and a.round = g.round
          where g.run_id = $1 and g.step_seq = $2 and g.round = $3 order by g.gate",
    )
    .bind(run_id)
    .bind(step_seq)
    .bind(round)
    .fetch_all(&mut *conn)
    .await
    .map_err(map_err)?;
    let mut out = Vec::new();
    for r in rows {
        match r.get::<Option<String>, _>("decision") {
            Some(d) => out.push((r.get("gate"), d, r.get("note"))),
            None => return Ok(None),
        }
    }
    Ok(Some(out))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Approved,
    Changes,
    Rejected,
}

impl Decision {
    pub fn as_str(self) -> &'static str {
        match self {
            Decision::Approved => "approved",
            Decision::Changes => "changes",
            Decision::Rejected => "rejected",
        }
    }
}

/// A gate answer from Board.
#[derive(Debug, Clone)]
pub struct Answer<'a> {
    pub run: &'a str,
    pub gate: &'a str,
    pub round: i16,
    pub subject_sha: &'a str,
    pub decision: Decision,
    pub note: Option<&'a str>,
}

impl Chief {
    /// `gate.approve | changes | reject` (ADR 0008 §6.3). The gate must be open and in `round`
    /// (`precondition`), and `subject_sha` must be the gate's (`gated`). When the step's last open
    /// gate is answered, the run continues, or joins the Lineup when heavy and the slot is busy.
    pub async fn answer(&self, a: Answer<'_>) -> Result<(RunRow, Status), NewsError> {
        let now = self.now();
        if a.decision == Decision::Changes && a.note.is_none_or(|n| n.trim().is_empty()) {
            return Err(NewsError::validation("Request changes needs a note"));
        }
        let sent = unhex(a.subject_sha)
            .ok_or_else(|| NewsError::usage("subject_sha must be 64 hex digits"))?;
        let mut tx = self.db().begin().await.map_err(map_err)?;
        sqlx::query(LINEUP_LOCK)
            .execute(&mut *tx)
            .await
            .map_err(map_err)?;
        let id = row::resolve(&mut tx, a.run).await?;
        let run = row::load(&mut tx, id, true).await?;
        let gate = sqlx::query(
            "select round, step_seq, subject_sha, answered_at from gates
              where run_id = $1 and gate = $2 and answered_at is null for update",
        )
        .bind(id)
        .bind(a.gate)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_err)?
        .ok_or_else(|| {
            NewsError::precondition(format!("{} has no open gate {:?}", run.short_id, a.gate))
        })?;
        let round: i16 = gate.get("round");
        let step_seq: i16 = gate.get("step_seq");
        let held: Vec<u8> = gate.get("subject_sha");
        if run.status() != Status::WaitingApproval {
            return Err(NewsError::precondition(format!(
                "{} is {}, not waiting for approval",
                run.short_id, run.status
            )));
        }
        if round != a.round {
            return Err(NewsError::precondition(format!(
                "gate {} is in round {round}, not {}",
                a.gate, a.round
            )));
        }
        if held != sent {
            return Err(NewsError::gated(format!(
                "gate {} holds a different subject: what you saw has changed",
                a.gate
            ))
            .with_details(json!({"reason": "subject_changed", "subject_sha": hex(&held)}))
            .with_text("What you approved has changed since Board showed it. Open the run again to see the current version."));
        }
        sqlx::query(
            "insert into approvals (run_id, gate, round, decision, note, decided_at, subject_sha, origin)
             values ($1, $2, $3, $4, $5, $6, $7, 'board')",
        )
        .bind(id)
        .bind(a.gate)
        .bind(round)
        .bind(a.decision.as_str())
        .bind(a.note)
        .bind(now)
        .bind(&held)
        .execute(&mut *tx)
        .await
        .map_err(map_err)?;
        sqlx::query(
            "update gates set answered_at = $3 where run_id = $1 and gate = $2 and round = $4",
        )
        .bind(id)
        .bind(a.gate)
        .bind(now)
        .bind(round)
        .execute(&mut *tx)
        .await
        .map_err(map_err)?;
        let open_left: i64 = sqlx::query_scalar(
            "select count(*) from gates where run_id = $1 and step_seq = $2 and answered_at is null",
        )
        .bind(id)
        .bind(step_seq)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_err)?;
        let status = if open_left == 0 {
            self.continue_run(&mut tx, &run, Status::WaitingApproval, now)
                .await?
                .0
        } else {
            Status::WaitingApproval
        };
        tx.commit().await.map_err(map_err)?;
        self.emit_raw(
            "gate",
            json!({"run_id": id, "short_id": run.short_id, "preset": run.preset, "gate": a.gate, "round": round,
                   "state": "answered", "subject_sha": hex(&held), "decision": a.decision.as_str(),
                   "text": format!("{} {}: gate {} {}", run.preset, run.short_id, a.gate, a.decision.as_str())}),
        );
        if status != Status::WaitingApproval {
            self.emit_run_pub(id, Some(Status::WaitingApproval), None)
                .await;
        }
        Ok((run, status))
    }

    /// `form.submit`: the answer to the run's open form step (Huddle's calibration). The run must wait
    /// at the form whose step handler is `form`, in `round`, not yet answered (`precondition`), and
    /// `form_sha` must be the hash of the form Board showed (`gated`). The answer is kept in the step's
    /// output and the run continues; the step's handler reads it when Chief enters the step again. The
    /// Hotline has already checked `answer` against the form's contract.
    pub async fn submit_form(&self, a: FormAnswer<'_>) -> Result<(RunRow, Status), NewsError> {
        let now = self.now();
        let mut tx = self.db().begin().await.map_err(map_err)?;
        sqlx::query(LINEUP_LOCK)
            .execute(&mut *tx)
            .await
            .map_err(map_err)?;
        let id = row::resolve(&mut tx, a.run).await?;
        let run = row::load(&mut tx, id, true).await?;
        let open: Option<(i16, Option<Value>)> = sqlx::query_as(
            "select seq, output from run_steps where run_id = $1 and kind = 'form' and status = 'running'
              for update",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_err)?;
        let (seq, output) = open
            .filter(|_| run.status() == Status::WaitingApproval)
            .ok_or_else(|| {
                NewsError::precondition(format!(
                    "{} is {}, not waiting at a form",
                    run.short_id, run.status
                ))
            })?;
        let handler = presets::preset(&run.preset)
            .and_then(|p| p.step_at(seq))
            .map_or("", |s| s.handler);
        let mut output = output.unwrap_or_else(|| json!({}));
        let summary = &output["summary"];
        let round = summary["round"].as_i64().unwrap_or(0);
        if handler != a.form || output.get("answer").is_some() || round != a.round {
            return Err(NewsError::precondition(format!(
                "{} waits at form {handler} round {round}{}, not {} round {}",
                run.short_id,
                if output.get("answer").is_some() {
                    " (answered)"
                } else {
                    ""
                },
                a.form,
                a.round
            )));
        }
        let held = summary["form_sha"].as_str().unwrap_or_default().to_owned();
        if held != a.form_sha {
            return Err(NewsError::gated(format!(
                "form {handler} holds a different form: what you saw has changed"
            ))
            .with_details(json!({"reason": "subject_changed", "form_sha": held}))
            .with_text("The questions have changed since Board showed them. Open the run again to see the current form."));
        }
        // Pairwise labels answer only the pairs the form asked, so a bad answer is refused here, not
        // when the step stores it.
        if let (Some(asked), Some(labels)) = (
            summary["form"]["questions"].as_array(),
            a.answer["labels"].as_array(),
        ) {
            let pair = |x: &Value, y: &Value| Some((x.as_i64()?, y.as_i64()?));
            let asked: std::collections::HashSet<(i64, i64)> = asked
                .iter()
                .filter_map(|q| pair(&q["a"]["story_id"], &q["b"]["story_id"]))
                .map(|(x, y)| (x.min(y), x.max(y)))
                .collect();
            for l in labels {
                match pair(&l["a"], &l["b"]) {
                    Some((x, y)) if x != y && asked.contains(&(x.min(y), x.max(y))) => {}
                    _ => {
                        return Err(NewsError::validation(format!(
                            "label {l} is not one of the form's questions"
                        )));
                    }
                }
            }
        }
        if let Some(o) = output.as_object_mut() {
            o.insert("answer".into(), a.answer);
            o.insert("answered_at".into(), json!(now));
        }
        sqlx::query("update run_steps set output = $3 where run_id = $1 and seq = $2")
            .bind(id)
            .bind(seq)
            .bind(&output)
            .execute(&mut *tx)
            .await
            .map_err(map_err)?;
        let (status, _) = self
            .continue_run(&mut tx, &run, Status::WaitingApproval, now)
            .await?;
        tx.commit().await.map_err(map_err)?;
        self.emit_run_pub(id, Some(Status::WaitingApproval), None)
            .await;
        Ok((run, status))
    }
}

/// A form answer from Board (`form.submit`).
#[derive(Debug, Clone)]
pub struct FormAnswer<'a> {
    pub run: &'a str,
    /// The form step's handler, e.g. `huddle.calibration`.
    pub form: &'a str,
    pub round: i64,
    pub form_sha: &'a str,
    pub answer: Value,
}

/// A G-tier request: the command, the run and step running it, and the item it acts on.
#[derive(Debug, Clone, Copy)]
pub struct GRequest<'a> {
    pub run_id: Uuid,
    pub step_seq: i16,
    pub command: GCommand,
    pub item: Option<&'a str>,
}

/// Proof that a G-tier command may run, borrowing the transaction that checked it, so whatever it
/// authorises (Press's outbox insert takes `&mut Authorized`) is written in that same transaction.
pub struct Authorized<'t> {
    conn: &'t mut PgConnection,
    pub run_id: Uuid,
    pub step_seq: i16,
    pub command: GCommand,
    /// The item this authorisation was checked for (`GRequest::item`); `None` means a step-wide check.
    pub item: Option<String>,
}

impl Authorized<'_> {
    pub fn conn(&mut self) -> &mut PgConnection {
        self.conn
    }
}

fn gated(why: &str) -> NewsError {
    NewsError::gated(format!("not authorised: {why}"))
        .with_hint("G-tier commands run only as the authorised step of a live run (Bureau §5.5)")
}

/// The G-tier rule (Bureau §5.5, ADR 0008 §6.3): `gated` unless (1) the run is `running` and the step
/// is its current running step and declares `command`; (2) the item is in the run's Budget (a start
/// cover) or in the approved subject (a gate cover); (3) the cover holds.
pub async fn authorize<'t>(
    conn: &'t mut PgConnection,
    req: GRequest<'_>,
) -> Result<Authorized<'t>, NewsError> {
    let run: Option<(String, String, Option<String>, Value, String)> = sqlx::query_as(
        "select status, preset, current_step, budget, origin from runs where run_id = $1 for share",
    )
    .bind(req.run_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(map_err)?;
    let Some((status, preset, current, budget, origin)) = run else {
        return Err(gated("no such run"));
    };
    if status != "running" {
        return Err(gated(&format!("the run is {status}, not running")));
    }
    let p = presets::preset(&preset).ok_or_else(|| gated("unknown preset"))?;
    let step = p
        .step_at(req.step_seq)
        .ok_or_else(|| gated("no such step"))?;
    let step_status: Option<String> =
        sqlx::query_scalar("select status from run_steps where run_id = $1 and seq = $2")
            .bind(req.run_id)
            .bind(req.step_seq)
            .fetch_optional(&mut *conn)
            .await
            .map_err(map_err)?;
    if current.as_deref() != Some(step.id) || step_status.as_deref() != Some("running") {
        return Err(gated("the step is not the run's current running step"));
    }
    if !step.g_tier.contains(&req.command) {
        return Err(gated(&format!(
            "step {} does not declare {}",
            step.id,
            req.command.cli()
        )));
    }
    let in_budget = |item: &str| {
        budget["items"].as_array().is_some_and(|l| {
            l.iter().any(|x| {
                x.as_str() == Some(item) || x.as_i64().is_some_and(|n| n.to_string() == item)
            })
        })
    };
    let covered_by_subject =
        |summary: &Value, item: Option<&str>| match (item, summary.get("subject_items")) {
            (Some(i), Some(Value::Array(l))) => l
                .iter()
                .any(|x| x.as_str() == Some(i) || x.as_i64().is_some_and(|n| n.to_string() == i)),
            _ => true,
        };
    match step.cover {
        Cover::None => return Err(gated("the step has no cover")),
        Cover::Start => {
            if !matches!(origin.as_str(), "board" | "schedule") {
                return Err(gated("the run was not started from Board or a schedule"));
            }
            if req.item.is_some_and(|i| !in_budget(i)) {
                return Err(gated("the item is not in the run's Budget"));
            }
        }
        Cover::Gate(g) => match approved_summary(&mut *conn, req.run_id, g).await? {
            Some(s) if covered_by_subject(&s, req.item) => {}
            Some(_) => return Err(gated("the item is not in the approved subject")),
            None => return Err(gated(&format!("no unused approval of gate {g}"))),
        },
        Cover::GateOrStart(g) => match approved_summary(&mut *conn, req.run_id, g).await? {
            Some(s) if covered_by_subject(&s, req.item) => {}
            Some(_) => return Err(gated("the item is not in the approved subject")),
            None => {
                // The approval step recorded Skip (e.g. review_plan off): the start covers it.
                let skipped: bool = sqlx::query_scalar(
                    "select exists (select 1 from run_steps s where s.run_id = $1 and s.status = 'skipped'
                                      and s.step = $2)",
                )
                .bind(req.run_id)
                .bind(p.steps.iter().find(|s| s.gate == Some(g)).map_or("", |s| s.id))
                .fetch_one(&mut *conn)
                .await
                .map_err(map_err)?;
                if !skipped {
                    return Err(gated(&format!("no unused approval of gate {g}")));
                }
                if req.item.is_some_and(|i| !in_budget(i)) {
                    return Err(gated("the item is not in the run's Budget"));
                }
            }
        },
        Cover::PerItemGate(f) => {
            let item = req
                .item
                .ok_or_else(|| gated("a per-item gate needs the item"))?;
            if approved_summary(&mut *conn, req.run_id, &format!("{f}:{item}"))
                .await?
                .is_none()
            {
                return Err(gated(&format!("no unused approval of gate {f}:{item}")));
            }
        }
    }
    Ok(Authorized {
        conn,
        run_id: req.run_id,
        step_seq: req.step_seq,
        command: req.command,
        item: req.item.map(str::to_owned),
    })
}

/// The summary of the gate's latest round when it has an unused approval from Board for its hash.
async fn approved_summary(
    conn: &mut PgConnection,
    run_id: Uuid,
    gate: &str,
) -> Result<Option<Value>, NewsError> {
    let row = sqlx::query(
        "select g.summary from gates g
           join approvals a on a.run_id = g.run_id and a.gate = g.gate and a.round = g.round
                           and a.subject_sha = g.subject_sha
          where g.run_id = $1 and g.gate = $2 and a.decision = 'approved' and a.origin = 'board'
            and a.consumed_at is null
            and g.round = (select max(round) from gates where run_id = $1 and gate = $2)",
    )
    .bind(run_id)
    .bind(gate)
    .fetch_optional(&mut *conn)
    .await
    .map_err(map_err)?;
    Ok(row.map(|r| r.get::<Value, _>("summary")))
}

/// Mark the approvals a finished step used as consumed (in the transaction that marks it done).
pub(crate) async fn consume(
    conn: &mut PgConnection,
    run_id: Uuid,
    cover: Cover,
    now: DateTime<Utc>,
) -> Result<(), NewsError> {
    let pattern = match cover {
        Cover::Gate(g) | Cover::GateOrStart(g) => g.to_owned(),
        Cover::PerItemGate(f) => format!("{f}:%"),
        Cover::None | Cover::Start => return Ok(()),
    };
    sqlx::query(
        "update approvals set consumed_at = $3
          where run_id = $1 and gate like $2 and decision = 'approved' and consumed_at is null",
    )
    .bind(run_id)
    .bind(pattern)
    .bind(now)
    .execute(&mut *conn)
    .await
    .map_err(map_err)?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn subject_hashes_are_stable_and_hex_round_trips() {
        let a = json!({"b": 1, "a": [1, 2]});
        let b: Value = serde_json::from_str(r#"{"a":[1,2],"b":1}"#).unwrap_or_default();
        assert_eq!(subject_sha(&a), subject_sha(&b));
        let h = hex(&subject_sha(&a));
        assert_eq!(h.len(), 64);
        assert_eq!(unhex(&h).as_deref(), Some(&subject_sha(&a)[..]));
        assert!(unhex("zz").is_none());
    }
}
