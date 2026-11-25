//! Rewrite against a real database (#20): the published pages a revision touches, the three
//! classifications, `news update <page>`, and the update job's input for A1.

use super::fixture::{Fixture, call};
use news_rewrite::classify::Kind;
use news_rewrite::{find, plan};
use serde_json::{Value, json};
use sqlx::PgPool;

/// One data point; `supersedes` names the point it replaces. Later vintages have later days.
async fn point(
    pool: &PgPool,
    id: &str,
    text: &str,
    fiscal: Option<&str>,
    supersedes: Option<&str>,
    day: i32,
) {
    sqlx::query(
        "insert into data_points (dp_id, context_key, series_key, concept, label, value_text, value, unit,
                                  period_label, period_type, vintage, snapshot_sha, locator, parser,
                                  retrieved_at, fiscal_period, supersedes)
         values ($1, '', $5, 'CPI', 'CPI', $2, $2::numeric, 'Percent', 'October 2026', 'duration',
                 ('2026-11-' || lpad($6::text, 2, '0') || 'T00:00:00Z')::timestamptz,
                 decode(repeat('ab', 32), 'hex'), '{\"kind\": \"json\"}', 'bls@1', now(), $3, $4)",
    )
    .bind(id)
    .bind(text)
    .bind(fiscal)
    .bind(supersedes)
    .bind(format!("bls:{id}"))
    .bind(day)
    .execute(pool)
    .await
    .unwrap();
}

async fn page(pool: &PgPool, id: &str, status: &str, refs: &[&str]) {
    sqlx::query(
        "insert into pages (page_id, slug, version, status, published_at) values ($1, $1, 1, $2, now())",
    )
    .bind(id)
    .bind(status)
    .execute(pool)
    .await
    .unwrap();
    for r in refs {
        sqlx::query("insert into page_refs (page_id, version, ref) values ($1, 1, $2)")
            .bind(id)
            .bind(r)
            .execute(pool)
            .await
            .unwrap();
    }
}

async fn seed(f: &Fixture) {
    sqlx::raw_sql(
        "insert into sources (source_id, name, publisher, connector, base_url, source_tier, licence, attribution,
                              redistribution, image_policy, rate_per_sec, regions, categories)
         values ('bls', 'BLS', 'BLS', 'bls', 'https://api.bls.gov', 1, 'public-domain', 'BLS', true, 'none', 0.5,
                 '{us}', '{economy}');
         insert into snapshots (sha256, source_id, url, fetched_at, http_status, bytes, storage_path)
         values (decode(repeat('ab', 32), 'hex'), 'bls', 'https://api.bls.gov/x', now(), 200, 10, 'work/raw/x');",
    )
    .execute(&f.pool)
    .await
    .unwrap();
    let p = &f.pool;
    // Value only: 2.7 becomes 2.6, and the change against 2.4 goes from 0.3 to 0.2.
    point(p, "dp_old", "2.7", None, None, 1).await;
    point(p, "dp_new", "2.6", None, Some("dp_old"), 5).await;
    point(p, "dp_base", "2.4", None, None, 1).await;
    sqlx::query(
        "insert into derived_points (dp_id, formula, inputs, value, unit, rounding, guard)
         values ('dv_old', 'pp_change(a,b)', '{dp_old,dp_base}', 0.3, 'Percentage points', 1, '{}')",
    )
    .execute(p)
    .await
    .unwrap();
    // A direction flip: -0.2 becomes 0.1.
    point(p, "dp_flip_old", "-0.2", None, None, 1).await;
    point(p, "dp_flip_new", "0.1", None, Some("dp_flip_old"), 5).await;
    // A restatement: an earnings fact (it has a fiscal period) is revised.
    point(p, "dp_fact_old", "10.0", Some("Q3"), None, 1).await;
    point(p, "dp_fact_new", "9.5", Some("Q3"), Some("dp_fact_old"), 5).await;
    // A new vintage with the same text needs nothing; a chain resolves to its newest vintage.
    point(p, "dp_same_old", "5.0", None, None, 1).await;
    point(p, "dp_same_new", "5.0", None, Some("dp_same_old"), 5).await;
    point(p, "dp_chain_a", "1.0", None, None, 1).await;
    point(p, "dp_chain_b", "1.1", None, Some("dp_chain_a"), 3).await;
    point(p, "dp_chain_c", "1.2", None, Some("dp_chain_b"), 6).await;

    page(p, "p-value", "published", &["dp_old", "dv_old", "dp_base"]).await;
    page(p, "p-flip", "published", &["dp_flip_old"]).await;
    page(p, "p-fact", "published", &["dp_fact_old"]).await;
    page(p, "p-same", "published", &["dp_same_old"]).await;
    page(p, "p-chain", "published", &["dp_chain_a"]).await;
    // Not published: never revised by Rewrite.
    page(p, "p-draft", "draft", &["dp_old"]).await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn revisions_are_found_through_page_refs_and_classified() {
    let f = Fixture::migrated().await;
    seed(&f).await;
    let found = find::find(&f.pool, None).await.unwrap();
    let plans = plan::plan(found);
    let kinds: Vec<(&str, Kind)> = plans
        .iter()
        .map(|p| (p.change.page_id.as_str(), p.kind))
        .collect();
    assert_eq!(
        kinds,
        [
            ("p-chain", Kind::ValueOnly),
            ("p-fact", Kind::Restatement),
            ("p-flip", Kind::DirectionFlip),
            ("p-value", Kind::ValueOnly),
        ]
    );
    let chain = &plans[0].change.points[0];
    assert_eq!(
        (chain.new_id.as_str(), chain.new_text.as_str()),
        ("dp_chain_c", "1.2")
    );
    // The derived change is recomputed exactly: 2.6 - 2.4.
    let value = &plans[3].change;
    assert_eq!(value.derived[0].new_value.as_deref(), Some("0.2"));
    assert_eq!(
        value.changed_refs(),
        ["dp_old".to_owned(), "dv_old".to_owned()]
    );
    // Value-only and restated pages need no AI; the flip does.
    assert_eq!(plan::counts(&plans)["needs_ai"], 1);
    // A source filter that matches nothing finds nothing.
    assert!(find::find(&f.pool, Some("fred")).await.unwrap().is_empty());
    assert_eq!(find::find(&f.pool, Some("bls")).await.unwrap().len(), 4);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn news_update_shows_a_published_page_and_its_changed_refs() {
    let f = Fixture::migrated().await;
    seed(&f).await;
    let run = |page: &str| {
        let mut cmd = f.news();
        call(&mut cmd, &["update", page])
    };
    let (env, code, _, _) = run("p-flip");
    assert_eq!(code, 0, "{env}");
    assert_eq!(env["data"]["page"]["revision"], "direction_flip");
    assert_eq!(env["data"]["page"]["by_code"], false);
    assert_eq!(env["data"]["changed_refs"], json!(["dp_flip_old"]));
    assert_eq!(
        env["data"]["page"]["changes"],
        json!([{"ref": "dp_flip_new", "old": "-0.2", "new": "0.1"}])
    );
    // A page path names the page too, and a page with current data has nothing changed.
    let (env, code, _, _) = run("/2026/11/p-fact");
    assert_eq!(
        (code, env["data"]["page"]["revision"].as_str()),
        (0, Some("restatement")),
        "{env}"
    );
    page(&f.pool, "p-fresh", "published", &["dp_base"]).await;
    let (env, _, _, _) = run("p-fresh");
    assert_eq!(env["data"]["page"]["revision"], "current");
    assert_eq!(env["data"]["changed_refs"], json!([]));
    // Not published (or not there): not_found.
    for missing in ["p-draft", "p-nowhere"] {
        let (env, code, _, _) = run(missing);
        assert_eq!(
            (code, env["error"]["code"].as_str()),
            (3, Some("not_found")),
            "{env}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn an_update_job_gives_a1_the_brief_the_previous_copy_and_the_diff() {
    let f = Fixture::migrated().await;
    seed(&f).await;
    let dir = std::env::temp_dir().join(format!("news-rewrite-{}", f.schema));
    std::fs::create_dir_all(dir.join("briefs")).unwrap();
    let brief = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../news-proof/tests/fixtures/brief.json");
    std::fs::copy(brief, dir.join("briefs/81234.json")).unwrap();
    let work = news_proof::work::Work::new(&dir);
    news_proof::work::save(
        &work,
        "81234",
        json!({"headline": "Rates {dp_flip_old}"}),
        vec![],
        "save",
        None,
        "2026-11-01T10:00:00Z".parse().unwrap(),
    )
    .unwrap();
    let flip = plan::plan(find::find(&f.pool, None).await.unwrap())
        .into_iter()
        .find(|p| p.change.page_id == "p-flip")
        .unwrap();
    let job =
        news_dispatch::queue::enqueue(&f.pool, &news_rewrite::update::update_job(&flip, "81234"))
            .await
            .unwrap();
    // Queuing the same flip again is the same job.
    let again =
        news_dispatch::queue::enqueue(&f.pool, &news_rewrite::update::update_job(&flip, "81234"))
            .await
            .unwrap();
    assert_eq!(job.job_id, again.job_id);
    let mut cmd = f.news();
    cmd.env("NEWS_WORK_DIR", &dir);
    let (env, code, _, _) = call(&mut cmd, &["ai", "input", &job.job_id.to_string()]);
    assert_eq!(code, 0, "{env}");
    let input: Value = serde_json::from_str(
        &std::fs::read_to_string(env["data"]["path"].as_str().unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(input["mode"], "update");
    assert_eq!(input["brief"]["item"]["story_id"], 81234);
    assert_eq!(input["previous_copy"]["headline"], "Rates {dp_flip_old}");
    assert_eq!(
        input["diff"],
        json!([{"ref": "dp_flip_new", "old": "-0.2", "new": "0.1"}])
    );
}
