//! Diary and Roster against PostgreSQL (#12): the calendar store, derived earnings windows from the
//! Ledger, seeds, the resolver through `pg_trgm`, the unresolved queue, `merge`, and the golden CLI
//! calls for `news calendar` and `news entities`.

use super::fixture::Fixture;
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use news_connectors::CalendarEntry;
use news_diary::{earnings, ics, rules, seed as cal_seed, store};
use news_roster::resolver::Step;
use news_roster::{seed, store as roster};
use sqlx::PgPool;

fn d(y: i32, m: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, day).unwrap()
}

fn t(y: i32, m: u32, day: u32, h: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(y, m, day, h, 0, 0).unwrap()
}

fn repo(path: &str) -> String {
    std::fs::read_to_string(format!("{}/../../../{path}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

async fn source(pool: &PgPool, id: &str, region: &str) {
    sqlx::query(
        "insert into sources (source_id, name, publisher, connector, base_url, source_tier, licence, attribution,
           redistribution, image_policy, rate_per_sec, regions, categories, calendar_kind)
         values ($1, $1, 'x', $1, 'https://x.test/', 1, 'public-domain', 'x', true, 'none', 1.0, $2, '{economy}', 'ics')
         on conflict do nothing",
    )
    .bind(id)
    .bind(vec![region.to_owned()])
    .execute(pool)
    .await
    .unwrap();
}

fn bls_entries() -> Vec<CalendarEntry> {
    let bytes = std::fs::read(format!(
        "{}/../news-diary/tests/fixtures/bls-schedule.ics",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    rules::entries_from_ics("bls", &ics::parse(&bytes).unwrap())
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn calendar_sync_is_idempotent_and_never_moves_a_captured_event() {
    let f = Fixture::migrated().await;
    source(&f.pool, "bls", "us").await;
    let entries = bls_entries();
    let first = store::upsert(&f.pool, &entries).await.unwrap();
    assert_eq!((first.events, first.created, first.updated), (6, 6, 0));
    let second = store::upsert(&f.pool, &entries).await.unwrap();
    assert_eq!(
        (second.created, second.updated),
        (0, 0),
        "running twice changes nothing"
    );

    // A date change on an expected event updates it.
    let mut moved = entries.clone();
    moved[0].expected_day = d(2026, 11, 9);
    let third = store::upsert(&f.pool, &moved).await.unwrap();
    assert_eq!((third.created, third.updated), (0, 1));

    // Captured events are left alone by a later sync.
    let key = &entries[2].event_key;
    assert!(
        store::mark_captured(&f.pool, key, t(2026, 11, 10, 14))
            .await
            .unwrap()
    );
    let mut shifted = entries.clone();
    shifted[2].expected_day = d(2026, 11, 30);
    let fourth = store::upsert(&f.pool, &shifted).await.unwrap();
    assert_eq!((fourth.created, fourth.updated), (0, 0));
    assert_eq!(
        f.scalar_str(&format!(
            "select status from calendar_events where event_key = '{key}'"
        ))
        .await,
        "captured"
    );
    assert!(
        !store::mark_captured(&f.pool, "macro:us.nope:2026-01", t(2026, 11, 10, 14))
            .await
            .unwrap()
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn upcoming_filters_by_window_and_region_and_late_follows_the_schedule() {
    let f = Fixture::migrated().await;
    source(&f.pool, "bls", "us").await;
    source(&f.pool, "cb-boe", "uk").await;
    store::upsert(&f.pool, &bls_entries()).await.unwrap();
    let boe = cal_seed::parse(&repo("data/calendar/seed.toml")).unwrap();
    store::upsert(&f.pool, &cal_seed::entries(&boe, d(2026, 11, 18)))
        .await
        .unwrap();

    let up = store::upcoming(&f.pool, d(2026, 11, 18), 30, None)
        .await
        .unwrap();
    let keys: Vec<&str> = up.iter().map(|e| e.event_key.as_str()).collect();
    assert_eq!(
        keys,
        [
            "macro:us.payrolls:2026-11",
            "macro:us.unemployment:2026-11",
            "macro:us.cpi:2026-11",
            "policy:boe:2026-12-17"
        ]
    );
    assert_eq!(up[0].expected_day, d(2026, 12, 4), "soonest first");
    let us = store::upcoming(&f.pool, d(2026, 11, 18), 30, Some("us"))
        .await
        .unwrap();
    assert!(us.iter().all(|e| e.source_id == "bls"));
    let uk = store::upcoming(&f.pool, d(2026, 11, 18), 30, Some("uk"))
        .await
        .unwrap();
    assert_eq!(uk.len(), 1);

    // On a virtual clock: 10 Dec 13:30 UTC CPI; the schedule is spent 26 h + 2 min later.
    assert_eq!(
        store::mark_late(&f.pool, t(2026, 12, 11, 15))
            .await
            .unwrap(),
        5,
        "the 10 Dec CPI is not late until 15:32"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn a_missed_release_ends_late_and_a_late_arrival_is_captured_after_all() {
    let f = Fixture::migrated().await;
    source(&f.pool, "bls", "us").await;
    store::upsert(&f.pool, &bls_entries()).await.unwrap();
    // 10 Dec 2026 13:30 UTC CPI: its schedule runs until 11 Dec 15:32 UTC.
    assert_eq!(
        store::mark_late(&f.pool, t(2026, 12, 11, 15))
            .await
            .unwrap(),
        5
    );
    assert_eq!(
        store::mark_late(&f.pool, t(2026, 12, 11, 15))
            .await
            .unwrap(),
        0,
        "idempotent"
    );
    let late = store::late(&f.pool, t(2026, 12, 11, 16)).await.unwrap();
    assert_eq!(late.len(), 6);
    assert!(late.iter().all(|e| e.status == "late"));
    assert!(
        store::mark_captured(&f.pool, "macro:us.cpi:2026-11", t(2026, 12, 12, 9))
            .await
            .unwrap()
    );
    assert_eq!(
        store::late(&f.pool, t(2026, 12, 12, 10))
            .await
            .unwrap()
            .len(),
        5
    );
}

async fn seed_sec_facts(pool: &PgPool) {
    source(pool, "sec-edgar", "us").await;
    sqlx::query(
        "insert into snapshots (sha256, source_id, url, fetched_at, http_status, bytes, storage_path)
         values ($1, 'sec-edgar', 'https://data.sec.gov/x', now(), 200, 1, 'x')",
    )
    .bind(vec![1u8; 32])
    .execute(pool)
    .await
    .unwrap();
    let ent: i64 = sqlx::query_scalar(
        "insert into entities (kind, name) values ('company', 'Apple Inc.') returning entity_id",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query(
        "insert into entity_ids (scheme, value, entity_id) values ('cik', '0000320193', $1)",
    )
    .bind(ent)
    .execute(pool)
    .await
    .unwrap();
    for (i, (fy, fp, filed)) in [
        (2026i16, "Q4", t(2026, 10, 30, 0)),
        (2026, "Q1", t(2026, 1, 30, 0)),
        (2026, "Q1", t(2026, 3, 1, 0)),
    ]
    .into_iter()
    .enumerate()
    {
        sqlx::query(
            "insert into data_points (dp_id, context_key, series_key, entity_id, concept, label, value_text, value, unit,
               period_label, period_type, fiscal_year, fiscal_period, vintage, snapshot_sha, locator, parser, retrieved_at)
             values ($1, '', 'sec:x', $2, 'NetIncomeLoss', 'Net income', '1', 1, 'USD', 'x', 'duration', $3, $4, $5, $6, '{}', 'sec@1', now())",
        )
        .bind(format!("dp_test{i}"))
        .bind(ent)
        .bind(fy)
        .bind(fp)
        .bind(filed)
        .bind(vec![1u8; 32])
        .execute(pool)
        .await
        .unwrap();
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn earnings_windows_come_from_the_ledgers_filing_history() {
    let f = Fixture::migrated().await;
    seed_sec_facts(&f.pool).await;
    assert_eq!(store::tracked_ciks(&f.pool).await.unwrap(), ["0000320193"]);
    let h = store::earnings_history(&f.pool, "320193").await.unwrap();
    assert_eq!(h.len(), 2);
    // Q1 was filed twice (an amendment): the first filing dates the period.
    assert_eq!(
        h.iter().find(|x| x.fiscal_period == "Q1").unwrap().filed,
        d(2026, 1, 30)
    );
    let e = earnings::derive(&h, d(2026, 11, 18));
    assert_eq!(
        e.iter().map(|e| e.event_key.as_str()).collect::<Vec<_>>(),
        ["earnings:0000320193:2027Q1", "earnings:0000320193:2027Q4"]
    );
    let r = store::upsert(&f.pool, &e).await.unwrap();
    assert_eq!(r.created, 2);
    let w = f
        .scalar_i64(
            "select window_days::bigint from calendar_events where event_key like '%2027Q1'",
        )
        .await;
    assert_eq!(w, 7);
}

// ---------------------------------------------------------------------------------------- Roster

fn universe() -> Vec<seed::Seed> {
    let mut all = seed::parse_toml(
        &std::fs::read_to_string(format!(
            "{}/../news-roster/tests/g7/universe.toml",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap(),
    )
    .unwrap();
    all.extend(seed::parse_toml(&repo("data/roster/central-banks.toml")).unwrap());
    all
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn seeds_apply_once_and_resolve_through_pg_trgm() {
    let f = Fixture::migrated().await;
    let seeds = universe();
    let first = roster::apply(&f.pool, &seeds, "seed").await.unwrap();
    assert!(first.created > 50 && first.aliases_added > 100, "{first:?}");
    let again = roster::apply(&f.pool, &seeds, "seed").await.unwrap();
    assert_eq!(
        (again.created, again.ids_added, again.aliases_added),
        (0, 0, 0),
        "re-running changes nothing"
    );
    assert_eq!(again.matched, seeds.len());
    let prom = seed::parse_prominence(&repo("data/roster/prominence.toml")).unwrap();
    assert!(roster::apply_prominence(&f.pool, &prom).await.unwrap() >= 10);
    assert_eq!(
        f.scalar_i64("select prominence::bigint from entities where name = 'Apple Inc.'")
            .await,
        1
    );

    let by_id = roster::resolve(&f.pool, "cik:320193", None).await.unwrap();
    assert_eq!(
        (by_id.name.as_deref(), by_id.step),
        (Some("Apple Inc."), Step::Identifier)
    );
    let alias = roster::resolve(&f.pool, "APPLE INC", Some("company"))
        .await
        .unwrap();
    assert_eq!(alias.step, Step::Alias);
    let gaz = roster::resolve(
        &f.pool,
        "Bank of England raises rates again",
        Some("central_bank"),
    )
    .await
    .unwrap();
    assert_eq!(
        (gaz.name.as_deref(), gaz.step),
        (Some("Bank of England"), Step::Gazetteer)
    );
    let fuzzy = roster::resolve(&f.pool, "Bank of Englnd", None)
        .await
        .unwrap();
    assert_eq!(
        (fuzzy.name.as_deref(), fuzzy.step),
        (Some("Bank of England"), Step::Fuzzy)
    );
    let amb = roster::resolve(&f.pool, "Delta", None).await.unwrap();
    assert_eq!(amb.step, Step::Unresolved);
    assert!(
        roster::resolve(&f.pool, "Bank of Chile", Some("central_bank"))
            .await
            .unwrap()
            .entity_id
            .is_none()
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn the_unresolved_queue_counts_repeats_and_merge_folds_one_entity_into_another() {
    let f = Fixture::migrated().await;
    roster::apply(&f.pool, &universe(), "seed").await.unwrap();
    let r = roster::resolve(&f.pool, "Delta", Some("company"))
        .await
        .unwrap();
    for _ in 0..3 {
        roster::record_unresolved(&f.pool, "Delta", Some("company"), "fred headline 12", &r)
            .await
            .unwrap();
    }
    let miss = roster::resolve(&f.pool, "Acme Corporation", Some("company"))
        .await
        .unwrap();
    roster::record_unresolved(
        &f.pool,
        "Acme Corporation",
        Some("company"),
        "sec 8-K",
        &miss,
    )
    .await
    .unwrap();
    let q = roster::unresolved(&f.pool, 10).await.unwrap();
    assert_eq!((q[0].mention.as_str(), q[0].count), ("Delta", 3));
    assert_eq!(q.len(), 2);
    // A resolved mention is not queued.
    let ok = roster::resolve(&f.pool, "Apple", Some("company"))
        .await
        .unwrap();
    roster::record_unresolved(&f.pool, "Apple", Some("company"), "x", &ok)
        .await
        .unwrap();
    assert_eq!(roster::unresolved(&f.pool, 10).await.unwrap().len(), 2);

    // H-tier alias add, then merge the two Delta companies into one (same kind).
    let a = roster::show(&f.pool, "cik:27904").await.unwrap().unwrap();
    let b = roster::show(&f.pool, "cik:1101396").await.unwrap().unwrap();
    assert!(
        roster::add_alias(&f.pool, a.entity_id, "Delta Air", "board")
            .await
            .unwrap()
    );
    assert!(
        !roster::add_alias(&f.pool, a.entity_id, "delta air", "board")
            .await
            .unwrap(),
        "already there"
    );
    let report = roster::merge(&f.pool, b.entity_id, a.entity_id)
        .await
        .unwrap();
    assert!(report.ids_moved >= 1 && report.aliases_moved >= 1);
    assert!(
        roster::show(&f.pool, &b.entity_id.to_string())
            .await
            .unwrap()
            .is_none()
    );
    let merged = roster::show(&f.pool, &a.entity_id.to_string())
        .await
        .unwrap()
        .unwrap();
    assert!(merged.ids.iter().any(|i| i.value == "0001101396"));
    // Different kinds refuse to merge; so does a merge into itself.
    let us = roster::show(&f.pool, "iso3166:US").await.unwrap().unwrap();
    assert!(
        roster::merge(&f.pool, us.entity_id, a.entity_id)
            .await
            .is_err()
    );
    assert!(
        roster::merge(&f.pool, a.entity_id, a.entity_id)
            .await
            .is_err()
    );
}

/// `merge` must know every table that points at `entities`.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn merge_knows_every_reference_to_entities() {
    let f = Fixture::migrated().await;
    let found: Vec<(String, String)> = sqlx::query_as(
        "select c.conrelid::regclass::text, a.attname
           from pg_constraint c
           join pg_attribute a on a.attrelid = c.conrelid and a.attnum = any (c.conkey)
          where c.contype = 'f' and c.confrelid = 'entities'::regclass
          order by 1, 2",
    )
    .fetch_all(&f.pool)
    .await
    .unwrap();
    let mut found: Vec<(String, String)> = found
        .into_iter()
        .map(|(t, c)| (t.rsplit('.').next().unwrap().to_owned(), c))
        .collect();
    found.sort();
    let mut known: Vec<(String, String)> = roster::REFERENCES
        .iter()
        .map(|(t, c)| ((*t).to_owned(), (*c).to_owned()))
        .collect();
    known.sort();
    assert_eq!(
        found, known,
        "a migration added a reference to entities: teach merge about it"
    );
}

// ------------------------------------------------------------------------------- golden CLI calls

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn golden_news_calendar() {
    let f = Fixture::migrated().await;
    source(&f.pool, "bls", "us").await;
    source(&f.pool, "cb-boe", "uk").await;
    let (env, code, _, _) = f.call(&["calendar", "upcoming", "--days", "3"]);
    assert_eq!(code, 0, "{env}");
    assert_eq!(env["data"]["items"], serde_json::json!([]));

    // Seed file (NEWS_CALENDAR_DIR) and nothing else: sync is idempotent.
    let dir = format!("{}/../../../data/calendar", env!("CARGO_MANIFEST_DIR"));
    let mut cmd = f.news();
    cmd.env("NEWS_CALENDAR_DIR", &dir);
    let (env, code, _, _) = super::fixture::call(&mut cmd, &["calendar", "sync"]);
    assert_eq!(code, 0, "{env}");
    let created = env["data"]["created"].as_u64().unwrap();
    assert!(
        created >= 1 && env["data"]["events"].as_u64().unwrap() >= created,
        "{env}"
    );
    assert!(
        env["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w.as_str().unwrap().contains("calendar.sync job"))
    );
    let mut cmd = f.news();
    cmd.env("NEWS_CALENDAR_DIR", &dir);
    let (env, _, _, _) = super::fixture::call(&mut cmd, &["calendar", "sync"]);
    assert_eq!(
        (
            env["data"]["created"].as_u64(),
            env["data"]["updated"].as_u64()
        ),
        (Some(0), Some(0)),
        "re-running changes nothing"
    );

    // Upcoming sees them (whatever "today" is, the seed runs to the end of 2027: widen the window).
    let (env, code, _, _) = f.call(&["calendar", "upcoming", "--days", "800", "--region", "uk"]);
    assert_eq!(code, 0, "{env}");
    let items = env["data"]["items"].as_array().unwrap();
    assert!(
        items
            .iter()
            .all(|i| i["source_id"] == "cb-boe" && i["status"] == "expected")
    );
    let (env, code, _, _) = f.call(&["calendar", "late"]);
    assert_eq!(code, 0, "{env}");
    assert!(env["data"]["items"].is_array());
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn golden_news_entities() {
    let f = Fixture::migrated().await;
    roster::apply(&f.pool, &universe(), "seed").await.unwrap();
    let (env, code, _, _) = f.call(&["entities", "resolve", "Apple Inc.", "--kind", "company"]);
    assert_eq!(code, 0, "{env}");
    assert_eq!(env["data"]["name"], "Apple Inc.");
    assert_eq!(env["data"]["step"], "alias");
    assert_eq!(env["data"]["score"], 1.0);
    let (env, _, _, _) = f.call(&["entities", "resolve", "cik:320193"]);
    assert_eq!(env["data"]["step"], "identifier");
    let (env, code, _, _) = f.call(&["entities", "resolve", "Delta"]);
    assert_eq!(
        code, 0,
        "an unresolved mention is a result, not an error: {env}"
    );
    assert_eq!(env["data"]["step"], "unresolved");
    assert!(env["data"]["entity_id"].is_null());
    let (env, code, _, _) = f.call(&["entities", "resolve", "x", "--kind", "planet"]);
    assert_eq!((code, env["error"]["code"].as_str()), (2, Some("usage")));

    let (env, code, _, _) = f.call(&["entities", "show", "cik:320193"]);
    assert_eq!(code, 0, "{env}");
    assert_eq!(env["data"]["name"], "Apple Inc.");
    assert_eq!(
        env["data"]["ids"],
        serde_json::json!([{"scheme": "cik", "value": "0000320193"}])
    );
    assert_eq!(env["data"]["aliases"], serde_json::json!(["apple"]));
    let (env, code, _, _) = f.call(&["entities", "show", "cik:999"]);
    assert_eq!(
        (code, env["error"]["code"].as_str()),
        (3, Some("not_found"))
    );

    let (env, code, _, _) = f.call(&["entities", "unresolved"]);
    assert_eq!(code, 0, "{env}");
    assert_eq!(env["data"]["items"], serde_json::json!([]));
    let r = roster::resolve(&f.pool, "Delta", None).await.unwrap();
    roster::record_unresolved(&f.pool, "Delta", None, "headline", &r)
        .await
        .unwrap();
    let (env, _, _, _) = f.call(&["entities", "unresolved"]);
    assert_eq!(
        env["data"]["items"][0],
        serde_json::json!({"mention": "Delta", "count": 1, "sample": "headline"})
    );
}
