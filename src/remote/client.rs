//! Remote server HTTP client
//!
//! Async client for memoraihub-server API.

use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::{Client, StatusCode};
use url::Url;

use super::types::*;
use crate::config::ServerConfig;

/// HTTP client for remote memoraihub-server
#[derive(Debug, Clone)]
pub struct RemoteClient {
    client: Client,
    base_url: Url,
    token: Option<String>,
}

impl RemoteClient {
    /// Create new client from server config
    pub fn from_config(config: &ServerConfig) -> Result<Self> {
        let url = config.url.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Server URL not configured. Set server.url in config or use --server-url flag."))?;
        
        Self::new(url, config.token.clone(), config.timeout_secs)
    }
    
    /// Create new client with explicit parameters
    pub fn new(base_url: &str, token: Option<String>, timeout_secs: u64) -> Result<Self> {
        let base_url = Url::parse(base_url)
            .with_context(|| format!("Invalid server URL: {}", base_url))?;
        
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .context("Failed to create HTTP client")?;
        
        Ok(Self { client, base_url, token })
    }
    
    /// Build a URL for an endpoint
    fn url(&self, path: &str) -> Result<Url> {
        self.base_url.join(path)
            .with_context(|| format!("Invalid endpoint path: {}", path))
    }
    
    /// Add auth header if token is set
    fn auth_header(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(ref token) = self.token {
            builder.header("Authorization", format!("Bearer {}", token))
        } else {
            builder
        }
    }

    // ============== Health ==============

    /// Check server health
    pub async fn health(&self) -> Result<HealthResponse> {
        let url = self.url("/health")?;
        
        let resp = self.client.get(url)
            .send()
            .await
            .context("Failed to connect to server")?;
        
        if !resp.status().is_success() {
            anyhow::bail!("Server health check failed: {}", resp.status());
        }
        
        resp.json().await.context("Failed to parse health response")
    }

    // ============== Knowledge Bases ==============

    /// List all accessible knowledge bases
    pub async fn list_kbs(&self) -> Result<Vec<KnowledgeBase>> {
        let url = self.url("/api/v1/kbs")?;
        
        let resp = self.auth_header(self.client.get(url))
            .send()
            .await
            .context("Failed to list knowledge bases")?;
        
        self.handle_response::<KbListResponse>(resp).await
            .map(|r| r.knowledge_bases)
    }
    
    /// Create a new knowledge base
    pub async fn create_kb(&self, req: CreateKbRequest) -> Result<KnowledgeBase> {
        let url = self.url("/api/v1/kbs")?;
        
        let resp = self.auth_header(self.client.post(url))
            .json(&req)
            .send()
            .await
            .context("Failed to create knowledge base")?;
        
        self.handle_response(resp).await
    }
    
    /// Get a specific knowledge base
    pub async fn get_kb(&self, slug: &str) -> Result<KnowledgeBase> {
        let url = self.url(&format!("/api/v1/kbs/{}", slug))?;
        
        let resp = self.auth_header(self.client.get(url))
            .send()
            .await
            .context("Failed to get knowledge base")?;
        
        self.handle_response(resp).await
    }
    
    /// Delete a knowledge base
    pub async fn delete_kb(&self, slug: &str) -> Result<()> {
        let url = self.url(&format!("/api/v1/kbs/{}", slug))?;
        
        let resp = self.auth_header(self.client.delete(url))
            .send()
            .await
            .context("Failed to delete knowledge base")?;
        
        if resp.status() == StatusCode::NOT_FOUND {
            anyhow::bail!("Knowledge base '{}' not found", slug);
        }
        
        if !resp.status().is_success() {
            let err = self.extract_error(resp).await;
            anyhow::bail!("Failed to delete knowledge base: {}", err);
        }
        
        Ok(())
    }

    // ============== Facts ==============

    /// List facts in a knowledge base
    pub async fn list_facts(&self, kb_slug: &str, level: Option<&str>, limit: Option<usize>) -> Result<Vec<RemoteFact>> {
        let mut url = self.url(&format!("/api/v1/kbs/{}/facts", kb_slug))?;
        
        if let Some(level) = level {
            url.query_pairs_mut().append_pair("level", level);
        }
        if let Some(limit) = limit {
            url.query_pairs_mut().append_pair("limit", &limit.to_string());
        }
        
        let resp = self.auth_header(self.client.get(url))
            .send()
            .await
            .context("Failed to list facts")?;
        
        self.handle_response::<FactListResponse>(resp).await
            .map(|r| r.facts)
    }
    
    /// Create a fact in a knowledge base
    pub async fn create_fact(&self, kb_slug: &str, req: CreateFactRequest) -> Result<RemoteFact> {
        let url = self.url(&format!("/api/v1/kbs/{}/facts", kb_slug))?;
        
        let resp = self.auth_header(self.client.post(url))
            .json(&req)
            .send()
            .await
            .context("Failed to create fact")?;
        
        self.handle_response(resp).await
    }
    
    /// Get a fact by ID
    pub async fn get_fact(&self, kb_slug: &str, fact_id: &str) -> Result<RemoteFact> {
        let url = self.url(&format!("/api/v1/kbs/{}/facts/{}", kb_slug, fact_id))?;
        
        let resp = self.auth_header(self.client.get(url))
            .send()
            .await
            .context("Failed to get fact")?;
        
        self.handle_response(resp).await
    }

    // ============== Search ==============

    /// Search facts in a knowledge base
    pub async fn search(&self, kb_slug: &str, query: &str, limit: Option<usize>, path_filter: Option<&str>) -> Result<Vec<RemoteFact>> {
        let url = self.url(&format!("/api/v1/kbs/{}/search", kb_slug))?;
        
        let req = SearchRequest {
            query: query.to_string(),
            limit,
            path_filter: path_filter.map(|s| s.to_string()),
        };
        
        let resp = self.auth_header(self.client.post(url))
            .json(&req)
            .send()
            .await
            .context("Failed to search")?;
        
        self.handle_response::<SearchResponse>(resp).await
            .map(|r| r.results)
    }

    // ============== Browse ==============

    /// Browse paths in a knowledge base
    pub async fn browse(&self, kb_slug: &str, path: Option<&str>, depth: Option<usize>) -> Result<Vec<BrowseNode>> {
        let path_suffix = path.unwrap_or("");
        let url = self.url(&format!("/api/v1/kbs/{}/browse/{}", kb_slug, path_suffix))?;
        
        let mut builder = self.auth_header(self.client.get(url));
        
        if let Some(depth) = depth {
            builder = builder.query(&[("depth", depth.to_string())]);
        }
        
        let resp = builder
            .send()
            .await
            .context("Failed to browse")?;
        
        self.handle_response::<BrowseResponse>(resp).await
            .map(|r| r.nodes)
    }

    // ============== Helpers ==============

    /// Handle response and deserialize
    async fn handle_response<T: serde::de::DeserializeOwned>(&self, resp: reqwest::Response) -> Result<T> {
        let status = resp.status();
        
        if status == StatusCode::NOT_FOUND {
            anyhow::bail!("Resource not found");
        }
        
        if !status.is_success() {
            let err = self.extract_error(resp).await;
            anyhow::bail!("API error ({}): {}", status, err);
        }
        
        resp.json().await.context("Failed to parse response")
    }
    
    /// Extract error message from response
    async fn extract_error(&self, resp: reqwest::Response) -> String {
        if let Ok(err) = resp.json::<ApiErrorResponse>().await {
            err.error
        } else {
            "Unknown error".to_string()
        }
    }
}
