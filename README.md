# memoraihub-cli (meh)

> **Knowledge Base for AI Agents** — A local-first, append-only knowledge management system.

`meh` (Memory Hub) is designed for AI agents to **read and write persistent knowledge** across sessions. Think of it as a Wikipedia that AI can edit — with full history, trust scoring, and hierarchical organization.

## 🚀 Quick Start

### Installation

```bash
# Build from source
cd memoraihub-cli
cargo build --release

# Binary location
# Windows: target/release/meh.exe
# Linux/macOS: target/release/meh
```

### First Use

```bash
# Initialize in current directory (creates .meh/data.db)
meh init

# Add your first fact
meh add --path "@project/config" "API timeout is 30 seconds"

# Search
meh search "timeout"

# Browse structure
meh tree
```

### Remote Usage

```bash
# Set remote context (persists across commands)
meh context set http://localhost:3000/my-kb

# Now all commands use remote KB
meh search "timeout"
meh add --path "@project/api" "New discovery"

# Check current context
meh context show

# Switch back to local
meh context clear
```

## 🤖 For AI Agents (MCP Integration)

meh exposes a **Model Context Protocol (MCP)** server with 10 tools.

### VS Code Setup

Add to `.vscode/mcp.json`:

```json
{
  "servers": {
    "meh": {
      "type": "stdio",
      "command": "C:/path/to/meh.exe",
      "args": ["serve", "--db", "C:/path/to/.meh/data.db"]
    }
  }
}
```

### Claude Desktop / Cursor

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

### Available MCP Tools

| Tool | Description |
|------|-------------|
| `meh_search` | Full-text search with filters (path, tags, trust, token-budget) |
| `meh_get_fact` | Get fact by ID or path (with history option) |
| `meh_browse` | List paths (like `ls` or `tree`) with pagination |
| `meh_add` | Add a new fact |
| `meh_correct` | Create correction (supersedes original) |
| `meh_extend` | Add information to existing fact |
| `meh_deprecate` | Mark fact as outdated |
| `meh_get_notifications` | Get pending notifications (per session) |
| `meh_ack_notifications` | Acknowledge notifications |
| `meh_subscribe` | Subscribe to categories/paths |

---

## 📘 How AI Should Use meh

### Copilot Instructions Example

Add to `.github/copilot-instructions.md` or system prompt:

```markdown
## 🧠 Knowledge Base (meh)

You have access to a persistent knowledge base via MCP tools. Use it to:
- **Remember** discoveries, decisions, and learnings across sessions
- **Search** before answering — the answer might already exist
- **Document** bugs, architecture decisions, and project conventions

### Workflow

1. **Before answering** — check if knowledge exists:
   ```
   meh_search("topic of the question")
   ```

2. **At session start** — see what's in the knowledge base:
   ```
   meh_browse(path="@")
   ```

3. **After discoveries** — save for future sessions:
   ```
   meh_add(path="@project/topic", content="What you learned")
   ```

### When to Add Facts

✅ **Add:**
- Found bugs and their fixes
- Architecture decisions with reasoning
- Project conventions and patterns
- API behaviors discovered through testing
- Configuration that wasn't obvious

❌ **Don't add:**
- Temporary notes
- Things obvious from code
- User-specific preferences

### Path Conventions

| Path | Use for |
|------|---------|
| `@project/bugs/*` | Found bugs |
| `@project/architecture/*` | Design decisions |
| `@project/api/*` | API documentation |
| `@project/conventions/*` | Code style, patterns |

### Example Session

```
User: "How does authentication work?"

AI: meh_search("authentication")
    → Found: @project/architecture/auth-flow

AI: meh_get_fact("@project/architecture/auth-flow")
    → "JWT tokens with 24h expiry, refresh via /auth/refresh"

AI: [Answers user with knowledge from meh]
```
```

---

## � Templates

The `templates/` folder contains ready-to-use templates:

### `copilot-instructions.md.template`

Copy to `.github/copilot-instructions.md` to give AI agents instructions on how to use meh:

```bash
cp templates/copilot-instructions.md.template .github/copilot-instructions.md
# Then customize for your project
```

### `readme.md.template`

Use as a starting point for `@readme` fact - the first thing AI reads when entering your KB:

```bash
# Add as a fact
meh add --path "@readme" --tags "onboarding,ai" "$(cat templates/readme.md.template)"
# Then edit with your project details
```

The `@readme` fact serves as onboarding for AI agents - explaining the KB structure, conventions, and workflow.

### Auto-Display on First Search

When an AI agent connects via MCP and performs their first `meh_search`, the `@readme` fact is **automatically displayed** at the top of results. This ensures every AI session starts with proper onboarding.

- Shown only once per MCP session
- No spam on subsequent searches
- Tip included to view again with `meh_get_fact`

---

## �📂 Path System

Facts are organized in hierarchical paths:

```
@                               # Root
├── @project/
│   ├── @project/bugs/
│   │   └── @project/bugs/login-issue
│   ├── @project/architecture/
│   │   └── @project/architecture/auth-flow
│   └── @project/api/
│       ├── @project/api/timeout
│       └── @project/api/rate-limits
└── @notes/
    └── @notes/meeting-2026-01-31
```

**Conventions:**
- Start with `@`
- Use `/` separator
- Lowercase, kebab-case
- No depth limit

---

## 🖥️ CLI Commands

### Core Operations

```bash
meh add --path "@path/to/fact" "Content of the fact"
meh add --path "@path" --tags "bug,critical" "Content"

meh show @path/to/fact              # Show fact
meh show meh-01ABC123               # Show by ID
meh show @path --with-history       # Show version history

meh search "query"                  # Full-text search
meh search "query" --path "@project" --limit 10
meh search "query" --min-trust 0.7 --active-only
```

### Navigation

```bash
meh ls                    # List root
meh ls @project           # List children
meh tree                  # Full tree
meh tree @project         # Subtree
```

### Modifications (Append-Only)

```bash
meh correct <id> "New corrected content"
meh extend <id> "Additional information"
meh deprecate <id> --reason "Outdated"
```

### Context Management

```bash
meh context                    # Show current context (local/remote)
meh context show               # Same as above
meh context set http://url/kb  # Set remote KB (persists)
meh context set other-kb       # Change KB on same server
meh context set local          # Switch to local mode
meh context clear              # Clear remote settings
```

### Notifications

```bash
meh notifications              # List pending
meh notifications count        # Show counts
meh notifications ack all      # Acknowledge all
meh notifications subscribe --categories "facts,security"
```

> **Hint:** After most commands, you'll see a notification hint if there are pending notifications:
> `📬 3 notification(s) pending (meh notifications)`

### Statistics

```bash
meh stats           # Show database statistics
meh stats --json    # JSON output
```

### MCP Server

```bash
meh serve                          # Start MCP server (STDIO)
meh serve --db /path/to/data.db    # Explicit database
meh serve --auto-init              # Create DB if missing
```

---

## 🏗️ Architecture

### Append-Only Model

meh **never modifies or deletes** facts:

- **Corrections** → Create new fact that supersedes original
- **Extensions** → Link additional info to existing fact
- **Deprecations** → Mark as outdated (but keep it)

This ensures full audit trail and prevents data loss.

### Storage

- SQLite with FTS5 full-text search
- WAL mode for concurrent access
- Single `.meh/data.db` file
- Works offline, no external dependencies

### Detail Levels

| Level | Returns | Use Case |
|-------|---------|----------|
| L0 Catalog | Path only | Listing |
| L1 Index | Path + title + trust | Overview |
| L2 Summary | + summary | Search results |
| L3 Full | Full content | Reading |

### Trust Scoring

Each fact has trust score (0.0-1.0) based on:
- Author type (Human: 0.8, AI: 0.5)
- Source (Local: 1.0, Global: 0.7)
- Age decay (0.5%/day after 90 days)
- Confirmation boosts (+0.1 each)

### Session-Based Features

Each AI session (MCP connection) has independent:
- **Read cursor** — what notifications were seen
- **Subscription preferences** — categories, paths, priority filters
- **Onboarding tracking** — `@readme` shown once per session

This allows multiple AI agents to work on the same KB without conflicts.

### AI Onboarding (`@readme`)

Create a `@readme` fact to onboard AI agents automatically:

```bash
meh add --path "@readme" --tags "onboarding" "# My Project KB

## How to use this knowledge base
...
"
```

AI agents see this on their first search. Include:
- What this KB is about
- Path conventions used
- What to document vs skip
- How to vote on proposals (use `meh_extend`)

---

## 📊 Example: Real Usage

After using meh in a project, your knowledge base might look like:

```bash
$ meh stats
📊 Knowledge Base Statistics

  Total facts:      42
  ├── Active:       33 (78%)
  ├── Deprecated:   4 (9%)
  └── Superseded:   5 (11%)

📂 Top-level paths:
  @project (30 facts)
  @notes (10 facts)
  @archive (2 facts)

$ meh tree @project
📂 @project
├── architecture/
│   ├── auth-flow
│   ├── database-schema
│   └── api-versioning
├── bugs/
│   ├── login-race-condition
│   └── cache-invalidation
├── api/
│   ├── rate-limits
│   └── timeout-config
└── conventions/
    ├── error-handling
    └── naming-patterns

18 facts total
```

---

## 🔧 Building

```bash
# Debug build
cargo build

# Release build (optimized)
cargo build --release

# Run tests
cargo test --release
```

## 📄 License

Apache-2.0
