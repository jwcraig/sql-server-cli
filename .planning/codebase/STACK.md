# Technology Stack

**Analysis Date:** 2026-01-11

## Languages

**Primary:**
- Rust 1.85+ (2024 edition) - All application code

**Secondary:**
- Shell scripts - Installation (`install.sh`)
- YAML - Configuration files

## Runtime

**Environment:**
- Rust binary compiled to native platform target
- Single executable, no runtime dependencies
- Async runtime: Tokio (full features)

**Package Manager:**
- Cargo (Rust's built-in package manager)
- Lockfile: `Cargo.lock` present

## Frameworks

**Core:**
- Clap 4.4 - CLI argument parsing with derive macros (`src/cli/args.rs`)
- Tokio 1.35 - Async runtime for database connections (`src/db/`)

**Testing:**
- cargo test - Built-in test runner
- assert_cmd 2.0 - CLI integration testing (`tests/`)
- predicates 3.0 - Test assertion library
- tempfile 3.10 - Temporary file handling in tests

**Build/Dev:**
- rustfmt - Code formatting
- clippy - Linting
- Pre-push hook in `.githooks/` - Runs fmt, clippy, test

## Key Dependencies

**Critical:**
- tiberius 0.12 - SQL Server database driver (`src/db/client.rs`)
- anyhow 1.0 - Error handling throughout codebase
- serde 1.0 + serde_yaml/serde_json - Configuration and output serialization

**Infrastructure:**
- comfy-table 7.1 - Terminal table formatting (`src/output/table.rs`)
- owo-colors 4.0 - Terminal coloring (`src/main.rs`)
- dotenvy 0.15.7 - Environment file loading (`src/config/env.rs`)
- reqwest 0.12 - HTTP client for update checks (`src/update.rs`)
- similar 2.4 - Text diffing for compare command (`src/commands/compare.rs`)
- chrono 0.4 - Date/time handling

## Configuration

**Environment:**
- `.env` files auto-loaded via dotenvy
- Extensive env var support: `SQL_SERVER`, `SQL_PASSWORD`, etc.
- sqlcmd compatibility: `SQLCMDSERVER`, `SQLCMDUSER`, etc.
- Custom env file via `--env-file` flag

**Build:**
- `Cargo.toml` - Dependencies and project metadata
- Feature flags: `tds73` (enabled by default for TDS 7.3 protocol)
- Binary install via cargo-binstall supported

## Platform Requirements

**Development:**
- Any platform with Rust 1.85+ toolchain
- No external dependencies (pure Rust)

**Production:**
- Distributed as single native binary
- Platforms: macOS (arm64/x86_64), Linux (x86_64), Windows (x86_64)
- Distribution: Homebrew, Scoop, cargo install, GitHub releases

---

*Stack analysis: 2026-01-11*
*Update after major dependency changes*
