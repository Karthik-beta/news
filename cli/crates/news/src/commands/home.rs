//! `news home candidates|save|check|preview|show` (Bureau §6.4; ADR 0005): the Frontpage preset's code
//! steps, in `news-home`. `publish` is G tier: its CLI form is `gated` (routed in `mod.rs`), and
//! `rollback` is H tier, Board's over the Hotline, with no command form.

use super::{Reply, database, read_json};
use crate::cli::HomeCmd;
use chrono::Utc;
use news_core::NewsResult;
use news_home::{candidates, ops, preview};

pub async fn run(cmd: HomeCmd) -> NewsResult<Reply> {
    match cmd {
        HomeCmd::Candidates => {
            let (db, _) = database(2).await?;
            let (data, warnings) = candidates::run(&db, Utc::now()).await?;
            let mut reply = Reply::new(data).default_limit(candidates::TOP as usize);
            reply.warnings = warnings;
            Ok(reply)
        }
        HomeCmd::Save { file } => {
            let draft = read_json(&file)?;
            let (db, _) = database(2).await?;
            ops::save(&db, draft).await.map(Reply::new)
        }
        HomeCmd::Check { draft } => {
            let id = ops::parse_id(&draft)?;
            let (db, _) = database(2).await?;
            ops::check(&db, id, Utc::now()).await.map(Reply::new)
        }
        HomeCmd::Preview { draft } => {
            let id = ops::parse_id(&draft)?;
            let (db, _) = database(2).await?;
            let base = std::env::var("NEWS_PREVIEW_URL")
                .unwrap_or_else(|_| preview::DEFAULT_URL.to_owned());
            ops::preview(&db, id, &base).await.map(Reply::new)
        }
        HomeCmd::Show => {
            let (db, _) = database(2).await?;
            ops::show(&db, Utc::now()).await.map(Reply::new)
        }
        // Routed to `gated` before this module; kept so the match stays exhaustive.
        HomeCmd::Publish { .. } => super::run::gated("home publish"),
    }
}
