# Testing Patterns

**Analysis Date:** 2026-01-11

## Test Framework

**Runner:**
- cargo test (Rust built-in)
- No external test runner

**Assertion Library:**
- assert_cmd 2.0 - CLI command execution
- predicates 3.0 - Flexible assertions
- Standard `assert!`, `assert_eq!` macros

**Run Commands:**
```bash
cargo test                              # Run all tests
cargo test -- --nocapture              # Show stdout/stderr
cargo test test_name                   # Run specific test
cargo test -- --test-threads=1        # Sequential execution
```

## Test File Organization

**Location:**
- Integration tests: `tests/*.rs` (separate from source)
- Unit tests: Inline in source files (not currently used extensively)
- Test utilities: `tests/common/mod.rs`

**Naming:**
- `*_test.rs` suffix for test files
- Descriptive names: `help_test.rs`, `init_command_test.rs`

**Structure:**
```
tests/
├── common/
│   └── mod.rs              # Shared test utilities
├── help_test.rs            # Help command tests
├── init_command_test.rs    # Init command tests
├── config_command_test.rs  # Config command tests
├── update_command_test.rs  # Update command tests
├── integrations_test.rs    # Integration skill tests
├── integration_commands_test.rs  # DB integration tests
└── p2_commands_test.rs     # Phase 2 command tests
```

## Test Structure

**Suite Organization:**
```rust
use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn test_help_shows_usage() {
    let mut cmd = Command::cargo_bin("sscli").unwrap();
    cmd.arg("help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage:"));
}

#[test]
fn test_invalid_command_fails() {
    let mut cmd = Command::cargo_bin("sscli").unwrap();
    cmd.arg("nonexistent")
        .assert()
        .failure();
}
```

**Patterns:**
- One test function per behavior
- Descriptive `test_*` function names
- Use `Command::cargo_bin("sscli")` to get CLI binary
- Chain `.arg()`, `.assert()`, `.success()`/`.failure()`
- Use predicates for flexible output matching

## Mocking

**Framework:**
- No dedicated mocking framework
- Tests use real binary execution via assert_cmd
- Environment variables for test configuration

**Patterns:**
```rust
// Set environment for test
cmd.env("SQL_SERVER", "localhost");

// Use temporary directories
use tempfile::TempDir;
let temp = TempDir::new().unwrap();
cmd.current_dir(temp.path());
```

**What to Mock:**
- Database connections (via env vars to test instance)
- File system (via tempfile crate)
- Environment variables

**What NOT to Mock:**
- CLI parsing logic
- Output formatting
- Internal pure functions

## Fixtures and Factories

**Test Data:**
```rust
// Common module provides shared utilities
mod common;

// Temporary directories for isolated tests
use tempfile::TempDir;

fn setup_test_dir() -> TempDir {
    let temp = TempDir::new().unwrap();
    // Setup test files if needed
    temp
}
```

**Location:**
- `tests/common/mod.rs` - Shared test utilities
- Inline test data for simple cases
- No separate fixtures directory

## Coverage

**Requirements:**
- No enforced coverage target
- Focus on critical paths: CLI parsing, error handling
- DB-backed tests are opt-in

**Configuration:**
- No coverage tooling configured
- Can add via `cargo tarpaulin` if needed

**View Coverage:**
```bash
# Install tarpaulin if needed
cargo install cargo-tarpaulin

# Run with coverage
cargo tarpaulin --out Html
open tarpaulin-report.html
```

## Test Types

**Unit Tests:**
- Scope: Not extensively used in this codebase
- Would be inline in source with `#[cfg(test)]` modules

**Integration Tests:**
- Scope: Test CLI end-to-end via assert_cmd
- Location: `tests/*.rs`
- Examples: `help_test.rs`, `init_command_test.rs`

**DB Integration Tests:**
- Opt-in: Require `SSCLI_INTEGRATION_TESTS=1` env var
- Require real SQL Server connection
- Location: `tests/integration_commands_test.rs`, `tests/p2_commands_test.rs`

```bash
# Run DB integration tests
SSCLI_INTEGRATION_TESTS=1 \
SQL_SERVER_CONFIG=/path/to/config.yaml \
SQL_PASSWORD=... \
cargo test
```

## Common Patterns

**CLI Testing:**
```rust
#[test]
fn test_command_with_args() {
    let mut cmd = Command::cargo_bin("sscli").unwrap();
    cmd.args(["tables", "--like", "%User%"])
        .assert()
        .success();
}
```

**Output Assertions:**
```rust
#[test]
fn test_json_output() {
    let mut cmd = Command::cargo_bin("sscli").unwrap();
    cmd.args(["status", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"status\":"));
}
```

**Error Testing:**
```rust
#[test]
fn test_invalid_input_fails() {
    let mut cmd = Command::cargo_bin("sscli").unwrap();
    cmd.args(["describe"])  // Missing required object
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}
```

**Temporary Files:**
```rust
use tempfile::TempDir;

#[test]
fn test_init_creates_config() {
    let temp = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("sscli").unwrap();
    cmd.current_dir(temp.path())
        .arg("init")
        .assert()
        .success();

    assert!(temp.path().join(".sql-server/config.yaml").exists());
}
```

---

*Testing analysis: 2026-01-11*
*Update when test patterns change*
