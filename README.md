# sscli

SQL Server CLI for AI coding agents.

One install. Your agents automatically know how to inspect SQL Server databases, run safe queries, and export results.

## Why sscli?

|                            |                                                                                |
| -------------------------- | ------------------------------------------------------------------------------ |
| **Token-efficient**        | Markdown output by default keeps agent context lean                            |
| **Read-only default**      | Blocks INSERT, UPDATE, DELETE unless overridden (--allow-write) — no accidents |
| **Single binary**          | Fast startup, no runtime dependencies                                          |
| **CLI over MCP**           | No "tool bloat" from verbose tool descriptions for tools that are rarely used  |
| **Progressive disclosure** | Core commands visible, advanced disclosed when needed                          |

## Why not sqlcmd

`sqlcmd` is a great general-purpose SQL Server client, especially for interactive sessions and ad-hoc work.

For tool-calling agents, `sqlcmd` tends to be a poor fit because it's optimized for humans, not for
structured, repeatable automation:

- **Output is hard to consume**: `sqlcmd` output is human-oriented text; agents usually want stable
  markdown tables or a single JSON object they can reliably parse.
- **Schema discovery is manual**: you end up writing catalog queries (`sys.tables`, `INFORMATION_SCHEMA`, etc.)
  instead of calling purpose-built primitives like `sscli tables`, `sscli describe`, and `sscli columns`.
- **No safety guardrails**: `sqlcmd` will happily run destructive statements if an agent makes a mistake.
  `sscli sql` blocks writes by default and requires `--allow-write` for mutations.
- **More setup friction**: `sqlcmd` is typically installed via Microsoft tooling and may require ODBC drivers
  depending on platform/CI image; sscli is a single binary with config + env var discovery built in.
- **No agent integration**: sscli can install a reusable skill/extension so agents "know the tool" without you
  pasting usage docs into every prompt.

Keep `sqlcmd` for interactive SQL. Reach for sscli when you want safe-by-default queries, fast schema
inspection, and output formats that are easy for agents to use.

## Quick Start (Agent Users)

**1. Install sscli**

```bash
# macOS/Linux
brew install jwcraig/tap/sscli

# Windows (PowerShell)
scoop bucket add jwcraig https://github.com/jwcraig/scoop-bucket
scoop install sscli

# or with cargo (any platform)
cargo install sscli
```

**2. Teach your agents**

```bash
sscli integrations skills add --global   # Claude Code + Codex
sscli integrations gemini add --global   # Gemini CLI
```

**Done.** Your agents now know how to browse schemas, run safe queries, and export results.

### What changes?

| Before                                | After                                                        |
| ------------------------------------- | ------------------------------------------------------------ |
| You paste schema context into prompts | Agent discovers schema on demand                             |
| Agent guesses at SQL Server commands  | Agent knows `sscli tables`, `sscli describe <Object>`, etc.  |
| Risk of accidental writes             | Read-only by default, explicit `--allow-write` override      |
| Verbose output bloats context         | Token-efficient markdown output by default, --json if needed |

## Manual Usage

For humans who want to use sscli directly.

### 1-minute setup (first run)

```bash
# Create a starter config in ./.sql-server/config.yaml (safe defaults)
sscli init

# Set the password env var referenced by passwordEnv in your config. sscli also reads
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
sscli sql --file [path/to/file]           # Run long queries, execute bulk statements
sscli update                              # Check for new releases (alias: sscli upgrade)
```

## Installation

### Homebrew (macOS/Linux)

```bash
brew install jwcraig/tap/sscli
```

### Scoop (Windows)

```powershell
scoop bucket add jwcraig https://github.com/jwcraig/scoop-bucket
scoop install sscli
```

### Quick install script

```bash
curl -sSL https://raw.githubusercontent.com/jwcraig/sql-server-cli/main/install.sh | sh
```

### Cargo binstall (fast, no compilation)

```bash
cargo binstall sscli
```

### From source

```bash
cargo install sscli
```

### Prebuilt binaries

Download from [GitHub Releases](https://github.com/jwcraig/sql-server-cli/releases).

### Development build

```bash
git clone https://github.com/jwcraig/sql-server-cli sscli
cd sscli
cargo build --release
./target/release/sscli --help
```

### Updating

```bash
# Check if you're up to date (alias: `sscli upgrade`)
sscli update

# Homebrew
brew upgrade sscli

# Cargo
cargo install sscli --force
```

### Automatic update notifications (optional)

By default, sscli does **not** check for updates automatically.

To enable lightweight update notifications (stderr, TTY-only, cached), create:

- `~/.config/sscli/settings.json` (Linux/XDG default)
- macOS often uses `~/Library/Application Support/sscli/settings.json` by default

Example `settings.json`:

```json
{ "autoUpdate": true }
```

## Agent Integration

### Supported agents

| Agent                 | Command                                                    | What it installs                  |
| --------------------- | ---------------------------------------------------------- | --------------------------------- |
| Claude Code           | `sscli integrations skills add --global`                   | `~/.claude/skills/sscli/SKILL.md` |
| Codex                 | (same command)                                             | `~/.codex/skills/sscli/SKILL.md`  |
| Gemini CLI            | `sscli integrations gemini add --global`                   | `~/.gemini/extensions/sscli/`     |
| Other agent harnesses | Via [OpenSkills](https://github.com/numman-ali/openskills) | Bridge to installed skills        |

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

sscli supports three ways to configure a connection (highest priority wins; env vars are skipped if you pass `--profile`):

```bash
# 1) CLI flags (one-off / scripts)
sscli status --server localhost --database master --user sa --password '...' # alias: --host

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
5. Environment variables (only applied when no `--profile` is provided)
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

Environment variables override values from the config file when no explicit `--profile` was passed. If you pass `--profile <name>`, the profile values win over env vars (flags still win over both).

**`.env` file support:** sscli automatically loads a `.env` file from the current directory if present, reading any of the supported variables listed below. Use `--env-file` to load a different file (e.g., `--env-file .env.dev`). This is useful for local development without polluting your shell environment.

| Purpose                  | Environment variables (first match wins)                                                                  |
| ------------------------ | --------------------------------------------------------------------------------------------------------- |
| Config path              | `SQL_SERVER_CONFIG`, `SQLSERVER_CONFIG`                                                                   |
| Profile                  | `SQL_SERVER_PROFILE`, `SQLSERVER_PROFILE`                                                                 |
| Connection URL           | `DATABASE_URL`, `DB_URL`, `SQLSERVER_URL`                                                                 |
| Server                   | `SQL_SERVER`, `SQLSERVER_HOST`, `DB_HOST`, `MSSQL_HOST`                                                   |
| Port                     | `SQL_PORT`, `SQLSERVER_PORT`, `DB_PORT`, `MSSQL_PORT`                                                     |
| Database                 | `SQL_DATABASE`, `SQLSERVER_DB`, `DATABASE`, `DB_NAME`, `MSSQL_DATABASE`                                   |
| User                     | `SQL_USER`, `SQLSERVER_USER`, `DB_USER`, `MSSQL_USER`                                                     |
| Password                 | `SQL_PASSWORD`, `SA_PASSWORD`, `MSSQL_SA_PASSWORD`, `SQLSERVER_PASSWORD`, `DB_PASSWORD`, `MSSQL_PASSWORD` |
| Encrypt                  | `SQL_ENCRYPT`                                                                                             |
| Trust server certificate | `SQL_TRUST_SERVER_CERTIFICATE`                                                                            |
| Connect timeout (ms)     | `SQL_CONNECT_TIMEOUT`, `DB_CONNECT_TIMEOUT`                                                               |

**sqlcmd compatibility:** The following `sqlcmd` environment variables are also supported:

| Purpose  | Variable         |
| -------- | ---------------- |
| Server   | `SQLCMDSERVER`   |
| User     | `SQLCMDUSER`     |
| Password | `SQLCMDPASSWORD` |
| Database | `SQLCMDDBNAME`   |

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

| Command        | Purpose                                        |
| -------------- | ---------------------------------------------- |
| `indexes`      | Index details with usage stats                 |
| `foreign-keys` | Table relationships                            |
| `stored-procs` | List and execute read-only procedures          |
| `sessions`     | Active database sessions                       |
| `query-stats`  | Top cached queries by resource usage           |
| `backups`      | Recent backup history                          |
| `compare`      | Schema drift detection between two connections |
| `integrations` | Install agent skills/extensions                |

Note: `sscli sessions` filters by client host name using `--client-host`. `--host` is reserved as an alias for `--server`.

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

| Command      | Shape                                                                                              |
| ------------ | -------------------------------------------------------------------------------------------------- |
| `status`     | `{ status, latencyMs, serverName, serverVersion, currentDatabase, timestamp, warnings }`           |
| `databases`  | `{ total, count, offset, limit, hasMore, nextOffset, databases: [...] }`                           |
| `tables`     | `{ total, count, offset, limit, hasMore, nextOffset, tables: [...] }`                              |
| `describe`   | `{ object: {schema, name, type}, columns, ddl?, indexes?, triggers?, foreignKeys?, constraints? }` |
| `table-data` | `{ table, columns, rows, total, offset, limit, hasMore, nextOffset }`                              |
| `sql`        | `{ success, batches, resultSets, csvPaths? }`                                                      |
| `compare`    | `{ modules, indexes, constraints, tables }` when `--summary`; `{ source, target }` snapshots with full metadata when `--json` without `--summary` |

Errors (stderr):

```json
{ "error": { "message": "...", "kind": "Config|Connection|Query|Internal" } }
```

## compare (schema drift)

Detects drift between two profiles or explicit connection strings.

Synopsis:

```
sscli compare --target <profile> [--source <profile>] [--schema web --schema dbo] \
  [--summary|--json] [--ignore-whitespace] [--strip-comments] \
  [--object dbo.ProcName] [--apply-script [path|-]] [--include-drops]
```

- `--target/--right` (required): profile to treat as the environment you want to align.
- `--source/--left`: reference profile (defaults to global `--profile` or config default).
- `--source-connection/--left-connection`, `--target-connection/--right-connection`: override profile with a connection string (URL or ADO-style `Server=...;Database=...`).
- `--schema/--schemas`: limit to specific schemas (repeatable or comma-separated).
- `--object`: emit unified diff for a single module (proc/view/function/trigger).
- `--ignore-whitespace`, `--strip-comments`: normalize noise before diffing definitions.
- `--summary`: compact drift counts; `--pretty` renders text; `--json` renders JSON.
- `--apply-script [path|-]`: generate SQL to align target to source; default path `db-apply-diff-YYYYMMDD-HHMMSS.sql` in cwd; use `-` for stdout.
- `--include-drops`: include DROP statements (disabled by default).
- Profiles are the names in your `.sql-server/config.*` (e.g., `dev`, `stage`, `prod`). `--source/--target` expect those names.

Examples:

```bash
# Summary with profile names
sscli compare --target prod --summary

# Object diff ignoring whitespace
sscli compare --target prod --object dbo.MyProc --ignore-whitespace

# Apply script to stdout
sscli compare --target prod --apply-script - --include-drops

# Using explicit connection strings instead of profiles
sscli compare --source-connection "Server=dev,1433;Database=app;User ID=sa;Password=..." \
              --target-connection "sqlserver://user:pass@prod:1433/app" \
              --summary
```

Exit codes: `0` = no drift, `3` = drift detected (summary/object/apply modes), `1` = error.

## Testing

```bash
cargo test
```

DB-backed integration tests (opt-in):

```bash
SSCLI_INTEGRATION_TESTS=1 SQL_SERVER_CONFIG=/path/to/config.yaml \
SQL_PASSWORD=... cargo test
```
