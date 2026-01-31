# memoraihub-cli (meh)

> **Wikipedia for AI** — A scalable knowledge base that AI agents can read and write.

**memoraihub-cli** is a local-first knowledge management system designed for AI agents. The CLI command is `meh` (Memory Hub). Think of it as Wikipedia, but:
- **Multiple pages** organized in hierarchical paths
- **AI-native** — designed for agents to read and write
- **Append-only** — no destructive updates, full history
- **Scalable** — designed for millions of facts

## Quick Start

```bash
# Initialize in current directory
meh init

# Add a fact
meh add --path "@products/api/timeout" "API timeout is 30 seconds for all endpoints"

# Search
meh search "timeout"

# Browse structure
meh tree
```

## For AI Agents (MCP)

meh exposes a **Model Context Protocol (MCP)** server for AI integration.

### VS Code Configuration

Add to `.vscode/mcp.json` in your workspace:

```json
{
  "servers": {
    "meh": {
      "type": "stdio",
      "command": "/path/to/meh",
      "args": ["serve", "--db", "/path/to/.meh/data.db"],
      "env": {}
    }
  }
}
```

### Claude Desktop / Cursor Configuration

```json
{
  "mcpServers": {
    "meh": {
      "command": "meh",
      "args": ["serve", "--db", "/path/to/.meh/data.db"]
    }
  }
}
```

### Server Options

```bash
meh serve                          # Auto-detect .meh/data.db
meh serve --db /path/to/data.db    # Explicit database path
meh serve --auto-init              # Create database if missing
```

### Available Tools

| Tool | Description |
|------|-------------|
| `meh_search` | Full-text search across all facts |
| `meh_get_fact` | Get a single fact by ID or path |
| `meh_browse` | List paths with pagination (limit, cursor) |
| `meh_add` | Add a new fact |
| `meh_correct` | Create a correction (supersedes original) |
| `meh_extend` | Add information to existing fact |
| `meh_deprecate` | Mark fact as outdated |

### Example MCP Usage

```
AI: I need to find information about API configuration.
→ meh_search(query="API configuration")

AI: Let me see what's in the products section.
→ meh_browse(path="@products")

AI: I'll document this new discovery.
→ meh_add(path="@products/auth/session", content="Session expires after 24h of inactivity")
```

## CLI Commands

```bash
# Core operations
meh add --path <path> <content>     # Add new fact
meh show <id-or-path>               # Show fact details
meh search <query>                  # Full-text search

# Navigation
meh ls [path]                       # List children at path
meh tree [path]                     # Show tree structure

# Modifications (append-only)
meh correct <id> <new-content>      # Create correction
meh extend <id> <additional>        # Extend with more info
meh deprecate <id> [--reason]       # Mark as deprecated

# Server
meh serve                           # Start MCP server (STDIO)
meh serve --db <path>               # Use specific database
meh serve --auto-init               # Create DB if missing
```

## Path System

Facts are organized in hierarchical paths, like a filesystem:

```
@                           # Root
├── @products/
│   ├── @products/alpha/
│   │   ├── @products/alpha/api/timeout
│   │   └── @products/alpha/api/rate-limit
│   └── @products/beta/
├── @architecture/
│   └── @architecture/decisions/auth-flow
└── @bugs/
    └── @bugs/login-issue-2024
```

**Path conventions:**
- Always start with `@`
- Use `/` as separator
- Lowercase, kebab-case recommended
- No depth limit

## Architecture

### Append-Only Model

meh never modifies or deletes facts. Instead:

- **Corrections** create new facts that supersede the original
- **Extensions** link additional information to existing facts
- **Deprecations** mark facts as outdated (but keep them)

This ensures full audit trail and prevents data loss.

### Storage

- SQLite with FTS5 for full-text search
- WAL mode for better concurrent access
- Single `.meh/data.db` file per project
- Works offline, no external dependencies

### Scalability

Designed to handle millions of facts:
- Paginated browse with cursor support
- Indexed paths for fast prefix queries
- FTS5 for efficient full-text search
- Busy timeout for concurrent writers

### Detail Levels

When querying, you can request different levels of detail:

| Level | Returns | Use Case |
|-------|---------|----------|
| L0 | Path only | Catalog, listing |
| L1 | Path + title + trust | Index, overview |
| L2 | + summary (1-3 sentences) | Search results |
| L3 | Full content | Reading a fact |

## Trust Scoring

Each fact has a trust score (0.0-1.0) based on:
- Author type (human > AI)
- Number of confirmations
- Age and usage patterns
- Correction history

## Building

```bash
cargo build --release
```

Binary will be at `target/release/meh.exe` (Windows) or `target/release/meh` (Unix).

## License

Apache-2.0
