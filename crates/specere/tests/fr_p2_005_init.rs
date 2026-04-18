//! FR-P2-005 / issue #15 — `specere init` meta-command:
//! - composes the 5 units (speckit, filter-state, claude-code-deploy,
//!   otel-collector, ears-linter)
//! - fail-fast on first unit error; partial installs recorded in manifest
//! - idempotent: re-run is a no-op

mod common;

use common::TempRepo;

#[test]
fn init_installs_all_five_units() {
    let repo = TempRepo::new();
    let out = repo
        .run_specere(&["init"])
        .env("SPECERE_TEST_SKIP_UVX", "1")
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "init failed — exit {:?}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );

    let manifest_text =
        std::fs::read_to_string(repo.abs(".specere/manifest.toml")).expect("manifest present");
    let m: toml::Value = toml::from_str(&manifest_text).expect("manifest parses");

    let units: Vec<&str> = m["units"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u["id"].as_str().unwrap_or(""))
        .collect();

    for expected in [
        "speckit",
        "filter-state",
        "claude-code-deploy",
        "otel-collector",
        "ears-linter",
    ] {
        assert!(
            units.contains(&expected),
            "init did not install unit `{expected}`; manifest has: {units:?}\nfull manifest:\n{manifest_text}"
        );
    }
}

#[test]
fn reinit_is_idempotent() {
    let repo = TempRepo::new();
    assert!(repo
        .run_specere(&["init"])
        .env("SPECERE_TEST_SKIP_UVX", "1")
        .output()
        .unwrap()
        .status
        .success());

    let manifest_sha_before = sha_of(&repo.abs(".specere/manifest.toml"));

    let out = repo
        .run_specere(&["init"])
        .env("SPECERE_TEST_SKIP_UVX", "1")
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "second init should be a no-op; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let manifest_sha_after = sha_of(&repo.abs(".specere/manifest.toml"));
    assert_eq!(
        manifest_sha_before, manifest_sha_after,
        "re-init mutated manifest — not idempotent"
    );
}

#[test]
fn init_stops_on_first_failure_and_records_partial_state() {
    // Fabricate an orphan `.specify/feature.json` BEFORE init runs.
    // speckit::preflight refuses on orphan state, so init fails fast on
    // the first unit (speckit) and never installs any of the others.
    let repo = TempRepo::new();
    repo.write(
        ".specify/feature.json",
        r#"{"feature_directory":"specs/001-ghost"}"#,
    );
    repo.write(
        "specs/001-ghost/spec.md",
        "# Feature Specification: [FEATURE NAME]\n",
    );

    let out = repo
        .run_specere(&["init"])
        .env("SPECERE_TEST_SKIP_UVX", "1")
        .output()
        .expect("spawn");
    assert!(
        !out.status.success(),
        "init should fail on orphan state; got success\nstdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );

    // No unit should be in the manifest — speckit was the first, it failed,
    // and no later unit ran. (If the manifest file exists at all.)
    if repo.abs(".specere/manifest.toml").exists() {
        let m: toml::Value =
            toml::from_str(&std::fs::read_to_string(repo.abs(".specere/manifest.toml")).unwrap())
                .unwrap();
        let empty: Vec<toml::Value> = Vec::new();
        let units = m.get("units").and_then(|u| u.as_array()).unwrap_or(&empty);
        assert!(
            units.is_empty(),
            "init should not have installed any unit after speckit failure; got {units:?}"
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
