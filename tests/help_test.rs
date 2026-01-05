use assert_cmd::cargo::cargo_bin_cmd;

#[test]
fn help_shows_core_commands_only() {
    let mut cmd = cargo_bin_cmd!("sscli");
    cmd.arg("--help");
    let output = cmd.assert().success().get_output().stdout.clone();
    let stdout = String::from_utf8_lossy(&output);

    for name in [
        "status",
        "databases",
        "tables",
        "describe",
        "sql",
        "table-data",
        "columns",
        "update",
        "init",
        "config",
    ] {
        assert!(stdout.contains(name), "missing core command: {}", name);
    }

    for name in [
        "sessions",
        "query-stats",
        "backups",
        "integrations",
        "foreign-keys",
        "indexes",
        "stored-procs",
        "completions",
    ] {
        assert!(!stdout.contains(name), "advanced command leaked: {}", name);
    }
}

#[test]
fn help_all_shows_advanced_commands() {
    let mut cmd = cargo_bin_cmd!("sscli");
    cmd.args(["help", "--all"]);
    let output = cmd.assert().success().get_output().stdout.clone();
    let stdout = String::from_utf8_lossy(&output);

    for name in [
        "sessions",
        "query-stats",
        "backups",
        "integrations",
        "foreign-keys",
        "indexes",
        "stored-procs",
        "completions",
    ] {
        assert!(stdout.contains(name), "missing advanced command: {}", name);
    }
}
