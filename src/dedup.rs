//! Persistent local MD5 dedup index.
//!
//! Mirrors the original client's `uploads.precomputed_md5` + `PreUploadCheck`
//! mechanism (`docs/SPEC_DEDUP.md` §1/§2.2): the duplicate identity is the MD5 of
//! the whole file body. Our **skip rule is path-centric** (user decision
//! 2026-10-13):
//!
//! * same path + same MD5  → skip (already uploaded),
//! * same path + other MD5 → re-upload (file changed),
//! * unknown path          → upload.
//!
//! The MD5 is still stored (and indexed) so a cross-path dedup can be added later
//! without a schema change. The index lives at
//! `~/Library/Application Support/amzn_photos_uploader/dedup.sqlite3` and survives
//! restarts. If the config directory cannot be created or opened, the index falls
//! back to an in-memory database (dedup still works for the session) and finally
//! to a no-op — it never panics.

use rusqlite::{Connection, OptionalExtension};
use std::path::{Path, PathBuf};

/// Local upload index. All methods are cheap and never panic; a broken or
/// unavailable database degrades to "nothing recorded / nothing skipped".
pub struct DedupIndex {
    conn: Option<Connection>,
}

impl DedupIndex {
    /// Open (creating if needed) the persistent index. Falls back to an
    /// in-memory database and then to a no-op on any error.
    pub fn open() -> Self {
        match Self::db_path() {
            Some(path) => {
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                match Connection::open(&path) {
                    Ok(conn) => Self::from_conn(conn),
                    Err(_) => Self::fallback(),
                }
            }
            None => Self::fallback(),
        }
    }

    fn fallback() -> Self {
        match Connection::open_in_memory() {
            Ok(conn) => Self::from_conn(conn),
            Err(_) => Self { conn: None },
        }
    }

    fn from_conn(conn: Connection) -> Self {
        let ok = conn
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS uploads (
                     path        TEXT PRIMARY KEY,
                     md5         TEXT NOT NULL,
                     size        INTEGER NOT NULL,
                     mtime       INTEGER NOT NULL,
                     uploaded_at TEXT NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS ix_uploads_md5 ON uploads(md5);",
            )
            .is_ok();
        Self {
            conn: if ok { Some(conn) } else { None },
        }
    }

    fn db_path() -> Option<PathBuf> {
        Some(
            dirs::config_dir()
                .unwrap_or_else(std::env::temp_dir)
                .join("amzn_photos_uploader")
                .join("dedup.sqlite3"),
        )
    }

    /// Path-centric pre-upload check: `true` when this exact path was already
    /// uploaded with the same MD5. `size`/`mtime` are recorded alongside but do
    /// not take part in the decision (MD5 is the identity, see docs/SPEC_DEDUP §1).
    pub fn should_skip(&self, path: &Path, md5: &str, size: u64, mtime: i64) -> bool {
        let Some(conn) = self.conn.as_ref() else {
            return false;
        };
        let path_str = path.to_string_lossy();
        let stored: Option<String> = conn
            .query_row(
                "SELECT md5 FROM uploads WHERE path = ?1",
                [path_str.as_ref()],
                |row| row.get(0),
            )
            .optional()
            .unwrap_or(None);
        match stored {
            Some(prev) if prev == md5 => {
                log::debug!(
                    "dedup: skip {} (md5 {}, size {}, mtime {})",
                    path.display(),
                    md5,
                    size,
                    mtime
                );
                true
            }
            _ => false,
        }
    }

    /// Record a successful upload (or a 409 duplicate accepted as success) so the
    /// next scan can skip it.
    pub fn record_upload(&self, path: &Path, md5: &str, size: u64, mtime: i64) {
        let Some(conn) = self.conn.as_ref() else {
            return;
        };
        let path_str = path.to_string_lossy();
        let uploaded_at = chrono::Utc::now().to_rfc3339();
        let _ = conn.execute(
            "INSERT INTO uploads (path, md5, size, mtime, uploaded_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(path) DO UPDATE SET
                 md5 = excluded.md5,
                 size = excluded.size,
                 mtime = excluded.mtime,
                 uploaded_at = excluded.uploaded_at",
            rusqlite::params![path_str.as_ref(), md5, size as i64, mtime, uploaded_at],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index() -> DedupIndex {
        DedupIndex::from_conn(Connection::open_in_memory().unwrap())
    }

    #[test]
    fn skip_only_on_same_path_and_md5() {
        let idx = index();
        let p = Path::new("/photos/a.jpg");

        // Unknown path → upload.
        assert!(!idx.should_skip(p, "aaaa", 10, 1));

        // Recorded, same md5 → skip.
        idx.record_upload(p, "aaaa", 10, 1);
        assert!(idx.should_skip(p, "aaaa", 10, 1));

        // Same path, different md5 → re-upload.
        assert!(!idx.should_skip(p, "bbbb", 20, 2));

        // Different path, same md5 → upload (path-centric rule).
        assert!(!idx.should_skip(Path::new("/photos/b.jpg"), "aaaa", 10, 1));
    }

    #[test]
    fn re_record_updates_md5() {
        let idx = index();
        let p = Path::new("/photos/a.jpg");
        idx.record_upload(p, "aaaa", 10, 1);
        idx.record_upload(p, "cccc", 30, 3);
        assert!(idx.should_skip(p, "cccc", 30, 3));
        assert!(!idx.should_skip(p, "aaaa", 10, 1));
    }
}
