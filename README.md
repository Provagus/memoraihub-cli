# meh — Knowledge Base for AI Agents

> Local-first knowledge base for AI. Store, search, and share knowledge across sessions.

## ⚡ Quickstart (3 steps)

### 1. Build

```bash
cd memoraihub-cli
cargo build --release
```

Binary location:
- **Windows:** `target\release\meh.exe`
- **Linux/macOS:** `target/release/meh`

### 2. Initialize in your project

```bash
cd /your/project
meh init              # Creates .meh/data.db and config.toml
meh add --path "@readme" "# My Project KB\n\nInstructions for AI..."
```

### 3. Connect to AI (VS Code)

Create `.vscode/mcp.json`:

**Windows:**
```json
{
  "servers": {
    "meh": {
      "type": "stdio",
      "command": "C:\\path\\to\\meh.exe",
      "args": ["serve", "--auto-init"]
    }
  }
}
```

**Linux/macOS:**
```json
{
  "servers": {
    "meh": {
      "type": "stdio",
      "command": "/path/to/meh",
      "args": ["serve", "--auto-init"]
    }
  }
}
```

**Done!** AI now has access to your project's knowledge base.

---

## 📋 Configuration

Config is auto-generated on first run at `~/.meh/config.toml` (global) or `.meh/config.toml` (local).

### Adding Knowledge Bases

Use the interactive wizard:

```bash
meh kbs add
```

This guides you step-by-step through adding a new KB (local SQLite or remote server).

### Manual Config Example

```toml
[kbs]
primary = "local"
search_order = ["local", "company"]

# Define servers with authentication
[[servers]]
name = "company-server"
url = "https://kb.company.com"
api_key = "meh_your_api_key_here"  # Or use env: api_key_env = "MEH_COMPANY_KEY"
timeout_secs = 30

# Define knowledge bases
[[kbs.kb]]
name = "local"
kb_type = "sqlite"
write = "allow"

[[kbs.kb]]
name = "company"
kb_type = "remote"
server = "company-server"  # References [[servers]] entry
slug = "my-team"
write = "allow"
```

See [config.example.toml](config.example.toml) for all options.

### MCP Server Options

| Option | Description |
| ------ | ----------- |
| `--stdio` | Run in MCP mode (JSON-RPC over stdin/stdout) |
| `--auto-init` | Create database and config if they don't exist |

### Environment Variables

| Variable | Description |
| -------- | ----------- |
| `MEH_DATABASE` | Path to database file (default: `.meh/meh.db`) |
| `MEH_CONFIG` | Path to config file (default: `.meh/config.toml`) |

---

## 🔧 Write Policy

Control what AI can write.

| Policy | Behavior |
| ------ | -------- |
| `allow` | AI writes immediately (default) |
| `deny` | AI cannot write (read-only) |
| `ask` | AI writes go to queue, user must approve |

With `write = "ask"`:

```bash
meh pending list              # View pending writes
meh pending approve meh-xxx   # Approve
meh pending reject meh-xxx    # Reject
```

---

## 🤖 MCP Tools for AI

Meh provides 4 unified tools (Merged Tools v2):

| Tool | Actions | Purpose |
| ---- | ------- | ------- |
| `mcp_meh_meh_facts` | search, get, browse, federated_search | FTS search, fetch fact, browse paths, multi-KB search |
| `mcp_meh_meh_write` | add, correct, extend, deprecate, bulk_vote | Create, supersede, extend, deprecate, batch votes |
| `mcp_meh_meh_notify` | get, ack, subscribe | Session notifications (pull, acknowledge, manage subscriptions) |
| `mcp_meh_meh_context` | list_kbs, switch_kb, switch_context, show | List/show/switch knowledge bases and contexts |

**Tool Naming:**
- VS Code Copilot: Always use prefixed names (`mcp_meh_meh_facts`, etc.) with an `action` parameter
- Other MCP clients: May use shorter names (`meh_facts`, `meh_write`, etc.)

**Example usage (VS Code):**
```json
{"tool": "mcp_meh_meh_facts", "action": "search", "query": "authentication"}
{"tool": "mcp_meh_meh_write", "action": "add", "path": "@project/bug", "content": "# Bug..."}
```

Onboarding: keep a fact at `@readme`; the MCP server auto-displays it on the first search in a session if present.

---

## 📁 AI Instructions

Add to `.github/copilot-instructions.md`:

```markdown
## 🧠 Knowledge Base (meh)

You have access to a knowledge base via MCP. Use it to:
- **Search** before answering — the answer might already exist
- **Save** discoveries, decisions, bugs for future sessions

### Workflow
1. At session start: `mcp_meh_meh_facts({"action": "browse", "path": "@", "mode": "tree", "depth": 2})`
2. Read onboarding: `mcp_meh_meh_facts({"action": "get", "id_or_path": "@readme"})`
3. Before answering: `mcp_meh_meh_facts({"action": "search", "query": "your topic"})`
4. After discoveries: `mcp_meh_meh_write({"action": "add", "path": "@project/topic", "content": "# Title\n\n..."})`

### Path Conventions
- `@project/bugs/*` — found bugs
- `@project/architecture/*` — architecture decisions  
- `@project/api/*` — API documentation
- `@project/changelog/*` — changes and refactoring
```

---

## 💻 CLI Commands

```bash
# Initialize
meh init                     # Create .meh/ in current directory
meh init --global            # Create ~/.meh/ (global)

# Adding facts
meh add --path "@project/api/timeout" "Timeout = 30s"

# Searching
meh search "timeout"

# Browsing
meh ls @project
meh tree

# Modifications (append-only)
meh correct <id> "Corrected content"
meh extend <id> "Additional info"
meh deprecate <id> --reason "Outdated"

# Knowledge bases
meh kbs add                  # Interactive wizard to add KB to config
meh kbs list                 # List remote KBs (requires server)
meh kbs use <slug>           # Set default remote KB

# Pending review (when write = "ask")
meh pending list
meh pending approve <id>
meh pending reject <id>

# Maintenance
meh stats                    # Show statistics
meh gc --dry-run             # Preview garbage collection
meh gc                       # Remove old deprecated facts
```

---

## 📚 More Information

### Architecture

- **Append-only** — facts are never deleted, only superseded/deprecated
- **SQLite + FTS5** — fast full-text search
- **Trust scoring** — each fact has trust 0.0-1.0
- **Per-session** — each MCP session has its own notifications

### Files

```
.meh/
├── config.toml      # Configuration (auto-generated)
├── meh.db           # Main facts database (SQLite + FTS5)
├── notifications.db # Sessions and notifications
└── pending_queue.db # Queue for write=ask policy
```

### Full Configuration

See [config.example.toml](config.example.toml) for all options.

---

## 📄 License

Apache-2.0
