//! Persistent local MD5 dedup index.
//!
//! Mirrors the original client's `uploads.precomputed_md5` + `PreUploadCheck`
//! mechanism (`docs/original/SPEC_DEDUP.md` §1/§2.2): the duplicate identity is the MD5 of
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
//!
//! Since 2026-10-16 each row also carries the account `node_id` its path was
//! uploaded to. That binding is what the safe-replace rule reads
//! ([`DedupIndex::node_id_for`]): `ConflictResolution::Replace` only overwrites a
//! name-clashing node when the index binds this very path to it, so a foreign
//! photo that merely shares the name is never destroyed.

use rusqlite::{Connection, OptionalExtension};
use std::path::{Path, PathBuf};

/// Local upload index. All methods are cheap and never panic; a broken or
/// unavailable database degrades to "nothing recorded / nothing skipped".
pub struct DedupIndex {
    conn: Option<Connection>,
}

/// Why a path is present in the index — the outcome of the run that recorded it.
///
/// The distinction matters at skip time (user decision 2026-10-14): a path whose
/// **content** was already in the account under another path (`ContentSkip`,
/// renamed/moved) must keep counting as "bereits im Konto" in *every* run, while
/// a path that itself was uploaded stays "unverändert".
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Origin {
    /// The file itself was uploaded (or a server 409 accepted it as present).
    Uploaded,
    /// The content was already in the account under a different path; nothing
    /// was sent (local cross-path content dedup).
    ContentSkip,
}

impl Origin {
    /// The value stored in `uploads.origin`.
    fn as_db(self) -> &'static str {
        match self {
            Origin::Uploaded => "uploaded",
            Origin::ContentSkip => "content_skip",
        }
    }

    /// Map a stored value back; anything unknown (a legacy row written before
    /// the column existed, or a future value) counts as a real upload — the
    /// conservative default.
    fn from_db(s: &str) -> Self {
        match s {
            "content_skip" => Origin::ContentSkip,
            _ => Origin::Uploaded,
        }
    }
}

/// The one and only definition of the `uploads` table (plus its MD5 index).
///
/// A fresh database and the repair path of [`DedupIndex::from_conn`] both run
/// exactly this DDL, so the two schemas can never drift apart.
const UPLOADS_DDL: &str = "\
CREATE TABLE IF NOT EXISTS uploads (
     path        TEXT PRIMARY KEY,
     md5         TEXT NOT NULL,
     size        INTEGER NOT NULL,
     mtime       INTEGER NOT NULL,
     uploaded_at TEXT NOT NULL,
     origin      TEXT NOT NULL DEFAULT 'uploaded',
     node_id     TEXT
 );
 CREATE INDEX IF NOT EXISTS ix_uploads_md5 ON uploads(md5);";

/// The remaining tables: the cold-start watermark (`folder_scan`) and the
/// persisted one-time drag & drop queue (`one_time_queue`). They are independent
/// of the `uploads` schema and survive a repair untouched.
const AUX_TABLES_DDL: &str = "\
CREATE TABLE IF NOT EXISTS folder_scan (
     path       TEXT PRIMARY KEY,
     max_mtime  INTEGER NOT NULL,
     scanned_at TEXT NOT NULL
 );
 -- Persisted one-time (drag & drop) queue (user decision 2026-10-14): one row
 -- per already-resolved file, ordered by `position`. The table holds at most
 -- ONE queue — a new drop replaces every row. `position` is the primary key, so
 -- the order is fixed and reproducible across a restart. The row's
 -- `display_path` is stored on every row (denormalised): the spec mandates one
 -- row per file, and the value is identical for all rows of a queue.
 CREATE TABLE IF NOT EXISTS one_time_queue (
     position     INTEGER PRIMARY KEY,
     display_path TEXT NOT NULL,
     path         TEXT NOT NULL
 );";

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
        let ok = conn.execute_batch(UPLOADS_DDL).is_ok()
            && conn.execute_batch(AUX_TABLES_DDL).is_ok();
        if !ok {
            return Self { conn: None };
        }
        // Databases created before 2026-10-14 lack the `origin` column, and
        // those created before 2026-10-16 lack `node_id`: the `CREATE TABLE IF
        // NOT EXISTS` above leaves an existing table untouched, so each missing
        // column is added explicitly. This is the **first** path — a plain
        // `ALTER TABLE` that keeps every existing row.
        //
        // The ignored `Result` is only acceptable because the schema is
        // *verified* right after (Welle 5.4). A migration that silently fails is
        // not a cosmetic loss: without `origin`, `should_skip`/`stat_matches`
        // (both `SELECT …, origin FROM uploads`) fail and answer `None`, and
        // `record` (which writes `origin`) writes nothing — the fast path and
        // the recording die **for good**, while `content_seen` (reads only
        // `md5`) keeps working, so the index never heals because nothing is ever
        // recorded. Without `node_id` the same hits `node_id_for` and with it the
        // safe-replace binding. When the check below still finds the schema
        // broken, it is therefore rebuilt from scratch
        // ([`DedupIndex::repair_uploads_schema`]). The accepted price is the lost
        // index content — including the `node_id` bindings, which makes the
        // safe-replace rule fall back to *renaming* for those files (the safe
        // direction, already accepted for legacy rows).
        let _ = Self::ensure_origin_column(&conn);
        let _ = Self::ensure_node_id_column(&conn);
        if !Self::uploads_schema_ok(&conn) {
            match Self::repair_uploads_schema(&conn) {
                Ok(()) => log::warn!(
                    "Der lokale Upload-Index war beschädigt und wurde neu angelegt; \
                     die nächste Prüfung liest alle Dateien erneut."
                ),
                Err(err) => {
                    log::debug!("dedup: Index-Reparatur fehlgeschlagen: {err}");
                    log::error!(
                        "Der lokale Upload-Index ist beschädigt und konnte nicht neu \
                         angelegt werden; in dieser Sitzung wird nichts übersprungen \
                         und nichts aufgezeichnet."
                    );
                    return Self { conn: None };
                }
            }
        }
        Self { conn: Some(conn) }
    }

    /// Add the `origin` column to an `uploads` table that predates it (user
    /// decision 2026-10-14). A fresh table already carries it, so this is a
    /// no-op there; an existing table is migrated in place, with the `DEFAULT`
    /// making every legacy row count as `'uploaded'` (nothing changes for it).
    fn ensure_origin_column(conn: &Connection) -> rusqlite::Result<()> {
        let has_origin = {
            let mut stmt = conn.prepare("PRAGMA table_info(uploads)")?;
            let mut rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
            rows.any(|name| name.map(|n| n == "origin").unwrap_or(false))
        };
        if !has_origin {
            conn.execute(
                "ALTER TABLE uploads ADD COLUMN origin TEXT NOT NULL DEFAULT 'uploaded'",
                [],
            )?;
        }
        Ok(())
    }

    /// Add the `node_id` column to an `uploads` table that predates it (user
    /// decision 2026-10-16). A fresh table already carries it, so this is a
    /// no-op there; an existing table is migrated in place. The column is
    /// nullable without a default, so every legacy row stays **unbound** —
    /// exactly what the safe-replace rule needs (it then renames instead of
    /// replacing, see [`DedupIndex::node_id_for`]).
    fn ensure_node_id_column(conn: &Connection) -> rusqlite::Result<()> {
        let has_node_id = {
            let mut stmt = conn.prepare("PRAGMA table_info(uploads)")?;
            let mut rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
            rows.any(|name| name.map(|n| n == "node_id").unwrap_or(false))
        };
        if !has_node_id {
            conn.execute("ALTER TABLE uploads ADD COLUMN node_id TEXT", [])?;
        }
        Ok(())
    }

    /// Whether `uploads` really carries both columns the code reads and writes
    /// unconditionally: `origin` (Welle 2) and `node_id` (Welle 4).
    ///
    /// A missing column does not fail loudly at the call sites — `should_skip`,
    /// `stat_matches` and `record` swallow the query error — so this check is
    /// what decides whether the schema is usable or has to be rebuilt
    /// (Welle 5.4, self-healing). The names are matched exactly, like the
    /// migrations above.
    fn uploads_schema_ok(conn: &Connection) -> bool {
        let Ok(mut stmt) = conn.prepare("PRAGMA table_info(uploads)") else {
            return false;
        };
        let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(1)) else {
            return false;
        };
        let mut has_origin = false;
        let mut has_node_id = false;
        for name in rows.flatten() {
            match name.as_str() {
                "origin" => has_origin = true,
                "node_id" => has_node_id = true,
                _ => {}
            }
        }
        has_origin && has_node_id
    }

    /// Rebuild a `uploads` table whose migration failed (Welle 5.4,
    /// self-healing): drop the broken table and recreate exactly the fresh
    /// schema ([`UPLOADS_DDL`]).
    ///
    /// The `folder_scan` watermark is cleared in the same transaction — a stale
    /// `max_mtime` would make every folder look "unchanged" and the freshly
    /// emptied index would never be refilled. `one_time_queue` is deliberately
    /// left alone: it does not depend on the `uploads` schema.
    ///
    /// Everything runs in one transaction, so a failure leaves the database
    /// exactly as it was; the caller then degrades to a no-op index.
    fn repair_uploads_schema(conn: &Connection) -> rusqlite::Result<()> {
        let tx = conn.unchecked_transaction()?;
        // Dropping the table also drops its indexes; the explicit `DROP INDEX`
        // covers an index that somehow exists on its own.
        tx.execute_batch(
            "DROP INDEX IF EXISTS ix_uploads_md5;
             DROP TABLE IF EXISTS uploads;",
        )?;
        tx.execute_batch(UPLOADS_DDL)?;
        tx.execute("DELETE FROM folder_scan", [])?;
        tx.commit()
    }

    fn db_path() -> Option<PathBuf> {
        Some(
            dirs::config_dir()
                .unwrap_or_else(std::env::temp_dir)
                .join("prime_photos")
                .join("dedup.sqlite3"),
        )
    }

    /// Path-centric pre-upload check: `Some(origin)` when this exact path was
    /// already recorded with the same MD5, `None` otherwise. `size`/`mtime` are
    /// recorded alongside but do not take part in the decision (MD5 is the
    /// identity, see docs/original/SPEC_DEDUP §1).
    ///
    /// The returned [`Origin`] tells the caller whether the path itself was
    /// uploaded ("unverändert") or only marked as a content skip ("bereits im
    /// Konto") — one query answers both.
    pub fn should_skip(&self, path: &Path, md5: &str, size: u64, mtime: i64) -> Option<Origin> {
        let Some(conn) = self.conn.as_ref() else {
            return None;
        };
        let path_str = path.to_string_lossy();
        let stored: Option<(String, String)> = conn
            .query_row(
                "SELECT md5, origin FROM uploads WHERE path = ?1",
                [path_str.as_ref()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .unwrap_or(None);
        match stored {
            Some((prev, origin)) if prev == md5 => {
                log::debug!(
                    "dedup: skip {} (md5 {}, size {}, mtime {})",
                    path.display(),
                    md5,
                    size,
                    mtime
                );
                Some(Origin::from_db(&origin))
            }
            _ => None,
        }
    }

    /// Cross-path content check: `true` when this MD5 is already in the index
    /// under **any** path.
    ///
    /// Content identity rule (user decision 2026-10-14): a file's identity is
    /// its **content**, not its path. A file whose content was already uploaded
    /// under a *different* path (renamed/moved) must be skipped locally, before
    /// the network call, instead of being re-sent only to be rejected by the
    /// server's `DuplicatesConflictError`. This answers exactly "does this
    /// content exist anywhere in the index?" and never takes a path — the
    /// same-path case ("unverändert") stays [`DedupIndex::should_skip`]'s job.
    /// Uses the existing `ix_uploads_md5` index; no schema change.
    pub fn content_seen(&self, md5: &str) -> bool {
        let Some(conn) = self.conn.as_ref() else {
            return false;
        };
        let exists: Option<i64> = conn
            .query_row(
                "SELECT 1 FROM uploads WHERE md5 = ?1 LIMIT 1",
                [md5],
                |row| row.get(0),
            )
            .optional()
            .unwrap_or(None);
        exists.is_some()
    }

    /// Cheap pre-read check: `Some(origin)` only when this exact path is in the
    /// index **and** the stored `size` **and** `mtime` match the given ones
    /// exactly; `None` otherwise.
    ///
    /// A stat hit is only the fast path — the caller then skips reading and
    /// hashing the file. The MD5 remains the duplicate identity
    /// (`docs/original/SPEC_DEDUP.md` §1): this check never replaces `should_skip`, it
    /// only avoids reading a file that provably did not change since it was
    /// recorded. The returned [`Origin`] carries the same distinction as
    /// [`DedupIndex::should_skip`] ("unverändert" vs. "bereits im Konto").
    /// A missing entry, a different size or a different mtime answer `None`,
    /// and the caller reads and hashes exactly as before.
    ///
    /// Safety rule: an **unknown** mtime must never count as a hit. A caller
    /// whose `fs::metadata` failed passes `mtime = 0`; a stored `mtime = 0`
    /// (recorded without a readable timestamp) must not produce spuriously
    /// lucky hits either — `0` on either side always answers `None`.
    pub fn stat_matches(&self, path: &Path, size: u64, mtime: i64) -> Option<Origin> {
        let Some(conn) = self.conn.as_ref() else {
            return None;
        };
        if mtime == 0 {
            return None;
        }
        let path_str = path.to_string_lossy();
        let stored: Option<(i64, i64, String)> = conn
            .query_row(
                "SELECT size, mtime, origin FROM uploads WHERE path = ?1",
                [path_str.as_ref()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .unwrap_or(None);
        match stored {
            Some((s, m, origin)) if s == size as i64 && m == mtime => {
                log::debug!(
                    "dedup: stat hit {} (size {}, mtime {})",
                    path.display(),
                    size,
                    mtime
                );
                Some(Origin::from_db(&origin))
            }
            _ => None,
        }
    }

    /// Record a successful upload (or a 409 duplicate accepted as success) so the
    /// next scan can skip it. Always marks the row as [`Origin::Uploaded`], even
    /// when the same path previously carried a content-skip marker — a real
    /// upload of the path wins over the skip note.
    ///
    /// `node_id` is the account node this path was uploaded to (`None` when the
    /// caller does not know it, e.g. a 409 that named a *foreign* node). It feeds
    /// the safe-replace rule via [`DedupIndex::node_id_for`].
    pub fn record_upload(
        &self,
        path: &Path,
        md5: &str,
        size: u64,
        mtime: i64,
        node_id: Option<&str>,
    ) {
        self.record(path, md5, size, mtime, Origin::Uploaded, node_id);
    }

    /// Remember a local **content skip** (user decision 2026-10-14): the file
    /// was not sent because its content was already in the account under another
    /// path. Recording it keeps the stat fast path alive on the next scan
    /// (no re-read, no re-hash) and keeps the outcome reading "bereits im Konto"
    /// instead of degrading to "unverändert".
    pub fn record_content_skip(&self, path: &Path, md5: &str, size: u64, mtime: i64) {
        self.record(path, md5, size, mtime, Origin::ContentSkip, None);
    }

    /// Shared upsert behind [`DedupIndex::record_upload`] and
    /// [`DedupIndex::record_content_skip`]; `origin` is what the next scan reads
    /// back. Same fallback contract: without a usable connection a silent no-op.
    fn record(
        &self,
        path: &Path,
        md5: &str,
        size: u64,
        mtime: i64,
        origin: Origin,
        node_id: Option<&str>,
    ) {
        let Some(conn) = self.conn.as_ref() else {
            return;
        };
        let path_str = path.to_string_lossy();
        let uploaded_at = chrono::Utc::now().to_rfc3339();
        let _ = conn.execute(
            // `COALESCE(excluded.node_id, node_id)` keeps a known binding: a
            // `None` (e.g. a 409 path that does not know the node) must not
            // erase it, while a `Some` always overwrites.
            "INSERT INTO uploads (path, md5, size, mtime, uploaded_at, origin, node_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(path) DO UPDATE SET
                 md5 = excluded.md5,
                 size = excluded.size,
                 mtime = excluded.mtime,
                 uploaded_at = excluded.uploaded_at,
                 origin = excluded.origin,
                 node_id = COALESCE(excluded.node_id, node_id)",
            rusqlite::params![
                path_str.as_ref(),
                md5,
                size as i64,
                mtime,
                uploaded_at,
                origin.as_db(),
                node_id
            ],
        );
    }

    /// The account node this path was last uploaded to, or `None` when the path
    /// is unknown or its stored value is NULL/empty.
    ///
    /// This is the safe-replace rule's lookup (user decision 2026-10-16): the
    /// `ConflictResolution::Replace` policy may only overwrite a name-clashing
    /// node when this returns exactly that node id — otherwise the clash is with
    /// a *foreign* file of the same name and must be renamed instead.
    pub fn node_id_for(&self, path: &Path) -> Option<String> {
        let conn = self.conn.as_ref()?;
        let path_str = path.to_string_lossy();
        let stored: Option<Option<String>> = conn
            .query_row(
                "SELECT node_id FROM uploads WHERE path = ?1",
                [path_str.as_ref()],
                |row| row.get(0),
            )
            .optional()
            .unwrap_or(None);
        stored.flatten().filter(|id| !id.is_empty())
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

    /// Replace the persisted **one-time** (drag & drop) queue with `files`,
    /// tagged with the Backup row's `display_path`.
    ///
    /// What is stored is the **resolved file list** (a dropped directory was
    /// already walked by the caller), not the raw dropped paths — so a restart
    /// never walks a directory again. One row per file, `position` = the order
    /// in the queue; the method atomically drops the previous content, so the
    /// table holds at most one queue and a half-written state is never visible.
    ///
    /// Same fallback contract as [`DedupIndex::record_upload`]: without a usable
    /// connection this is a silent no-op, so a lost queue only costs a repeat
    /// drop, it never breaks anything.
    pub fn set_one_time_queue(&self, display_path: &Path, files: &[PathBuf]) {
        let Some(conn) = self.conn.as_ref() else {
            return;
        };
        // `unchecked_transaction` borrows `&self` (the connection is behind a
        // shared reference here) and is rolled back if we return before commit.
        let Ok(tx) = conn.unchecked_transaction() else {
            return;
        };
        if tx.execute("DELETE FROM one_time_queue", []).is_err() {
            return; // drop of `tx` rolls the DELETE back
        }
        let display = display_path.to_string_lossy();
        {
            let Ok(mut stmt) =
                tx.prepare("INSERT INTO one_time_queue (position, display_path, path) VALUES (?1, ?2, ?3)")
            else {
                return; // `tx` dropped ⇒ rollback, previous queue untouched
            };
            for (position, file) in files.iter().enumerate() {
                let path = file.to_string_lossy();
                // A failing row aborts the whole write (rollback) rather than
                // committing a truncated queue.
                if stmt
                    .execute(rusqlite::params![
                        position as i64,
                        display.as_ref(),
                        path.as_ref()
                    ])
                    .is_err()
                {
                    return;
                }
            }
        }
        let _ = tx.commit();
    }

    /// The persisted one-time queue as `(display_path, files)` in its original
    /// order, or `None` when empty (or the index is unavailable).
    ///
    /// A set-but-empty queue never exists (`set_one_time_queue` with no files
    /// deletes everything), so `None` is unambiguous.
    pub fn one_time_queue(&self) -> Option<(PathBuf, Vec<PathBuf>)> {
        let conn = self.conn.as_ref()?;
        let mut stmt = conn
            .prepare("SELECT display_path, path FROM one_time_queue ORDER BY position ASC")
            .ok()?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .ok()?;
        let mut display: Option<PathBuf> = None;
        let mut files: Vec<PathBuf> = Vec::new();
        for row in rows {
            let Ok((d, p)) = row else {
                return None;
            };
            if display.is_none() {
                display = Some(PathBuf::from(d));
            }
            files.push(PathBuf::from(p));
        }
        let display = display?;
        if files.is_empty() {
            return None;
        }
        Some((display, files))
    }

    /// Drop the persisted one-time queue (after a cleanly finished run). A
    /// no-op without a usable connection or with nothing stored.
    pub fn clear_one_time_queue(&self) {
        let Some(conn) = self.conn.as_ref() else {
            return;
        };
        let _ = conn.execute("DELETE FROM one_time_queue", []);
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
        assert_eq!(idx.should_skip(p, "aaaa", 10, 1), None);

        // Recorded, same md5 → skip, and it was a real upload.
        idx.record_upload(p, "aaaa", 10, 1, None);
        assert_eq!(idx.should_skip(p, "aaaa", 10, 1), Some(Origin::Uploaded));

        // Same path, different md5 → re-upload.
        assert_eq!(idx.should_skip(p, "bbbb", 20, 2), None);

        // Different path, same md5 → upload (path-centric rule).
        assert_eq!(idx.should_skip(Path::new("/photos/b.jpg"), "aaaa", 10, 1), None);
    }

    #[test]
    fn content_seen_is_path_independent() {
        let idx = index();
        let a = Path::new("/photos/a.jpg");
        let b = Path::new("/photos/renamed.jpg");

        // Empty index → nothing seen.
        assert!(!idx.content_seen("aaaa"));

        // Recorded under `a`; the lookup never names a path, so the same
        // content under a *different* path (`b`, renamed/moved) is found too.
        idx.record_upload(a, "aaaa", 10, 1, None);
        assert!(idx.content_seen("aaaa"));

        // Unknown content → never seen.
        assert!(!idx.content_seen("bbbb"));

        // The path-centric rule and the content rule disagree on `b` by design:
        // `should_skip` says "upload" (this path was never recorded), while
        // `content_seen` already knows the content is in the account.
        assert_eq!(idx.should_skip(b, "aaaa", 10, 1), None);
        assert!(idx.content_seen("aaaa"));
    }

    #[test]
    fn re_record_updates_md5() {
        let idx = index();
        let p = Path::new("/photos/a.jpg");
        idx.record_upload(p, "aaaa", 10, 1, None);
        idx.record_upload(p, "cccc", 30, 3, None);
        assert_eq!(idx.should_skip(p, "cccc", 30, 3), Some(Origin::Uploaded));
        assert_eq!(idx.should_skip(p, "aaaa", 10, 1), None);
    }

    #[test]
    fn stat_matches_needs_same_path_size_and_mtime() {
        let idx = index();
        let p = Path::new("/photos/a.jpg");

        // Unknown path → no stat hit.
        assert_eq!(idx.stat_matches(p, 10, 1), None);

        idx.record_upload(p, "aaaa", 10, 1, None);

        // Recorded with the very same size + mtime → hit, origin is the upload.
        assert_eq!(idx.stat_matches(p, 10, 1), Some(Origin::Uploaded));

        // Different size → no hit (re-read and hash).
        assert_eq!(idx.stat_matches(p, 20, 1), None);

        // Different mtime → no hit.
        assert_eq!(idx.stat_matches(p, 10, 2), None);

        // Unknown mtime (0, e.g. `fs::metadata` failed) → never a hit, not
        // even when the size matches.
        assert_eq!(idx.stat_matches(p, 10, 0), None);

        // Different path → no hit (path-centric rule).
        assert_eq!(idx.stat_matches(Path::new("/photos/b.jpg"), 10, 1), None);
    }

    #[test]
    fn content_skip_is_remembered_and_overwritten_by_a_real_upload() {
        let idx = index();
        let p = Path::new("/photos/renamed.jpg");

        // A content skip is remembered with its own origin, so both fast paths
        // keep answering "bereits im Konto" on every later scan.
        idx.record_content_skip(p, "aaaa", 10, 1);
        assert_eq!(idx.should_skip(p, "aaaa", 10, 1), Some(Origin::ContentSkip));
        assert_eq!(idx.stat_matches(p, 10, 1), Some(Origin::ContentSkip));

        // A later real upload of the same path wins and flips the origin back.
        idx.record_upload(p, "aaaa", 10, 1, None);
        assert_eq!(idx.should_skip(p, "aaaa", 10, 1), Some(Origin::Uploaded));
        assert_eq!(idx.stat_matches(p, 10, 1), Some(Origin::Uploaded));
    }

    #[test]
    fn migration_adds_origin_and_legacy_rows_count_as_uploaded() {
        // A database written before the `origin` column existed.
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE uploads (
                 path        TEXT PRIMARY KEY,
                 md5         TEXT NOT NULL,
                 size        INTEGER NOT NULL,
                 mtime       INTEGER NOT NULL,
                 uploaded_at TEXT NOT NULL
             );
             INSERT INTO uploads (path, md5, size, mtime, uploaded_at)
             VALUES ('/photos/legacy.jpg', 'aaaa', 10, 1, '2026-01-01T00:00:00Z');",
        )
        .unwrap();

        // Opening it runs the migration in place.
        let idx = DedupIndex::from_conn(conn);

        // The legacy row still matches (the column exists and its DEFAULT made
        // the old row count as a real upload — nothing changes for it).
        assert_eq!(
            idx.should_skip(Path::new("/photos/legacy.jpg"), "aaaa", 10, 1),
            Some(Origin::Uploaded)
        );
        assert_eq!(
            idx.stat_matches(Path::new("/photos/legacy.jpg"), 10, 1),
            Some(Origin::Uploaded)
        );

        // The migrated table accepts new origins too.
        let p = Path::new("/photos/new.jpg");
        idx.record_content_skip(p, "bbbb", 20, 2);
        assert_eq!(idx.should_skip(p, "bbbb", 20, 2), Some(Origin::ContentSkip));
    }

    #[test]
    fn content_seen_and_origin_checks_degrade_without_db() {
        let idx = DedupIndex { conn: None };
        // No connection: nothing seen, nothing matched, nothing recorded — and
        // no panic (same contract as `record_upload`).
        assert!(!idx.content_seen("aaaa"));
        assert_eq!(idx.should_skip(Path::new("/photos/a.jpg"), "aaaa", 10, 1), None);
        assert_eq!(idx.stat_matches(Path::new("/photos/a.jpg"), 10, 1), None);
        idx.record_content_skip(Path::new("/photos/a.jpg"), "aaaa", 10, 1);
        assert!(!idx.content_seen("aaaa"));
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

    #[test]
    fn one_time_queue_round_trips_with_order_and_special_chars() {
        let idx = index();
        let display = Path::new("/photos/Urlaub & Café");
        let files = vec![
            PathBuf::from("/photos/Urlaub & Café/b).jpg"),
            PathBuf::from("/photos/Urlaub & Café/a #1.jpg"),
            PathBuf::from("/photos/Urlaub & Café/ünïcode – 2.jpeg"),
        ];

        // Nothing stored yet.
        assert_eq!(idx.one_time_queue(), None);

        idx.set_one_time_queue(display, &files);
        let (got_display, got_files) = idx.one_time_queue().unwrap();
        assert_eq!(got_display, display);
        // Same order as handed in (`position`), not insertion/rowid order.
        assert_eq!(got_files, files);
    }

    #[test]
    fn one_time_queue_replacement_leaves_no_rest_of_the_old_one() {
        let idx = index();
        idx.set_one_time_queue(
            Path::new("/photos/old"),
            &[PathBuf::from("/photos/old/x.jpg"), PathBuf::from("/photos/old/y.jpg")],
        );
        idx.set_one_time_queue(
            Path::new("/photos/new"),
            &[PathBuf::from("/photos/new/z.jpg")],
        );

        let (display, files) = idx.one_time_queue().unwrap();
        assert_eq!(display, Path::new("/photos/new"));
        assert_eq!(files, vec![PathBuf::from("/photos/new/z.jpg")]);
    }

    #[test]
    fn one_time_queue_clear_and_empty_set_answer_none() {
        let idx = index();
        idx.set_one_time_queue(Path::new("/photos/a"), &[PathBuf::from("/photos/a/x.jpg")]);
        assert!(idx.one_time_queue().is_some());

        idx.clear_one_time_queue();
        assert_eq!(idx.one_time_queue(), None);

        // Setting a queue and then an empty list leaves nothing behind.
        idx.set_one_time_queue(Path::new("/photos/a"), &[PathBuf::from("/photos/a/x.jpg")]);
        idx.set_one_time_queue(Path::new("/photos/a"), &[]);
        assert_eq!(idx.one_time_queue(), None);
    }

    #[test]
    fn one_time_queue_degrades_without_db() {
        let idx = DedupIndex { conn: None };
        // No connection: nothing written, nothing read, no panic.
        idx.set_one_time_queue(Path::new("/photos/a"), &[PathBuf::from("/photos/a/x.jpg")]);
        assert_eq!(idx.one_time_queue(), None);
        idx.clear_one_time_queue();
        assert_eq!(idx.one_time_queue(), None);
    }

    #[test]
    fn node_id_binds_a_path_and_unknown_paths_answer_none() {
        let idx = index();
        let p = Path::new("/photos/a.jpg");

        // Unknown path → no binding.
        assert_eq!(idx.node_id_for(p), None);

        // A recorded node id round-trips.
        idx.record_upload(p, "aaaa", 10, 1, Some("N1"));
        assert_eq!(idx.node_id_for(p), Some("N1".to_string()));

        // A record without a node id leaves an unknown path unbound.
        let q = Path::new("/photos/foreign.jpg");
        idx.record_upload(q, "bbbb", 20, 2, None);
        assert_eq!(idx.node_id_for(q), None);

        // A second upload of the same path overwrites the binding.
        idx.record_upload(p, "aaaa", 10, 1, Some("N2"));
        assert_eq!(idx.node_id_for(p), Some("N2".to_string()));
    }

    #[test]
    fn node_id_none_keeps_a_known_binding_and_content_skip_keeps_it_too() {
        let idx = index();
        let p = Path::new("/photos/a.jpg");
        idx.record_upload(p, "aaaa", 10, 1, Some("N1"));

        // `None` (a 409 that does not know the node) must not erase the binding.
        idx.record_upload(p, "bbbb", 20, 2, None);
        assert_eq!(idx.node_id_for(p), Some("N1".to_string()));

        // A content skip carries no node id either and leaves the binding intact.
        idx.record_content_skip(p, "cccc", 30, 3);
        assert_eq!(idx.node_id_for(p), Some("N1".to_string()));
    }

    #[test]
    fn node_id_degrades_without_db() {
        let idx = DedupIndex { conn: None };
        // No connection: no binding, and a record is a silent no-op.
        assert_eq!(idx.node_id_for(Path::new("/photos/a.jpg")), None);
        idx.record_upload(Path::new("/photos/a.jpg"), "aaaa", 10, 1, Some("N1"));
        assert_eq!(idx.node_id_for(Path::new("/photos/a.jpg")), None);
    }

    #[test]
    fn migration_adds_node_id_and_legacy_rows_stay_unbound() {
        // A database written before the `node_id` column existed.
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE uploads (
                 path        TEXT PRIMARY KEY,
                 md5         TEXT NOT NULL,
                 size        INTEGER NOT NULL,
                 mtime       INTEGER NOT NULL,
                 uploaded_at TEXT NOT NULL,
                 origin      TEXT NOT NULL DEFAULT 'uploaded'
             );
             INSERT INTO uploads (path, md5, size, mtime, uploaded_at, origin)
             VALUES ('/photos/legacy.jpg', 'aaaa', 10, 1, '2026-01-01T00:00:00Z', 'uploaded');",
        )
        .unwrap();

        // Opening it runs the migration in place: the column is present, but the
        // legacy row is unbound (no value) — a first content change renames it
        // instead of replacing it.
        let idx = DedupIndex::from_conn(conn);
        assert_eq!(idx.node_id_for(Path::new("/photos/legacy.jpg")), None);

        // The migrated table accepts new bindings.
        let p = Path::new("/photos/new.jpg");
        idx.record_upload(p, "bbbb", 20, 2, Some("N1"));
        assert_eq!(idx.node_id_for(p), Some("N1".to_string()));
    }

    /// A database whose `uploads` table is *unmigratable*: it carries the two
    /// columns under the names `ORIGIN`/`NODE_ID`. SQLite compares column names
    /// case-insensitively, so both `ALTER TABLE … ADD COLUMN` migrations fail
    /// with "duplicate column name" while the exact-name check
    /// ([`DedupIndex::uploads_schema_ok`]) sees neither column — exactly the
    /// broken state Welle 5.4 heals. A `folder_scan` watermark is present too.
    fn broken_index_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE uploads (
                 path        TEXT PRIMARY KEY,
                 md5         TEXT NOT NULL,
                 size        INTEGER NOT NULL,
                 mtime       INTEGER NOT NULL,
                 uploaded_at TEXT NOT NULL,
                 ORIGIN      TEXT NOT NULL DEFAULT 'uploaded',
                 NODE_ID     TEXT
             );
             INSERT INTO uploads (path, md5, size, mtime, uploaded_at, ORIGIN, NODE_ID)
             VALUES ('/photos/old.jpg', 'aaaa', 10, 1, '2026-01-01T00:00:00Z', 'uploaded', 'N9');
             CREATE TABLE folder_scan (
                 path       TEXT PRIMARY KEY,
                 max_mtime  INTEGER NOT NULL,
                 scanned_at TEXT NOT NULL
             );
             INSERT INTO folder_scan (path, max_mtime, scanned_at)
             VALUES ('/photos/album', 100, '2026-01-01T00:00:00Z');",
        )
        .unwrap();
        conn
    }

    #[test]
    fn broken_schema_is_rebuilt_and_works_again() {
        let idx = DedupIndex::from_conn(broken_index_db());

        // The broken table was dropped: the row it carried is gone rather than
        // half-readable.
        assert_eq!(
            idx.should_skip(Path::new("/photos/old.jpg"), "aaaa", 10, 1),
            None
        );
        assert_eq!(idx.node_id_for(Path::new("/photos/old.jpg")), None);

        // The rebuilt table is fully functional again: recording and both fast
        // paths round-trip.
        let p = Path::new("/photos/a.jpg");
        idx.record_upload(p, "bbbb", 20, 2, Some("N1"));
        assert_eq!(idx.should_skip(p, "bbbb", 20, 2), Some(Origin::Uploaded));
        assert_eq!(idx.stat_matches(p, 20, 2), Some(Origin::Uploaded));
        assert_eq!(idx.node_id_for(p), Some("N1".to_string()));
        assert!(idx.content_seen("bbbb"));
    }

    #[test]
    fn repair_clears_the_folder_watermark() {
        let conn = broken_index_db();
        // Before the repair the watermark is there …
        let watermark: i64 = conn
            .query_row(
                "SELECT max_mtime FROM folder_scan WHERE path = '/photos/album'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(watermark, 100);

        let idx = DedupIndex::from_conn(conn);

        // … after it the folder counts as unscanned, so the next start re-reads
        // it instead of trusting a stale "unchanged".
        assert_eq!(idx.folder_max_mtime(Path::new("/photos/album")), None);
    }

    #[test]
    fn intact_schema_is_left_untouched() {
        let mut idx = index();
        let p = Path::new("/photos/a.jpg");
        idx.record_upload(p, "aaaa", 10, 1, Some("N1"));
        idx.record_folder_scan(Path::new("/photos/album"), 100);

        // Reopen the very same database: a healthy schema must not trigger the
        // repair, so every row (and the watermark) survives.
        let conn = idx.conn.take().unwrap();
        let reopened = DedupIndex::from_conn(conn);
        assert_eq!(
            reopened.should_skip(p, "aaaa", 10, 1),
            Some(Origin::Uploaded)
        );
        assert_eq!(reopened.node_id_for(p), Some("N1".to_string()));
        assert_eq!(
            reopened.folder_max_mtime(Path::new("/photos/album")),
            Some(100)
        );
    }
}
