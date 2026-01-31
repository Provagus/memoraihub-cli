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
    path: Option<std::path::PathBuf>,
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

        // Enable WAL mode for better concurrent access
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")?;

        let storage = Self {
            conn,
            path: Some(path.to_path_buf()),
        };
        storage.init_schema()?;

        Ok(storage)
    }

    /// Open an in-memory database (for testing)
    pub fn open_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let storage = Self { conn, path: None };
        storage.init_schema()?;
        Ok(storage)
    }

    /// Clone by opening a new connection to the same database
    /// This is needed for async operations with spawn_blocking
    pub fn clone_connection(&self) -> Result<Self> {
        match &self.path {
            Some(path) => Self::open(path),
            None => Self::open_memory(),
        }
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
        let mut stmt = self.conn.prepare("SELECT * FROM facts WHERE id = ?1")?;

        let result = stmt.query_row([id.to_string()], |row| Self::row_to_fact(row));

        match result {
            Ok(fact) => Ok(Some(fact)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Get facts by path (exact match)
    pub fn get_by_path(&self, path: &str) -> Result<Vec<Fact>> {
        let mut stmt = self.conn.prepare(
            "SELECT * FROM facts WHERE path = ?1 AND status = 'active' ORDER BY created_at DESC",
        )?;

        let facts = stmt
            .query_map([path], |row| Self::row_to_fact(row))?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(facts)
    }

    /// Get facts by path prefix
    pub fn get_by_path_prefix(&self, prefix: &str) -> Result<Vec<Fact>> {
        let pattern = format!("{}%", prefix.trim_end_matches('/'));

        let mut stmt = self.conn.prepare(
            "SELECT * FROM facts WHERE path LIKE ?1 AND status = 'active' ORDER BY path, created_at DESC"
        )?;

        let facts = stmt
            .query_map([pattern], |row| Self::row_to_fact(row))?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(facts)
    }

    /// List child paths (for browse/ls) with pagination
    /// Returns (paths, has_more) tuple
    pub fn list_children(
        &self,
        parent: &str,
        limit: i64,
        cursor: Option<&str>,
    ) -> Result<(Vec<PathInfo>, bool)> {
        // Handle root "@" specially - it matches all paths starting with "@"
        // For "@meh", we want to find children like "@meh/architecture"
        let (prefix, pattern) = if parent.is_empty() || parent == "/" || parent == "@" {
            // Root: match all paths starting with @, group by first segment
            ("@".to_string(), "@%".to_string())
        } else {
            // Non-root: match paths that start with parent + "/"
            let p = format!("{}/", parent.trim_end_matches('/'));
            let pat = format!("{}%", p);
            (p, pat)
        };
        let cursor_path = cursor.unwrap_or("");

        // Fetch limit+1 to know if there are more results
        let fetch_limit = limit + 1;

        let mut stmt = self.conn.prepare(
            r#"
            SELECT 
                grouped_path,
                SUM(cnt) as count
            FROM (
                SELECT 
                    CASE 
                        WHEN instr(substr(path, length(?2) + 1), '/') > 0 
                        THEN substr(path, 1, length(?2) + instr(substr(path, length(?2) + 1), '/') - 1)
                        ELSE path
                    END as grouped_path,
                    COUNT(*) as cnt
                FROM facts 
                WHERE path LIKE ?1 AND status = 'active'
                GROUP BY path
            )
            WHERE grouped_path > ?3
            GROUP BY grouped_path
            ORDER BY grouped_path
            LIMIT ?4
            "#
        )?;

        let mut results: Vec<PathInfo> = stmt
            .query_map(
                rusqlite::params![&pattern, &prefix, cursor_path, fetch_limit],
                |row| {
                    Ok(PathInfo {
                        path: row.get(0)?,
                        fact_count: row.get::<_, i64>(1)? as usize,
                    })
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;

        // Check if there are more results
        let has_more = results.len() > limit as usize;
        if has_more {
            results.pop(); // Remove the extra item
        }

        Ok((results, has_more))
    }

    /// List child paths without pagination (for CLI backwards compatibility)
    pub fn list_children_all(&self, parent: &str) -> Result<Vec<PathInfo>> {
        let (results, _) = self.list_children(parent, 10000, None)?;
        Ok(results)
    }

    /// Escape and prepare query for FTS5
    /// Converts natural language query to FTS5 syntax with OR between words
    fn escape_fts_query(query: &str) -> String {
        // Split into words and quote each one (to handle special chars like hyphens)
        // Use OR so any matching word returns results (more forgiving for AI queries)
        let words: Vec<String> = query
            .split_whitespace()
            .filter(|w| !w.is_empty())
            .map(|w| {
                // Escape quotes and wrap in quotes to handle special chars
                let escaped = w.replace('"', "\"\"");
                format!("\"{}\"", escaped)
            })
            .collect();

        if words.is_empty() {
            return String::new();
        }

        // Join with OR - finds documents with ANY of the words
        // This is more forgiving for natural language queries from AI
        words.join(" OR ")
    }

    /// Full-text search
    pub fn search(&self, query: &str, limit: i64) -> Result<Vec<Fact>> {
        let fts_query = Self::escape_fts_query(query);

        let mut stmt = self.conn.prepare(
            r#"
            SELECT f.* 
            FROM facts f
            JOIN facts_fts fts ON f.id = fts.id
            WHERE facts_fts MATCH ?1 AND f.status = 'active'
            ORDER BY bm25(facts_fts, 0, 10.0, 5.0, 1.0, 1.0, 1.0)
            LIMIT ?2
            "#,
        )?;

        let facts = stmt
            .query_map(params![fts_query, limit as i32], |row| {
                Self::row_to_fact(row)
            })?
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

    /// Get history chain for a fact (follow supersedes backwards)
    /// Returns list of facts from oldest to newest
    pub fn get_history_chain(&self, id: &Ulid) -> Result<Vec<Fact>> {
        let mut chain = Vec::new();
        let mut current_id = Some(*id);

        // First, find the current fact and follow supersedes backwards
        while let Some(id) = current_id {
            if let Some(fact) = self.get_by_id(&id)? {
                chain.push(fact.clone());
                current_id = fact.supersedes;
            } else {
                break;
            }
        }

        // Reverse to get oldest first
        chain.reverse();
        Ok(chain)
    }

    /// Get all facts that supersede a given fact (follow supersedes forward)
    /// Returns list of facts from oldest to newest
    pub fn get_superseding_facts(&self, id: &Ulid) -> Result<Vec<Fact>> {
        let mut chain = Vec::new();

        // Find facts where supersedes = current id
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM facts WHERE supersedes = ?1")?;

        let mut current_facts: Vec<Fact> = stmt
            .query_map([id.to_string()], |row| Self::row_to_fact(row))?
            .filter_map(|r| r.ok())
            .collect();

        while !current_facts.is_empty() {
            // Take first (should only be one in proper chain)
            let fact = current_facts.remove(0);
            let next_id = fact.id;
            chain.push(fact);

            // Find next
            current_facts = stmt
                .query_map([next_id.to_string()], |row| Self::row_to_fact(row))?
                .filter_map(|r| r.ok())
                .collect();
        }

        Ok(chain)
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
                "pending_review" => Status::PendingReview,
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
        let total: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM facts", [], |row| row.get(0))?;

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

        let pending_review: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM facts WHERE status = 'pending_review'",
            [],
            |row| row.get(0),
        )?;

        Ok(StorageStats {
            total,
            total_facts: total as usize,
            active_facts: active as usize,
            deprecated_facts: deprecated as usize,
            pending_review_facts: pending_review as usize,
        })
    }

    /// Approve a pending_review fact (set status to active)
    pub fn approve_fact(&self, id: &Ulid) -> Result<()> {
        let updated = self.conn.execute(
            "UPDATE facts SET status = 'active', updated_at = ?2 WHERE id = ?1 AND status = 'pending_review'",
            params![id.to_string(), chrono::Utc::now().to_rfc3339()],
        )?;

        if updated == 0 {
            anyhow::bail!("Fact {} not found or not pending review", id);
        }
        Ok(())
    }

    /// Reject a pending_review fact (delete it)
    pub fn reject_fact(&self, id: &Ulid) -> Result<()> {
        let deleted = self.conn.execute(
            "DELETE FROM facts WHERE id = ?1 AND status = 'pending_review'",
            params![id.to_string()],
        )?;

        if deleted == 0 {
            anyhow::bail!("Fact {} not found or not pending review", id);
        }

        // Rebuild FTS
        self.conn
            .execute("INSERT INTO facts_fts(facts_fts) VALUES('rebuild')", [])?;
        Ok(())
    }

    /// Get all pending_review facts
    pub fn get_pending_review(&self) -> Result<Vec<Fact>> {
        let mut stmt = self.conn.prepare(
            "SELECT * FROM facts WHERE status = 'pending_review' ORDER BY created_at DESC",
        )?;

        let facts = stmt
            .query_map([], |row| Self::row_to_fact(row))?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(facts)
    }

    /// Garbage collect old deprecated/superseded facts
    ///
    /// Removes facts that are:
    /// - status = 'deprecated' AND updated_at older than retention_days
    /// - superseded_by IS NOT NULL (someone created a correction) AND updated_at older than retention_days
    ///
    /// # Arguments
    /// * `retention_days` - How many days to keep deprecated facts (default: 30)
    /// * `dry_run` - If true, only return candidates without deleting
    ///
    /// # Returns
    /// GcResult with count of deleted facts and list of candidates
    pub fn garbage_collect(&self, retention_days: u32, dry_run: bool) -> Result<GcResult> {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(retention_days as i64);
        let cutoff_str = cutoff.to_rfc3339();

        // Find candidates
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, path, status, updated_at,
                   CASE 
                       WHEN status = 'deprecated' THEN 'deprecated'
                       ELSE 'superseded'
                   END as reason
            FROM facts 
            WHERE (status = 'deprecated' OR id IN (
                SELECT supersedes FROM facts WHERE supersedes IS NOT NULL
            ))
            AND updated_at < ?1
            ORDER BY updated_at ASC
            "#,
        )?;

        let candidates: Vec<GcCandidate> = stmt
            .query_map([&cutoff_str], |row| {
                let reason_str: String = row.get(4)?;
                Ok(GcCandidate {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    reason: if reason_str == "deprecated" {
                        GcReason::Deprecated
                    } else {
                        GcReason::Superseded
                    },
                    updated_at: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        if dry_run {
            return Ok(GcResult {
                deleted_count: 0,
                candidates,
            });
        }

        // Actually delete (also from FTS)
        let deleted_count = self.conn.execute(
            r#"
            DELETE FROM facts 
            WHERE (status = 'deprecated' OR id IN (
                SELECT supersedes FROM facts WHERE supersedes IS NOT NULL
            ))
            AND updated_at < ?1
            "#,
            [&cutoff_str],
        )?;

        // Rebuild FTS index after deletion
        self.conn
            .execute("INSERT INTO facts_fts(facts_fts) VALUES('rebuild')", [])?;

        Ok(GcResult {
            deleted_count,
            candidates,
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
#[derive(Debug, Clone)]
pub struct StorageStats {
    pub total: i64,
    pub total_facts: usize,
    pub active_facts: usize,
    pub deprecated_facts: usize,
    pub pending_review_facts: usize,
}

/// Result of garbage collection
#[derive(Debug, Clone)]
pub struct GcResult {
    /// Number of facts deleted
    pub deleted_count: usize,
    /// Facts that would be deleted (dry-run)
    pub candidates: Vec<GcCandidate>,
}

/// A fact candidate for garbage collection
#[derive(Debug, Clone)]
pub struct GcCandidate {
    pub id: String,
    pub path: String,
    pub reason: GcReason,
    pub updated_at: String,
}

/// Why a fact is being garbage collected
#[derive(Debug, Clone)]
pub enum GcReason {
    Deprecated,
    Superseded,
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

    #[test]
    fn test_garbage_collect_deprecated() -> Result<()> {
        let storage = Storage::open_memory()?;

        // Create a deprecated fact with old timestamp
        let mut fact = Fact::new("@test/deprecated", "Old fact", "This is deprecated");
        fact.status = crate::core::fact::Status::Deprecated;
        fact.updated_at = chrono::Utc::now() - chrono::Duration::days(60);
        storage.insert(&fact)?;

        // Create an active fact
        storage.insert(&Fact::new("@test/active", "Active", "Keep this"))?;

        // Dry run should find the deprecated fact
        let dry_result = storage.garbage_collect(30, true)?;
        assert_eq!(dry_result.candidates.len(), 1);
        assert_eq!(dry_result.deleted_count, 0); // Dry run doesn't delete

        // Actually delete
        let result = storage.garbage_collect(30, false)?;
        assert_eq!(result.deleted_count, 1);

        // Active fact should still exist
        let stats = storage.stats()?;
        assert_eq!(stats.total_facts, 1);

        Ok(())
    }

    #[test]
    fn test_garbage_collect_respects_retention() -> Result<()> {
        let storage = Storage::open_memory()?;

        // Create a recently deprecated fact (should NOT be collected)
        let mut recent = Fact::new("@test/recent", "Recent", "Recently deprecated");
        recent.status = crate::core::fact::Status::Deprecated;
        recent.updated_at = chrono::Utc::now() - chrono::Duration::days(5);
        storage.insert(&recent)?;

        // Create an old deprecated fact (should be collected)
        let mut old = Fact::new("@test/old", "Old", "Long ago deprecated");
        old.status = crate::core::fact::Status::Deprecated;
        old.updated_at = chrono::Utc::now() - chrono::Duration::days(60);
        storage.insert(&old)?;

        let result = storage.garbage_collect(30, false)?;
        assert_eq!(result.deleted_count, 1); // Only old one

        // Recent should still exist (check by ID since search filters by status)
        let remaining = storage.get_by_id(&recent.id)?;
        assert!(remaining.is_some());

        // Old should be gone
        let deleted = storage.get_by_id(&old.id)?;
        assert!(deleted.is_none());

        Ok(())
    }

    #[test]
    fn test_garbage_collect_empty_db() -> Result<()> {
        let storage = Storage::open_memory()?;

        let result = storage.garbage_collect(30, false)?;
        assert_eq!(result.deleted_count, 0);
        assert!(result.candidates.is_empty());

        Ok(())
    }
}
