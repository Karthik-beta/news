//! `news stories show|members|similar`: Taster's read-only tools (Bureau §6.4). `merge` and `split`
//! are H-tier: Board sends them over the Hotline (ADR 0007), so they have no command form here.

use super::{Reply, database};
use crate::cli::StoriesCmd;
use news_core::{NewsError, NewsResult};
use news_taster::read;
use serde_json::json;

pub async fn run(cmd: StoriesCmd) -> NewsResult<Reply> {
    let (db, _) = database(2).await?;
    match cmd {
        StoriesCmd::Show { story } => read::show(&db, &story).await.map(Reply::new),
        StoriesCmd::Members { story } => read::members(&db, &story).await.map(Reply::new),
        StoriesCmd::Similar {
            text: Some(text), ..
        } => read::similar_text(&db, &text).await.map(Reply::new),
        StoriesCmd::Similar {
            cand: Some(cand), ..
        } => match read::similar_cand(&db, &cand).await? {
            Some(items) => Ok(Reply::new(items)),
            None => {
                let mut reply = Reply::new(json!({"items": []}));
                reply
                    .warnings
                    .push(format!("candidate {cand} has no document text to compare"));
                Ok(reply)
            }
        },
        // clap requires one of the two.
        StoriesCmd::Similar { .. } => Err(NewsError::usage("give --text or --cand")),
    }
}
