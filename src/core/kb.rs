//! Knowledge Base abstraction
//!
//! Provides a unified interface for local (SQLite) and remote (HTTP) knowledge bases.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────┐
//! │         KnowledgeBase               │
//! │  ┌─────────────┬─────────────┐      │
//! │  │   LocalKb   │  RemoteKb   │      │
//! │  │  (Storage)  │ (HTTP API)  │      │
//! │  └─────────────┴─────────────┘      │
//! └─────────────────────────────────────┘
//! ```

use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::Result;
use async_trait::async_trait;

use super::fact::Fact;
use super::storage::PathInfo;

/// Statistics for a knowledge base
#[derive(Debug, Clone, Default)]
pub struct KbStats {
    pub total_facts: i64,
    pub active_facts: i64,
    pub deprecated_facts: i64,
    pub superseded_facts: i64,
}

/// Backend trait for knowledge base operations
///
/// Implemented by both LocalKb (SQLite) and RemoteKb (HTTP API)
#[async_trait]
pub trait KnowledgeBaseBackend: Send + Sync {
    /// Search facts by query
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<Fact>>;

    /// Get a fact by ID or path
    async fn get_fact(&self, id_or_path: &str) -> Result<Option<Fact>>;

    /// Add a new fact
    async fn add_fact(&self, fact: &Fact) -> Result<()>;

    /// List children at a path
    async fn list_children(&self, path: &str, limit: usize) -> Result<Vec<PathInfo>>;

    /// Get statistics
    async fn stats(&self) -> Result<KbStats>;

    /// Mark a fact as superseded
    async fn mark_superseded(&self, id: &ulid::Ulid, by: &ulid::Ulid) -> Result<()>;

    /// Mark a fact as deprecated
    async fn mark_deprecated(&self, id: &ulid::Ulid) -> Result<()>;

    /// Whether this KB is read-only
    fn is_readonly(&self) -> bool;

    /// Human-readable name
    fn name(&self) -> &str;
}

/// Local knowledge base (SQLite)
///
/// Uses Mutex to make Storage thread-safe for async operations
pub struct LocalKb {
    storage: Mutex<super::storage::Storage>,
    name: String,
    path: PathBuf,
}

impl LocalKb {
    /// Open a local KB from a database path
    pub fn open(path: PathBuf) -> Result<Self> {
        let storage = super::storage::Storage::open(&path)?;
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("local")
            .to_string();

        Ok(Self {
            storage: Mutex::new(storage),
            name,
            path,
        })
    }

    /// Get the database path
    pub fn path(&self) -> &PathBuf {
        &self.path
    }
}

#[async_trait]
impl KnowledgeBaseBackend for LocalKb {
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<Fact>> {
        let storage = self
            .storage
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        storage.search(query, limit as i64)
    }

    async fn get_fact(&self, id_or_path: &str) -> Result<Option<Fact>> {
        let storage = self
            .storage
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;

        // Try as ULID first
        if let Ok(ulid) = id_or_path.parse::<ulid::Ulid>() {
            return storage.get_by_id(&ulid);
        }

        // Try as path
        let facts = storage.get_by_path(id_or_path)?;
        Ok(facts.into_iter().next())
    }

    async fn add_fact(&self, fact: &Fact) -> Result<()> {
        let storage = self
            .storage
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        storage.insert(fact)
    }

    async fn list_children(&self, path: &str, limit: usize) -> Result<Vec<PathInfo>> {
        let storage = self
            .storage
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        let (children, _has_more) = storage.list_children(path, limit as i64, None)?;
        Ok(children)
    }

    async fn stats(&self) -> Result<KbStats> {
        let storage = self
            .storage
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        let stats = storage.stats()?;
        Ok(KbStats {
            total_facts: stats.total,
            active_facts: stats.active_facts as i64,
            deprecated_facts: stats.deprecated_facts as i64,
            superseded_facts: 0, // Not tracked in StorageStats
        })
    }

    async fn mark_superseded(&self, id: &ulid::Ulid, _by: &ulid::Ulid) -> Result<()> {
        let storage = self
            .storage
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        storage.mark_superseded(id)
    }

    async fn mark_deprecated(&self, id: &ulid::Ulid) -> Result<()> {
        let storage = self
            .storage
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?;
        storage.mark_deprecated(id)
    }

    fn is_readonly(&self) -> bool {
        false
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Remote knowledge base (HTTP API)
pub struct RemoteKb {
    client: crate::remote::RemoteClient,
    kb_slug: String,
}

impl RemoteKb {
    /// Create a new remote KB
    pub fn new(
        server_url: &str,
        kb_slug: &str,
        token: Option<String>,
        api_key: Option<String>,
        timeout_secs: u64,
    ) -> Result<Self> {
        let client = crate::remote::RemoteClient::new(server_url, token, api_key, timeout_secs)?;
        Ok(Self {
            client,
            kb_slug: kb_slug.to_string(),
        })
    }

    /// Get the KB slug
    pub fn slug(&self) -> &str {
        &self.kb_slug
    }

    /// Get the client for operations not in trait
    pub fn client(&self) -> &crate::remote::RemoteClient {
        &self.client
    }
}

#[async_trait]
impl KnowledgeBaseBackend for RemoteKb {
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<Fact>> {
        let results = self
            .client
            .search(&self.kb_slug, query, Some(limit), None)
            .await?;

        // Convert RemoteFact to Fact
        let facts: Vec<Fact> = results
            .into_iter()
            .map(|rf| Fact {
                id: rf.id.parse().unwrap_or_else(|_| ulid::Ulid::new()),
                path: rf.path,
                title: rf.title,
                content: rf.content.unwrap_or_default(),
                summary: rf.summary,
                tags: rf.tags,
                trust_score: rf.trust_score,
                status: super::fact::Status::Active,
                fact_type: super::fact::FactType::Fact,
                source: super::fact::Source::Local,
                namespace: String::new(),
                supersedes: None,
                extends: Vec::new(),
                author_type: super::fact::AuthorType::Ai,
                author_id: String::new(),
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                accessed_at: None,
            })
            .collect();

        Ok(facts)
    }

    async fn get_fact(&self, id_or_path: &str) -> Result<Option<Fact>> {
        match self.client.get_fact(&self.kb_slug, id_or_path).await {
            Ok(rf) => {
                let fact = Fact {
                    id: rf.id.parse().unwrap_or_else(|_| ulid::Ulid::new()),
                    path: rf.path,
                    title: rf.title,
                    content: rf.content.unwrap_or_default(),
                    summary: rf.summary,
                    tags: rf.tags,
                    trust_score: rf.trust_score,
                    status: super::fact::Status::Active,
                    fact_type: super::fact::FactType::Fact,
                    source: super::fact::Source::Local,
                    namespace: String::new(),
                    supersedes: None,
                    extends: Vec::new(),
                    author_type: super::fact::AuthorType::Ai,
                    author_id: String::new(),
                    created_at: chrono::Utc::now(),
                    updated_at: chrono::Utc::now(),
                    accessed_at: None,
                };
                Ok(Some(fact))
            }
            Err(e) => {
                // Check if it's a 404
                if e.to_string().contains("not found") {
                    Ok(None)
                } else {
                    Err(e)
                }
            }
        }
    }

    async fn add_fact(&self, fact: &Fact) -> Result<()> {
        let req = crate::remote::CreateFactRequest {
            path: fact.path.clone(),
            title: fact.title.clone(),
            content: fact.content.clone(),
            tags: if fact.tags.is_empty() {
                None
            } else {
                Some(fact.tags.clone())
            },
        };

        self.client.create_fact(&self.kb_slug, req).await?;
        Ok(())
    }

    async fn list_children(&self, path: &str, _limit: usize) -> Result<Vec<PathInfo>> {
        let nodes = self.client.browse(&self.kb_slug, Some(path), None).await?;

        let children: Vec<PathInfo> = nodes
            .into_iter()
            .map(|n| PathInfo {
                path: n.path,
                fact_count: n.fact_count,
            })
            .collect();

        Ok(children)
    }

    async fn stats(&self) -> Result<KbStats> {
        // Remote doesn't have a stats endpoint yet
        // TODO: Add /api/v1/kbs/:slug/stats endpoint
        Ok(KbStats::default())
    }

    async fn mark_superseded(&self, _id: &ulid::Ulid, _by: &ulid::Ulid) -> Result<()> {
        // TODO: Implement remote correct endpoint
        anyhow::bail!("Remote correct not yet implemented")
    }

    async fn mark_deprecated(&self, _id: &ulid::Ulid) -> Result<()> {
        // TODO: Implement remote deprecate endpoint
        anyhow::bail!("Remote deprecate not yet implemented")
    }

    fn is_readonly(&self) -> bool {
        false // Remote KBs are writable (if authenticated)
    }

    fn name(&self) -> &str {
        &self.kb_slug
    }
}

/// Unified knowledge base - can be local or remote
pub enum KnowledgeBase {
    Local(LocalKb),
    Remote(RemoteKb),
}

impl KnowledgeBase {
    /// Create from CLI args and config
    ///
    /// Priority:
    /// 1. --server flag → Remote (find server in config by URL)
    /// 2. --kb flag → Use named KB from config
    /// 3. Primary KB from config
    pub fn from_args(
        server_url: Option<&str>,
        kb_slug: Option<&str>,
        config: &crate::config::Config,
    ) -> Result<Self> {
        // If explicit server URL provided, find matching server
        if let Some(url) = server_url {
            let server = config.servers.iter().find(|s| {
                s.url.trim_end_matches('/') == url.trim_end_matches('/')
            });

            let slug = kb_slug.ok_or_else(|| {
                anyhow::anyhow!(
                    "Knowledge base slug required for remote server.\n\
                     Use --kb flag to specify the KB slug."
                )
            })?;

            return if let Some(srv) = server {
                Ok(KnowledgeBase::Remote(RemoteKb::new(
                    url,
                    slug,
                    None,
                    srv.api_key.clone(),
                    srv.timeout_secs,
                )?))
            } else {
                // Unknown server - no auth
                Ok(KnowledgeBase::Remote(RemoteKb::new(
                    url,
                    slug,
                    None,
                    None,
                    30,
                )?))
            };
        }

        // Use named KB from config (from --kb flag or primary)
        let kb_name = kb_slug.unwrap_or(&config.kbs.primary);
        
        if let Some(kb_config) = config.get_kb(kb_name) {
            match kb_config.kb_type.as_str() {
                "remote" => {
                    let server = config.get_server_for_kb(kb_name)
                        .ok_or_else(|| anyhow::anyhow!(
                            "No server configured for remote KB '{}'. \
                             Check your config's [[servers]] section.", kb_name
                        ))?;
                    let slug = kb_config.slug.as_deref()
                        .ok_or_else(|| anyhow::anyhow!(
                            "No slug configured for remote KB '{}'", kb_name
                        ))?;
                    
                    Ok(KnowledgeBase::Remote(RemoteKb::new(
                        &server.url,
                        slug,
                        None,
                        server.api_key.clone(),
                        server.timeout_secs,
                    )?))
                }
                _ => {
                    // SQLite / local
                    let db_path = if let Some(path) = &kb_config.path {
                        std::path::PathBuf::from(path)
                    } else {
                        config.data_dir()
                    };
                    Ok(KnowledgeBase::Local(LocalKb::open(db_path)?))
                }
            }
        } else {
            // Fallback to local default
            let db_path = config.data_dir();
            Ok(KnowledgeBase::Local(LocalKb::open(db_path)?))
        }
    }

    /// Create a local KB from the default config
    pub fn local_default() -> Result<Self> {
        let config = crate::config::Config::load()?;
        let db_path = config.data_dir();
        Ok(KnowledgeBase::Local(LocalKb::open(db_path)?))
    }

    /// Check if this is a local KB
    pub fn is_local(&self) -> bool {
        matches!(self, KnowledgeBase::Local(_))
    }

    /// Check if this is a remote KB
    pub fn is_remote(&self) -> bool {
        matches!(self, KnowledgeBase::Remote(_))
    }
}

#[async_trait]
impl KnowledgeBaseBackend for KnowledgeBase {
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<Fact>> {
        match self {
            KnowledgeBase::Local(kb) => kb.search(query, limit).await,
            KnowledgeBase::Remote(kb) => kb.search(query, limit).await,
        }
    }

    async fn get_fact(&self, id_or_path: &str) -> Result<Option<Fact>> {
        match self {
            KnowledgeBase::Local(kb) => kb.get_fact(id_or_path).await,
            KnowledgeBase::Remote(kb) => kb.get_fact(id_or_path).await,
        }
    }

    async fn add_fact(&self, fact: &Fact) -> Result<()> {
        match self {
            KnowledgeBase::Local(kb) => kb.add_fact(fact).await,
            KnowledgeBase::Remote(kb) => kb.add_fact(fact).await,
        }
    }

    async fn list_children(&self, path: &str, limit: usize) -> Result<Vec<PathInfo>> {
        match self {
            KnowledgeBase::Local(kb) => kb.list_children(path, limit).await,
            KnowledgeBase::Remote(kb) => kb.list_children(path, limit).await,
        }
    }

    async fn stats(&self) -> Result<KbStats> {
        match self {
            KnowledgeBase::Local(kb) => kb.stats().await,
            KnowledgeBase::Remote(kb) => kb.stats().await,
        }
    }

    async fn mark_superseded(&self, id: &ulid::Ulid, by: &ulid::Ulid) -> Result<()> {
        match self {
            KnowledgeBase::Local(kb) => kb.mark_superseded(id, by).await,
            KnowledgeBase::Remote(kb) => kb.mark_superseded(id, by).await,
        }
    }

    async fn mark_deprecated(&self, id: &ulid::Ulid) -> Result<()> {
        match self {
            KnowledgeBase::Local(kb) => kb.mark_deprecated(id).await,
            KnowledgeBase::Remote(kb) => kb.mark_deprecated(id).await,
        }
    }

    fn is_readonly(&self) -> bool {
        match self {
            KnowledgeBase::Local(kb) => kb.is_readonly(),
            KnowledgeBase::Remote(kb) => kb.is_readonly(),
        }
    }

    fn name(&self) -> &str {
        match self {
            KnowledgeBase::Local(kb) => kb.name(),
            KnowledgeBase::Remote(kb) => kb.name(),
        }
    }
}
