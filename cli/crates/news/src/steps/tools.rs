//! What the page steps use from outside the database: rendering and checking a page folder, and
//! sending the outbox. Production binds them to Plate, Proof and the write API; tests bind them to
//! fakes (a bundle assembled from a canned render; a scripted API), as the Frontpage steps do for
//! the home page (`HomePublisher`).

use chrono::Utc;
use news_chief::BoxFut;
use news_core::{NewsError, NewsResult};
use news_press::store::Drained;
use news_proof::axe::BunAxe;
use news_proof::bundle::Bundle;
use news_proof::checks::{self, AxeMode, Duplicate, Env};
use news_proof::lexicon::Lexicon;
use news_proof::links::{CachedLinks, HttpProbe};
use news_proof::plate::Plate;
use news_proof::render as bundle;
use news_proof::validate::validate;
use news_proof::work::Work;
use news_store::{PgPool, map_err};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

/// A rendered page folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    pub bundle_sha: String,
    pub renderer_version: String,
}

/// Proof's verdict on a page. `Pass` carries the `bundle_sha256` of the bytes it checked (register
/// takes no other); `Fail` lists `{check, where, message}` for each blocking failure.
#[derive(Debug, Clone, PartialEq)]
pub enum Check {
    Pass(String),
    Fail(Vec<Value>),
}

pub trait PageTools: Send + Sync {
    /// `news render`: the story's page folder becomes a bundle. A Copy the validators refuse is an
    /// error with class `validation_failed`, never a bundle.
    fn render<'a>(&'a self, work: &'a Work, story: &'a str) -> BoxFut<'a, NewsResult<Rendered>>;
    /// `news check`: C1 to C14 on the bundle in the story's page folder.
    fn check<'a>(
        &'a self,
        db: &'a PgPool,
        work: &'a Work,
        story: &'a str,
    ) -> BoxFut<'a, NewsResult<Check>>;
}

/// Sends the outbox rows of one run.
pub trait PagePublisher: Send + Sync {
    fn drain<'a>(
        &'a self,
        db: &'a PgPool,
        work: &'a Path,
        run: Uuid,
    ) -> BoxFut<'a, NewsResult<Drained>>;
}

/// The write API over HTTPS (`NEWS_API_ORIGIN`); the key is read when the client is built, inside Press.
pub struct HttpPress;

impl PagePublisher for HttpPress {
    fn drain<'a>(
        &'a self,
        db: &'a PgPool,
        work: &'a Path,
        run: Uuid,
    ) -> BoxFut<'a, NewsResult<Drained>> {
        Box::pin(async move {
            let origin = std::env::var(news_press::api::ORIGIN_VAR).map_err(|_| {
                NewsError::precondition(format!("{} is not set", news_press::api::ORIGIN_VAR))
                    .with_hint("the write API's origin, for example https://api.example.com")
            })?;
            let api = news_press::api::ReqwestApi::from_credentials(&origin)?;
            let client =
                news_press::api::Client::new(Arc::new(api), Arc::new(news_wire::pace::SystemClock));
            news_press::store::drain(db, &client, work, Some(run)).await
        })
    }
}

/// Plate and Proof on this machine.
pub struct LocalTools {
    /// The site folder, where axe runs (`NEWS_SITE_DIR`).
    pub site: PathBuf,
}

fn off(var: &str) -> bool {
    matches!(std::env::var(var).as_deref(), Ok("off" | "0" | "false"))
}

impl PageTools for LocalTools {
    fn render<'a>(&'a self, work: &'a Work, story: &'a str) -> BoxFut<'a, NewsResult<Rendered>> {
        Box::pin(async move {
            let inputs = bundle::load_inputs(work, story)?;
            let plate = Plate::connect().await?;
            let checked = plate.check(&inputs.copy.oui).await?;
            let errors = validate(&inputs.stored.copy, Some(&inputs.brief), Some(&checked));
            if let Some(e) = errors.first() {
                return Err(NewsError::validation(format!(
                    "the Copy fails {} validator check(s): {} at {}: {}",
                    errors.len(),
                    e.rule,
                    e.at,
                    e.message
                )));
            }
            let rendered = plate.render(&inputs.render_request()).await?;
            let built = bundle::assemble(&inputs, &rendered)?;
            bundle::write(&built, &work.page(story))?;
            Ok(Rendered {
                bundle_sha: built.sha256,
                renderer_version: rendered.renderer_version,
            })
        })
    }

    fn check<'a>(
        &'a self,
        db: &'a PgPool,
        work: &'a Work,
        story: &'a str,
    ) -> BoxFut<'a, NewsResult<Check>> {
        Box::pin(async move {
            let dir = work.page(story);
            let b = Bundle::load(&dir);
            let checked = b.bundle_sha256();
            let page_id = b.page_id();
            let story_id: Option<i64> = story.parse().ok();
            let slugs: Vec<String> = {
                let path = b.meta_str("path");
                [path, path.rsplit('/').next().unwrap_or(path)]
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect()
            };
            // C14: another ready or published page with this slug, or one for the same story.
            let rows: Vec<(String, String, Option<i64>)> = sqlx::query_as(
                "select page_id, slug, story_id from pages
                  where page_id <> $1 and status in ('ready', 'published') and (slug = any($2) or story_id = $3)",
            )
            .bind(&page_id)
            .bind(&slugs)
            .bind(story_id)
            .fetch_all(db)
            .await
            .map_err(map_err)?;
            let dups: Vec<Duplicate> = rows
                .into_iter()
                .map(|(page_id, slug, sid)| Duplicate {
                    reason: if story_id.is_some() && sid == story_id {
                        "event key (the story already has a page)"
                    } else {
                        "slug"
                    }
                    .into(),
                    page_id,
                    slug,
                })
                .collect();
            let keys: Vec<(i64, i64)> = b
                .data_map("quotes")
                .into_iter()
                .flatten()
                .filter_map(|(_, q)| {
                    Some((
                        q["locator"]["doc"].as_i64()?,
                        q["locator"]["block"].as_i64()?,
                    ))
                })
                .collect();
            let blocks: BTreeMap<(i64, i64), String> = if keys.is_empty() {
                BTreeMap::new()
            } else {
                let docs: Vec<i64> = keys.iter().map(|k| k.0).collect();
                let nos: Vec<i32> = keys
                    .iter()
                    .filter_map(|k| i32::try_from(k.1).ok())
                    .collect();
                let rows: Vec<(i64, i32, String)> = sqlx::query_as(
                    "select t.doc_id, t.block_no, t.text from text_blocks t
                     join unnest($1::bigint[], $2::int[]) as k(doc_id, block_no) using (doc_id, block_no)",
                )
                .bind(&docs)
                .bind(&nos)
                .fetch_all(db)
                .await
                .map_err(map_err)?;
                rows.into_iter()
                    .map(|(d, n, t)| ((d, i64::from(n)), t))
                    .collect()
            };
            let lexicon = Lexicon::v1().map_err(NewsError::internal)?;
            let cache = work.page("..").join("cache").join("proof-links.json");
            let site = self.site.clone();
            let markup = tokio::task::spawn_blocking(move || {
                news_wire::runner::install_tls_provider();
                let links = if off("NEWS_PROOF_LINKS") {
                    None
                } else {
                    HttpProbe::new().ok().map(|p| {
                        CachedLinks::new(p, Some(cache), lexicon.c10.cache_hours, Utc::now())
                    })
                };
                let env = Env {
                    lexicon: &lexicon,
                    text_blocks: &blocks,
                    duplicates: Some(&dups),
                    links: links
                        .as_ref()
                        .map(|l| l as &dyn news_proof::links::LinkChecker),
                    axe: &BunAxe { site_dir: site },
                    axe_mode: if off("NEWS_PROOF_AXE") {
                        AxeMode::Off
                    } else {
                        AxeMode::Auto
                    },
                };
                let markup = checks::run(&b, &env);
                if let Some(l) = &links {
                    let _ = l.save();
                }
                markup
            })
            .await
            .map_err(|e| NewsError::internal(e.to_string()))?;
            if markup.pass {
                return Ok(Check::Pass(checked));
            }
            let v =
                serde_json::to_value(&markup).map_err(|e| NewsError::internal(e.to_string()))?;
            Ok(Check::Fail(
                v["failures"].as_array().cloned().unwrap_or_default(),
            ))
        })
    }
}
