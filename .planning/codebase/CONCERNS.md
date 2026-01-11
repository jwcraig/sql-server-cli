# Codebase Concerns

**Analysis Date:** 2026-01-11

## Tech Debt

**Manual drop statement generation:**
- Issue: Compare command generates `-- TODO:` comments instead of actual DROP statements
- Files: `src/commands/compare.rs` (lines 1401, 1470)
- Why: Safety-first approach, drops are destructive
- Impact: Users must manually add DROP statements
- Fix approach: Add `--include-drops` flag (already implemented but incomplete for some object types)

**Large CLI args file:**
- Issue: `src/cli/args.rs` is 600+ lines with all argument definitions
- Files: `src/cli/args.rs`
- Why: Incremental growth as commands were added
- Impact: Hard to navigate, find specific argument definitions
- Fix approach: Split into per-command arg files or use clap derive macros more

## Known Bugs

**None identified during analysis.**

Codebase appears clean with no obvious bugs. TODOs in compare.rs are intentional incomplete features, not bugs.

## Security Considerations

**Read-only enforcement is client-side:**
- Risk: SQL blocking is regex-based in `src/safety/read_only.rs`, could potentially be bypassed
- Files: `src/safety/read_only.rs`
- Current mitigation: Conservative keyword blocking, `--allow-write` explicit override
- Recommendations: Consider adding server-side READONLY transaction mode when possible

**Password in memory:**
- Risk: Password passed via CLI args or env vars is held in memory
- Files: `src/cli/args.rs`, `src/config/env.rs`
- Current mitigation: Not logged, no persistent storage
- Recommendations: Consider `secrecy` crate for sensitive data

## Performance Bottlenecks

**No significant concerns.**

- Single query per command, no N+1 patterns
- Async database connection (tiberius + tokio)
- Streaming results where applicable

## Fragile Areas

**Compare command complexity:**
- Files: `src/commands/compare.rs` (~1500 lines)
- Why fragile: Complex multi-profile comparison, diff generation, apply script creation
- Common failures: Edge cases in object type handling
- Safe modification: Add tests for specific object types before changing
- Test coverage: Limited integration tests for compare

## Scaling Limits

**Not applicable:**
- CLI tool, not a service
- Each invocation is independent
- No connection pooling needed

## Dependencies at Risk

**Tiberius (SQL Server driver):**
- Status: Actively maintained but small community
- Risk: Breaking changes in SQL Server protocol support
- Impact: Core database connectivity
- Migration plan: No alternative pure-Rust SQL Server driver exists

**Rust 2024 edition:**
- Status: Uses `edition = "2024"` which requires Rust 1.85+
- Risk: Users may have older Rust versions
- Impact: Compilation failures on older toolchains
- Migration plan: Document Rust version requirement clearly

## Missing Critical Features

**No stored procedure parameter inspection:**
- Problem: `describe` shows proc definition but not parameter types/defaults
- Current workaround: Users run catalog queries manually
- Blocks: Full schema documentation
- Implementation complexity: Low (add INFORMATION_SCHEMA query)

**No view dependency tracking:**
- Problem: No way to see which tables a view depends on
- Current workaround: Read view definition manually
- Blocks: Impact analysis before table changes
- Implementation complexity: Medium (parse view definition or use sys.sql_expression_dependencies)

## Test Coverage Gaps

**Compare command integration tests:**
- What's not tested: Full compare workflow with two profiles
- Files: `src/commands/compare.rs`
- Risk: Schema drift detection could break silently
- Priority: Medium
- Difficulty: Requires two SQL Server instances or profiles

**Error classification paths:**
- What's not tested: All error kinds properly classified
- Files: `src/error.rs`
- Risk: Wrong error type in JSON output
- Priority: Low
- Difficulty: Easy to add unit tests

---

*Concerns audit: 2026-01-11*
*Update as issues are fixed or new ones discovered*
