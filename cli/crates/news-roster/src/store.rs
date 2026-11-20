//! The PostgreSQL side of Roster: applying seeds, fetching what the resolver needs for one text, the
//! unresolved queue, `entities show`, and the H-tier `alias add` / `merge` (called by Board over the
//! Hotline, never by a shell command).

use std::collections::BTreeSet;

use serde::Serialize;
use sqlx::{PgPool, Row};

use crate::norm::{normalize, tokens};
use crate::resolver::{Entity, Index, Resolution, canonical_id, resolve as resolve_in};
use crate::seed::{Prominence, Seed};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ApplyReport {
    pub created: usize,
    pub matched: usize,
    pub ids_added: usize,
    pub aliases_added: usize,
}

async fn entity_by_ids(pool: &PgPool, seed: &Seed) -> Result<Option<i64>, sqlx::Error> {
    for p in &seed.ids {
        let found: Option<i64> = sqlx::query_scalar(
            "select entity_id from entity_ids where scheme = $1 and value = $2 and valid_to is null
              order by valid_from desc limit 1",
        )
        .bind(&p.scheme)
        .bind(canonical_id(&p.scheme, &p.value))
        .fetch_optional(pool)
        .await?;
        if found.is_some() {
            return Ok(found);
        }
    }
    Ok(None)
}

/// Create or complete the entities of `seeds`. Idempotent: a second run reports every entity as
/// matched and adds nothing. `source` labels the aliases it adds (`sec`, `gleif`, `seed`).
pub async fn apply(
    pool: &PgPool,
    seeds: &[Seed],
    source: &str,
) -> Result<ApplyReport, sqlx::Error> {
    let mut r = ApplyReport::default();
    for s in seeds {
        let id = match entity_by_ids(pool, s).await? {
            Some(id) => {
                r.matched += 1;
                // Complete, never overwrite: region only when empty, prominence only when seeded.
                sqlx::query(
                    "update entities set region = coalesce(region, $2), prominence = coalesce($3, prominence)
                      where entity_id = $1",
                )
                .bind(id)
                .bind(&s.region)
                .bind(s.prominence)
                .execute(pool)
                .await?;
                id
            }
            None => {
                r.created += 1;
                sqlx::query_scalar(
                    "insert into entities (kind, name, region, prominence) values ($1, $2, $3, coalesce($4, 3))
                     returning entity_id",
                )
                .bind(&s.kind)
                .bind(&s.name)
                .bind(&s.region)
                .bind(s.prominence)
                .fetch_one(pool)
                .await?
            }
        };
        for p in &s.ids {
            let n = sqlx::query(
                "insert into entity_ids (scheme, value, entity_id) values ($1, $2, $3)
                 on conflict (scheme, value, valid_from) do nothing",
            )
            .bind(&p.scheme)
            .bind(canonical_id(&p.scheme, &p.value))
            .bind(id)
            .execute(pool)
            .await?
            .rows_affected();
            r.ids_added += usize::try_from(n).unwrap_or(0);
        }
        for a in std::iter::once(&s.name).chain(s.aliases.iter()) {
            r.aliases_added += usize::from(add_alias(pool, id, a, source).await?);
        }
    }
    Ok(r)
}

/// Set prominence for entities named by identifier (`data/roster/prominence.toml`). Returns how many
/// entities were found.
pub async fn apply_prominence(pool: &PgPool, list: &[Prominence]) -> Result<usize, sqlx::Error> {
    let mut found = 0;
    for p in list {
        let n = sqlx::query(
            "update entities set prominence = $3
              where entity_id = (select entity_id from entity_ids where scheme = $1 and value = $2
                                  and valid_to is null limit 1)",
        )
        .bind(&p.scheme)
        .bind(canonical_id(&p.scheme, &p.value))
        .bind(p.prominence)
        .execute(pool)
        .await?
        .rows_affected();
        found += usize::try_from(n).unwrap_or(0);
    }
    Ok(found)
}

/// H-tier `alias add`: one more way to name an entity. Returns false when it was already there.
pub async fn add_alias(
    pool: &PgPool,
    entity_id: i64,
    alias: &str,
    source: &str,
) -> Result<bool, sqlx::Error> {
    let norm = normalize(alias);
    if norm.is_empty() {
        return Ok(false);
    }
    let n = sqlx::query(
        "insert into entity_aliases (alias_norm, entity_id, source) values ($1, $2, $3)
         on conflict (alias_norm, entity_id) do nothing",
    )
    .bind(norm)
    .bind(entity_id)
    .bind(source)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(n > 0)
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct MergeReport {
    pub ids_moved: u64,
    pub aliases_moved: u64,
    pub edges_moved: u64,
    pub data_points_moved: u64,
}

/// Every table column that references `entities`. A test compares this list with the catalogue, so a
/// later migration that adds a reference cannot be forgotten by `merge`.
pub const REFERENCES: &[(&str, &str)] = &[
    ("data_points", "entity_id"),
    ("edges", "dst"),
    ("edges", "src"),
    ("entity_aliases", "entity_id"),
    ("entity_ids", "entity_id"),
    ("unresolved_mentions", "resolved_entity"),
];

/// H-tier `merge`: fold entity `from` into `into` (same kind), moving identifiers, aliases, edges,
/// data points and the unresolved queue's links, then delete `from`. One transaction.
pub async fn merge(pool: &PgPool, from: i64, into: i64) -> Result<MergeReport, sqlx::Error> {
    if from == into {
        return Err(sqlx::Error::Protocol(
            "cannot merge an entity into itself".into(),
        ));
    }
    let mut tx = pool.begin().await?;
    let kinds: Vec<(i64, String)> =
        sqlx::query_as("select entity_id, kind from entities where entity_id = any ($1)")
            .bind(vec![from, into])
            .fetch_all(&mut *tx)
            .await?;
    if kinds.len() != 2 || kinds[0].1 != kinds[1].1 {
        return Err(sqlx::Error::Protocol(
            "both entities must exist and have the same kind".into(),
        ));
    }

    let ids_moved = sqlx::query("update entity_ids set entity_id = $2 where entity_id = $1")
        .bind(from)
        .bind(into)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    let aliases_moved = sqlx::query(
        "insert into entity_aliases (alias_norm, entity_id, source, weight)
         select alias_norm, $2, source, weight from entity_aliases where entity_id = $1
         on conflict (alias_norm, entity_id) do nothing",
    )
    .bind(from)
    .bind(into)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    sqlx::query("delete from entity_aliases where entity_id = $1")
        .bind(from)
        .execute(&mut *tx)
        .await?;
    let edges_moved = sqlx::query(
        "insert into edges (src, rel, dst, weight, since)
         select case when src = $1 then $2 else src end, rel, case when dst = $1 then $2 else dst end, weight, since
           from edges where src = $1 or dst = $1
         on conflict (src, rel, dst) do nothing",
    )
    .bind(from)
    .bind(into)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    sqlx::query("delete from edges where src = $1 or dst = $1")
        .bind(from)
        .execute(&mut *tx)
        .await?;
    // A merge can leave a self-edge behind (A→B merged into one entity): drop it.
    sqlx::query("delete from edges where src = dst")
        .execute(&mut *tx)
        .await?;
    let data_points_moved =
        sqlx::query("update data_points set entity_id = $2 where entity_id = $1")
            .bind(from)
            .bind(into)
            .execute(&mut *tx)
            .await?
            .rows_affected();
    sqlx::query("update unresolved_mentions set resolved_entity = $2 where resolved_entity = $1")
        .bind(from)
        .bind(into)
        .execute(&mut *tx)
        .await?;
    sqlx::query("delete from entities where entity_id = $1")
        .bind(from)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(MergeReport {
        ids_moved,
        aliases_moved,
        edges_moved,
        data_points_moved,
    })
}

/// Fetch just the rows the resolver could use for `text`: the identifier, aliases equal to the text
/// or to any of its word spans, and the trigram neighbours. The same [`resolve_in`] then decides.
pub async fn index_for(
    pool: &PgPool,
    text: &str,
    kind: Option<&str>,
) -> Result<Index, sqlx::Error> {
    let mut ix = Index::default();
    let mut entity_ids: BTreeSet<i64> = BTreeSet::new();
    let t = text.trim();
    if let Some((scheme, value)) = t.split_once(':') {
        let rows = sqlx::query(
            "select entity_id from entity_ids where scheme = $1 and value = $2 and valid_to is null",
        )
        .bind(scheme)
        .bind(canonical_id(scheme, value))
        .fetch_all(pool)
        .await?;
        for row in rows {
            let id: i64 = row.try_get("entity_id")?;
            ix.add_id(id, scheme, value);
            entity_ids.insert(id);
        }
    }
    let norm = normalize(text);
    let toks = tokens(text);
    let mut phrases: BTreeSet<String> = BTreeSet::new();
    phrases.insert(norm.clone());
    for start in 0..toks.len() {
        for len in 1..=6.min(toks.len() - start) {
            phrases.insert(toks[start..start + len].join(" "));
        }
    }
    let phrases: Vec<String> = phrases.into_iter().filter(|p| !p.is_empty()).collect();
    let mut alias_rows: Vec<(String, i64)> = sqlx::query_as(
        "select alias_norm, entity_id from entity_aliases where alias_norm = any ($1)",
    )
    .bind(&phrases)
    .fetch_all(pool)
    .await?;
    if !norm.is_empty() {
        let near: Vec<(String, i64)> = sqlx::query_as(
            "select alias_norm, entity_id from entity_aliases where alias_norm % $1
              order by similarity(alias_norm, $1) desc, entity_id limit 40",
        )
        .bind(&norm)
        .fetch_all(pool)
        .await?;
        alias_rows.extend(near);
    }
    for (_, id) in &alias_rows {
        entity_ids.insert(*id);
    }
    let ids: Vec<i64> = entity_ids.into_iter().collect();
    let ents = sqlx::query(
        "select entity_id, kind, name, prominence from entities where entity_id = any ($1)",
    )
    .bind(&ids)
    .fetch_all(pool)
    .await?;
    for e in ents {
        let (id, k): (i64, String) = (e.try_get("entity_id")?, e.try_get("kind")?);
        if kind.is_none_or(|want| want == k) {
            ix.add_entity(Entity {
                id,
                kind: k,
                name: e.try_get("name")?,
                prominence: e.try_get("prominence")?,
            });
        }
    }
    for (norm, id) in alias_rows {
        ix.add_alias_norm(id, norm);
    }
    Ok(ix)
}

/// Resolve `text` against the database.
pub async fn resolve(
    pool: &PgPool,
    text: &str,
    kind: Option<&str>,
) -> Result<Resolution, sqlx::Error> {
    let ix = index_for(pool, text, kind).await?;
    Ok(resolve_in(&ix, text, kind))
}

/// Put an unresolved mention in the queue (or count one more sighting of it).
pub async fn record_unresolved(
    pool: &PgPool,
    mention: &str,
    kind: Option<&str>,
    context: &str,
    res: &Resolution,
) -> Result<(), sqlx::Error> {
    let Some(reason) = res.reason else {
        return Ok(());
    };
    let norm = normalize(mention);
    if norm.is_empty() {
        return Ok(());
    }
    let candidates = serde_json::to_value(&res.candidates).unwrap_or_default();
    sqlx::query(
        "insert into unresolved_mentions (mention_norm, mention, kind, reason, candidates, context)
         values ($1, $2, $3, $4, $5, $6)
         on conflict (mention_norm, kind) do update set
           seen = unresolved_mentions.seen + 1, last_seen = now(), reason = excluded.reason,
           candidates = excluded.candidates",
    )
    .bind(norm)
    .bind(mention)
    .bind(kind)
    .bind(reason.as_str())
    .bind(candidates)
    .bind(context)
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct UnresolvedRow {
    pub mention: String,
    pub count: i32,
    pub sample: String,
}

/// Open unresolved mentions, most frequent first.
pub async fn unresolved(pool: &PgPool, limit: i64) -> Result<Vec<UnresolvedRow>, sqlx::Error> {
    sqlx::query_as(
        "select mention, seen as count, context as sample from unresolved_mentions
          where status = 'open' order by seen desc, mention_id limit $1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Shown {
    pub entity_id: i64,
    pub kind: String,
    pub name: String,
    pub ids: Vec<ShownId>,
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ShownId {
    pub scheme: String,
    pub value: String,
}

/// One entity by number or by identifier (`cik:320193`).
pub async fn show(pool: &PgPool, which: &str) -> Result<Option<Shown>, sqlx::Error> {
    let id: Option<i64> = if let Ok(n) = which.trim().parse::<i64>() {
        Some(n)
    } else if let Some((scheme, value)) = which.split_once(':') {
        sqlx::query_scalar(
            "select entity_id from entity_ids where scheme = $1 and value = $2 and valid_to is null limit 1",
        )
        .bind(scheme)
        .bind(canonical_id(scheme, value))
        .fetch_optional(pool)
        .await?
    } else {
        None
    };
    let Some(id) = id else { return Ok(None) };
    let Some(row) = sqlx::query("select entity_id, kind, name from entities where entity_id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?
    else {
        return Ok(None);
    };
    let ids: Vec<(String, String)> = sqlx::query_as(
        "select scheme, value from entity_ids where entity_id = $1 and valid_to is null order by scheme, value",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    let aliases: Vec<String> = sqlx::query_scalar(
        "select alias_norm from entity_aliases where entity_id = $1 order by alias_norm",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    Ok(Some(Shown {
        entity_id: row.try_get("entity_id")?,
        kind: row.try_get("kind")?,
        name: row.try_get("name")?,
        ids: ids
            .into_iter()
            .map(|(scheme, value)| ShownId { scheme, value })
            .collect(),
        aliases,
    }))
}
