# Neon Context Server — Zed Extension

A **community-maintained** [Zed editor](https://zed.dev) context server for [Neon](https://neon.tech) serverless Postgres. Query databases, explore schemas, list projects and branches — all from the Zed Assistant Panel.

## Features

| Tool | Description |
|------|-------------|
| `neon_query` | Execute read-only SQL queries with a 500-row result cap |
| `neon_schema` | Display full database schema grouped by table |
| `neon_list_tables` | List all user tables, optionally filtered by schema |
| `neon_describe_table` | Show columns, indexes (with sizes), and constraints |
| `neon_list_schemas` | List all schemas in the database |
| `neon_list_databases` | List databases in the current project |
| `neon_list_projects` | Discover available Neon project IDs |
| `neon_list_branches` | List branches in a project |
| `neon_explain` | Human-readable PostgreSQL execution plan tree |
| `neon_get_connection_string` | Connection URI with password masked |

## Installation

### Prerequisites

- [Zed](https://zed.dev/download) editor (latest stable)
- Rust toolchain for building: `rustup`, `cargo`
- A [Neon account](https://console.neon.tech/signup) and project

### Build & Install

```bash
git clone https://github.com/peterkyle01/neon-context-server.git
cd neon-context-server
cd server && cargo build --release && cd ..
cp server/target/release/neon-context-server neon-context-server
```

Then in Zed: **Extensions** (`Ctrl+Shift+X`) → **Install Dev Extension** → select this directory.

## Configuration

The **only required setting** is your Neon API key:

```json
{
  "context_servers": {
    "neon-context-server": {
      "settings": {
        "neon_api_key": "napi_your_key_here"
      }
    }
  }
}
```

Everything else (project, branch, database) is auto-discovered at runtime.

### All Settings

| Setting | Required | Default | Description |
|---------|----------|---------|-------------|
| `neon_api_key` | **Yes** | — | Neon API key |
| `neon_project_id` | No | auto-discovered | Pin to a specific project |
| `neon_database` | No | `neondb` | Default database |
| `neon_branch` | No | default branch | Default branch |
| `database_url` | No | — | Direct connection string (bypasses API) |

## Security

- **Write-protected**: INSERT, UPDATE, DELETE, DROP, TRUNCATE, and other DDL/DML are blocked
- **Password masked**: Connection string output hides the password
- **Row-limited**: Query results capped at 500 rows
- **Info leak blocked**: `SHOW ALL`, `SHOW neon.*`, and `pg_settings` access are blocked
- **CTE bypass blocked**: Write operations inside `WITH` clauses are detected

## Development

```bash
# Build the server binary
cd server && cargo build --release

# Run locally for testing
NEON_API_KEY=napi_xx NEON_PROJECT_ID=your-project cargo run

# The WASM extension wrapper compiles automatically when installed in Zed
```

### Project Structure

```
├── extension.toml         # Zed extension manifest
├── Cargo.toml             # WASM extension wrapper
├── src/lib.rs             # Extension — reads settings, launches binary
├── server/                # Native MCP server binary
│   ├── Cargo.toml
│   └── src/               # main.rs, config.rs, neon_client.rs, commands.rs
├── configuration/         # Settings schema + install docs
└── README.md
```

## Credits

Built by [Peter Kyle](mailto:kylepeterkoine4@gmail.com). Community extension — not affiliated with or endorsed by Neon, Inc.
