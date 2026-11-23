//! `news sources sync|list|show|health` against a real database. The sources come from two
//! literal files in a temporary directory (`NEWS_SOURCES_DIR`), so the golden output does not move when
//! someone edits `data/sources/`. No network: `probe` is covered by the Runner's own fake-transport tests.

use super::fixture::{Fixture, call};
use serde_json::{Value, json};

const FRED: &str = r#"
source_id = "fred"
name = "FRED and ALFRED"
publisher = "Federal Reserve Bank of St. Louis"
connector = "fred"
base_url = "https://api.stlouisfed.org/fred/"
source_tier = 2
licence = "custom"
attribution = "Source: Federal Reserve Bank of St. Louis, FRED."
redistribution = false
image_policy = "none"
rate_per_sec = 0.5
documented_rate_per_sec = 2.0
documented_limit = "test fixture"
auth_env = "NEWS_TEST_ABSENT_KEY"
regions = ["us"]
categories = ["economy"]
"#;

const BLS: &str = r#"
source_id = "bls"
name = "BLS Public Data API"
publisher = "U.S. Bureau of Labor Statistics"
connector = "bls"
base_url = "https://api.bls.gov/publicAPI/v2/"
source_tier = 1
licence = "public-domain"
attribution = "Source: U.S. Bureau of Labor Statistics."
redistribution = true
image_policy = "none"
rate_per_sec = 0.5
documented_rate_per_sec = 5.0
documented_limit = "test fixture"
daily_quota = 400
regions = ["us"]
categories = ["economy"]
enabled = false
"#;

/// The binary pointed at the test schema and a directory holding the two sources.
fn news(f: &Fixture, dir: &std::path::Path) -> assert_cmd::Command {
    let mut cmd = f.news();
    cmd.env("NEWS_SOURCES_DIR", dir)
        .env_remove("NEWS_TEST_ABSENT_KEY");
    cmd
}

fn sources_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("news-sources-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("fred.toml"), FRED).unwrap();
    std::fs::write(dir.join("bls.toml"), BLS).unwrap();
    dir
}

fn pick(v: &Value, keys: &[&str]) -> Value {
    Value::Object(
        keys.iter()
            .map(|k| ((*k).to_owned(), v[*k].clone()))
            .collect(),
    )
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs Postgres: `bun run infra:up`, then `bun run test:integration`"]
async fn sources_sync_list_show_and_health() {
    let f = Fixture::migrated().await;
    let dir = sources_dir();

    // Sync is idempotent and loads both files.
    for _ in 0..2 {
        let (env, code, _, _) = call(&mut news(&f, &dir), &["sources", "sync"]);
        assert_eq!(code, 0, "{env}");
        assert_eq!(env["data"], json!({ "synced": 2 }));
    }

    // List: every source, in source_id order, with the contract's fields.
    let (env, code, _, _) = call(&mut news(&f, &dir), &["sources", "list"]);
    assert_eq!(code, 0, "{env}");
    assert_eq!(
        env["data"],
        json!({ "items": [
            { "source_id": "bls", "name": "BLS Public Data API",
              "publisher": "U.S. Bureau of Labor Statistics", "source_tier": 1,
              "licence": "public-domain", "enabled": false, "health": "unknown" },
            { "source_id": "fred", "name": "FRED and ALFRED",
              "publisher": "Federal Reserve Bank of St. Louis", "source_tier": 2,
              "licence": "custom", "enabled": true, "health": "unknown" },
        ] })
    );

    // Show: the whole Contacts row.
    let (env, code, _, _) = call(&mut news(&f, &dir), &["sources", "show", "fred"]);
    assert_eq!(code, 0, "{env}");
    assert_eq!(
        pick(
            &env["data"],
            &[
                "source_id",
                "connector",
                "base_url",
                "source_tier",
                "licence",
                "attribution",
                "redistribution",
                "image_policy",
                "auth_env",
                "rate_per_sec",
                "max_concurrency",
                "daily_quota",
                "robots_respect",
                "robots_exempt",
                "requires_contact_email",
                "regions",
                "categories",
                "calendar_kind",
                "enabled",
            ]
        ),
        json!({
            "source_id": "fred",
            "connector": "fred",
            "base_url": "https://api.stlouisfed.org/fred/",
            "source_tier": 2,
            "licence": "custom",
            "attribution": "Source: Federal Reserve Bank of St. Louis, FRED.",
            "redistribution": false,
            "image_policy": "none",
            "auth_env": "NEWS_TEST_ABSENT_KEY",
            "rate_per_sec": 0.5,
            "max_concurrency": 1,
            "daily_quota": null,
            "robots_respect": true,
            "robots_exempt": [],
            "requires_contact_email": false,
            "regions": ["us"],
            "categories": ["economy"],
            "calendar_kind": null,
            "enabled": true,
        })
    );

    // An unknown source is not_found (exit 3).
    let (env, code, _, _) = call(&mut news(&f, &dir), &["sources", "show", "nope"]);
    assert_eq!(code, 3, "{env}");
    assert_eq!(env["error"]["code"], "not_found");

    // Health: nothing fetched yet. A disabled source and a source whose free key is absent are amber,
    // and the two columns that live elsewhere are empty.
    let (env, code, _, _) = call(&mut news(&f, &dir), &["sources", "health"]);
    assert_eq!(code, 0, "{env}");
    let items = env["data"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(
        items[0]["reasons"],
        json!(["disabled", "never fetched"]),
        "{env}"
    );
    assert_eq!(
        items[1]["reasons"],
        json!(["free key absent (NEWS_TEST_ABSENT_KEY)", "never fetched"]),
        "{env}"
    );
    for i in items {
        assert_eq!(i["health"], "amber");
        assert_eq!(i["error_classes"], json!({}));
        assert_eq!(i["quota_used"], Value::Null);
        assert_eq!(i["last_success"], Value::Null);
    }

    // A fetch 2 days ago (200, then a 304) and one URL that is failing: green becomes red.
    for (url, status, errors) in [("https://a/", 200_i16, 0_i16), ("https://b/", 304, 3)] {
        sqlx::query(
            "insert into fetch_state (url_hash, url, source_id, last_status, last_fetched, consecutive_errors)
             values (sha256(convert_to($1, 'UTF8')), $1, 'fred', $2, now() - interval '2 days', $3)",
        )
        .bind(url)
        .bind(status)
        .bind(errors)
        .execute(&f.pool)
        .await
        .unwrap();
    }
    let (env, code, _, _) = call(&mut news(&f, &dir), &["sources", "health"]);
    assert_eq!(code, 0, "{env}");
    let fred = &env["data"]["items"][1];
    assert_eq!(fred["health"], "red", "{env}");
    assert_eq!(fred["ratio_304"], 0.5);
    assert!(fred["last_success"].is_string());
    assert!(
        fred["reasons"]
            .as_array()
            .unwrap()
            .contains(&json!("3 consecutive errors on one URL"))
    );
    // `--since 1d` puts that fetch outside the window: nothing counted, no recent success.
    let (env, _, _, _) = call(&mut news(&f, &dir), &["sources", "health", "--since", "1d"]);
    assert_eq!(env["data"]["items"][1]["ratio_304"], 0.0, "{env}");
    let (env, code, _, _) = call(&mut news(&f, &dir), &["sources", "health", "--since", "7w"]);
    assert_eq!((code, &env["error"]["code"]), (2, &json!("usage")));
    std::fs::remove_dir_all(dir).unwrap();
}
