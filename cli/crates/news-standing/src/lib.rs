//! Standing: Karthik's settings, versioned in PostgreSQL (ADR 0008 §10).
//!
//! One row of `standing` per version holds the full map of explicitly set keys; a key that is not set
//! takes its registry default. [`set`] writes one new version from a `base_version` that must be the
//! latest (`conflict` otherwise), recording each change's old and new effective value and `who`
//! (always `board`: the only writer is the Hotline's `standing.set`, sent from Board). Every run
//! records the version it started with (`runs.settings_version`).
//!
//! No key is ever a secret.

use chrono::{DateTime, Utc};
use news_core::NewsError;
use news_store::map_err;
use serde::Serialize;
use serde_json::{Map, Value, json};
use sqlx::{PgConnection, PgPool, Row};

/// The type of a key's value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyType {
    Integer {
        min: i64,
        max: i64,
    },
    Boolean,
    Text,
    /// A `claude -p --model` value: an alias or model id, `^[a-z0-9][a-z0-9.\-\[\]]{0,63}$`, so it can
    /// never carry a flag, a space or a shell character onto the command line.
    Model,
    /// `HH:MM`, 24-hour, local.
    TimeOfDay,
    /// An IANA zone name; null means the laptop's zone.
    TimeZone,
    Enum(&'static [&'static str]),
    /// A token count; null means no limit.
    Tokens,
    /// A decimal number as published (a share such as 0.05, never rescaled), bounded in thousandths.
    Decimal {
        min_milli: i64,
        max_milli: i64,
    },
    /// `{"preset", "args", "cron", "enabled"}` (only under `schedules.`).
    Schedule,
}

impl KeyType {
    pub fn name(self) -> &'static str {
        match self {
            KeyType::Integer { .. } => "integer",
            KeyType::Boolean => "boolean",
            // A model is text on the wire; only the check is narrower.
            KeyType::Text | KeyType::Model => "text",
            KeyType::TimeOfDay => "time_of_day",
            KeyType::TimeZone => "time_zone",
            KeyType::Enum(_) => "enum",
            KeyType::Tokens => "tokens",
            KeyType::Decimal { .. } => "decimal",
            KeyType::Schedule => "schedule",
        }
    }
}

/// One registered key. `default` is JSON text.
#[derive(Debug, Clone, Copy)]
pub struct KeyDecl {
    pub key: &'static str,
    pub ty: KeyType,
    pub default: &'static str,
    pub doc: &'static str,
}

const fn key(key: &'static str, ty: KeyType, default: &'static str, doc: &'static str) -> KeyDecl {
    KeyDecl {
        key,
        ty,
        default,
        doc,
    }
}

const MINUTES: KeyType = KeyType::Integer {
    min: 1,
    max: 10_080,
};

/// The keys #10 registers (ADR 0008 §10). `schedules.<id>` keys are matched by [`SCHEDULE_PREFIX`].
pub const REGISTRY: &[KeyDecl] = &[
    key(
        "board.frontpage_due_days",
        KeyType::Integer { min: 1, max: 60 },
        "2",
        "Frontpage is due this many days after the live curation",
    ),
    key(
        "board.huddle_every_days",
        KeyType::Integer { min: 1, max: 60 },
        "7",
        "Huddle is due this many days after the last one",
    ),
    key(
        "board.rollcall_every_days",
        KeyType::Integer { min: 1, max: 60 },
        "7",
        "Rollcall is due this many days after the last Sitrep",
    ),
    key(
        "chief.stall_minutes",
        MINUTES,
        "15",
        "A run is stalled when nothing of it has been seen for this long",
    ),
    key(
        "home.stale_days",
        KeyType::Integer { min: 1, max: 60 },
        "4",
        "The live home curation is stale after this many days",
    ),
    key(
        "lineup.queue_late_limit_minutes",
        MINUTES,
        "120",
        "A scheduled run that cannot start within this long of its slot is skipped",
    ),
    key(
        "models.a1",
        KeyType::Model,
        "null",
        "Model for step A1, Write page (new, or update after a direction flip) (a claude -p --model value; null: the role's model)",
    ),
    key(
        "models.a10",
        KeyType::Model,
        "null",
        "Model for step A10, Alt text when the image source gives none (a claude -p --model value; null: the role's model)",
    ),
    key(
        "models.a11",
        KeyType::Model,
        "null",
        "Model for step A11, Rollcall proposals (a claude -p --model value; null: the role's model)",
    ),
    key(
        "models.a12",
        KeyType::Model,
        "null",
        "Model for step A12, Eval judge (a claude -p --model value; null: the role's model)",
    ),
    key(
        "models.a2",
        KeyType::Model,
        "null",
        "Model for step A2, Repair a Copy (a claude -p --model value; null: the role's model)",
    ),
    key(
        "models.a3",
        KeyType::Model,
        "null",
        "Model for step A3, Classify an ambiguous item (a claude -p --model value; null: the role's model)",
    ),
    key(
        "models.a4",
        KeyType::Model,
        "null",
        "Model for step A4, Same story? (grey zone) (a claude -p --model value; null: the role's model)",
    ),
    key(
        "models.a5",
        KeyType::Model,
        "null",
        "Model for step A5, Condense long source text for a brief (a claude -p --model value; null: the role's model)",
    ),
    key(
        "models.a6",
        KeyType::Model,
        "null",
        "Model for step A6, Pick an entity from a shortlist (a claude -p --model value; null: the role's model)",
    ),
    key(
        "models.a7",
        KeyType::Model,
        "null",
        "Model for step A7, Sample spot check (a claude -p --model value; null: the role's model)",
    ),
    key(
        "models.a8",
        KeyType::Model,
        "null",
        "Model for step A8, Home curation (Frontpage) (a claude -p --model value; null: the role's model)",
    ),
    key(
        "models.a9",
        KeyType::Model,
        "null",
        "Model for step A9, Triage a reader error report (a claude -p --model value; null: the role's model)",
    ),
    key(
        "models.curator",
        KeyType::Model,
        "\"sonnet\"",
        "Model role for the home page curator (A8) unless that step has its own",
    ),
    key(
        "models.light",
        KeyType::Model,
        "\"haiku\"",
        "Model role for the light steps (A3 to A7, A9, A10) unless a step has its own",
    ),
    key(
        "models.main",
        KeyType::Model,
        "\"sonnet\"",
        "Model role for the main drafting steps (A1, A2, A11, A12) unless a step has its own",
    ),
    key(
        "next.dead_jobs_min",
        KeyType::Integer {
            min: 1,
            max: 100_000,
        },
        "10",
        "Next-action rule 7: dead jobs since the last Salvage",
    ),
    key(
        "next.edition_gap_hours",
        KeyType::Integer { min: 1, max: 168 },
        "6",
        "Next-action rule 6: hours since the last Edition",
    ),
    key(
        "next.stories_score_min",
        KeyType::Integer { min: 0, max: 100 },
        "40",
        "Next-action rule 6: the score a waiting story needs",
    ),
    key(
        "next.stories_waiting_min",
        KeyType::Integer {
            min: 1,
            max: 100_000,
        },
        "20",
        "Next-action rule 6: stories waiting",
    ),
    key(
        "next.tier1_flash_max",
        KeyType::Integer { min: 0, max: 10 },
        "3",
        "Next-action rule 2: Flash instead of Edition up to this many Tier 1 stories",
    ),
    key(
        "pools.ai.max_running",
        KeyType::Integer { min: 1, max: 32 },
        "3",
        "Parallel AI calls (claude -p); replaces NEWS_PARALLEL",
    ),
    key(
        "proof.sample_rate",
        KeyType::Decimal {
            min_milli: 0,
            max_milli: 1000,
        },
        "0.05",
        "Share of pages given the Haiku spot check (a share from 0 to 1, as written)",
    ),
    key(
        "publish.index_per_day",
        KeyType::Integer {
            min: 0,
            max: 10_000,
        },
        "20",
        "Pages offered to search engines per day",
    ),
    key(
        "quiet_hours.ai_slots",
        KeyType::Integer { min: 0, max: 32 },
        "1",
        "AI slots during quiet hours in reduced mode",
    ),
    key(
        "quiet_hours.enabled",
        KeyType::Boolean,
        "false",
        "Quiet hours on or off",
    ),
    key(
        "quiet_hours.end",
        KeyType::TimeOfDay,
        "\"07:00\"",
        "Quiet hours end (local time)",
    ),
    key(
        "quiet_hours.mode",
        KeyType::Enum(&["dont_start", "reduced"]),
        "\"dont_start\"",
        "dont_start holds heavy starts; reduced starts them with fewer AI slots",
    ),
    key(
        "quiet_hours.start",
        KeyType::TimeOfDay,
        "\"23:00\"",
        "Quiet hours start (local time; may wrap midnight)",
    ),
    key(
        "quota.ai_run_tokens",
        KeyType::Tokens,
        "2500000",
        "Token ceiling per run (input-equivalent): new AI jobs stop when it is reached and the run finishes its code work; null: no ceiling",
    ),
    key(
        "quota.bureau_daily_tokens",
        KeyType::Tokens,
        "null",
        "Cap on Bureau's own tokens per local day (null: no cap)",
    ),
    key(
        "quota.bureau_weekly_tokens",
        KeyType::Tokens,
        "null",
        "Cap on Bureau's own tokens per ISO week (null: no cap)",
    ),
    key(
        "quota.reserve_daily_tokens",
        KeyType::Tokens,
        "null",
        "Stop AI claims when all Claude Code usage today reaches this (null: none)",
    ),
    key(
        "quota.reserve_weekly_tokens",
        KeyType::Tokens,
        "null",
        "Stop AI claims when all Claude Code usage this week reaches this (null: none)",
    ),
    key(
        "retain.raw_days_data",
        KeyType::Integer { min: 1, max: 3650 },
        "400",
        "Days to keep raw structured snapshots (JSON, XBRL, CSV, SDMX)",
    ),
    key(
        "retain.raw_days_docs",
        KeyType::Integer { min: 1, max: 3650 },
        "90",
        "Days to keep raw HTML and PDF snapshots",
    ),
    key(
        "rota.missed_slot_limit_minutes",
        MINUTES,
        "120",
        "A late slot is dropped when the next slot is under this many minutes away",
    ),
    key(
        "rota.time_zone",
        KeyType::TimeZone,
        "null",
        "Time zone for schedules, quiet hours and quota days (null: the laptop's)",
    ),
    key(
        "run.backfill_from",
        KeyType::Text,
        "null",
        "Earliest date for backfill, YYYY-MM-DD (null: the start of the current year)",
    ),
    key(
        "run.target_pages",
        KeyType::Integer {
            min: 1,
            max: 10_000,
        },
        "100",
        "Suggested pages per run (not a hard limit)",
    ),
    key(
        "wire.contact_email",
        KeyType::Text,
        "null",
        "Contact e-mail sent in the User-Agent only to sources that require one (SEC EDGAR); a dedicated address, never a secret",
    ),
];

/// Schedules are keys `schedules.<id>`, `id` matching `^[a-z0-9-]{1,32}$`.
pub const SCHEDULE_PREFIX: &str = "schedules.";
const SCHEDULE_DOC: &str =
    "A preset schedule: {preset, args, cron (five fields, in rota.time_zone), enabled}";

/// Sources on or off are keys `sources.<source_id>.enabled` (`^[a-z0-9-]{1,48}$`): true or false
/// overrides the Contacts file's `enabled`, null (unset) leaves the file in charge.
pub const SOURCE_PREFIX: &str = "sources.";
const SOURCE_DOC: &str = "A source on or off; unset: the Contacts file decides";

pub fn is_source_key(key: &str) -> bool {
    key.strip_prefix(SOURCE_PREFIX)
        .and_then(|r| r.strip_suffix(".enabled"))
        .is_some_and(|id| {
            (1..=48).contains(&id.len())
                && id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}

/// Keys that are set explicitly and are not in the registry: schedules and source switches.
fn is_dynamic_key(key: &str) -> bool {
    key.starts_with(SCHEDULE_PREFIX) || is_source_key(key)
}

/// The declaration of `key`, including `schedules.<id>` and `sources.<id>.enabled`.
pub fn decl(key: &str) -> Option<KeyDecl> {
    if is_source_key(key) {
        return Some(KeyDecl {
            key: "sources.*.enabled",
            ty: KeyType::Boolean,
            default: "null",
            doc: SOURCE_DOC,
        });
    }
    if let Some(id) = key.strip_prefix(SCHEDULE_PREFIX) {
        return valid_schedule_id(id).then_some(KeyDecl {
            key: "schedules.*",
            ty: KeyType::Schedule,
            default: "null",
            doc: SCHEDULE_DOC,
        });
    }
    REGISTRY.iter().find(|d| d.key == key).copied()
}

pub fn valid_schedule_id(id: &str) -> bool {
    (1..=32).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn default_of(d: &KeyDecl) -> Value {
    serde_json::from_str(d.default).unwrap_or(Value::Null)
}

/// A schedule's value.
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Schedule {
    pub preset: String,
    pub args: Map<String, Value>,
    pub cron: String,
    pub enabled: bool,
}

/// Checks a schedule's preset and arguments; supplied by the caller (Chief knows the presets), so this
/// crate never depends on Chief.
pub type ScheduleCheck<'a> = &'a (dyn Fn(&Schedule) -> Result<(), NewsError> + Sync);

/// Parse a 5-field cron expression.
pub fn parse_cron(expr: &str) -> Result<croner::Cron, NewsError> {
    if expr.split_whitespace().count() != 5 {
        return Err(NewsError::validation(format!(
            "cron {expr:?} must have five fields"
        )));
    }
    expr.parse::<croner::Cron>()
        .map_err(|e| NewsError::validation(format!("invalid cron expression {expr:?}: {e}")))
}

/// A `claude -p --model` value: `^[a-z0-9][a-z0-9.\-\[\]]{0,63}$` (`sonnet`, `claude-opus-5-5`,
/// `opus[1m]`).
pub fn is_model_name(s: &str) -> bool {
    let mut chars = s.chars();
    s.len() <= 64
        && chars
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && chars.all(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '-' | '[' | ']')
        })
}

/// Check `value` against the key's type. Null is the default for every key.
pub fn validate(key: &str, value: &Value, check: ScheduleCheck<'_>) -> Result<(), NewsError> {
    let d = decl(key).ok_or_else(|| NewsError::validation(format!("unknown setting {key:?}")))?;
    if value.is_null() {
        return Ok(());
    }
    let bad = |want: &str| {
        Err(NewsError::validation(format!(
            "{key} must be {want}, not {value}"
        )))
    };
    match d.ty {
        KeyType::Integer { min, max } => match value.as_i64() {
            Some(n) if (min..=max).contains(&n) => Ok(()),
            _ => bad(&format!("an integer from {min} to {max}")),
        },
        KeyType::Tokens => match value.as_i64() {
            Some(n) if n >= 0 => Ok(()),
            _ => bad("a token count (0 or more) or null"),
        },
        KeyType::Decimal {
            min_milli,
            max_milli,
        } => match value.as_f64() {
            Some(n) if n >= min_milli as f64 / 1000.0 && n <= max_milli as f64 / 1000.0 => Ok(()),
            _ => bad(&format!(
                "a number from {} to {}",
                min_milli as f64 / 1000.0,
                max_milli as f64 / 1000.0
            )),
        },
        KeyType::Boolean if value.is_boolean() => Ok(()),
        KeyType::Boolean => bad("true or false"),
        KeyType::Text if value.is_string() => Ok(()),
        KeyType::Text => bad("text"),
        KeyType::Model => match value.as_str() {
            Some(s) if is_model_name(s) => Ok(()),
            _ => bad("a model name: lower-case letters, digits, '.', '-', '[' or ']', at most 64"),
        },
        KeyType::TimeOfDay => match value.as_str().and_then(parse_hhmm) {
            Some(_) => Ok(()),
            None => bad("a time of day, HH:MM"),
        },
        KeyType::TimeZone => match value.as_str().map(str::parse::<chrono_tz::Tz>) {
            Some(Ok(_)) => Ok(()),
            _ => bad("an IANA time zone name"),
        },
        KeyType::Enum(choices) => match value.as_str() {
            Some(s) if choices.contains(&s) => Ok(()),
            _ => bad(&format!("one of {choices:?}")),
        },
        KeyType::Schedule => {
            let s: Schedule = serde_json::from_value(value.clone()).map_err(|e| {
                NewsError::validation(format!(
                    "{key} must be {{preset, args, cron, enabled}}: {e}"
                ))
            })?;
            parse_cron(&s.cron)?;
            check(&s)
        }
    }
}

/// The time zone schedules, quiet hours and quota days use: `rota.time_zone`, else the laptop's zone,
/// else UTC.
pub fn time_zone(s: &Standing) -> chrono_tz::Tz {
    s.text("rota.time_zone")
        .or_else(|| iana_time_zone::get_timezone().ok())
        .and_then(|z| z.parse().ok())
        .unwrap_or(chrono_tz::UTC)
}

/// The first slot of `cron` strictly after `after`, evaluated in `tz` (ADR 0008 §7.1). A local time
/// that does not exist (spring forward) fires at the first instant after the gap; one that occurs twice
/// (fall back) fires once, at the first.
pub fn next_slot(
    cron: &str,
    tz: chrono_tz::Tz,
    after: DateTime<Utc>,
) -> Result<DateTime<Utc>, NewsError> {
    let c = parse_cron(cron)?;
    c.find_next_occurrence(&after.with_timezone(&tz), false)
        .map(|t| t.with_timezone(&Utc))
        .map_err(|e| NewsError::validation(format!("cron {cron:?} has no next occurrence: {e}")))
}

/// `HH:MM` as minutes after midnight.
pub fn parse_hhmm(s: &str) -> Option<u32> {
    let (h, m) = s.split_once(':')?;
    if h.len() != 2 || m.len() != 2 {
        return None;
    }
    let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24 && m < 60).then_some(h * 60 + m)
}

/// One version of Standing, with typed reads of effective values.
#[derive(Debug, Clone, PartialEq)]
pub struct Standing {
    pub version: i32,
    pub created_at: DateTime<Utc>,
    /// Explicitly set keys.
    pub settings: Map<String, Value>,
}

impl Standing {
    /// The registry defaults, as version 1 holds them.
    pub fn defaults() -> Self {
        Self {
            version: 1,
            created_at: DateTime::<Utc>::UNIX_EPOCH,
            settings: Map::new(),
        }
    }

    /// The effective value: explicit, else the default (null for an unknown key).
    pub fn get(&self, key: &str) -> Value {
        match self.settings.get(key) {
            Some(v) if !v.is_null() => v.clone(),
            _ => decl(key).map_or(Value::Null, |d| default_of(&d)),
        }
    }

    pub fn int(&self, key: &str) -> i64 {
        self.get(key).as_i64().unwrap_or(0)
    }

    pub fn opt_int(&self, key: &str) -> Option<i64> {
        self.get(key).as_i64()
    }

    pub fn bool(&self, key: &str) -> bool {
        self.get(key).as_bool().unwrap_or(false)
    }

    pub fn text(&self, key: &str) -> Option<String> {
        self.get(key).as_str().map(str::to_owned)
    }

    /// The schedules, sorted by key; ones that do not parse are left out.
    pub fn schedules(&self) -> Vec<(String, Schedule)> {
        let mut out: Vec<(String, Schedule)> = self
            .settings
            .iter()
            .filter(|(k, _)| k.starts_with(SCHEDULE_PREFIX))
            .filter_map(|(k, v)| Some((k.clone(), serde_json::from_value(v.clone()).ok()?)))
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// Every effective value: the registry keys and the set schedules. Runs snapshot this as `config`.
    pub fn effective(&self) -> Map<String, Value> {
        let mut m: Map<String, Value> = REGISTRY
            .iter()
            .map(|d| (d.key.to_owned(), self.get(d.key)))
            .collect();
        for (k, v) in &self.settings {
            if is_dynamic_key(k) {
                m.insert(k.clone(), v.clone());
            }
        }
        m
    }

    /// The `standing.get` data: every key (or those under `prefix`) with value, default and type.
    pub fn items(&self, prefix: Option<&str>) -> Value {
        let mut keys: Vec<String> = REGISTRY.iter().map(|d| d.key.to_owned()).collect();
        keys.extend(self.settings.keys().filter(|k| is_dynamic_key(k)).cloned());
        keys.sort();
        let items: Vec<Value> = keys
            .iter()
            .filter(|k| prefix.is_none_or(|p| k.starts_with(p)))
            .filter_map(|k| {
                let d = decl(k)?;
                let (choices, min, max) = match d.ty {
                    KeyType::Enum(c) => (json!(c), Value::Null, Value::Null),
                    KeyType::Integer { min, max } => (Value::Null, json!(min), json!(max)),
                    _ => (Value::Null, Value::Null, Value::Null),
                };
                Some(json!({
                    "key": k,
                    "value": self.get(k),
                    "default": default_of(&d),
                    "is_set": self.settings.get(k).is_some_and(|v| !v.is_null()),
                    "type": d.ty.name(),
                    "doc": d.doc,
                    "choices": choices,
                    "min": min,
                    "max": max,
                }))
            })
            .collect();
        json!({ "version": self.version, "created_at": self.created_at, "items": items })
    }
}

fn row_to_standing(row: &sqlx::postgres::PgRow) -> Standing {
    let settings: Value = row.get("settings");
    Standing {
        version: row.get("version"),
        created_at: row.get("created_at"),
        settings: match settings {
            Value::Object(m) => m,
            _ => Map::new(),
        },
    }
}

/// The latest version.
pub async fn current(db: &PgPool) -> Result<Standing, NewsError> {
    let mut conn = db.acquire().await.map_err(map_err)?;
    current_on(&mut conn).await
}

/// [`current`] on a given connection or transaction.
pub async fn current_on(conn: &mut PgConnection) -> Result<Standing, NewsError> {
    let row = sqlx::query(
        "select version, settings, created_at from standing order by version desc limit 1",
    )
    .fetch_optional(&mut *conn)
    .await
    .map_err(map_err)?
    .ok_or_else(|| NewsError::precondition("standing has no version; run `news db migrate`"))?;
    Ok(row_to_standing(&row))
}

/// One version.
pub async fn at(db: &PgPool, version: i32) -> Result<Standing, NewsError> {
    let row = sqlx::query("select version, settings, created_at from standing where version = $1")
        .bind(version)
        .fetch_optional(db)
        .await
        .map_err(map_err)?
        .ok_or_else(|| NewsError::not_found(format!("standing version {version}")))?;
    Ok(row_to_standing(&row))
}

/// One entry of the history.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Version {
    pub version: i32,
    pub who: String,
    pub created_at: DateTime<Utc>,
    pub note: Option<String>,
    pub changes: Value,
}

/// Versions, newest first, `limit` at most, older than `before` when given. With `key`, only versions
/// that changed it, each with only that key's change.
pub async fn history(
    db: &PgPool,
    key: Option<&str>,
    limit: i64,
    before: Option<i32>,
) -> Result<Vec<Version>, NewsError> {
    let rows = sqlx::query(
        "select version, who, created_at, note,
                case when $1::text is null then changes
                     else (select coalesce(jsonb_agg(c), '[]') from jsonb_array_elements(changes) c where c->>'key' = $1)
                end as changes
           from standing
          where ($2::int is null or version < $2)
            and ($1::text is null or exists (select 1 from jsonb_array_elements(changes) c where c->>'key' = $1))
          order by version desc limit $3",
    )
    .bind(key)
    .bind(before)
    .bind(limit)
    .fetch_all(db)
    .await
    .map_err(map_err)?;
    Ok(rows
        .iter()
        .map(|r| Version {
            version: r.get("version"),
            who: r.get("who"),
            created_at: r.get("created_at"),
            note: r.get("note"),
            changes: r.get("changes"),
        })
        .collect())
}

/// What [`set`] wrote.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SetResult {
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub changes: Vec<Value>,
}

/// Write one new version (`who = 'board'`). `base_version` must be the latest (`conflict` otherwise,
/// with the latest version in `details`). Unknown keys, wrong types and bad schedules are
/// `validation_failed`. A change that leaves the effective value as it was is dropped; if nothing is
/// left, it is `validation_failed` ("no change").
pub async fn set(
    db: &PgPool,
    base_version: i32,
    changes: &[(String, Value)],
    note: Option<&str>,
    check: ScheduleCheck<'_>,
) -> Result<SetResult, NewsError> {
    for (k, v) in changes {
        validate(k, v, check)?;
    }
    let mut tx = db.begin().await.map_err(map_err)?;
    // Serialise writers: the latest version is locked, so a second writer from the same base waits and
    // then sees it is stale.
    let latest = sqlx::query(
        "select version, settings, created_at from standing order by version desc limit 1 for update",
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(map_err)?;
    let latest = row_to_standing(&latest);
    if latest.version != base_version {
        return Err(NewsError::conflict(format!(
            "settings changed since version {base_version}; the latest is {}",
            latest.version
        ))
        .with_details(json!({"reason": "stale_base", "latest_version": latest.version}))
        .with_text(format!(
            "Settings were changed elsewhere (now version {}). Reload and apply your change again.",
            latest.version
        )));
    }
    let mut settings = latest.settings.clone();
    let mut recorded = Vec::new();
    for (k, v) in changes {
        let old = latest.get(k);
        if v.is_null() {
            settings.remove(k);
        } else {
            settings.insert(k.clone(), v.clone());
        }
        let next = Standing {
            settings: settings.clone(),
            ..latest.clone()
        };
        let new = next.get(k);
        if old != new {
            recorded.push(json!({"key": k, "old": old, "new": new}));
        }
    }
    if recorded.is_empty() {
        return Err(NewsError::validation(
            "no change: every value is already in effect",
        ));
    }
    let row = sqlx::query(
        "insert into standing (version, settings, changes, who, note)
         values ($1, $2, $3, 'board', $4)
         returning created_at",
    )
    .bind(latest.version + 1)
    .bind(Value::Object(settings))
    .bind(Value::Array(recorded.clone()))
    .bind(note)
    .fetch_one(&mut *tx)
    .await
    .map_err(map_err)?;
    tx.commit().await.map_err(map_err)?;
    Ok(SetResult {
        version: latest.version + 1,
        created_at: row.get("created_at"),
        changes: recorded,
    })
}

/// The `created_at` of the version that last changed `key` (or 1 when nothing ever did): Rota's lower
/// bound for a schedule (ADR 0008 §7.1).
pub async fn last_changed(db: &PgPool, key: &str) -> Result<DateTime<Utc>, NewsError> {
    sqlx::query_scalar(
        "select created_at from standing
          where version = 1 or exists (select 1 from jsonb_array_elements(changes) c where c->>'key' = $1)
          order by version desc limit 1",
    )
    .bind(key)
    .fetch_one(db)
    .await
    .map_err(map_err)
}

/// Code-rendered text for a set.
pub fn set_text(r: &SetResult) -> String {
    let mut s = format!("Settings saved as version {}.", r.version);
    for c in &r.changes {
        s.push_str(&format!(
            "\n  {}: {} -> {}",
            c["key"].as_str().unwrap_or("?"),
            c["old"],
            c["new"]
        ));
    }
    s
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn ok(_: &Schedule) -> Result<(), NewsError> {
        Ok(())
    }

    #[test]
    fn model_values_are_names_and_never_flags_or_shell_text() {
        for good in [
            "sonnet",
            "haiku",
            "claude-opus-5-5",
            "opus[1m]",
            "0",
            "claude-3.5",
        ] {
            assert!(is_model_name(good), "{good}");
            validate("models.a1", &json!(good), &ok).unwrap_or_else(|e| panic!("{good}: {e}"));
        }
        let long = "a".repeat(65);
        for bad in [
            "",
            "--dangerously-skip-permissions",
            "-p",
            "Sonnet",
            "sonnet extra",
            "sonnet;calc",
            "a\"b",
            "\u{0}",
            "opus\n",
            ".sonnet",
            "[1m]",
            "s\u{f6}nnet",
            long.as_str(),
        ] {
            assert!(!is_model_name(bad), "{bad:?}");
            assert!(
                validate("models.main", &json!(bad), &ok).is_err(),
                "{bad:?}"
            );
        }
        assert!(is_model_name(&"a".repeat(64)));
        // Every models.* key is checked this way; null still means the default.
        for d in REGISTRY.iter().filter(|d| d.key.starts_with("models.")) {
            assert_eq!(d.ty, KeyType::Model, "{}", d.key);
            assert!(validate(d.key, &json!("x y"), &ok).is_err(), "{}", d.key);
            validate(d.key, &Value::Null, &ok).unwrap();
        }
    }

    #[test]
    fn registry_keys_are_unique_sorted_and_their_defaults_valid() {
        let keys: Vec<&str> = REGISTRY.iter().map(|d| d.key).collect();
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(keys, sorted);
        for d in REGISTRY {
            let def: Value =
                serde_json::from_str(d.default).unwrap_or_else(|e| panic!("{}: {e}", d.key));
            validate(d.key, &def, &ok).unwrap_or_else(|e| panic!("{}: {e}", d.key));
        }
        assert!(decl("pools.ai.max_running").is_some());
    }

    #[test]
    fn the_config_models_and_source_keys_are_registered_and_checked() {
        // The former environment variables of Technical Design "Settings", with their starting values.
        let s = Standing::defaults();
        assert_eq!(s.int("run.target_pages"), 100);
        assert_eq!(s.int("publish.index_per_day"), 20);
        assert_eq!(s.get("proof.sample_rate"), json!(0.05));
        assert_eq!(s.int("quota.ai_run_tokens"), 2_500_000);
        assert_eq!(s.int("retain.raw_days_data"), 400);
        assert_eq!(s.int("retain.raw_days_docs"), 90);
        assert_eq!(s.text("models.main").as_deref(), Some("sonnet"));
        assert_eq!(s.text("models.light").as_deref(), Some("haiku"));
        assert_eq!(s.text("models.curator").as_deref(), Some("sonnet"));
        assert_eq!(
            s.text("models.a1"),
            None,
            "a step follows its role until set"
        );
        assert_eq!(
            REGISTRY
                .iter()
                .filter(|d| d.key.starts_with("models.a"))
                .count(),
            12
        );
        // A decimal is kept as written and bounded, never rescaled.
        assert!(validate("proof.sample_rate", &json!(0.1), &ok).is_ok());
        assert!(validate("proof.sample_rate", &json!(1.5), &ok).is_err());
        assert!(validate("proof.sample_rate", &json!("5%"), &ok).is_err());
        // Sources on or off.
        assert!(is_source_key("sources.sec-edgar.enabled"));
        assert!(!is_source_key("sources..enabled"));
        assert!(!is_source_key("sources.Bad.enabled"));
        assert!(validate("sources.fred.enabled", &json!(false), &ok).is_ok());
        assert!(validate("sources.fred.enabled", &json!("no"), &ok).is_err());
        assert!(validate("sources.x.disabled", &json!(true), &ok).is_err());
        let mut set = Standing::defaults();
        set.settings
            .insert("sources.fred.enabled".into(), json!(false));
        assert_eq!(set.effective()["sources.fred.enabled"], json!(false));
        let items = set.items(Some("sources."));
        assert_eq!(items["items"][0]["key"], "sources.fred.enabled");
        assert_eq!(items["items"][0]["type"], "boolean");
    }

    #[test]
    fn values_are_checked_against_their_type() {
        let bad = [
            ("pools.ai.max_running", json!(0)),
            ("pools.ai.max_running", json!("3")),
            ("quiet_hours.start", json!("7:00")),
            ("quiet_hours.start", json!("24:00")),
            ("quiet_hours.mode", json!("off")),
            ("rota.time_zone", json!("Mars/Olympus")),
            ("quota.bureau_daily_tokens", json!(-1)),
            ("no.such.key", json!(1)),
            (
                "schedules.Bad_Id",
                json!({"preset": "edition", "args": {}, "cron": "0 11 * * *", "enabled": true}),
            ),
            (
                "schedules.am",
                json!({"preset": "edition", "args": {}, "cron": "0 11 * *", "enabled": true}),
            ),
            (
                "schedules.am",
                json!({"preset": "edition", "cron": "0 11 * * *", "enabled": true}),
            ),
        ];
        for (k, v) in bad {
            let e = validate(k, &v, &ok).unwrap_err();
            assert_eq!(e.code, news_core::ErrorCode::ValidationFailed, "{k} {v}");
        }
        for (k, v) in [
            ("quiet_hours.start", json!("23:30")),
            ("rota.time_zone", json!("America/New_York")),
            ("quota.bureau_daily_tokens", json!(2_000_000)),
            (
                "schedules.edition-am",
                json!({"preset": "edition", "args": {}, "cron": "0 11 * * *", "enabled": true}),
            ),
            ("pools.ai.max_running", Value::Null),
        ] {
            validate(k, &v, &ok).unwrap_or_else(|e| panic!("{k}: {e}"));
        }
        let refuse = |_: &Schedule| Err(NewsError::validation("unknown preset"));
        assert!(
            validate(
                "schedules.x",
                &json!({"preset": "nope", "args": {}, "cron": "0 1 * * *", "enabled": true}),
                &refuse
            )
            .is_err()
        );
    }

    #[test]
    fn effective_values_fall_back_to_defaults() {
        let mut s = Standing::defaults();
        assert_eq!(s.int("pools.ai.max_running"), 3);
        assert_eq!(s.opt_int("quota.bureau_daily_tokens"), None);
        s.settings.insert("pools.ai.max_running".into(), json!(5));
        s.settings.insert(
            "schedules.am".into(),
            json!({"preset": "edition", "args": {}, "cron": "0 11 * * *", "enabled": true}),
        );
        assert_eq!(s.int("pools.ai.max_running"), 5);
        assert_eq!(s.schedules().len(), 1);
        let items = s.items(Some("pools."));
        assert_eq!(items["items"][0]["is_set"], true);
        assert_eq!(items["items"][0]["default"], 3);
        assert!(s.effective().contains_key("schedules.am"));
    }

    #[test]
    fn times_of_day_parse() {
        assert_eq!(parse_hhmm("00:00"), Some(0));
        assert_eq!(parse_hhmm("23:59"), Some(1439));
        assert_eq!(parse_hhmm("7:00"), None);
    }
}
