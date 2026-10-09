use assert_cmd::cargo::cargo_bin_cmd;
use std::fs;
use tempfile::TempDir;

#[test]
fn config_command_emits_json() {
    let mut cmd = cargo_bin_cmd!("sscli");
    cmd.args(["config", "--json"])
        .env("SQL_SERVER", "env-host")
        .env("SQL_DATABASE", "env-db")
        .env("SQL_USER", "env-user")
        .env("SQL_PASSWORD", "env-pass");

    let output = cmd.assert().success().get_output().stdout.clone();
    let value: serde_json::Value = serde_json::from_slice(&output).expect("json");

    assert_eq!(value["connection"]["server"], "env-host");
    assert_eq!(value["connection"]["database"], "env-db");
    assert_eq!(value["connection"]["user"], "env-user");
    assert!(value["connection"]["password"].is_null());
    assert_eq!(value["connection"]["passwordSet"], true);
    assert_eq!(value["connection"]["passwordSource"], "env");
    assert!(value["settings"].get("allowWriteDefault").is_none());
}

#[test]
fn config_command_uses_password_env_flag_without_exposing_secret() {
    let mut cmd = cargo_bin_cmd!("sscli");
    cmd.args([
        "config",
        "--json",
        "--user",
        "sa",
        "--password-env",
        "PLQ_SQL_PASSWORD",
    ])
    .env("PLQ_SQL_PASSWORD", "super-secret");

    let output = cmd.assert().success().get_output().stdout.clone();
    let value: serde_json::Value = serde_json::from_slice(&output).expect("json");

    assert_eq!(value["connection"]["user"], "sa");
    assert!(value["connection"]["password"].is_null());
    assert_eq!(value["connection"]["passwordSet"], true);
    assert_eq!(value["connection"]["passwordSource"], "env");
    assert_eq!(value["connection"]["passwordEnv"], "PLQ_SQL_PASSWORD");
}

#[test]
fn config_command_uses_sscli_url_query_options() {
    let mut cmd = cargo_bin_cmd!("sscli");
    cmd.args(["config", "--json"]).env(
        "SSCLI_URL",
        "sqlserver://sa:secret@127.0.0.1:15433/WDM_VERIFY?trustServerCertificate=false&encrypt=false&timeoutMs=12345",
    );

    let output = cmd.assert().success().get_output().stdout.clone();
    let value: serde_json::Value = serde_json::from_slice(&output).expect("json");

    assert_eq!(value["connection"]["server"], "127.0.0.1");
    assert_eq!(value["connection"]["port"], 15433);
    assert_eq!(value["connection"]["database"], "WDM_VERIFY");
    assert_eq!(value["connection"]["user"], "sa");
    assert!(value["connection"]["password"].is_null());
    assert_eq!(value["connection"]["passwordSet"], true);
    assert_eq!(value["connection"]["passwordSource"], "url");
    assert_eq!(value["connection"]["trustCert"], false);
    assert_eq!(value["connection"]["encrypt"], false);
    assert_eq!(value["connection"]["timeoutMs"], 12345);
}

#[test]
fn config_command_accepts_host_alias() {
    let mut cmd = cargo_bin_cmd!("sscli");
    cmd.args(["config", "--json", "--host", "cli-host"])
        .env("SQL_SERVER", "env-host")
        .env("SQL_DATABASE", "env-db")
        .env("SQL_USER", "env-user")
        .env("SQL_PASSWORD", "env-pass");

    let output = cmd.assert().success().get_output().stdout.clone();
    let value: serde_json::Value = serde_json::from_slice(&output).expect("json");

    assert_eq!(value["connection"]["server"], "cli-host");
}

#[test]
fn config_command_does_not_auto_load_cwd_dotenv() {
    let temp_dir = TempDir::new().expect("temp dir");
    fs::write(
        temp_dir.path().join(".env"),
        "SQL_SERVER=dotenv-host\nSQL_DATABASE=dotenv-db\n",
    )
    .expect("write dotenv");

    let mut cmd = cargo_bin_cmd!("sscli");
    cmd.current_dir(temp_dir.path())
        .env_clear()
        .args(["config", "--json"]);

    let output = cmd.assert().success().get_output().stdout.clone();
    let value: serde_json::Value = serde_json::from_slice(&output).expect("json");

    assert_eq!(value["connection"]["server"], "localhost");
    assert_eq!(value["connection"]["database"], "master");
}

#[test]
fn config_command_loads_explicit_env_file() {
    let temp_dir = TempDir::new().expect("temp dir");
    let env_path = temp_dir.path().join("dev.env");
    fs::write(
        &env_path,
        "SQL_SERVER=dotenv-host\nSQL_DATABASE=dotenv-db\nSQL_USER=dotenv-user\n",
    )
    .expect("write env file");

    let mut cmd = cargo_bin_cmd!("sscli");
    cmd.current_dir(temp_dir.path())
        .env_clear()
        .args(["config", "--json", "--env-file"])
        .arg(&env_path);

    let output = cmd.assert().success().get_output().stdout.clone();
    let value: serde_json::Value = serde_json::from_slice(&output).expect("json");

    assert_eq!(value["connection"]["server"], "dotenv-host");
    assert_eq!(value["connection"]["database"], "dotenv-db");
    assert_eq!(value["connection"]["user"], "dotenv-user");
}

#[test]
fn explicit_env_file_overrides_ambient_environment() {
    let temp_dir = TempDir::new().expect("temp dir");
    let env_path = temp_dir.path().join("dev.env");
    fs::write(
        &env_path,
        "SQL_SERVER=file-host\nSQL_DATABASE=file-db\nSQL_USER=file-user\n",
    )
    .expect("write env file");

    let mut cmd = cargo_bin_cmd!("sscli");
    cmd.current_dir(temp_dir.path())
        .env("SQL_SERVER", "ambient-host")
        .env("SQL_DATABASE", "ambient-db")
        .env("SQL_USER", "ambient-user")
        .args(["config", "--json", "--env-file"])
        .arg(&env_path);

    let output = cmd.assert().success().get_output().stdout.clone();
    let value: serde_json::Value = serde_json::from_slice(&output).expect("json");

    assert_eq!(value["connection"]["server"], "file-host");
    assert_eq!(value["connection"]["database"], "file-db");
    assert_eq!(value["connection"]["user"], "file-user");
}

#[test]
fn config_command_errors_for_missing_env_file() {
    let temp_dir = TempDir::new().expect("temp dir");

    let mut cmd = cargo_bin_cmd!("sscli");
    cmd.current_dir(temp_dir.path()).env_clear().args([
        "config",
        "--json",
        "--env-file",
        "missing.env",
    ]);

    let output = cmd.assert().failure().get_output().stderr.clone();
    let stderr = String::from_utf8_lossy(&output);

    assert!(stderr.contains("Failed to load env file"));
    assert!(stderr.contains("missing.env"));
}

#[test]
fn config_command_ignores_legacy_allow_write_default_setting() {
    let temp_dir = TempDir::new().expect("temp dir");
    let config_path = temp_dir.path().join("config.yaml");
    fs::write(
        &config_path,
        r#"
defaultProfile: default
settings:
  allowWriteDefault: true
profiles:
  default:
    server: legacy-host
    database: legacy-db
"#,
    )
    .expect("write config");

    let mut cmd = cargo_bin_cmd!("sscli");
    cmd.current_dir(temp_dir.path())
        .env_clear()
        .args(["config", "--json", "--config"])
        .arg(&config_path);

    let output = cmd.assert().success().get_output().stdout.clone();
    let value: serde_json::Value = serde_json::from_slice(&output).expect("json");

    assert_eq!(value["connection"]["server"], "legacy-host");
    assert_eq!(value["connection"]["database"], "legacy-db");
    assert!(value["settings"].get("allowWriteDefault").is_none());
}

/// A command run with no config file and nothing naming a server.
fn isolated_cmd(temp_dir: &TempDir) -> assert_cmd::Command {
    let mut cmd = cargo_bin_cmd!("sscli");
    cmd.current_dir(temp_dir.path())
        .env_clear()
        .env("HOME", temp_dir.path())
        .env("XDG_CONFIG_HOME", temp_dir.path());
    cmd
}

#[test]
fn unknown_profile_is_an_error_that_lists_available_profiles() {
    let temp_dir = TempDir::new().expect("temp dir");
    let config_path = temp_dir.path().join("config.yaml");
    fs::write(
        &config_path,
        "profiles:\n  dev:\n    server: dev-host\n  prod:\n    server: prod-host\n",
    )
    .expect("write config");

    let output = isolated_cmd(&temp_dir)
        .args(["config", "--profile", "dve", "--config"])
        .arg(&config_path)
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let stderr = String::from_utf8_lossy(&output);

    assert!(stderr.contains("Profile 'dve' not found"), "{stderr}");
    assert!(stderr.contains("available: dev, prod"), "{stderr}");
}

#[test]
fn profile_without_any_config_file_is_an_error() {
    let temp_dir = TempDir::new().expect("temp dir");

    let output = isolated_cmd(&temp_dir)
        .args(["config", "--profile", "t1-shared"])
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let stderr = String::from_utf8_lossy(&output);

    assert!(stderr.contains("Profile 't1-shared' requested but no config file was found"));
}

#[test]
fn builtin_default_target_warns_on_stderr() {
    let temp_dir = TempDir::new().expect("temp dir");

    let output = isolated_cmd(&temp_dir)
        .args(["config", "--json"])
        .assert()
        .success()
        .get_output()
        .stderr
        .clone();

    assert!(String::from_utf8_lossy(&output).contains("built-in default localhost:1433/master"));
}

#[test]
fn explicit_server_does_not_warn_about_builtin_default() {
    let temp_dir = TempDir::new().expect("temp dir");

    isolated_cmd(&temp_dir)
        .args(["config", "--json", "--server", "db-host"])
        .assert()
        .success()
        .stderr(predicates::str::is_empty());
}

#[test]
fn sql_banner_names_profile_and_config_file() {
    let temp_dir = TempDir::new().expect("temp dir");
    let config_path = temp_dir.path().join("config.yaml");
    fs::write(&config_path, "profiles:\n  dev:\n    server: dev-host\n").expect("write config");

    let output = isolated_cmd(&temp_dir)
        .args(["sql", "--dry-run", "--profile", "dev", "--config"])
        .arg(&config_path)
        .arg("SELECT 1")
        .assert()
        .success()
        .get_output()
        .stderr
        .clone();
    let stderr = String::from_utf8_lossy(&output);

    assert!(
        stderr.contains("Target: dev-host:1433/master (profile dev, "),
        "{stderr}"
    );
    assert!(stderr.contains("config.yaml)"), "{stderr}");
}

#[test]
fn compare_label_with_connection_string_is_not_a_profile() {
    let temp_dir = TempDir::new().expect("temp dir");
    let unreachable = "Server=127.0.0.1,1;Database=app;User ID=sa;Password=x";

    let output = isolated_cmd(&temp_dir)
        .args([
            "compare",
            "--target",
            "prod",
            "--source-connection",
            unreachable,
            "--target-connection",
            unreachable,
            "--summary",
            "--timeout",
            "2000",
        ])
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let stderr = String::from_utf8_lossy(&output);

    assert!(!stderr.contains("Profile 'prod'"), "{stderr}");
}

#[test]
fn init_ignores_profile_selected_by_environment() {
    let temp_dir = TempDir::new().expect("temp dir");
    let config_path = temp_dir.path().join("new.yaml");

    isolated_cmd(&temp_dir)
        .env("SQL_SERVER_PROFILE", "dev")
        .args(["init", "--profile", "dev", "--path"])
        .arg(&config_path)
        .assert()
        .success();

    assert!(config_path.is_file());
}
