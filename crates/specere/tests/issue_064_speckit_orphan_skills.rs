//! Issue #64 regression — `specere remove speckit` must sweep orphan
//! `.claude/skills/speckit-git-*` directories that the upstream speckit
//! integration drops but doesn't enumerate on uninstall.
//!
//! Scope: we only claim ownership of `speckit-git-*` — other `speckit-*`
//! skills (plan, implement, specify, etc.) belong to `claude-code-deploy`
//! and must survive `specere remove speckit`.

mod common;

use common::TempRepo;

fn seed_skill_dirs(repo: &TempRepo, names: &[&str]) {
    for n in names {
        let dir = repo.abs(&format!(".claude/skills/{n}"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("SKILL.md"), "# placeholder\n").unwrap();
    }
}

#[test]
fn remove_speckit_sweeps_orphan_speckit_git_skills() {
    let repo = TempRepo::new();
    // Install speckit (no-op uvx) so the manifest has an entry to remove.
    let out = repo
        .run_specere(&["add", "speckit"])
        .env("SPECERE_TEST_SKIP_UVX", "1")
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "add speckit failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Simulate the orphan state: upstream integration dropped 5 skill dirs
    // but didn't record them in its own manifest.
    seed_skill_dirs(
        &repo,
        &[
            "speckit-git-commit",
            "speckit-git-feature",
            "speckit-git-initialize",
            "speckit-git-remote",
            "speckit-git-validate",
            // Two skills that belong to OTHER units and must be preserved.
            "speckit-plan",
            "specere-observe-step",
        ],
    );

    let out = repo
        .run_specere(&["remove", "speckit", "--force"])
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "remove speckit failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // All speckit-git-* dirs must be gone.
    for orphan in [
        "speckit-git-commit",
        "speckit-git-feature",
        "speckit-git-initialize",
        "speckit-git-remote",
        "speckit-git-validate",
    ] {
        let path = repo.abs(&format!(".claude/skills/{orphan}"));
        assert!(
            !path.exists(),
            "orphan {orphan} survived `specere remove speckit`"
        );
    }

    // Non-speckit-git skills must NOT have been touched.
    for preserved in ["speckit-plan", "specere-observe-step"] {
        let path = repo.abs(&format!(".claude/skills/{preserved}"));
        assert!(
            path.exists(),
            "non-orphan {preserved} was swept — should belong to another unit"
        );
    }
}

#[test]
fn remove_speckit_is_safe_when_skills_dir_is_absent() {
    let repo = TempRepo::new();
    let out = repo
        .run_specere(&["add", "speckit"])
        .env("SPECERE_TEST_SKIP_UVX", "1")
        .output()
        .expect("spawn");
    assert!(out.status.success());
    // Explicitly ensure no .claude/skills/ exists so the sweep hits the
    // "no entries" path.
    let _ = std::fs::remove_dir_all(repo.abs(".claude"));

    let out = repo
        .run_specere(&["remove", "speckit", "--force"])
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "remove speckit with no skills dir should succeed, got:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
