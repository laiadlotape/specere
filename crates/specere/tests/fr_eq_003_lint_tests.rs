//! FR-EQ-003 — `specere lint tests` end-to-end.
//!
//! Strategy: seed a fixture with three test files exhibiting each smell
//! (tautological-assert, no-assertion, mock-only) plus one clean test,
//! drive the CLI, then verify `test_smell_detected` events were emitted
//! with correct attribution via `.specere/sensor-map.toml`.

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

const TAUTOLOGICAL_SRC: &str = r#"
#[cfg(test)]
mod tests {
    #[test]
    fn tautological() {
        let x = 42;
        assert_eq!(x, x);
    }
}
"#;

const NO_ASSERTION_SRC: &str = r#"
#[cfg(test)]
mod tests {
    #[test]
    fn does_nothing() {
        let _x = 1 + 1;
    }
}
"#;

const MOCK_ONLY_SRC: &str = r#"
#[cfg(test)]
mod tests {
    #[test]
    fn mock_only_test() {
        let mock_svc = Mock::new();
        mock_svc.expect_foo().returning(|| 1);
        mock_svc.expect_bar().returning(|| 2);
        mock_svc.expect_baz().returning(|| 3);
    }
}
"#;

const CLEAN_SRC: &str = r#"
pub fn charge(amount: u32) -> u32 { amount + 1 }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clean() {
        assert_eq!(charge(1), 2);
    }
}
"#;

#[test]
fn emits_one_event_per_smell_with_spec_attribution() {
    let repo = TempRepo::new();
    seed_sensor_map(&repo);
    repo.write("src/auth/mod.rs", TAUTOLOGICAL_SRC);
    repo.write("src/auth/token.rs", NO_ASSERTION_SRC);
    repo.write("src/billing/charge.rs", MOCK_ONLY_SRC);
    repo.write("src/billing/refund.rs", CLEAN_SRC);

    let out = repo
        .run_specere(&["lint", "tests"])
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "lint tests failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let raw = std::fs::read_to_string(repo.abs(".specere/events.jsonl"))
        .expect("events.jsonl should exist");
    let lines: Vec<&str> = raw.lines().filter(|l| !l.trim().is_empty()).collect();
    assert!(
        lines.len() >= 3,
        "expected at least 3 test_smell_detected events; got {} in:\n{raw}",
        lines.len()
    );

    let mut auth_tautological = 0;
    let mut auth_no_assertion = 0;
    let mut billing_mock_only = 0;
    for line in &lines {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        let attrs = v["attrs"].as_object().unwrap();
        assert_eq!(
            attrs.get("event_kind").and_then(|v| v.as_str()),
            Some("test_smell_detected"),
            "event_kind should be test_smell_detected"
        );
        assert_eq!(
            attrs.get("severity").and_then(|v| v.as_str()),
            Some("info"),
            "severity should be info (advisory per v1 questionnaire)"
        );
        let spec = attrs.get("spec_id").and_then(|v| v.as_str());
        let kind = attrs.get("smell_kind").and_then(|v| v.as_str()).unwrap();
        match (spec, kind) {
            (Some("auth"), "tautological-assert") => auth_tautological += 1,
            (Some("auth"), "no-assertion") => auth_no_assertion += 1,
            (Some("billing"), "mock-only") => billing_mock_only += 1,
            _ => {}
        }
    }
    assert_eq!(auth_tautological, 1, "expected 1 tautological in auth");
    assert_eq!(auth_no_assertion, 1, "expected 1 no-assertion in auth");
    assert_eq!(billing_mock_only, 1, "expected 1 mock-only in billing");
}

#[test]
fn clean_repo_emits_no_events() {
    let repo = TempRepo::new();
    seed_sensor_map(&repo);
    repo.write("src/billing/charge.rs", CLEAN_SRC);

    let out = repo
        .run_specere(&["lint", "tests"])
        .output()
        .expect("spawn");
    assert!(out.status.success(), "lint tests should succeed on clean repo");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("0 smell"),
        "summary should report 0 smells:\n{stdout}"
    );

    let events_path = repo.abs(".specere/events.jsonl");
    if events_path.exists() {
        let raw = std::fs::read_to_string(&events_path).unwrap();
        assert!(
            raw.trim().is_empty(),
            "no events should be emitted for a clean repo, got:\n{raw}"
        );
    }
}

#[test]
fn lint_tests_always_exits_zero_advisory() {
    // Per v1 questionnaire answer: smells are INFO severity, never block.
    let repo = TempRepo::new();
    seed_sensor_map(&repo);
    repo.write("src/auth/mod.rs", TAUTOLOGICAL_SRC);

    let out = repo
        .run_specere(&["lint", "tests"])
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "lint tests must exit 0 even when smells are detected (advisory-only)"
    );
}

#[test]
fn handles_missing_sensor_map_gracefully() {
    // No `.specere/sensor-map.toml` — smells still emit, just unattributed.
    let repo = TempRepo::new();
    repo.write("src/auth/mod.rs", TAUTOLOGICAL_SRC);

    let out = repo
        .run_specere(&["lint", "tests"])
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "lint tests should not panic on missing sensor-map:\nstderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let events_path = repo.abs(".specere/events.jsonl");
    if events_path.exists() {
        let raw = std::fs::read_to_string(&events_path).unwrap();
        for line in raw.lines().filter(|l| !l.trim().is_empty()) {
            let v: serde_json::Value = serde_json::from_str(line).unwrap();
            // Without a sensor-map, events still emit but have no spec_id.
            assert!(
                v["attrs"].get("spec_id").is_none(),
                "spec_id should be absent without sensor-map: {line}"
            );
        }
    }
}
