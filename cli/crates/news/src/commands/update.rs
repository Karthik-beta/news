//! `news update <page>` (R tier, Bureau §3.11, §6.4): load a published page for revision. Read only:
//! it finds the page's superseded data points, classifies the revision and says which refs changed.
//! Re-rendering, queuing the update job and publishing are Followup's steps (and Edition's step 10).

use super::{Reply, database};
use news_core::{NewsError, NewsResult};
use news_rewrite::{find, plan, update};

/// A page ID, or the page's path (`/2026/11/us-cpi-october-2026`), whose last segment is the ID.
fn page_id(arg: &str) -> NewsResult<&str> {
    let id = arg.trim_end_matches('/').rsplit('/').next().unwrap_or("");
    if news_press::bundle::is_page_id(id) {
        Ok(id)
    } else {
        Err(NewsError::usage(format!(
            "{arg:?} is not a page ID: use lower-case letters, digits and hyphens"
        )))
    }
}

pub async fn run(page: &str) -> NewsResult<Reply> {
    let id = page_id(page)?;
    let (db, _) = database(2).await?;
    let version: i32 =
        sqlx::query_scalar("select version from pages where page_id = $1 and status = 'published'")
            .bind(id)
            .fetch_optional(&db)
            .await
            .map_err(news_store::map_err)?
            .ok_or_else(|| NewsError::not_found(format!("no published page {id}")))?;
    let change = find::for_page(&db, id).await?;
    let planned = change.map(|c| plan::plan(vec![c]).remove(0));
    Ok(Reply::new(update::show(planned.as_ref(), id, version)))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn the_page_is_an_id_or_a_path_to_one() {
        assert_eq!(
            page_id("us-cpi-october-2026").unwrap(),
            "us-cpi-october-2026"
        );
        assert_eq!(
            page_id("/2026/11/us-cpi-october-2026/").unwrap(),
            "us-cpi-october-2026"
        );
        for bad in ["", "..", "A", "a b", "a:b", r"x\y"] {
            assert!(page_id(bad).is_err(), "{bad:?}");
        }
    }
}
