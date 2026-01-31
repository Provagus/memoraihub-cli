//! Pending Queue - Local queue for remote KB writes
//!
//! When a remote KB has `write = "ask"`, writes are stored locally
//! in this queue until the user approves them. Only then are they
//! pushed to the remote server.

use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

/// Type of pending write operation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PendingWriteType {
    Add,
    Correct,
    Extend,
    Deprecate,
}

impl std::fmt::Display for PendingWriteType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PendingWriteType::Add => write!(f, "add"),
            PendingWriteType::Correct => write!(f, "correct"),
            PendingWriteType::Extend => write!(f, "extend"),
            PendingWriteType::Deprecate => write!(f, "deprecate"),
        }
    }
}

/// A pending write waiting for user approval
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingWrite {
    pub id: Ulid,
    /// Target KB name (e.g., "company", "public")
    pub target_kb: String,
    /// Target KB URL
    pub target_url: String,
    /// Type of operation
    pub write_type: PendingWriteType,
    /// Path for the fact
    pub path: String,
    /// Content of the fact
    pub content: String,
    /// Title (for add/correct/extend)
    pub title: Option<String>,
    /// Tags (JSON array)
    pub tags: Vec<String>,
    /// For corrections: ID of fact being superseded
    pub supersedes: Option<String>,
    /// For extensions: ID of fact being extended
    pub extends: Option<String>,
    /// For deprecations: reason
    pub reason: Option<String>,
    /// When queued
    pub created_at: DateTime<Utc>,
}

impl PendingWrite {
    pub fn new_add(
        target_kb: &str,
        target_url: &str,
        path: &str,
        content: &str,
        tags: Vec<String>,
    ) -> Self {
        let title = content
            .lines()
            .next()
            .unwrap_or(content)
            .chars()
            .take(50)
            .collect();

        Self {
            id: Ulid::new(),
            target_kb: target_kb.to_string(),
            target_url: target_url.to_string(),
            write_type: PendingWriteType::Add,
            path: path.to_string(),
            content: content.to_string(),
            title: Some(title),
            tags,
            supersedes: None,
            extends: None,
            reason: None,
            created_at: Utc::now(),
        }
    }

    pub fn new_correct(
        target_kb: &str,
        target_url: &str,
        path: &str,
        content: &str,
        supersedes: &str,
    ) -> Self {
        Self {
            id: Ulid::new(),
            target_kb: target_kb.to_string(),
            target_url: target_url.to_string(),
            write_type: PendingWriteType::Correct,
            path: path.to_string(),
            content: content.to_string(),
            title: Some(format!("Correction")),
            tags: vec![],
            supersedes: Some(supersedes.to_string()),
            extends: None,
            reason: None,
            created_at: Utc::now(),
        }
    }

    pub fn new_extend(
        target_kb: &str,
        target_url: &str,
        path: &str,
        content: &str,
        extends: &str,
    ) -> Self {
        Self {
            id: Ulid::new(),
            target_kb: target_kb.to_string(),
            target_url: target_url.to_string(),
            write_type: PendingWriteType::Extend,
            path: path.to_string(),
            content: content.to_string(),
            title: Some(format!("Extension")),
            tags: vec![],
            supersedes: None,
            extends: Some(extends.to_string()),
            reason: None,
            created_at: Utc::now(),
        }
    }

    pub fn new_deprecate(
        target_kb: &str,
        target_url: &str,
        fact_id: &str,
        reason: Option<&str>,
    ) -> Self {
        Self {
            id: Ulid::new(),
            target_kb: target_kb.to_string(),
            target_url: target_url.to_string(),
            write_type: PendingWriteType::Deprecate,
            path: fact_id.to_string(), // Store fact_id in path field
            content: String::new(),
            title: None,
            tags: vec![],
            supersedes: None,
            extends: None,
            reason: reason.map(|s| s.to_string()),
            created_at: Utc::now(),
        }
    }
}

/// Storage for pending writes queue
pub struct PendingQueue {
    conn: Connection,
}

impl PendingQueue {
    /// Open or create the pending queue database
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path).context("Failed to open pending queue database")?;

        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")?;

        let queue = Self { conn };
        queue.init_schema()?;

        Ok(queue)
    }

    /// Open in-memory database (for testing)
    pub fn open_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let queue = Self { conn };
        queue.init_schema()?;
        Ok(queue)
    }

    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS pending_writes (
                id TEXT PRIMARY KEY,
                target_kb TEXT NOT NULL,
                target_url TEXT NOT NULL,
                write_type TEXT NOT NULL,
                path TEXT NOT NULL,
                content TEXT NOT NULL,
                title TEXT,
                tags TEXT,
                supersedes TEXT,
                extends TEXT,
                reason TEXT,
                created_at TEXT NOT NULL
            );
            
            CREATE INDEX IF NOT EXISTS idx_pending_target_kb ON pending_writes(target_kb);
            CREATE INDEX IF NOT EXISTS idx_pending_created ON pending_writes(created_at);
            "#,
        )?;
        Ok(())
    }

    /// Add a pending write to the queue
    pub fn enqueue(&self, write: &PendingWrite) -> Result<()> {
        let tags_json = serde_json::to_string(&write.tags)?;

        self.conn.execute(
            r#"
            INSERT INTO pending_writes (
                id, target_kb, target_url, write_type, path, content,
                title, tags, supersedes, extends, reason, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            "#,
            params![
                write.id.to_string(),
                write.target_kb,
                write.target_url,
                write.write_type.to_string(),
                write.path,
                write.content,
                write.title,
                tags_json,
                write.supersedes,
                write.extends,
                write.reason,
                write.created_at.to_rfc3339(),
            ],
        )?;

        Ok(())
    }

    /// Get a pending write by ID
    pub fn get(&self, id: &Ulid) -> Result<Option<PendingWrite>> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM pending_writes WHERE id = ?1")?;

        let result = stmt.query_row([id.to_string()], |row| Self::row_to_pending_write(row));

        match result {
            Ok(write) => Ok(Some(write)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// List all pending writes
    pub fn list_all(&self) -> Result<Vec<PendingWrite>> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM pending_writes ORDER BY created_at DESC")?;

        let writes = stmt
            .query_map([], |row| Self::row_to_pending_write(row))?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(writes)
    }

    /// List pending writes for a specific KB
    pub fn list_for_kb(&self, kb_name: &str) -> Result<Vec<PendingWrite>> {
        let mut stmt = self.conn.prepare(
            "SELECT * FROM pending_writes WHERE target_kb = ?1 ORDER BY created_at DESC",
        )?;

        let writes = stmt
            .query_map([kb_name], |row| Self::row_to_pending_write(row))?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(writes)
    }

    /// Remove a pending write (after approval or rejection)
    pub fn remove(&self, id: &Ulid) -> Result<bool> {
        let deleted = self
            .conn
            .execute("DELETE FROM pending_writes WHERE id = ?1", [id.to_string()])?;

        Ok(deleted > 0)
    }

    /// Count pending writes
    pub fn count(&self) -> Result<usize> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM pending_writes", [], |row| row.get(0))?;

        Ok(count as usize)
    }

    /// Count pending writes for a specific KB
    pub fn count_for_kb(&self, kb_name: &str) -> Result<usize> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM pending_writes WHERE target_kb = ?1",
            [kb_name],
            |row| row.get(0),
        )?;

        Ok(count as usize)
    }

    fn row_to_pending_write(row: &rusqlite::Row) -> rusqlite::Result<PendingWrite> {
        let id_str: String = row.get("id")?;
        let write_type_str: String = row.get("write_type")?;
        let tags_json: String = row.get("tags")?;
        let created_str: String = row.get("created_at")?;

        Ok(PendingWrite {
            id: Ulid::from_string(&id_str).unwrap_or_else(|_| Ulid::new()),
            target_kb: row.get("target_kb")?,
            target_url: row.get("target_url")?,
            write_type: match write_type_str.as_str() {
                "add" => PendingWriteType::Add,
                "correct" => PendingWriteType::Correct,
                "extend" => PendingWriteType::Extend,
                "deprecate" => PendingWriteType::Deprecate,
                _ => PendingWriteType::Add,
            },
            path: row.get("path")?,
            content: row.get("content")?,
            title: row.get("title")?,
            tags: serde_json::from_str(&tags_json).unwrap_or_default(),
            supersedes: row.get("supersedes")?,
            extends: row.get("extends")?,
            reason: row.get("reason")?,
            created_at: chrono::DateTime::parse_from_rfc3339(&created_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enqueue_and_list() -> Result<()> {
        let queue = PendingQueue::open_memory()?;

        let write = PendingWrite::new_add(
            "company",
            "https://kb.company.com",
            "@project/idea",
            "My great idea",
            vec!["proposal".to_string()],
        );

        queue.enqueue(&write)?;

        let all = queue.list_all()?;
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].path, "@project/idea");
        assert_eq!(all[0].target_kb, "company");

        Ok(())
    }

    #[test]
    fn test_remove() -> Result<()> {
        let queue = PendingQueue::open_memory()?;

        let write = PendingWrite::new_add(
            "company",
            "https://kb.company.com",
            "@project/idea",
            "Content",
            vec![],
        );
        let id = write.id;

        queue.enqueue(&write)?;
        assert_eq!(queue.count()?, 1);

        queue.remove(&id)?;
        assert_eq!(queue.count()?, 0);

        Ok(())
    }

    #[test]
    fn test_list_for_kb() -> Result<()> {
        let queue = PendingQueue::open_memory()?;

        queue.enqueue(&PendingWrite::new_add("company", "url1", "@a", "A", vec![]))?;
        queue.enqueue(&PendingWrite::new_add("public", "url2", "@b", "B", vec![]))?;
        queue.enqueue(&PendingWrite::new_add("company", "url1", "@c", "C", vec![]))?;

        let company = queue.list_for_kb("company")?;
        assert_eq!(company.len(), 2);

        let public = queue.list_for_kb("public")?;
        assert_eq!(public.len(), 1);

        Ok(())
    }
}
