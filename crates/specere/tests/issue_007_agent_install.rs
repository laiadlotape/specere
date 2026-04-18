//! Issue #7 — claude-code-deploy installs `.claude/agents/*.md` alongside skills.

mod common;

use common::TempRepo;

#[test]
fn agent_file_written_on_install() {
    let repo = TempRepo::new();
    let out = repo
        .run_specere(&["add", "claude-code-deploy"])
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "install failed — stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let agent_file = repo.abs(".claude/agents/specere-reviewer.md");
    assert!(
        agent_file.exists(),
        "expected .claude/agents/specere-reviewer.md after install"
    );
    let content = std::fs::read_to_string(&agent_file).unwrap();
    assert!(
        content.contains("name: specere-reviewer"),
        "agent file missing frontmatter name:\n{content}"
    );
    assert!(
        content.contains("constitution"),
        "agent prompt missing constitution mention"
    );
}

#[test]
fn remove_strips_agent_file() {
    let repo = TempRepo::new();
    assert!(repo
        .run_specere(&["add", "claude-code-deploy"])
        .output()
        .unwrap()
        .status
        .success());
    assert!(repo.abs(".claude/agents/specere-reviewer.md").exists());

    assert!(repo
        .run_specere(&["remove", "claude-code-deploy"])
        .output()
        .unwrap()
        .status
        .success());
    assert!(
        !repo.abs(".claude/agents/specere-reviewer.md").exists(),
        "agent file should be stripped on remove"
    );
}

#[test]
fn manifest_records_agent_role() {
    let repo = TempRepo::new();
    assert!(repo
        .run_specere(&["add", "claude-code-deploy"])
        .output()
        .unwrap()
        .status
        .success());

    let m: toml::Value =
        toml::from_str(&std::fs::read_to_string(repo.abs(".specere/manifest.toml")).unwrap())
            .unwrap();
    let deploy = m["units"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"].as_str() == Some("claude-code-deploy"))
        .unwrap();
    let files = deploy["files"].as_array().unwrap();
    let agent_entry = files
        .iter()
        .find(|f| f["path"].as_str() == Some(".claude/agents/specere-reviewer.md"))
        .expect("agent in manifest");
    assert_eq!(
        agent_entry["role"].as_str(),
        Some("claude-code-agent-specere-reviewer"),
        "agent role string mismatch"
    );
}
