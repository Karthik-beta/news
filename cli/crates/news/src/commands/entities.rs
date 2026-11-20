//! `news entities …`: Roster's read-only tools (Bureau §6.4). `alias add` and `merge` are H-tier:
//! Board sends them over the Hotline (ADR 0007), so they have no command form here.

use std::fmt::Write as _;

use serde_json::json;

use super::{Reply, database};
use crate::cli::EntitiesCmd;
use news_core::{NewsError, NewsResult};
use news_roster::{resolver::Step, seed, store};

fn db_err(e: sqlx::Error) -> NewsError {
    news_store::map_err(e)
}

pub async fn run(cmd: EntitiesCmd) -> NewsResult<Reply> {
    // Validate arguments before touching the database.
    if let EntitiesCmd::Resolve { text, kind } = &cmd {
        if let Some(k) = kind
            && !seed::KINDS.contains(&k.as_str())
        {
            return Err(NewsError::usage(format!(
                "--kind {k:?} is not one of {}",
                seed::KINDS.join(", ")
            )));
        }
        if text.trim().is_empty() {
            return Err(NewsError::usage("the mention is empty"));
        }
    }
    let (db, _) = database(2).await?;
    match cmd {
        EntitiesCmd::Resolve { text, kind } => {
            let r = store::resolve(&db, &text, kind.as_deref())
                .await
                .map_err(db_err)?;
            let step = match r.step {
                Step::Identifier => "identifier",
                Step::Alias => "alias",
                Step::Gazetteer => "gazetteer",
                Step::Fuzzy => "fuzzy",
                Step::Unresolved => "unresolved",
            };
            let text_out = match (&r.name, r.reason) {
                (Some(name), _) => format!("{name} ({step})"),
                (None, Some(reason)) => format!("unresolved ({})", reason.as_str()),
                (None, None) => "unresolved".into(),
            };
            Ok(Reply::new(json!({
                "entity_id": r.entity_id,
                "name": r.name,
                "kind": r.kind,
                "step": step,
                "score": r.score,
                "text": text_out,
            })))
        }
        EntitiesCmd::Show { entity } => match store::show(&db, &entity).await.map_err(db_err)? {
            Some(s) => {
                let mut text = format!("{} ({}) #{}\n", s.name, s.kind, s.entity_id);
                for i in &s.ids {
                    let _ = writeln!(text, "  {}:{}", i.scheme, i.value);
                }
                Ok(Reply::new(json!({
                    "entity_id": s.entity_id,
                    "kind": s.kind,
                    "name": s.name,
                    "ids": s.ids,
                    "aliases": s.aliases,
                    "text": text.trim_end(),
                })))
            }
            None => Err(NewsError::not_found(format!("no entity {entity:?}"))
                .with_hint("use an entity number or scheme:value, for example cik:320193")),
        },
        EntitiesCmd::Unresolved => {
            let rows = store::unresolved(&db, 500).await.map_err(db_err)?;
            let mut text = String::new();
            for r in &rows {
                let _ = writeln!(text, "{:>4}  {}  ({})", r.count, r.mention, r.sample);
            }
            Ok(Reply::new(
                json!({ "items": rows, "text": text.trim_end() }),
            ))
        }
    }
}
