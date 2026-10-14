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
//! `~/Library/Application Support/prime_photos/dedup.sqlite3` and survives
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
                 CREATE INDEX IF NOT EXISTS ix_uploads_md5 ON uploads(md5);
                 CREATE TABLE IF NOT EXISTS folder_scan (
                     path       TEXT PRIMARY KEY,
                     max_mtime  INTEGER NOT NULL,
                     scanned_at TEXT NOT NULL
                 );",
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
                .join("prime_photos")
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

    /// Cheap pre-read check: `true` only when this exact path is in the index
    /// **and** the stored `size` **and** `mtime` match the given ones exactly.
    ///
    /// A stat hit is only the fast path — the caller then skips reading and
    /// hashing the file. The MD5 remains the duplicate identity
    /// (`docs/SPEC_DEDUP.md` §1): this check never replaces `should_skip`, it
    /// only avoids reading a file that provably did not change since it was
    /// recorded. A missing entry, a different size or a different mtime answer
    /// `false`, and the caller reads and hashes exactly as before.
    ///
    /// Safety rule: an **unknown** mtime must never count as a hit. A caller
    /// whose `fs::metadata` failed passes `mtime = 0`; a stored `mtime = 0`
    /// (recorded without a readable timestamp) must not produce spuriously
    /// lucky hits either — `0` on either side always answers `false`.
    pub fn stat_matches(&self, path: &Path, size: u64, mtime: i64) -> bool {
        let Some(conn) = self.conn.as_ref() else {
            return false;
        };
        if mtime == 0 {
            return false;
        }
        let path_str = path.to_string_lossy();
        let stored: Option<(i64, i64)> = conn
            .query_row(
                "SELECT size, mtime FROM uploads WHERE path = ?1",
                [path_str.as_ref()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .unwrap_or(None);
        match stored {
            Some((s, m)) if s == size as i64 && m == mtime => {
                log::debug!(
                    "dedup: stat hit {} (size {}, mtime {})",
                    path.display(),
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

    /// Largest modification time (Unix seconds) recorded for a **backup folder**
    /// the last time it was scanned, or `None` when it was never scanned (or the
    /// index is unavailable).
    ///
    /// This is the cold-start watermark (user decision 2026-10-14): it answers
    /// "did anything in this folder change since we last looked?" without
    /// hashing or even reading a single file. It says nothing about individual
    /// files — that stays the `uploads` table's job.
    pub fn folder_max_mtime(&self, path: &Path) -> Option<i64> {
        let conn = self.conn.as_ref()?;
        let path_str = path.to_string_lossy();
        conn.query_row(
            "SELECT max_mtime FROM folder_scan WHERE path = ?1",
            [path_str.as_ref()],
            |row| row.get(0),
        )
        .optional()
        .ok()
        .flatten()
    }

    /// Remember the watermark of a scanned backup folder (upsert), so the next
    /// start can tell an unchanged folder from a changed one.
    ///
    /// Same fallback contract as [`DedupIndex::record_upload`]: without a usable
    /// connection this is a silent no-op — a lost watermark only costs a scan,
    /// it never breaks anything.
    pub fn record_folder_scan(&self, path: &Path, max_mtime: i64) {
        let Some(conn) = self.conn.as_ref() else {
            return;
        };
        let path_str = path.to_string_lossy();
        let scanned_at = chrono::Utc::now().to_rfc3339();
        let _ = conn.execute(
            "INSERT INTO folder_scan (path, max_mtime, scanned_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(path) DO UPDATE SET
                 max_mtime = excluded.max_mtime,
                 scanned_at = excluded.scanned_at",
            rusqlite::params![path_str.as_ref(), max_mtime, scanned_at],
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

    #[test]
    fn stat_matches_needs_same_path_size_and_mtime() {
        let idx = index();
        let p = Path::new("/photos/a.jpg");

        // Unknown path → no stat hit.
        assert!(!idx.stat_matches(p, 10, 1));

        idx.record_upload(p, "aaaa", 10, 1);

        // Recorded with the very same size + mtime → hit.
        assert!(idx.stat_matches(p, 10, 1));

        // Different size → no hit (re-read and hash).
        assert!(!idx.stat_matches(p, 20, 1));

        // Different mtime → no hit.
        assert!(!idx.stat_matches(p, 10, 2));

        // Unknown mtime (0, e.g. `fs::metadata` failed) → never a hit, not
        // even when the size matches.
        assert!(!idx.stat_matches(p, 10, 0));

        // Different path → no hit (path-centric rule).
        assert!(!idx.stat_matches(Path::new("/photos/b.jpg"), 10, 1));
    }

    #[test]
    fn folder_watermark_round_trips_and_updates() {
        let idx = index();
        let p = Path::new("/photos/album");

        // Never scanned → no watermark.
        assert_eq!(idx.folder_max_mtime(p), None);

        idx.record_folder_scan(p, 100);
        assert_eq!(idx.folder_max_mtime(p), Some(100));

        // A newer scan overwrites the older value (upsert).
        idx.record_folder_scan(p, 250);
        assert_eq!(idx.folder_max_mtime(p), Some(250));

        // A second folder is tracked independently.
        let other = Path::new("/photos/other");
        assert_eq!(idx.folder_max_mtime(other), None);
        idx.record_folder_scan(other, 5);
        assert_eq!(idx.folder_max_mtime(p), Some(250));
        assert_eq!(idx.folder_max_mtime(other), Some(5));
    }

    #[test]
    fn folder_watermark_degrades_without_db() {
        let idx = DedupIndex { conn: None };
        let p = Path::new("/photos/album");
        // No connection: nothing recorded, nothing read, no panic.
        assert_eq!(idx.folder_max_mtime(p), None);
        idx.record_folder_scan(p, 100);
        assert_eq!(idx.folder_max_mtime(p), None);
    }
}
