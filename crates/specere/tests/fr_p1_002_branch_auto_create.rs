//! FR-P1-002. After `specere add speckit` on a git repo, the working tree
//! MUST be on a feature branch (`000-baseline` by default, overridable via
//! `--branch` or `$SPECERE_FEATURE_BRANCH`).
//!
//! These tests set `SPECERE_TEST_SKIP_UVX=1` to bypass the real `uvx specify
//! init` subprocess — the test harness does not have network, and the
//! branch-creation logic is the unit under test.

mod common;

use common::TempRepo;

#[test]
fn default_branch_is_000_baseline() {
    let repo = TempRepo::new();
    let out = repo
        .run_specere(&["add", "speckit"])
        .env("SPECERE_TEST_SKIP_UVX", "1")
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "exit {:?}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(repo.current_branch(), "000-baseline");
}

#[test]
fn branch_override_via_env_var() {
    let repo = TempRepo::new();
    let out = repo
        .run_specere(&["add", "speckit"])
        .env("SPECERE_TEST_SKIP_UVX", "1")
        .env("SPECERE_FEATURE_BRANCH", "alpha-baseline")
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "exit {:?}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(repo.current_branch(), "alpha-baseline");
}

#[test]
fn branch_override_via_cli_flag_wins_over_env() {
    let repo = TempRepo::new();
    let out = repo
        .run_specere(&["add", "speckit", "--branch", "cli-baseline"])
        .env("SPECERE_TEST_SKIP_UVX", "1")
        .env("SPECERE_FEATURE_BRANCH", "env-baseline")
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "exit {:?}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(repo.current_branch(), "cli-baseline");
}

#[test]
fn non_git_target_does_no_branch_op() {
    let repo = TempRepo::new_non_git();
    let out = repo
        .run_specere(&["add", "speckit"])
        .env("SPECERE_TEST_SKIP_UVX", "1")
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "exit {:?}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!repo.abs(".git").exists(), "non-git target grew a .git dir");
}

#[test]
fn preexisting_branch_is_switched_to_not_recreated() {
    let repo = TempRepo::new();
    repo.create_branch("000-baseline");
    let out = repo
        .run_specere(&["add", "speckit"])
        .env("SPECERE_TEST_SKIP_UVX", "1")
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "exit {:?}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(repo.current_branch(), "000-baseline");
}
