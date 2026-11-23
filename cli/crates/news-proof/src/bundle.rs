//! A rendered page bundle on disk (Technical Design, "Page bundle"; `contracts/bundle/`): `page.oui`,
//! `data.json`, `sources.json`, `meta.json`, `index.html`, `index.md` and `charts/<id>.csv`.
//!
//! Loading never fails: a missing or unreadable file is recorded in `problems`, and check C1 reports it.

use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The six files every bundle has.
pub const REQUIRED: [&str; 6] = [
    "page.oui",
    "data.json",
    "sources.json",
    "meta.json",
    "index.html",
    "index.md",
];

#[derive(Debug, Default)]
pub struct Bundle {
    pub dir: PathBuf,
    /// Bundle-relative path (`index.html`, `charts/x.csv`) to size in bytes.
    pub files: BTreeMap<String, u64>,
    pub oui: String,
    pub html: String,
    pub md: String,
    pub data: Value,
    pub sources: Value,
    pub meta: Value,
    /// `charts/<id>.csv` contents by path.
    pub csv: BTreeMap<String, String>,
    /// (file, what is wrong): missing, unreadable, or not JSON.
    pub problems: Vec<(String, String)>,
}

impl Bundle {
    pub fn load(dir: &Path) -> Self {
        let mut b = Bundle {
            dir: dir.to_owned(),
            data: Value::Null,
            sources: Value::Null,
            meta: Value::Null,
            ..Bundle::default()
        };
        for (rel, size) in list(dir) {
            b.files.insert(rel, size);
        }
        for name in REQUIRED {
            if !b.files.contains_key(name) {
                b.problems.push((name.to_owned(), "file is missing".into()));
            }
        }
        b.oui = b.text("page.oui");
        b.html = b.text("index.html");
        b.md = b.text("index.md");
        b.data = b.json("data.json");
        b.sources = b.json("sources.json");
        b.meta = b.json("meta.json");
        let csvs: Vec<String> = b
            .files
            .keys()
            .filter(|p| p.starts_with("charts/") && p.ends_with(".csv"))
            .cloned()
            .collect();
        for p in csvs {
            let t = b.text(&p);
            b.csv.insert(p, t);
        }
        b
    }

    fn text(&mut self, rel: &str) -> String {
        if !self.files.contains_key(rel) {
            return String::new();
        }
        std::fs::read_to_string(self.dir.join(rel)).unwrap_or_else(|e| {
            self.problems
                .push((rel.to_owned(), format!("unreadable: {e}")));
            String::new()
        })
    }

    fn json(&mut self, rel: &str) -> Value {
        let t = self.text(rel);
        if t.is_empty() {
            return Value::Null;
        }
        serde_json::from_str(&t).unwrap_or_else(|e| {
            self.problems
                .push((rel.to_owned(), format!("not valid JSON: {e}")));
            Value::Null
        })
    }

    pub fn total_bytes(&self) -> u64 {
        self.files.values().sum()
    }

    /// `meta.json`'s string field, trimmed; empty when absent.
    pub fn meta_str(&self, key: &str) -> &str {
        self.meta[key].as_str().unwrap_or_default().trim()
    }

    pub fn page_id(&self) -> String {
        let id = self.meta_str("id");
        if id.is_empty() {
            self.dir
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        } else {
            id.to_owned()
        }
    }

    /// A map in `data.json` (`points`, `derived`, `series`, `periods`, `quotes`, `context`).
    pub fn data_map(&self, kind: &str) -> Option<&serde_json::Map<String, Value>> {
        self.data[kind].as_object()
    }

    /// The Copy's head text: `meta.json`'s `fly_title`, `title` and `summary`, as the page writer wrote them
    /// (with `{ref}` tokens; the renderer fills the values into `index.html`).
    pub fn head_text(&self) -> Vec<(&'static str, String)> {
        ["fly_title", "title", "summary"]
            .into_iter()
            .map(|k| (k, self.meta_str(k).to_owned()))
            .filter(|(_, v)| !v.is_empty())
            .collect()
    }
}

fn list(dir: &Path) -> Vec<(String, u64)> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return out;
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_file() {
            out.push((name, e.metadata().map_or(0, |m| m.len())));
        } else if ft.is_dir()
            && name == "charts"
            && let Ok(inner) = std::fs::read_dir(e.path())
        {
            for f in inner.flatten() {
                if f.file_type().is_ok_and(|t| t.is_file()) {
                    out.push((
                        format!("charts/{}", f.file_name().to_string_lossy()),
                        f.metadata().map_or(0, |m| m.len()),
                    ));
                }
            }
        }
    }
    out
}
