use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;

#[test]
fn sql_dry_run_accepts_stdin() {
    let mut cmd = cargo_bin_cmd!("sscli");
    cmd.args(["sql", "--json", "--dry-run", "--stdin"])
        .write_stdin("SELECT 1 AS value");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"dryRun\": true"))
        .stdout(predicate::str::contains("\"batchCount\": 1"));
}

#[test]
fn sql_json_keeps_banner_on_stderr() {
    let mut cmd = cargo_bin_cmd!("sscli");
    cmd.args(["sql", "--json", "--dry-run", "SELECT 1 AS value"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::starts_with("{"))
        .stderr(predicate::str::starts_with("Target: "))
        .stderr(predicate::str::contains("/"));
}

#[test]
fn sql_quiet_target_suppresses_banner() {
    let mut cmd = cargo_bin_cmd!("sscli");
    cmd.args([
        "sql",
        "--json",
        "--dry-run",
        "--quiet-target",
        "SELECT 1 AS value",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::starts_with("{"))
        .stderr(predicate::str::is_empty());
}

#[test]
fn sql_quiet_suppresses_banner() {
    let mut cmd = cargo_bin_cmd!("sscli");
    cmd.args(["-q", "sql", "--json", "--dry-run", "SELECT 1 AS value"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::is_empty());
}

#[test]
fn bare_sql_shorthand_accepts_leading_sql_flags() {
    let mut cmd = cargo_bin_cmd!("sscli");
    cmd.args(["--json", "--dry-run", "SELECT 1 AS value"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"batchCount\": 1"))
        .stderr(predicate::str::starts_with("Target: "));
}

#[test]
fn bare_sql_shorthand_accepts_attached_short_option_values() {
    let mut cmd = cargo_bin_cmd!("sscli");
    cmd.args(["-Hlocalhost", "-dmaster", "--json", "--dry-run", "SELECT 1"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"batchCount\": 1"))
        .stderr(predicate::str::contains("Target: localhost:"))
        .stderr(predicate::str::contains("/master"));
}

#[test]
fn bare_sql_shorthand_accepts_sql_starting_with_comment() {
    let mut cmd = cargo_bin_cmd!("sscli");
    cmd.args(["--json", "--dry-run", "-- header\nSELECT 1"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"batchCount\": 1"))
        .stdout(predicate::str::contains("-- header\\nSELECT 1"));
}

#[test]
fn sql_dry_run_handles_nested_block_comments_around_go() {
    let mut cmd = cargo_bin_cmd!("sscli");
    cmd.args(["sql", "--json", "--dry-run", "--stdin"])
        .write_stdin("/* outer\n/* inner */\nGO\n*/\nSELECT 1\nGO\nSELECT 2\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("\"batchCount\": 2"));
}
