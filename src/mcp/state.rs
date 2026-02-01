//! MCP Server state management

use std::path::PathBuf;
use ulid::Ulid;

use crate::config::{Config, WritePolicy};
use crate::core::notifications::NotificationStorage;
use crate::core::pending_queue::PendingQueue;
use crate::core::storage::Storage;

/// MCP Server state - holds all runtime data
pub struct ServerState {
    /// Fact storage (SQLite)
    pub storage: Storage,
    /// Whether client has sent initialize
    pub initialized: bool,
    /// Unique session ID for this MCP connection
    pub session_id: String,
    /// Current KB name (for write policy checking)
    pub kb_name: String,
    /// Write policy for current KB
    pub write_policy: WritePolicy,
    /// Whether current KB is remote
    pub is_remote_kb: bool,
    /// Remote KB URL (if remote)
    pub remote_url: Option<String>,
}

impl ServerState {
    /// Create new server state with given storage
    pub fn new(storage: Storage) -> Self {
        let session_id = format!("mcp-{}", Ulid::new());

        let (kb_name, write_policy, is_remote, remote_url) = match Config::load() {
            Ok(config) => {
                let kb_name = config.primary_kb().to_string();
                let policy = config.get_write_policy(&kb_name);
                let kb_config = config.get_kb(&kb_name);
                let is_remote = kb_config.map(|k| k.kb_type == "remote").unwrap_or(false);

                let url = kb_config.and_then(|k| {
                    k.server.as_ref().and_then(|srv_name| {
                        config.get_server(srv_name).map(|s| s.url.clone())
                    })
                });

                (kb_name, policy, is_remote, url)
            }
            Err(_) => ("local".to_string(), WritePolicy::Allow, false, None),
        };

        Self {
            storage,
            initialized: false,
            session_id,
            kb_name,
            write_policy,
            is_remote_kb: is_remote,
            remote_url,
        }
    }

    /// Open pending queue for remote KB writes
    pub fn open_pending_queue(&self) -> Result<PendingQueue, String> {
        let config = Config::load().map_err(|e| format!("Config error: {}", e))?;

        let queue_path = config
            .data_dir()
            .parent()
            .map(|p| p.join("pending_queue.db"))
            .unwrap_or_else(|| PathBuf::from(".meh/pending_queue.db"));

        PendingQueue::open(&queue_path).map_err(|e| format!("Pending queue error: {}", e))
    }

    /// Open notification storage
    pub fn open_notification_storage(&self) -> anyhow::Result<NotificationStorage> {
        let db_path = if let Ok(env_path) = std::env::var("MEH_DATABASE") {
            PathBuf::from(env_path)
        } else {
            Config::load()
                .map(|c| c.data_dir())
                .unwrap_or_else(|_| PathBuf::from(".meh/data.db"))
        };

        let notif_path = db_path
            .parent()
            .map(|p| p.join("notifications.db"))
            .unwrap_or_else(|| db_path.with_extension("notifications.db"));

        NotificationStorage::open(&notif_path)
    }

    /// Check if writes are allowed
    pub fn check_write_allowed(&self) -> Result<(), String> {
        if self.write_policy == WritePolicy::Deny {
            Err(format!(
                "Write denied: KB '{}' has write policy 'deny'",
                self.kb_name
            ))
        } else {
            Ok(())
        }
    }

    /// Check if this is "ask" policy (pending review)
    #[allow(dead_code)]
    pub fn is_pending_review(&self) -> bool {
        self.write_policy == WritePolicy::Ask
    }

    /// Switch to a different KB
    pub fn switch_kb(&mut self, kb_name: &str) -> Result<(), String> {
        let config = Config::load().map_err(|e| format!("Config error: {}", e))?;

        let kb_config = config.get_kb(kb_name).ok_or_else(|| {
            let available: Vec<_> = config.kbs.kb.iter().map(|k| k.name.as_str()).collect();
            format!(
                "KB '{}' not found. Available: {}",
                kb_name,
                available.join(", ")
            )
        })?;

        // Update remote status
        if kb_config.kb_type == "remote" {
            let server = config.get_server_for_kb(kb_name);
            self.is_remote_kb = true;
            self.remote_url = server.map(|s| s.url.clone());
        } else {
            self.is_remote_kb = false;
            self.remote_url = None;
        }

        // Update write policy
        self.write_policy = config.get_write_policy(kb_name);
        self.kb_name = kb_name.to_string();

        // For local SQLite KB, switch storage
        if kb_config.kb_type == "sqlite" {
            let db_path = if let Some(path) = &kb_config.path {
                PathBuf::from(path)
            } else {
                config.data_dir()
            };

            self.storage =
                Storage::open(&db_path).map_err(|e| format!("Failed to open KB: {}", e))?;
        }

        Ok(())
    }
}
