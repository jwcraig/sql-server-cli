use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value;
use std::env;
use std::ffi::OsStr;

pub fn integration_enabled() -> bool {
    env::var("SSCLI_INTEGRATION_TESTS")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

pub fn run_json<I, S>(args: I) -> Value
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut cmd = cargo_bin_cmd!("sscli");
    cmd.args(args);
    let output = cmd.assert().success().get_output().stdout.clone();
    serde_json::from_slice(&output).expect("json")
}
