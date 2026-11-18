//! RSS 2.0 and Atom readers (pure), shared by the central-bank statement Stringers.

use chrono::{DateTime, NaiveDate, Utc};

use crate::ParseError;
use crate::util::shape;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub title: String,
    pub link: String,
    pub id: String,
    pub published: Option<DateTime<Utc>>,
}

impl Entry {
    pub fn day(&self) -> Option<NaiveDate> {
        self.published.map(|p| p.date_naive())
    }
}

fn text<'a>(n: roxmltree::Node<'a, 'a>, name: &str) -> Option<String> {
    n.children()
        .find(|c| c.is_element() && c.tag_name().name() == name)
        .and_then(|c| c.text())
        .map(|t| t.trim().to_owned())
        .filter(|t| !t.is_empty())
}

/// Dates as feeds write them: RFC 2822 (RSS `pubDate`), RFC 3339 (Atom `updated`, RDF `dc:date`) or
/// a bare `YYYY-MM-DD`.
fn when(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc2822(s)
        .or_else(|_| DateTime::parse_from_rfc3339(s))
        // RDF feeds (the RBA's) may leave out the seconds: 2026-11-06T10:45+11:00.
        .or_else(|_| DateTime::parse_from_str(s, "%Y-%m-%dT%H:%M%:z"))
        .ok()
        .map(|d| d.with_timezone(&Utc))
        .or_else(|| {
            NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .ok()
                .and_then(|d| d.and_hms_opt(0, 0, 0))
                .map(|t| t.and_utc())
        })
}

/// Entries of an RSS 2.0, RSS 1.0 (RDF) or Atom document, in document order.
pub fn entries(bytes: &[u8]) -> Result<Vec<Entry>, ParseError> {
    let text_doc = std::str::from_utf8(bytes).map_err(|e| shape("$", e))?;
    let text_doc = text_doc.trim_start_matches('\u{feff}');
    let doc = roxmltree::Document::parse(text_doc).map_err(|e| shape("$", e))?;
    let mut out = Vec::new();
    for n in doc
        .descendants()
        .filter(|n| n.is_element() && matches!(n.tag_name().name(), "item" | "entry"))
    {
        let title = text(n, "title").unwrap_or_default();
        // Atom links are attributes; RSS links are text.
        let link = n
            .children()
            .find(|c| c.is_element() && c.tag_name().name() == "link")
            .and_then(|c| {
                c.attribute("href")
                    .map(str::to_owned)
                    .or_else(|| c.text().map(|t| t.trim().to_owned()))
            })
            .unwrap_or_default();
        let id = text(n, "guid")
            .or_else(|| text(n, "id"))
            .unwrap_or_else(|| link.clone());
        let published = ["pubDate", "published", "updated", "date"]
            .iter()
            .find_map(|k| text(n, k))
            .and_then(|d| when(&d));
        if link.is_empty() {
            return Err(shape("$.item.link", format!("entry {title:?} has no link")));
        }
        out.push(Entry {
            title,
            link,
            id,
            published,
        });
    }
    Ok(out)
}

/// The `<title>` of an HTML page, tags and entities left alone beyond trimming.
pub fn html_title(bytes: &[u8]) -> Option<String> {
    let s = String::from_utf8_lossy(bytes);
    let lower = s.to_ascii_lowercase();
    let start = lower.find("<title")?;
    let open_end = start + lower[start..].find('>')? + 1;
    let end = open_end + lower[open_end..].find("</title>")?;
    Some(
        s[open_end..end]
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
    )
}
