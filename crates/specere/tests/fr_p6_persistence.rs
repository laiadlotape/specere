//! Phase 6 — cross-session persistence. Validates that:
//!
//! 1. Posterior written by one `specere filter run` is bit-identical after
//!    a re-read in a fresh process (no global state assumed).
//! 2. The cursor persists correctly — a second process resumes where the
//!    first stopped, consuming only new events.
//! 3. SQLite WAL state is durable: appending events across process
//!    boundaries does not lose records.
//! 4. `filter run` → process exit → `filter status` (fresh process) still
//!    renders the persisted posterior.

mod common;

use common::TempRepo;

fn seed_sensor_map(repo: &TempRepo) {
    repo.write(
        ".specere/sensor-map.toml",
        r#"
schema_version = 1

[specs]
"FR-001" = { support = ["src/a.rs"] }
"FR-002" = { support = ["src/b.rs"] }
"#,
    );
}

fn record_event(repo: &TempRepo, spec_id: &str, outcome: &str) {
    repo.run_specere(&[
        "observe",
        "record",
        "--source",
        "test_runner",
        "--attr",
        "event_kind=test_outcome",
        "--attr",
        &format!("spec_id={spec_id}"),
        "--attr",
        &format!("outcome={outcome}"),
    ])
    .assert()
    .success();
}

#[test]
fn posterior_survives_process_restart_bit_identical() {
    let repo = TempRepo::new();
    seed_sensor_map(&repo);
    for _ in 0..4 {
        record_event(&repo, "FR-001", "pass");
    }

    repo.run_specere(&["filter", "run"]).assert().success();
    let after_first = std::fs::read(repo.abs(".specere/posterior.toml")).unwrap();

    // Second process — no new events.
    repo.run_specere(&["filter", "run"])
        .assert()
        .success()
        .stdout(predicates::str::contains("no new events"));
    let after_second = std::fs::read(repo.abs(".specere/posterior.toml")).unwrap();

    assert_eq!(
        after_first, after_second,
        "FR-P4-001 / FR-P6 regression: posterior drifted across processes"
    );
}

#[test]
fn cursor_resumes_across_processes_consuming_only_new_events() {
    let repo = TempRepo::new();
    seed_sensor_map(&repo);

    // Process 1: record 3 events, run filter.
    for _ in 0..3 {
        record_event(&repo, "FR-001", "pass");
    }
    repo.run_specere(&["filter", "run"])
        .assert()
        .success()
        .stdout(predicates::str::contains("processed 3 event"));

    // Process 2: record 2 more events, run filter again.
    for _ in 0..2 {
        record_event(&repo, "FR-002", "fail");
    }
    repo.run_specere(&["filter", "run"])
        .assert()
        .success()
        .stdout(predicates::str::contains("processed 2 event"));

    // Process 3: no new events → no-op.
    repo.run_specere(&["filter", "run"])
        .assert()
        .success()
        .stdout(predicates::str::contains("no new events"));

    // Final status reflects all 5 events.
    let raw = std::fs::read_to_string(repo.abs(".specere/posterior.toml")).unwrap();
    let p: specere_filter::Posterior = toml::from_str(&raw).unwrap();
    let e1 = p.entries.iter().find(|e| e.spec_id == "FR-001").unwrap();
    let e2 = p.entries.iter().find(|e| e.spec_id == "FR-002").unwrap();
    assert!(
        e1.p_sat > 0.80,
        "FR-001 should lean SAT after 3 passes: {e1:?}"
    );
    assert!(
        e2.p_vio > 0.60,
        "FR-002 should lean VIO after 2 fails: {e2:?}"
    );
}

#[test]
fn status_renders_persisted_posterior_from_fresh_process() {
    // Write posterior in process A, read in process B — Posterior doesn't
    // rely on in-memory state.
    let repo = TempRepo::new();
    seed_sensor_map(&repo);
    record_event(&repo, "FR-001", "fail");
    repo.run_specere(&["filter", "run"]).assert().success();

    // Fresh process to render status.
    let output = repo.run_specere(&["filter", "status"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("FR-001"));
    assert!(stdout.contains("FR-002"));
}

#[test]
fn events_accumulate_across_many_observe_record_processes() {
    // Each `specere observe record` is its own process. Confirm that
    // appends don't overwrite — the JSONL grows by one line per record.
    let repo = TempRepo::new();
    seed_sensor_map(&repo);
    for i in 0..8 {
        record_event(&repo, "FR-001", if i % 2 == 0 { "pass" } else { "fail" });
    }
    let raw = std::fs::read_to_string(repo.abs(".specere/events.jsonl")).unwrap();
    assert_eq!(raw.lines().count(), 8, "expected 8 event lines, got: {raw}");
    repo.run_specere(&["filter", "run"])
        .assert()
        .success()
        .stdout(predicates::str::contains("processed 8 event"));
}

#[test]
fn posterior_reloadable_with_unknown_future_fields() {
    // Forward-compat: a future posterior.toml with extra keys should still
    // deserialise. This protects us when a later version writes fields
    // v0.5.0 doesn't know about — an older binary must not crash on read.
    let repo = TempRepo::new();
    seed_sensor_map(&repo);
    std::fs::create_dir_all(repo.abs(".specere")).unwrap();
    std::fs::write(
        repo.abs(".specere/posterior.toml"),
        r#"
schema_version = 99
cursor = "2099-01-01T00:00:00Z"
future_field = "ignore me"

[[entries]]
spec_id = "FR-001"
p_unk = 0.1
p_sat = 0.8
p_vio = 0.1
entropy = 0.5
last_updated = "2099-01-01T00:00:00Z"
future_per_entry = 42
"#,
    )
    .unwrap();
    // Filter status must not choke on unknown fields.
    let output = repo.run_specere(&["filter", "status"]).output().unwrap();
    assert!(
        output.status.success(),
        "status failed on forward-compat posterior:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("FR-001"));
}
