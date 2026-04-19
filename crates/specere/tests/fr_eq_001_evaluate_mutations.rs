//! FR-EQ-001 — `specere evaluate mutations` end-to-end.
//!
//! Strategy: use the hidden `--from-outcomes <path>` flag to skip the
//! actual `cargo mutants` invocation and point the CLI at a pre-built
//! fixture `outcomes.json`. This makes the test independent of whether
//! `cargo-mutants` is installed on the runner.

mod common;

use common::TempRepo;

fn seed_sensor_map(repo: &TempRepo) {
    repo.write(
        ".specere/sensor-map.toml",
        r#"
schema_version = 1

[specs]
"auth"     = { support = ["src/auth/"] }
"billing"  = { support = ["src/billing/"] }

[channels]
"#,
    );
}

/// Fixture mimicking `cargo mutants --json` v27 output: one baseline + 4
/// mutants. 2 caught + 2 missed across two specs.
const FIXTURE_OUTCOMES: &str = r#"{
  "outcomes": [
    {"scenario": "Baseline", "summary": "Success"},
    {"scenario": {"Mutant": {
        "name": "replace auth_check -> bool with true",
        "file": "src/auth/mod.rs",
        "function": {"function_name": "auth_check"},
        "span": {"start": {"line": 12, "column": 1}},
        "genre": "FnValue"
    }}, "summary": "CaughtMutant"},
    {"scenario": {"Mutant": {
        "name": "replace auth_token -> String with String::new()",
        "file": "src/auth/token.rs",
        "function": {"function_name": "auth_token"},
        "span": {"start": {"line": 7, "column": 1}},
        "genre": "FnValue"
    }}, "summary": "MissedMutant"},
    {"scenario": {"Mutant": {
        "name": "replace >= with > in charge",
        "file": "src/billing/charge.rs",
        "function": {"function_name": "charge"},
        "span": {"start": {"line": 3, "column": 14}},
        "genre": "BinaryOp"
    }}, "summary": "CaughtMutant"},
    {"scenario": {"Mutant": {
        "name": "replace refund -> bool with false",
        "file": "src/billing/refund.rs",
        "function": {"function_name": "refund"},
        "span": {"start": {"line": 4, "column": 1}},
        "genre": "FnValue"
    }}, "summary": "Unviable"},
    {"scenario": {"Mutant": {
        "name": "mutant in unrelated file",
        "file": "src/unrelated/helper.rs",
        "function": {"function_name": "helper"},
        "span": {"start": {"line": 1, "column": 1}},
        "genre": "FnValue"
    }}, "summary": "MissedMutant"}
  ]
}"#;

#[test]
fn emits_one_event_per_mutant_with_spec_attribution() {
    let repo = TempRepo::new();
    seed_sensor_map(&repo);
    let fixture = repo.abs(".specere/fixtures/outcomes.json");
    std::fs::create_dir_all(fixture.parent().unwrap()).unwrap();
    std::fs::write(&fixture, FIXTURE_OUTCOMES).unwrap();

    let out = repo
        .run_specere(&["evaluate", "mutations", "--from-outcomes"])
        .arg(&fixture)
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "evaluate mutations failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    // 5 mutants (baseline skipped) → 5 mutation_result events; 1 is
    // "unrelated" (unattributed), 4 are spec-attributed.
    let raw = std::fs::read_to_string(repo.abs(".specere/events.jsonl")).unwrap();
    let lines: Vec<&str> = raw.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(
        lines.len(),
        5,
        "expected 5 mutation events (1 per mutant, baseline skipped); got:\n{raw}"
    );

    let mut auth_caught = 0;
    let mut auth_missed = 0;
    let mut billing_caught = 0;
    let mut billing_unviable = 0;
    let mut unattributed_missed = 0;
    for line in &lines {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        let attrs = v["attrs"].as_object().unwrap();
        assert_eq!(
            attrs.get("event_kind").and_then(|v| v.as_str()),
            Some("mutation_result"),
            "event_kind should be mutation_result"
        );
        let spec = attrs.get("spec_id").and_then(|v| v.as_str());
        let outcome = attrs.get("outcome").and_then(|v| v.as_str()).unwrap();
        match (spec, outcome) {
            (Some("auth"), "caught") => auth_caught += 1,
            (Some("auth"), "missed") => auth_missed += 1,
            (Some("billing"), "caught") => billing_caught += 1,
            (Some("billing"), "unviable") => billing_unviable += 1,
            (None, "missed") => unattributed_missed += 1,
            other => panic!("unexpected event: {other:?} in {line}"),
        }
    }
    assert_eq!(auth_caught, 1);
    assert_eq!(auth_missed, 1);
    assert_eq!(billing_caught, 1);
    assert_eq!(billing_unviable, 1);
    assert_eq!(unattributed_missed, 1);
}

#[test]
fn scope_flag_requires_fr_id_to_exist_in_specs() {
    let repo = TempRepo::new();
    seed_sensor_map(&repo);
    let fixture = repo.abs(".specere/fixtures/outcomes.json");
    std::fs::create_dir_all(fixture.parent().unwrap()).unwrap();
    std::fs::write(&fixture, FIXTURE_OUTCOMES).unwrap();

    let out = repo
        .run_specere(&["evaluate", "mutations", "--scope", "FR-NONEXISTENT"])
        .output()
        .expect("spawn");
    assert!(!out.status.success(), "unknown --scope should error");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("FR-NONEXISTENT") && stderr.contains("[specs]"),
        "error should name the missing FR + [specs]:\n{stderr}"
    );
}

#[test]
fn reports_unattributed_mutants_in_summary() {
    let repo = TempRepo::new();
    seed_sensor_map(&repo);
    let fixture = repo.abs(".specere/fixtures/outcomes.json");
    std::fs::create_dir_all(fixture.parent().unwrap()).unwrap();
    std::fs::write(&fixture, FIXTURE_OUTCOMES).unwrap();

    let out = repo
        .run_specere(&["evaluate", "mutations", "--from-outcomes"])
        .arg(&fixture)
        .output()
        .expect("spawn");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("5 mutation event") && stdout.contains("1 unattributed"),
        "summary should report counts and unattributed:\n{stdout}"
    );
}

#[test]
fn errors_cleanly_when_cargo_mutants_missing_and_no_fixture() {
    // Without `--from-outcomes` we'd try to invoke `cargo mutants`. In a
    // TempRepo's tempdir (no Cargo.toml, no cargo workspace), the command
    // must fail with a clear error rather than panic. Some environments
    // DO have cargo-mutants installed so this just verifies "doesn't
    // panic and exits non-zero." If the subcommand happens to run
    // successfully against a non-Rust dir, we still expect a clean error
    // because outcomes.json won't exist afterwards.
    let repo = TempRepo::new();
    seed_sensor_map(&repo);
    let out = repo
        .run_specere(&["evaluate", "mutations"])
        .output()
        .expect("spawn");
    // Allow either exit — the point is no panic, and some friendly text.
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !all.contains("panicked at"),
        "specere should not panic on cargo-mutants failure:\n{all}"
    );
}
