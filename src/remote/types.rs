//! Remote API types
//!
//! DTOs for server communication.

use serde::{Deserialize, Serialize};

// ============== Knowledge Base Types ==============

/// Visibility of a knowledge base
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    #[default]
    Public,
    Private,
}

impl std::fmt::Display for Visibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Visibility::Public => write!(f, "public"),
            Visibility::Private => write!(f, "private"),
        }
    }
}

/// Knowledge base info from server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeBase {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub owner_id: String,
    pub visibility: String,
    pub created_at: String,
}

/// Response from list KBs endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KbListResponse {
    pub knowledge_bases: Vec<KnowledgeBase>,
    pub total: usize,
}

/// Request to create a KB
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateKbRequest {
    pub slug: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<String>,
}

// ============== Fact Types ==============

/// Fact from server (simplified)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteFact {
    pub id: String,
    pub path: String,
    pub title: String,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub trust_score: f32,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
}

/// Response from list facts endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactListResponse {
    pub facts: Vec<RemoteFact>,
    pub total: usize,
}

/// Request to create a fact
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateFactRequest {
    pub path: String,
    pub title: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

// ============== Search Types ==============

/// Search request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchRequest {
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_filter: Option<String>,
}

/// Search response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    pub results: Vec<RemoteFact>,
    pub total: usize,
}

// ============== Browse Types ==============

/// Browse tree node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowseNode {
    pub path: String,
    pub name: String,
    #[serde(default)]
    pub has_children: bool,
    #[serde(default)]
    pub fact_count: usize,
}

/// Browse response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowseResponse {
    pub nodes: Vec<BrowseNode>,
    #[serde(default)]
    pub cursor: Option<String>,
}

// ============== Health Types ==============

/// Health check response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

// ============== Error Types ==============

/// API error response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiErrorResponse {
    pub error: String,
    #[serde(default)]
    pub details: Option<String>,
}
