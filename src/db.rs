use chrono::{DateTime, Local, TimeZone};
use rusqlite::{params, Connection, Result};
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct ClipEntry {
    pub id: i64,
    pub content: String,
    pub hash: String,
    pub timestamp: DateTime<Local>,
    pub app_name: Option<String>,
}

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS clips (
                id        INTEGER PRIMARY KEY AUTOINCREMENT,
                content   TEXT NOT NULL,
                hash      TEXT NOT NULL,
                timestamp INTEGER NOT NULL,
                app_name  TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_clips_hash ON clips(hash);
            CREATE INDEX IF NOT EXISTS idx_clips_ts ON clips(timestamp DESC);
            CREATE INDEX IF NOT EXISTS idx_clips_content ON clips(content);",
        )?;
        Ok(Store { conn })
    }

    /// Insert a new clip. If the same content already exists, move it to the top
    /// by updating its timestamp. Returns the entry id.
    pub fn insert(&self, content: &str, app_name: Option<&str>) -> Result<i64> {
        let hash = content_hash(content);
        let now = Local::now().timestamp();

        // Check if this exact content already exists
        let existing: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM clips WHERE hash = ?1 LIMIT 1",
                params![hash],
                |row| row.get(0),
            )
            .ok();

        if let Some(id) = existing {
            // Move to top by updating timestamp
            self.conn.execute(
                "UPDATE clips SET timestamp = ?1 WHERE id = ?2",
                params![now, id],
            )?;
            Ok(id)
        } else {
            self.conn.execute(
                "INSERT INTO clips (content, hash, timestamp, app_name) VALUES (?1, ?2, ?3, ?4)",
                params![content, hash, now, app_name],
            )?;
            Ok(self.conn.last_insert_rowid())
        }
    }

    pub fn recent(&self, limit: usize) -> Result<Vec<ClipEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, content, hash, timestamp, app_name FROM clips ORDER BY timestamp DESC LIMIT ?1",
        )?;
        let entries = stmt
            .query_map(params![limit as i64], |row| {
                Ok(ClipEntry {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    hash: row.get(2)?,
                    timestamp: Local.timestamp_opt(row.get::<_, i64>(3)?, 0).unwrap(),
                    app_name: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>>>()?;
        Ok(entries)
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<ClipEntry>> {
        let pattern = format!("%{query}%");
        let mut stmt = self.conn.prepare(
            "SELECT id, content, hash, timestamp, app_name FROM clips WHERE content LIKE ?1 ORDER BY timestamp DESC LIMIT ?2",
        )?;
        let entries = stmt
            .query_map(params![pattern, limit as i64], |row| {
                Ok(ClipEntry {
                    id: row.get(0)?,
                    content: row.get(1)?,
                    hash: row.get(2)?,
                    timestamp: Local.timestamp_opt(row.get::<_, i64>(3)?, 0).unwrap(),
                    app_name: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>>>()?;
        Ok(entries)
    }

    pub fn get(&self, id: i64) -> Result<Option<ClipEntry>> {
        self.conn
            .query_row(
                "SELECT id, content, hash, timestamp, app_name FROM clips WHERE id = ?1",
                params![id],
                |row| {
                    Ok(ClipEntry {
                        id: row.get(0)?,
                        content: row.get(1)?,
                        hash: row.get(2)?,
                        timestamp: Local.timestamp_opt(row.get::<_, i64>(3)?, 0).unwrap(),
                        app_name: row.get(4)?,
                    })
                },
            )
            .optional()
    }

    pub fn delete(&self, id: i64) -> Result<bool> {
        let affected = self
            .conn
            .execute("DELETE FROM clips WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    pub fn clear(&self) -> Result<usize> {
        let count: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM clips", [], |row| row.get(0))?;
        self.conn.execute("DELETE FROM clips", [])?;
        Ok(count as usize)
    }

    pub fn all(&self) -> Result<Vec<ClipEntry>> {
        self.recent(10000)
    }
}

fn content_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}

trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>>;
}

impl<T> OptionalExt<T> for Result<T> {
    fn optional(self) -> Result<Option<T>> {
        match self {
            Ok(val) => Ok(Some(val)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
