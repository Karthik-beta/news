//! The Morgue: bytes exactly as received, content-addressed and zstd-compressed (Bureau §3.2).
//!
//! `work/raw/sha256/ab/cd/abcd…ef.zst`. Writing is idempotent (same bytes, same path) and atomic
//! (temporary file, then rename). Reading checks the hash, so a corrupted file is never parsed.

use std::io;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use sha2::{Digest, Sha256};

const ZSTD_LEVEL: i32 = 3;
/// Structured data (JSON, XBRL, CSV, SDMX) is the proof behind every figure (decision D7).
pub const STRUCTURED_DAYS: i64 = 400;
/// HTML, PDF and other documents.
pub const DOCUMENT_DAYS: i64 = 90;

pub struct Morgue {
    root: PathBuf,
}

pub fn hex(sha: &[u8; 32]) -> String {
    sha.iter().map(|b| format!("{b:02x}")).collect()
}

impl Morgue {
    /// `root` is the work directory (`NEWS_WORK_DIR`, default `work`).
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Path relative to the work directory, as stored in `snapshots.storage_path`.
    pub fn relative_path(sha: &[u8; 32]) -> String {
        let h = hex(sha);
        format!("raw/sha256/{}/{}/{h}.zst", &h[0..2], &h[2..4])
    }

    pub fn path(&self, sha: &[u8; 32]) -> PathBuf {
        self.root.join(Self::relative_path(sha))
    }

    /// Store `bytes` under their SHA-256 (computed by the Runner while streaming). Returns the
    /// relative storage path.
    pub fn put(&self, sha: &[u8; 32], bytes: &[u8]) -> io::Result<String> {
        let rel = Self::relative_path(sha);
        let path = self.root.join(&rel);
        if path.exists() {
            return Ok(rel);
        }
        let dir = path.parent().unwrap_or(Path::new("."));
        std::fs::create_dir_all(dir)?;
        let compressed = zstd::bulk::compress(bytes, ZSTD_LEVEL)?;
        let tmp = dir.join(format!(".{}.{}.tmp", hex(sha), std::process::id()));
        std::fs::write(&tmp, compressed)?;
        match std::fs::rename(&tmp, &path) {
            Ok(()) => Ok(rel),
            // Another writer won the race with the same bytes.
            Err(_) if path.exists() => {
                let _ = std::fs::remove_file(&tmp);
                Ok(rel)
            }
            Err(e) => Err(e),
        }
    }

    /// The bytes for `sha`, verified.
    pub fn get(&self, sha: &[u8; 32]) -> io::Result<Vec<u8>> {
        let compressed = std::fs::read(self.path(sha))?;
        let bytes = zstd::stream::decode_all(compressed.as_slice())?;
        let actual: [u8; 32] = Sha256::digest(&bytes).into();
        if actual != *sha {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("morgue file for {} does not match its hash", hex(sha)),
            ));
        }
        Ok(bytes)
    }
}

/// When a snapshot may be pruned: 400 days for structured data, 90 for documents (D7). The hash,
/// URL and locators are kept for ever in the database.
pub fn retain_until(content_type: Option<&str>, fetched_at: DateTime<Utc>) -> DateTime<Utc> {
    let essence = content_type
        .and_then(|c| c.split(';').next())
        .map(|c| c.trim().to_ascii_lowercase())
        .unwrap_or_default();
    let structured = essence.ends_with("json")
        || essence.ends_with("xml")
        || essence == "text/csv"
        || essence.contains("sdmx")
        || essence == "text/plain";
    let structured = structured && essence != "application/xhtml+xml";
    fetched_at
        + Duration::days(if structured {
            STRUCTURED_DAYS
        } else {
            DOCUMENT_DAYS
        })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn put_get_round_trip_and_idempotence() {
        let dir = std::env::temp_dir().join(format!("news-morgue-{}", std::process::id()));
        let m = Morgue::new(&dir);
        let bytes = br#"{"status":"REQUEST_SUCCEEDED"}"#;
        let sha: [u8; 32] = Sha256::digest(bytes).into();
        let rel = m.put(&sha, bytes).unwrap();
        assert!(rel.starts_with(&format!("raw/sha256/{}/", &hex(&sha)[..2])));
        assert!(rel.ends_with(".zst"));
        assert_eq!(m.put(&sha, bytes).unwrap(), rel);
        assert_eq!(m.get(&sha).unwrap(), bytes);
        // A tampered file is refused.
        std::fs::write(m.path(&sha), zstd::bulk::compress(b"other", 3).unwrap()).unwrap();
        assert!(m.get(&sha).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn retention_by_content_type() {
        let t = Utc.with_ymd_and_hms(2026, 11, 16, 0, 0, 0).unwrap();
        assert_eq!(
            (retain_until(Some("application/json; charset=utf-8"), t) - t).num_days(),
            400
        );
        assert_eq!((retain_until(Some("text/html"), t) - t).num_days(), 90);
        assert_eq!(
            (retain_until(Some("application/pdf"), t) - t).num_days(),
            90
        );
        assert_eq!((retain_until(None, t) - t).num_days(), 90);
    }
}
