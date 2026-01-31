//! Notifications module - Push-Pull hybrid notification system
//!
//! Stores pending notifications from remote sources and local events.
//! Supports multiple AI sessions with independent read cursors and subscriptions.
//!
//! # Key Concepts
//!
//! - **Category**: Notifications are grouped by category (facts, ci, security, etc.)
//! - **Session**: Each AI session has its own cursor tracking what it has read
//! - **Subscription**: AI subscribes to categories and/or path prefixes it cares about

use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use ulid::Ulid;

/// Notification priority level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Normal = 0,
    High = 1,
    Critical = 2,
}

impl Priority {
    pub fn as_str(&self) -> &'static str {
        match self {
            Priority::Normal => "normal",
            Priority::High => "high",
            Priority::Critical => "critical",
        }
    }

    pub fn parse_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "normal" => Some(Priority::Normal),
            "high" => Some(Priority::High),
            "critical" => Some(Priority::Critical),
            _ => None,
        }
    }
}

impl std::fmt::Display for Priority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Notification category - what topic this notification is about
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Category {
    /// Fact changes (new, corrected, deprecated, extended)
    Facts,
    /// CI/CD events (build, deploy, test)
    Ci,
    /// Security alerts
    Security,
    /// Documentation updates
    Docs,
    /// System alerts (storage, performance)
    System,
    /// Custom category
    Custom(String),
}

impl Category {
    pub fn as_str(&self) -> &str {
        match self {
            Category::Facts => "facts",
            Category::Ci => "ci",
            Category::Security => "security",
            Category::Docs => "docs",
            Category::System => "system",
            Category::Custom(s) => s,
        }
    }

    pub fn parse_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "facts" => Category::Facts,
            "ci" => Category::Ci,
            "security" => Category::Security,
            "docs" => Category::Docs,
            "system" => Category::System,
            other => Category::Custom(other.to_string()),
        }
    }
}

impl std::fmt::Display for Category {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Notification type - what happened
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationType {
    /// New fact added
    NewFact,
    /// Existing fact was corrected
    Correction,
    /// Fact was deprecated
    Deprecation,
    /// Fact was extended
    Extension,
    /// External alert (CI/CD, monitoring)
    Alert,
    /// Subscription match
    Match,
}

impl NotificationType {
    pub fn as_str(&self) -> &'static str {
        match self {
            NotificationType::NewFact => "new_fact",
            NotificationType::Correction => "correction",
            NotificationType::Deprecation => "deprecation",
            NotificationType::Extension => "extension",
            NotificationType::Alert => "alert",
            NotificationType::Match => "match",
        }
    }

    pub fn parse_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "new_fact" => Some(NotificationType::NewFact),
            "correction" => Some(NotificationType::Correction),
            "deprecation" => Some(NotificationType::Deprecation),
            "extension" => Some(NotificationType::Extension),
            "alert" => Some(NotificationType::Alert),
            "match" => Some(NotificationType::Match),
            _ => None,
        }
    }
}

impl std::fmt::Display for NotificationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A notification about a knowledge base change
#[derive(Debug, Clone)]
pub struct Notification {
    pub id: Ulid,
    pub category: Category,
    pub priority: Priority,
    pub source: String,
    pub notification_type: NotificationType,
    pub fact_id: Option<Ulid>,
    pub path: Option<String>,
    pub title: String,
    pub summary: String,
    pub created_at: DateTime<Utc>,
}

impl Notification {
    /// Create a new notification
    pub fn new(
        category: Category,
        priority: Priority,
        source: impl Into<String>,
        notification_type: NotificationType,
        title: impl Into<String>,
        summary: impl Into<String>,
    ) -> Self {
        Self {
            id: Ulid::new(),
            category,
            priority,
            source: source.into(),
            notification_type,
            fact_id: None,
            path: None,
            title: title.into(),
            summary: summary.into(),
            created_at: Utc::now(),
        }
    }

    /// Set related fact ID and path
    pub fn with_fact(mut self, fact_id: Ulid, path: impl Into<String>) -> Self {
        self.fact_id = Some(fact_id);
        self.path = Some(path.into());
        self
    }

    /// Create notification for new fact
    pub fn for_new_fact(fact_id: Ulid, path: &str, title: &str, source: &str) -> Self {
        Self::new(
            Category::Facts,
            Priority::Normal,
            source,
            NotificationType::NewFact,
            format!("New: {}", title),
            format!("New fact added at {}", path),
        )
        .with_fact(fact_id, path)
    }

    /// Create notification for correction
    pub fn for_correction(fact_id: Ulid, path: &str, source: &str) -> Self {
        Self::new(
            Category::Facts,
            Priority::High,
            source,
            NotificationType::Correction,
            format!("Corrected: {}", path),
            "A fact was corrected".to_string(),
        )
        .with_fact(fact_id, path)
    }

    /// Create notification for deprecation
    pub fn for_deprecation(fact_id: Ulid, path: &str, reason: &str, source: &str) -> Self {
        Self::new(
            Category::Facts,
            Priority::High,
            source,
            NotificationType::Deprecation,
            format!("Deprecated: {}", path),
            reason.to_string(),
        )
        .with_fact(fact_id, path)
    }

    /// Create notification for CI event
    pub fn for_ci(title: &str, summary: &str, priority: Priority) -> Self {
        Self::new(
            Category::Ci,
            priority,
            "ci",
            NotificationType::Alert,
            title,
            summary,
        )
    }

    /// Create notification for security alert
    pub fn for_security(title: &str, summary: &str) -> Self {
        Self::new(
            Category::Security,
            Priority::Critical,
            "security",
            NotificationType::Alert,
            title,
            summary,
        )
    }

    /// Create notification for system alert
    pub fn for_system(title: &str, summary: &str, priority: Priority) -> Self {
        Self::new(
            Category::System,
            priority,
            "system",
            NotificationType::Alert,
            title,
            summary,
        )
    }
}

/// Session subscription - what a session wants to receive
#[derive(Debug, Clone, Default)]
pub struct Subscription {
    /// Categories to subscribe to (empty = all)
    pub categories: Vec<Category>,
    /// Path prefixes to subscribe to (empty = all)
    pub path_prefixes: Vec<String>,
    /// Minimum priority (None = all)
    pub priority_min: Option<Priority>,
}

impl Subscription {
    /// Subscribe to specific categories
    pub fn categories(mut self, cats: Vec<Category>) -> Self {
        self.categories = cats;
        self
    }

    /// Subscribe to specific path prefixes
    pub fn paths(mut self, paths: Vec<String>) -> Self {
        self.path_prefixes = paths;
        self
    }

    /// Set minimum priority
    pub fn priority_min(mut self, priority: Priority) -> Self {
        self.priority_min = Some(priority);
        self
    }

    /// Check if notification matches subscription
    pub fn matches(&self, notif: &Notification) -> bool {
        // Check priority
        if let Some(min) = self.priority_min {
            if notif.priority < min {
                return false;
            }
        }

        // Check category (empty = all)
        if !self.categories.is_empty() {
            let cat_matches = self
                .categories
                .iter()
                .any(|c| c.as_str() == notif.category.as_str());
            if !cat_matches {
                return false;
            }
        }

        // Check path prefix (empty = all)
        if !self.path_prefixes.is_empty() {
            if let Some(ref path) = notif.path {
                let path_matches = self.path_prefixes.iter().any(|p| path.starts_with(p));
                if !path_matches {
                    return false;
                }
            } else {
                // No path on notification, but we have path filters - no match
                return false;
            }
        }

        true
    }

    /// Serialize to JSON string for storage
    pub fn to_json(&self) -> String {
        let cats: Vec<&str> = self.categories.iter().map(|c| c.as_str()).collect();
        let priority = self.priority_min.map(|p| p.as_str());
        serde_json::json!({
            "categories": cats,
            "path_prefixes": self.path_prefixes,
            "priority_min": priority
        })
        .to_string()
    }

    /// Deserialize from JSON string
    pub fn from_json(s: &str) -> Result<Self> {
        let v: serde_json::Value = serde_json::from_str(s)?;

        let categories = v["categories"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(Category::parse_str))
                    .collect()
            })
            .unwrap_or_default();

        let path_prefixes = v["path_prefixes"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let priority_min = v["priority_min"].as_str().and_then(Priority::parse_str);

        Ok(Self {
            categories,
            path_prefixes,
            priority_min,
        })
    }
}

/// Notification storage with session support
pub struct NotificationStorage {
    conn: Connection,
}

impl NotificationStorage {
    /// Open or create notification database
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        let storage = Self { conn };
        storage.init_schema()?;
        Ok(storage)
    }

    /// Create in-memory database (for testing)
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let storage = Self { conn };
        storage.init_schema()?;
        Ok(storage)
    }

    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA busy_timeout = 5000;
            
            -- Notifications table
            CREATE TABLE IF NOT EXISTS notifications (
                id TEXT PRIMARY KEY,
                category TEXT NOT NULL,
                priority INTEGER NOT NULL,
                source TEXT NOT NULL,
                notification_type TEXT NOT NULL,
                fact_id TEXT,
                path TEXT,
                title TEXT NOT NULL,
                summary TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            
            CREATE INDEX IF NOT EXISTS idx_notifications_created 
                ON notifications(created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_notifications_category 
                ON notifications(category);
            CREATE INDEX IF NOT EXISTS idx_notifications_priority 
                ON notifications(priority DESC);
            CREATE INDEX IF NOT EXISTS idx_notifications_path 
                ON notifications(path);
            
            -- Sessions table - tracks each AI session's read position
            CREATE TABLE IF NOT EXISTS sessions (
                session_id TEXT PRIMARY KEY,
                last_seen_id TEXT,
                subscription TEXT NOT NULL DEFAULT '{}',
                onboarding_shown INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                last_active TEXT NOT NULL
            );

            -- Migration: add onboarding_shown column if missing
            -- SQLite doesn't support ADD COLUMN IF NOT EXISTS, so we check first
            "#,
        )?;

        // Check if onboarding_shown column exists, add if not
        let has_onboarding: bool = self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name='onboarding_shown'",
                [],
                |row| Ok(row.get::<_, i64>(0)? > 0),
            )
            .unwrap_or(false);

        if !has_onboarding {
            let _ = self.conn.execute(
                "ALTER TABLE sessions ADD COLUMN onboarding_shown INTEGER NOT NULL DEFAULT 0",
                [],
            );
        }
        Ok(())
    }

    /// Add a notification
    pub fn add(&self, notification: &Notification) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO notifications 
                (id, category, priority, source, notification_type, fact_id, path, title, summary, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            "#,
            params![
                notification.id.to_string(),
                notification.category.as_str(),
                notification.priority as i32,
                notification.source,
                notification.notification_type.as_str(),
                notification.fact_id.map(|id| id.to_string()),
                notification.path,
                notification.title,
                notification.summary,
                notification.created_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    /// Get or create a session
    pub fn get_or_create_session(&self, session_id: &str) -> Result<(Option<Ulid>, Subscription)> {
        let now = Utc::now().to_rfc3339();

        // Try to get existing session
        let result: Option<(Option<String>, String)> = self
            .conn
            .query_row(
                "SELECT last_seen_id, subscription FROM sessions WHERE session_id = ?1",
                params![session_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .ok();

        if let Some((last_seen_str, sub_json)) = result {
            // Update last_active
            self.conn.execute(
                "UPDATE sessions SET last_active = ?1 WHERE session_id = ?2",
                params![&now, session_id],
            )?;

            let last_seen = last_seen_str.and_then(|s| Ulid::from_string(&s).ok());
            let subscription = Subscription::from_json(&sub_json).unwrap_or_default();

            Ok((last_seen, subscription))
        } else {
            // Create new session
            self.conn.execute(
                "INSERT INTO sessions (session_id, subscription, created_at, last_active) VALUES (?1, ?2, ?3, ?4)",
                params![session_id, "{}", &now, &now],
            )?;

            Ok((None, Subscription::default()))
        }
    }

    /// Update session's subscription
    pub fn update_subscription(&self, session_id: &str, subscription: &Subscription) -> Result<()> {
        let now = Utc::now().to_rfc3339();

        // Ensure session exists
        let _ = self.get_or_create_session(session_id)?;

        self.conn.execute(
            "UPDATE sessions SET subscription = ?1, last_active = ?2 WHERE session_id = ?3",
            params![subscription.to_json(), &now, session_id],
        )?;

        Ok(())
    }

    /// Mark notifications as seen for a session (update cursor)
    pub fn mark_seen(&self, session_id: &str, last_seen_id: &Ulid) -> Result<()> {
        let now = Utc::now().to_rfc3339();

        // Ensure session exists
        let _ = self.get_or_create_session(session_id)?;

        self.conn.execute(
            "UPDATE sessions SET last_seen_id = ?1, last_active = ?2 WHERE session_id = ?3",
            params![last_seen_id.to_string(), &now, session_id],
        )?;

        Ok(())
    }

    /// Get new notifications for a session (respecting its subscription)
    pub fn get_for_session(&self, session_id: &str, limit: usize) -> Result<Vec<Notification>> {
        let (last_seen, subscription) = self.get_or_create_session(session_id)?;

        // Build query based on last_seen cursor
        let mut sql = String::from(
            "SELECT id, category, priority, source, notification_type, fact_id, path, title, summary, created_at
             FROM notifications WHERE 1=1"
        );

        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![];

        // Only get notifications newer than last_seen
        if let Some(last_id) = last_seen {
            sql.push_str(" AND id > ?");
            params_vec.push(Box::new(last_id.to_string()));
        }

        sql.push_str(" ORDER BY id ASC LIMIT ?");
        params_vec.push(Box::new(limit as i64));

        let params_refs: Vec<&dyn rusqlite::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();

        let mut stmt = self.conn.prepare(&sql)?;
        let all_notifications: Vec<Notification> = stmt
            .query_map(params_refs.as_slice(), |row| self.row_to_notification(row))?
            .collect::<Result<Vec<_>, _>>()
            .context("Failed to fetch notifications")?;

        // Filter by subscription
        let matching: Vec<Notification> = all_notifications
            .into_iter()
            .filter(|n| subscription.matches(n))
            .collect();

        Ok(matching)
    }

    /// Get pending count for a session
    pub fn pending_count(&self, session_id: &str) -> Result<usize> {
        let (last_seen, subscription) = self.get_or_create_session(session_id)?;

        let mut sql = String::from("SELECT COUNT(*) FROM notifications WHERE 1=1");
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![];

        if let Some(last_id) = last_seen {
            sql.push_str(" AND id > ?");
            params_vec.push(Box::new(last_id.to_string()));
        }

        // Category filter
        if !subscription.categories.is_empty() {
            let placeholders: Vec<String> = subscription
                .categories
                .iter()
                .enumerate()
                .map(|(i, _)| format!("?{}", params_vec.len() + i + 1))
                .collect();
            sql.push_str(&format!(" AND category IN ({})", placeholders.join(",")));
            for cat in &subscription.categories {
                params_vec.push(Box::new(cat.as_str().to_string()));
            }
        }

        // Priority filter
        if let Some(min_p) = subscription.priority_min {
            sql.push_str(" AND priority >= ?");
            params_vec.push(Box::new(min_p as i32));
        }

        let params_refs: Vec<&dyn rusqlite::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();

        let count: i64 = self
            .conn
            .query_row(&sql, params_refs.as_slice(), |row| row.get(0))?;
        Ok(count as usize)
    }

    /// Alias for pending_count (used by CLI hint)
    pub fn count_pending_for_session(&self, session_id: &str) -> Result<usize> {
        self.pending_count(session_id)
    }

    /// Check if onboarding (@readme) was shown for this session
    pub fn is_onboarding_shown(&self, session_id: &str) -> Result<bool> {
        // Ensure session exists
        let _ = self.get_or_create_session(session_id)?;

        let shown: i64 = self
            .conn
            .query_row(
                "SELECT onboarding_shown FROM sessions WHERE session_id = ?1",
                params![session_id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        Ok(shown != 0)
    }

    /// Mark onboarding as shown for this session
    pub fn set_onboarding_shown(&self, session_id: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();

        // Ensure session exists
        let _ = self.get_or_create_session(session_id)?;

        self.conn.execute(
            "UPDATE sessions SET onboarding_shown = 1, last_active = ?1 WHERE session_id = ?2",
            params![&now, session_id],
        )?;

        Ok(())
    }

    /// Get critical pending count for a session
    pub fn critical_count(&self, session_id: &str) -> Result<usize> {
        let (last_seen, _) = self.get_or_create_session(session_id)?;

        let count: i64 = if let Some(last_id) = last_seen {
            self.conn.query_row(
                "SELECT COUNT(*) FROM notifications WHERE id > ?1 AND priority = ?2",
                params![last_id.to_string(), Priority::Critical as i32],
                |row| row.get(0),
            )?
        } else {
            self.conn.query_row(
                "SELECT COUNT(*) FROM notifications WHERE priority = ?1",
                params![Priority::Critical as i32],
                |row| row.get(0),
            )?
        };

        Ok(count as usize)
    }

    /// Global unread count (for CLI, ignores sessions)
    pub fn unread_count(&self) -> Result<usize> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM notifications", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    /// Acknowledge all for a session (mark everything as seen)
    pub fn acknowledge_all(&self, session_id: &str) -> Result<usize> {
        // Get latest notification ID
        let latest: Option<String> = self
            .conn
            .query_row(
                "SELECT id FROM notifications ORDER BY id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .ok();

        if let Some(latest_id) = latest {
            if let Ok(ulid) = Ulid::from_string(&latest_id) {
                let count = self.pending_count(session_id)?;
                self.mark_seen(session_id, &ulid)?;
                return Ok(count);
            }
        }

        Ok(0)
    }

    /// Clear old notifications (global cleanup)
    pub fn clear_old(&self, older_than_days: i64) -> Result<usize> {
        let cutoff = Utc::now() - chrono::Duration::days(older_than_days);
        let rows = self.conn.execute(
            "DELETE FROM notifications WHERE created_at < ?1",
            params![cutoff.to_rfc3339()],
        )?;
        Ok(rows)
    }

    /// List available categories with counts
    pub fn list_categories(&self) -> Result<Vec<(String, usize)>> {
        let mut stmt = self.conn.prepare(
            "SELECT category, COUNT(*) as cnt FROM notifications GROUP BY category ORDER BY cnt DESC"
        )?;

        let categories = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(categories)
    }

    fn row_to_notification(&self, row: &rusqlite::Row) -> rusqlite::Result<Notification> {
        let id_str: String = row.get(0)?;
        let category_str: String = row.get(1)?;
        let priority_int: i32 = row.get(2)?;
        let source: String = row.get(3)?;
        let type_str: String = row.get(4)?;
        let fact_id_str: Option<String> = row.get(5)?;
        let path: Option<String> = row.get(6)?;
        let title: String = row.get(7)?;
        let summary: String = row.get(8)?;
        let created_str: String = row.get(9)?;

        let id = Ulid::from_string(&id_str).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })?;

        let category = Category::parse_str(&category_str);

        let priority = match priority_int {
            0 => Priority::Normal,
            1 => Priority::High,
            _ => Priority::Critical,
        };

        let notification_type =
            NotificationType::parse_str(&type_str).unwrap_or(NotificationType::Alert);

        let fact_id = fact_id_str.and_then(|s| Ulid::from_string(&s).ok());

        let created_at = DateTime::parse_from_rfc3339(&created_str)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        Ok(Notification {
            id,
            category,
            priority,
            source,
            notification_type,
            fact_id,
            path,
            title,
            summary,
            created_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_create() {
        let notif = Notification::new(
            Category::System,
            Priority::High,
            "test",
            NotificationType::Alert,
            "Test Alert",
            "This is a test",
        );

        assert_eq!(notif.category.as_str(), "system");
        assert_eq!(notif.priority, Priority::High);
        assert_eq!(notif.source, "test");
    }

    #[test]
    fn test_session_tracking() {
        let storage = NotificationStorage::in_memory().unwrap();

        // Add notifications with deterministic ordering
        // ULIDs in the same millisecond may have random order, so we use sleep
        let n1 = Notification::for_new_fact(Ulid::new(), "@test/a", "Fact A", "local");
        storage.add(&n1).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let n2 = Notification::for_new_fact(Ulid::new(), "@test/b", "Fact B", "local");
        storage.add(&n2).unwrap();

        // Session 1 gets both
        let pending1 = storage.get_for_session("session1", 10).unwrap();
        assert_eq!(pending1.len(), 2);

        // Session 1 marks n2 (the latest) as seen
        storage.mark_seen("session1", &n2.id).unwrap();

        // Session 1 gets nothing new (both n1 and n2 are <= n2)
        let pending1_after = storage.get_for_session("session1", 10).unwrap();
        assert_eq!(pending1_after.len(), 0);

        // Session 2 (new) still gets both
        let pending2 = storage.get_for_session("session2", 10).unwrap();
        assert_eq!(pending2.len(), 2);
    }

    #[test]
    fn test_subscription_filtering() {
        let storage = NotificationStorage::in_memory().unwrap();

        // Add mixed notifications
        storage
            .add(&Notification::for_new_fact(
                Ulid::new(),
                "@products/alpha",
                "Alpha",
                "local",
            ))
            .unwrap();
        storage
            .add(&Notification::for_ci(
                "Build OK",
                "Success",
                Priority::Normal,
            ))
            .unwrap();
        storage
            .add(&Notification::for_security("Vuln found", "Critical CVE"))
            .unwrap();

        // Subscribe only to facts category
        let sub = Subscription::default().categories(vec![Category::Facts]);
        storage.update_subscription("session1", &sub).unwrap();

        let pending = storage.get_for_session("session1", 10).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].category.as_str(), "facts");
    }

    #[test]
    fn test_path_subscription() {
        let storage = NotificationStorage::in_memory().unwrap();

        // Add notifications with different paths
        storage
            .add(&Notification::for_new_fact(
                Ulid::new(),
                "@products/alpha/api",
                "API",
                "local",
            ))
            .unwrap();
        storage
            .add(&Notification::for_new_fact(
                Ulid::new(),
                "@products/beta/api",
                "Beta API",
                "local",
            ))
            .unwrap();
        storage
            .add(&Notification::for_new_fact(
                Ulid::new(),
                "@docs/readme",
                "Readme",
                "local",
            ))
            .unwrap();

        // Subscribe only to @products/alpha
        let sub = Subscription::default().paths(vec!["@products/alpha".to_string()]);
        storage.update_subscription("session1", &sub).unwrap();

        let pending = storage.get_for_session("session1", 10).unwrap();
        assert_eq!(pending.len(), 1);
        assert!(pending[0]
            .path
            .as_ref()
            .unwrap()
            .starts_with("@products/alpha"));
    }

    #[test]
    fn test_priority_subscription() {
        let storage = NotificationStorage::in_memory().unwrap();

        // Add mixed priority
        storage
            .add(&Notification::for_ci(
                "Normal build",
                "ok",
                Priority::Normal,
            ))
            .unwrap();
        storage
            .add(&Notification::for_ci("High build", "warn", Priority::High))
            .unwrap();
        storage
            .add(&Notification::for_security("Critical!", "CVE"))
            .unwrap();

        // Subscribe to high+ only
        let sub = Subscription::default().priority_min(Priority::High);
        storage.update_subscription("session1", &sub).unwrap();

        let pending = storage.get_for_session("session1", 10).unwrap();
        assert_eq!(pending.len(), 2);
        assert!(pending.iter().all(|n| n.priority >= Priority::High));
    }

    #[test]
    fn test_list_categories() {
        let storage = NotificationStorage::in_memory().unwrap();

        storage
            .add(&Notification::for_new_fact(Ulid::new(), "@a", "A", "local"))
            .unwrap();
        storage
            .add(&Notification::for_new_fact(Ulid::new(), "@b", "B", "local"))
            .unwrap();
        storage
            .add(&Notification::for_ci("CI", "ok", Priority::Normal))
            .unwrap();

        let cats = storage.list_categories().unwrap();
        assert_eq!(cats.len(), 2);
        assert!(cats.iter().any(|(c, count)| c == "facts" && *count == 2));
        assert!(cats.iter().any(|(c, count)| c == "ci" && *count == 1));
    }
}
