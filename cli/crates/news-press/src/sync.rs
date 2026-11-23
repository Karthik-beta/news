//! Taxonomy and hub sync to the api (`PUT /v1/taxonomy`, `PUT /v1/redirects`).
//!
//! Categories are data, not code: the `taxonomy` table is the source and the API's `TAX#` nodes the copy.
//! A category's hub (`cat/<slug>`, with its parents) exists as soon as a page lists it, and entity
//! records and their hubs (`ent/<id>`) are written from each page's `meta.json` when it is published, so
//! neither has a route of its own. A renamed hub is a new node plus a redirect from the old path.
//! Keys are the hash of the body: an unchanged taxonomy sends the same key and writes nothing.

use news_chief::gates::hex;
use news_core::NewsResult;
use news_store::{PgPool, map_err};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::Row;

use crate::api::{ApiRequest, Client, Method};

/// The API takes at most 100 nodes or redirects a call.
const CHUNK: usize = 100;

/// Every category as an API node, parents before children. `id` and `parent` are slug paths
/// (`economy/inflation`); `lang` is `en` until a second language exists.
pub async fn taxonomy_nodes(db: &PgPool) -> NewsResult<Vec<Value>> {
    let rows = sqlx::query(
        "with recursive t as (
           select tax_id, slug::text as path, null::text as parent, name, sort_order
             from taxonomy where parent_id is null
           union all
           select c.tax_id, t.path || '/' || c.slug, t.path, c.name, c.sort_order
             from taxonomy c join t on c.parent_id = t.tax_id)
         select path, parent, name, sort_order from t order by path",
    )
    .fetch_all(db)
    .await
    .map_err(map_err)?;
    Ok(rows
        .iter()
        .map(|r| {
            json!({"id": r.get::<String, _>("path"), "name": r.get::<String, _>("name"),
                   "parent": r.get::<Option<String>, _>("parent"),
                   "order": r.get::<i32, _>("sort_order"), "lang": "en"})
        })
        .collect())
}

fn request(path: &str, field: &str, items: &[Value]) -> ApiRequest {
    let body = json!({ field: items }).to_string().into_bytes();
    ApiRequest {
        method: Method::Put,
        path: path.to_owned(),
        idempotency_key: format!("{}:{}", &path[4..], hex(&Sha256::digest(&body))),
        body,
    }
}

async fn put(client: &Client, path: &str, field: &str, items: &[Value]) -> NewsResult<i64> {
    let mut written = 0;
    for chunk in items.chunks(CHUNK) {
        let sent = client.send(&request(path, field, chunk)).await;
        let answer = sent.result.map_err(|f| f.error())?;
        written += answer["written"].as_i64().unwrap_or(0);
    }
    Ok(written)
}

/// Send the taxonomy; returns how many nodes the API wrote.
pub async fn sync_taxonomy(db: &PgPool, client: &Client) -> NewsResult<i64> {
    put(client, "/v1/taxonomy", "nodes", &taxonomy_nodes(db).await?).await
}

/// Send redirects (`{from, to, status}` with a 301 or 308); returns how many the API wrote.
pub async fn sync_redirects(client: &Client, redirects: &[Value]) -> NewsResult<i64> {
    put(client, "/v1/redirects", "redirects", redirects).await
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn the_key_is_the_body_hash_and_fits_the_api_pattern() {
        let a = request("/v1/taxonomy", "nodes", &[json!({"id": "economy"})]);
        let b = request("/v1/taxonomy", "nodes", &[json!({"id": "economy"})]);
        let c = request("/v1/taxonomy", "nodes", &[json!({"id": "science"})]);
        assert_eq!(a.idempotency_key, b.idempotency_key);
        assert_ne!(a.idempotency_key, c.idempotency_key);
        assert!(a.idempotency_key.starts_with("taxonomy:") && a.idempotency_key.len() == 9 + 64);
        assert_eq!(
            request("/v1/redirects", "redirects", &[]).idempotency_key[..10],
            *"redirects:"
        );
    }
}
