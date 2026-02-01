//! Notification handlers for MCP

use serde_json::Value;
use ulid::Ulid;

use super::ToolResult;
use crate::core::notifications::{Category, Priority, Subscription};
use crate::mcp::state::ServerState;
use crate::mcp::tools::{MehAckNotificationsTool, MehGetNotificationsTool};

/// Get pending notifications for this session
pub fn do_get_notifications(state: &ServerState, args: &Value) -> ToolResult {
    let tool_args: MehGetNotificationsTool =
        serde_json::from_value(args.clone()).map_err(|e| format!("Invalid params: {}", e))?;

    let notif_storage = state
        .open_notification_storage()
        .map_err(|e| format!("Notification storage error: {}", e))?;

    // Get notifications for this session
    let notifications = notif_storage
        .get_for_session(&state.session_id, tool_args.limit as usize)
        .map_err(|e| format!("Get notifications error: {}", e))?;

    // Apply additional priority filter if specified
    let notifications: Vec<_> = if let Some(ref p) = tool_args.priority_min {
        if let Some(min_p) = Priority::parse_str(p) {
            notifications
                .into_iter()
                .filter(|n| n.priority >= min_p)
                .collect()
        } else {
            notifications
        }
    } else {
        notifications
    };

    let pending_count = notif_storage
        .pending_count(&state.session_id)
        .map_err(|e| format!("Count error: {}", e))?;

    if notifications.is_empty() {
        return Ok(format!(
            "✓ No new notifications for this session (pending: {})",
            pending_count
        ));
    }

    let mut output = format!("📬 {} new notification(s):\n\n", notifications.len());

    for notif in &notifications {
        let priority_icon = match notif.priority {
            Priority::Critical => "🔴",
            Priority::High => "🟠",
            Priority::Normal => "🟢",
        };

        let cat_icon = match notif.category {
            Category::Facts => "📝",
            Category::Ci => "🔧",
            Category::Security => "🔒",
            Category::Docs => "📚",
            Category::System => "⚙️",
            Category::Custom(_) => "📌",
        };

        output.push_str(&format!(
            "{} {} {} [{}]\n",
            priority_icon,
            cat_icon,
            notif.title,
            notif.category.as_str()
        ));
        output.push_str(&format!("   {}\n", notif.summary));
        if let Some(path) = &notif.path {
            output.push_str(&format!("   📁 {}\n", path));
        }
        output.push_str(&format!("   ID: meh-{}\n\n", notif.id));
    }

    // Auto-mark as seen
    if let Some(last) = notifications.last() {
        let _ = notif_storage.mark_seen(&state.session_id, &last.id);
    }

    output.push_str(&format!(
        "Session: {} | Pending: {}",
        state.session_id, pending_count
    ));
    Ok(output)
}

/// Acknowledge notifications
pub fn do_ack_notifications(state: &ServerState, args: &Value) -> ToolResult {
    let tool_args: MehAckNotificationsTool =
        serde_json::from_value(args.clone()).map_err(|e| format!("Invalid params: {}", e))?;

    let notif_storage = state
        .open_notification_storage()
        .map_err(|e| format!("Notification storage error: {}", e))?;

    // Check for "*" meaning all
    if tool_args.notification_ids.len() == 1 && tool_args.notification_ids[0] == "*" {
        let count = notif_storage
            .acknowledge_all(&state.session_id)
            .map_err(|e| format!("Ack error: {}", e))?;
        return Ok(format!(
            "✓ Acknowledged {} notification(s) for session {}",
            count, state.session_id
        ));
    }

    // For specific IDs, mark up to the last one as seen
    if let Some(last_id) = tool_args.notification_ids.last() {
        let ulid_str = last_id.strip_prefix("meh-").unwrap_or(last_id);
        if let Ok(ulid) = Ulid::from_string(ulid_str) {
            notif_storage
                .mark_seen(&state.session_id, &ulid)
                .map_err(|e| format!("Mark seen error: {}", e))?;
        }
    }

    Ok(format!(
        "✓ Marked {} notification(s) as seen",
        tool_args.notification_ids.len()
    ))
}

/// Configure notification subscription
pub fn do_subscribe(state: &ServerState, args: &Value) -> ToolResult {
    let show = args["show"].as_bool().unwrap_or(false);

    let notif_storage = state
        .open_notification_storage()
        .map_err(|e| format!("Notification storage error: {}", e))?;

    if show {
        let (_, sub) = notif_storage
            .get_or_create_session(&state.session_id)
            .map_err(|e| format!("Session error: {}", e))?;

        let cats: Vec<&str> = sub.categories.iter().map(|c| c.as_str()).collect();
        let cats_str = if cats.is_empty() {
            "all".to_string()
        } else {
            cats.join(", ")
        };
        let paths_str = if sub.path_prefixes.is_empty() {
            "all".to_string()
        } else {
            sub.path_prefixes.join(", ")
        };
        let priority_str = sub
            .priority_min
            .map(|p| p.as_str().to_string())
            .unwrap_or_else(|| "all".to_string());

        return Ok(format!(
            "📋 Current subscription for session {}:\n   Categories: {}\n   Paths: {}\n   Min priority: {}",
            state.session_id, cats_str, paths_str, priority_str
        ));
    }

    // Build subscription from args
    let mut sub = Subscription::default();

    if let Some(cats) = args["categories"].as_array() {
        let cat_list: Vec<Category> = cats
            .iter()
            .filter_map(|v| v.as_str())
            .map(Category::parse_str)
            .collect();
        sub = sub.categories(cat_list);
    }

    if let Some(paths) = args["path_prefixes"].as_array() {
        let path_list: Vec<String> = paths
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        sub = sub.paths(path_list);
    }

    if let Some(p) = args["priority_min"].as_str() {
        if let Some(prio) = Priority::parse_str(p) {
            sub = sub.priority_min(prio);
        }
    }

    notif_storage
        .update_subscription(&state.session_id, &sub)
        .map_err(|e| format!("Update subscription error: {}", e))?;

    let cats: Vec<&str> = sub.categories.iter().map(|c| c.as_str()).collect();
    let cats_str = if cats.is_empty() {
        "all".to_string()
    } else {
        cats.join(", ")
    };
    let paths_str = if sub.path_prefixes.is_empty() {
        "all".to_string()
    } else {
        sub.path_prefixes.join(", ")
    };
    let priority_str = sub
        .priority_min
        .map(|p| p.as_str().to_string())
        .unwrap_or_else(|| "all".to_string());

    Ok(format!(
        "✓ Subscription updated for session {}:\n   Categories: {}\n   Paths: {}\n   Min priority: {}",
        state.session_id, cats_str, paths_str, priority_str
    ))
}
