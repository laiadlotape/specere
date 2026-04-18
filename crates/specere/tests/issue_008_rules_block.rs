//! Issue #8 — claude-code-deploy writes a `rules` marker-fenced block in
//! CLAUDE.md, disjoint from the existing `harness` block.

mod common;

use common::TempRepo;

#[test]
fn rules_block_written_on_install_when_claude_md_absent() {
    let repo = TempRepo::new();
    assert!(!repo.abs("CLAUDE.md").exists());

    assert!(repo
        .run_specere(&["add", "claude-code-deploy"])
        .output()
        .unwrap()
        .status
        .success());

    let cm = std::fs::read_to_string(repo.abs("CLAUDE.md")).unwrap();
    assert!(cm.contains("<!-- specere:begin rules -->"));
    assert!(cm.contains("<!-- specere:end rules -->"));
    assert!(
        cm.contains("The 10 composition rules"),
        "rules body missing:\n{cm}"
    );
}

#[test]
fn rules_block_coexists_with_user_claude_md() {
    let repo = TempRepo::new();
    let user_content = "# My project\n\nSome user notes.\n";
    repo.write("CLAUDE.md", user_content);

    assert!(repo
        .run_specere(&["add", "claude-code-deploy"])
        .output()
        .unwrap()
        .status
        .success());

    let cm = std::fs::read_to_string(repo.abs("CLAUDE.md")).unwrap();
    assert!(
        cm.starts_with("# My project"),
        "user content clobbered:\n{cm}"
    );
    assert!(cm.contains("<!-- specere:begin rules -->"));
}

#[test]
fn round_trip_leaves_claude_md_byte_identical() {
    let repo = TempRepo::new();
    let original = "# Project\n\n<!-- SPECKIT START -->\npointer to plan\n<!-- SPECKIT END -->\n";
    repo.write("CLAUDE.md", original);

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

    let after = std::fs::read_to_string(repo.abs("CLAUDE.md")).unwrap();
    assert_eq!(
        original, after,
        "CLAUDE.md not byte-identical after round-trip"
    );
}

#[test]
fn rules_block_stripped_on_remove() {
    let repo = TempRepo::new();
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

    if repo.abs("CLAUDE.md").exists() {
        let cm = std::fs::read_to_string(repo.abs("CLAUDE.md")).unwrap();
        assert!(!cm.contains("specere:begin rules"));
        assert!(!cm.contains("10 composition rules"));
    }
    // If CLAUDE.md was removed entirely because it became empty, that also
    // satisfies "rules block stripped".
}
