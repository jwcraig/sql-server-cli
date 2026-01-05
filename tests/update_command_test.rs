use std::io::{Read, Write};
use std::net::TcpListener;

use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::Value;
use tempfile::tempdir;

fn start_github_stub(latest_tag: &str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let addr = listener.local_addr().expect("local addr");
    let latest_tag = latest_tag.to_string();

    std::thread::spawn(move || {
        if let Ok((mut stream, _peer)) = listener.accept() {
            let mut buffer = [0u8; 2048];
            let _ = stream.read(&mut buffer);

            let body = format!(
                "{{\"tag_name\":\"{}\",\"html_url\":\"https://example.test/releases/tag/{}\"}}",
                latest_tag, latest_tag
            );
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });

    format!("http://{}", addr)
}

fn run_update_json(subcommand: &str, latest_tag: &str) -> Value {
    let config_dir = tempdir().expect("temp config dir");
    let api_base = start_github_stub(latest_tag);

    let mut cmd = cargo_bin_cmd!("sscli");
    cmd.args([subcommand, "--json"]);
    cmd.env("SSCLI_GITHUB_API_BASE", api_base);
    cmd.env("SSCLI_CONFIG_DIR", config_dir.path());
    let output = cmd.assert().success().get_output().stdout.clone();
    serde_json::from_slice(&output).expect("json")
}

#[test]
fn update_reports_update_available_in_json() {
    let value = run_update_json("update", "v999.0.0");
    assert_eq!(value["latestVersion"], "999.0.0");
    assert_eq!(value["updateAvailable"], true);
    assert_eq!(value["repo"], "jwcraig/sql-server-cli");
}

#[test]
fn upgrade_is_an_alias_for_update() {
    let value = run_update_json("upgrade", "v999.0.0");
    assert_eq!(value["latestVersion"], "999.0.0");
    assert_eq!(value["updateAvailable"], true);
}

#[test]
fn update_reports_no_update_when_versions_match() {
    let current = format!("v{}", env!("CARGO_PKG_VERSION"));
    let value = run_update_json("update", &current);
    assert_eq!(value["latestVersion"], env!("CARGO_PKG_VERSION"));
    assert_eq!(value["updateAvailable"], false);
}
