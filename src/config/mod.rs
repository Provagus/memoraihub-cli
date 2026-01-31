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
    
    #[serde(default)]
    pub server: ServerConfig,
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
}

impl Default for CoreConfig {
    fn default() -> Self {
        Self {
            default_source: default_source(),
            cache_dir: None,
            cache_max_mb: default_cache_max_mb(),
        }
    }
}

fn default_source() -> String {
    "local".to_string()
}

fn default_cache_max_mb() -> usize {
    100
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

/// Server configuration for remote knowledge bases
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Server URL (e.g., "http://localhost:3000")
    #[serde(default)]
    pub url: Option<String>,
    
    /// Authentication token
    #[serde(default)]
    pub token: Option<String>,
    
    /// Default knowledge base slug
    #[serde(default)]
    pub default_kb: Option<String>,
    
    /// Connection timeout in seconds
    #[serde(default = "default_server_timeout")]
    pub timeout_secs: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            url: None,
            token: None,
            default_kb: None,
            timeout_secs: default_server_timeout(),
        }
    }
}

fn default_server_timeout() -> u64 {
    30
}

impl Config {
    /// Load config from default locations
    pub fn load() -> Result<Self> {
        // Try local config first, then global
        if let Some(local) = Self::find_local_config() {
            return Self::load_from(&local);
        }

        if let Some(global) = Self::global_config_path() {
            if global.exists() {
                return Self::load_from(&global);
            }
        }

        // Return default config
        Ok(Self::default())
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
}

/// Helper to get directories crate functionality
mod dirs {
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
