use assert_cmd::cargo::cargo_bin_cmd;

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
}
