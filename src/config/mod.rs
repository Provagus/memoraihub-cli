//! Configuration module

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub user: UserConfig,

    #[serde(default)]
    pub core: CoreConfig,

    #[serde(default)]
    pub search: SearchConfig,

    #[serde(default)]
    pub trust: TrustConfig,

    /// Remote servers configuration
    #[serde(default)]
    pub servers: Vec<ServerEntry>,

    /// Knowledge base configurations
    #[serde(default)]
    pub kbs: KnowledgeBasesConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserConfig {
    #[serde(default = "default_user_name")]
    pub name: String,

    #[serde(default)]
    pub agent_id: String,
}

impl Default for UserConfig {
    fn default() -> Self {
        Self {
            name: default_user_name(),
            agent_id: String::new(),
        }
    }
}

fn default_user_name() -> String {
    "AI".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreConfig {
    #[serde(default = "default_source")]
    pub default_source: String,

    #[serde(default)]
    pub cache_dir: Option<PathBuf>,

    #[serde(default = "default_cache_max_mb")]
    pub cache_max_mb: usize,

    /// Retention period for deprecated/superseded facts (days)
    #[serde(default = "default_gc_retention_days")]
    pub gc_retention_days: u32,

    /// Auto-run GC on MCP server start
    #[serde(default = "default_gc_auto")]
    pub gc_auto: bool,
}

impl Default for CoreConfig {
    fn default() -> Self {
        Self {
            default_source: default_source(),
            cache_dir: None,
            cache_max_mb: default_cache_max_mb(),
            gc_retention_days: default_gc_retention_days(),
            gc_auto: default_gc_auto(),
        }
    }
}

fn default_source() -> String {
    "local".to_string()
}

fn default_cache_max_mb() -> usize {
    100
}

fn default_gc_retention_days() -> u32 {
    30
}

fn default_gc_auto() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchConfig {
    #[serde(default = "default_limit")]
    pub default_limit: usize,

    #[serde(default = "default_token_budget")]
    pub token_budget: usize,

    #[serde(default = "default_timeout_secs")]
    pub federated_timeout_secs: u64,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            default_limit: default_limit(),
            token_budget: default_token_budget(),
            federated_timeout_secs: default_timeout_secs(),
        }
    }
}

fn default_limit() -> usize {
    20
}

fn default_token_budget() -> usize {
    3000
}

fn default_timeout_secs() -> u64 {
    5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustConfig {
    #[serde(default = "default_trust_score")]
    pub default_score: f32,

    #[serde(default = "default_decay_rate")]
    pub decay_rate: f32,
}

impl Default for TrustConfig {
    fn default() -> Self {
        Self {
            default_score: default_trust_score(),
            decay_rate: default_decay_rate(),
        }
    }
}

fn default_trust_score() -> f32 {
    0.5
}

fn default_decay_rate() -> f32 {
    0.01
}

/// Server entry - defines a remote server with auth
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerEntry {
    /// Server name/identifier (referenced by KBs)
    pub name: String,

    /// Server URL (e.g., "https://api.memoraihub.com")
    pub url: String,

    /// API key for authentication (meh_xxx format)
    #[serde(default)]
    pub api_key: Option<String>,

    /// Connection timeout in seconds
    #[serde(default = "default_server_timeout")]
    pub timeout_secs: u64,
}

fn default_server_timeout() -> u64 {
    30
}

/// Knowledge bases configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KnowledgeBasesConfig {
    /// Primary KB name (used when no --kb specified)
    #[serde(default = "default_primary_kb")]
    pub primary: String,

    /// Order for federated search
    #[serde(default)]
    pub search_order: Vec<String>,

    /// Individual KB configurations
    #[serde(default)]
    pub kb: Vec<KbConfig>,
}

fn default_primary_kb() -> String {
    "local".to_string()
}

/// Single knowledge base configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KbConfig {
    /// KB name/identifier
    pub name: String,

    /// Type: "sqlite" or "remote"
    #[serde(default = "default_kb_type")]
    pub kb_type: String,

    /// Path for sqlite KB
    #[serde(default)]
    pub path: Option<String>,

    /// Server name (for remote KB - references [[servers]] entry)
    #[serde(default)]
    pub server: Option<String>,

    /// KB slug on the remote server
    #[serde(default)]
    pub slug: Option<String>,

    /// Write policy: allow, deny, ask
    #[serde(default = "default_write_policy")]
    pub write: WritePolicy,
}

fn default_kb_type() -> String {
    "sqlite".to_string()
}

/// Write policy for a knowledge base
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum WritePolicy {
    /// AI can write freely
    #[default]
    Allow,
    /// AI cannot write
    Deny,
    /// AI writes go to pending_review, user must approve
    Ask,
}

fn default_write_policy() -> WritePolicy {
    WritePolicy::Allow
}

impl Config {
    /// Load config from default locations
    /// Creates default config file if none exists
    ///
    /// Priority:
    /// 1. MEH_CONFIG env var (explicit path)
    /// 2. MEH_WORKSPACE env var + .meh/config.toml (VS Code workspace)
    /// 3. Local .meh/config.toml (walking up from CWD)
    /// 4. Global ~/.meh/config.toml
    pub fn load() -> Result<Self> {
        // 1. Explicit config path from env
        if let Ok(config_path) = std::env::var("MEH_CONFIG") {
            let path = std::path::PathBuf::from(&config_path);
            if path.exists() {
                return Self::load_from(&path);
            }
        }

        // 2. Workspace path from env (set by MCP server from VS Code)
        if let Ok(workspace) = std::env::var("MEH_WORKSPACE") {
            let config_path = std::path::PathBuf::from(&workspace)
                .join(".meh")
                .join("config.toml");
            if config_path.exists() {
                return Self::load_from(&config_path);
            }
        }

        // 3. Try local config first (walking up from CWD)
        if let Some(local) = Self::find_local_config() {
            return Self::load_from(&local);
        }

        // 4. Global config
        if let Some(global) = Self::global_config_path() {
            if global.exists() {
                return Self::load_from(&global);
            }
        }

        // No config exists - create default global config
        let config = Self::default();
        if let Some(global_path) = Self::global_config_path() {
            // Try to create default config file (ignore errors - may not have permissions)
            let _ = config.save_to(&global_path);
        }

        Ok(config)
    }

    /// Load config from a specific file
    pub fn load_from(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }

    /// Save config to a file
    pub fn save_to(&self, path: &Path) -> Result<()> {
        let content = toml::to_string_pretty(self)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Find local .meh/config.toml walking up directories
    pub fn find_local_config() -> Option<PathBuf> {
        let mut current = std::env::current_dir().ok()?;

        loop {
            let config_path = current.join(".meh").join("config.toml");
            if config_path.exists() {
                return Some(config_path);
            }

            if !current.pop() {
                break;
            }
        }

        None
    }

    /// Find local .meh/data.db walking up directories
    pub fn find_local_db() -> Option<PathBuf> {
        let mut current = std::env::current_dir().ok()?;

        loop {
            let db_path = current.join(".meh").join("data.db");
            if db_path.exists() {
                return Some(db_path);
            }

            if !current.pop() {
                break;
            }
        }

        None
    }

    /// Get global config path (~/.meh/config.toml)
    pub fn global_config_path() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".meh").join("config.toml"))
    }

    /// Get global database path (~/.meh/data.db)
    pub fn global_db_path() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join(".meh").join("data.db"))
    }

    /// Get data directory path with priority:
    /// 1. MEH_DATABASE env var
    /// 2. Local .meh/data.db (walking up from CWD)
    /// 3. Global ~/.meh/data.db
    pub fn data_dir(&self) -> PathBuf {
        // 1. Environment variable
        if let Ok(env_path) = std::env::var("MEH_DATABASE") {
            return PathBuf::from(env_path);
        }

        // 2. Local .meh/data.db (search up from current directory)
        if let Some(local_db) = Self::find_local_db() {
            return local_db;
        }

        // 3. Local .meh/ directory exists (even without data.db yet)
        if let Some(local_config) = Self::find_local_config() {
            return local_config.parent().unwrap().join("data.db");
        }

        // 4. Global ~/.meh/data.db
        if let Some(global) = Self::global_db_path() {
            return global;
        }

        // 5. Fallback to current directory
        PathBuf::from(".meh").join("data.db")
    }

    /// Get write policy for a knowledge base by name
    /// Returns Allow if KB not found (backward compatible)
    pub fn get_write_policy(&self, kb_name: &str) -> WritePolicy {
        self.kbs
            .kb
            .iter()
            .find(|kb| kb.name == kb_name)
            .map(|kb| kb.write)
            .unwrap_or(WritePolicy::Allow)
    }

    /// Get KB config by name
    pub fn get_kb(&self, kb_name: &str) -> Option<&KbConfig> {
        self.kbs.kb.iter().find(|kb| kb.name == kb_name)
    }

    /// Get server config by name
    pub fn get_server(&self, server_name: &str) -> Option<&ServerEntry> {
        self.servers.iter().find(|s| s.name == server_name)
    }

    /// Get server for a KB (if remote)
    pub fn get_server_for_kb(&self, kb_name: &str) -> Option<&ServerEntry> {
        self.get_kb(kb_name)
            .and_then(|kb| kb.server.as_ref())
            .and_then(|server_name| self.get_server(server_name))
    }

    /// Get primary KB name
    pub fn primary_kb(&self) -> &str {
        &self.kbs.primary
    }
}

/// Helper to get directories crate functionality
pub mod dirs {
    use std::path::PathBuf;

    pub fn home_dir() -> Option<PathBuf> {
        #[cfg(windows)]
        {
            std::env::var("USERPROFILE").ok().map(PathBuf::from)
        }
        #[cfg(not(windows))]
        {
            std::env::var("HOME").ok().map(PathBuf::from)
        }
    }
}
