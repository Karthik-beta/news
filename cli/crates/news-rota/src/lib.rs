//! Rota, the scheduler (ADR 0008 §7).
//!
//! Every 30 seconds, and at start, Rota reads Standing, applies quiet hours to the `ai` pool, refreshes
//! the usage reading, and fires the schedules that are due. For each schedule only the latest due slot
//! is considered, and the rules run in order: (1) the missed-slot rule drops a late slot whose next
//! slot is close (`missed`, no run); (2) the late limit ends one past `lineup.queue_late_limit_minutes`
//! as `skipped` with a Wrap; (3) otherwise it fires through Chief (`origin = 'schedule'`). Kiri starts
//! at boot, so being signed out does not stop Rota; while the laptop is off or asleep, or PostgreSQL is
//! down, nothing ticks, and the first tick after applies the same rules, so a slot that fell meanwhile
//! runs if still within the late limit and is skipped otherwise.
//!
//! [`RotaPolicy`] is Chief's `StartPolicy` and the `ai` pool's pre-claim hook: quiet hours hold heavy
//! starts (`dont_start`) or shrink the `ai` pool (`reduced`); the quota guard refuses starts at the
//! cap or reserve and stops `ai` claims.

use chrono::{DateTime, Datelike, Duration, NaiveTime, TimeZone, Utc};
use news_chief::presets::PresetDecl;
use news_chief::{AiClaim, Chief, IfBusy, Origin, Policy, StartPolicy, StartReq, Started};
use news_core::NewsError;
use news_core::clock::Clock;
use news_standing::Standing;
use news_store::{PgPool, map_err};
use serde_json::{Value, json};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

/// Tokens as the guard counts them: input + output + cache creation; cache reads are not counted
/// (ADR 0008 §7.3). The one place that rule lives.
pub fn tokens(input: u64, output: u64, cache_creation: u64, _cache_read: u64) -> u64 {
    input + output + cache_creation
}

/// A usage reading for the current local day and ISO week.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Usage {
    pub bureau_today: u64,
    pub bureau_week: u64,
    pub total_today: u64,
    pub total_week: u64,
}

/// Where usage is read from (ClickHouse `ai_calls` and `claude_code.token.usage`). `Err` means
/// unknown: the guard then allows, and Problems says "usage unknown".
pub trait UsageSource: Send + Sync {
    fn read<'a>(
        &'a self,
        day_start: DateTime<Utc>,
        week_start: DateTime<Utc>,
    ) -> Pin<Box<dyn Future<Output = Result<Usage, NewsError>> + Send + 'a>>;
}

/// No usage source yet (the ClickHouse reader lands with Ticker's tables): always unknown.
pub struct Unknown;
impl UsageSource for Unknown {
    fn read<'a>(
        &'a self,
        _: DateTime<Utc>,
        _: DateTime<Utc>,
    ) -> Pin<Box<dyn Future<Output = Result<Usage, NewsError>> + Send + 'a>> {
        Box::pin(async { Err(NewsError::upstream("no usage source")) })
    }
}

/// Start of the local day and of the ISO week containing `now`, in `tz`, as UTC.
pub fn day_and_week_start(tz: chrono_tz::Tz, now: DateTime<Utc>) -> (DateTime<Utc>, DateTime<Utc>) {
    let local = now.with_timezone(&tz).date_naive();
    let monday = local - Duration::days(i64::from(local.weekday().num_days_from_monday()));
    let start = |d: chrono::NaiveDate| {
        tz.from_local_datetime(&d.and_time(NaiveTime::MIN))
            .earliest()
            .map_or(now, |t| t.with_timezone(&Utc))
    };
    (start(local), start(monday))
}

/// Whether `now` is inside quiet hours (`start`..`end`, local, may wrap midnight).
pub fn in_quiet_hours(s: &Standing, tz: chrono_tz::Tz, now: DateTime<Utc>) -> bool {
    if !s.bool("quiet_hours.enabled") {
        return false;
    }
    let parse = |k| s.text(k).as_deref().and_then(news_standing::parse_hhmm);
    let (Some(start), Some(end)) = (parse("quiet_hours.start"), parse("quiet_hours.end")) else {
        return false;
    };
    let t = now.with_timezone(&tz);
    let m = t.time().format("%H:%M").to_string();
    let m = news_standing::parse_hhmm(&m).unwrap_or(0);
    if start <= end {
        (start..end).contains(&m)
    } else {
        m >= start || m < end
    }
}

struct State {
    standing: Standing,
    usage: Option<Usage>,
    read_at: Option<DateTime<Utc>>,
}

/// Chief's `StartPolicy` and the `ai` pre-claim hook, from cached Standing and usage (refreshed by
/// [`Rota::tick`]).
#[derive(Clone)]
pub struct RotaPolicy(Arc<Mutex<State>>);

impl Default for RotaPolicy {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(State {
            standing: Standing::defaults(),
            usage: None,
            read_at: None,
        })))
    }
}

impl RotaPolicy {
    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.0.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn set_standing(&self, s: Standing) {
        self.state().standing = s;
    }

    pub fn set_usage(&self, u: Option<Usage>, at: DateTime<Utc>) {
        let mut st = self.state();
        st.usage = u;
        st.read_at = Some(at);
    }

    /// `Some("cap")` or `Some("reserve")` when reached.
    fn quota(&self) -> Option<&'static str> {
        let st = self.state();
        let u = st.usage?;
        let s = &st.standing;
        let over =
            |used: u64, key: &str| s.opt_int(key).is_some_and(|lim| used >= lim.max(0) as u64);
        if over(u.bureau_today, "quota.bureau_daily_tokens")
            || over(u.bureau_week, "quota.bureau_weekly_tokens")
        {
            Some("cap")
        } else if over(u.total_today, "quota.reserve_daily_tokens")
            || over(u.total_week, "quota.reserve_weekly_tokens")
        {
            Some("reserve")
        } else {
            None
        }
    }
}

impl StartPolicy for RotaPolicy {
    fn check(&self, preset: &PresetDecl, _origin: Origin, now: DateTime<Utc>) -> Policy {
        if let Some(q) = self.quota() {
            return Policy::Refuse {
                reason: "quota".into(),
                text: format!(
                    "Usage has reached the {q}; {} will not start until it falls back.",
                    preset.name
                ),
            };
        }
        let (quiet, dont_start) = {
            let st = self.state();
            let tz = news_standing::time_zone(&st.standing);
            (
                in_quiet_hours(&st.standing, tz, now),
                st.standing.text("quiet_hours.mode").as_deref() == Some("dont_start"),
            )
        };
        if preset.heavy && quiet && dont_start {
            return Policy::Hold {
                reason: "quiet_hours".into(),
                text: format!(
                    "Quiet hours: {} (heavy) does not start until they end.",
                    preset.name
                ),
            };
        }
        Policy::Allow
    }

    fn ai_claim(&self, _now: DateTime<Utc>) -> AiClaim {
        match self.quota() {
            Some("cap") => AiClaim::Cap,
            Some(_) => AiClaim::Reserve,
            None => AiClaim::Allow,
        }
    }

    fn usage(&self) -> Value {
        let st = self.state();
        let s = &st.standing;
        let lim = |k: &str| s.opt_int(k);
        match st.usage {
            Some(u) => {
                json!({"known": true, "bureau_today": u.bureau_today, "bureau_week": u.bureau_week,
                "total_today": u.total_today, "total_week": u.total_week,
                "cap_today": lim("quota.bureau_daily_tokens"), "cap_week": lim("quota.bureau_weekly_tokens"),
                "reserve_today": lim("quota.reserve_daily_tokens"), "reserve_week": lim("quota.reserve_weekly_tokens")})
            }
            None => {
                json!({"known": false, "bureau_today": null, "bureau_week": null, "total_today": null,
                "total_week": null, "cap_today": lim("quota.bureau_daily_tokens"), "cap_week": lim("quota.bureau_weekly_tokens"),
                "reserve_today": lim("quota.reserve_daily_tokens"), "reserve_week": lim("quota.reserve_weekly_tokens")})
            }
        }
    }
}

/// The due slots of a schedule: the latest one after `lower` and up to `now`, and how many older ones
/// were passed over. `None` when nothing is due.
pub fn latest_due(
    cron: &str,
    tz: chrono_tz::Tz,
    lower: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<Option<(DateTime<Utc>, i32)>, NewsError> {
    let mut latest = None;
    let mut count = 0i32;
    let mut t = lower;
    // A week of minutely slots at most; schedules are a few a day.
    for _ in 0..20_000 {
        let next = news_standing::next_slot(cron, tz, t)?;
        if next > now {
            break;
        }
        latest = Some(next);
        count += 1;
        t = next;
    }
    Ok(latest.map(|l| (l, count - 1)))
}

/// What Rota decided for one schedule.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum Fire {
    Missed {
        slot: DateTime<Utc>,
        missed_before: i32,
    },
    Fired {
        slot: DateTime<Utc>,
        missed_before: i32,
        outcome: Started,
    },
}

pub struct Rota {
    db: PgPool,
    chief: Chief,
    clock: Arc<dyn Clock>,
    policy: RotaPolicy,
    usage: Arc<dyn UsageSource>,
    emit: news_chief::Emit,
}

impl Rota {
    pub fn new(
        db: PgPool,
        chief: Chief,
        clock: Arc<dyn Clock>,
        policy: RotaPolicy,
        usage: Arc<dyn UsageSource>,
        emit: news_chief::Emit,
    ) -> Self {
        Self {
            db,
            chief,
            clock,
            policy,
            usage,
            emit,
        }
    }

    pub fn policy(&self) -> &RotaPolicy {
        &self.policy
    }

    /// One tick: refresh Standing and usage, size the `ai` pool, then the schedules.
    pub async fn tick(&self) -> Result<Vec<(String, Fire)>, NewsError> {
        let now = self.clock.now();
        let standing = news_standing::current(&self.db).await?;
        let tz = news_standing::time_zone(&standing);
        self.policy.set_standing(standing.clone());
        let stale = self
            .policy
            .state()
            .read_at
            .is_none_or(|t| now - t >= Duration::seconds(60));
        if stale {
            let (day, week) = day_and_week_start(tz, now);
            let u = self.usage.read(day, week).await.ok();
            self.policy.set_usage(u, now);
        }
        // The `ai` pool: Standing's size, or quiet hours' slots in reduced mode.
        let reduced = in_quiet_hours(&standing, tz, now)
            && standing.text("quiet_hours.mode").as_deref() == Some("reduced");
        let ai = if reduced {
            standing.int("quiet_hours.ai_slots")
        } else {
            standing.int("pools.ai.max_running")
        };
        news_dispatch::queue::set_max_running(&self.db, "ai", ai.clamp(0, 32) as i16).await?;
        self.schedules(&standing, tz, now).await
    }

    async fn schedules(
        &self,
        standing: &Standing,
        tz: chrono_tz::Tz,
        now: DateTime<Utc>,
    ) -> Result<Vec<(String, Fire)>, NewsError> {
        let mut out = Vec::new();
        for (key, s) in standing.schedules() {
            if !s.enabled {
                continue;
            }
            let last: Option<DateTime<Utc>> =
                sqlx::query_scalar("select max(slot_at) from rota_fires where schedule_key = $1")
                    .bind(&key)
                    .fetch_one(&self.db)
                    .await
                    .map_err(map_err)?;
            let changed = news_standing::last_changed(&self.db, &key).await?;
            let lower = last.map_or(changed, |l| l.max(changed));
            let Some((slot, missed_before)) = latest_due(&s.cron, tz, lower, now)? else {
                continue;
            };
            let late = now - slot > Duration::minutes(1);
            let next = news_standing::next_slot(&s.cron, tz, slot)?;
            let limit = Duration::minutes(standing.int("rota.missed_slot_limit_minutes"));
            let fire = if late && next - now < limit {
                let r = sqlx::query(
                    "insert into rota_fires (schedule_key, slot_at, outcome, reason, missed_before, settings_version, fired_at)
                     values ($1, $2, 'missed', 'next_slot_close', $3, $4, $5) on conflict do nothing",
                )
                .bind(&key)
                .bind(slot)
                .bind(missed_before)
                .bind(standing.version)
                .bind(now)
                .execute(&self.db)
                .await
                .map_err(map_err)?;
                if r.rows_affected() == 0 {
                    continue;
                }
                Fire::Missed {
                    slot,
                    missed_before,
                }
            } else {
                // Rule 2 (the late limit) is Chief's: every start checks `start_by` first.
                let req = StartReq {
                    preset: &s.preset,
                    args: s.args.clone(),
                    origin: Origin::Schedule,
                    schedule: Some((&key, slot)),
                    if_busy: IfBusy::Queue,
                    missed_before,
                };
                match self.chief.start(req).await {
                    Ok(outcome) => Fire::Fired {
                        slot,
                        missed_before,
                        outcome,
                    },
                    Err(e) if e.code == news_core::ErrorCode::Conflict => continue,
                    Err(e) => {
                        tracing::warn!(schedule = %key, error = %e, "schedule fire failed");
                        continue;
                    }
                }
            };
            (self.emit)("schedule", schedule_event(&key, &s.preset, &fire));
            out.push((key, fire));
        }
        Ok(out)
    }
}

fn schedule_event(key: &str, preset: &str, f: &Fire) -> Value {
    let (slot, missed_before, outcome, reason, run) = match f {
        Fire::Missed {
            slot,
            missed_before,
        } => (
            *slot,
            *missed_before,
            "missed",
            Some("next_slot_close".to_owned()),
            None,
        ),
        Fire::Fired {
            slot,
            missed_before,
            outcome,
        } => match outcome {
            Started::Running(r) => (*slot, *missed_before, "started", None, Some(r)),
            Started::Queued { run, .. } => (*slot, *missed_before, "queued", None, Some(run)),
            Started::Skipped { run, reason } => (
                *slot,
                *missed_before,
                "skipped",
                Some(reason.clone()),
                Some(run),
            ),
        },
    };
    let text = match (outcome, &reason) {
        ("missed", _) => format!("{key}: the {slot} slot was missed (the next one is close)"),
        ("skipped", Some(r)) => format!("{key}: the {slot} slot was skipped ({r})"),
        (o, _) => format!("{key}: {preset} {o} for the {slot} slot"),
    };
    json!({"schedule_key": key, "preset": preset, "slot_at": slot, "outcome": outcome, "reason": reason,
           "missed_before": missed_before, "run_id": run.map(|r| r.run_id), "short_id": run.map(|r| &r.short_id),
           "text": text})
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use chrono_tz::Tz;

    fn utc(s: &str) -> DateTime<Utc> {
        s.parse().unwrap_or_else(|e| panic!("{s}: {e}"))
    }

    #[test]
    fn only_the_latest_due_slot_is_taken_and_older_ones_counted() {
        let tz: Tz = chrono_tz::UTC;
        let got = latest_due(
            "0 */6 * * *",
            tz,
            utc("2026-11-12T00:30:00Z"),
            utc("2026-11-12T19:00:00Z"),
        )
        .unwrap_or_default();
        assert_eq!(got, Some((utc("2026-11-12T18:00:00Z"), 2)));
        assert_eq!(
            latest_due(
                "0 11 * * *",
                tz,
                utc("2026-11-12T11:00:00Z"),
                utc("2026-11-12T19:00:00Z")
            )
            .unwrap_or_default(),
            None
        );
    }

    #[test]
    fn schedules_follow_the_time_zone_across_dst() {
        let ny: Tz = "America/New_York".parse().unwrap_or(chrono_tz::UTC);
        // 08 Mar 2026: 02:30 does not exist in New York; it fires at the first instant after the gap.
        let t = news_standing::next_slot("30 2 * * *", ny, utc("2026-03-08T05:00:00Z"))
            .unwrap_or(utc("2000-01-01T00:00:00Z"));
        assert_eq!(
            t.with_timezone(&ny).date_naive().to_string(),
            "2026-03-08",
            "{t}"
        );
        assert!(
            t.with_timezone(&ny).format("%H:%M").to_string().as_str() >= "03:00",
            "{t}"
        );
        // 01 Nov 2026: 01:30 happens twice; it fires once, at the first.
        let first = news_standing::next_slot("30 1 * * *", ny, utc("2026-11-01T04:00:00Z"))
            .unwrap_or(utc("2000-01-01T00:00:00Z"));
        assert_eq!(first, utc("2026-11-01T05:30:00Z"));
        let after = news_standing::next_slot("30 1 * * *", ny, first)
            .unwrap_or(utc("2000-01-01T00:00:00Z"));
        assert_eq!(
            after.with_timezone(&ny).date_naive().to_string(),
            "2026-11-02",
            "{after}"
        );
        let akl: Tz = "Pacific/Auckland".parse().unwrap_or(chrono_tz::UTC);
        let t = news_standing::next_slot("0 7 * * *", akl, utc("2026-11-12T00:00:00Z"))
            .unwrap_or(utc("2000-01-01T00:00:00Z"));
        assert_eq!(t, utc("2026-11-12T18:00:00Z"));
    }

    #[test]
    fn quiet_hours_wrap_midnight() {
        let mut s = Standing::defaults();
        s.settings.insert("quiet_hours.enabled".into(), json!(true));
        s.settings
            .insert("quiet_hours.start".into(), json!("23:00"));
        s.settings.insert("quiet_hours.end".into(), json!("07:00"));
        let tz = chrono_tz::UTC;
        assert!(in_quiet_hours(&s, tz, utc("2026-11-12T23:30:00Z")));
        assert!(in_quiet_hours(&s, tz, utc("2026-11-12T06:59:00Z")));
        assert!(!in_quiet_hours(&s, tz, utc("2026-11-12T07:00:00Z")));
        assert!(!in_quiet_hours(&s, tz, utc("2026-11-12T12:00:00Z")));
        s.settings
            .insert("quiet_hours.enabled".into(), json!(false));
        assert!(!in_quiet_hours(&s, tz, utc("2026-11-12T23:30:00Z")));
    }

    #[test]
    fn the_policy_holds_heavy_starts_in_dont_start_quiet_hours_and_refuses_at_the_quota() {
        let p = RotaPolicy::default();
        let mut s = Standing::defaults();
        s.settings.insert("quiet_hours.enabled".into(), json!(true));
        s.settings.insert("rota.time_zone".into(), json!("UTC"));
        p.set_standing(s.clone());
        let heavy = news_chief::presets::preset("edition").unwrap_or_else(|| panic!("edition"));
        let light = news_chief::presets::preset("frontpage").unwrap_or_else(|| panic!("frontpage"));
        let night = utc("2026-11-12T23:30:00Z");
        assert!(matches!(
            p.check(heavy, Origin::Schedule, night),
            Policy::Hold { .. }
        ));
        assert_eq!(p.check(light, Origin::Schedule, night), Policy::Allow);
        s.settings
            .insert("quiet_hours.mode".into(), json!("reduced"));
        p.set_standing(s.clone());
        assert_eq!(p.check(heavy, Origin::Schedule, night), Policy::Allow);

        s.settings
            .insert("quota.bureau_daily_tokens".into(), json!(1000));
        s.settings
            .insert("quota.reserve_daily_tokens".into(), json!(5000));
        p.set_standing(s);
        p.set_usage(None, night);
        assert_eq!(p.ai_claim(night), AiClaim::Allow, "unknown usage allows");
        assert_eq!(p.usage()["known"], false);
        p.set_usage(
            Some(Usage {
                bureau_today: 10,
                total_today: 6000,
                ..Usage::default()
            }),
            night,
        );
        assert_eq!(p.ai_claim(night), AiClaim::Reserve);
        assert!(matches!(
            p.check(light, Origin::Board, night),
            Policy::Refuse { .. }
        ));
        p.set_usage(
            Some(Usage {
                bureau_today: 1000,
                ..Usage::default()
            }),
            night,
        );
        assert_eq!(p.ai_claim(night), AiClaim::Cap);
        // The cap stops new starts too, light or heavy, scheduled or from Board.
        for (preset, origin) in [(light, Origin::Board), (heavy, Origin::Schedule)] {
            assert!(matches!(
                p.check(preset, origin, night),
                Policy::Refuse { reason, .. } if reason == "quota"
            ));
        }
        p.set_usage(Some(Usage::default()), night);
        assert_eq!(p.ai_claim(night), AiClaim::Allow);
        assert_eq!(p.check(light, Origin::Board, night), Policy::Allow);
    }

    #[test]
    fn tokens_leave_out_cache_reads_and_days_are_local() {
        assert_eq!(tokens(10, 20, 30, 1_000_000), 60);
        let ny: Tz = "America/New_York".parse().unwrap_or(chrono_tz::UTC);
        // Thu 12 Nov 2026 02:00 UTC is Wed 11 Nov 21:00 in New York; its ISO week starts Mon 9 Nov.
        let (day, week) = day_and_week_start(ny, utc("2026-11-12T02:00:00Z"));
        assert_eq!(day, utc("2026-11-11T05:00:00Z"));
        assert_eq!(week, utc("2026-11-09T05:00:00Z"));
    }
}
