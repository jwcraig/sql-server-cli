# Coding Conventions

**Analysis Date:** 2026-01-11

## Naming Patterns

**Files:**
- snake_case.rs for all modules (`foreign_keys.rs`, `query_stats.rs`)
- `mod.rs` for module definition files
- Test files: `*_test.rs` in tests/ directory

**Functions:**
- snake_case for all functions
- `run(args, cmd_args) -> Result<()>` pattern for command handlers
- `parse()` for parsing functions
- `emit_*` for output functions (`emit_json`, `emit_json_value`)

**Variables:**
- snake_case for variables
- SCREAMING_SNAKE_CASE for constants
- `args` for CLI arguments, `cmd` for command-specific args

**Types:**
- PascalCase for structs, enums, traits
- `*Args` suffix for command argument structs (`SqlArgs`, `TablesArgs`)
- `*Kind` suffix for enum discriminants (`CommandKind`, `ErrorKind`)
- No `I` prefix for traits

## Code Style

**Formatting:**
- rustfmt for formatting (default configuration)
- 4 space indentation
- 100 character line limit (rustfmt default)
- No trailing whitespace

**Linting:**
- clippy with `-D warnings` in CI
- `#![allow(clippy::uninlined_format_args)]` project-wide exception
- Pre-push hook runs `cargo fmt --check` and `cargo clippy -D warnings`

**Imports:**
- Standard library first, external crates second, local modules third
- Grouped by category with blank lines between
- Explicit imports preferred, wildcards only for tests (`use super::*`)

## Import Organization

**Order:**
1. Standard library (`std::*`)
2. External crates (`clap`, `tokio`, `anyhow`)
3. Local crate modules (`crate::cli`, `crate::db`)

**Grouping:**
- Blank line between groups
- Multiple imports from same module on one line when short
- Line breaks for long import lists

**Path Aliases:**
- `crate::` for absolute paths within the project
- No custom path aliases defined

## Error Handling

**Patterns:**
- Return `anyhow::Result<T>` from functions
- Custom `AppError` with `ErrorKind` for classified errors (`src/error.rs`)
- Propagate errors with `?` operator
- Context added with `.context()` or `.with_context()`

**Error Types:**
- `ErrorKind::Config` - Configuration issues
- `ErrorKind::Connection` - Database connectivity
- `ErrorKind::Query` - SQL execution errors
- `ErrorKind::Internal` - Unexpected/unknown errors

**Error Output:**
- Errors to stderr (never stdout)
- JSON format when `--json` flag present
- Colored output when TTY detected

## Logging

**Framework:**
- tracing + tracing-subscriber
- Output to stderr only

**Patterns:**
- `tracing::info!`, `tracing::debug!`, `tracing::warn!`, `tracing::error!`
- Verbosity controlled by `-v` flags (default: warn)
- Structured fields: `tracing::info!(key = value, "message")`

**When:**
- Debug: Connection details, query text
- Info: Major operations starting/completing
- Warn: Recoverable issues, deprecations
- Error: Logged before returning error

## Comments

**When to Comment:**
- Complex SQL query builders
- Non-obvious business logic
- Safety-critical code (read-only enforcement)
- TODO for incomplete work

**Doc Comments:**
- Required for public functions per AGENTS.md
- `///` for doc comments
- Include examples for complex functions

**TODO Comments:**
- Format: `// TODO: description`
- Found in: `src/commands/compare.rs` for manual drop statements

## Function Design

**Size:**
- Keep functions focused, single responsibility
- Extract helpers for complex logic
- Commands often 100-200 lines (acceptable for CLI handlers)

**Parameters:**
- Pass `&CliArgs` for global args
- Pass command-specific `&XxxArgs` for subcommand params
- Prefer borrowing over ownership

**Return Values:**
- `anyhow::Result<()>` for command handlers
- `anyhow::Result<T>` for functions returning values
- Early return for error cases

## Module Design

**Exports:**
- `pub use` in mod.rs for public API
- Keep internals private by default
- One command per file in commands/

**Barrel Files:**
- `mod.rs` re-exports public items
- `src/lib.rs` exports top-level modules
- Commands accessed via `commands::dispatch()`

**Dependencies:**
- Commands depend on config, db, output, safety
- Lower layers don't depend on higher layers
- Avoid circular dependencies

---

*Convention analysis: 2026-01-11*
*Update when patterns change*
