//! FR-EQ-006 — `specere doctor --suspicious` end-to-end.
//!
//! Seeds a posterior + evidence events, runs the CLI, and verifies the
//! review-queue.md entry is appended (or absent) based on the p_sat ×
//! quality thresholds.

mod common;

use common::TempRepo;

fn seed_sensor_map(repo: &TempRepo) {
    repo.write(
        ".specere/sensor-map.toml",
        r#"
schema_version = 1

[specs]
"FR-shaky"    = { support = ["src/shaky/"] }
"FR-solid"    = { support = ["src/solid/"] }

[channels]
"#,
    );
}

fn seed_posterior(repo: &TempRepo) {
    repo.write(
        ".specere/posterior.toml",
        r#"
cursor = "2026-04-19T12:00:00Z"
schema_version = 1

[[entries]]
spec_id = "FR-shaky"
p_unk = 0.02
p_sat = 0.97
p_vio = 0.01
entropy = 0.2
last_updated = "2026-04-19T12:00:00Z"

[[entries]]
spec_id = "FR-solid"
p_unk = 0.05
p_sat = 0.80
p_vio = 0.15
entropy = 0.7
last_updated = "2026-04-19T12:00:00Z"
"#,
    );
}

/// Seed N mutation_result events for a spec with a given kill rate.
fn seed_mutations(repo: &TempRepo, spec_id: &str, caught: usize, missed: usize) {
    let mut lines = String::new();
    let existing = std::fs::read_to_string(repo.abs(".specere/events.jsonl")).unwrap_or_default();
    lines.push_str(&existing);
    for i in 0..caught {
        lines.push_str(&format!(
            r#"{{"ts":"2026-04-19T12:00:{:02}.000Z","source":"t","signal":"traces","attrs":{{"event_kind":"mutation_result","spec_id":"{spec_id}","outcome":"caught"}}}}{}"#,
            i,
            "\n"
        ));
    }
    for i in 0..missed {
        lines.push_str(&format!(
            r#"{{"ts":"2026-04-19T12:01:{:02}.000Z","source":"t","signal":"traces","attrs":{{"event_kind":"mutation_result","spec_id":"{spec_id}","outcome":"missed"}}}}{}"#,
            i,
            "\n"
        ));
    }
    std::fs::write(repo.abs(".specere/events.jsonl"), lines).unwrap();
}

#[test]
fn flags_suspicious_spec_high_psat_low_quality() {
    let repo = TempRepo::new();
    seed_sensor_map(&repo);
    seed_posterior(&repo);
    // FR-shaky: kill rate = 2/10 = 0.2 → quality 0.2 → below 0.5.
    seed_mutations(&repo, "FR-shaky", 2, 8);
    // FR-solid: kill rate = 9/10 = 0.9 → quality 0.9 → above 0.5.
    seed_mutations(&repo, "FR-solid", 9, 1);

    let out = repo
        .run_specere(&["doctor", "--suspicious"])
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "doctor --suspicious failed:\nstderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("flagged 1 spec"),
        "should flag exactly 1 spec; got:\n{stdout}"
    );

    let queue = std::fs::read_to_string(repo.abs(".specere/review-queue.md"))
        .expect("review-queue.md should be written");
    assert!(
        queue.contains("Suspicious high-confidence SAT — FR-shaky"),
        "FR-shaky should be flagged; got:\n{queue}"
    );
    assert!(
        !queue.contains("Suspicious high-confidence SAT — FR-solid"),
        "FR-solid should NOT be flagged; got:\n{queue}"
    );
    assert!(
        queue.contains("p_sat = 0.97"),
        "entry should quote p_sat; got:\n{queue}"
    );
    assert!(
        queue.contains("mutation kill 0.20"),
        "entry should quote kill rate; got:\n{queue}"
    );
}

#[test]
fn no_flag_when_all_specs_well_calibrated() {
    let repo = TempRepo::new();
    seed_sensor_map(&repo);
    seed_posterior(&repo);
    // Both specs have high kill rate → quality ≈ 1.0 → neither suspicious.
    seed_mutations(&repo, "FR-shaky", 9, 1);
    seed_mutations(&repo, "FR-solid", 9, 1);

    let out = repo
        .run_specere(&["doctor", "--suspicious"])
        .output()
        .expect("spawn");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("no suspicious specs"),
        "expected `no suspicious specs`; got:\n{stdout}"
    );
    // review-queue.md must not have been created.
    assert!(
        !repo.abs(".specere/review-queue.md").exists(),
        "review-queue.md should not be written when no specs flagged"
    );
}

#[test]
fn empty_posterior_reports_no_flag() {
    let repo = TempRepo::new();
    seed_sensor_map(&repo);
    // No posterior file at all.

    let out = repo
        .run_specere(&["doctor", "--suspicious"])
        .output()
        .expect("spawn");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("posterior is empty"),
        "expected friendly message; got:\n{stdout}"
    );
}

#[test]
fn configurable_thresholds_via_sensor_map() {
    let repo = TempRepo::new();
    // Override thresholds — make FR-solid (p_sat=0.80, kill=0.9 → q=0.9) a
    // suspicious candidate by dropping quality_max to 0.95 and p_sat_min to
    // 0.70. FR-shaky is also flagged. Both should end up in review queue.
    repo.write(
        ".specere/sensor-map.toml",
        r#"
schema_version = 1

[specs]
"FR-shaky" = { support = ["src/shaky/"] }
"FR-solid" = { support = ["src/solid/"] }

[review]
suspicious_p_sat_min = 0.70
suspicious_quality_max = 0.95

[channels]
"#,
    );
    seed_posterior(&repo);
    seed_mutations(&repo, "FR-shaky", 2, 8);
    seed_mutations(&repo, "FR-solid", 9, 1);

    let out = repo
        .run_specere(&["doctor", "--suspicious"])
        .output()
        .expect("spawn");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("flagged 2 spec"),
        "expected 2 flagged specs with relaxed thresholds; got:\n{stdout}"
    );
}
