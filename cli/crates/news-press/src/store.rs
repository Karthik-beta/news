//! The PostgreSQL side of Press: staging into the outbox, draining it, the index budget's candidates.
//!
//! Staging takes `&mut Authorized`, so a row is written only inside the transaction that passed the
//! G-tier check (Bureau §5.5) and in that same transaction as the check result (`pages.status = 'ready'`):
//! a crash can leave a staged row, never a ready page without one. [`drain`] sends rows in order, one
//! at a time. A row is *to send* (not sent, not dead), *sent*, *dead* (a 4xx, class `bug`), and an unsent
//! row with `unknown_since` is one whose last send got no answer: the same key goes again.

use std::collections::HashSet;
use std::path::Path;

use chrono::{DateTime, DurationRound, TimeDelta, Utc};
use news_chief::gates::{Authorized, hex};
use news_chief::presets::GCommand;
use news_core::{NewsError, NewsResult};
use news_store::{PgPool, map_err};
use serde_json::{Value, json};
use sqlx::{PgConnection, Row};
use uuid::Uuid;

use crate::api::{ApiRequest, Client, Failure, Method, page_key};
use crate::budget::{self, Candidate, Pick};
use crate::bundle;

fn need_publish(auth: &Authorized<'_>) -> NewsResult<()> {
    if auth.command == GCommand::Publish {
        Ok(())
    } else {
        Err(NewsError::gated(format!(
            "{} does not authorise publishing",
            auth.command.cli()
        )))
    }
}

/// The authorisation must have been checked for this very page (`GRequest::item`), not for any page.
fn need_page(auth: &Authorized<'_>, page_id: &str) -> NewsResult<()> {
    if auth.item.as_deref() == Some(page_id) {
        Ok(())
    } else {
        Err(NewsError::gated(format!(
            "the authorisation was not checked for page {page_id}"
        )))
    }
}

/// Insert an outbox row, or return the one this key already has (staging is idempotent).
#[allow(clippy::too_many_arguments)]
async fn insert_row(
    conn: &mut PgConnection,
    run_id: Uuid,
    page_id: &str,
    version: i32,
    sha: &[u8],
    key: &str,
    op: &str,
    payload: Option<Value>,
    now: DateTime<Utc>,
) -> NewsResult<i64> {
    sqlx::query_scalar(
        "insert into publish_outbox (page_id, version, bundle_sha, idempotency_key, op, run_id, payload, created_at)
         values ($1, $2, $3, $4, $5, $6, $7, $8)
         on conflict (idempotency_key) do update
            set dead_at = case when publish_outbox.sent_at is null then null else publish_outbox.dead_at end
         returning outbox_id",
    )
    .bind(page_id)
    .bind(version)
    .bind(sha)
    .bind(key)
    .bind(op)
    .bind(run_id)
    .bind(payload)
    .bind(now)
    .fetch_one(conn)
    .await
    .map_err(map_err)
}

/// Stage a page's put: the page row (at this version and bundle) becomes `ready` and the outbox row is
/// inserted, in the caller's transaction. Staging the same page, version and bundle again returns the
/// first row. The key is `page_id:version:bundle_sha256`.
pub async fn stage_put(
    auth: &mut Authorized<'_>,
    page_id: &str,
    version: i32,
    sha: &[u8; 32],
    now: DateTime<Utc>,
) -> NewsResult<i64> {
    need_publish(auth)?;
    need_page(auth, page_id)?;
    let run_id = auth.run_id;
    let conn = auth.conn();
    let key = page_key(page_id, version, &hex(sha), "put_page")?;
    let staged: Option<i64> =
        sqlx::query_scalar("select outbox_id from publish_outbox where idempotency_key = $1")
            .bind(&key)
            .fetch_optional(&mut *conn)
            .await
            .map_err(map_err)?;
    if let Some(id) = staged {
        // Staging a dead row again puts it back on the list to send.
        sqlx::query(
            "update publish_outbox set dead_at = null, last_error = null
              where outbox_id = $1 and dead_at is not null and sent_at is null",
        )
        .bind(id)
        .execute(&mut *conn)
        .await
        .map_err(map_err)?;
        return Ok(id);
    }
    // The next version of a live page waits in `page_drafts`; the page itself stays published.
    let pending = sqlx::query(
        "update page_drafts set state = 'ready', updated_at = $4
          where page_id = $1 and version = $2 and bundle_sha = $3 and state in ('draft', 'ready')",
    )
    .bind(page_id)
    .bind(version)
    .bind(&sha[..])
    .bind(now)
    .execute(&mut *conn)
    .await
    .map_err(map_err)?;
    let ready = if pending.rows_affected() > 0 {
        pending
    } else {
        sqlx::query(
            "update pages set status = 'ready', updated_at = $4
              where page_id = $1 and version = $2 and bundle_sha = $3 and status in ('draft', 'ready')",
        )
        .bind(page_id)
        .bind(version)
        .bind(&sha[..])
        .bind(now)
        .execute(&mut *conn)
        .await
        .map_err(map_err)?
    };
    if ready.rows_affected() == 0 {
        return Err(NewsError::conflict(format!(
            "page {page_id} is not a draft at version {version} with that bundle"
        )));
    }
    insert_row(
        conn,
        run_id,
        page_id,
        version,
        &sha[..],
        &key,
        "put_page",
        None,
        now,
    )
    .await
}

/// Stage a withdrawal of a published page. `reason` is shown to nobody but the API's record (3–300 chars).
pub async fn stage_withdraw(
    auth: &mut Authorized<'_>,
    page_id: &str,
    reason: &str,
    now: DateTime<Utc>,
) -> NewsResult<i64> {
    need_publish(auth)?;
    need_page(auth, page_id)?;
    if !(3..=300).contains(&reason.chars().count()) {
        return Err(NewsError::validation(
            "a withdrawal reason is 3 to 300 characters",
        ));
    }
    let run_id = auth.run_id;
    let conn = auth.conn();
    let row = sqlx::query(
        "select version, bundle_sha from pages where page_id = $1 and status = 'published'",
    )
    .bind(page_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(map_err)?
    .ok_or_else(|| NewsError::not_found(format!("no published page {page_id}")))?;
    let (version, sha): (i32, Vec<u8>) = (row.get("version"), row.get("bundle_sha"));
    let key = page_key(page_id, version, &hex(&sha), "withdraw")?;
    insert_row(
        conn,
        run_id,
        page_id,
        version,
        &sha,
        &key,
        "withdraw",
        Some(json!({"reason": reason})),
        now,
    )
    .await
}

/// The index budget at Edition step 11: stage `index_promote` rows for the best published `noindex`
/// pages, up to what is left of `per_day` after today's (UTC) promotions. Returns the picks. A page with
/// an unsent promotion, live or dead, is not picked again. When the authorisation names an item, only
/// that page can be promoted; with none the step's own check covers the picks.
pub async fn stage_index_promotions(
    auth: &mut Authorized<'_>,
    per_day: i64,
    now: DateTime<Utc>,
) -> NewsResult<Vec<Pick>> {
    need_publish(auth)?;
    let run_id = auth.run_id;
    let auth_item = auth.item.clone();
    let conn = auth.conn();
    let day = now
        .duration_trunc(TimeDelta::days(1))
        .map_err(|e| NewsError::internal(e.to_string()))?;
    let today: i64 = sqlx::query_scalar(
        "select count(*) from publish_outbox
          where op = 'index_promote' and dead_at is null and created_at >= $1 and created_at < $2",
    )
    .bind(day)
    .bind(day + TimeDelta::days(1))
    .fetch_one(&mut *conn)
    .await
    .map_err(map_err)?;
    let room = usize::try_from((per_day - today).max(0)).unwrap_or(0);
    if room == 0 {
        return Ok(Vec::new());
    }
    let rows = sqlx::query(
        "select p.page_id, p.version, p.bundle_sha, p.tier, p.flags, p.published_at,
                coalesce((select s.total::float8 from scores s where s.story_id = p.story_id
                           order by s.computed_at desc limit 1), 0)::float8 as score,
                coalesce((select array_agg(r.ref) from page_refs r
                           where r.page_id = p.page_id and r.version = p.version and left(r.ref, 3) = 'sr_'),
                         '{}') as series
           from pages p
          where p.status = 'published' and p.index_status = 'noindex' and p.bundle_sha is not null
            and not exists (select 1 from publish_outbox o
                             where o.page_id = p.page_id and o.op = 'index_promote'
                               and o.sent_at is null)",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(map_err)?;
    let indexed: Vec<String> = sqlx::query_scalar(
        "select distinct r.ref from page_refs r
           join pages p on p.page_id = r.page_id and p.version = r.version
          where p.status = 'published' and p.index_status = 'index' and left(r.ref, 3) = 'sr_'",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(map_err)?;

    let mut versions = std::collections::HashMap::new();
    let pool: Vec<Candidate> = rows
        .iter()
        .filter(|r| {
            auth_item
                .as_deref()
                .is_none_or(|i| r.get::<String, _>("page_id") == i)
        })
        .map(|r| {
            let id: String = r.get("page_id");
            versions.insert(
                id.clone(),
                (
                    r.get::<i32, _>("version"),
                    r.get::<Vec<u8>, _>("bundle_sha"),
                ),
            );
            Candidate {
                page_id: id,
                tier: r.get("tier"),
                score: r.get("score"),
                flags: r.get("flags"),
                series: r.get("series"),
                published_at: r.get("published_at"),
            }
        })
        .collect();
    let picks = budget::select(pool, indexed.into_iter().collect::<HashSet<_>>(), room);
    for p in &picks {
        if let Some((version, sha)) = versions.get(&p.page_id) {
            let key = page_key(&p.page_id, *version, &hex(sha), "index_promote")?;
            let payload = json!({"index_status": "index", "priority": p.priority, "uniqueness": p.uniqueness});
            insert_row(
                conn,
                run_id,
                &p.page_id,
                *version,
                sha,
                &key,
                "index_promote",
                Some(payload),
                now,
            )
            .await?;
        }
    }
    Ok(picks)
}

/// A row the drain is about to send.
struct OutRow {
    id: i64,
    page_id: String,
    version: i32,
    sha: Vec<u8>,
    key: String,
    op: String,
    payload: Option<Value>,
}

/// What one [`drain`] did.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Drained {
    pub sent: u32,
    pub dead: u32,
    /// 1 when the drain stopped on a row whose answer never came; it is sent again next time.
    pub unknown: u32,
    /// 1 when the API refused the key (401 or 403): the row is not dead, the drain stopped, and the same
    /// row goes first once the key is fixed.
    pub refused: u32,
}

fn request(row: &OutRow, work: &Path) -> Result<ApiRequest, String> {
    let page = &row.page_id;
    let (method, path, body) = match row.op.as_str() {
        "put_page" => {
            let b = bundle::load(work, page, row.version)?;
            if b.bundle_sha[..] != row.sha[..] {
                return Err("the bundle on disk is not the one that was staged".into());
            }
            (Method::Put, format!("/v1/pages/{page}"), b.body)
        }
        "withdraw" => {
            let reason = row
                .payload
                .as_ref()
                .and_then(|p| p["reason"].as_str())
                .unwrap_or_default();
            (
                Method::Post,
                format!("/v1/pages/{page}/withdraw"),
                json!({"reason": reason}).to_string().into_bytes(),
            )
        }
        "index_promote" => (
            Method::Post,
            format!("/v1/pages/{page}/index"),
            json!({"index_status": "index"}).to_string().into_bytes(),
        ),
        other => return Err(format!("Press does not send {other} rows")),
    };
    Ok(ApiRequest {
        method,
        path,
        idempotency_key: row.key.clone(),
        body,
    })
}

async fn next_row(db: &PgPool, run: Option<Uuid>) -> NewsResult<Option<OutRow>> {
    let r = sqlx::query(
        "select outbox_id, page_id, version, bundle_sha, idempotency_key, op, payload
           from publish_outbox
          where sent_at is null and dead_at is null and op <> 'home_curation'
            and ($1::uuid is null or run_id = $1)
          order by outbox_id limit 1",
    )
    .bind(run)
    .fetch_optional(db)
    .await
    .map_err(map_err)?;
    Ok(r.map(|r| OutRow {
        id: r.get("outbox_id"),
        page_id: r.get("page_id"),
        version: r.get("version"),
        sha: r.get("bundle_sha"),
        key: r.get("idempotency_key"),
        op: r.get("op"),
        payload: r.get("payload"),
    }))
}

async fn mark_dead(
    db: &PgPool,
    id: i64,
    attempts: u32,
    why: &str,
    now: DateTime<Utc>,
) -> NewsResult<()> {
    let row = sqlx::query("update publish_outbox set dead_at = $2, last_error = $3, attempts = attempts + $4 where outbox_id = $1 returning page_id, version, op")
        .bind(id)
        .bind(now)
        .bind(why)
        .bind(i32::try_from(attempts).unwrap_or(i32::MAX))
        .fetch_one(db)
        .await
        .map_err(map_err)?;
    // A dead put leaves the live page as it was: its pending next version goes.
    if row.get::<String, _>("op") == "put_page" {
        let page: String = row.get("page_id");
        let v: i32 = row.get("version");
        sqlx::query("delete from page_refs where page_id = $1 and version = $2 and exists (select 1 from page_drafts d where d.page_id = $1 and d.version = $2)")
            .bind(&page)
            .bind(v)
            .execute(db)
            .await
            .map_err(map_err)?;
        sqlx::query("delete from page_drafts where page_id = $1 and version = $2")
            .bind(&page)
            .bind(v)
            .execute(db)
            .await
            .map_err(map_err)?;
    }
    Ok(())
}

async fn mark_sent(
    db: &PgPool,
    row: &OutRow,
    answer: Value,
    attempts: u32,
    now: DateTime<Utc>,
) -> NewsResult<()> {
    // The row and the page move together: a page is `published` exactly when its row is sent.
    let mut tx = db.begin().await.map_err(map_err)?;
    sqlx::query(
        "update publish_outbox set sent_at = $2, response = $3, attempts = attempts + $4,
                unknown_since = null, last_error = null where outbox_id = $1",
    )
    .bind(row.id)
    .bind(now)
    .bind(answer)
    .bind(i32::try_from(attempts).unwrap_or(i32::MAX))
    .execute(&mut *tx)
    .await
    .map_err(map_err)?;
    let promoted = if row.op == "put_page" {
        sqlx::query(
            "update pages p set version = d.version, bundle_sha = d.bundle_sha, oui_sha = d.oui_sha,
                    renderer_version = d.renderer_version, flags = d.flags, tier = coalesce(d.tier, p.tier),
                    page_type = coalesce(d.page_type, p.page_type), status = 'published',
                    published_at = coalesce(p.published_at, $3), updated_at = $3
               from page_drafts d
              where p.page_id = $1 and d.page_id = p.page_id and d.version = $2",
        )
        .bind(&row.page_id)
        .bind(row.version)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(map_err)?
        .rows_affected()
    } else {
        0
    };
    if promoted > 0 {
        sqlx::query("delete from page_drafts where page_id = $1 and version = $2")
            .bind(&row.page_id)
            .bind(row.version)
            .execute(&mut *tx)
            .await
            .map_err(map_err)?;
        return tx.commit().await.map_err(map_err);
    }
    let page = match row.op.as_str() {
        "put_page" => sqlx::query(
            "update pages set status = 'published', published_at = coalesce(published_at, $3), updated_at = $3
              where page_id = $1 and version = $2",
        ),
        "withdraw" => sqlx::query("update pages set status = 'withdrawn', updated_at = $3 where page_id = $1 and version = $2"),
        _ => sqlx::query(
            "update pages set index_status = 'index', updated_at = $3
              where page_id = $1 and version = $2 and status = 'published'",
        ),
    };
    page.bind(&row.page_id)
        .bind(row.version)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(map_err)?;
    tx.commit().await.map_err(map_err)
}

/// Send the outbox in order (optionally only one run's rows). Per-row failures never stop the drain:
/// a 4xx puts the row on the dead list and the next row goes, except 401 and 403 (the key, not the row,
/// is wrong), which stop the drain and leave the row to send. A row with no answer after the retries
/// stops it (order matters, and the API may be down); the row keeps its key and is sent again by the
/// next drain, or by the resume of its run.
pub async fn drain(
    db: &PgPool,
    client: &Client,
    work: &Path,
    run: Option<Uuid>,
) -> NewsResult<Drained> {
    let mut out = Drained::default();
    while let Some(row) = next_row(db, run).await? {
        let req = match request(&row, work) {
            Ok(r) => r,
            Err(why) => {
                mark_dead(db, row.id, 0, &why, client.now()).await?;
                out.dead += 1;
                continue;
            }
        };
        let sent = client.send(&req).await;
        let now = client.now();
        match sent.result {
            Ok(answer) => {
                mark_sent(db, &row, answer, sent.attempts, now).await?;
                out.sent += 1;
            }
            Err(Failure::Dead { message, .. }) => {
                mark_dead(db, row.id, sent.attempts, &message, now).await?;
                out.dead += 1;
            }
            Err(Failure::Auth { message, .. }) => {
                sqlx::query(
                    "update publish_outbox set attempts = attempts + $2, last_error = $3 where outbox_id = $1",
                )
                .bind(row.id)
                .bind(i32::try_from(sent.attempts).unwrap_or(i32::MAX))
                .bind(message)
                .execute(db)
                .await
                .map_err(map_err)?;
                out.refused = 1;
                break;
            }
            Err(Failure::Unknown { message }) => {
                sqlx::query(
                    "update publish_outbox set attempts = attempts + $2, unknown_since = coalesce(unknown_since, $3),
                            last_error = $4 where outbox_id = $1",
                )
                .bind(row.id)
                .bind(i32::try_from(sent.attempts).unwrap_or(i32::MAX))
                .bind(now)
                .bind(message)
                .execute(db)
                .await
                .map_err(map_err)?;
                out.unknown = 1;
                break;
            }
        }
    }
    Ok(out)
}

/// `news stats`: pages, the outbox and the index budget, as `{group, key, count}` items and a text.
pub async fn stats(db: &PgPool, per_day: i64, now: DateTime<Utc>) -> NewsResult<Value> {
    let group = |name: &'static str, rows: Vec<(String, i64)>| {
        rows.into_iter()
            .map(move |(k, n)| json!({"group": name, "key": k, "count": n}))
    };
    let pages: Vec<(String, i64)> =
        sqlx::query_as("select status, count(*) from pages group by status order by status")
            .fetch_all(db)
            .await
            .map_err(map_err)?;
    let index: Vec<(String, i64)> = sqlx::query_as(
        "select index_status, count(*) from pages where status = 'published' group by index_status order by index_status",
    )
    .fetch_all(db)
    .await
    .map_err(map_err)?;
    let outbox: Vec<(String, i64)> = sqlx::query_as(
        "select case when sent_at is not null then 'sent' when dead_at is not null then 'dead'
                     when unknown_since is not null then 'no_answer' else 'to_send' end as state, count(*)
           from publish_outbox group by 1 order by 1",
    )
    .fetch_all(db)
    .await
    .map_err(map_err)?;
    let retries: i64 = sqlx::query_scalar(
        "select coalesce(sum(greatest(attempts - 1, 0)), 0)::bigint from publish_outbox",
    )
    .fetch_one(db)
    .await
    .map_err(map_err)?;
    let day = now
        .duration_trunc(TimeDelta::days(1))
        .map_err(|e| NewsError::internal(e.to_string()))?;
    let today: i64 = sqlx::query_scalar(
        "select count(*) from publish_outbox
          where op = 'index_promote' and dead_at is null and created_at >= $1 and created_at < $2",
    )
    .bind(day)
    .bind(day + TimeDelta::days(1))
    .fetch_one(db)
    .await
    .map_err(map_err)?;

    let mut items: Vec<Value> = group("pages", pages)
        .chain(group("index_status", index))
        .chain(group("outbox", outbox))
        .collect();
    items.push(json!({"group": "retries", "key": "outbox", "count": retries}));
    items.push(json!({"group": "index_today", "key": format!("of {per_day}"), "count": today}));
    let line = |g: &str| {
        let parts: Vec<String> = items
            .iter()
            .filter(|i| i["group"] == g)
            .map(|i| format!("{} {}", i["count"], i["key"].as_str().unwrap_or_default()))
            .collect();
        if parts.is_empty() {
            "none".to_owned()
        } else {
            parts.join(", ")
        }
    };
    let text = format!(
        "Pages: {}\nIndexed or not (published): {}\nOutbox: {}\nRetries: {retries}\nIndex promotions today: {today} of {per_day}",
        line("pages"),
        line("index_status"),
        line("outbox"),
    );
    Ok(json!({"text": text, "items": items}))
}
