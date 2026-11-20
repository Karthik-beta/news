//! `calendar_events`: upsert, upcoming, late and captured. Every write is idempotent: syncing the
//! same entries twice changes nothing, and an event already `captured`, `late` or `cancelled` is never
//! moved by a later sync.

use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgPool};

use news_connectors::CalendarEntry;

use crate::earnings::Filing;
use crate::schedule::{self, Timing};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SyncReport {
    pub events: usize,
    pub created: usize,
    pub updated: usize,
}

/// A calendar row as `news calendar upcoming|late` shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, FromRow)]
pub struct EventRow {
    pub event_key: String,
    pub title: String,
    pub source_id: String,
    pub expected_at: Option<DateTime<Utc>>,
    pub expected_day: NaiveDate,
    pub precision: String,
    pub status: String,
}

/// Insert new events and update those still `expected` whose date or title changed.
pub async fn upsert(pool: &PgPool, entries: &[CalendarEntry]) -> Result<SyncReport, sqlx::Error> {
    let mut report = SyncReport {
        events: entries.len(),
        ..SyncReport::default()
    };
    for e in entries {
        let created: Option<bool> = sqlx::query_scalar(
            "insert into calendar_events
               (source_id, event_key, title, expected_at, expected_day, precision, window_days, origin)
             values ($1, $2, $3, $4, $5, $6, $7, $8)
             on conflict (event_key) do update set
               source_id = excluded.source_id, title = excluded.title,
               expected_at = excluded.expected_at, expected_day = excluded.expected_day,
               precision = excluded.precision, window_days = excluded.window_days,
               origin = excluded.origin
             where calendar_events.status = 'expected'
               and (calendar_events.source_id, calendar_events.title, calendar_events.expected_at,
                    calendar_events.expected_day, calendar_events.precision,
                    calendar_events.window_days, calendar_events.origin)
                   is distinct from
                   (excluded.source_id, excluded.title, excluded.expected_at, excluded.expected_day,
                    excluded.precision, excluded.window_days, excluded.origin)
             returning (xmax = 0)",
        )
        .bind(&e.source_id)
        .bind(&e.event_key)
        .bind(&e.title)
        .bind(e.expected_at)
        .bind(e.expected_day)
        .bind(&e.precision)
        .bind(i16::try_from(e.window_days).unwrap_or(i16::MAX))
        .bind(&e.origin)
        .fetch_optional(pool)
        .await?;
        match created {
            Some(true) => report.created += 1,
            Some(false) => report.updated += 1,
            None => {}
        }
    }
    Ok(report)
}

/// Events from `today` through `today + days`, soonest first, optionally for one region.
pub async fn upcoming(
    pool: &PgPool,
    today: NaiveDate,
    days: u32,
    region: Option<&str>,
) -> Result<Vec<EventRow>, sqlx::Error> {
    let to = today + chrono::Duration::days(i64::from(days));
    sqlx::query_as::<_, EventRow>(
        "select e.event_key, e.title, e.source_id, e.expected_at, e.expected_day, e.precision, e.status
           from calendar_events e join sources s using (source_id)
          where e.expected_day between $1 and $2 and e.status <> 'cancelled'
            and ($3::text is null or $3 = any (s.regions))
          order by e.expected_day, e.expected_at nulls last, e.event_key",
    )
    .bind(today)
    .bind(to)
    .bind(region)
    .fetch_all(pool)
    .await
}

#[derive(FromRow)]
struct Pending {
    cal_id: i64,
    expected_day: NaiveDate,
    expected_at: Option<DateTime<Utc>>,
    window_days: i16,
}

/// Mark `late` every event still `expected` whose capture schedule is spent at `now`
/// ([`schedule::late_after`]). Returns how many were marked.
pub async fn mark_late(pool: &PgPool, now: DateTime<Utc>) -> Result<usize, sqlx::Error> {
    let pending: Vec<Pending> = sqlx::query_as(
        "select cal_id, expected_day, expected_at, window_days from calendar_events
          where status = 'expected' and expected_day <= $1",
    )
    .bind(now.date_naive())
    .fetch_all(pool)
    .await?;
    let overdue: Vec<i64> = pending
        .iter()
        .filter(|p| {
            let t = Timing {
                expected_day: p.expected_day,
                expected_at: p.expected_at,
                window_days: u16::try_from(p.window_days).unwrap_or(0),
            };
            schedule::late_after(&t) < now
        })
        .map(|p| p.cal_id)
        .collect();
    if overdue.is_empty() {
        return Ok(0);
    }
    let n = sqlx::query(
        "update calendar_events set status = 'late' where cal_id = any ($1) and status = 'expected'",
    )
    .bind(&overdue)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(usize::try_from(n).unwrap_or(0))
}

/// Events that did not arrive, most recent first (after marking the overdue ones).
pub async fn late(pool: &PgPool, now: DateTime<Utc>) -> Result<Vec<EventRow>, sqlx::Error> {
    mark_late(pool, now).await?;
    sqlx::query_as::<_, EventRow>(
        "select event_key, title, source_id, expected_at, expected_day, precision, status
           from calendar_events where status = 'late' order by expected_day desc, event_key",
    )
    .fetch_all(pool)
    .await
}

/// The release arrived: its candidate carried this event key. A `late` event that turns up is
/// captured after all. Returns false when there is no such open event.
pub async fn mark_captured(
    pool: &PgPool,
    event_key: &str,
    at: DateTime<Utc>,
) -> Result<bool, sqlx::Error> {
    let n = sqlx::query(
        "update calendar_events set status = 'captured', captured_at = $2
          where event_key = $1 and status in ('expected', 'late')",
    )
    .bind(event_key)
    .bind(at)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(n > 0)
}

#[derive(FromRow)]
struct FilingRow {
    fiscal_year: i16,
    fiscal_period: String,
    filed: NaiveDate,
}

/// The company's own filing history from the Ledger: each fiscal period it has reported, dated by
/// its earliest vintage (SEC `filed`). `cik` is 1–10 digits.
pub async fn earnings_history(pool: &PgPool, cik: &str) -> Result<Vec<Filing>, sqlx::Error> {
    let cik10 = news_ledger::eventkey::cik10(cik).unwrap_or_default();
    let rows: Vec<FilingRow> = sqlx::query_as(
        "select d.fiscal_year, d.fiscal_period, (min(d.vintage) at time zone 'UTC')::date as filed
           from data_points d
           join entity_ids i on i.entity_id = d.entity_id and i.scheme = 'cik' and i.value = $1
          where d.fiscal_year is not null and d.fiscal_period is not null
          group by d.fiscal_year, d.fiscal_period",
    )
    .bind(&cik10)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| Filing {
            cik: cik10.clone(),
            fiscal_year: r.fiscal_year,
            fiscal_period: r.fiscal_period,
            filed: r.filed,
        })
        .collect())
}

/// CIKs the Ledger has fiscal-period facts for: the companies whose earnings windows can be derived.
pub async fn tracked_ciks(pool: &PgPool) -> Result<Vec<String>, sqlx::Error> {
    sqlx::query_scalar(
        "select distinct i.value from entity_ids i
           join data_points d on d.entity_id = i.entity_id
          where i.scheme = 'cik' and d.fiscal_year is not null and d.fiscal_period is not null
          order by 1",
    )
    .fetch_all(pool)
    .await
}

/// Source ids present in `sources` (a calendar entry for any other source cannot be stored).
pub async fn known_sources(
    pool: &PgPool,
) -> Result<std::collections::BTreeSet<String>, sqlx::Error> {
    let v: Vec<String> = sqlx::query_scalar("select source_id from sources")
        .fetch_all(pool)
        .await?;
    Ok(v.into_iter().collect())
}
