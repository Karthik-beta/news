//! The page inputs code writes before the writer runs (`news_proof::work`): the brief (Fixer), and the
//! story folder's `data.json`, `sources.json` and `page.json`, built from PostgreSQL with no model in the
//! loop. Every figure is the Ledger's text; nothing is converted or recomputed here.

use chrono::{DateTime, Utc};
use news_core::{NewsError, NewsResult};
use news_fixer::page::Provenance;
use news_proof::work::{Work, pretty, write_atomic};
use news_store::{PgPool, map_err};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

/// What the brief step wrote for one story.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    pub story: String,
    pub page_id: String,
    pub version: i32,
    pub tier: u8,
}

/// The page-local source id of a registry source (`^src_[a-z0-9]+(_[a-z0-9]+)*$`).
fn src_id(source_id: &str) -> String {
    let s: Vec<String> = source_id
        .to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect();
    format!("src_{}", s.join("_"))
}

fn doc_type(kind: &str) -> &'static str {
    match kind {
        "filing_section" => "filing",
        "press_release" => "press_release",
        "statement" => "statement",
        "report_chapter" => "report",
        "abstract" => "paper",
        _ => "other",
    }
}

fn licence_name(l: &str) -> &'static str {
    match l {
        "public-domain" => "Public domain",
        "cc-by-4.0" => "CC BY 4.0",
        "custom" => "Publisher's terms",
        _ => "Terms unclear",
    }
}

/// Build and write a story's brief and page inputs. `work` is the work directory (`<root>/work`).
/// A story that already has a page is briefed as its next version, keeping its first publication.
pub async fn write(
    db: &PgPool,
    work: &Work,
    root: &std::path::Path,
    story_id: i64,
) -> NewsResult<Written> {
    let inputs = news_fixer::db::load(db, &story_id.to_string()).await?;
    let built = news_fixer::brief(&inputs)?;
    news_fixer::db::save(db, &root.join("work"), &built).await?;

    // Provenance: each point's source and locator; each document's source.
    let ids: Vec<String> = inputs
        .history
        .iter()
        .chain(&inputs.points)
        .map(|p| p.dp_id.clone())
        .collect();
    let rows: Vec<(String, String, Value)> = sqlx::query_as(
        "select dp.dp_id, sn.source_id, dp.locator from data_points dp
           join snapshots sn on sn.sha256 = dp.snapshot_sha where dp.dp_id = any($1)",
    )
    .bind(&ids)
    .fetch_all(db)
    .await
    .map_err(map_err)?;
    let mut prov = Provenance::default();
    let mut used: BTreeMap<String, ()> = BTreeMap::new();
    for (dp, source, locator) in rows {
        used.insert(source.clone(), ());
        prov.points.insert(dp, (src_id(&source), locator));
    }
    let doc_ids: Vec<i64> = inputs.documents.iter().map(|d| d.doc_id).collect();
    type DocRow = (
        i64,
        String,
        Option<String>,
        String,
        Option<DateTime<Utc>>,
        String,
        DateTime<Utc>,
    );
    let docs: Vec<DocRow> = sqlx::query_as(
        "select d.doc_id, sn.source_id, d.title, d.kind, d.published_at, sn.url, sn.fetched_at
           from documents d join snapshots sn on sn.sha256 = d.snapshot_sha where d.doc_id = any($1)",
    )
    .bind(&doc_ids)
    .fetch_all(db)
    .await
    .map_err(map_err)?;
    for d in &docs {
        used.insert(d.1.clone(), ());
        prov.docs.insert(d.0, src_id(&d.1));
    }
    if let Some(first) = used.keys().next() {
        for i in &inputs.images {
            if let Some(url) = i["url"].as_str() {
                prov.images.insert(url.to_owned(), src_id(first));
            }
        }
    }
    let data = news_fixer::page::data(&inputs, &built, &prov)?;

    // sources.json: a document's own fields where a document stands behind the source, else the
    // source's latest snapshot; publisher, connector and redistribution from the registry.
    let mut sources = Map::new();
    for source in used.keys() {
        let reg: (String, String, String, String, bool) = sqlx::query_as(
            "select name, publisher, connector, licence, redistribution from sources where source_id = $1",
        )
        .bind(source)
        .fetch_one(db)
        .await
        .map_err(map_err)?;
        let doc = docs.iter().find(|d| &d.1 == source);
        let snap: (String, DateTime<Utc>) = match doc {
            Some(d) => (d.5.clone(), d.6),
            None => sqlx::query_as(
                "select s.url, s.fetched_at from snapshots s join data_points dp on dp.snapshot_sha = s.sha256
                  where s.source_id = $1 and dp.dp_id = any($2) order by s.fetched_at desc limit 1",
            )
            .bind(source)
            .bind(&ids)
            .fetch_one(db)
            .await
            .map_err(map_err)?,
        };
        sources.insert(
            src_id(source),
            json!({
                "title": doc.and_then(|d| d.2.clone()).unwrap_or_else(|| reg.0.clone()),
                "short": reg.0, "publisher": reg.1, "connector": reg.2,
                "type": doc.map_or("dataset", |d| doc_type(&d.3)), "primary": true, "url": snap.0,
                "licence": {"name": licence_name(&reg.3), "url": null, "redistribution": reg.4},
                "published_at": doc.and_then(|d| d.4).map(|t| t.date_naive().to_string()),
                "retrieved_at": snap.1.date_naive().to_string(),
            }),
        );
    }

    // page.json: the page's own facts. A page that exists is revised to its next version.
    let existing: Option<(String, i32, Option<DateTime<Utc>>, String)> = sqlx::query_as(
        "select page_id, version, published_at, status from pages where story_id = $1 order by version desc limit 1",
    )
    .bind(story_id)
    .fetch_optional(db)
    .await
    .map_err(map_err)?;
    let region: Option<String> = sqlx::query_scalar(
        "select region from candidates where story_id = $1 and region is not null order by cand_id limit 1",
    )
    .bind(story_id)
    .fetch_optional(db)
    .await
    .map_err(map_err)?;
    let mut page = json!({
        "categories": [inputs.categories.join("/")],
        "region": region.unwrap_or_else(|| "global".into()),
        "tags": [],
    });
    let page_id = match (&existing, &built.brief.item.slug_hint) {
        (Some((id, ..)), _) | (None, Some(id)) => id.clone(),
        (None, None) => format!("story-{story_id}"),
    };
    page["page_id"] = json!(page_id);
    // A published page is revised to its next version; an unsent draft is rebuilt at its own.
    let version = existing
        .as_ref()
        .map_or(1, |e| if e.3 == "published" { e.1 + 1 } else { e.1 });
    page["version"] = json!(version);
    if let Some((_, _, Some(t), _)) = &existing {
        page["published_at"] = json!(news_proof::work::timestamp(*t));
    }
    let story = story_id.to_string();
    let dir = work.page(&story);
    write_atomic(&dir.join("data.json"), &pretty(&data))?;
    write_atomic(
        &dir.join("sources.json"),
        &pretty(&json!({"version": 1, "sources": sources})),
    )?;
    write_atomic(&dir.join("page.json"), &pretty(&page))?;
    Ok(Written {
        story,
        page_id,
        version,
        tier: built.brief.item.tier,
    })
}

/// Something wrong with a story that is the story's own (no page type yet, no points): the item fails,
/// the run does not.
pub fn is_item_error(e: &NewsError) -> bool {
    !e.retryable
}
