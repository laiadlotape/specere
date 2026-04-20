//! FR-HM-070..072 — `specere harness tui` smoke test via `--headless-frames`.
//!
//! The TUI is interactive and can't run against a real terminal in CI,
//! but the `--headless-frames N` path paints to a ratatui TestBackend
//! and exits — confirming the widget tree builds end-to-end on all
//! platforms.

mod common;

use common::TempRepo;

#[test]
fn tui_headless_smoke_runs_to_completion() {
    let repo = TempRepo::new();
    // Seed a minimal harness graph so the TUI has data to render.
    repo.write(
        ".specere/harness-graph.toml",
        r#"
schema_version = 1

[[nodes]]
id = "aaaaaaaaaaaaaaaa"
path = "tests/a.rs"
category = "integration"
category_confidence = 1.0
test_names = ["test_a"]

[[nodes]]
id = "bbbbbbbbbbbbbbbb"
path = "tests/b.rs"
category = "unit"
category_confidence = 0.9
"#,
    );
    let out = repo
        .run_specere(&["harness", "tui", "--headless-frames", "1"])
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "tui headless smoke failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn tui_without_scan_prints_friendly_message() {
    let repo = TempRepo::new();
    // No harness-graph.toml — should not try to open the terminal.
    let out = repo
        .run_specere(&["harness", "tui", "--headless-frames", "1"])
        .output()
        .expect("spawn");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("run `specere harness scan` first"),
        "expected guidance; got:\n{stdout}"
    );
}

#[test]
fn tui_headless_smoke_survives_with_events_jsonl() {
    let repo = TempRepo::new();
    repo.write(
        ".specere/harness-graph.toml",
        r#"
schema_version = 1

[[nodes]]
id = "aaaaaaaaaaaaaaaa"
path = "tests/a.rs"
category = "integration"
category_confidence = 1.0
"#,
    );
    // A small events.jsonl for the timeline footer.
    repo.write(
        ".specere/events.jsonl",
        r#"{"ts":"2026-04-20T10:00:00Z","source":"x","signal":"traces","attrs":{"event_kind":"harness_scan_completed"}}
"#,
    );
    let out = repo
        .run_specere(&["harness", "tui", "--headless-frames", "2"])
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "tui failed on happy path:\nstderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
