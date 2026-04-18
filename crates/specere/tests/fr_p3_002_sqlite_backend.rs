//! Issue #29 — SQLite event store as primary backend; JSONL as mirror.

mod common;

use common::TempRepo;

#[test]
fn record_writes_both_sqlite_and_jsonl() {
    let repo = TempRepo::new();
    assert!(repo
        .run_specere(&["observe", "record", "--source", "plan"])
        .output()
        .unwrap()
        .status
        .success());

    assert!(
        repo.abs(".specere/events.sqlite").exists(),
        "SQLite primary store not written"
    );
    assert!(
        repo.abs(".specere/events.jsonl").exists(),
        "JSONL mirror not written"
    );
    // Verify journal_mode=WAL survives across connections (the -wal file can
    // be auto-checkpointed on process exit so we don't assert on its presence
    // here; the WAL-file-during-write invariant is covered by the unit test
    // in `sqlite_backend::tests::open_creates_db_and_wal`).
    let conn = specere_telemetry::sqlite_backend::open(repo.path()).unwrap();
    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        mode.to_lowercase(),
        "wal",
        "journal_mode should persist as WAL"
    );
}

#[test]
fn jsonl_count_matches_sqlite_query_count_after_mirrored_writes() {
    let repo = TempRepo::new();
    for verb in ["specify", "clarify", "plan", "tasks", "implement"] {
        repo.run_specere(&["observe", "record", "--source", verb])
            .output()
            .unwrap();
    }

    // JSONL line count
    let jsonl = std::fs::read_to_string(repo.abs(".specere/events.jsonl")).unwrap();
    let jsonl_lines = jsonl.lines().count();
    assert_eq!(jsonl_lines, 5, "expected 5 mirror lines");

    // SQLite query count (via `specere observe query --format json`)
    let out = repo
        .run_specere(&["observe", "query", "--format", "json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let events: Vec<serde_json::Value> = serde_json::from_str(stdout.trim()).expect("json parse");
    assert_eq!(events.len(), 5, "SQLite should mirror JSONL count");
}

#[test]
fn query_by_source_uses_indexed_path_on_10k_events() {
    // This is the FR-P3-003 / FR-P3-004 smoke: 10k events appended, then a
    // filtered query. Not a tight p50 benchmark (CI timing is noisy) — just
    // proves the indexed read path works at scale without stalling.
    let repo = TempRepo::new();
    // Use the direct SQLite backend for the 10k writes to skip CLI overhead
    // (5k × ~1ms fork/exec would exceed CI budget). Tests the store shape,
    // not the CLI (covered in fr_p3_001 + record_writes_both_*).
    std::fs::create_dir_all(repo.abs(".specere")).unwrap();
    let conn = specere_telemetry::sqlite_backend::open(repo.path()).unwrap();
    let tx = conn.unchecked_transaction().unwrap();
    for i in 0..10_000 {
        specere_telemetry::sqlite_backend::append(
            &tx,
            &specere_telemetry::Event {
                ts: format!("2026-04-18T15:00:{:02}Z", i % 60),
                source: if i % 2 == 0 { "implement" } else { "plan" }.into(),
                signal: "traces".into(),
                name: Some(format!("step-{i}")),
                feature_dir: None,
                attrs: Default::default(),
            },
        )
        .unwrap();
    }
    tx.commit().unwrap();

    let start = std::time::Instant::now();
    let got = specere_telemetry::sqlite_backend::query(
        &conn,
        &specere_telemetry::QueryFilters {
            source: Some("implement".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let elapsed = start.elapsed();
    assert_eq!(got.len(), 5000);
    // Generous ceiling for debug-build CI. Real p50 on release is ~30-60ms.
    assert!(
        elapsed < std::time::Duration::from_millis(2000),
        "10k-event indexed query took {elapsed:?}"
    );
}

#[test]
fn backfill_from_jsonl_when_sqlite_absent() {
    // Simulate post-#28 / pre-#29 state: JSONL has entries, no SQLite yet.
    let repo = TempRepo::new();
    std::fs::create_dir_all(repo.abs(".specere")).unwrap();
    let jsonl = repo.abs(".specere/events.jsonl");
    let mut body = String::new();
    for i in 0..3 {
        body.push_str(&format!(
            "{{\"ts\":\"2026-04-18T15:00:0{i}Z\",\"source\":\"seed\",\"signal\":\"traces\",\"attrs\":{{}}}}\n"
        ));
    }
    std::fs::write(&jsonl, body).unwrap();
    // Note: no `.specere/events.sqlite` exists yet.

    // First `query` call opens SQLite + runs the backfill transparently.
    let out = repo
        .run_specere(&["observe", "query", "--format", "json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let events: Vec<serde_json::Value> = serde_json::from_str(stdout.trim()).expect("json parse");
    assert_eq!(events.len(), 3, "backfill should have imported 3 events");
    assert!(
        repo.abs(".specere/events.sqlite").exists(),
        "SQLite was not created by backfill path"
    );
}
