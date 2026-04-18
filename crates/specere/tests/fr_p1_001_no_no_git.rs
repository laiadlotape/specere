//! FR-P1-001. The `speckit` unit installer MUST NOT request `--no-git`
//! behaviour from the underlying SpecKit scaffolder when the target directory
//! contains a `.git/` directory.

mod common;

use common::TempRepo;

#[test]
fn add_speckit_dry_run_on_git_repo_omits_no_git_flag() {
    let repo = TempRepo::new();
    let out = repo
        .run_specere(&["--dry-run", "add", "speckit"])
        .env("SPECERE_TEST_SKIP_UVX", "1")
        .output()
        .expect("spawn specere");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let all = format!("{stdout}\n{stderr}");

    // The plan should contain the uvx command line but NOT the `--no-git` arg
    // on a git target.
    assert!(
        all.contains("uvx") && all.contains("specify") && all.contains("init"),
        "expected dry-run to print the uvx invocation; got:\n{all}"
    );
    assert!(
        !all.contains("--no-git"),
        "FR-P1-001 violated — `--no-git` appeared in plan for a git repo:\n{all}"
    );
}

#[test]
fn add_speckit_dry_run_on_non_git_repo_includes_no_git_flag() {
    let repo = TempRepo::new_non_git();
    let out = repo
        .run_specere(&["--dry-run", "add", "speckit"])
        .env("SPECERE_TEST_SKIP_UVX", "1")
        .output()
        .expect("spawn specere");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let all = format!("{stdout}\n{stderr}");

    // On a non-git target, `--no-git` IS expected.
    assert!(
        all.contains("--no-git"),
        "non-git fallback missing `--no-git`:\n{all}"
    );
}
