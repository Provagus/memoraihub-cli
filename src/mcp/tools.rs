//! MCP Tool argument structs for meh knowledge base
//!
//! Simple structs for deserializing tool arguments.

use serde::{Deserialize, Serialize};

/// Search the knowledge base for facts matching a query
#[derive(Debug, Deserialize, Serialize)]
pub struct MehSearchTool {
    /// Search query - natural language or keywords
    pub query: String,
    /// Optional path prefix filter (e.g. '@products/alpha')
    #[serde(default)]
    pub path_filter: Option<String>,
    /// Maximum number of results (default: 20)
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 { 20 }

/// Get a single fact by ID or path
#[derive(Debug, Deserialize, Serialize)]
pub struct MehGetFactTool {
    /// Fact ID (meh-xxx) or path (@path/to/fact)
    pub id_or_path: String,
    /// Include superseded/extended facts chain
    #[serde(default)]
    pub include_history: bool,
}

/// Browse the knowledge base path structure
#[derive(Debug, Deserialize, Serialize)]
pub struct MehBrowseTool {
    /// Path prefix to browse (default: root)
    #[serde(default = "default_path")]
    pub path: String,
    /// Browse mode: 'ls' for flat list, 'tree' for hierarchical view
    #[serde(default = "default_mode")]
    pub mode: String,
    /// Maximum depth for tree mode (default: 3)
    #[serde(default = "default_depth")]
    pub depth: i32,
}

fn default_path() -> String { "@".to_string() }
fn default_mode() -> String { "ls".to_string() }
fn default_depth() -> i32 { 3 }

/// Add a new fact to the knowledge base
#[derive(Debug, Deserialize, Serialize)]
pub struct MehAddTool {
    /// Fact path (e.g. '@products/alpha/api/timeout')
    pub path: String,
    /// Fact content in Markdown format
    pub content: String,
    /// Optional tags for categorization
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Correct an existing fact
#[derive(Debug, Deserialize, Serialize)]
pub struct MehCorrectTool {
    /// ID of the fact to correct (meh-xxx)
    pub fact_id: String,
    /// Corrected content in Markdown format
    pub new_content: String,
    /// Optional reason for correction
    #[serde(default)]
    pub reason: Option<String>,
}

/// Extend an existing fact with additional information
#[derive(Debug, Deserialize, Serialize)]
pub struct MehExtendTool {
    /// ID of the fact to extend (meh-xxx)
    pub fact_id: String,
    /// Additional content to add
    pub extension: String,
}

/// Mark a fact as deprecated
#[derive(Debug, Deserialize, Serialize)]
pub struct MehDeprecateTool {
    /// ID of the fact to deprecate (meh-xxx)
    pub fact_id: String,
    /// Reason for deprecation
    #[serde(default)]
    pub reason: Option<String>,
}
