//! Search handlers for MCP

use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;
use ulid::Ulid;

use super::ToolResult;
use crate::config::Config;
use crate::core::fact::Fact;
use crate::core::storage::Storage;
use crate::mcp::state::ServerState;
use crate::mcp::tools::{MehFederatedSearchTool, MehSearchTool};

/// Search the current KB
pub fn do_search(state: &ServerState, args: &Value) -> ToolResult {
    let tool_args: MehSearchTool =
        serde_json::from_value(args.clone()).map_err(|e| format!("Invalid params: {}", e))?;

    let facts = state
        .storage
        .search(&tool_args.query, tool_args.limit)
        .map_err(|e| format!("Search error: {}", e))?;

    // Check for pending notifications and inject at the top
    let notification_header = get_notification_header(state);

    // Check for onboarding - show @readme on first search of session
    let onboarding_content = get_onboarding_content(state);

    if facts.is_empty() {
        let mut result = onboarding_content;
        result.push_str(&notification_header);
        result.push_str("No facts found matching your query.");
        return Ok(result);
    }

    // Check if results contain proposals/todos that might benefit from voting
    let voting_hint = get_voting_hint(state, &facts);

    let mut result = onboarding_content;
    result.push_str(&notification_header);
    result.push_str(&format!("Found {} facts:\n\n", facts.len()));

    for fact in &facts {
        result.push_str(&format!(
            "## {} (meh-{})\n**Path:** {}\n**Trust:** {:.2}\n{}\n\n---\n\n",
            fact.title,
            fact.id,
            fact.path,
            fact.trust_score,
            fact.summary.as_deref().unwrap_or(&fact.content)
        ));
    }

    // Warn if results may be truncated
    if facts.len() >= tool_args.limit as usize {
        result.push_str(&format!(
            "\n⚠️ Note: results limited to {} items; there may be more matching facts. Narrow your query or increase `limit`.\n",
            tool_args.limit
        ));
    }

    // Add voting hint at the end if applicable
    if !voting_hint.is_empty() {
        result.push_str(&voting_hint);
    }

    Ok(result)
}

/// Search across multiple KBs
pub fn do_federated_search(_state: &ServerState, args: &Value) -> ToolResult {
    let tool_args: MehFederatedSearchTool =
        serde_json::from_value(args.clone()).map_err(|e| format!("Invalid params: {}", e))?;

    let config = Config::load().map_err(|e| format!("Config error: {}", e))?;

    // Determine which KBs to search
    let kbs_to_search: Vec<String> = if tool_args.kbs.is_empty() {
        config.kbs.search_order.clone()
    } else {
        tool_args.kbs.clone()
    };

    if kbs_to_search.is_empty() {
        return Err("No KBs configured in search_order".to_string());
    }

    let mut all_results: Vec<(String, Vec<Fact>)> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let timeout = Duration::from_secs(config.search.federated_timeout_secs);

    for kb_name in &kbs_to_search {
        match search_single_kb(
            &config,
            kb_name,
            &tool_args.query,
            tool_args.limit_per_kb,
            timeout,
        ) {
            Ok(facts) => all_results.push((kb_name.clone(), facts)),
            Err(e) => errors.push(e),
        }
    }

    // Build result
    let mut result = format!("🔍 Federated search for: \"{}\"\n\n", tool_args.query);
    let total_count: usize = all_results.iter().map(|(_, facts)| facts.len()).sum();

    if total_count == 0 && errors.is_empty() {
        result.push_str("No results found in any KB.\n");
    }

    for (kb_name, facts) in &all_results {
        if facts.is_empty() {
            continue;
        }
        result.push_str(&format!("## 📚 {} ({} results)\n\n", kb_name, facts.len()));
        for fact in facts {
            result.push_str(&format!(
                "- **{}** (meh-{})\n  Path: {} | Trust: {:.2}\n",
                fact.title, fact.id, fact.path, fact.trust_score
            ));
            if !fact.content.is_empty() {
                let summary = fact.summary.as_deref().unwrap_or(&fact.content);
                let short = if summary.len() > 100 {
                    &summary[..100]
                } else {
                    summary
                };
                result.push_str(&format!("  {}\n", short.replace('\n', " ")));
            }
            result.push('\n');
        }
    }

    if !errors.is_empty() {
        result.push_str("\n⚠️ Errors:\n");
        for e in &errors {
            result.push_str(&format!("  - {}\n", e));
        }
    }

    result.push_str(&format!(
        "\n📊 Total: {} results from {} KB(s)",
        total_count,
        all_results.len()
    ));

    Ok(result)
}

/// Search a single KB (helper for federated search)
fn search_single_kb(
    config: &Config,
    kb_name: &str,
    query: &str,
    limit: i64,
    timeout: Duration,
) -> Result<Vec<Fact>, String> {
    let kb_config = config
        .get_kb(kb_name)
        .ok_or_else(|| format!("{}: not found in config", kb_name))?;

    match kb_config.kb_type.as_str() {
        "sqlite" => search_sqlite_kb(config, kb_config, query, limit),
        "remote" => search_remote_kb(config, kb_name, kb_config, query, limit, timeout),
        other => Err(format!("{}: unknown type '{}'", kb_name, other)),
    }
}

/// Search local SQLite KB
fn search_sqlite_kb(
    config: &Config,
    kb_config: &crate::config::KbConfig,
    query: &str,
    limit: i64,
) -> Result<Vec<Fact>, String> {
    let db_path = if let Some(path) = &kb_config.path {
        PathBuf::from(path)
    } else {
        config.data_dir()
    };

    let storage =
        Storage::open(&db_path).map_err(|e| format!("{}: open error: {}", kb_config.name, e))?;

    storage
        .search(query, limit)
        .map_err(|e| format!("{}: search error: {}", kb_config.name, e))
}

/// Search remote KB via HTTP
fn search_remote_kb(
    config: &Config,
    kb_name: &str,
    kb_config: &crate::config::KbConfig,
    query: &str,
    limit: i64,
    timeout: Duration,
) -> Result<Vec<Fact>, String> {
    let server = config
        .get_server_for_kb(kb_name)
        .ok_or_else(|| format!("{}: no server configured", kb_name))?;

    let slug = kb_config
        .slug
        .as_ref()
        .ok_or_else(|| format!("{}: no slug configured", kb_name))?;

    let client = reqwest::blocking::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|e| format!("{}: client error: {}", kb_name, e))?;

    let encoded_query = encode_query(query);
    let url = format!(
        "{}/api/v1/kbs/{}/search?q={}&limit={}",
        server.url, slug, encoded_query, limit
    );

    let mut request = client.get(&url);
    if let Some(ref key) = server.api_key {
        request = request.header("X-API-Key", key);
    }

    let response = request
        .send()
        .map_err(|e| format!("{}: request error: {}", kb_name, e))?;

    if !response.status().is_success() {
        return Err(format!("{}: HTTP {}", kb_name, response.status()));
    }

    let json_facts: Vec<serde_json::Value> = response
        .json()
        .map_err(|e| format!("{}: parse error: {}", kb_name, e))?;

    // Convert to Fact structs
    let facts = json_facts
        .iter()
        .filter_map(|v| {
            let id = v["id"].as_str()?;
            let path = v["path"].as_str()?;
            let title = v["title"].as_str()?;
            let summary = v["summary"].as_str();
            let trust = v["trust_score"].as_f64().unwrap_or(0.5) as f32;

            let mut fact = Fact::new(path, title, summary.unwrap_or(""));
            fact.trust_score = trust;
            if let Ok(ulid) = Ulid::from_string(id.trim_start_matches("meh-")) {
                fact.id = ulid;
            }
            Some(fact)
        })
        .collect();

    Ok(facts)
}

/// Simple URL encoding for query string
fn encode_query(query: &str) -> String {
    query
        .chars()
        .map(|c| match c {
            ' ' => "+".to_string(),
            c if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' => c.to_string(),
            c => format!("%{:02X}", c as u8),
        })
        .collect()
}

/// Check if search results contain proposals that AI should vote on
fn get_voting_hint(state: &ServerState, facts: &[Fact]) -> String {
    let proposal_paths = ["@meh/todo/", "@meh/board/", "@meh/rfc/", "@meh/proposal/"];

    // Filter to proposals that this session hasn't voted on yet
    let unvoted_count = facts
        .iter()
        .filter(|f| proposal_paths.iter().any(|p| f.path.contains(p)))
        .filter(|f| {
            !state
                .storage
                .has_session_voted(&f.id, &state.session_id)
                .unwrap_or(true)
        })
        .count();

    if unvoted_count > 0 {
        format!(
            "\n💡 **Tip:** Found {} proposal(s)/TODO(s) you haven't voted on yet. To vote, use `meh_bulk_vote` with votes array, e.g.: `meh_bulk_vote({{\"votes\": [{{\"fact_id\": \"meh-01XXX\", \"vote\": \"+1\", \"reason\": \"Agree because...\"}}]}})`\n",
            unvoted_count
        )
    } else {
        String::new()
    }
}

/// Get notification header to inject in search results
fn get_notification_header(state: &ServerState) -> String {
    if let Ok(notif_storage) = state.open_notification_storage() {
        let critical = notif_storage.critical_count(&state.session_id).unwrap_or(0);
        let pending = notif_storage.pending_count(&state.session_id).unwrap_or(0);

        if critical > 0 {
            return format!(
                "🔴 **{} critical notification(s)!** Use `meh_get_notifications` to view.\n\n",
                critical
            );
        } else if pending > 5 {
            return format!(
                "📬 {} new notification(s). Use `meh_get_notifications` to view.\n\n",
                pending
            );
        }
    }
    String::new()
}

/// Get onboarding content (@readme) for first search of session
fn get_onboarding_content(state: &ServerState) -> String {
    if let Ok(notif_storage) = state.open_notification_storage() {
        let already_shown = notif_storage
            .is_onboarding_shown(&state.session_id)
            .unwrap_or(false);

        if already_shown {
            return String::new();
        }

        let _ = notif_storage.set_onboarding_shown(&state.session_id);

        // Try KB-specific readme first: @readme/{kb_name}
        let kb_readme_path = format!("@readme/{}", state.kb_name);
        let readme = state
            .storage
            .get_by_path(&kb_readme_path)
            .ok()
            .and_then(|facts| facts.into_iter().next())
            .or_else(|| {
                // Fall back to generic @readme
                state
                    .storage
                    .get_by_path("@readme")
                    .ok()
                    .and_then(|facts| facts.into_iter().next())
            });

        if readme.is_some() {
            // Custom @readme exists - show short hint to read it
            return "📖 **Welcome!** Use `mcp_meh_meh_get_fact(id_or_path=\"@readme\")` for full instructions.\n\n".to_string();
        } else {
            // No @readme - show default short hint
            return get_default_onboarding_hint();
        }
    }
    String::new()
}

/// Short default onboarding hint when no @readme exists
fn get_default_onboarding_hint() -> String {
    r#"📖 **Welcome to meh knowledge base!**

**Quick start (4 merged tools):**
- `mcp_meh_meh_facts({{"action": "search", "query": "..."}})` - search knowledge
- `mcp_meh_meh_facts({{"action": "browse", "path": "@"}})` - see structure
- `mcp_meh_meh_write({{"action": "add", "path": "@path", "content": "..."}})` - add knowledge
- `mcp_meh_meh_facts({{"action": "get", "id_or_path": "@readme"}})` - full instructions

**Tip:** Add `@readme` fact with instructions, or `@readme/{{kb}}` for this KB specifically.

"#
    .to_string()
}
