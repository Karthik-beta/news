//! The embedding joiner (Bureau §3.6 step 4, ADR 0011).
//!
//! A candidate without an event key is embedded (title, a blank line, the first paragraph), its vector
//! stored as `embeddings(owner_kind = 'candidate')`, and the nearest story centroids are read back with
//! pgvector. When detection puts the candidate in a story the story's centroid, the mean of its
//! members' vectors (`owner_kind = 'story'`), is recomputed.
//!
//! The model sits behind [`Embedder`]. [`FastEmbedder`] (cargo feature `embed`, off by default) is
//! bge-small-en-v1.5 fp32 on ONNX Runtime with 2 intra-op threads; without the feature nothing
//! downloads or links the runtime and the service uses [`NoEmbedder`](crate::detect::NoEmbedder).
//! Tests give [`EmbedJoiner`] an embedder that returns stored vectors.
//!
//! The embedder is synchronous, about 0.2 s a document: run detection on a multi-thread runtime.

use sqlx::PgConnection;

use crate::config::Thresholds;
use crate::detect::{Cand, Joiner, Neighbour};

/// The `embeddings.model` value (ADR 0011).
pub const MODEL: &str = "bge-small-en-v1.5";
/// The width of `embeddings.v`.
pub const DIM: usize = 384;
/// Stories returned per candidate; `detect::decide` uses the closest.
const NEAREST: i32 = 8;

/// Text in, a [`DIM`]-wide vector out.
pub trait Embedder: Send + Sync {
    fn embed(&self, text: &str) -> Result<Vec<f32>, String>;
}

/// bge-small-en-v1.5 (fp32) through fastembed, loaded once.
#[cfg(feature = "embed")]
pub struct FastEmbedder(std::sync::Mutex<fastembed::TextEmbedding>);

#[cfg(feature = "embed")]
impl FastEmbedder {
    /// Load the model from `cache_dir`, downloading it there on first use. The directory must be
    /// outside the repository (the service's data directory); weights are never committed.
    pub fn new(cache_dir: impl Into<std::path::PathBuf>) -> Result<Self, String> {
        let opts = fastembed::TextInitOptions::new(fastembed::EmbeddingModel::BGESmallENV15)
            .with_cache_dir(cache_dir.into())
            .with_intra_threads(2);
        let model = fastembed::TextEmbedding::try_new(opts).map_err(|e| e.to_string())?;
        Ok(Self(std::sync::Mutex::new(model)))
    }
}

#[cfg(feature = "embed")]
impl Embedder for FastEmbedder {
    fn embed(&self, text: &str) -> Result<Vec<f32>, String> {
        let mut model = self.0.lock().map_err(|_| "embedder lock poisoned")?;
        let mut out = model.embed([text], None).map_err(|e| e.to_string())?;
        out.pop().ok_or_else(|| "no embedding returned".to_owned())
    }
}

/// The text that is embedded: the title, a blank line, the first paragraph. `None` when both are empty.
/// The model cuts it at its 512 tokens.
pub fn text(title: Option<&str>, paragraph: Option<&str>) -> Option<String> {
    fn part(s: Option<&str>) -> Option<&str> {
        s.map(str::trim).filter(|s| !s.is_empty())
    }
    match (part(title), part(paragraph)) {
        (Some(t), Some(p)) => Some(format!("{t}\n\n{p}")),
        (Some(s), None) | (None, Some(s)) => Some(s.to_owned()),
        (None, None) => None,
    }
}

/// A pgvector text literal of `v` scaled to unit length, so a centroid (a mean) stays comparable.
fn literal(v: &[f32]) -> Result<String, sqlx::Error> {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if v.len() != DIM || !norm.is_finite() || norm == 0.0 {
        return Err(sqlx::Error::Protocol(format!(
            "embedding must be {DIM} finite numbers, not all zero (got {})",
            v.len()
        )));
    }
    let parts: Vec<String> = v.iter().map(|x| (x / norm).to_string()).collect();
    Ok(format!("[{}]", parts.join(",")))
}

/// Finds neighbours by embedding the candidate and asking pgvector for the nearest story centroids.
pub struct EmbedJoiner<E> {
    embedder: E,
}

impl<E: Embedder> EmbedJoiner<E> {
    pub fn new(embedder: E) -> Self {
        Self { embedder }
    }
}

impl<E: Embedder> Joiner for EmbedJoiner<E> {
    async fn neighbours(
        &self,
        db: &mut PgConnection,
        cand: &Cand,
        t: &Thresholds,
    ) -> Result<Vec<Neighbour>, sqlx::Error> {
        // shortcut: the first block of the document that is not a heading stands for its first paragraph;
        // revisit when Clerk's block kinds settle.
        let paragraph: Option<String> = sqlx::query_scalar(
            "select tb.text from candidates c join text_blocks tb on tb.doc_id = c.doc_ids[1]
              where c.cand_id = $1 and tb.kind is distinct from 'heading' and btrim(tb.text) <> ''
              order by tb.block_no limit 1",
        )
        .bind(cand.cand_id)
        .fetch_optional(&mut *db)
        .await?;
        let Some(text) = text(cand.title.as_deref(), paragraph.as_deref()) else {
            return Ok(Vec::new());
        };
        let v = literal(
            &self
                .embedder
                .embed(&text)
                .map_err(|e| sqlx::Error::Protocol(format!("embedding failed: {e}")))?,
        )?;
        sqlx::query(
            "insert into embeddings (owner_kind, owner_id, model, v)
             values ('candidate', $1::bigint::text, $2, $3::text::halfvec)
             on conflict (owner_kind, owner_id, model) do update set v = excluded.v, created_at = now()",
        )
        .bind(cand.cand_id)
        .bind(MODEL)
        .bind(&v)
        .execute(&mut *db)
        .await?;
        // The HNSW index also holds candidate and page vectors; iterative scans keep the owner and
        // window filters below from starving the result.
        sqlx::query("select set_config('hnsw.iterative_scan', 'relaxed_order', true)")
            .execute(&mut *db)
            .await?;
        // A story is a neighbour while it was active in the last `window_hours`, or for `page_days` after
        // its page was published. ADR 0011 decision 7: a candidate with an event key is never matched by
        // cosine to a story that has one (only key equality joins those); a candidate without a key is
        // matched to any.
        let rows: Vec<(i64, f64, bool, bool)> = sqlx::query_as(
            "select s.story_id, (1 - (e.v <=> $1::text::halfvec))::float8,
                    s.entity_ids && $2::bigint[],
                    exists (select 1 from pages p where p.story_id = s.story_id and p.status = 'published')
               from embeddings e join stories s on e.owner_id = s.story_id::text
              where e.owner_kind = 'story' and e.model = $3
                and (not $4 or s.event_key is null)
                and (s.last_seen >= $5 - make_interval(hours => $6)
                     or exists (select 1 from pages p where p.story_id = s.story_id and p.status = 'published'
                                   and p.published_at >= $5 - make_interval(days => $7)))
              order by e.v <=> $1::text::halfvec limit $8",
        )
        .bind(&v)
        .bind(&cand.entity_ids)
        .bind(MODEL)
        .bind(cand.event_key.is_some())
        .bind(cand.first_seen)
        .bind(t.window_hours)
        .bind(t.page_days)
        .bind(i64::from(NEAREST))
        .fetch_all(&mut *db)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(story_id, cos, shared_entity, has_page)| Neighbour {
                story_id,
                cos,
                shared_entity,
                has_page,
            })
            .collect())
    }

    /// The story's centroid is the mean of its candidates' vectors.
    async fn assigned(
        &self,
        db: &mut PgConnection,
        _: &Cand,
        story_id: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "insert into embeddings (owner_kind, owner_id, model, v)
             select 'story', $1::bigint::text, $2::text, avg(e.v)
               from candidates c join embeddings e
                 on e.owner_kind = 'candidate' and e.owner_id = c.cand_id::text and e.model = $2
              where c.story_id = $1 having count(*) > 0
             on conflict (owner_kind, owner_id, model) do update set v = excluded.v, created_at = now()",
        )
        .bind(story_id)
        .bind(MODEL)
        .execute(db)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_text_is_title_then_first_paragraph() {
        assert_eq!(
            text(Some(" Fed holds "), Some("Rates unchanged.\n")).as_deref(),
            Some("Fed holds\n\nRates unchanged.")
        );
        assert_eq!(text(Some("Fed holds"), None).as_deref(), Some("Fed holds"));
        assert_eq!(text(None, Some(" ")), None);
    }

    #[test]
    fn a_vector_is_scaled_to_unit_length_and_must_fit_the_column() {
        let mut v = vec![0.0; DIM];
        v[0] = 3.0;
        v[1] = 4.0;
        let l = literal(&v).unwrap_or_default();
        assert!(l.starts_with("[0.6,0.8,0,"), "{l}");
        assert!(literal(&[1.0; 3]).is_err());
        assert!(literal(&[0.0; DIM]).is_err());
        assert!(literal(&[f32::NAN; DIM]).is_err());
    }
}
