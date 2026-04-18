//! Issue #28 / FR-P3-004 partial — `specere observe record` and
//! `specere observe query` round-trip via the JSONL event store.

mod common;

use common::TempRepo;

#[test]
fn record_creates_events_jsonl_with_one_line() {
    let repo = TempRepo::new();
    let out = repo
        .run_specere(&[
            "observe",
            "record",
            "--source",
            "implement",
            "--name",
            "specere.observe.implement",
        ])
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "record failed — exit {:?}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let jsonl = std::fs::read_to_string(repo.abs(".specere/events.jsonl")).unwrap();
    assert_eq!(
        jsonl.lines().count(),
        1,
        "expected exactly one JSON line; got:\n{jsonl}"
    );
    // The single line is valid JSON.
    let parsed: serde_json::Value = serde_json::from_str(jsonl.lines().next().unwrap()).unwrap();
    assert_eq!(parsed["source"].as_str(), Some("implement"));
    assert_eq!(parsed["name"].as_str(), Some("specere.observe.implement"));
}

#[test]
fn multiple_records_append_without_interleaving() {
    let repo = TempRepo::new();
    for verb in ["specify", "clarify", "plan"] {
        assert!(repo
            .run_specere(&["observe", "record", "--source", verb])
            .output()
            .unwrap()
            .status
            .success());
    }
    let jsonl = std::fs::read_to_string(repo.abs(".specere/events.jsonl")).unwrap();
    let lines: Vec<_> = jsonl.lines().collect();
    assert_eq!(lines.len(), 3, "expected 3 appended lines");
    // Each line independently parseable — i.e., no mid-line interleaving.
    for line in &lines {
        serde_json::from_str::<serde_json::Value>(line).expect("line must parse");
    }
}

#[test]
fn query_returns_appended_events() {
    let repo = TempRepo::new();
    assert!(repo
        .run_specere(&[
            "observe",
            "record",
            "--source",
            "plan",
            "--attr",
            "gen_ai.system=claude-code",
            "--attr",
            "specere.workflow_step=plan",
        ])
        .output()
        .unwrap()
        .status
        .success());

    let out = repo
        .run_specere(&["observe", "query", "--format", "json"])
        .output()
        .expect("spawn");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Strip trailing newline from println!, then parse.
    let trimmed = stdout.trim();
    let events: Vec<serde_json::Value> = serde_json::from_str(trimmed).unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["source"].as_str(), Some("plan"));
    assert_eq!(
        events[0]["attrs"]["gen_ai.system"].as_str(),
        Some("claude-code")
    );
}

#[test]
fn query_limit_takes_most_recent() {
    let repo = TempRepo::new();
    for verb in ["specify", "clarify", "plan", "tasks", "implement"] {
        repo.run_specere(&["observe", "record", "--source", verb])
            .output()
            .unwrap();
    }
    let out = repo
        .run_specere(&["observe", "query", "--limit", "2", "--format", "json"])
        .output()
        .expect("spawn");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let events: Vec<serde_json::Value> = serde_json::from_str(stdout.trim()).expect("valid JSON");
    assert_eq!(events.len(), 2);
    // Most recent two are tasks + implement (append order).
    assert_eq!(events[0]["source"].as_str(), Some("tasks"));
    assert_eq!(events[1]["source"].as_str(), Some("implement"));
}

#[test]
fn query_since_excludes_older_events() {
    // Records default to now-ish timestamps. Inject a far-future --since filter
    // and assert the query comes back empty.
    let repo = TempRepo::new();
    repo.run_specere(&["observe", "record", "--source", "x"])
        .output()
        .unwrap();
    let out = repo
        .run_specere(&[
            "observe",
            "query",
            "--since",
            "2099-01-01T00:00:00Z",
            "--format",
            "json",
        ])
        .output()
        .expect("spawn");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let events: Vec<serde_json::Value> = serde_json::from_str(stdout.trim()).expect("valid JSON");
    assert_eq!(events.len(), 0);
}

#[test]
fn query_format_table_emits_headers() {
    let repo = TempRepo::new();
    repo.run_specere(&["observe", "record", "--source", "plan"])
        .output()
        .unwrap();
    let out = repo
        .run_specere(&["observe", "query", "--format", "table"])
        .output()
        .expect("spawn");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("ts"));
    assert!(stdout.contains("source"));
    assert!(stdout.contains("plan"));
}

#[test]
fn query_format_toml_is_parseable() {
    let repo = TempRepo::new();
    repo.run_specere(&["observe", "record", "--source", "plan"])
        .output()
        .unwrap();
    let out = repo
        .run_specere(&["observe", "query", "--format", "toml"])
        .output()
        .expect("spawn");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let _parsed: toml::Value = toml::from_str(&stdout).expect("TOML must parse");
}
