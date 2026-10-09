# sscli

SQL Server CLI for AI coding agents.

One install. Your agents automatically know how to inspect SQL Server databases,
run SQL, and export results.

## Command model direction

`sql` is the canonical raw-SQL surface in this project.

You can run raw SQL either with the explicit subcommand:

```bash
sscli sql "SELECT 1"
```

or with the top-level shorthand:

```bash
sscli "SELECT 1"
```

For configuration, prefer explicit `--env-file` over relying on ambient `.env`
files in the working directory.

## Why sscli?

|                            |                                                                                |
| -------------------------- | ------------------------------------------------------------------------------ |
| **Token-efficient**        | Markdown output by default keeps agent context lean                           |
| **SQL-first workflows**    | Canonical `sql` command for raw SQL, plus schema/discovery helpers            |
| **Single binary**          | Fast startup, no runtime dependencies                                         |
| **CLI over MCP**           | No "tool bloat" from verbose tool descriptions for tools that are rarely used |
| **Progressive disclosure** | Core commands visible, advanced disclosed when needed                         |

## Why not sqlcmd

`sqlcmd` is a great general-purpose SQL Server client, especially for interactive sessions and ad-hoc work.

For tool-calling agents, `sqlcmd` tends to be a poor fit because it's optimized for humans, not for
structured, repeatable automation:

- **Output is hard to consume**: `sqlcmd` output is human-oriented text; agents usually want stable
  markdown tables or a single JSON object they can reliably parse.
- **Schema discovery is manual**: you end up writing catalog queries (`sys.tables`, `INFORMATION_SCHEMA`, etc.)
  instead of calling purpose-built primitives like `sscli tables`, `sscli describe`, and `sscli columns`.
- **Structured SQL workflows**: `sscli` keeps raw SQL, schema discovery, and machine-readable output in
  one tool, so agents do not have to switch between `sqlcmd` for execution and a second tool for inspection.
- **More setup friction**: `sqlcmd` is typically installed via Microsoft tooling and may require ODBC drivers
  depending on platform/CI image; sscli is a single binary with config + env var discovery built in.
- **No agent integration**: sscli can install a reusable skill/extension so agents "know the tool" without you
  pasting usage docs into every prompt.

Keep `sqlcmd` for interactive SQL. Reach for `sscli` when you want raw SQL plus fast schema inspection
and output formats that are easy for agents to use.

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

**Done.** Your agents now know how to browse schemas, run SQL, and export results.

### What changes?

| Before                                | After                                                        |
| ------------------------------------- | ------------------------------------------------------------ |
| You paste schema context into prompts | Agent discovers schema on demand                             |
| Agent guesses at SQL Server commands  | Agent knows `sscli sql`, `sscli tables`, `sscli describe`    |
| Raw SQL needs a second tool           | Canonical `sql` command plus schema/discovery helpers        |
| Verbose output bloats context         | Token-efficient markdown output by default, `--json` if needed |

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
sscli table-data equipment                # Browse rows (schema auto-resolved; prompts on conflicts)
sscli sql "SELECT TOP 5 * FROM Users"
sscli "SELECT COUNT(*) FROM Users"        # Top-level shorthand for inline SQL
sscli sql --file [path/to/file]           # Run long queries, execute bulk statements
cat patch.sql | sscli sql --stdin         # Pipe a script on stdin
sscli --format tsv --no-headers "SELECT COUNT_BIG(*) FROM Users"
sscli --trust-cert true --quiet-tls-warning "SELECT 1"
sscli --password-env SQL_PASSWORD status  # Avoid putting passwords on the command line
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

The installer verifies the downloaded artifact against the release `checksums-sha256.txt`.

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

- When to use sscli (database inspection, schema discovery, raw SQL execution)
- Available commands and their purpose
- Output preferences (markdown for context efficiency, `--json` for structured data)
- `sql` as the main raw-SQL surface, with top-level shorthand for simple inline
  queries

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

A profile named by `--profile`, `SQL_SERVER_PROFILE` or `defaultProfile` must exist:
an unknown name is an error that lists the profiles the config defines, rather
than a silent fall back to `localhost`. When no config file, `--server` or
`SQL_SERVER`/`SSCLI_URL` names a server, commands warn on stderr that they are
using the built-in `localhost:1433/master` default.

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

**`.env` file support:** use `--env-file` to load environment variables from a
specific file, for example `--env-file .env.dev`. `sscli` does not implicitly
load `.env` from the current working directory.

| Purpose                  | Environment variables (first match wins)                                                                  |
| ------------------------ | --------------------------------------------------------------------------------------------------------- |
| Config path              | `SQL_SERVER_CONFIG`, `SQLSERVER_CONFIG`                                                                   |
| Profile                  | `SQL_SERVER_PROFILE`, `SQLSERVER_PROFILE`                                                                 |
| Connection URL           | `SSCLI_URL`, `DATABASE_URL`, `DB_URL`, `SQLSERVER_URL`                                                    |
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

Connection URL query options include `trustServerCertificate=true|false`,
`encrypt=true|false`, and `timeoutMs=<milliseconds>`.

## Commands

**Core** (shown in `--help`):

| Command      | Purpose                                              |
| ------------ | ---------------------------------------------------- |
| `status`     | Connectivity check                                   |
| `databases`  | List databases                                       |
| `tables`     | Browse tables and views (`--describe` for batch DDL) |
| `describe`   | Any object: table, view, trigger, proc, function     |
| `sql`        | Execute SQL                                          |
| `table-data` | Sample rows from a table                             |
| `columns`    | Find columns across tables/views/procs (first result set) |

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
| `compare-counts` | Row-count validation across databases        |
| `health`       | Constraint, trigger, and identity summaries    |
| `admin`        | SQL Server maintenance SQL, backup, and DBCC (see [admin](#admin-maintenance-sql)) |
| `integrations` | Install agent skills/extensions                |

Note: `sscli sessions` filters by client host name using `--client-host`. `--host` is reserved as an alias for `--server`.

## Output Formats

| Context         | Default                   |
| --------------- | ------------------------- |
| Terminal (TTY)  | Pretty tables             |
| Piped / non-TTY | Markdown tables           |
| `--json` flag   | Stable JSON (v1 contract) |
| `--csv <file>`  | CSV export                |
| `--format tsv`  | Raw TSV rows              |
| `--format csv`  | Raw CSV rows              |
| `--format jsonl` | One JSON object per row  |

JSON output emits exactly one object to stdout. Errors go to stderr.
`sql` and `admin sql` print an `Execution` summary table when a script returns no
result sets, so a statement that succeeded silently is still visible. See
[admin](#admin-maintenance-sql).
Raw row formats keep stdout data-only; target banners, progress, warnings, and
errors go to stderr. Use `--no-headers` for headerless TSV/CSV evidence files
and `--output <path|->` to write raw row output.
Use `--quiet-tls-warning` when `--trust-cert true` is intentional and verbose
logging would otherwise print Tiberius' certificate-validation warning.

## Safety

- keep `sql` as the canonical raw-SQL command
- support full SQL execution, including file/stdin-driven scripts
- keep the existing profile/config connection model
- prefer explicit `--env-file` over ambient cwd configuration
- prefer lightweight safety rails and explicit target visibility over hidden
  parser restrictions

If you need a locked-down distribution, maintain a custom build or wrapper that
strips write capability. The shipped tool is intended to be full-capability.

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
| `compare-counts` | `{ sourceDb, targetDb, counts }`                                                               |
| `health`     | `{ kind, checks }`                                                                                 |
| `admin sql`  | `{ status, error, partialEffectPossible, elapsedMs, batchCount, batchesSucceeded, batches, resultSetCount, rowsReturned, resultSets, verify }` |

`config --json` never emits plaintext passwords by default. It reports
`passwordSet`, `passwordSource`, and `passwordEnv` metadata instead.

Errors (stderr):

```json
{ "error": { "message": "...", "kind": "Config|Connection|Query|Internal" } }
```

## Scripts and `GO` batches

`sql --file`, `sql --stdin` and `admin sql` run scripts the way `sqlcmd -b` does:

- A line holding only `GO` (any case, optionally `GO n` to repeat the batch, or a
  trailing `--` comment) ends a batch. `GO` inside a string, bracketed
  identifier or comment is left alone.
- Each batch is sent byte for byte, line endings and blank lines included, so
  a `CREATE TRIGGER` or `CREATE PROCEDURE` stores the same definition `sqlcmd`
  would. A leading UTF-8 byte order mark is ignored.
- Every batch runs in order on one session. A transaction opened in one batch
  can be committed in a later one, and `SET` options, `USE` and `#temp` tables
  carry over, as they do in `sqlcmd`.
- The first failing batch stops the script with a non-zero exit, and the error
  names the batch (`Batch 3 of 5 failed: ...`). The session then closes, so the
  server rolls back a transaction the script left open. `sql --continue-on-error`
  keeps going instead.

A batch that uses `--param` values runs through `sp_executesql`, so a
transaction cannot stay open across that batch. Batches that use no parameter
are unaffected.

`sql` runs any statement, writes included. The `sql` banner on stderr shows the
target, profile and config file (`Target: host:port/db (profile dev,
/path/.sql-server/config.yaml)`), so check it before you run a write script.

## admin (maintenance SQL)

`admin sql` runs full-capability maintenance SQL. `GO` splits the script into
batches, which run in order on one connection and stop at the first failure.

### Execution summary

Maintenance statements usually return no rows, so `admin sql` and `sql` always
print an `Execution` summary. A successful `DROP DATABASE` is therefore visibly
different from a command that never ran:

```
$ sscli admin sql "DROP DATABASE IF EXISTS [WDM_VERIFY];"
Target: localhost:1433/master
| Execution    | Value            |
|--------------|------------------|
| Status       | ok               |
| Batches      | 1 of 1 succeeded |
| ResultSets   | 0                |
| RowsReturned | 0                |
| ElapsedMs    | 11               |
```

Multi-batch scripts also print a per-batch table, so a run that stops halfway
shows which batches already applied. That matters when an earlier batch leaves
state behind, such as `ALTER DATABASE ... SET SINGLE_USER` followed by a
`DROP DATABASE` that fails. Failures exit non-zero with the server message on
stderr.

A batch is not all-or-nothing. It holds every statement up to the next `GO`, and
semicolon-separated statements before the failing one have already committed. So
`Batches 0 of 1 succeeded` means the batch did not complete, not that nothing
happened, and the summary says so:

```
| Status       | failed                                                  |
| Batches      | 0 of 1 succeeded                                        |
| FailedBatch  | 1 (statements before the failure may have taken effect) |
```

After any failed batch, read the current state back rather than assuming a
rollback. In JSON, `partialEffectPossible` carries the same warning.

### Verified maintenance

`Status ok` proves the statement executed, not that it changed anything:
`DROP DATABASE IF EXISTS` reports success even as a no-op. Use `--verify` to run
a read-back on the same connection and `--expect-rows` to assert its row count.
A mismatch prints the summary, then exits non-zero.

```bash
# Drop a database and prove it is gone
sscli admin sql "DROP DATABASE [WDM_VERIFY];" \
  --verify "SELECT name FROM sys.databases WHERE name = 'WDM_VERIFY'" \
  --expect-rows 0

# Prove a database was created
sscli admin sql "CREATE DATABASE [WDM_VERIFY];" \
  --verify "SELECT name FROM sys.databases WHERE name = 'WDM_VERIFY'" \
  --expect-rows 1
```

`--verify` without `--expect-rows` reports the read-back rows without asserting
them. The read-back is skipped when a batch fails, because it would describe a
half-applied script.

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

### Pre-push hook (local)

This repo ships a local pre-push hook that runs `cargo fmt --check`, `cargo clippy -D warnings`, and `cargo test`. It’s already enabled via `core.hooksPath=.githooks`. If you need to bypass temporarily:

```bash
HUSKY=0 git push   # or
SKIP=1 git push    # (any env; git ignores but hook can read if we add later)
```

DB-backed integration tests (opt-in):

```bash
SSCLI_INTEGRATION_TESTS=1 SQL_SERVER_CONFIG=/path/to/config.yaml \
SQL_PASSWORD=... cargo test
```
