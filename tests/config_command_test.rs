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
    assert_eq!(value["connection"]["password"], "env-pass");
    assert!(value["settings"].get("allowWriteDefault").is_none());
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
