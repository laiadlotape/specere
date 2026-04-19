//! FR-EQ-004 — `specere calibrate motion-from-evidence` end-to-end.
//!
//! Strategy: seed `.specere/events.jsonl` with controlled mutation +
//! test_outcome events, drive the CLI, and verify the emitted TOML
//! snippet. Three scenarios: enough history fits, too little history
//! reports insufficient, mixed specs produce mixed output.

mod common;

use common::TempRepo;

fn seed_sensor_map(repo: &TempRepo) {
    repo.write(
        ".specere/sensor-map.toml",
        r#"
schema_version = 1

[specs]
"FR-auth"    = { support = ["src/auth/"] }
"FR-billing" = { support = ["src/billing/"] }

[channels]
"#,
    );
}

fn append_event(repo: &TempRepo, spec_id: &str, kind: &str, outcome: &str, ts: &str) {
    let line = format!(
        r#"{{"ts":"{ts}","source":"fr-eq-004-test","signal":"traces","attrs":{{"event_kind":"{kind}","spec_id":"{spec_id}","outcome":"{outcome}"}}}}"#
    );
    let path = repo.abs(".specere/events.jsonl");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let combined = if existing.is_empty() {
        format!("{line}\n")
    } else {
        format!("{existing}{line}\n")
    };
    std::fs::write(&path, combined).unwrap();
}

#[test]
fn enough_history_emits_fitted_motion_snippet() {
    let repo = TempRepo::new();
    seed_sensor_map(&repo);
    // Seed 22 events for FR-auth: 18 caught + 4 missed mutations
    // → kill_rate = 18/22 ≈ 0.818; > 20 event threshold.
    for i in 0..22 {
        let outcome = if i < 18 { "caught" } else { "missed" };
        let ts = format!("2026-04-18T12:00:{:02}.000Z", i);
        append_event(&repo, "FR-auth", "mutation_result", outcome, &ts);
    }

    let out = repo
        .run_specere(&["calibrate", "motion-from-evidence"])
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "calibrate failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("[motion.\"FR-auth\"]"),
        "expected fitted motion table; got:\n{stdout}"
    );
    assert!(
        stdout.contains("t_good ="),
        "expected t_good row; got:\n{stdout}"
    );
    assert!(
        stdout.contains("[calibration.\"FR-auth\"]"),
        "expected calibration table; got:\n{stdout}"
    );
    assert!(
        stdout.contains("kill_rate="),
        "summary should include kill_rate; got:\n{stdout}"
    );
}

#[test]
fn insufficient_history_reports_per_spec() {
    let repo = TempRepo::new();
    seed_sensor_map(&repo);
    // Only 5 events for FR-auth — below the default 20-event threshold.
    for i in 0..5 {
        let ts = format!("2026-04-18T12:00:{:02}.000Z", i);
        append_event(&repo, "FR-auth", "test_outcome", "pass", &ts);
    }

    let out = repo
        .run_specere(&["calibrate", "motion-from-evidence"])
        .output()
        .expect("spawn");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("FR-auth: insufficient history"),
        "expected insufficient-history comment for FR-auth; got:\n{stdout}"
    );
    // No [motion."..."] table should be emitted for an insufficient spec.
    assert!(
        !stdout.contains("[motion.\"FR-auth\"]"),
        "insufficient spec must NOT emit a motion table; got:\n{stdout}"
    );
}

#[test]
fn custom_min_events_threshold_is_respected() {
    let repo = TempRepo::new();
    seed_sensor_map(&repo);
    // 10 events — below default 20 but above custom 5.
    for i in 0..10 {
        let ts = format!("2026-04-18T12:00:{:02}.000Z", i);
        append_event(&repo, "FR-billing", "test_outcome", "pass", &ts);
    }

    let out = repo
        .run_specere(&["calibrate", "motion-from-evidence", "--min-events", "5"])
        .output()
        .expect("spawn");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("[motion.\"FR-billing\"]"),
        "with --min-events=5, 10 events should fit; got:\n{stdout}"
    );
}

#[test]
fn empty_event_store_reports_all_insufficient() {
    let repo = TempRepo::new();
    seed_sensor_map(&repo);
    // No events at all.

    let out = repo
        .run_specere(&["calibrate", "motion-from-evidence"])
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "should succeed with no events:\nstderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("FR-auth: insufficient history")
            && stdout.contains("FR-billing: insufficient history"),
        "both specs should report insufficient history; got:\n{stdout}"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("0 spec(s) fitted") && stderr.contains("2 with insufficient history"),
        "summary should report 0/2; got:\n{stderr}"
    );
}
