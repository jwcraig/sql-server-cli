# Architecture

**Analysis Date:** 2026-01-11

## Pattern Overview

**Overall:** Monolithic CLI Application

**Key Characteristics:**
- Single executable with subcommands
- Command dispatcher pattern
- Read-only by default (safety guard)
- Multi-format output (table, markdown, JSON, CSV)
- Layered architecture: CLI → Commands → Services → Database

## Layers

**CLI Layer:**
- Purpose: Parse user input, build arguments structure
- Contains: Argument definitions, parsing logic
- Location: `src/cli/args.rs`, `src/cli/mod.rs`
- Depends on: Clap framework
- Used by: Entry point (`src/main.rs`)

**Command Layer:**
- Purpose: Handle subcommands, orchestrate business logic
- Contains: One module per command (status, tables, describe, sql, etc.)
- Location: `src/commands/*.rs`
- Depends on: Config, DB, Output, Safety layers
- Used by: Dispatcher (`src/commands/mod.rs`)

**Config Layer:**
- Purpose: Load and merge configuration from multiple sources
- Contains: Config file loading, env var parsing, profile management
- Location: `src/config/loader.rs`, `src/config/env.rs`, `src/config/schema.rs`
- Depends on: File system, environment
- Used by: Commands for connection parameters

**Database Layer:**
- Purpose: SQL Server connectivity and query execution
- Contains: Connection management, query builders, result handling
- Location: `src/db/client.rs`, `src/db/executor.rs`, `src/db/queries.rs`
- Depends on: Tiberius driver, Tokio async runtime
- Used by: Commands

**Output Layer:**
- Purpose: Format and render results
- Contains: Table, JSON, CSV, Markdown formatters
- Location: `src/output/table.rs`, `src/output/json.rs`, `src/output/csv.rs`
- Depends on: comfy-table, serde_json
- Used by: Commands

**Safety Layer:**
- Purpose: Enforce read-only mode, block dangerous SQL
- Contains: SQL statement analysis, keyword detection
- Location: `src/safety/read_only.rs`, `src/safety/mod.rs`
- Depends on: Regex
- Used by: SQL command

## Data Flow

**CLI Command Execution:**

1. User runs: `sscli tables --like "%User%"`
2. `main()` calls `cli::parse()` to build `CliArgs` struct
3. Logging initialized based on verbosity level
4. `commands::dispatch(&args)` matches command kind
5. Command handler (e.g., `tables::run`) invoked
6. Config loaded: CLI flags → env vars → config file → defaults
7. Database connection established via `db::client`
8. Query executed, results fetched
9. Output formatted based on `--json`/`--markdown`/TTY detection
10. Results printed to stdout, errors to stderr
11. Update notice optionally shown (TTY only)
12. Process exits with status code

**State Management:**
- Stateless: Each invocation is independent
- Connection per-request: No persistent connection pool
- Config cached within single execution

## Key Abstractions

**CliArgs:**
- Purpose: Holds all parsed command-line arguments
- Location: `src/cli/args.rs`
- Pattern: Struct with nested command-specific args

**CommandKind:**
- Purpose: Enum discriminating all available subcommands
- Location: `src/cli/args.rs`
- Pattern: Tagged union with per-command arg structs

**Config:**
- Purpose: Merged configuration from all sources
- Location: `src/config/schema.rs`
- Pattern: Profile-based configuration with defaults

**OutputFlags:**
- Purpose: Control output format (JSON, markdown, pretty)
- Location: `src/cli/args.rs`
- Pattern: Flags struct passed through command chain

## Entry Points

**CLI Entry:**
- Location: `src/main.rs`
- Triggers: User runs `sscli <command>`
- Responsibilities: Parse args, init logging, dispatch, handle errors

**Library Entry:**
- Location: `src/lib.rs`
- Triggers: Integration tests
- Responsibilities: Export public modules

## Error Handling

**Strategy:** Result propagation with anyhow, classification at top level

**Patterns:**
- Commands return `anyhow::Result<()>`
- Custom `AppError` with `ErrorKind` enum (`src/error.rs`)
- Errors caught in `main()`, classified, formatted to stderr
- JSON error output when `--json` flag present

**Error Kinds:**
- `Config` - Configuration issues
- `Connection` - Database connectivity problems
- `Query` - SQL execution errors
- `Internal` - Unexpected errors

## Cross-Cutting Concerns

**Logging:**
- tracing + tracing-subscriber for structured logging
- Verbosity levels: warn (default), info (-v), debug (-vv), trace (-vvv)
- Output to stderr, never stdout

**Safety:**
- Read-only by default (`src/safety/read_only.rs`)
- SQL statement blocking: INSERT, UPDATE, DELETE, DROP, etc.
- Override via `--allow-write` flag

**Output Formatting:**
- TTY detection for pretty vs markdown output
- `--json` for machine-readable output
- `--csv` for file export
- `NO_COLOR` env var support

---

*Architecture analysis: 2026-01-11*
*Update when major patterns change*
