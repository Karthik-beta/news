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
    let read = |p: &Path| std::fs::read(p).map_err(|e| format!("cannot read {}: {e}", p.display()));
    let manifest: Value = serde_json::from_slice(&read(&root.join("manifest.json"))?)
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
        // The manifest pattern allows no `..` or absolute path; refuse before touching the disk.
        if path.starts_with('/') || path.split('/').any(|s| s.is_empty() || s == "..") {
            return Err(format!("manifest path {path:?} is not a bundle path"));
        }
        let bytes = read(&root.join(path))?;
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
