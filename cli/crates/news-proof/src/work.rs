//! The page folders in the work directory (`NEWS_WORK_DIR`, default `work`; Technical Design, "Local folders").
//!
//! ```text
//! work/briefs/<story>.json          the brief (brief.v1), written by Fixer (#18)
//! work/pages/<story>/data.json      the page's data.json and sources.json (bundle contract), written by Fixer
//! work/pages/<story>/sources.json     with the brief, holding every value the brief's references stand for
//! work/pages/<story>/page.json      page facts the brief does not carry (categories, region, tags, version,
//!                                     first publication, index status): PageFacts in bundle.rs
//! work/pages/<story>/copy.json      the stored Copy: `news save`, `news ai accept`, `news ai fallback`
//! work/pages/<story>/rejected.json  the last Copy `news ai accept` refused, with its errors (input to A2)
//! work/pages/<story>/               also the bundle `news render` writes (index.html, meta.json, charts/…),
//!                                     with manifest.json; `news check` reads it here
//! work/ai/<job>.json                an AI job's composed input file (A2 repair)
//! ```

use chrono::{DateTime, SecondsFormat, Utc};
use news_core::{NewsError, NewsResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Where the Copy came from.
pub const SOURCES: [&str; 3] = ["save", "ai", "fallback"];

/// The flag on a page built by code only (D10): never indexed, upgraded by a later run.
pub const TEMPLATED: &str = "templated";

#[derive(Debug, Clone)]
pub struct Work {
    root: PathBuf,
}

impl Work {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn from_env() -> Self {
        Self::new(
            std::env::var_os("NEWS_WORK_DIR")
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| "work".into()),
        )
    }

    pub fn brief(&self, story: &str) -> PathBuf {
        self.root.join("briefs").join(format!("{story}.json"))
    }
    pub fn page(&self, story: &str) -> PathBuf {
        self.root.join("pages").join(story)
    }
    pub fn copy(&self, story: &str) -> PathBuf {
        self.page(story).join("copy.json")
    }
    pub fn rejected(&self, story: &str) -> PathBuf {
        self.page(story).join("rejected.json")
    }
    pub fn ai_input(&self, job: i64) -> PathBuf {
        self.root.join("ai").join(format!("{job}.json"))
    }
}

/// A story names a folder, so it must be a plain path segment: a story id or a lower-case slug.
pub fn check_story(story: &str) -> NewsResult<()> {
    let ok = !story.is_empty()
        && story.len() <= 64
        && story
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
        && !story.starts_with(['-', '_']);
    if ok {
        Ok(())
    } else {
        Err(NewsError::usage(format!(
            "story {story:?} is not a story id: use lower-case letters, digits, '-' and '_' (up to 64)"
        )))
    }
}

/// `copy.json`: a Copy as stored, with where it came from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stored {
    pub draft_id: String,
    pub story: String,
    /// UTC, RFC 3339 with `Z`. A re-save of the same Copy keeps the first time, so renders stay idempotent.
    pub saved_at: String,
    pub source: String,
    pub job: Option<i64>,
    pub flags: Vec<String>,
    pub copy: Value,
}

impl Stored {
    pub fn templated(&self) -> bool {
        self.flags.iter().any(|f| f == TEMPLATED)
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub fn timestamp(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// Read and parse a JSON file; a missing file is `not_found`.
pub fn read_json(path: &Path) -> NewsResult<Value> {
    let text = read_text(path)?;
    serde_json::from_str(&text)
        .map_err(|e| NewsError::validation(format!("{} is not valid JSON: {e}", path.display())))
}

/// A file's text; a missing file is `not_found`.
pub fn read_text(path: &Path) -> NewsResult<String> {
    std::fs::read_to_string(path)
        .map_err(|e| NewsError::not_found(format!("cannot read {}: {e}", path.display())))
}

/// Write `text` to `path` through a temporary file and a rename, so a reader never sees half a file.
pub fn write_atomic(path: &Path, text: &str) -> NewsResult<()> {
    let io =
        |e: std::io::Error| NewsError::internal(format!("cannot write {}: {e}", path.display()));
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(io)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, text).map_err(io)?;
    std::fs::rename(&tmp, path).map_err(io)
}

pub fn pretty(v: &impl Serialize) -> String {
    // Serialising a Value or a derived struct cannot fail.
    serde_json::to_string_pretty(v).unwrap_or_default() + "\n"
}

/// Store a Copy for `story`. Idempotent: the same Copy and flags give the same draft ID and leave the stored
/// file (and its `saved_at`) as it was.
pub fn save(
    work: &Work,
    story: &str,
    copy: Value,
    flags: Vec<String>,
    source: &str,
    job: Option<i64>,
    now: DateTime<Utc>,
) -> NewsResult<Stored> {
    check_story(story)?;
    let digest = sha256_hex(format!("{copy}\n{}", flags.join(",")).as_bytes());
    let draft_id = format!("{story}-{}", &digest[..12]);
    let path = work.copy(story);
    if let Ok(v) = read_json(&path)
        && let Ok(old) = serde_json::from_value::<Stored>(v)
        && old.draft_id == draft_id
    {
        return Ok(old);
    }
    let stored = Stored {
        draft_id,
        story: story.to_string(),
        saved_at: timestamp(now),
        source: source.to_string(),
        job,
        flags,
        copy,
    };
    write_atomic(&path, &pretty(&stored))?;
    let _ = std::fs::remove_file(work.rejected(story));
    Ok(stored)
}

/// The stored Copy for `story`.
pub fn load(work: &Work, story: &str) -> NewsResult<Stored> {
    check_story(story)?;
    let path = work.copy(story);
    serde_json::from_value(
        read_json(&path).map_err(|e| {
            e.with_hint(format!("store a Copy first: news save {story} <copy.json>"))
        })?,
    )
    .map_err(|e| NewsError::validation(format!("{} is not a stored Copy: {e}", path.display())))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn story_ids_are_plain_path_segments() {
        for ok in ["81234", "fixture-earnings", "a_b"] {
            assert!(check_story(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "..",
            "../x",
            "a/b",
            "a\\b",
            "A",
            "-x",
            "macro:us.cpi",
            &"x".repeat(65),
        ] {
            assert!(check_story(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn saving_the_same_copy_twice_keeps_the_first_record() {
        let dir = std::env::temp_dir().join(format!("news-proof-save-{}", std::process::id()));
        let work = Work::new(&dir);
        let t1 = DateTime::parse_from_rfc3339("2026-11-20T10:00:00Z")
            .unwrap()
            .to_utc();
        let t2 = DateTime::parse_from_rfc3339("2026-11-21T10:00:00Z")
            .unwrap()
            .to_utc();
        let a = save(
            &work,
            "s1",
            json!({"headline": "x"}),
            vec![],
            "save",
            None,
            t1,
        )
        .unwrap();
        let b = save(
            &work,
            "s1",
            json!({"headline": "x"}),
            vec![],
            "ai",
            Some(3),
            t2,
        )
        .unwrap();
        assert_eq!(a, b);
        assert_eq!(load(&work, "s1").unwrap().saved_at, "2026-11-20T10:00:00Z");
        let c = save(
            &work,
            "s1",
            json!({"headline": "y"}),
            vec![],
            "save",
            None,
            t2,
        )
        .unwrap();
        assert_ne!(c.draft_id, a.draft_id);
        assert_eq!(load(&work, "s1").unwrap(), c);
        let t = save(
            &work,
            "s1",
            json!({"headline": "y"}),
            vec![TEMPLATED.into()],
            "fallback",
            None,
            t2,
        )
        .unwrap();
        assert!(t.templated());
        assert_ne!(t.draft_id, c.draft_id);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
