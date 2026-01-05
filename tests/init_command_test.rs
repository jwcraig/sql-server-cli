use assert_cmd::cargo::cargo_bin_cmd;
use std::fs;
use tempfile::TempDir;

#[test]
fn init_creates_valid_yaml() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join(".sql-server").join("config.yaml");

    let mut cmd = cargo_bin_cmd!("sscli");
    cmd.args(["init", "--path"])
        .arg(temp_dir.path())
        .assert()
        .success();

    assert!(config_path.exists(), "config.yaml should be created");

    let content = fs::read_to_string(&config_path).expect("read config");

    // Parse as YAML to verify it's valid
    let yaml: serde_yaml::Value =
        serde_yaml::from_str(&content).expect("config.yaml should be valid YAML");

    // Verify expected structure
    assert!(yaml.get("defaultProfile").is_some(), "should have defaultProfile");
    assert!(yaml.get("settings").is_some(), "should have settings");
    assert!(yaml.get("profiles").is_some(), "should have profiles");

    // Verify nested structure under settings
    let settings = yaml.get("settings").unwrap();
    assert!(
        settings.get("allowWriteDefault").is_some(),
        "settings should have allowWriteDefault"
    );
    assert!(settings.get("output").is_some(), "settings should have output");

    // Verify nested structure under settings.output
    let output = settings.get("output").unwrap();
    assert!(
        output.get("defaultFormat").is_some(),
        "output should have defaultFormat"
    );
    assert!(output.get("json").is_some(), "output should have json");
    assert!(output.get("csv").is_some(), "output should have csv");

    // Verify profile structure
    let profiles = yaml.get("profiles").unwrap();
    let default_profile = profiles.get("default").unwrap();
    assert!(default_profile.get("server").is_some(), "profile should have server");
    assert!(default_profile.get("port").is_some(), "profile should have port");
    assert!(default_profile.get("database").is_some(), "profile should have database");
}

#[test]
fn init_with_custom_profile_name() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join(".sql-server").join("config.yaml");

    let mut cmd = cargo_bin_cmd!("sscli");
    cmd.args(["init", "--path"])
        .arg(temp_dir.path())
        .args(["--profile", "production"])
        .assert()
        .success();

    let content = fs::read_to_string(&config_path).expect("read config");
    let yaml: serde_yaml::Value = serde_yaml::from_str(&content).expect("valid YAML");

    assert_eq!(
        yaml.get("defaultProfile").and_then(|v| v.as_str()),
        Some("production")
    );
    assert!(
        yaml.get("profiles").and_then(|p| p.get("production")).is_some(),
        "should have production profile"
    );
}

#[test]
fn init_fails_if_exists_without_force() {
    let temp_dir = TempDir::new().unwrap();

    // First init succeeds
    let mut cmd = cargo_bin_cmd!("sscli");
    cmd.args(["init", "--path"])
        .arg(temp_dir.path())
        .assert()
        .success();

    // Second init fails without --force
    let mut cmd = cargo_bin_cmd!("sscli");
    cmd.args(["init", "--path"])
        .arg(temp_dir.path())
        .assert()
        .failure();
}

#[test]
fn init_succeeds_with_force() {
    let temp_dir = TempDir::new().unwrap();

    // First init
    let mut cmd = cargo_bin_cmd!("sscli");
    cmd.args(["init", "--path"])
        .arg(temp_dir.path())
        .assert()
        .success();

    // Second init with --force succeeds
    let mut cmd = cargo_bin_cmd!("sscli");
    cmd.args(["init", "--force", "--path"])
        .arg(temp_dir.path())
        .assert()
        .success();
}
