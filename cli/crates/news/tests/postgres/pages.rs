//! The page-producing presets end to end (#20, part 2) on a fixture database, with the model, Plate,
//! Proof and the write API replaced by fakes: Edition (twelve steps to a Wrap), Flash, Catchup (a chunk
//! and the `plan` gate) and Reprint (the `sample` gate and a second version), plus Press's `register`
//! step (idempotent), `news prune` (keeps the inputs of a published page) and Followup's bulk guard.
//!
//! The fake `claude` is a job handler keyed by run: it stores a Copy for the story the job names, as
//! Ghost's accept sink would. The fake tools assemble a real bundle (`news_proof::render::assemble`)
//! from a canned render, so register, drain and prune handle real files.

use super::fixture::Fixture;
use super::presets::{answer, chief, open_gate, wait_status};
use super::taster::{doc, source};
use chrono::{DateTime, Utc};
use news::steps::tools::{Check, PagePublisher, PageTools, Rendered};
use news::steps::{Ctx, HomePublisher};
use news_chief::gates::Decision;
use news_chief::{BoxFut, IfBusy, StartReq};
use news_core::NewsResult;
use news_core::job::{Handler, JobFuture, JobInput};
use news_dispatch::worker::{self, ActiveJobs, Resolver, WorkOptions};
use news_press::api::Client;
use news_press::fake::FakeApi;
use news_press::store::Drained;
use news_proof::work::{Work, save};
use news_wire::pace::VirtualClock;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use uuid::Uuid;

const FIXTURE: &str = include_str!("../../../news-nose/tests/fixtures/worked-example.json");
const KEY: &str = "macro:us.t:2026-06";

fn now() -> DateTime<Utc> {
    super::home::now()
}

/// A work folder under the target directory (on the Dev Drive), one per test.
pub(super) fn root_for(f: &Fixture) -> PathBuf {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("pages-{}", f.schema));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("work/ai")).unwrap();
    // The writer's prefix is built from the site's component index.
    let index = root.join(news_fixer::components::INDEX_REL);
    std::fs::create_dir_all(index.parent().unwrap()).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/component-index.json"),
        index,
    )
    .unwrap();
    root
}

/// What a published story leaves in the work folder for an update: its brief and its stored Copy.
pub(super) fn published_story(root: &Path, story: &str) {
    let work = Work::new(root.join("work"));
    std::fs::create_dir_all(root.join("work/briefs")).unwrap();
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../news-proof/tests/fixtures/brief.json"),
        work.brief(story),
    )
    .unwrap();
    save(&work, story, copy("Written"), vec![], "ai", None, now()).unwrap();
}

// ---- the fakes ------------------------------------------------------------------------------------

/// Where each run's repository root is, for the fake `claude` (a plain function has no state).
static ROOTS: Mutex<Option<HashMap<Uuid, PathBuf>>> = Mutex::new(None);

fn root_of(run: Option<Uuid>) -> PathBuf {
    ROOTS
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|m| m.get(&run?).cloned())
        .expect("the test registered the run's root")
}

fn copy(headline: &str) -> Value {
    json!({"headline": headline, "fly_title": "Fixture", "summary": "A fixture page.",
           "oui": "root = Page([a])", "refs_used": [], "components_used": ["Page"], "angle": "fixture"})
}

fn fake_claude(input: JobInput) -> JobFuture {
    Box::pin(async move {
        let root = root_of(input.run_id);
        let story = input.args["story"].as_str().unwrap_or_default().to_owned();
        let work = Work::new(root.join("work"));
        match input.kind.as_str() {
            // A direction flip's A1 update: Ghost's arguments must be there (an input under work/, the
            // page schema and A1's prefix), with the input composed from the previous Copy and the diff.
            "ai.write" if input.args["mode"] == "update" => {
                let file = |k: &str| {
                    input.args[k]
                        .as_str()
                        .filter(|p| p.starts_with("work/"))
                        .map(|p| root.join(p))
                        .filter(|p| p.is_file())
                };
                let composed = file("input").and_then(|p| news_proof::work::read_json(&p).ok());
                let ok = input.args["schema"] == news::steps::pipeline::PAGE_SCHEMA
                    && file("prefix").is_some()
                    && composed.as_ref().is_some_and(|c| {
                        c["mode"] == "update"
                            && c["previous_copy"]["headline"] == "Written"
                            && c["diff"].as_array().is_some_and(|d| !d.is_empty())
                    });
                if !ok {
                    return Err(news_core::job::JobFailure::new(
                        news_core::job::ErrorClass::Bug,
                        format!("bad update job: args {} input {composed:?}", input.args),
                    ));
                }
                save(
                    &work,
                    &story,
                    copy("Updated"),
                    vec![],
                    "ai",
                    Some(input.job_id),
                    now(),
                )
                .unwrap();
            }
            "ai.write" => {
                save(
                    &work,
                    &story,
                    copy("Written"),
                    vec![],
                    "ai",
                    Some(input.job_id),
                    now(),
                )
                .unwrap();
            }
            "ai.repair" => {
                save(
                    &work,
                    &story,
                    copy("Repaired"),
                    vec![],
                    "ai",
                    Some(input.job_id),
                    now(),
                )
                .unwrap();
            }
            "ai.spotcheck" => {
                let out = root.join(format!("work/ai/spot-out-{}.json", input.job_id));
                std::fs::write(&out, json!({"verdict": "pass", "issues": []}).to_string()).unwrap();
                return Ok(json!({"step": "A7", "valid": true, "output": out.to_string_lossy()}));
            }
            other => panic!("the fake claude does not run {other}"),
        }
        Ok(json!({"valid": true, "stored": true}))
    })
}

fn code_job(_: JobInput) -> JobFuture {
    Box::pin(async { Ok(json!({})) })
}

fn resolve() -> Resolver {
    |kind| match kind {
        "ai.write" | "ai.repair" | "ai.spotcheck" => Some(fake_claude as Handler),
        "source.discover" | "calendar.capture" | "story.detect" => Some(code_job as Handler),
        _ => None,
    }
}

/// Plate and Proof: a real bundle from a canned render; a check that fails once for the stories named.
pub(super) struct FakeTools {
    version: Mutex<String>,
    fail_once: Mutex<HashSet<String>>,
    /// Stories whose render fails for a reason of their own (not retryable).
    render_error: Mutex<HashSet<String>>,
}

impl FakeTools {
    fn new(version: &str) -> Arc<Self> {
        Arc::new(Self {
            version: Mutex::new(version.into()),
            fail_once: Mutex::default(),
            render_error: Mutex::default(),
        })
    }
}

impl PageTools for FakeTools {
    fn render<'a>(&'a self, work: &'a Work, story: &'a str) -> BoxFut<'a, NewsResult<Rendered>> {
        Box::pin(async move {
            if self.render_error.lock().unwrap().contains(story) {
                return Err(news_core::NewsError::precondition("the page has no inputs"));
            }
            let inputs = news_proof::render::load_inputs(work, story)?;
            let version = self.version.lock().unwrap().clone();
            let points: Vec<String> = inputs.data["points"]
                .as_object()
                .map(|p| p.keys().cloned().collect())
                .unwrap_or_default();
            let rendered = news_proof::render::Rendered {
                renderer_version: version.clone(),
                files: BTreeMap::from([
                    (
                        "index.html".into(),
                        format!("<html>{}</html>", inputs.copy.headline),
                    ),
                    ("index.md".into(), format!("# {}", inputs.copy.headline)),
                ]),
                charts: vec![],
                refs_used: points,
                problems: vec![],
            };
            let built = news_proof::render::assemble(&inputs, &rendered)?;
            news_proof::render::write(&built, &work.page(story))?;
            Ok(Rendered {
                bundle_sha: built.sha256,
                renderer_version: version,
            })
        })
    }

    fn check<'a>(
        &'a self,
        _: &'a news_store::PgPool,
        work: &'a Work,
        story: &'a str,
    ) -> BoxFut<'a, NewsResult<Check>> {
        Box::pin(async move {
            Ok(if self.fail_once.lock().unwrap().remove(story) {
                Check::Fail(vec![
                    json!({"check": "C4", "where": "headline", "message": "a number in prose"}),
                ])
            } else {
                // What Proof's check reads: the story folder's files, hashed by the manifest's rule.
                Check::Pass(news_proof::bundle::Bundle::load(&work.page(story)).bundle_sha256())
            })
        })
    }
}

/// The write API: every row sent to a scripted fake, paced on a virtual clock.
struct FakePress(Arc<FakeApi>);

impl PagePublisher for FakePress {
    fn drain<'a>(
        &'a self,
        db: &'a news_store::PgPool,
        work: &'a Path,
        run: Uuid,
    ) -> BoxFut<'a, NewsResult<Drained>> {
        Box::pin(async move {
            let client = Client::new(self.0.clone(), Arc::new(VirtualClock::new(now())));
            news_press::store::drain(db, &client, work, Some(run)).await
        })
    }
}

struct NoHome;
impl HomePublisher for NoHome {
    fn publish<'a>(
        &'a self,
        _: &'a news_store::PgPool,
        _: i64,
        _: DateTime<Utc>,
    ) -> BoxFut<'a, NewsResult<Value>> {
        Box::pin(async { Err(news_core::NewsError::internal("no home here")) })
    }
}

pub(super) fn tools() -> Arc<FakeTools> {
    FakeTools::new("plate-1.0.0")
}

pub(super) fn api() -> Arc<FakeApi> {
    Arc::new(FakeApi::new())
}

pub(super) fn ctx(root: &Path, tools: Arc<FakeTools>, api: Arc<FakeApi>) -> Ctx {
    Ctx {
        root: root.to_path_buf(),
        preview_base: "http://127.0.0.1:9".into(),
        home: Arc::new(NoHome),
        tools,
        press: Arc::new(FakePress(api)),
    }
}

// ---- the world --------------------------------------------------------------------------------------

/// One source, one story with thirty months of a series, a document, a candidate and a scoring input
/// (the Bureau §4.6 CPI story, published six hours before the run starts).
async fn seed(f: &Fixture) -> i64 {
    let p = &f.pool;
    source(p, "fred", 1).await;
    let mut last = String::new();
    for m in 0..30u32 {
        let (y, mo) = (2024 + i32::try_from(m / 12).unwrap(), m % 12 + 1);
        let start = chrono::NaiveDate::from_ymd_opt(y, mo, 1).unwrap();
        let end = start
            .checked_add_months(chrono::Months::new(1))
            .unwrap()
            .pred_opt()
            .unwrap();
        let value = if m == 29 {
            "2.0".to_owned()
        } else {
            format!("{}.{}", 2 + m % 3, m % 10)
        };
        last = format!("dp_t{m:02}");
        sqlx::query(
            "insert into data_points (dp_id, context_key, series_key, concept, label, value_text, value, unit,
               period_label, period_start, period_end, period_type, vintage, snapshot_sha, locator, parser, retrieved_at)
             values ($1, '', 'fred:T', 'c', 'Test rate', $2, $2::numeric, 'Percent', $3, $4, $5, 'duration',
               $6, sha256('fred'::bytea), '{}'::jsonb, 'test@1', now())",
        )
        .bind(&last)
        .bind(&value)
        .bind(start.format("%B %Y").to_string())
        .bind(start)
        .bind(end)
        .bind(now() - chrono::TimeDelta::days(3))
        .execute(p)
        .await
        .unwrap();
    }
    let entity: i64 = sqlx::query_scalar(
        "insert into entities (kind, name) values ('country', 'United States') returning entity_id",
    )
    .fetch_one(p)
    .await
    .unwrap();
    let mut input = serde_json::from_str::<Value>(FIXTURE).unwrap()["stories"][0]["input"].clone();
    input["published_at"] = json!(now() - chrono::TimeDelta::hours(6));
    let story: i64 = sqlx::query_scalar(
        "insert into stories (event_key, title, first_seen, last_seen, entity_ids, score_input)
         values ($1, 'Source headline', $2, $2, $3, $4) returning story_id",
    )
    .bind(KEY)
    .bind(now() - chrono::TimeDelta::hours(5))
    .bind(vec![entity])
    .bind(input)
    .fetch_one(p)
    .await
    .unwrap();
    let d = doc(p, "fred", "The rate fell. Officials said “it fell”.", "t").await;
    sqlx::query(
        "insert into text_blocks (doc_id, block_no, kind, text) values
           ($1, 0, 'heading', 'Test release'), ($1, 1, 'paragraph', 'The rate fell. Officials said “it fell”.')",
    )
    .bind(d)
    .execute(p)
    .await
    .unwrap();
    sqlx::query(
        "insert into candidates (source_id, url, title, event_key, story_id, doc_ids, dp_count, dp_ids,
           page_type, page_type_by, categories, region, published_at, first_seen, status)
         values ('fred', 'https://x.test/t', 't', $5, $1, $2, 1, $3, 'macro', 'rule',
           '{economy,inflation}', 'us', $4, $4, 'grouped')",
    )
    .bind(story)
    .bind(vec![d])
    .bind(vec![last])
    .bind(now() - chrono::TimeDelta::hours(5))
    .bind(KEY)
    .execute(p)
    .await
    .unwrap();
    story
}

struct Started {
    run_id: Uuid,
    short_id: String,
}

struct World {
    f: Fixture,
    root: PathBuf,
    story: i64,
    tools: Arc<FakeTools>,
    api: Arc<FakeApi>,
    ch: news_chief::Chief,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    work: tokio::task::JoinHandle<Result<worker::WorkReport, news_core::NewsError>>,
}

async fn world() -> World {
    world_with(|h| h).await
}

async fn world_with(
    wrap: impl FnOnce(news_chief::driver::Handlers) -> news_chief::driver::Handlers,
) -> World {
    let f = Fixture::migrated().await;
    let story = seed(&f).await;
    let root = root_for(&f);
    let tools = FakeTools::new("plate-1.0.0");
    let api = Arc::new(FakeApi::new());
    let handlers = wrap(news::steps::register(
        news_chief::driver::Handlers::default(),
        ctx(&root, tools.clone(), api.clone()),
    ));
    let ch = chief(&f, handlers);
    let (stop, rx) = tokio::sync::oneshot::channel::<()>();
    let mut opts = WorkOptions::new(vec!["ai".into(), "cpu".into(), "net".into()], "pages-w");
    opts.poll = Duration::from_millis(100);
    opts.maintenance = false;
    opts.active = ActiveJobs::default();
    let work = tokio::spawn(worker::run(f.pool.clone(), opts, resolve(), async move {
        let _ = rx.await;
    }));
    World {
        f,
        root,
        story,
        tools,
        api,
        ch,
        stop: Some(stop),
        work,
    }
}

impl World {
    async fn start(&self, preset: &str, args: Value) -> Started {
        let args: Map<String, Value> = args.as_object().cloned().unwrap_or_default();
        let run = self
            .ch
            .start(StartReq::board(preset, args, IfBusy::Offer))
            .await
            .unwrap()
            .run()
            .clone();
        let started = Started {
            run_id: run.run_id,
            short_id: run.short_id.clone(),
        };
        ROOTS
            .lock()
            .unwrap()
            .get_or_insert_default()
            .insert(run.run_id, self.root.clone());
        self.ch.ensure_drivers().await.unwrap();
        started
    }

    async fn finish(mut self) {
        let _ = self.stop.take().unwrap().send(());
        let _ = self.work.await;
        let _ = std::fs::remove_dir_all(&self.root);
    }

    async fn n(&self, sql: &str) -> i64 {
        self.f.scalar_i64(sql).await
    }
}

fn page_dir(w: &World) -> PathBuf {
    w.root.join("work/pages").join(w.story.to_string())
}

// ---- tests ------------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn edition_runs_twelve_steps_to_a_wrap_and_prunes_but_keeps_the_inputs() {
    let w = world().await;
    // Proof blocks the first check; the one A2 repair fixes it.
    w.tools
        .fail_once
        .lock()
        .unwrap()
        .insert(w.story.to_string());
    let run = w.start("edition", json!({})).await;
    wait_status(&w.f, run.run_id, "succeeded").await;

    let page = "macro-us-t-2026-06";
    assert_eq!(
        w.f.scalar_str(&format!("select status || ':' || version::text || ':' || index_status from pages where page_id = '{page}'")).await,
        "published:1:index"
    );
    assert_eq!(
        w.n("select count(*) from jobs where kind = 'ai.repair'")
            .await,
        1,
        "one repair, no more"
    );
    // A1 and A2 carry the page type's prefix, written under work/prefix/ by its hash.
    let prefixes: Vec<Option<String>> = sqlx::query_scalar(
        "select args->>'prefix' from jobs where kind in ('ai.write', 'ai.repair') order by job_id",
    )
    .fetch_all(&w.f.pool)
    .await
    .unwrap();
    assert_eq!(prefixes.len(), 2);
    for p in &prefixes {
        let p = p.as_deref().expect("every page call names its prefix");
        assert!(
            p.starts_with("work/prefix/macro-release.") && p.ends_with(".md"),
            "{p}"
        );
        assert!(w.root.join(p).is_file(), "{p}");
    }
    assert_eq!(prefixes[0], prefixes[1], "one prefix per page type");
    assert_eq!(
        w.f.scalar_str("select string_agg(op, ',' order by outbox_id) from publish_outbox where sent_at is not null").await,
        "put_page,index_promote"
    );
    assert!(
        w.n("select count(*) from page_refs").await > 0,
        "the refs of the page are filled"
    );
    assert_eq!(w.api.applied().len(), 2);
    // The Budget the run published under names the page.
    assert_eq!(
        w.f.scalar_str(&format!(
            "select budget->'items'->>0 from runs where run_id = '{}'",
            run.run_id
        ))
        .await,
        page
    );
    // Pruned: the built bundle and the sent version's folder are gone; the inputs stay.
    let dir = page_dir(&w);
    for gone in [
        "index.html",
        "index.md",
        "meta.json",
        "manifest.json",
        "page.oui",
    ] {
        assert!(!dir.join(gone).exists(), "{gone} should be pruned");
    }
    assert!(!w.root.join("work/pages").join(page).exists());
    for kept in ["data.json", "sources.json", "page.json", "copy.json"] {
        assert!(dir.join(kept).is_file(), "{kept} must stay");
    }
    assert!(
        w.root
            .join(format!("work/briefs/{}.json", w.story))
            .is_file()
    );
    // The Wrap counts what the steps did.
    let wrap: Value = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "select summary from runs where run_id = '{}'",
        run.run_id
    )))
    .fetch_one(&w.f.pool)
    .await
    .unwrap();
    assert_eq!(wrap["counts"]["published"], 1, "{wrap}");
    w.finish().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn flash_publishes_the_named_story_without_discovery() {
    let w = world().await;
    let run = w.start("flash", json!({"keys": [KEY]})).await;
    wait_status(&w.f, run.run_id, "succeeded").await;
    assert_eq!(
        w.f.scalar_str("select status || ':' || index_status from pages")
            .await,
        "published:noindex",
        "Flash promotes nothing"
    );
    assert_eq!(
        w.n("select count(*) from jobs where kind in ('source.discover', 'story.detect')")
            .await,
        0
    );
    // A story that does not exist stops the run at capture, with the key named.
    let bad = w.start("flash", json!({"keys": ["no-such-key"]})).await;
    wait_status(&w.f, bad.run_id, "failed").await;
    w.finish().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn catchup_stops_at_the_plan_gate_then_publishes_its_chunks() {
    let w = world().await;
    let run = w.start("catchup", json!({"chunk": 1})).await;
    wait_status(&w.f, run.run_id, "waiting_approval").await;
    let g = open_gate(&w.f, run.run_id).await;
    assert_eq!(g.0, "plan");
    assert_eq!(
        w.n("select count(*) from pages").await,
        0,
        "nothing before the approval"
    );
    assert_eq!(
        w.f.scalar_str(
            "select string_agg(priority::text, ',') from jobs where kind = 'source.discover'"
        )
        .await,
        "4",
        "backfill jobs use priority 4"
    );
    answer(&w.ch, &run.short_id, &g, Decision::Approved, None).await;
    wait_status(&w.f, run.run_id, "succeeded").await;
    assert_eq!(
        w.f.scalar_str("select status from pages").await,
        "published"
    );
    w.finish().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn reprint_rebuilds_a_pruned_page_on_a_newer_renderer_and_publishes_version_two() {
    let w = world().await;
    let first = w.start("edition", json!({})).await;
    wait_status(&w.f, first.run_id, "succeeded").await;
    let v1: String =
        w.f.scalar_str("select encode(bundle_sha, 'hex') from pages")
            .await;

    // A newer renderer, and a page whose built files `news prune` has removed.
    *w.tools.version.lock().unwrap() = "plate-2.0.0".into();
    let run = w.start("reprint", json!({"renderer": "plate-2.0.0"})).await;
    wait_status(&w.f, run.run_id, "waiting_approval").await;
    let g = open_gate(&w.f, run.run_id).await;
    assert_eq!(g.0, "sample");
    assert_eq!(
        w.f.scalar_str("select status || ':' || version::text from pages")
            .await,
        "published:1",
        "the live page stays published while version 2 waits"
    );
    assert_eq!(w.n("select version::bigint from page_drafts").await, 2);
    answer(&w.ch, &run.short_id, &g, Decision::Approved, None).await;
    wait_status(&w.f, run.run_id, "succeeded").await;
    assert_eq!(
        w.f.scalar_str(
            "select status || ':' || version::text || ':' || renderer_version from pages"
        )
        .await,
        "published:2:plate-2.0.0"
    );
    assert_ne!(
        w.f.scalar_str("select encode(bundle_sha, 'hex') from pages")
            .await,
        v1
    );
    assert_eq!(
        w.n("select count(*) from publish_outbox where op = 'put_page' and sent_at is not null")
            .await,
        2
    );

    assert_eq!(w.n("select count(*) from page_drafts").await, 0);

    // A rejected sample leaves the live page as it was and throws the draft away.
    *w.tools.version.lock().unwrap() = "plate-3.0.0".into();
    let again = w.start("reprint", json!({"renderer": "plate-3.0.0"})).await;
    wait_status(&w.f, again.run_id, "waiting_approval").await;
    let g = open_gate(&w.f, again.run_id).await;
    answer(&w.ch, &again.short_id, &g, Decision::Rejected, None).await;
    wait_status(&w.f, again.run_id, "succeeded").await;
    assert_eq!(
        w.f.scalar_str("select version::text from pages").await,
        "2",
        "a rejected sample changes nothing"
    );
    assert_eq!(
        w.f.scalar_str("select status from pages").await,
        "published"
    );
    assert_eq!(
        w.n("select count(*) from page_drafts").await,
        0,
        "the rejected draft is discarded"
    );
    w.finish().await;
}

// ---- Press's register step and prune ------------------------------------------------------------------

/// A bundle as `news render` leaves it in the story folder, with its manifest; returns the bundle SHA.
fn story_bundle(work: &Path, story: &str, page_id: &str, version: i32, html: &str) -> [u8; 32] {
    let dir = work.join("pages").join(story);
    std::fs::create_dir_all(&dir).unwrap();
    let meta = json!({"id": page_id, "version": version, "page_type": "macro", "tier": 2,
                      "renderer_version": "plate-1.0.0", "data_ids": ["dp_a", "dv_b"]})
    .to_string();
    let files = [
        ("data.json", r#"{"series": {"sr_x": {}}}"#.to_owned()),
        ("index.html", html.to_owned()),
        ("meta.json", meta),
        ("page.oui", "root = Page([])".to_owned()),
    ];
    let mut listing = String::new();
    let mut entries = Vec::new();
    for (path, text) in &files {
        std::fs::write(dir.join(path), text).unwrap();
        let sha = news_chief::gates::hex(&Sha256::digest(text.as_bytes()));
        listing.push_str(&format!("{path} {sha}\n"));
        entries.push(json!({"path": path, "sha256": sha, "bytes": text.len()}));
    }
    let sha: [u8; 32] = Sha256::digest(listing.as_bytes()).into();
    let manifest = json!({"schema": "manifest.v1", "page_id": page_id, "version": version,
                          "bundle_sha256": news_chief::gates::hex(&sha), "files": entries});
    std::fs::write(dir.join("manifest.json"), manifest.to_string()).unwrap();
    sha
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn register_copies_the_bundle_fills_the_rows_and_is_idempotent() {
    let f = Fixture::migrated().await;
    let story: i64 = sqlx::query_scalar("insert into stories (event_key, first_seen, last_seen) values ('e', now(), now()) returning story_id")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    let root = root_for(&f);
    let work = root.join("work");
    let s = story.to_string();
    let sha = story_bundle(&work, &s, "a-page", 1, "<p>one</p>");
    let reg = |sha: [u8; 32]| {
        let (pool, work, s) = (f.pool.clone(), work.clone(), s.clone());
        async move {
            let checked = news_chief::gates::hex(&sha);
            news_press::register::register(&pool, &work, &s, story, None, &checked, now()).await
        }
    };

    // Bytes other than the ones Proof checked are refused, and nothing is registered.
    let e = reg([7; 32]).await.unwrap_err();
    assert_eq!(e.code, news_core::ErrorCode::Conflict);
    assert_eq!(f.scalar_i64("select count(*) from pages").await, 0);

    let r = reg(sha).await.unwrap();
    assert_eq!(
        (r.page_id.as_str(), r.version, r.bundle_sha, r.changed),
        ("a-page", 1, sha, true)
    );
    news_press::bundle::load(&work, "a-page", 1).expect("the copy in Press's layout verifies");
    assert_eq!(
        f.scalar_str("select status || ':' || tier::text || ':' || page_type from pages")
            .await,
        "draft:2:macro"
    );
    assert_eq!(
        f.scalar_str("select string_agg(ref, ',' order by ref) from page_refs where version = 1")
            .await,
        "dp_a,dv_b,sr_x"
    );

    // The same bundle again: nothing changes, nothing is duplicated.
    let again = reg(sha).await.unwrap();
    assert!(!again.changed);
    assert_eq!(f.scalar_i64("select count(*) from pages").await, 1);
    assert_eq!(f.scalar_i64("select count(*) from page_refs").await, 3);

    // A re-render of the unsent version replaces the bytes; a version that skips one is refused.
    let sha2 = story_bundle(&work, &s, "a-page", 1, "<p>one again</p>");
    let r = reg(sha2).await.unwrap();
    assert!(r.changed && r.bundle_sha == sha2);
    let sha3 = story_bundle(&work, &s, "a-page", 3, "<p>three</p>");
    let e = reg(sha3).await.unwrap_err();
    assert_eq!(e.code, news_core::ErrorCode::Conflict);

    // A bundle staged for sending (`ready`) is never replaced, and its bytes on disk stay as staged.
    sqlx::query("update pages set status = 'ready'")
        .execute(&f.pool)
        .await
        .unwrap();
    let other = story_bundle(&work, &s, "a-page", 1, "<p>one, third time</p>");
    let e = reg(other).await.unwrap_err();
    assert_eq!(e.code, news_core::ErrorCode::Conflict);
    assert_eq!(
        news_press::bundle::load(&work, "a-page", 1)
            .unwrap()
            .bundle_sha,
        sha2
    );

    // A link in the story folder is refused, not followed.
    #[cfg(unix)]
    {
        sqlx::query("update pages set status = 'draft'")
            .execute(&f.pool)
            .await
            .unwrap();
        let dir = work.join("pages").join(&s);
        let linked = story_bundle(&work, &s, "a-page", 1, "<p>linked</p>");
        let outside = root.join("outside.html");
        std::fs::write(&outside, "<p>linked</p>").unwrap();
        std::fs::remove_file(dir.join("index.html")).unwrap();
        std::os::unix::fs::symlink(&outside, dir.join("index.html")).unwrap();
        let e = reg(linked).await.unwrap_err();
        assert_eq!(e.code, news_core::ErrorCode::ValidationFailed, "{e:?}");
        // Drop the link, so the next bundle writes a regular file instead of writing through it.
        std::fs::remove_file(dir.join("index.html")).unwrap();
    }

    // Once published, the next version advances the same row.
    sqlx::query("update pages set status = 'published'")
        .execute(&f.pool)
        .await
        .unwrap();
    let sha_v2 = story_bundle(&work, &s, "a-page", 2, "<p>two</p>");
    let r = reg(sha_v2).await.unwrap();
    assert_eq!((r.version, r.changed), (2, true));
    assert_eq!(
        f.scalar_str("select status || ':' || version::text from pages")
            .await,
        "published:1",
        "the live version stays published"
    );
    assert_eq!(
        f.scalar_i64("select version::bigint from page_drafts")
            .await,
        2
    );
    let again = reg(sha_v2).await.unwrap();
    assert!(!again.changed, "the same draft again is a no-op");
    assert!(
        news_press::register::discard_draft(&f.pool, "a-page")
            .await
            .unwrap()
    );
    assert_eq!(f.scalar_i64("select count(*) from page_drafts").await, 0);
    assert_eq!(
        f.scalar_i64("select count(*) from page_refs where version = 2")
            .await,
        0
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn prune_removes_the_built_bundle_and_screenshots_and_keeps_what_a_rerender_needs() {
    let f = Fixture::migrated().await;
    let story: i64 = sqlx::query_scalar("insert into stories (event_key, first_seen, last_seen) values ('e', now(), now()) returning story_id")
        .fetch_one(&f.pool)
        .await
        .unwrap();
    let root = root_for(&f);
    let work = root.join("work");
    let s = story.to_string();
    let sha = story_bundle(&work, &s, "a-page", 1, "<p>one</p>");
    let dir = work.join("pages").join(&s);
    for kept in ["page.json", "copy.json", "sources.json"] {
        std::fs::write(dir.join(kept), "{}").unwrap();
    }
    std::fs::write(dir.join("home.png"), "png").unwrap();
    std::fs::create_dir_all(work.join("briefs")).unwrap();
    std::fs::write(work.join("briefs").join(format!("{s}.json")), "{}").unwrap();
    let checked = news_chief::gates::hex(&sha);
    news_press::register::register(&f.pool, &work, &s, story, None, &checked, now())
        .await
        .unwrap();
    // Sent and published.
    sqlx::query("update pages set status = 'published'")
        .execute(&f.pool)
        .await
        .unwrap();
    sqlx::query(
        "insert into publish_outbox (page_id, version, bundle_sha, idempotency_key, op, sent_at)
         values ('a-page', 1, $1, 'page:a-page:1:k', 'put_page', now())",
    )
    .bind(&sha[..])
    .execute(&f.pool)
    .await
    .unwrap();

    let (files, _) = news_press::prune::prune(&f.pool, &work).await.unwrap();
    assert!(
        files >= 9,
        "the Press copy (5 files), the built files and the screenshot: {files}"
    );
    for gone in [
        "index.html",
        "meta.json",
        "manifest.json",
        "page.oui",
        "home.png",
    ] {
        assert!(!dir.join(gone).exists(), "{gone}");
    }
    assert!(!work.join("pages/a-page").exists());
    for kept in ["data.json", "page.json", "copy.json", "sources.json"] {
        assert!(dir.join(kept).is_file(), "{kept} must stay");
    }
    assert!(
        work.join("briefs").join(format!("{s}.json")).is_file(),
        "the brief stays for a re-render"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_home_draft_passes_the_a8_accept_branch() {
    let ok = news::steps::accept::home_draft(super::home::draft());
    assert!(ok.valid && !ok.stored, "{:?}", ok.errors);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn page_revise_by_code_refuses_a_flip_and_a_page_with_no_story_folder() {
    let f = Fixture::migrated().await;
    super::rewrite::seed(&f).await;
    let root = root_for(&f);
    let work = Work::new(root.join("work"));
    let flip = news_rewrite::revise(&f.pool, &work, "p-flip", now())
        .await
        .unwrap_err();
    assert_eq!(flip.class, news_core::job::ErrorClass::InvalidData);
    assert!(flip.message.contains("direction flip"), "{}", flip.message);
    // A value-only page whose story has no page folder is an item failure, not a panic or a bug.
    let value = news_rewrite::revise(&f.pool, &work, "p-value", now())
        .await
        .unwrap_err();
    assert_eq!(
        value.class,
        news_core::job::ErrorClass::InvalidData,
        "{}",
        value.message
    );
    // The handler without its context is a bug, as before.
    let none = news_rewrite::handle(JobInput {
        job_id: 1,
        kind: "page.revise".into(),
        pool: "cpu".into(),
        attempt: 1,
        run_id: None,
        item_ref: None,
        args: json!({"page_id": "p-value"}),
        lease: Default::default(),
    })
    .await
    .unwrap_err();
    assert_eq!(none.class, news_core::job::ErrorClass::Bug);
    let _ = std::fs::remove_dir_all(&root);
}

// ---- the fix round: live pages stay live, rejections, re-entries -------------------------------------

/// A new vintage of the story's latest point: 2.0 becomes `to` (2.1 is a value-only revision, a
/// negative value a direction flip).
async fn revise_point(f: &Fixture, to: &str) {
    sqlx::query(
        "insert into data_points (dp_id, context_key, series_key, concept, label, value_text, value, unit,
           period_label, period_start, period_end, period_type, vintage, supersedes, snapshot_sha, locator,
           parser, retrieved_at)
         select 'dp_t30', context_key, series_key, concept, label, $1, $1::numeric, unit, period_label, period_start,
                period_end, period_type, '2026-11-22T00:00:00Z', 'dp_t29', snapshot_sha, locator, parser, now()
           from data_points where dp_id = 'dp_t29'",
    )
    .bind(to)
    .execute(&f.pool)
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn followup_re_renders_a_value_only_revision_and_ends_with_the_page_published_at_version_two()
{
    let w = world().await;
    let first = w.start("edition", json!({})).await;
    wait_status(&w.f, first.run_id, "succeeded").await;
    revise_point(&w.f, "2.1").await;
    let run = w.start("followup", json!({})).await;
    wait_status(&w.f, run.run_id, "succeeded").await;
    assert_eq!(
        w.f.scalar_str("select status || ':' || version::text from pages")
            .await,
        "published:2"
    );
    assert_eq!(w.n("select count(*) from page_drafts").await, 0);
    assert_eq!(
        w.n("select count(*) from publish_outbox where op = 'put_page' and sent_at is not null")
            .await,
        2
    );
    w.finish().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn followup_sends_a_direction_flip_to_a1_and_publishes_the_update_at_version_two() {
    let w = world().await;
    let first = w.start("edition", json!({})).await;
    wait_status(&w.f, first.run_id, "succeeded").await;
    // 2.0 becomes -0.3: the sign changes, so the prose may no longer hold and A1 rewrites it.
    revise_point(&w.f, "-0.3").await;
    let run = w.start("followup", json!({})).await;
    wait_status(&w.f, run.run_id, "succeeded").await;
    let (job, state, key, args): (i64, String, String, Value) = sqlx::query_as(
        "select job_id, state, unique_key, args from jobs where run_id = $1 and kind = 'ai.write'",
    )
    .bind(run.run_id)
    .fetch_one(&w.f.pool)
    .await
    .unwrap();
    assert_eq!(state, "completed", "{args}");
    assert!(key.starts_with("update:macro-us-t-2026-06:2:"), "{key}");
    // The update's Copy is the story's stored Copy (as NewsAccept stores it), and the next version.
    let stored =
        news_proof::work::load(&Work::new(w.root.join("work")), &w.story.to_string()).unwrap();
    assert_eq!(stored.copy["headline"], "Updated");
    assert_eq!(stored.job, Some(job));
    assert_eq!(
        w.f.scalar_str("select status || ':' || version::text from pages")
            .await,
        "published:2"
    );
    assert_eq!(w.n("select count(*) from page_drafts").await, 0);
    assert_eq!(
        w.n("select count(*) from publish_outbox where op = 'put_page' and sent_at is not null")
            .await,
        2
    );
    w.finish().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn edition_re_entered_after_a_lost_answer_still_publishes_its_planned_revision() {
    let w = world().await;
    let first = w.start("edition", json!({})).await;
    wait_status(&w.f, first.run_id, "succeeded").await;
    // Nothing new to write; one value-only revision to publish, and the API does not answer at first.
    sqlx::query("update stories set status = 'covered'")
        .execute(&w.f.pool)
        .await
        .unwrap();
    revise_point(&w.f, "2.1").await;
    w.api.script(std::iter::repeat_n(
        news_press::fake::Fault::Unreachable,
        news_press::api::MAX_ATTEMPTS as usize,
    ));
    let run = w.start("edition", json!({})).await;
    wait_status(&w.f, run.run_id, "succeeded").await;
    assert_eq!(
        w.f.scalar_str("select status || ':' || version::text from pages")
            .await,
        "published:2"
    );
    assert_eq!(w.n("select count(*) from page_drafts").await, 0);
    w.finish().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn edition_ignores_nothing_when_the_plan_is_rejected() {
    let w = world().await;
    let run = w.start("edition", json!({"review_plan": true})).await;
    wait_status(&w.f, run.run_id, "waiting_approval").await;
    let g = open_gate(&w.f, run.run_id).await;
    assert_eq!(g.0, "plan");
    answer(&w.ch, &run.short_id, &g, Decision::Rejected, None).await;
    wait_status(&w.f, run.run_id, "succeeded").await;
    assert_eq!(w.n("select count(*) from pages").await, 0);
    assert_eq!(
        w.n("select count(*) from jobs where kind in ('ai.write', 'ai.repair')")
            .await,
        0
    );
    assert_eq!(
        w.f.scalar_str(&format!(
            "select status from run_steps where run_id = '{}' and step = 'publish'",
            run.run_id
        ))
        .await,
        "skipped"
    );
    w.finish().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn one_page_that_cannot_be_built_is_dead_and_the_run_goes_on() {
    let w = world().await;
    w.tools
        .render_error
        .lock()
        .unwrap()
        .insert(w.story.to_string());
    let run = w.start("edition", json!({})).await;
    wait_status(&w.f, run.run_id, "succeeded").await;
    assert_eq!(w.n("select count(*) from pages").await, 0);
    assert_eq!(
        w.n("select count(*) from jobs where kind = 'ai.repair'")
            .await,
        0,
        "no repair for a page with no inputs"
    );
    assert_eq!(
        w.f.scalar_str(&format!(
            "select output->'dead'->0->>'error' from run_steps where run_id = '{}' and step = 'render'",
            run.run_id
        ))
        .await,
        "the page has no inputs"
    );
    w.finish().await;
}

/// Runs the step, then fails it once after it yielded: a crash between a chunk's publish and its checkpoint.
struct CrashAfterYield(
    Arc<dyn news_chief::driver::StepHandler>,
    std::sync::atomic::AtomicBool,
);

impl news_chief::driver::StepHandler for CrashAfterYield {
    fn run<'a>(
        &'a self,
        cx: &'a mut news_chief::driver::StepCx,
    ) -> BoxFut<'a, Result<news_chief::driver::StepOutcome, news_chief::driver::StepError>> {
        Box::pin(async move {
            let out = self.0.run(cx).await;
            if matches!(out, Ok(news_chief::driver::StepOutcome::Yield(_)))
                && !self.1.swap(true, std::sync::atomic::Ordering::SeqCst)
            {
                return Err(news_chief::driver::StepError::Retry("crash".into()));
            }
            out
        })
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn catchup_re_entering_a_published_chunk_does_not_publish_a_needless_next_version() {
    let w = world_with(|h| {
        let inner = h.get("catchup.chunks");
        h.with(
            "catchup.chunks",
            Arc::new(CrashAfterYield(inner, Default::default())),
        )
    })
    .await;
    let run = w.start("catchup", json!({"chunk": 1})).await;
    wait_status(&w.f, run.run_id, "waiting_approval").await;
    let g = open_gate(&w.f, run.run_id).await;
    answer(&w.ch, &run.short_id, &g, Decision::Approved, None).await;
    wait_status(&w.f, run.run_id, "succeeded").await;
    assert_eq!(
        w.f.scalar_str("select status || ':' || version::text from pages")
            .await,
        "published:1",
        "the chunk ran twice but the page was sent once"
    );
    assert_eq!(
        w.n("select count(*) from publish_outbox where op = 'put_page'")
            .await,
        1
    );
    w.finish().await;
}

// ---- a run publishes only what it registered, and Reprint only what Board approved ----------------

struct StepDone;
impl news_chief::driver::StepHandler for StepDone {
    fn run<'a>(
        &'a self,
        _: &'a mut news_chief::driver::StepCx,
    ) -> BoxFut<'a, Result<news_chief::driver::StepOutcome, news_chief::driver::StepError>> {
        Box::pin(async { Ok(news_chief::driver::StepOutcome::Done(json!({}))) })
    }
}

struct StepSkip;
impl news_chief::driver::StepHandler for StepSkip {
    fn run<'a>(
        &'a self,
        _: &'a mut news_chief::driver::StepCx,
    ) -> BoxFut<'a, Result<news_chief::driver::StepOutcome, news_chief::driver::StepError>> {
        Box::pin(async { Ok(news_chief::driver::StepOutcome::Skip("no bulk".into())) })
    }
}

/// A publish step whose run has the page in its Budget (so the start covers it) but never rendered or
/// checked it: it hands the page's story to the shared publish phase, as Edition does for its updates.
struct PublishStory;
impl news_chief::driver::StepHandler for PublishStory {
    fn run<'a>(
        &'a self,
        cx: &'a mut news_chief::driver::StepCx,
    ) -> BoxFut<'a, Result<news_chief::driver::StepOutcome, news_chief::driver::StepError>> {
        Box::pin(async move {
            let (story, page): (i64, String) =
                sqlx::query_as("select story_id, page_id from pages limit 1")
                    .fetch_one(&cx.db)
                    .await
                    .unwrap();
            sqlx::query(
                "update runs set budget = jsonb_build_object('items', jsonb_build_array($2::text))
                  where run_id = $1",
            )
            .bind(cx.run_id)
            .bind(&page)
            .execute(&cx.db)
            .await
            .unwrap();
            let ctx = ctx(&root_of(Some(cx.run_id)), tools(), api());
            let (staged, _, _) = news::steps::pipeline::publish(cx, &ctx, &[story], None).await?;
            Ok(news_chief::driver::StepOutcome::Done(
                json!({"staged": staged}),
            ))
        })
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres"]
async fn a_run_publishes_only_its_own_drafts_and_reprint_only_the_bytes_board_approved() {
    let w = world_with(|h| {
        h.with("followup.find", Arc::new(StepDone))
            .with("followup.classify", Arc::new(StepDone))
            .with("followup.bulk_review", Arc::new(StepSkip))
            .with("followup.rerender", Arc::new(StepDone))
            .with("followup.update", Arc::new(StepDone))
            .with("followup.publish", Arc::new(PublishStory))
    })
    .await;
    let first = w.start("edition", json!({})).await;
    wait_status(&w.f, first.run_id, "succeeded").await;

    // Reprint registers version 2 and waits at its sample gate.
    *w.tools.version.lock().unwrap() = "plate-2.0.0".into();
    let reprint = w.start("reprint", json!({"renderer": "plate-2.0.0"})).await;
    wait_status(&w.f, reprint.run_id, "waiting_approval").await;
    let puts = "select count(*) from publish_outbox where op = 'put_page'";
    assert_eq!(w.n(puts).await, 1);

    // Another run, with the page in its Budget, does not publish the draft Reprint is waiting on.
    let other = w.start("followup", json!({})).await;
    wait_status(&w.f, other.run_id, "succeeded").await;
    assert_eq!(
        w.f.scalar_str(&format!(
            "select output->>'staged' from run_steps where run_id = '{}' and step = 'publish'",
            other.run_id
        ))
        .await,
        "0"
    );
    assert_eq!(w.n(puts).await, 1, "nothing staged for version 2");
    assert_eq!(
        w.f.scalar_str("select state from page_drafts").await,
        "draft"
    );
    assert_eq!(
        w.f.scalar_str("select status || ':' || version::text from pages")
            .await,
        "published:1"
    );

    // Reprint's draft changes after Board saw the sample: the approval does not cover those bytes.
    sqlx::query("update page_drafts set bundle_sha = sha256('not what Board saw'::bytea)")
        .execute(&w.f.pool)
        .await
        .unwrap();
    let g = open_gate(&w.f, reprint.run_id).await;
    answer(&w.ch, &reprint.short_id, &g, Decision::Approved, None).await;
    wait_status(&w.f, reprint.run_id, "failed").await;
    assert_eq!(w.n(puts).await, 1, "nothing staged under the approval");
    assert_eq!(
        w.f.scalar_str("select status || ':' || version::text from pages")
            .await,
        "published:1"
    );
    assert_eq!(w.api.applied().len(), 2, "v1 and its index promotion only");
    w.finish().await;
}
