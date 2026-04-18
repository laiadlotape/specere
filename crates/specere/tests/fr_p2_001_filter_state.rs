//! FR-P2-001. `filter-state` unit creates `.specere/` skeleton + gitignore
//! allowlist, is idempotent, and round-trips cleanly.
//!
//! Issue #12 acceptance criteria:
//! - Install writes .specere/{events.sqlite, posterior.toml, sensor-map.toml}
//! - .gitignore has `.specere/*` + allowlist block (marker-fenced)
//! - Idempotent re-install is a no-op
//! - Remove inverts cleanly (byte-identical round-trip)
//! - Manifest records every file with role `filter-state-*`

mod common;

use common::TempRepo;

fn install(repo: &TempRepo) {
    let out = repo
        .run_specere(&["add", "filter-state"])
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "install failed — exit {:?}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn fresh_install_creates_skeleton() {
    let repo = TempRepo::new();
    install(&repo);

    assert!(repo.abs(".specere").is_dir(), ".specere/ dir not created");
    assert!(repo.abs(".specere/events.sqlite").exists());
    assert!(repo.abs(".specere/posterior.toml").exists());
    assert!(repo.abs(".specere/sensor-map.toml").exists());

    // posterior.toml must carry a schema_version stamp (Phase 3+ filter engine
    // reads this).
    let p = std::fs::read_to_string(repo.abs(".specere/posterior.toml")).unwrap();
    assert!(
        p.contains("schema_version"),
        "posterior.toml missing schema_version:\n{p}"
    );
}

#[test]
fn install_writes_gitignore_marker_block_with_allowlist() {
    let repo = TempRepo::new();
    install(&repo);

    let ign = std::fs::read_to_string(repo.abs(".gitignore")).unwrap();
    assert!(
        ign.contains("specere:begin filter-state"),
        ".gitignore missing filter-state marker block:\n{ign}"
    );
    assert!(
        ign.contains(".specere/*"),
        ".gitignore missing .specere/* glob"
    );
    // Allowlist: at least these four must survive a `git clean -fX`.
    for keep in [
        "!.specere/manifest.toml",
        "!.specere/sensor-map.toml",
        "!.specere/review-queue.md",
        "!.specere/decisions.log",
    ] {
        assert!(
            ign.contains(keep),
            ".gitignore missing allowlist entry `{keep}`:\n{ign}"
        );
    }
}

#[test]
fn gitignore_preserves_user_lines() {
    let repo = TempRepo::new();
    repo.write(".gitignore", "/target\n*.log\n");
    install(&repo);

    let ign = std::fs::read_to_string(repo.abs(".gitignore")).unwrap();
    assert!(
        ign.starts_with("/target\n*.log"),
        "pre-existing .gitignore content altered:\n{ign}"
    );
    assert!(ign.contains("specere:begin filter-state"));
}

#[test]
fn reinstall_is_idempotent() {
    let repo = TempRepo::new();
    install(&repo);
    let sha_before = sha_of(&repo.abs(".gitignore"));
    let out = repo
        .run_specere(&["add", "filter-state"])
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "re-install should be a no-op; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let sha_after = sha_of(&repo.abs(".gitignore"));
    assert_eq!(
        sha_before, sha_after,
        "re-install mutated .gitignore — not idempotent"
    );
}

#[test]
fn remove_round_trip_is_byte_identical() {
    let repo = TempRepo::new();
    let original = "/target\n*.log\n";
    repo.write(".gitignore", original);
    let pre = std::fs::read_to_string(repo.abs(".gitignore")).unwrap();

    install(&repo);
    let out = repo
        .run_specere(&["remove", "filter-state"])
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "remove failed — stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let post = std::fs::read_to_string(repo.abs(".gitignore")).unwrap();
    assert_eq!(pre, post, ".gitignore not byte-identical after round-trip");
    assert!(!repo.abs(".specere").exists(), ".specere/ not removed");
}

#[test]
fn manifest_records_files_with_filter_state_role() {
    let repo = TempRepo::new();
    install(&repo);

    let m: toml::Value =
        toml::from_str(&std::fs::read_to_string(repo.abs(".specere/manifest.toml")).unwrap())
            .unwrap();
    let fs_unit = m["units"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"].as_str() == Some("filter-state"))
        .expect("filter-state in manifest");
    let files = fs_unit["files"].as_array().unwrap();
    let roles: Vec<&str> = files
        .iter()
        .map(|f| f["role"].as_str().unwrap_or(""))
        .collect();
    // Every recorded file's role starts with filter-state- (skeleton entries).
    for role in &roles {
        assert!(
            role.starts_with("filter-state-"),
            "role `{role}` doesn't match `filter-state-*`"
        );
    }
}

fn sha_of(path: &std::path::Path) -> String {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(path).unwrap_or_default();
    let mut h = Sha256::new();
    h.update(&bytes);
    hex::encode(h.finalize())
}
