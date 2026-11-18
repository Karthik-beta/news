//! NASA Image and Video Library (`images-api.nasa.gov`). No key.
//!
//! Imagery: each item becomes an [`ImageRef`] (URL, credit, licence, alt text). NASA's media
//! guidelines say its own content is generally not copyrighted but may include third-party
//! material, so the registry records the licence as `custom` and this parser sets `rehost` only for
//! items whose credit line is NASA's own (`NASA/…`); anything else, or an item with no credit, stays a link
//! to NASA's file. Alt text is the item's `description_508` (NASA's accessibility text), else its
//! title. **Not built**: NASA news releases (the library holds media, not releases).
//!
//! `Cx::tracked("nasa")` lists search terms. Discovery searches the current year per term and
//! emits items whose `nasa_id` is not in the cursor; fetch looks the item up by id.

use std::collections::BTreeSet;

use chrono::Datelike;
use serde::Deserialize;
use serde_json::json;

use crate::util::{day_vintage, find, shape};
use crate::{Connector, Cx, Discovery, ImageRef, ItemRef, ParseError, Parsed, Request, Snapshot};

pub const ID: &str = "nasa";
pub const PARSER_VERSION: u32 = 1;
const BASE: &str = "https://images-api.nasa.gov/search";
/// The registry's `licence` for NASA (see data/sources/nasa.toml).
const LICENCE: &str = "custom";

pub struct Nasa;

#[derive(Deserialize)]
struct Envelope {
    collection: Collection,
}

#[derive(Deserialize)]
struct Collection {
    items: Vec<Item>,
}

#[derive(Deserialize)]
struct Item {
    data: Vec<Data>,
    #[serde(default)]
    links: Vec<Link>,
}

#[derive(Deserialize)]
struct Data {
    nasa_id: String,
    title: String,
    #[serde(default)]
    description_508: Option<String>,
    #[serde(default)]
    date_created: Option<String>,
    #[serde(default)]
    photographer: Option<String>,
    #[serde(default)]
    media_type: String,
}

#[derive(Deserialize)]
struct Link {
    href: String,
    #[serde(default)]
    rel: String,
    #[serde(default)]
    render: String,
    #[serde(default)]
    width: Option<u32>,
    #[serde(default)]
    height: Option<u32>,
}

fn search_url(q: &str, year: i32) -> String {
    format!("{BASE}?q={q}&media_type=image&year_start={year}&page_size=20")
}

fn lookup_url(id: &str) -> String {
    format!("{BASE}?nasa_id={}", id.replace(' ', "%20"))
}

fn seen_set(c: Option<&str>) -> BTreeSet<String> {
    c.and_then(|c| serde_json::from_str(c).ok())
        .unwrap_or_default()
}

impl Connector for Nasa {
    fn id(&self) -> &'static str {
        ID
    }
    fn source_id(&self) -> &'static str {
        ID
    }
    fn parser_version(&self) -> u32 {
        PARSER_VERSION
    }

    fn discovery_requests(&self, cx: &Cx, _cursor: Option<&str>) -> Vec<Request> {
        cx.tracked(ID)
            .iter()
            .map(|q| Request::json(search_url(q, cx.today.year())))
            .collect()
    }

    fn parse_discovery(
        &self,
        cx: &Cx,
        cursor: Option<&str>,
        snaps: &[Snapshot],
    ) -> Result<Discovery, ParseError> {
        let mut seen = seen_set(cursor);
        let mut out = Discovery::default();
        for q in cx.tracked(ID) {
            let Some(snap) = snaps
                .iter()
                .find(|s| s.url == search_url(q, cx.today.year()))
            else {
                continue;
            };
            let env: Envelope =
                serde_json::from_slice(&snap.bytes).map_err(|e| shape("$.collection.items", e))?;
            for it in env.collection.items {
                let Some(d) = it.data.first() else { continue };
                if !seen.insert(d.nasa_id.clone()) {
                    continue;
                }
                out.refs.push(ItemRef {
                    source_id: ID.into(),
                    kind: "nasa.image".into(),
                    key: d.nasa_id.clone(),
                    url: format!(
                        "https://images.nasa.gov/details/{}",
                        d.nasa_id.replace(' ', "%20")
                    ),
                    event_key_hint: None,
                    published_at: d
                        .date_created
                        .as_deref()
                        .and_then(crate::util::date_prefix)
                        .map(day_vintage),
                    title: Some(d.title.trim().to_owned()),
                    meta: json!({"query": q}),
                });
            }
        }
        let keep: Vec<&String> = seen.iter().rev().take(2000).collect();
        out.next_cursor = serde_json::to_string(&keep).ok();
        Ok(out)
    }

    fn fetch_requests(&self, item: &ItemRef) -> Vec<Request> {
        vec![Request::json(lookup_url(&item.key))]
    }

    fn parse(&self, item: &ItemRef, snaps: &[Snapshot]) -> Result<Parsed, ParseError> {
        let snap = find(snaps, &lookup_url(&item.key))?;
        let env: Envelope =
            serde_json::from_slice(&snap.bytes).map_err(|e| shape("$.collection.items", e))?;
        let it = env
            .collection
            .items
            .into_iter()
            .find(|i| i.data.first().is_some_and(|d| d.nasa_id == item.key))
            .ok_or_else(|| {
                shape(
                    "$.collection.items",
                    format!("{} not in the response", item.key),
                )
            })?;
        let d = it
            .data
            .first()
            .ok_or_else(|| shape("$.collection.items[0].data", "empty"))?;
        if d.media_type != "image" {
            return Err(shape(
                "$.data[0].media_type",
                format!("{:?} is not an image", d.media_type),
            ));
        }
        // The largest "alternate" rendition: a web-sized file, not the multi-megabyte original.
        let best = it
            .links
            .iter()
            .filter(|l| l.render == "image" && l.rel == "alternate")
            .max_by_key(|l| l.width.unwrap_or(0))
            .ok_or_else(|| shape("$.links", "no web-sized image rendition"))?;
        let credit = d.photographer.clone().filter(|c| !c.trim().is_empty());
        let rehost = credit.as_deref().is_some_and(|c| c.starts_with("NASA"));
        let alt = d
            .description_508
            .clone()
            .filter(|a| !a.trim().is_empty())
            .unwrap_or_else(|| d.title.trim().to_owned());
        let mut out = Parsed::default();
        out.images.push(ImageRef {
            url: best.href.replace(' ', "%20"),
            credit,
            licence: LICENCE.into(),
            alt,
            width: best.width,
            height: best.height,
            rehost,
        });
        Ok(out)
    }
}
