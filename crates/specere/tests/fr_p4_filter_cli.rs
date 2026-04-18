//! Issue #43 / FR-P4-001, 003, 004. End-to-end tests for
//! `specere filter run` and `specere filter status` driven through the
//! binary.
//!
//! Setup per test:
//!   1. TempRepo with `.specere/` + a hand-authored `sensor-map.toml`
//!      carrying a `[specs]` section.
//!   2. `.specere/events.jsonl` seeded with synthetic events the driver
//!      can consume (event_kind=test_outcome, spec_id, outcome).
//!   3. SQLite mirror bootstrapped via `specere observe record` so the
//!      query path works (the CLI reads events from the JSONL store).
//!
//! FR-P4-002 (<2 pp Python parity) and FR-P4-005 (≥1000 events/s) are
//! covered in follow-up work — this file scopes to the three FRs #43
//! is committed to close.

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
fn filter_run_emits_posterior_with_expected_structure() {
    let repo = TempRepo::new();
    seed_sensor_map(&repo);
    for _ in 0..3 {
        record_event(&repo, "FR-001", "pass");
    }
    record_event(&repo, "FR-002", "fail");
    record_event(&repo, "FR-002", "fail");

    repo.run_specere(&["filter", "run"])
        .assert()
        .success()
        .stdout(predicates::str::contains("processed"));

    let posterior = std::fs::read_to_string(repo.abs(".specere/posterior.toml")).unwrap();
    assert!(posterior.contains("FR-001"));
    assert!(posterior.contains("FR-002"));
    assert!(posterior.contains("schema_version = 1"));
    assert!(posterior.contains("cursor"));

    // Parse + sanity-check qualitative behaviour: FR-001 (three passes)
    // should hold more SAT mass than FR-002 (two fails) does, and
    // conversely for VIO.
    let p: specere_filter::Posterior = toml::from_str(&posterior).unwrap();
    let e1 = p.entries.iter().find(|e| e.spec_id == "FR-001").unwrap();
    let e2 = p.entries.iter().find(|e| e.spec_id == "FR-002").unwrap();
    assert!(
        e1.p_sat > e2.p_sat,
        "FR-001 SAT mass did not exceed FR-002's"
    );
    assert!(
        e2.p_vio > e1.p_vio,
        "FR-002 VIO mass did not exceed FR-001's"
    );
}

#[test]
fn filter_run_is_idempotent_under_no_new_events() {
    // FR-P4-001: a second `run` with no new events since the cursor must
    // leave the posterior byte-identical.
    let repo = TempRepo::new();
    seed_sensor_map(&repo);
    for _ in 0..2 {
        record_event(&repo, "FR-001", "pass");
    }

    repo.run_specere(&["filter", "run"]).assert().success();
    let first = std::fs::read(repo.abs(".specere/posterior.toml")).unwrap();

    repo.run_specere(&["filter", "run"])
        .assert()
        .success()
        .stdout(predicates::str::contains("no new events"));
    let second = std::fs::read(repo.abs(".specere/posterior.toml")).unwrap();

    assert_eq!(first, second, "posterior byte-drifted on idempotent re-run");
}

#[test]
fn filter_status_sorts_by_entropy_descending_by_default() {
    // FR-P4-003. Seed events so FR-001 is highly concentrated (low entropy)
    // and FR-002 stays near-uniform (high entropy). Status default should
    // print FR-002 before FR-001.
    let repo = TempRepo::new();
    seed_sensor_map(&repo);
    for _ in 0..6 {
        record_event(&repo, "FR-001", "pass");
    }
    record_event(&repo, "FR-002", "pass");
    record_event(&repo, "FR-002", "fail");

    repo.run_specere(&["filter", "run"]).assert().success();

    let output = String::from_utf8(
        repo.run_specere(&["filter", "status"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();

    let fr1_pos = output.find("FR-001").expect("FR-001 missing from status");
    let fr2_pos = output.find("FR-002").expect("FR-002 missing from status");
    assert!(
        fr2_pos < fr1_pos,
        "default sort did not place high-entropy FR-002 before low-entropy FR-001\n--- output ---\n{output}"
    );
}

#[test]
fn filter_status_respects_sort_override() {
    let repo = TempRepo::new();
    seed_sensor_map(&repo);
    record_event(&repo, "FR-001", "fail");
    record_event(&repo, "FR-002", "pass");
    repo.run_specere(&["filter", "run"]).assert().success();

    // `p_sat,desc` ⇒ FR-002 (more SAT) before FR-001.
    let output = String::from_utf8(
        repo.run_specere(&["filter", "status", "--sort", "p_sat,desc"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    let fr1_pos = output.find("FR-001").unwrap();
    let fr2_pos = output.find("FR-002").unwrap();
    assert!(fr2_pos < fr1_pos, "p_sat,desc sort order wrong\n{output}");
}

#[test]
fn filter_status_emits_json_when_requested() {
    let repo = TempRepo::new();
    seed_sensor_map(&repo);
    record_event(&repo, "FR-001", "pass");
    repo.run_specere(&["filter", "run"]).assert().success();

    let out = repo
        .run_specere(&["filter", "status", "--format", "json"])
        .output()
        .unwrap();
    let s = String::from_utf8(out.stdout).unwrap();
    let parsed: serde_json::Value =
        serde_json::from_str(&s).expect("status --format json must be valid JSON");
    assert!(parsed.is_array(), "expected JSON array of entries");
}

#[test]
fn filter_status_on_empty_repo_prints_hint() {
    let repo = TempRepo::new();
    seed_sensor_map(&repo);
    let out = String::from_utf8(
        repo.run_specere(&["filter", "status"])
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    assert!(out.contains("no posterior yet"));
}

#[test]
fn filter_run_is_deterministic_across_invocations() {
    // FR-P4-004: same inputs + same seed ⇒ byte-identical posterior. We
    // run twice into two repos and compare. The `last_updated` field is
    // bound to the last-event ts (not wall clock), so identical event
    // streams produce identical posteriors.
    let build = || {
        let r = TempRepo::new();
        seed_sensor_map(&r);
        record_event(&r, "FR-001", "pass");
        record_event(&r, "FR-002", "fail");
        r.run_specere(&["filter", "run"]).assert().success();
        std::fs::read_to_string(r.abs(".specere/posterior.toml")).unwrap()
    };
    // The two repos will have different event ts values since `observe
    // record` stamps with `now_rfc3339()`. So we only compare the
    // numeric belief fields, not the cursor/last_updated fields.
    let a = build();
    let b = build();
    let pa: specere_filter::Posterior = toml::from_str(&a).unwrap();
    let pb: specere_filter::Posterior = toml::from_str(&b).unwrap();
    assert_eq!(pa.entries.len(), pb.entries.len());
    for (ea, eb) in pa.entries.iter().zip(pb.entries.iter()) {
        assert_eq!(ea.spec_id, eb.spec_id);
        assert!((ea.p_unk - eb.p_unk).abs() < 1e-12);
        assert!((ea.p_sat - eb.p_sat).abs() < 1e-12);
        assert!((ea.p_vio - eb.p_vio).abs() < 1e-12);
        assert!((ea.entropy - eb.entropy).abs() < 1e-12);
    }
}
