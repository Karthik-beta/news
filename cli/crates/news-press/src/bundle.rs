//! A rendered page bundle on disk: `<work>/pages/<page_id>/v<version>/manifest.json` and the files it
//! lists (`contracts/bundle/manifest.schema.json`). Press reads it, checks it against the manifest and
//! the outbox row, and sends `{manifest, files}`; it never changes a byte.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use news_chief::gates::hex;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// The write API's body limit for a page (ADR 0006): checked here so an oversized page fails before sending.
pub const MAX_BODY: usize = 1_048_576;

/// A page id is lowercase letters, digits and hyphens (`common.schema.json`); anything else never
/// becomes a path.
pub fn is_page_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// The manifest schema's path allowlist (`contracts/bundle/manifest.schema.json`), by hand: `index.html`,
/// `index.md`, `data.json`, `sources.json`, `meta.json`, `page.oui`, or `charts/<slug>.csv` or `.json` with a slug of
/// `[a-z0-9][a-z0-9-]{0,63}`. No `\`, `:`, `..` or absolute path can match.
pub fn is_bundle_path(path: &str) -> bool {
    if matches!(
        path,
        "index.html" | "index.md" | "data.json" | "sources.json" | "meta.json" | "page.oui"
    ) {
        return true;
    }
    path.strip_prefix("charts/")
        .and_then(|r| r.strip_suffix(".csv").or_else(|| r.strip_suffix(".json")))
        .is_some_and(|slug| {
            (1..=64).contains(&slug.len())
                && !slug.starts_with('-')
                && slug
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}

/// Read a regular file of at most [`MAX_BODY`] bytes: a symlink (or anything else) is refused, and the size
/// is checked before the read.
fn read_regular(p: &Path) -> Result<Vec<u8>, String> {
    let name = p.display();
    let meta = std::fs::symlink_metadata(p).map_err(|e| format!("cannot read {name}: {e}"))?;
    if !meta.is_file() {
        return Err(format!("{name} is not a regular file"));
    }
    if meta.len() > MAX_BODY as u64 {
        return Err(format!("{name} is over the {MAX_BODY}-byte body limit"));
    }
    std::fs::read(p).map_err(|e| format!("cannot read {name}: {e}"))
}

pub fn dir(work: &Path, page_id: &str, version: i32) -> PathBuf {
    work.join("pages").join(page_id).join(format!("v{version}"))
}

#[derive(Debug)]
pub struct Bundle {
    pub bundle_sha: [u8; 32],
    pub body: Vec<u8>,
}

/// Load and verify a bundle: the manifest names this page and version, every file is present with the
/// size and SHA-256 it lists, `bundle_sha256` is what the manifest rules say it is, and the body fits.
pub fn load(work: &Path, page_id: &str, version: i32) -> Result<Bundle, String> {
    if !is_page_id(page_id) {
        return Err(format!("{page_id:?} is not a page id"));
    }
    let root = dir(work, page_id, version);
    let manifest: Value = serde_json::from_slice(&read_regular(&root.join("manifest.json"))?)
        .map_err(|e| format!("manifest.json is not JSON: {e}"))?;
    if manifest["page_id"] != page_id || manifest["version"] != version {
        return Err(format!(
            "manifest.json is for {} v{}, not {page_id} v{version}",
            manifest["page_id"], manifest["version"]
        ));
    }
    let mut files = BTreeMap::new();
    let mut lines = BTreeMap::new();
    for f in manifest["files"].as_array().map_or(&[][..], Vec::as_slice) {
        let path = f["path"].as_str().ok_or("a manifest file has no path")?;
        // Refuse before touching the disk (a `\` or `C:` path would escape the bundle on Windows).
        if !is_bundle_path(path) {
            return Err(format!("manifest path {path:?} is not a bundle path"));
        }
        let charts = root.join("charts");
        if path.starts_with("charts/")
            && std::fs::symlink_metadata(&charts).is_ok_and(|m| !m.is_dir())
        {
            return Err("charts is not a directory".into());
        }
        let bytes = read_regular(&root.join(path))?;
        let sha = hex(&Sha256::digest(&bytes));
        if f["sha256"] != sha || f["bytes"] != bytes.len() {
            return Err(format!("{path} does not match the manifest"));
        }
        let text = String::from_utf8(bytes).map_err(|_| format!("{path} is not UTF-8"))?;
        lines.insert(path.to_owned(), sha);
        files.insert(path.to_owned(), text);
    }
    let listing: String = lines.iter().map(|(p, s)| format!("{p} {s}\n")).collect();
    let bundle_sha: [u8; 32] = Sha256::digest(listing.as_bytes()).into();
    if manifest["bundle_sha256"] != hex(&bundle_sha) {
        return Err("bundle_sha256 does not match the files".into());
    }
    let body = serde_json::to_vec(&json!({"manifest": manifest, "files": files}))
        .map_err(|e| e.to_string())?;
    if body.len() > MAX_BODY {
        return Err(format!(
            "the bundle is {} bytes, over the API's {MAX_BODY}-byte body limit",
            body.len()
        ));
    }
    Ok(Bundle { bundle_sha, body })
}

/// Test helper and fixture builder: write a bundle (manifest included) the way Render will, and return
/// its `bundle_sha256`.
pub fn write(work: &Path, page_id: &str, version: i32, files: &[(&str, &str)]) -> [u8; 32] {
    let root = dir(work, page_id, version);
    let mut entries = Vec::new();
    let mut listing = String::new();
    let mut sorted: Vec<_> = files.to_vec();
    sorted.sort_unstable();
    for (path, text) in &sorted {
        let target = root.join(path);
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&target, text);
        let sha = hex(&Sha256::digest(text.as_bytes()));
        listing.push_str(&format!("{path} {sha}\n"));
        entries.push(json!({"path": path, "sha256": sha, "bytes": text.len()}));
    }
    let sha: [u8; 32] = Sha256::digest(listing.as_bytes()).into();
    let manifest = json!({"schema": "manifest.v1", "page_id": page_id, "version": version,
                          "bundle_sha256": hex(&sha), "files": entries});
    let _ = std::fs::write(root.join("manifest.json"), manifest.to_string());
    sha
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const FILES: [(&str, &str); 3] = [
        ("index.html", "<!doctype html>"),
        ("data.json", "{}"),
        ("charts/a.csv", "x,y\n1,2\n"),
    ];

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("news-press-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn a_bundle_round_trips_and_the_body_holds_every_file() {
        let work = tmp("bundle-ok");
        let sha = write(&work, "p-1", 2, &FILES);
        let b = load(&work, "p-1", 2).unwrap();
        assert_eq!(b.bundle_sha, sha);
        let body: Value = serde_json::from_slice(&b.body).unwrap();
        assert_eq!(body["files"]["charts/a.csv"], "x,y\n1,2\n");
        assert_eq!(body["manifest"]["version"], 2);
        let _ = std::fs::remove_dir_all(&work);
    }

    /// A manifest listing `path` with an invented hash: the path check must fire before any file is read.
    fn manifest_with(work: &Path, path: &str) {
        let root = dir(work, "p-9", 1);
        std::fs::create_dir_all(&root).unwrap();
        let m = json!({"schema": "manifest.v1", "page_id": "p-9", "version": 1, "bundle_sha256": "0",
                       "files": [{"path": path, "sha256": "0", "bytes": 1}]});
        std::fs::write(root.join("manifest.json"), m.to_string()).unwrap();
    }

    #[test]
    fn paths_outside_the_schema_allowlist_are_refused_before_the_disk_is_touched() {
        for p in [
            r"..\..\.env",
            r"C:\Windows\win.ini",
            "C:/Windows/win.ini",
            "/etc/passwd",
            "../x.json",
            "charts/../index.html",
            r"charts\a.csv",
            "charts/A.csv",
            "charts/-a.csv",
            "charts/a/b.csv",
            ".env",
            "manifest.json",
        ] {
            let work = tmp("bundle-path");
            manifest_with(&work, p);
            let e = load(&work, "p-9", 1).unwrap_err();
            assert!(e.contains("not a bundle path"), "{p}: {e}");
            let _ = std::fs::remove_dir_all(&work);
        }
        for p in [
            "index.html",
            "page.oui",
            "charts/a-1.csv",
            "charts/a-1.json",
        ] {
            assert!(is_bundle_path(p), "{p}");
        }
        assert!(is_bundle_path(&format!("charts/{}.csv", "a".repeat(64))));
        assert!(!is_bundle_path(&format!("charts/{}.csv", "a".repeat(65))));
    }

    #[test]
    fn a_file_over_the_limit_is_refused_without_being_read() {
        let work = tmp("bundle-size");
        write(&work, "p-3", 1, &FILES);
        let f = std::fs::File::create(dir(&work, "p-3", 1).join("data.json")).unwrap();
        f.set_len(MAX_BODY as u64 + 1).unwrap();
        assert!(load(&work, "p-3", 1).unwrap_err().contains("body limit"));
        let _ = std::fs::remove_dir_all(&work);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_file_is_refused() {
        let work = tmp("bundle-link");
        write(&work, "p-4", 1, &FILES);
        let secret = work.join("secret.json");
        std::fs::write(&secret, "{}").unwrap();
        let target = dir(&work, "p-4", 1).join("data.json");
        std::fs::remove_file(&target).unwrap();
        std::os::unix::fs::symlink(&secret, &target).unwrap();
        assert!(
            load(&work, "p-4", 1)
                .unwrap_err()
                .contains("not a regular file")
        );
        let _ = std::fs::remove_dir_all(&work);
    }

    #[test]
    fn a_changed_file_a_wrong_version_and_a_big_body_are_refused() {
        let work = tmp("bundle-bad");
        write(&work, "p-1", 1, &FILES);
        std::fs::write(dir(&work, "p-1", 1).join("data.json"), "{ }").unwrap();
        assert!(load(&work, "p-1", 1).unwrap_err().contains("data.json"));
        assert!(load(&work, "p-1", 2).is_err());
        assert!(
            load(&work, "../p-1", 1)
                .unwrap_err()
                .contains("not a page id")
        );

        let big = "x".repeat(MAX_BODY);
        write(&work, "p-2", 1, &[("index.html", big.as_str())]);
        assert!(load(&work, "p-2", 1).unwrap_err().contains("body limit"));
        let _ = std::fs::remove_dir_all(&work);
    }
}
