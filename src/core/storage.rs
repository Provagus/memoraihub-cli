//! Storage - SQLite backend
//!
//! Uses SQLite with FTS5 for full-text search.
//!
//! # Architecture
//! See `../../plan/ANALYSIS_STORAGE_ARCHITECTURE.md`
//!
//! # Key Points
//! - SQLite with FTS5 for search
//! - Append-only: no UPDATE, only INSERT
//! - Path index for fast prefix queries

use std::path::Path as FilePath;

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OpenFlags};
use ulid::Ulid;

use super::fact::{AuthorType, Fact, FactType, Status};

/// Database storage
pub struct Storage {
    conn: Connection,
}

impl Storage {
    /// Open or create a database
    pub fn open(path: &FilePath) -> Result<Self> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .context("Failed to open database")?;

        let storage = Self { conn };
        storage.init_schema()?;
        
        Ok(storage)
    }

    /// Open an in-memory database (for testing)
    pub fn open_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let storage = Self { conn };
        storage.init_schema()?;
        Ok(storage)
    }

    /// Initialize database schema
    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            -- Facts table
            CREATE TABLE IF NOT EXISTS facts (
                id TEXT PRIMARY KEY,
                path TEXT NOT NULL,
                title TEXT NOT NULL,
                content TEXT NOT NULL,
                summary TEXT,
                tags TEXT,  -- JSON array
                source TEXT NOT NULL DEFAULT 'local',
                namespace TEXT DEFAULT '',
                trust_score REAL NOT NULL DEFAULT 0.5,
                status TEXT NOT NULL DEFAULT 'active',
                fact_type TEXT NOT NULL DEFAULT 'fact',
                supersedes TEXT,
                extends TEXT,  -- JSON array
                author_type TEXT NOT NULL DEFAULT 'ai',
                author_id TEXT DEFAULT '',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                accessed_at TEXT
            );

            -- Path index for prefix queries
            CREATE INDEX IF NOT EXISTS idx_facts_path ON facts(path);
            
            -- Status index
            CREATE INDEX IF NOT EXISTS idx_facts_status ON facts(status);
            
            -- Supersedes index for finding chains
            CREATE INDEX IF NOT EXISTS idx_facts_supersedes ON facts(supersedes);

            -- FTS5 virtual table for full-text search
            CREATE VIRTUAL TABLE IF NOT EXISTS facts_fts USING fts5(
                id UNINDEXED,
                path,
                title,
                content,
                summary,
                tags,
                content='facts',
                content_rowid='rowid'
            );

            -- Triggers to keep FTS in sync
            CREATE TRIGGER IF NOT EXISTS facts_ai AFTER INSERT ON facts BEGIN
                INSERT INTO facts_fts(rowid, id, path, title, content, summary, tags)
                VALUES (new.rowid, new.id, new.path, new.title, new.content, new.summary, new.tags);
            END;

            -- Note: No UPDATE trigger needed - append-only model
            "#,
        )?;

        Ok(())
    }

    /// Insert a new fact
    pub fn insert(&self, fact: &Fact) -> Result<()> {
        let tags_json = serde_json::to_string(&fact.tags)?;
        let extends_json = serde_json::to_string(&fact.extends)?;

        self.conn.execute(
            r#"
            INSERT INTO facts (
                id, path, title, content, summary, tags, source, namespace,
                trust_score, status, fact_type, supersedes, extends,
                author_type, author_id, created_at, updated_at, accessed_at
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8,
                ?9, ?10, ?11, ?12, ?13,
                ?14, ?15, ?16, ?17, ?18
            )
            "#,
            params![
                fact.id.to_string(),
                fact.path,
                fact.title,
                fact.content,
                fact.summary,
                tags_json,
                format!("{}", fact.source),
                fact.namespace,
                fact.trust_score,
                format!("{:?}", fact.status).to_lowercase(),
                format!("{:?}", fact.fact_type).to_lowercase(),
                fact.supersedes.map(|u| u.to_string()),
                extends_json,
                format!("{:?}", fact.author_type).to_lowercase(),
                fact.author_id,
                fact.created_at.to_rfc3339(),
                fact.updated_at.to_rfc3339(),
                fact.accessed_at.map(|dt| dt.to_rfc3339()),
            ],
        )?;

        Ok(())
    }

    /// Get a fact by ID
    pub fn get_by_id(&self, id: &Ulid) -> Result<Option<Fact>> {
        let mut stmt = self.conn.prepare(
            "SELECT * FROM facts WHERE id = ?1"
        )?;

        let result = stmt.query_row([id.to_string()], |row| {
            Self::row_to_fact(row)
        });

        match result {
            Ok(fact) => Ok(Some(fact)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Get facts by path (exact match)
    pub fn get_by_path(&self, path: &str) -> Result<Vec<Fact>> {
        let mut stmt = self.conn.prepare(
            "SELECT * FROM facts WHERE path = ?1 AND status = 'active' ORDER BY created_at DESC"
        )?;

        let facts = stmt.query_map([path], |row| Self::row_to_fact(row))?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(facts)
    }

    /// Get facts by path prefix
    pub fn get_by_path_prefix(&self, prefix: &str) -> Result<Vec<Fact>> {
        let pattern = format!("{}%", prefix.trim_end_matches('/'));
        
        let mut stmt = self.conn.prepare(
            "SELECT * FROM facts WHERE path LIKE ?1 AND status = 'active' ORDER BY path, created_at DESC"
        )?;

        let facts = stmt.query_map([pattern], |row| Self::row_to_fact(row))?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(facts)
    }

    /// List child paths (for browse/ls)
    pub fn list_children(&self, parent: &str) -> Result<Vec<PathInfo>> {
        let prefix = if parent.is_empty() || parent == "/" {
            String::new()
        } else {
            format!("{}/", parent.trim_end_matches('/'))
        };

        let pattern = format!("{}%", prefix);
        
        let mut stmt = self.conn.prepare(
            r#"
            SELECT 
                path,
                COUNT(*) as count
            FROM facts 
            WHERE path LIKE ?1 AND status = 'active'
            GROUP BY 
                CASE 
                    WHEN instr(substr(path, length(?2) + 1), '/') > 0 
                    THEN substr(path, 1, length(?2) + instr(substr(path, length(?2) + 1), '/') - 1)
                    ELSE path
                END
            ORDER BY path
            "#
        )?;

        let results = stmt.query_map([&pattern, &prefix], |row| {
            Ok(PathInfo {
                path: row.get(0)?,
                fact_count: row.get::<_, i64>(1)? as usize,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

        Ok(results)
    }

    /// Full-text search
    pub fn search(&self, query: &str, limit: i64) -> Result<Vec<Fact>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT f.* 
            FROM facts f
            JOIN facts_fts fts ON f.id = fts.id
            WHERE facts_fts MATCH ?1 AND f.status = 'active'
            ORDER BY bm25(facts_fts, 0, 10.0, 5.0, 1.0, 1.0, 1.0)
            LIMIT ?2
            "#
        )?;

        let facts = stmt.query_map(params![query, limit as i32], |row| Self::row_to_fact(row))?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(facts)
    }

    /// Mark a fact as superseded
    pub fn mark_superseded(&self, id: &Ulid) -> Result<()> {
        // Note: This is one of the few "updates" allowed - status change
        self.conn.execute(
            "UPDATE facts SET status = 'superseded', updated_at = ?2 WHERE id = ?1",
            params![id.to_string(), chrono::Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    /// Mark a fact as deprecated
    pub fn mark_deprecated(&self, id: &Ulid) -> Result<()> {
        self.conn.execute(
            "UPDATE facts SET status = 'deprecated', updated_at = ?2 WHERE id = ?1",
            params![id.to_string(), chrono::Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    /// Convert a database row to a Fact
    fn row_to_fact(row: &rusqlite::Row) -> rusqlite::Result<Fact> {
        let id_str: String = row.get("id")?;
        let tags_json: String = row.get("tags")?;
        let extends_json: String = row.get("extends")?;
        let source_str: String = row.get("source")?;
        let status_str: String = row.get("status")?;
        let fact_type_str: String = row.get("fact_type")?;
        let author_type_str: String = row.get("author_type")?;
        let created_str: String = row.get("created_at")?;
        let updated_str: String = row.get("updated_at")?;
        let accessed_str: Option<String> = row.get("accessed_at")?;
        let supersedes_str: Option<String> = row.get("supersedes")?;

        Ok(Fact {
            id: Ulid::from_string(&id_str).unwrap_or_else(|_| Ulid::new()),
            path: row.get("path")?,
            title: row.get("title")?,
            content: row.get("content")?,
            summary: row.get("summary")?,
            tags: serde_json::from_str(&tags_json).unwrap_or_default(),
            source: source_str.parse().unwrap_or_default(),
            namespace: row.get("namespace")?,
            trust_score: row.get("trust_score")?,
            status: match status_str.as_str() {
                "active" => Status::Active,
                "superseded" => Status::Superseded,
                "deprecated" => Status::Deprecated,
                "archived" => Status::Archived,
                _ => Status::Active,
            },
            fact_type: match fact_type_str.as_str() {
                "fact" => FactType::Fact,
                "correction" => FactType::Correction,
                "extension" => FactType::Extension,
                "warning" => FactType::Warning,
                "deprecation" => FactType::Deprecation,
                _ => FactType::Fact,
            },
            supersedes: supersedes_str.and_then(|s| Ulid::from_string(&s).ok()),
            extends: serde_json::from_str(&extends_json).unwrap_or_default(),
            author_type: match author_type_str.as_str() {
                "human" => AuthorType::Human,
                "ai" => AuthorType::Ai,
                "system" => AuthorType::System,
                _ => AuthorType::Ai,
            },
            author_id: row.get("author_id")?,
            created_at: chrono::DateTime::parse_from_rfc3339(&created_str)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
            updated_at: chrono::DateTime::parse_from_rfc3339(&updated_str)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
            accessed_at: accessed_str.and_then(|s| {
                chrono::DateTime::parse_from_rfc3339(&s)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .ok()
            }),
        })
    }

    /// Get database statistics
    pub fn stats(&self) -> Result<StorageStats> {
        let total: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM facts",
            [],
            |row| row.get(0),
        )?;

        let active: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM facts WHERE status = 'active'",
            [],
            |row| row.get(0),
        )?;

        let deprecated: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM facts WHERE status = 'deprecated'",
            [],
            |row| row.get(0),
        )?;

        Ok(StorageStats {
            total_facts: total as usize,
            active_facts: active as usize,
            deprecated_facts: deprecated as usize,
        })
    }
}

/// Path information for listing
#[derive(Debug)]
pub struct PathInfo {
    pub path: String,
    pub fact_count: usize,
}

/// Storage statistics
#[derive(Debug)]
pub struct StorageStats {
    pub total_facts: usize,
    pub active_facts: usize,
    pub deprecated_facts: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_query() -> Result<()> {
        let storage = Storage::open_memory()?;

        let fact = Fact::new(
            "@products/alpha/api/timeout",
            "API Timeout",
            "API timeout is 30 seconds.",
        );

        storage.insert(&fact)?;

        let retrieved = storage.get_by_id(&fact.id)?;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().title, "API Timeout");

        Ok(())
    }

    #[test]
    fn test_search() -> Result<()> {
        let storage = Storage::open_memory()?;

        storage.insert(&Fact::new(
            "@products/alpha/api/timeout",
            "API Timeout",
            "API timeout is 30 seconds.",
        ))?;

        storage.insert(&Fact::new(
            "@products/beta/api/timeout",
            "Beta API Timeout",
            "Beta API timeout is 60 seconds.",
        ))?;

        let results = storage.search("timeout", 10)?;
        assert_eq!(results.len(), 2);

        Ok(())
    }

    #[test]
    fn test_path_prefix() -> Result<()> {
        let storage = Storage::open_memory()?;

        storage.insert(&Fact::new("@products/alpha/api/timeout", "T1", "C1"))?;
        storage.insert(&Fact::new("@products/alpha/api/rate", "T2", "C2"))?;
        storage.insert(&Fact::new("@products/beta/api/timeout", "T3", "C3"))?;

        let alpha_facts = storage.get_by_path_prefix("@products/alpha")?;
        assert_eq!(alpha_facts.len(), 2);

        Ok(())
    }
}
