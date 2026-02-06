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
    /// Current KB name from config (for display and write policy)
    pub kb_name: String,
    /// Current KB slug (for remote API calls)
    pub kb_slug: Option<String>,
    /// Write policy for current KB
    pub write_policy: WritePolicy,
    /// Whether current KB is remote
    pub is_remote_kb: bool,
    /// Remote KB URL (if remote)
    pub remote_url: Option<String>,
    /// Session-level context override (None = use config default)
    /// Format: "local" or "http://server:3000/kb-slug"
    pub session_context: Option<String>,
    /// Whether this is the first tool call in this session
    pub first_tool_call: bool,
}

impl ServerState {
    /// Create new server state with given storage
    pub fn new(storage: Storage) -> Self {
        let session_id = format!("mcp-{}", Ulid::new());

        let (kb_name, kb_slug, write_policy, is_remote, remote_url, session_context) =
            match Config::load() {
                Ok(config) => {
                    let kb_name = config.primary_kb().to_string();
                    let policy = config.get_write_policy(&kb_name);
                    let kb_config = config.get_kb(&kb_name);
                    let is_remote = kb_config.map(|k| k.kb_type == "remote").unwrap_or(false);

                    // Get slug from KB config (for remote API calls)
                    let slug = kb_config.and_then(|k| k.slug.clone());

                    let url = kb_config.and_then(|k| {
                        k.server
                            .as_ref()
                            .and_then(|srv_name| config.get_server(srv_name).map(|s| s.url.clone()))
                    });

                    // Build session_context for remote KB
                    let ctx = if is_remote {
                        slug.as_ref().and_then(|s| {
                            url.as_ref()
                                .map(|u| format!("{}/{}", u.trim_end_matches('/'), s))
                        })
                    } else {
                        Some("local".to_string())
                    };

                    (kb_name, slug, policy, is_remote, url, ctx)
                }
                Err(_) => (
                    "local".to_string(),
                    None,
                    WritePolicy::Allow,
                    false,
                    None,
                    Some("local".to_string()),
                ),
            };

        Self {
            storage,
            initialized: false,
            session_id,
            kb_name,
            kb_slug,
            write_policy,
            is_remote_kb: is_remote,
            remote_url,
            session_context,
            first_tool_call: true,
        }
    }

    /// Get welcome message for first tool call (includes readme + context)
    pub fn get_first_call_message(&mut self) -> Option<String> {
        if !self.first_tool_call {
            return None;
        }
        self.first_tool_call = false;

        let context_info = format!(
            "📍 **Current KB:** {} ({})\n   **Write:** {:?}\n   **Session:** {}\n",
            self.kb_name,
            if self.is_remote_kb { "remote" } else { "local" },
            self.write_policy,
            self.session_id
        );

        let quick_start = r#"
## 🚀 Quick Start

**Search before answering:**
```
mcp_meh_meh_facts({"action": "search", "query": "topic"})
```

**Browse structure:**
```
mcp_meh_meh_facts({"action": "browse", "path": "@", "mode": "tree"})
```

**Add knowledge:**
```
mcp_meh_meh_write({"action": "add", "path": "@path/to/fact", "content": "..."})
```

**Switch KB:**
```
mcp_meh_meh_context({"action": "list_kbs"})
mcp_meh_meh_context({"action": "switch_kb", "kb_name": "..."})
```

💡 **Tip:** Always document bugs fixed and changes made!
"#;

        Some(format!(
            "🧠 **meh Knowledge Base - Session Started**\n\n{}\n{}",
            context_info, quick_start
        ))
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

        // Update KB slug (for remote API calls)
        self.kb_slug = kb_config.slug.clone();

        // Update remote status and session_context
        if kb_config.kb_type == "remote" {
            let server = config.get_server_for_kb(kb_name);
            self.is_remote_kb = true;
            self.remote_url = server.map(|s| s.url.clone());

            // Build full URL for session_context
            if let (Some(ref url), Some(ref slug)) = (&self.remote_url, &kb_config.slug) {
                self.session_context = Some(format!("{}/{}", url.trim_end_matches('/'), slug));
            } else {
                self.session_context = None;
            }
        } else {
            self.is_remote_kb = false;
            self.remote_url = None;
            self.session_context = Some("local".to_string());
        }

        // Update write policy and name
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

    /// Get current effective context (session override or config default)
    #[allow(dead_code)]
    pub fn get_effective_context(&self) -> String {
        if let Some(ref ctx) = self.session_context {
            ctx.clone()
        } else if self.is_remote_kb {
            format!(
                "{}/{}",
                self.remote_url.as_deref().unwrap_or(""),
                self.kb_name
            )
        } else {
            "local".to_string()
        }
    }

    /// Switch session context (session-level, doesn't affect config)
    pub fn switch_session_context(&mut self, context: &str) -> Result<String, String> {
        if context == "local" {
            // Switch to local
            self.session_context = Some("local".to_string());
            self.is_remote_kb = false;
            self.remote_url = None;
            self.kb_slug = None;

            // Load local storage
            let config = Config::load().map_err(|e| format!("Config error: {}", e))?;
            let db_path = config.data_dir();
            self.storage = Storage::open(&db_path)
                .map_err(|e| format!("Failed to open local storage: {}", e))?;

            self.kb_name = "local".to_string();
            self.write_policy = WritePolicy::Allow;

            return Ok("✅ Switched to local KB".to_string());
        }

        // Parse remote URL: http://server:3000/kb-slug
        let url = url::Url::parse(context).map_err(|e| {
            format!(
                "Invalid URL: {}. Use format: http://server:3000/kb-slug or 'local'",
                e
            )
        })?;

        let parsed_slug = url.path().trim_start_matches('/');
        if parsed_slug.is_empty() {
            return Err("URL must include KB slug: http://server:3000/KB_SLUG".to_string());
        }

        // Extract server base URL
        let mut base_url = url.clone();
        base_url.set_path("");
        let server_url = base_url.to_string().trim_end_matches('/').to_string();

        // Update session state
        self.session_context = Some(context.to_string());
        self.is_remote_kb = true;
        self.remote_url = Some(server_url.clone());
        self.kb_slug = Some(parsed_slug.to_string());

        // Check if there's a KB config matching this server+slug
        let config = Config::load().ok();
        let matched_kb = config.as_ref().and_then(|cfg| {
            cfg.kbs.kb.iter().find(|kb| {
                kb.kb_type == "remote"
                    && kb.slug.as_deref() == Some(parsed_slug)
                    && kb.server.as_ref().is_some_and(|srv_name| {
                        cfg.get_server(srv_name).is_some_and(|s| {
                            s.url.trim_end_matches('/') == server_url.trim_end_matches('/')
                        })
                    })
            })
        });

        // Use KB name from config if found, otherwise use slug
        self.kb_name = matched_kb
            .map(|kb| kb.name.clone())
            .unwrap_or_else(|| parsed_slug.to_string());

        // Use policy from config if KB is configured, otherwise default to Ask for safety
        self.write_policy = matched_kb.map(|kb| kb.write).unwrap_or(WritePolicy::Ask);

        Ok(format!(
            "✅ Switched to remote KB\n   Name:   {}\n   Server: {}\n   Slug:   {}",
            self.kb_name, server_url, parsed_slug
        ))
    }

    /// Show current session context info
    pub fn show_session_context(&self) -> String {
        let mut output = String::from("📍 Current Session Context\n\n");

        output.push_str(&format!("   KB Name: {}\n", self.kb_name));

        if self.is_remote_kb {
            output.push_str("   Type:    remote\n");
            output.push_str(&format!(
                "   Server:  {}\n",
                self.remote_url.as_deref().unwrap_or("unknown")
            ));
            if let Some(ref ctx) = self.session_context {
                if ctx != "local" {
                    output.push_str(&format!("   URL:     {}\n", ctx));
                }
            }
        } else {
            output.push_str("   Type:    local (SQLite)\n");
        }

        output.push_str(&format!("   Write:   {:?}\n", self.write_policy));
        output.push_str(&format!("   Session: {}\n", self.session_id));

        output.push_str("\n💡 Commands:\n");
        output.push_str("   mcp_meh_meh_context({\"action\": \"list_kbs\"})  # Show all KBs\n");
        output.push_str("   mcp_meh_meh_context({\"action\": \"switch_kb\", \"kb_name\": \"...\"})  # Switch by name\n");

        output
    }
}
