# External Integrations

**Analysis Date:** 2026-01-11

## APIs & External Services

**GitHub Releases API:**
- Purpose: Check for updates, fetch latest version
- Location: `src/update.rs`, `src/commands/update.rs`
- SDK/Client: reqwest (blocking client)
- Auth: None required (public API)
- Endpoint: `https://api.github.com/repos/jwcraig/sql-server-cli/releases/latest`
- Rate limits: GitHub API limits apply

## Data Storage

**Databases:**
- SQL Server - Target database for inspection (user-provided)
  - Connection: Via tiberius driver (`src/db/client.rs`)
  - Auth: SQL auth or Windows auth (via config)
  - No ORM - raw SQL queries in `src/db/queries.rs`

**File Storage:**
- Local filesystem only
- Config files: `.sql-server/config.yaml` or global config
- No cloud storage integration

**Caching:**
- Update check results cached in settings file
- Location: `~/.config/sscli/settings.json` (platform-dependent)
- TTL: 24 hours for update check cache

## Authentication & Identity

**Database Auth:**
- SQL Server authentication (username/password)
- Password stored in env var (e.g., `SQL_PASSWORD`)
- `passwordEnv` config option references env var name
- Multiple env var aliases supported (sqlcmd compatibility)

**No OAuth/SSO:**
- No external identity providers
- No user accounts or sessions

## Monitoring & Observability

**Error Tracking:**
- None (no Sentry, Datadog, etc.)
- Errors printed to stderr with classification

**Analytics:**
- None

**Logs:**
- tracing to stderr only
- No external log aggregation
- Verbosity controlled by `-v` flags

## CI/CD & Deployment

**Hosting:**
- GitHub repository: `jwcraig/sql-server-cli`
- GitHub Releases for binary distribution
- crates.io for cargo install

**CI Pipeline:**
- GitHub Actions (assumed from standard Rust project)
- Pre-push hook: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`
- Build targets: macOS (arm64, x86_64), Linux (x86_64), Windows (x86_64)

**Distribution:**
- Homebrew: `jwcraig/tap/sscli`
- Scoop: `jwcraig/scoop-bucket`
- cargo-binstall: Pre-built binaries
- cargo install: From source

## Environment Configuration

**Development:**
- Required: Rust 1.85+ toolchain
- Optional: SQL Server for integration tests
- Secrets: `SQL_PASSWORD` env var for testing
- Local env file: `.env` (gitignored)

**Production/User:**
- Config discovery: CLI flags → env vars → config file → defaults
- Config locations: `.sql-server/config.yaml` (project), `~/.config/sql-server/config.yaml` (global)
- Env vars documented in README.md

## Agent Integrations

**Claude Code / Codex:**
- Skill file: `~/.claude/skills/sscli/SKILL.md` (global) or `.claude/skills/sscli/` (project)
- Install: `sscli integrations skills add --global`
- Content: Command reference, safety model, output preferences

**Gemini CLI:**
- Extension: `~/.gemini/extensions/sscli/`
- Install: `sscli integrations gemini add --global`
- Content: Extension manifest from `assets/GEMINI.md`

**OpenSkills:**
- Bridge to installed skills
- No direct integration

## Webhooks & Callbacks

**Incoming:**
- None

**Outgoing:**
- None

---

*Integration audit: 2026-01-11*
*Update when adding/removing external services*
