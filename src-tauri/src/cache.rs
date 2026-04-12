use std::path::Path;
use std::sync::Mutex;
use std::collections::HashMap;
use rusqlite::{Connection, params};
use anyhow::Result;

#[derive(Debug, Clone)]
pub struct CachedFile {
    pub blake3:      String,
    pub phash:       Option<String>,  // precise pHash (full decode)
    pub fast_phash:  Option<String>,  // fast pHash (EXIF thumbnail)
    pub header_hash: Option<String>,  // Blake3 of first 4096 bytes — used for cache validation
    pub size_bytes:  u64,
    pub width:       u32,
    pub height:      u32,
    pub modified:    String,
}

/// A cache entry key — used to check if a file has changed.
#[derive(Debug, Clone)]
pub struct CacheKey {
    pub size_bytes: u64,
    pub mtime:      String,
    pub data:       CachedFile,
}

/// Thread-safe SQLite cache for file hashes.
pub struct Cache {
    conn: Mutex<Connection>,
}

impl Cache {
    pub fn open(app_data_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(app_data_dir)?;
        let conn = Connection::open(app_data_dir.join("rustymirror_cache.db"))?;
        conn.execute_batch("
            PRAGMA journal_mode=WAL;
            PRAGMA synchronous=NORMAL;
            CREATE TABLE IF NOT EXISTS file_cache (
                path        TEXT    NOT NULL PRIMARY KEY,
                size_bytes  INTEGER NOT NULL,
                mtime       TEXT    NOT NULL,
                blake3      TEXT    NOT NULL,
                phash       TEXT,
                fast_phash  TEXT,
                header_hash TEXT,
                width       INTEGER NOT NULL DEFAULT 0,
                height      INTEGER NOT NULL DEFAULT 0,
                modified    TEXT    NOT NULL DEFAULT ''
            );
        ")?;
        // Migrate existing databases that lack newer columns.
        let _ = conn.execute_batch("ALTER TABLE file_cache ADD COLUMN fast_phash TEXT;");
        let _ = conn.execute_batch("ALTER TABLE file_cache ADD COLUMN header_hash TEXT;");
        Ok(Self { conn: Mutex::new(conn) })
    }

    /// Number of cached entries.
    pub fn count(&self) -> usize {
        let conn = self.conn.lock().unwrap();
        conn.query_row("SELECT COUNT(*) FROM file_cache", [], |r| r.get::<_, i64>(0))
            .unwrap_or(0) as usize
    }

    /// Load ALL cached entries for a list of paths in one query.
    /// Returns HashMap<path, CacheKey> — caller checks size+mtime to validate.
    pub fn get_bulk(&self, paths: &[String]) -> HashMap<String, CacheKey> {
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return HashMap::new(),
        };
        let mut result = HashMap::with_capacity(paths.len());

        // Process in chunks of 500 (SQLite variable limit)
        for chunk in paths.chunks(500) {
            let placeholders = (1..=chunk.len())
                .map(|i| format!("?{}", i))
                .collect::<Vec<_>>().join(",");
            let sql = format!(
                "SELECT path, size_bytes, mtime, blake3, phash, fast_phash, header_hash, width, height, modified
                 FROM file_cache WHERE path IN ({})", placeholders
            );
            let mut stmt = match conn.prepare(&sql) { Ok(s) => s, Err(_) => continue };

            let param_refs: Vec<&dyn rusqlite::types::ToSql> = chunk.iter()
                .map(|s| s as &dyn rusqlite::types::ToSql).collect();

            let rows: Vec<_> = match stmt.query_map(param_refs.as_slice(), |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)? as u64,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, i64>(7)? as u32,
                    row.get::<_, i64>(8)? as u32,
                    row.get::<_, String>(9)?,
                ))
            }) {
                Ok(r) => r.flatten().collect(),
                Err(_) => continue,
            };
            // stmt dropped here — safe to use rows
            for row in rows {
                result.insert(row.0.clone(), CacheKey {
                    size_bytes: row.1,
                    mtime:      row.2.clone(),
                    data: CachedFile {
                        blake3:      row.3,
                        phash:       row.4,
                        fast_phash:  row.5,
                        header_hash: row.6,
                        size_bytes:  row.1,
                        width:       row.7,
                        height:      row.8,
                        modified:    row.9,
                    },
                });
            }
        }
        result
    }

    #[allow(dead_code)]
    pub fn set(&self, path: &str, mtime: &str, e: &CachedFile) {
        if let Ok(conn) = self.conn.lock() {
            let _ = conn.execute(
                "INSERT OR REPLACE INTO file_cache
                 (path, size_bytes, mtime, blake3, phash, fast_phash, header_hash, width, height, modified)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                params![path, e.size_bytes as i64, mtime,
                        &e.blake3, &e.phash, &e.fast_phash, &e.header_hash,
                        e.width as i64, e.height as i64, &e.modified],
            );
        }
    }

    /// Batch-insert entries in a single transaction.
    pub fn put_batch(&self, entries: &[(String, String, CachedFile)]) -> Result<()> {
        if entries.is_empty() { return Ok(()); }
        let mut conn = self.conn.lock().map_err(|_| anyhow::anyhow!("mutex poisoned"))?;
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT OR REPLACE INTO file_cache
                 (path, size_bytes, mtime, blake3, phash, fast_phash, header_hash, width, height, modified)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)"
            )?;
            for (path, mtime, e) in entries {
                stmt.execute(params![
                    path, e.size_bytes as i64, mtime,
                    &e.blake3, &e.phash, &e.fast_phash, &e.header_hash,
                    e.width as i64, e.height as i64, &e.modified
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Remove cache entries for paths that were deleted from disk.
    pub fn evict_deleted(&self, paths: &[String]) {
        if let Ok(conn) = self.conn.lock() {
            for path in paths {
                let _ = conn.execute(
                    "DELETE FROM file_cache WHERE path = ?1",
                    params![path],
                );
            }
        }
    }
}
