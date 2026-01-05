# sscli

SQL Server CLI for AI coding agents.

One install. Your agents automatically know how to inspect SQL Server databases, run safe queries, and export results.

## Quick Start (Agent Users)

**1. Install sscli**

```bash
cargo install sscli
```

**2. Teach your agents**

```bash
sscli integrations skills add --global   # Claude Code + Codex
sscli integrations gemini add --global   # Gemini CLI
```

**Done.** Your agents now know how to browse schemas, run safe queries, and export results.

### What changes?

| Before                                | After                                                   |
| ------------------------------------- | ------------------------------------------------------- |
| You paste schema context into prompts | Agent discovers schema on demand                        |
| Agent guesses at SQL Server commands  | Agent knows `sscli tables`, `sscli describe`, etc.      |
| Risk of accidental writes             | Read-only by default, explicit `--allow-write` override |
| Verbose output bloats context         | Token-efficient markdown output                         |

## Manual Usage

For humans who want to use sscli directly.

### 1-minute setup (first run)

```bash
# Create a starter config in ./.sql-server/config.yaml (safe defaults)
sscli init

# Set the password env var referenced by passwordEnv in your config
export SQL_PASSWORD='...'

# Sanity-check connectivity + server metadata
sscli status

# See the effective settings + which config file was used
sscli config
```

### Common commands

```bash
sscli status                              # Check connectivity
sscli tables                              # List tables
sscli tables --like "%User%" --describe   # Describe all User-related tables
sscli describe Users                      # DDL, columns, indexes, triggers
sscli describe T_Users_Trig               # Trigger definition (auto-detected)
sscli sql "SELECT TOP 5 * FROM Users"
```

## Why sscli?

|                            |                                                     |
| -------------------------- | --------------------------------------------------- |
| **Token-efficient**        | Markdown output by default keeps agent context lean |
| **Read-only safe**         | Blocks INSERT, UPDATE, DELETE — no accidents        |
| **Single binary**          | Fast startup, no runtime dependencies               |
| **CLI over MCP**           | No "tool bloat" from long-running servers           |
| **Progressive disclosure** | Core commands visible, advanced hidden until needed |

For interactive SQL, keep using `sqlcmd`. For agent workflows, scripts, and CI — sscli is built for that.

## Installation

### From source (recommended)

```bash
cargo install sscli
```

### Development build

```bash
git clone https://github.com/jwcraig/sql-server-cli sscli
cd sscli
cargo build --release
./target/release/sscli --help
```

### Updating

```bash
cargo install sscli --force
```

> **Coming soon:** Homebrew formula and prebuilt binaries for Linux, macOS, and Windows.

## Agent Integration

### Supported agents

| Agent        | Command                                                  | What it installs                  |
| ------------ | -------------------------------------------------------- | --------------------------------- |
| Claude Code  | `sscli integrations skills add --global`                 | `~/.claude/skills/sscli/SKILL.md` |
| Codex        | (same command)                                           | `~/.codex/skills/sscli/SKILL.md`  |
| Gemini CLI   | `sscli integrations gemini add --global`                 | `~/.gemini/extensions/sscli/`     |
| Other agents | Via [OpenSkills](https://github.com/numman-ali/openskills) | Bridge to installed skills        |

### Per-project vs global

| Flag       | Installs to         | Use case                  |
| ---------- | ------------------- | ------------------------- |
| `--global` | `~/.claude/skills/` | Available in all projects |
| (none)     | `./.claude/skills/` | Project-specific override |

### What the skill teaches agents

The installed skill file tells agents:

- When to use sscli (database inspection, schema discovery, safe queries)
- Available commands and their purpose
- Output preferences (markdown for context efficiency, `--json` for structured data)
- Safety model (read-only default, `--allow-write` for mutations)

## Configuration

sscli supports three ways to configure a connection (highest priority wins):

```bash
# 1) CLI flags (one-off / scripts)
sscli status --server localhost --database master --user sa --password '...'

# 2) Environment variables (CI-friendly)
export SQL_SERVER=localhost SQL_DATABASE=master SQL_USER=sa SQL_PASSWORD='...'
sscli status

# 3) Config file (recommended for repeated use)
sscli init && export SQL_PASSWORD='...' && sscli status
```

### Creating a config file

Generate a commented template (writes `./.sql-server/config.yaml` by default):

```bash
sscli init
```

Or copy the example file in this repo:

```bash
mkdir -p .sql-server
cp config.example.yaml .sql-server/config.yaml
```

### Config discovery (where sscli looks)

1. `--config <PATH>`
2. `SQL_SERVER_CONFIG` / `SQLSERVER_CONFIG`
3. Walk up from CWD looking for `.sql-server/config.{yaml,yml,json}` or `.sqlserver/config.{yaml,yml,json}`
4. Global config: `$XDG_CONFIG_HOME/sql-server/config.{yaml,yml,json}` (platform-dependent)
5. Environment variables
6. Hardcoded defaults

Run `sscli config` to confirm which config file is being used and what values are in effect.

### Example `config.yaml`

```yaml
defaultProfile: default
profiles:
  default:
    server: localhost
    port: 1433
    database: master
    user: sa
    passwordEnv: SQL_PASSWORD
    encrypt: true
    trustCert: true
```

For a fully commented example (including `settings.output.*`, `timeout`, and `defaultSchemas`), see `config.example.yaml`.

### Environment variables

Environment variables override values from the config file.

**`.env` file support:** sscli automatically loads a `.env` file from the current directory if present. This is useful for local development without polluting your shell environment.

| Purpose | Environment variables (first match wins) |
| --- | --- |
| Config path | `SQL_SERVER_CONFIG`, `SQLSERVER_CONFIG` |
| Profile | `SQL_SERVER_PROFILE`, `SQLSERVER_PROFILE` |
| Connection URL | `DATABASE_URL`, `DB_URL`, `SQLSERVER_URL` |
| Server | `SQL_SERVER`, `SQLSERVER_HOST`, `DB_HOST` |
| Port | `SQL_PORT`, `SQLSERVER_PORT`, `DB_PORT` |
| Database | `SQL_DATABASE`, `SQLSERVER_DB`, `DATABASE`, `DB_NAME` |
| User | `SQL_USER`, `SQLSERVER_USER`, `DB_USER` |
| Password | `SQL_PASSWORD`, `SQLSERVER_PASSWORD`, `DB_PASSWORD` |
| Encrypt | `SQL_ENCRYPT` |
| Trust server certificate | `SQL_TRUST_SERVER_CERTIFICATE` |
| Connect timeout (ms) | `SQL_CONNECT_TIMEOUT`, `DB_CONNECT_TIMEOUT` |

## Commands

**Core** (shown in `--help`):

| Command      | Purpose                                              |
| ------------ | ---------------------------------------------------- |
| `status`     | Connectivity check                                   |
| `databases`  | List databases                                       |
| `tables`     | Browse tables and views (`--describe` for batch DDL) |
| `describe`   | Any object: table, view, trigger, proc, function     |
| `sql`        | Execute read-only SQL                                |
| `table-data` | Sample rows from a table                             |
| `columns`    | Find columns across tables                           |

**Advanced** (shown in `help --all`):

| Command        | Purpose                               |
| -------------- | ------------------------------------- |
| `indexes`      | Index details with usage stats        |
| `foreign-keys` | Table relationships                   |
| `stored-procs` | List and execute read-only procedures |
| `sessions`     | Active database sessions              |
| `query-stats`  | Top cached queries by resource usage  |
| `backups`      | Recent backup history                 |
| `integrations` | Install agent skills/extensions       |

## Output Formats

| Context         | Default                   |
| --------------- | ------------------------- |
| Terminal (TTY)  | Pretty tables             |
| Piped / non-TTY | Markdown tables           |
| `--json` flag   | Stable JSON (v1 contract) |
| `--csv <file>`  | CSV export                |

JSON output emits exactly one object to stdout. Errors go to stderr.

## Safety

`sscli sql` enforces read-only mode by default:

- **Allowed:** SELECT, WITH (CTEs), whitelisted stored procedures
- **Blocked:** INSERT, UPDATE, DELETE, DROP, ALTER, TRUNCATE, MERGE, etc.

Override with `--allow-write` when you intentionally need mutations.

## JSON Contract (v1)

Each command returns a stable top-level object:

| Command      | Shape                                                                                    |
| ------------ | ---------------------------------------------------------------------------------------- |
| `status`     | `{ status, latencyMs, serverName, serverVersion, currentDatabase, timestamp, warnings }` |
| `databases`  | `{ total, count, offset, limit, hasMore, nextOffset, databases: [...] }`                 |
| `tables`     | `{ total, count, offset, limit, hasMore, nextOffset, tables: [...] }`                    |
| `describe`   | `{ object: {schema, name, type}, columns, ddl?, indexes?, triggers?, foreignKeys?, constraints? }` |
| `table-data` | `{ table, columns, rows, total, offset, limit, hasMore, nextOffset }`                    |
| `sql`        | `{ success, batches, resultSets, csvPaths? }`                                            |

Errors (stderr):

```json
{ "error": { "message": "...", "kind": "Config|Connection|Query|Internal" } }
```

## Testing

```bash
cargo test
```

DB-backed integration tests (opt-in):

```bash
SSCLI_INTEGRATION_TESTS=1 SQL_SERVER_CONFIG=/path/to/config.yaml \
SQL_PASSWORD=... cargo test
```
