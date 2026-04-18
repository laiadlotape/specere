//! FR-P1-006. `specere add <unit> && specere remove <unit>` restores the
//! tree to byte-identical pre-install state (modulo files flipped to
//! Owner::UserEditedAfterInstall, not exercised here).

mod common;

use common::TempRepo;

fn sha(path: &std::path::Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(&bytes);
    Some(hex::encode(h.finalize()))
}

#[test]
fn deploy_install_remove_leaves_gitignore_byte_identical() {
    let repo = TempRepo::new();
    repo.write(".gitignore", "/target\n*.log\n");
    let pre = std::fs::read_to_string(repo.abs(".gitignore")).unwrap();

    let out = repo
        .run_specere(&["add", "claude-code-deploy"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "install: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let out = repo
        .run_specere(&["remove", "claude-code-deploy"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "remove: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let post = std::fs::read_to_string(repo.abs(".gitignore")).unwrap();
    assert_eq!(pre, post, ".gitignore not byte-identical after round-trip");
}

#[test]
fn deploy_install_remove_leaves_extensions_yml_byte_identical() {
    let repo = TempRepo::new();
    std::fs::create_dir_all(repo.abs(".specify")).unwrap();
    let base_yml = "installed: []\nsettings:\n  auto_execute_hooks: true\nhooks:\n  after_implement:\n  - extension: git\n    command: speckit.git.commit\n    enabled: true\n    optional: true\n    prompt: Commit implementation changes?\n    description: Auto-commit after implementation\n    condition: null\n";
    repo.write(".specify/extensions.yml", base_yml);
    let pre_sha = sha(&repo.abs(".specify/extensions.yml")).unwrap();

    assert!(repo
        .run_specere(&["add", "claude-code-deploy"])
        .output()
        .unwrap()
        .status
        .success());
    assert!(repo
        .run_specere(&["remove", "claude-code-deploy"])
        .output()
        .unwrap()
        .status
        .success());

    let post_sha = sha(&repo.abs(".specify/extensions.yml")).unwrap();
    assert_eq!(
        pre_sha, post_sha,
        "extensions.yml not byte-identical after round-trip"
    );
}

#[test]
fn deploy_install_remove_with_empty_gitignore_removes_file() {
    // When install created the .gitignore (no pre-existing), remove should
    // leave no .gitignore behind.
    let repo = TempRepo::new();
    assert!(!repo.abs(".gitignore").exists());

    assert!(repo
        .run_specere(&["add", "claude-code-deploy"])
        .output()
        .unwrap()
        .status
        .success());
    assert!(
        repo.abs(".gitignore").exists(),
        "install should have created .gitignore"
    );

    assert!(repo
        .run_specere(&["remove", "claude-code-deploy"])
        .output()
        .unwrap()
        .status
        .success());
    assert!(
        !repo.abs(".gitignore").exists(),
        ".gitignore should be gone after remove (pre-install state had none)"
    );
}

#[test]
fn user_added_gitignore_lines_preserved_across_round_trip() {
    let repo = TempRepo::new();
    let original = "/target\n*.log\n";
    repo.write(".gitignore", original);
    assert!(repo
        .run_specere(&["add", "claude-code-deploy"])
        .output()
        .unwrap()
        .status
        .success());

    // User edits the gitignore AFTER install, adding an unrelated line.
    let mut edited = std::fs::read_to_string(repo.abs(".gitignore")).unwrap();
    edited.push_str("/user-added-dir/\n");
    repo.write(".gitignore", &edited);

    assert!(repo
        .run_specere(&["remove", "claude-code-deploy"])
        .output()
        .unwrap()
        .status
        .success());
    let post = std::fs::read_to_string(repo.abs(".gitignore")).unwrap();
    assert!(post.contains("/target"));
    assert!(post.contains("*.log"));
    assert!(post.contains("/user-added-dir/"));
    assert!(!post.contains(".claude/settings.local.json"));
}
