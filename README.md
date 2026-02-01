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
meh init              # Creates .meh/data.db
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

## 📋 Configuration Details

### MCP Server Options

| Option | Description |
| ------ | ----------- |
| `--db <path>` | Path to database (default: `.meh/data.db`) |
| `--auto-init` | Create database if it doesn't exist |

### Claude Desktop / Cursor

**Windows:**
```json
{
  "mcpServers": {
    "meh": {
      "command": "C:\\path\\to\\meh.exe",
      "args": ["serve", "--db", "C:\\path\\to\\.meh\\data.db"]
    }
  }
}
```

**Linux/macOS:**
```json
{
  "mcpServers": {
    "meh": {
      "command": "/path/to/meh",
      "args": ["serve", "--db", "/path/to/.meh/data.db"]
    }
  }
}
```

### Environment Variables

```json
{
  "servers": {
    "meh": {
      "type": "stdio",
      "command": "meh",
      "args": ["serve"],
      "env": {
        "MEH_DATABASE": "/project/.meh/data.db"
      }
    }
  }
}
```

---

## 🔧 Write Policy Configuration

By default, AI can write freely. You can change this.

Create `.meh/config.toml`:

```toml
[kbs]
primary = "local"

[[kbs.kb]]
name = "local"
kb_type = "sqlite"
write = "ask"    # "allow" | "deny" | "ask"
```

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

| Tool | Description |
| ---- | ----------- |
| `meh_search` | Search facts |
| `meh_get_fact` | Get fact details |
| `meh_browse` | Browse path structure |
| `meh_add` | Add new fact |
| `meh_correct` | Correct existing fact |
| `meh_extend` | Extend fact with additional info |
| `meh_deprecate` | Mark as outdated |
| `meh_get_notifications` | Get notifications |
| `meh_ack_notifications` | Acknowledge notifications |
| `meh_subscribe` | Subscribe to categories/paths |
| `meh_bulk_vote` | Record multiple votes in a single call (creates extensions per vote) |

---

## 📁 AI Instructions

Add to `.github/copilot-instructions.md`:

```markdown
## 🧠 Knowledge Base (meh)

You have access to a knowledge base via MCP. Use it to:
- **Search** before answering — the answer might already exist
- **Save** discoveries, decisions, bugs for future sessions

### Workflow
1. Before answering: `meh_search("topic")`
2. At session start: `meh_browse(path="@")`
3. After discoveries: `meh_add(path="@project/topic", content="...")`

### Path Conventions
- `@project/bugs/*` — found bugs
- `@project/architecture/*` — architecture decisions
- `@project/api/*` — API documentation
```

---

## 💻 CLI — Basic Commands

```bash
# Adding
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

# Statistics
meh stats

# GC (remove old deprecated facts)
meh gc --dry-run
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
├── config.toml      # Configuration (optional)
├── data.db          # Main facts database
├── notifications.db # Sessions and notifications
└── pending_queue.db # Queue for remote KB writes
```

### Full Configuration

See [config.example.toml](config.example.toml) for all options.

---

## 📄 License

Apache-2.0
