//! FR-P1-004. `claude-code-deploy` install appends `.claude/settings.local.json`
//! to the target repo's `.gitignore` inside a SpecERE marker-fenced block,
//! preserving all pre-existing content.

mod common;

use common::TempRepo;

fn install_deploy(repo: &TempRepo) {
    let out = repo
        .run_specere(&["add", "claude-code-deploy"])
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "install failed — stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn gitignore_is_created_when_absent() {
    let repo = TempRepo::new();
    // Tempfile tests start with no .gitignore.
    assert!(!repo.abs(".gitignore").exists());
    install_deploy(&repo);

    let ignore = std::fs::read_to_string(repo.abs(".gitignore")).unwrap();
    assert!(
        ignore.contains("specere:begin claude-code-deploy"),
        "fenced block missing:\n{ignore}"
    );
    assert!(
        ignore.contains(".claude/settings.local.json"),
        "settings.local.json entry missing:\n{ignore}"
    );
}

#[test]
fn gitignore_preexisting_content_preserved() {
    let repo = TempRepo::new();
    repo.write(".gitignore", "/target\n*.log\n");

    install_deploy(&repo);
    let ignore = std::fs::read_to_string(repo.abs(".gitignore")).unwrap();
    assert!(
        ignore.starts_with("/target\n*.log"),
        "pre-existing lines altered:\n{ignore}"
    );
    assert!(ignore.contains("specere:begin claude-code-deploy"));
}

#[test]
fn install_is_idempotent_on_gitignore() {
    let repo = TempRepo::new();
    install_deploy(&repo);
    let first = std::fs::read_to_string(repo.abs(".gitignore")).unwrap();
    let first_sha = sha(&first);
    // Second install (no-op at unit level) should not grow .gitignore.
    let _ = repo
        .run_specere(&["add", "claude-code-deploy"])
        .output()
        .expect("spawn");
    let second = std::fs::read_to_string(repo.abs(".gitignore")).unwrap();
    assert_eq!(first_sha, sha(&second), "second install grew .gitignore");
}

fn sha(s: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(s);
    hex::encode(h.finalize())
}
