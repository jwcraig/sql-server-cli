use assert_cmd::cargo::cargo_bin_cmd;
use std::fs;
use std::path::PathBuf;

fn temp_dir(name: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!("sscli-test-{}-{}", name, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[test]
fn integrations_skills_name_is_respected() {
    let dir = temp_dir("skills");
    let mut cmd = cargo_bin_cmd!("sscli");
    cmd.current_dir(&dir)
        .args(["integrations", "skills", "add", "--name", "custom-skill"]);
    cmd.assert().success();

    let codex = fs::read_to_string(dir.join(".codex/skills/custom-skill/SKILL.md"))
        .expect("read codex skill");
    let claude = fs::read_to_string(dir.join(".claude/skills/custom-skill/SKILL.md"))
        .expect("read claude skill");

    assert!(codex.contains("name: custom-skill"));
    assert!(claude.contains("name: custom-skill"));
}

#[test]
fn integrations_gemini_name_is_respected() {
    let dir = temp_dir("gemini");
    let mut cmd = cargo_bin_cmd!("sscli");
    cmd.current_dir(&dir)
        .args(["integrations", "gemini", "add", "--name", "custom-gemini"]);
    cmd.assert().success();

    let json =
        fs::read_to_string(dir.join(".gemini/extensions/custom-gemini/gemini-extension.json"))
            .expect("read gemini json");
    assert!(json.contains("\"name\": \"custom-gemini\""));
}
