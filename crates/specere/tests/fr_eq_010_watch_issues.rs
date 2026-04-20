//! FR-EQ-010..013 — `specere observe watch-issues` + bug_reported events
//! in the filter calibration path.

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
"#,
    );
}

const FIXTURE: &str = r#"[
  {
    "number": 101,
    "title": "login fails under load",
    "body": "trace shows `src/auth/token.rs:42` panicking on empty JWT",
    "url": "https://github.com/x/y/issues/101",
    "state": "open",
    "labels": [{"name": "critical"}],
    "created_at": "2026-04-10T10:00:00Z"
  },
  {
    "number": 102,
    "title": "typo in README",
    "body": "README.md says 'the the'",
    "url": "https://github.com/x/y/issues/102",
    "state": "open",
    "labels": [{"name": "docs"}],
    "created_at": "2026-04-15T00:00:00Z"
  },
  {
    "number": 103,
    "title": "billing off-by-one",
    "body": "src/billing/charge.rs computes wrong amount",
    "url": "https://github.com/x/y/issues/103",
    "state": "closed",
    "labels": [{"name": "bug"}],
    "created_at": "2026-04-05T00:00:00Z",
    "pull_request": {"url": "pr/1"}
  }
]"#;

#[test]
fn watch_issues_emits_one_event_per_actionable_issue() {
    let repo = TempRepo::new();
    seed_sensor_map(&repo);
    let fixture_path = repo.abs(".specere/issues-fixture.json");
    std::fs::create_dir_all(fixture_path.parent().unwrap()).unwrap();
    std::fs::write(&fixture_path, FIXTURE).unwrap();

    let out = repo
        .run_specere(&[
            "observe",
            "watch-issues",
            "--provider",
            "github",
            "--from-fixture",
            fixture_path.to_str().unwrap(),
            "--once",
        ])
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "watch-issues failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("2 bug_reported event(s) emitted"),
        "expected 2 emitted (101 + 103 — 102 is docs-labelled); got:\n{stdout}"
    );
    assert!(
        stdout.contains("1 skipped"),
        "expected 1 skipped (docs label); got:\n{stdout}"
    );

    let events_raw = std::fs::read_to_string(repo.abs(".specere/events.jsonl")).unwrap();
    let lines: Vec<_> = events_raw.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines.len(), 2, "two events: {events_raw}");

    // Find the FR-auth event (critical, open, triaged from body path).
    let auth = lines
        .iter()
        .find(|l| l.contains("FR-auth"))
        .expect("FR-auth event present");
    let v: serde_json::Value = serde_json::from_str(auth).unwrap();
    assert_eq!(v["attrs"]["event_kind"].as_str(), Some("bug_reported"));
    assert_eq!(v["attrs"]["severity"].as_str(), Some("critical"));
    assert_eq!(v["attrs"]["state"].as_str(), Some("open"));
    assert_eq!(v["attrs"]["issue_number"].as_str(), Some("101"));
}

#[test]
fn bug_reported_events_reduce_filter_calibration_quality() {
    // A spec receives 10 mutation_result caught events (kill_rate = 1.0)
    // AND one critical bug_reported event. The per-spec quality should
    // drop below 1.0 because the bug signal compresses smell_penalty.
    let repo = TempRepo::new();
    seed_sensor_map(&repo);
    // 10 caught mutations → kill_rate 1.0.
    let mut events = String::new();
    for i in 0..10 {
        events.push_str(&format!(
            r#"{{"ts":"2026-04-20T10:00:{:02}.000Z","source":"t","signal":"traces","attrs":{{"event_kind":"mutation_result","spec_id":"FR-auth","outcome":"caught"}}}}{}"#,
            i, "\n"
        ));
    }
    // One critical bug, 5 days old.
    events.push_str(
        r#"{"ts":"2026-04-20T11:00:00.000Z","source":"t","signal":"traces","attrs":{"event_kind":"bug_reported","spec_id":"FR-auth","severity":"critical","state":"open","age_days":"5"}}
"#,
    );
    // At least one test_outcome so filter run processes something.
    events.push_str(
        r#"{"ts":"2026-04-20T12:00:00.000Z","source":"t","signal":"traces","attrs":{"event_kind":"test_outcome","spec_id":"FR-auth","outcome":"pass"}}
"#,
    );
    std::fs::write(repo.abs(".specere/events.jsonl"), events).unwrap();

    let out = repo
        .run_specere(&["filter", "run"])
        .output()
        .expect("spawn");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    // The calibration-summary block prints per-spec `q=…` when any spec
    // deviates from prototype. With a bug signal, FR-auth's q MUST be
    // < 1.0.
    assert!(
        stdout.contains("FR-auth") && stdout.contains("q=0."),
        "expected FR-auth's quality below 1.0 due to bug_reported event; got:\n{stdout}"
    );
}

#[test]
fn watch_issues_handles_empty_fixture() {
    let repo = TempRepo::new();
    seed_sensor_map(&repo);
    let fixture = repo.abs(".specere/empty.json");
    std::fs::create_dir_all(fixture.parent().unwrap()).unwrap();
    std::fs::write(&fixture, "[]").unwrap();
    let out = repo
        .run_specere(&[
            "observe",
            "watch-issues",
            "--provider",
            "github",
            "--from-fixture",
            fixture.to_str().unwrap(),
            "--once",
        ])
        .output()
        .expect("spawn");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("0 bug_reported event(s)"));
}
