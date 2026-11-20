//! `news calendar …`: Diary's tools (Bureau §6.4).
//!
//! `sync` does the part that needs no network: the hand-checked seed file and the earnings windows
//! derived from the Ledger. The ICS, HTML and API calendars (BLS, BEA, FOMC, FRED) are fetched by
//! the `calendar.sync` job through the Runner once the Bureau service hosts it (#10, #29).

use std::fmt::Write as _;

use chrono::Utc;
use serde_json::json;

use super::{Reply, database};
use crate::cli::CalendarCmd;
use news_core::{NewsError, NewsResult};
use news_diary::{earnings, seed, store};

fn db_err(e: sqlx::Error) -> NewsError {
    news_store::map_err(e)
}

fn render(rows: &[store::EventRow]) -> String {
    let mut text = String::new();
    for r in rows {
        let at = r.expected_at.map_or_else(
            || "day only".to_owned(),
            |t| t.format("%H:%M UTC").to_string(),
        );
        let _ = writeln!(
            text,
            "{}  {:<9} {:<8} {}  [{}] {}",
            r.expected_day, at, r.status, r.event_key, r.source_id, r.title
        );
    }
    text.trim_end().to_owned()
}

pub async fn run(cmd: CalendarCmd) -> NewsResult<Reply> {
    let (db, _) = database(2).await?;
    let now = Utc::now();
    match cmd {
        CalendarCmd::Upcoming { days, region } => {
            let rows = store::upcoming(&db, now.date_naive(), days, region.as_deref())
                .await
                .map_err(db_err)?;
            Ok(Reply::new(json!({ "items": rows, "text": render(&rows) })))
        }
        CalendarCmd::Late => {
            let rows = store::late(&db, now).await.map_err(db_err)?;
            Ok(Reply::new(json!({ "items": rows, "text": render(&rows) })))
        }
        CalendarCmd::Sync => {
            let today = now.date_naive();
            let mut warnings = Vec::new();
            let mut entries = Vec::new();
            let cwd = std::env::current_dir().unwrap_or_default();
            match seed::seed_path(&cwd) {
                Some(path) => {
                    let text = std::fs::read_to_string(&path).map_err(|e| {
                        NewsError::not_found(format!("cannot read {}: {e}", path.display()))
                    })?;
                    let s = seed::parse(&text)
                        .map_err(|e| NewsError::validation(format!("{}: {e}", path.display())))?;
                    for e in seed::stale(&s, today) {
                        warnings.push(format!(
                            "seed entry {} {} was last checked on {}: check it against the bank's page",
                            e.bank, e.date, e.checked_on
                        ));
                    }
                    entries.extend(seed::entries(&s, today));
                }
                None => {
                    warnings.push("no data/calendar/seed.toml found: seed calendars skipped".into())
                }
            }
            for cik in store::tracked_ciks(&db).await.map_err(db_err)? {
                let history = store::earnings_history(&db, &cik).await.map_err(db_err)?;
                entries.extend(earnings::derive(&history, today));
            }
            // A calendar entry needs its source in `sources` (the foreign key).
            let known = store::known_sources(&db).await.map_err(db_err)?;
            let (keep, skip): (Vec<_>, Vec<_>) = entries
                .into_iter()
                .partition(|e| known.contains(&e.source_id));
            if !skip.is_empty() {
                let mut ids: Vec<&str> = skip.iter().map(|e| e.source_id.as_str()).collect();
                ids.sort_unstable();
                ids.dedup();
                warnings.push(format!(
                    "{} entries skipped: source not in the sources table ({})",
                    skip.len(),
                    ids.join(", ")
                ));
            }
            let report = store::upsert(&db, &keep).await.map_err(db_err)?;
            warnings.push(
                "BLS, BEA, FOMC and FRED calendars sync in the calendar.sync job once the Bureau service runs (#10, #29)"
                    .into(),
            );
            let mut reply = Reply::new(json!({
                "events": report.events,
                "created": report.created,
                "updated": report.updated,
            }));
            reply.warnings = warnings;
            Ok(reply)
        }
    }
}
