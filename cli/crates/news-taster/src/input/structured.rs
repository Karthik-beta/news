//! The data behind a structured story's magnitude (Bureau §4.2, "Magnitude / surprise (M)"): macro
//! observations, a policy rate's moves, revenue growth and forecast revisions, read from the Ledger's
//! `data_points` through the candidates' `dp_ids`. Numbers come from stored values by code; a value is
//! only compared with one of the same series and unit, and nothing is converted: `change_bp` is the
//! difference of two percent values times 100 (what a basis point is), and only for a percent unit.
//!
//! A history is read as of the story's own data: for each period the latest vintage no later than the
//! anchor point's, so a re-run gives the same input.

use super::Member;
use chrono::{DateTime, Datelike, NaiveDate, Utc};
use news_clerk::news_ledger::units::{UnitClass, classify};
use news_nose::score::Page;
use sqlx::{PgConnection, Row, postgres::PgRow};

const REVENUE: &[&str] = &[
    "us-gaap:Revenues",
    "us-gaap:RevenueFromContractWithCustomerExcludingAssessedTax",
    "us-gaap:RevenueFromContractWithCustomerIncludingAssessedTax",
    "us-gaap:SalesRevenueNet",
    "ifrs-full:Revenue",
];
const INCOME: &[&str] = &[
    "us-gaap:NetIncomeLoss",
    "us-gaap:ProfitLoss",
    "ifrs-full:ProfitLoss",
];

#[derive(Debug, Clone)]
struct Dp {
    dp_id: String,
    series_key: String,
    unit: String,
    concept: String,
    start: Option<NaiveDate>,
    end: Option<NaiveDate>,
    vintage: DateTime<Utc>,
    value: Option<f64>,
}

impl Dp {
    fn from_row(r: &PgRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            dp_id: r.try_get("dp_id")?,
            series_key: r.try_get("series_key")?,
            unit: r.try_get("unit")?,
            concept: r.try_get("concept")?,
            start: r.try_get("period_start")?,
            end: r.try_get("period_end")?,
            vintage: r.try_get("vintage")?,
            value: r.try_get("value")?,
        })
    }

    /// Length of the period in days, both ends included.
    fn days(&self) -> Option<i64> {
        Some((self.end? - self.start?).num_days() + 1)
    }

    /// 1 for a quarter, 2 for a year: the periods a year-on-year growth compares.
    fn class(&self) -> Option<u8> {
        match self.days()? {
            85..=100 => Some(1),
            350..=380 => Some(2),
            _ => None,
        }
    }
}

async fn points(tx: &mut PgConnection, ids: &[String]) -> Result<Vec<Dp>, sqlx::Error> {
    sqlx::query(
        "select dp_id, series_key, unit, concept, period_start, period_end, vintage,
                value::float8 as value
           from data_points where dp_id = any($1)",
    )
    .bind(ids)
    .fetch_all(&mut *tx)
    .await?
    .iter()
    .map(Dp::from_row)
    .collect()
}

/// The point with the latest period, then the latest vintage.
fn latest<'a>(points: impl Iterator<Item = &'a Dp>) -> Option<&'a Dp> {
    points
        .filter(|p| p.end.is_some())
        .max_by(|x, y| (x.end, x.vintage, &x.dp_id).cmp(&(y.end, y.vintage, &y.dp_id)))
}

/// The anchor point: the latest of the best member's own points; the other members' only when the
/// best member has none (two members can hold different series, e.g. SA and NSA, of one event).
fn anchor<'a>(pts: &'a [Dp], best_ids: &[String]) -> Option<&'a Dp> {
    latest(pts.iter().filter(|p| best_ids.contains(&p.dp_id))).or_else(|| latest(pts.iter()))
}

/// The history's last run of consecutive periods (`per_year` per year, compared on the end month), so
/// a missing period is not read as a one-step change across the gap.
fn trailing_run(h: &[Dp], per_year: u32) -> &[Dp] {
    let step = i64::from(12 / per_year);
    let month = |p: &Dp| {
        p.end
            .map(|e| i64::from(e.year()) * 12 + i64::from(e.month()))
    };
    let mut from = h.len().saturating_sub(1);
    while from > 0
        && month(&h[from])
            .zip(month(&h[from - 1]))
            .is_some_and(|(a, b)| a - b == step)
    {
        from -= 1;
    }
    &h[from..]
}

/// The revenue point to anchor on: the latest period across all revenue concepts (a filing's older
/// comparative may use another concept than its current year), then a quarter over a year, then the
/// concept's priority, the latest vintage.
fn pick_revenue(pts: &[Dp]) -> Option<&Dp> {
    let rank = |p: &Dp| REVENUE.iter().position(|c| *c == p.concept);
    let key = |p: &Dp| {
        (
            p.end,
            std::cmp::Reverse(p.class()),
            std::cmp::Reverse(rank(p)),
            p.vintage,
            p.dp_id.clone(),
        )
    };
    pts.iter()
        .filter(|p| rank(p).is_some() && p.class().is_some())
        .max_by_key(|p| key(p))
}

/// The anchor's series as of its vintage, one point per period, oldest first.
async fn history(tx: &mut PgConnection, a: &Dp) -> Result<Vec<Dp>, sqlx::Error> {
    let mut h: Vec<Dp> = sqlx::query(
        "select distinct on (period_start, period_end) dp_id, series_key, unit, concept, period_start,
                period_end, vintage, value::float8 as value
           from data_points
          where series_key = $1 and unit = $2 and period_end <= $3 and vintage <= $4 and value is not null
          order by period_start, period_end, vintage desc, dp_id desc",
    )
    .bind(&a.series_key)
    .bind(&a.unit)
    .bind(a.end)
    .bind(a.vintage)
    .fetch_all(&mut *tx)
    .await?
    .iter()
    .map(Dp::from_row)
    .collect::<Result<_, _>>()?;
    h.sort_by_key(|p| (p.end, p.start));
    Ok(h)
}

/// The same period a year earlier (a gap of 357 to 373 days, the SEC connector's own window).
fn year_ago<'a>(h: &'a [Dp], p: &Dp) -> Option<&'a Dp> {
    h.iter().find(|q| {
        q.end
            .zip(p.end)
            .is_some_and(|(q, p)| (357..=373).contains(&(p - q).num_days()))
    })
}

/// Observations per year from the period's length, for the periods a macro series has.
fn per_year(days: i64) -> Option<u32> {
    match days {
        28..=31 => Some(12),
        89..=92 => Some(4),
        181..=184 => Some(2),
        365..=366 => Some(1),
        _ => None,
    }
}

/// The page data of a story's best page type, from the data points of its candidates (best member
/// first). `None` when the candidates carry no points or the data is not enough to compute M.
pub async fn load(
    tx: &mut PgConnection,
    best: &Member,
    members: &[Member],
) -> Result<Option<Page>, sqlx::Error> {
    let Some(kind @ ("macro" | "policy" | "earnings" | "report")) = best.page_type.as_deref()
    else {
        return Ok(None);
    };
    let mut ids: Vec<String> = Vec::new();
    for m in std::iter::once(best).chain(members.iter().filter(|m| m.cand_id != best.cand_id)) {
        for id in &m.dp_ids {
            if !ids.contains(id) {
                ids.push(id.clone());
            }
        }
    }
    if ids.is_empty() {
        return Ok(None);
    }
    match kind {
        "macro" => macro_page(tx, &best.dp_ids, &ids).await,
        "policy" => policy(tx, &best.dp_ids, &ids).await,
        "earnings" => earnings(tx, &ids).await,
        _ => report(tx, &ids).await,
    }
}

/// The observations of the anchor's series, latest last, the trailing run without a missing period.
async fn macro_page(
    tx: &mut PgConnection,
    best_ids: &[String],
    ids: &[String],
) -> Result<Option<Page>, sqlx::Error> {
    let pts = points(tx, ids).await?;
    let Some(a) = anchor(&pts, best_ids) else {
        return Ok(None);
    };
    let Some(per_year) = a.days().and_then(per_year) else {
        return Ok(None);
    };
    let h = history(tx, a).await?;
    let values = trailing_run(&h, per_year)
        .iter()
        .filter_map(|p| p.value)
        .collect();
    Ok(Some(Page::Macro { values, per_year }))
}

/// A policy rate series with one point per decision: the last move, the holds before it and whether
/// it reverses the move before. `statement_change` is left at 0: the statement text diff is not built.
///
/// Groundwork only: no connector yet supplies decision-dated rate points (statement candidates have no
/// dp_ids, and the ECB series is business-day), and `holds_before` assumes one point per decision.
async fn policy(
    tx: &mut PgConnection,
    best_ids: &[String],
    ids: &[String],
) -> Result<Option<Page>, sqlx::Error> {
    let pts = points(tx, ids).await?;
    let Some(a) = anchor(&pts, best_ids) else {
        return Ok(None);
    };
    if classify(&a.unit).map(|u| u.class) != Some(UnitClass::Percent) {
        return Ok(None);
    }
    let v: Vec<f64> = history(tx, a)
        .await?
        .iter()
        .filter_map(|p| p.value)
        .collect();
    let moves: Vec<f64> = v.windows(2).map(|w| w[1] - w[0]).collect();
    let Some((last, before)) = moves.split_last() else {
        return Ok(None);
    };
    Ok(Some(Page::Policy {
        change_bp: (last * 100.0).round() as i32,
        statement_change: 0.0,
        holds_before: u32::try_from(before.iter().rev().take_while(|m| **m == 0.0).count())
            .unwrap_or(u32::MAX),
        reversal: *last != 0.0
            && before
                .iter()
                .rev()
                .find(|m| **m != 0.0)
                .is_some_and(|m| m.signum() != last.signum()),
    }))
}

/// Year-on-year revenue growth of the filing's quarter (a year when it has none) and of the earlier
/// ones; `flip` when net income changed sign against a year earlier, or the filing restates revenue or
/// net income.
async fn earnings(tx: &mut PgConnection, ids: &[String]) -> Result<Option<Page>, sqlx::Error> {
    let pts = points(tx, ids).await?;
    let Some(a) = pick_revenue(&pts) else {
        return Ok(None);
    };
    let mut h = history(tx, a).await?;
    h.retain(|p| p.class() == a.class());
    let growth: Vec<(NaiveDate, f64)> = h
        .iter()
        .filter_map(|p| {
            let before = year_ago(&h, p)?.value.filter(|b| *b > 0.0)?;
            Some((p.end?, p.value? / before - 1.0))
        })
        .collect();
    let Some(now) = growth
        .iter()
        .find(|(e, _)| Some(*e) == a.end)
        .map(|(_, g)| *g)
    else {
        return Ok(None);
    };
    let past_growth = growth
        .iter()
        .filter(|(e, _)| Some(*e) < a.end)
        .map(|(_, g)| *g)
        .collect();

    let income = INCOME.iter().find_map(|c| {
        pts.iter()
            .filter(|p| p.concept == *c && p.start == a.start && p.end == a.end)
            .max_by(|x, y| (x.vintage, &x.dp_id).cmp(&(y.vintage, &y.dp_id)))
    });
    let mut flip = false;
    if let Some(n) = income {
        let mut hist = history(tx, n).await?;
        hist.retain(|p| p.class() == n.class());
        let sign = |v: Option<f64>| v.filter(|x| *x != 0.0).map(f64::signum);
        let cur = hist.iter().find(|p| p.end == n.end).and_then(|p| p.value);
        let prior = year_ago(&hist, n).and_then(|p| p.value);
        flip = sign(cur).zip(sign(prior)).is_some_and(|(c, p)| c != p);
    }
    let concepts: Vec<&str> = REVENUE.iter().chain(INCOME).copied().collect();
    let restated: bool = sqlx::query_scalar(
        "select exists (select 1 from data_points d join data_points o on o.dp_id = d.supersedes
                         where d.dp_id = any($1) and d.concept = any($2) and o.value is distinct from d.value)",
    )
    .bind(ids)
    .bind(&concepts)
    .fetch_one(&mut *tx)
    .await?;
    Ok(Some(Page::Earnings {
        growth: now,
        past_growth,
        flip: flip || restated,
    }))
}

/// The headline forecast (the first point of the best member that has points) against the edition it
/// supersedes, and the revisions of the same series before it.
async fn report(tx: &mut PgConnection, ids: &[String]) -> Result<Option<Page>, sqlx::Error> {
    let Some(headline) = ids.first() else {
        return Ok(None);
    };
    let pts = points(tx, std::slice::from_ref(headline)).await?;
    let Some(a) = pts.first() else {
        return Ok(None);
    };
    let revision = sqlx::query_scalar::<_, Option<f64>>(
        "select (d.value - o.value)::float8 from data_points d join data_points o on o.dp_id = d.supersedes
          where d.dp_id = $1",
    )
    .bind(&a.dp_id)
    .fetch_optional(&mut *tx)
    .await?
    .flatten();
    let past_revisions: Vec<f64> = sqlx::query_scalar(
        "select (d.value - o.value)::float8 from data_points d join data_points o on o.dp_id = d.supersedes
          where d.series_key = $1 and d.unit = $2 and d.vintage < $3 and d.value is not null and o.value is not null
          order by d.vintage, d.dp_id",
    )
    .bind(&a.series_key)
    .bind(&a.unit)
    .bind(a.vintage)
    .fetch_all(&mut *tx)
    .await?;
    Ok(Some(Page::Report {
        revision,
        past_revisions,
    }))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn periods_map_to_frequencies_and_growth_classes() {
        assert_eq!(per_year(31), Some(12));
        assert_eq!(per_year(28), Some(12));
        assert_eq!(per_year(90), Some(4));
        assert_eq!(per_year(365), Some(1));
        assert_eq!(per_year(7), None);
        let d = |s: &str, e: &str| Dp {
            dp_id: "dp_1".into(),
            series_key: "s".into(),
            unit: "USD".into(),
            concept: "c".into(),
            start: s.parse().ok(),
            end: e.parse().ok(),
            vintage: "2026-01-01T00:00:00Z".parse().unwrap(),
            value: Some(1.0),
        };
        assert_eq!(d("2026-03-29", "2026-06-27").class(), Some(1));
        assert_eq!(d("2025-09-28", "2026-09-26").class(), Some(2));
        // A nine-month year-to-date is neither.
        assert_eq!(d("2025-09-28", "2026-06-27").class(), None);
    }

    fn dp(id: &str, series: &str, concept: &str, s: &str, e: &str, vintage: &str) -> Dp {
        Dp {
            dp_id: id.into(),
            series_key: series.into(),
            unit: "USD".into(),
            concept: concept.into(),
            start: s.parse().ok(),
            end: e.parse().ok(),
            vintage: vintage.parse().unwrap(),
            value: Some(1.0),
        }
    }

    #[test]
    fn anchor_stays_on_the_best_members_series() {
        let (s, e) = ("2026-10-01", "2026-10-31");
        let sa = dp("dp_sa", "fred:CPIAUCSL", "c", s, e, "2026-11-13T00:00:00Z");
        let nsa = dp(
            "dp_nsa",
            "bls:CUUR0000SA0",
            "c",
            s,
            e,
            "2026-11-14T00:00:00Z",
        );
        let best = ["dp_sa".to_string()];
        // The other member's later vintage does not move the anchor, in either order.
        let id = |pts: &[Dp]| anchor(pts, &best).unwrap().dp_id.clone();
        assert_eq!(id(&[sa.clone(), nsa.clone()]), "dp_sa");
        assert_eq!(id(&[nsa.clone(), sa]), "dp_sa");
        // A best member with no points falls back to the others.
        assert_eq!(id(&[nsa]), "dp_nsa");
    }

    #[test]
    fn revenue_anchors_on_the_latest_period_across_concepts() {
        let v = "2026-10-30T00:00:00Z";
        let old = dp(
            "old",
            "s",
            "us-gaap:Revenues",
            "2024-09-29",
            "2025-09-27",
            v,
        );
        let other = "us-gaap:RevenueFromContractWithCustomerExcludingAssessedTax";
        let cur = dp("cur", "s", other, "2025-09-28", "2026-09-26", v);
        assert_eq!(pick_revenue(&[old, cur.clone()]).unwrap().dp_id, "cur");
        // The same period: the higher-priority concept wins.
        let same = dp(
            "same",
            "s",
            "us-gaap:Revenues",
            "2025-09-28",
            "2026-09-26",
            v,
        );
        assert_eq!(pick_revenue(&[cur, same]).unwrap().dp_id, "same");
    }

    #[test]
    fn history_is_cut_to_the_trailing_run_of_periods() {
        let m = |id: &str, s: &str, e: &str| dp(id, "s", "c", s, e, "2026-11-13T00:00:00Z");
        let h = [
            m("a", "2025-08-01", "2025-08-31"),
            m("b", "2025-09-01", "2025-09-30"),
            // October 2025 is missing.
            m("c", "2025-11-01", "2025-11-30"),
            m("d", "2025-12-01", "2025-12-31"),
        ];
        let ids = |r: &[Dp]| r.iter().map(|p| p.dp_id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(trailing_run(&h, 12)), ["c", "d"]);
        assert_eq!(ids(trailing_run(&h[..2], 12)), ["a", "b"]);
        assert!(trailing_run(&[], 12).is_empty());
    }
}
