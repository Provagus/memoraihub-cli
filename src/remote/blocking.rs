//! Blocking (synchronous) HTTP client for remote KB operations
//!
//! This module provides blocking versions of remote API calls,
//! used by both CLI and MCP server (which run synchronously).

use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

use crate::config::Config;

/// URL-encode a string for use in query parameters
fn encode_uri_component(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 3);
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
            _ => {
                for byte in c.to_string().as_bytes() {
                    result.push_str(&format!("%{:02X}", byte));
                }
            }
        }
    }
    result
}

/// Blocking client for remote KB operations
#[derive(Debug, Clone)]
pub struct BlockingRemoteClient {
    client: Client,
    server_url: String,
    kb_slug: String,
    api_key: Option<String>,
    timeout_secs: u64,
}

/// Response from creating a fact
#[derive(Debug, Deserialize)]
pub struct CreateFactResponse {
    pub id: String,
    pub path: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub trust_score: f32,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub created_at: Option<String>,
}

/// Response from search
#[derive(Debug, Deserialize)]
pub struct SearchResponse {
    pub facts: Vec<RemoteFactSummary>,
    pub total: u64,
}

/// Fact summary in search results
#[derive(Debug, Deserialize)]
pub struct RemoteFactSummary {
    pub id: String,
    pub path: String,
    pub title: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub trust_score: f32,
}

/// Browse entry
#[derive(Debug, Deserialize)]
pub struct BrowseEntry {
    pub path: String,
    pub fact_count: u64,
}

/// Browse response
#[derive(Debug, Deserialize)]
pub struct BrowseResponse {
    pub entries: Vec<BrowseEntry>,
    #[serde(default)]
    pub has_more: bool,
}

impl BlockingRemoteClient {
    /// Create client from config for a specific KB
    pub fn from_config(config: &Config, kb_name: &str) -> Result<Self> {
        let kb = config
            .get_kb(kb_name)
            .ok_or_else(|| anyhow::anyhow!("KB '{}' not found in config", kb_name))?;

        if kb.kb_type != "remote" {
            anyhow::bail!("KB '{}' is not a remote KB (type: {})", kb_name, kb.kb_type);
        }

        let server_name = kb
            .server
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("KB '{}' has no server configured", kb_name))?;

        let server = config
            .get_server(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server '{}' not found in config", server_name))?;

        let slug = kb
            .slug
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("KB '{}' has no slug configured", kb_name))?;

        Self::new(
            &server.url,
            slug,
            server.api_key.clone(),
            server.timeout_secs,
        )
    }

    /// Create client from URL and config (matches server by URL)
    pub fn from_url(server_url: &str, kb_slug: &str, config: &Config) -> Result<Self> {
        let normalized_url = server_url.trim_end_matches('/');

        let server = config
            .servers
            .iter()
            .find(|s| s.url.trim_end_matches('/') == normalized_url);

        let (api_key, timeout) = match server {
            Some(s) => (s.api_key.clone(), s.timeout_secs),
            None => (None, 30),
        };

        Self::new(server_url, kb_slug, api_key, timeout)
    }

    /// Create client with explicit parameters
    pub fn new(
        server_url: &str,
        kb_slug: &str,
        api_key: Option<String>,
        timeout_secs: u64,
    ) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            client,
            server_url: server_url.trim_end_matches('/').to_string(),
            kb_slug: kb_slug.to_string(),
            api_key,
            timeout_secs,
        })
    }

    /// Build request with auth header
    fn auth_request(&self, builder: reqwest::blocking::RequestBuilder) -> reqwest::blocking::RequestBuilder {
        if let Some(ref key) = self.api_key {
            builder.header("X-API-Key", key)
        } else {
            builder
        }
    }

    /// Handle response, returning parsed JSON or error
    fn handle_response<T: for<'de> Deserialize<'de>>(&self, response: reqwest::blocking::Response) -> Result<T> {
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            anyhow::bail!("Remote API error ({}): {}", status, body);
        }
        response.json().context("Failed to parse response")
    }

    // ============== Fact Operations ==============

    /// Add a new fact
    pub fn add_fact(&self, path: &str, content: &str, tags: &[String]) -> Result<CreateFactResponse> {
        let url = format!("{}/api/v1/kbs/{}/facts", self.server_url, self.kb_slug);

        let payload = serde_json::json!({
            "path": path,
            "content": content,
            "tags": tags,
        });

        let request = self.auth_request(self.client.post(&url)).json(&payload);
        let response = request.send().context("Failed to send add request")?;

        self.handle_response(response)
    }

    /// Correct (supersede) a fact
    pub fn correct_fact(&self, fact_id: &str, new_content: &str) -> Result<CreateFactResponse> {
        let url = format!(
            "{}/api/v1/kbs/{}/facts/{}/correct",
            self.server_url, self.kb_slug, fact_id
        );

        let payload = serde_json::json!({
            "new_content": new_content,
        });

        let request = self.auth_request(self.client.post(&url)).json(&payload);
        let response = request.send().context("Failed to send correct request")?;

        self.handle_response(response)
    }

    /// Extend a fact
    pub fn extend_fact(&self, fact_id: &str, extension: &str) -> Result<CreateFactResponse> {
        let url = format!(
            "{}/api/v1/kbs/{}/facts/{}/extend",
            self.server_url, self.kb_slug, fact_id
        );

        let payload = serde_json::json!({
            "extension": extension,
        });

        let request = self.auth_request(self.client.post(&url)).json(&payload);
        let response = request.send().context("Failed to send extend request")?;

        self.handle_response(response)
    }

    /// Deprecate a fact
    pub fn deprecate_fact(&self, fact_id: &str, reason: Option<&str>) -> Result<()> {
        let url = format!(
            "{}/api/v1/kbs/{}/facts/{}/deprecate",
            self.server_url, self.kb_slug, fact_id
        );

        let payload = serde_json::json!({
            "reason": reason.unwrap_or("Deprecated"),
        });

        let request = self.auth_request(self.client.post(&url)).json(&payload);
        let response = request.send().context("Failed to send deprecate request")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            anyhow::bail!("Remote API error ({}): {}", status, body);
        }

        Ok(())
    }

    /// Search facts
    pub fn search(&self, query: &str, path_filter: Option<&str>, limit: Option<u32>) -> Result<SearchResponse> {
        let mut url = format!(
            "{}/api/v1/kbs/{}/search?q={}",
            self.server_url,
            self.kb_slug,
            encode_uri_component(query)
        );

        if let Some(path) = path_filter {
            url.push_str(&format!("&path={}", encode_uri_component(path)));
        }
        if let Some(lim) = limit {
            url.push_str(&format!("&limit={}", lim));
        }

        let request = self.auth_request(self.client.get(&url));
        let response = request.send().context("Failed to send search request")?;

        self.handle_response(response)
    }

    /// Browse paths
    pub fn browse(&self, path: &str, limit: Option<u32>, cursor: Option<&str>) -> Result<BrowseResponse> {
        let mut url = format!(
            "{}/api/v1/kbs/{}/browse?path={}",
            self.server_url,
            self.kb_slug,
            encode_uri_component(path)
        );

        if let Some(lim) = limit {
            url.push_str(&format!("&limit={}", lim));
        }
        if let Some(cur) = cursor {
            url.push_str(&format!("&cursor={}", encode_uri_component(cur)));
        }

        let request = self.auth_request(self.client.get(&url));
        let response = request.send().context("Failed to send browse request")?;

        self.handle_response(response)
    }

    /// Get a single fact by ID
    pub fn get_fact(&self, fact_id: &str) -> Result<serde_json::Value> {
        let url = format!(
            "{}/api/v1/kbs/{}/facts/{}",
            self.server_url, self.kb_slug, fact_id
        );

        let request = self.auth_request(self.client.get(&url));
        let response = request.send().context("Failed to send get request")?;

        self.handle_response(response)
    }

    // ============== Voting ==============

    /// Vote on a fact
    pub fn vote(&self, fact_id: &str, vote: &str, reason: Option<&str>) -> Result<()> {
        let url = format!(
            "{}/api/v1/kbs/{}/facts/{}/vote",
            self.server_url, self.kb_slug, fact_id
        );

        let payload = serde_json::json!({
            "vote": vote,
            "reason": reason,
        });

        let request = self.auth_request(self.client.post(&url)).json(&payload);
        let response = request.send().context("Failed to send vote request")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            anyhow::bail!("Remote API error ({}): {}", status, body);
        }

        Ok(())
    }

    /// Bulk vote on multiple facts
    pub fn bulk_vote(&self, votes: &[BulkVoteItem]) -> Result<BulkVoteResponse> {
        let url = format!("{}/api/v1/kbs/{}/votes", self.server_url, self.kb_slug);

        let payload = serde_json::json!({
            "votes": votes,
        });

        let request = self.auth_request(self.client.post(&url)).json(&payload);
        let response = request.send().context("Failed to send bulk vote request")?;

        self.handle_response(response)
    }
}

/// Item for bulk voting
#[derive(Debug, Serialize, Deserialize)]
pub struct BulkVoteItem {
    pub fact_id: String,
    pub vote: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Response from bulk vote
#[derive(Debug, Deserialize)]
pub struct BulkVoteResponse {
    pub processed: u32,
    pub failed: u32,
    #[serde(default)]
    pub errors: Vec<String>,
}
