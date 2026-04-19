//! FR-EQ-005 — filter-run integration: consumes mutation_result +
//! test_smell_detected events, computes per-spec Calibration, logs
//! the calibration summary, and uses the per-spec alphas for future
//! test_outcome updates.

mod common;

use common::TempRepo;

fn seed_sensor_map(repo: &TempRepo) {
    repo.write(
        ".specere/sensor-map.toml",
        r#"
schema_version = 1

[specs]
"auth"    = { support = ["src/auth/"] }
"billing" = { support = ["src/billing/"] }

[channels]
"#,
    );
}

fn seed_mutation_events(repo: &TempRepo, spec_id: &str, caught: u32, missed: u32) {
    for _ in 0..caught {
        repo.run_specere(&[
            "observe",
            "record",
            "--source",
            "cargo-mutants",
            "--attr",
            "event_kind=mutation_result",
            "--attr",
            &format!("spec_id={spec_id}"),
            "--attr",
            "outcome=caught",
        ])
        .output()
        .expect("spawn");
    }
    for _ in 0..missed {
        repo.run_specere(&[
            "observe",
            "record",
            "--source",
            "cargo-mutants",
            "--attr",
            "event_kind=mutation_result",
            "--attr",
            &format!("spec_id={spec_id}"),
            "--attr",
            "outcome=missed",
        ])
        .output()
        .expect("spawn");
    }
}

#[test]
fn filter_run_uses_prototype_calibration_when_no_evidence() {
    // With zero mutation or smell events, Calibration falls back to prototype
    // (q=1.0). The output should match v1.0.4 behaviour. We check by running
    // filter without evidence and confirming no "calibration" block in stdout.
    let repo = TempRepo::new();
    seed_sensor_map(&repo);
    // Add a pass event so filter has something to process.
    repo.run_specere(&[
        "observe",
        "record",
        "--source",
        "test",
        "--attr",
        "event_kind=test_outcome",
        "--attr",
        "spec_id=auth",
        "--attr",
        "outcome=pass",
    ])
    .output()
    .expect("spawn");

    let out = repo
        .run_specere(&["filter", "run"])
        .output()
        .expect("spawn");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    // No calibration block — every spec at q=1.0 is suppressed.
    assert!(
        !stdout.contains("calibration (per spec"),
        "expected no calibration block when every spec at q=1.0; got:\n{stdout}"
    );
}

#[test]
fn filter_run_reports_calibration_when_weak_tests_detected() {
    // Seed 2 caught + 8 missed mutations for `billing` → kill_rate = 0.20.
    // With no smells, q = 0.20 clamped to 0.30 floor.
    let repo = TempRepo::new();
    seed_sensor_map(&repo);
    seed_mutation_events(&repo, "billing", 2, 8);
    // Also need a test_outcome event so filter runs (no-new-events is a no-op).
    repo.run_specere(&[
        "observe",
        "record",
        "--source",
        "test",
        "--attr",
        "event_kind=test_outcome",
        "--attr",
        "spec_id=auth",
        "--attr",
        "outcome=pass",
    ])
    .output()
    .expect("spawn");

    let out = repo
        .run_specere(&["filter", "run"])
        .output()
        .expect("spawn");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("calibration (per spec"),
        "expected calibration block; got:\n{stdout}"
    );
    assert!(
        stdout.contains("billing"),
        "billing should appear in calibration block:\n{stdout}"
    );
    // q=0.30 → α_sat = 0.661, α_vio = 0.585. We look for the string "q=0.30".
    assert!(
        stdout.contains("q=0.30"),
        "expected billing at q=0.30 (clamped); got:\n{stdout}"
    );
    // Low-quality flag fires (< 0.5).
    assert!(
        stdout.contains("low evidence"),
        "expected low-evidence flag on billing:\n{stdout}"
    );
}

#[test]
fn filter_run_high_kill_rate_stays_near_prototype() {
    // 18 caught + 2 missed → kill_rate = 0.90, q = 0.90.
    // α_sat = 0.55 + 0.90*0.37 = 0.883 → "q=0.90"
    let repo = TempRepo::new();
    seed_sensor_map(&repo);
    seed_mutation_events(&repo, "auth", 18, 2);
    repo.run_specere(&[
        "observe",
        "record",
        "--source",
        "test",
        "--attr",
        "event_kind=test_outcome",
        "--attr",
        "spec_id=billing",
        "--attr",
        "outcome=pass",
    ])
    .output()
    .expect("spawn");

    let out = repo
        .run_specere(&["filter", "run"])
        .output()
        .expect("spawn");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("q=0.90"),
        "expected auth at q=0.90; got:\n{stdout}"
    );
    // High kill rate → NOT flagged as suspicious.
    assert!(
        !stdout.contains("auth")
            || !stdout
                .lines()
                .any(|l| l.contains("auth") && l.contains("low evidence")),
        "high-kill-rate spec should NOT be flagged low-evidence:\n{stdout}"
    );
}

#[test]
fn filter_run_smells_compress_calibration() {
    // No mutations, but 3 smells on `auth`:
    //   smell_penalty = 1.0 - 0.15*3 = 0.55
    //   kill_rate = 1.0 (no mutation evidence)
    //   q = 1.0 * 0.55 = 0.55
    let repo = TempRepo::new();
    seed_sensor_map(&repo);
    for (i, smell) in ["tautological-assert", "no-assertion", "mock-only"]
        .iter()
        .enumerate()
    {
        repo.run_specere(&[
            "observe",
            "record",
            "--source",
            "specere-lint-tests",
            "--attr",
            "event_kind=test_smell_detected",
            "--attr",
            "spec_id=auth",
            "--attr",
            &format!("test_fn=tests::smell_{i}"),
            "--attr",
            &format!("smell_kind={smell}"),
            "--attr",
            "severity=info",
        ])
        .output()
        .expect("spawn");
    }
    repo.run_specere(&[
        "observe",
        "record",
        "--source",
        "test",
        "--attr",
        "event_kind=test_outcome",
        "--attr",
        "spec_id=billing",
        "--attr",
        "outcome=pass",
    ])
    .output()
    .expect("spawn");

    let out = repo
        .run_specere(&["filter", "run"])
        .output()
        .expect("spawn");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("q=0.55"),
        "expected q=0.55 from 3 smells; got:\n{stdout}"
    );
}
