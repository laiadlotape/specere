//! Issue #16 — `speckit::preflight` refuses over an orphan `.specify/`
//! artifact left behind by an aborted `specify workflow run`. `specere doctor
//! --clean-orphans` sweeps the artifact.

mod common;

use common::TempRepo;

/// Fabricate the exact state observed on 2026-04-18: `.specify/feature.json`
/// points at a `specs/NNN-.../` dir whose `spec.md` is still the unfilled
/// template (i.e. contains `[FEATURE NAME]`).
fn fabricate_orphan(repo: &TempRepo) {
    repo.write(
        ".specify/feature.json",
        r#"{"feature_directory":"specs/001-ghost"}"#,
    );
    repo.write(
        "specs/001-ghost/spec.md",
        "# Feature Specification: [FEATURE NAME]\n\n**Feature Branch**: `[###-feature-name]`\n",
    );
}

#[test]
fn preflight_refuses_on_orphan_specify_state() {
    let repo = TempRepo::new();
    fabricate_orphan(&repo);

    let out = repo
        .run_specere(&["--dry-run", "add", "speckit"])
        .env("SPECERE_TEST_SKIP_UVX", "1")
        .output()
        .expect("spawn");
    assert!(
        !out.status.success(),
        "expected refuse on orphan state; got exit 0\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("orphan") || stderr.contains("Orphan"),
        "stderr should mention orphan; got:\n{stderr}"
    );
    assert!(
        stderr.contains("doctor"),
        "stderr should cite `specere doctor --clean-orphans`; got:\n{stderr}"
    );
}

#[test]
fn clean_orphans_removes_fabricated_state() {
    let repo = TempRepo::new();
    fabricate_orphan(&repo);

    let out = repo
        .run_specere(&["doctor", "--clean-orphans"])
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "clean-orphans failed — exit {:?}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(
        !repo.abs("specs/001-ghost").exists(),
        "orphan spec dir not removed"
    );
    assert!(
        !repo.abs(".specify/feature.json").exists(),
        "orphan feature.json not removed"
    );
}

#[test]
fn non_orphan_spec_preserved() {
    // A real feature dir: spec.md with `[FEATURE NAME]` replaced.
    let repo = TempRepo::new();
    repo.write(
        ".specify/feature.json",
        r#"{"feature_directory":"specs/001-real-feature"}"#,
    );
    repo.write(
        "specs/001-real-feature/spec.md",
        "# Feature Specification: Real Feature\n\nContent.\n",
    );
    let out = repo
        .run_specere(&["doctor", "--clean-orphans"])
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "doctor should succeed on non-orphan state"
    );
    // Critical: real content preserved.
    assert!(repo.abs("specs/001-real-feature/spec.md").exists());
    assert!(repo.abs(".specify/feature.json").exists());
}

#[test]
fn no_specify_state_at_all_passes_clean() {
    let repo = TempRepo::new();
    // No .specify/ or specs/ at all — doctor must not error.
    let out = repo
        .run_specere(&["doctor", "--clean-orphans"])
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "doctor should succeed when there's no state\nstderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
