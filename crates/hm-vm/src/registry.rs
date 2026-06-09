//! Snapshot registry backed by `SQLite`.
//!
//! Provides persistent LRU caching of [`SnapshotId`]s across process restarts.
//! The registry evicts the least-recently-accessed entries when the capacity is
//! exceeded, returning the evicted snapshot IDs so the caller can clean up
//! backend resources.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use std::sync::Mutex;

use anyhow::Result;
use rusqlite::Connection;

use crate::types::SnapshotId;

/// Persistent LRU cache mapping opaque keys to [`SnapshotId`]s.
///
/// Backed by a single `SQLite` table with WAL journaling. The registry tracks
/// the last-access timestamp for every entry and evicts the oldest entries
/// when the configured capacity is exceeded.
///
/// The inner `Connection` is wrapped in a [`Mutex`] so that the registry
/// (and any struct containing it, e.g. [`crate::vm::HmVm`]) satisfies
/// `Send + Sync` for safe sharing across async tasks.
#[derive(derive_more::Debug)]
pub struct ImageRegistry {
    #[debug(skip)]
    conn: Mutex<Connection>,
    capacity: u64,
}

/// Returns the current Unix epoch in seconds.
fn epoch_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .try_into()
        .unwrap_or(i64::MAX)
}

impl ImageRegistry {
    /// Open or create the registry database at `path`.
    ///
    /// The parent directory is created if it does not exist. The database uses
    /// WAL mode and `NORMAL` synchronous for a good balance of durability and
    /// performance.
    ///
    /// # Errors
    ///
    /// Returns an error if the database cannot be opened or the schema cannot
    /// be applied.
    pub fn open(path: &Path, capacity: u64) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(path)?;

        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;",
        )?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS snapshots (
                 key         TEXT PRIMARY KEY,
                 snapshot_id TEXT NOT NULL,
                 accessed_at INTEGER NOT NULL
             );",
        )?;

        // Idempotent migration: add workspace_dir column if missing.
        let has_ws_col: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('snapshots') WHERE name='workspace_dir'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0)
            > 0;
        if !has_ws_col {
            conn.execute_batch("ALTER TABLE snapshots ADD COLUMN workspace_dir TEXT;")?;
        }

        Ok(Self {
            conn: Mutex::new(conn),
            capacity,
        })
    }

    /// Look up a cached snapshot and update its access time.
    ///
    /// Returns `None` if no entry exists for `key`.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<SnapshotId> {
        self.get_with_workspace(key).map(|(snap, _)| snap)
    }

    /// Look up a cached snapshot and its workspace directory, updating the
    /// access time.
    ///
    /// Returns `None` if no entry exists for `key`.
    #[must_use]
    pub fn get_with_workspace(&self, key: &str) -> Option<(SnapshotId, Option<String>)> {
        let now = epoch_secs();
        let conn = self.conn.lock().ok()?;

        let result: Option<(String, Option<String>)> = conn
            .query_row(
                "SELECT snapshot_id, workspace_dir FROM snapshots WHERE key = ?1",
                [key],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .ok();

        if result.is_some() {
            let _ = conn.execute(
                "UPDATE snapshots SET accessed_at = ?1 WHERE key = ?2",
                rusqlite::params![now, key],
            );
        }

        drop(conn);
        result.map(|(snap, ws)| (SnapshotId(snap), ws))
    }

    /// Insert or update a cache entry.
    ///
    /// Returns the evicted entries (snapshot ID and optional workspace directory)
    /// to keep the registry within its configured capacity. The caller is
    /// responsible for cleaning up backend resources and workspace directories
    /// associated with evicted entries.
    pub fn put(&self, key: &str, snapshot: &SnapshotId) -> Vec<(SnapshotId, Option<String>)> {
        let now = epoch_secs();

        let Ok(conn) = self.conn.lock() else {
            return Vec::new();
        };

        // INSERT OR REPLACE handles both new and updated entries.
        let _result = conn.execute(
            "INSERT OR REPLACE INTO snapshots (key, snapshot_id, accessed_at)
             VALUES (?1, ?2, ?3)",
            rusqlite::params![key, snapshot.0, now],
        );

        drop(conn);
        self.evict_overflow()
    }

    /// Insert or update a cache entry with an associated workspace directory.
    ///
    /// Returns the evicted entries (snapshot ID and optional workspace directory)
    /// to keep the registry within its configured capacity.
    pub fn put_with_workspace(
        &self,
        key: &str,
        snapshot: &SnapshotId,
        workspace_dir: &str,
    ) -> Vec<(SnapshotId, Option<String>)> {
        let now = epoch_secs();

        let Ok(conn) = self.conn.lock() else {
            return Vec::new();
        };

        let _result = conn.execute(
            "INSERT OR REPLACE INTO snapshots (key, snapshot_id, accessed_at, workspace_dir)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![key, snapshot.0, now, workspace_dir],
        );

        drop(conn);
        self.evict_overflow()
    }

    /// Remove a specific entry.
    ///
    /// Returns the removed snapshot's ID and workspace directory so the
    /// caller can clean up backend resources, or `None` if the key was
    /// not present.
    #[must_use]
    pub fn invalidate(&self, key: &str) -> Option<(SnapshotId, Option<String>)> {
        let conn = self.conn.lock().ok()?;

        let row: Option<(String, Option<String>)> = conn
            .query_row(
                "SELECT snapshot_id, workspace_dir FROM snapshots WHERE key = ?1",
                [key],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();

        if row.is_some() {
            let _ = conn.execute("DELETE FROM snapshots WHERE key = ?1", [key]);
        }

        drop(conn);
        row.map(|(snap, ws)| (SnapshotId(snap), ws))
    }

    /// Returns the number of cached entries.
    #[must_use]
    pub fn len(&self) -> u64 {
        let Ok(conn) = self.conn.lock() else {
            return 0;
        };
        conn.query_row("SELECT COUNT(*) FROM snapshots", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap_or(0)
        .try_into()
        .unwrap_or(0)
    }

    /// Returns `true` if the registry contains no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Evict the oldest entries (by `accessed_at`) when the registry exceeds
    /// its capacity. Returns the snapshot IDs and optional workspace directories
    /// of evicted entries.
    fn evict_overflow(&self) -> Vec<(SnapshotId, Option<String>)> {
        let count = self.len();
        if count <= self.capacity {
            return Vec::new();
        }

        let overflow = count - self.capacity;

        let Ok(conn) = self.conn.lock() else {
            return Vec::new();
        };

        let Ok(mut stmt) = conn.prepare(
            "SELECT snapshot_id, workspace_dir FROM snapshots ORDER BY accessed_at ASC LIMIT ?1",
        ) else {
            return Vec::new();
        };

        let evicted: Vec<(SnapshotId, Option<String>)> = stmt
            .query_map([overflow], |row| {
                Ok((
                    SnapshotId(row.get::<_, String>(0)?),
                    row.get::<_, Option<String>>(1)?,
                ))
            })
            .ok()
            .map(|rows| rows.filter_map(Result::ok).collect())
            .unwrap_or_default();

        // Drop stmt before using conn again for the delete.
        drop(stmt);

        // Delete those entries.
        let _deleted = conn.execute(
            "DELETE FROM snapshots WHERE key IN (
                 SELECT key FROM snapshots ORDER BY accessed_at ASC LIMIT ?1
             )",
            [overflow],
        );

        evicted
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn open_temp(capacity: u64) -> (ImageRegistry, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let db_path = dir.path().join("registry.db");
        let registry = ImageRegistry::open(&db_path, capacity).expect("failed to open registry");
        (registry, dir)
    }

    #[test]
    fn get_returns_none_for_unknown_key() {
        let (reg, _dir) = open_temp(10);
        assert!(reg.get("nonexistent").is_none());
    }

    #[test]
    fn put_then_get_returns_snapshot() {
        let (reg, _dir) = open_temp(10);
        let snap = SnapshotId("snap-abc".into());
        let evicted = reg.put("my-key", &snap);
        assert!(evicted.is_empty());

        let got = reg.get("my-key");
        assert_eq!(got, Some(SnapshotId("snap-abc".into())));
    }

    #[test]
    fn get_updates_access_time() {
        let (reg, _dir) = open_temp(2);

        // Insert a, then b. "a" is older by insertion order.
        reg.put("a", &SnapshotId("snap-a".into()));

        // Tiny sleep so timestamps differ.
        std::thread::sleep(std::time::Duration::from_secs(1));

        reg.put("b", &SnapshotId("snap-b".into()));

        // Touch "a" so it becomes the most recently accessed.
        std::thread::sleep(std::time::Duration::from_secs(1));
        let _ = reg.get("a");

        // Now insert "c" -- capacity is 2, so one must be evicted.
        // "b" should be evicted since "a" was touched more recently.
        std::thread::sleep(std::time::Duration::from_secs(1));
        let evicted = reg.put("c", &SnapshotId("snap-c".into()));

        assert_eq!(evicted.len(), 1);
        assert_eq!(evicted[0].0, SnapshotId("snap-b".into()));

        // "a" should still be present.
        assert!(reg.get("a").is_some());
        // "b" should be gone.
        assert!(reg.get("b").is_none());
    }

    #[test]
    fn eviction_returns_overflow_entries() {
        let (reg, _dir) = open_temp(2);

        reg.put("x", &SnapshotId("snap-x".into()));
        std::thread::sleep(std::time::Duration::from_secs(1));
        reg.put("y", &SnapshotId("snap-y".into()));
        std::thread::sleep(std::time::Duration::from_secs(1));

        // This third insert should evict the oldest ("x").
        let evicted = reg.put("z", &SnapshotId("snap-z".into()));

        assert_eq!(evicted.len(), 1);
        assert_eq!(evicted[0].0, SnapshotId("snap-x".into()));
        assert_eq!(reg.len(), 2);
    }

    #[test]
    fn survives_reopen() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let db_path = dir.path().join("registry.db");

        {
            let reg = ImageRegistry::open(&db_path, 10).expect("open");
            reg.put("persistent", &SnapshotId("snap-persist".into()));
            assert_eq!(reg.len(), 1);
            // reg is dropped here, closing the connection.
        }

        let reg2 = ImageRegistry::open(&db_path, 10).expect("reopen");
        assert_eq!(reg2.len(), 1);
        let got = reg2.get("persistent");
        assert_eq!(got, Some(SnapshotId("snap-persist".into())));
    }

    #[test]
    fn invalidate_returns_removed_snapshot() {
        let (reg, _dir) = open_temp(10);
        let snap = SnapshotId("snap-rm".into());
        reg.put("to-remove", &snap);

        let removed = reg.invalidate("to-remove");
        assert_eq!(removed, Some((SnapshotId("snap-rm".into()), None)));
        assert!(reg.get("to-remove").is_none());
        assert_eq!(reg.len(), 0);

        // Invalidating a non-existent key returns None.
        let removed2 = reg.invalidate("to-remove");
        assert!(removed2.is_none());
    }

    #[test]
    fn put_with_workspace_then_get_returns_both() {
        let (reg, _dir) = open_temp(10);
        let snap = SnapshotId("snap-ws".into());
        reg.put_with_workspace("ws-key", &snap, "/cache/workspaces/ws-key");

        let (got_snap, got_ws) = reg.get_with_workspace("ws-key").unwrap();
        assert_eq!(got_snap, SnapshotId("snap-ws".into()));
        assert_eq!(got_ws.as_deref(), Some("/cache/workspaces/ws-key"));
    }

    #[test]
    fn put_without_workspace_returns_none_workspace() {
        let (reg, _dir) = open_temp(10);
        reg.put("plain-key", &SnapshotId("snap-plain".into()));

        let (_, got_ws) = reg.get_with_workspace("plain-key").unwrap();
        assert!(got_ws.is_none());
    }

    #[test]
    fn eviction_includes_workspace_path() {
        let (reg, _dir) = open_temp(1);
        reg.put_with_workspace("a", &SnapshotId("snap-a".into()), "/ws/a");
        std::thread::sleep(std::time::Duration::from_secs(1));

        let evicted = reg.put_with_workspace("b", &SnapshotId("snap-b".into()), "/ws/b");
        assert_eq!(evicted.len(), 1);
        assert_eq!(evicted[0].0, SnapshotId("snap-a".into()));
        assert_eq!(evicted[0].1.as_deref(), Some("/ws/a"));
    }
}
