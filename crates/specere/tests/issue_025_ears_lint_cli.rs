//! Issue #25 — `specere lint ears` CLI subcommand that runs the rules from
//! `.specere/lint/ears.toml` against the active feature's `spec.md` and
//! prints findings. Exits 0 regardless of findings (advisory per FR-P2-003).
//!
//! This test replaces the dogfood walk-through the user surfaced: fabricate
//! a foo feature with 3 broken FRs + 2 compliant, assert the known findings.

mod common;

use common::TempRepo;

const FOO_SPEC: &str = r#"# Feature Specification: Foo (lint test)

## Requirements

### Functional Requirements

- **FR-001**: System MUST authenticate users via password.
- **FR-002**: System MUST be robust and intuitive.
- User can reset password.
- **FR-003**: WHEN a user logs in, session MUST be created.
- **FR-004**: System will ensure efficient data retrieval.
"#;

fn setup_foo_feature(repo: &TempRepo) {
    // Minimum ears-linter scaffold + feature pointer. We don't run the full
    // `specere add ears-linter` because that also touches extensions.yml and
    // is exercised elsewhere — this test focuses on the lint itself.
    std::fs::create_dir_all(repo.abs(".specere/lint")).unwrap();
    repo.write(
        ".specere/lint/ears.toml",
        include_str!("../../specere-units/src/ears_linter/rules.toml"),
    );
    std::fs::create_dir_all(repo.abs(".specify")).unwrap();
    repo.write(
        ".specify/feature.json",
        r#"{"feature_directory":"specs/999-foo-lint-test"}"#,
    );
    std::fs::create_dir_all(repo.abs("specs/999-foo-lint-test")).unwrap();
    repo.write("specs/999-foo-lint-test/spec.md", FOO_SPEC);
}

#[test]
fn lint_ears_accepts_feature_dir_alias() {
    // Issue #61 regression — the parser used to only accept the full
    // `feature_directory` key name. The self-dogfood guide's T-31 scenario
    // documented `feature_dir` which errored. Now both aliases work.
    let repo = TempRepo::new();
    std::fs::create_dir_all(repo.abs(".specere/lint")).unwrap();
    repo.write(
        ".specere/lint/ears.toml",
        include_str!("../../specere-units/src/ears_linter/rules.toml"),
    );
    std::fs::create_dir_all(repo.abs(".specify")).unwrap();
    repo.write(
        ".specify/feature.json",
        r#"{"feature_dir":"specs/999-alias-test"}"#,
    );
    std::fs::create_dir_all(repo.abs("specs/999-alias-test")).unwrap();
    repo.write("specs/999-alias-test/spec.md", FOO_SPEC);

    let out = repo.run_specere(&["lint", "ears"]).output().expect("spawn");
    assert!(
        out.status.success(),
        "lint ears should exit 0 on feature_dir alias.\nstderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Sanity: findings still produced (same spec content as the main test).
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("FR-002") || stdout.contains("findings"),
        "expected ears findings in stdout; got: {stdout}"
    );
}

#[test]
fn lint_ears_rejects_malformed_feature_json() {
    // Issue #61 — the parser now uses serde_json so malformed JSON gets a
    // clear parse-error chain rather than a silent "could not parse" string.
    let repo = TempRepo::new();
    std::fs::create_dir_all(repo.abs(".specere/lint")).unwrap();
    repo.write(
        ".specere/lint/ears.toml",
        include_str!("../../specere-units/src/ears_linter/rules.toml"),
    );
    std::fs::create_dir_all(repo.abs(".specify")).unwrap();
    repo.write(".specify/feature.json", "not valid json {{");
    let out = repo.run_specere(&["lint", "ears"]).output().expect("spawn");
    assert!(
        !out.status.success(),
        "lint ears should error on malformed JSON"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("feature_directory") && stderr.contains("feature_dir"),
        "error should name both accepted keys for discoverability:\n{stderr}"
    );
}

#[test]
fn lint_ears_catches_three_bad_bullets() {
    let repo = TempRepo::new();
    setup_foo_feature(&repo);

    let out = repo.run_specere(&["lint", "ears"]).output().expect("spawn");
    assert!(
        out.status.success(),
        "lint must exit 0 regardless of findings; got {:?}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);

    // Every rule id should appear at least once across the three bad bullets.
    for rule in [
        "ears-fr-prefix",
        "ears-must-should",
        "ears-no-ambiguous-adj",
    ] {
        assert!(
            stdout.contains(rule),
            "stdout missing rule id `{rule}`; got:\n{stdout}"
        );
    }

    // Specific findings we expect (the exact hits reported in issue #25):
    for excerpt in [
        "robust",
        "intuitive",
        "efficient",
        "User can reset password",
    ] {
        assert!(
            stdout.contains(excerpt),
            "stdout missing excerpt `{excerpt}`; got:\n{stdout}"
        );
    }
}

#[test]
fn lint_ears_exits_zero_on_compliant_spec() {
    let repo = TempRepo::new();
    std::fs::create_dir_all(repo.abs(".specere/lint")).unwrap();
    repo.write(
        ".specere/lint/ears.toml",
        include_str!("../../specere-units/src/ears_linter/rules.toml"),
    );
    std::fs::create_dir_all(repo.abs(".specify")).unwrap();
    repo.write(
        ".specify/feature.json",
        r#"{"feature_directory":"specs/001-clean"}"#,
    );
    std::fs::create_dir_all(repo.abs("specs/001-clean")).unwrap();
    repo.write(
        "specs/001-clean/spec.md",
        "## Requirements\n\n### Functional Requirements\n\n- **FR-001**: System MUST do X.\n- **FR-002**: System MUST do Y.\n",
    );

    let out = repo.run_specere(&["lint", "ears"]).output().expect("spawn");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    // No findings — just a short OK line.
    assert!(
        !stdout.contains("[WARN") && !stdout.contains("[INFO") && !stdout.contains("[ERROR"),
        "compliant spec should produce zero findings; got:\n{stdout}"
    );
}

#[test]
fn lint_ears_handles_missing_feature_json_gracefully() {
    let repo = TempRepo::new();
    // No .specify/feature.json — the lint should print a skip message, exit 0.
    std::fs::create_dir_all(repo.abs(".specere/lint")).unwrap();
    repo.write(
        ".specere/lint/ears.toml",
        include_str!("../../specere-units/src/ears_linter/rules.toml"),
    );

    let out = repo.run_specere(&["lint", "ears"]).output().expect("spawn");
    assert!(
        out.status.success(),
        "missing feature.json should not error; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        all.to_lowercase().contains("no active feature") || all.to_lowercase().contains("skipping"),
        "expected skip message; got:\n{all}"
    );
}

#[test]
fn lint_ears_handles_missing_rules_gracefully() {
    let repo = TempRepo::new();
    // Rules file absent — print a message, exit 0.
    std::fs::create_dir_all(repo.abs(".specify")).unwrap();
    repo.write(
        ".specify/feature.json",
        r#"{"feature_directory":"specs/001"}"#,
    );

    let out = repo.run_specere(&["lint", "ears"]).output().expect("spawn");
    assert!(
        out.status.success(),
        "missing rules should not error; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
