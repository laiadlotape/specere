//! SQLite backend for the event store. Issue #29 / FR-P3-003 / FR-P3-004 /
//! FR-P3-005.
//!
//! Primary store at `.specere/events.sqlite`; the JSONL file remains as a
//! human-inspectable mirror. WAL mode is set at open; callers can force a
//! checkpoint via [`checkpoint_truncate`] (the `specere serve` long-runner in
//! issue #30 calls it on idle).
//!
//! Schema: one `events` table with a `signal` column. Issue #29 proposed one
//! table per signal type; we collapsed to a single table because the query
//! shape is identical across signals and a `signal` index is cheap. Can be
//! split later without changing the on-the-wire `Event` type.

use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};

use crate::event_store::{Event, QueryFilters};

/// Resolve the default SQLite path for a repo (`<repo>/.specere/events.sqlite`).
pub fn default_path(repo: &Path) -> PathBuf {
    repo.join(".specere").join("events.sqlite")
}

/// Open-or-create the SQLite store, running the one-time schema migration and
/// switching to WAL mode on success. Idempotent.
pub fn open(repo: &Path) -> anyhow::Result<Connection> {
    let path = default_path(repo);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(&path)?;
    // WAL = concurrent reads during writes + crash-safe with checkpoints.
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS events (
             ts          TEXT NOT NULL,
             source      TEXT NOT NULL,
             signal      TEXT NOT NULL,
             name        TEXT,
             feature_dir TEXT,
             attrs_json  TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_events_ts      ON events(ts);
         CREATE INDEX IF NOT EXISTS idx_events_source  ON events(source);
         CREATE INDEX IF NOT EXISTS idx_events_signal  ON events(signal);",
    )?;
    Ok(conn)
}

/// Append one event to SQLite. Timestamp and other fields must already be
/// populated by the caller (use [`crate::record`] for the default-timestamp
/// path).
pub fn append(conn: &Connection, event: &Event) -> anyhow::Result<()> {
    let attrs_json = serde_json::to_string(&event.attrs)?;
    conn.execute(
        "INSERT INTO events (ts, source, signal, name, feature_dir, attrs_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            event.ts,
            event.source,
            event.signal,
            event.name,
            event.feature_dir,
            attrs_json,
        ],
    )?;
    Ok(())
}

/// Query events with the same filter semantics as [`crate::event_store::query`].
/// Uses indexed SQL where applicable — significantly faster than tail-reading
/// the JSONL at > few-hundred-event scale.
pub fn query(conn: &Connection, filters: &QueryFilters) -> anyhow::Result<Vec<Event>> {
    // Build a WHERE clause incrementally so unused filters don't cost a scan.
    let mut sql = String::from(
        "SELECT ts, source, signal, name, feature_dir, attrs_json FROM events WHERE 1=1",
    );
    let mut params_vec: Vec<String> = Vec::new();
    if let Some(since) = &filters.since {
        sql.push_str(" AND ts >= ?");
        params_vec.push(since.clone());
    }
    if let Some(signal) = &filters.signal {
        sql.push_str(" AND signal = ?");
        params_vec.push(signal.clone());
    }
    if let Some(source) = &filters.source {
        sql.push_str(" AND source = ?");
        params_vec.push(source.clone());
    }
    sql.push_str(" ORDER BY ts ASC, rowid ASC");
    if let Some(limit) = filters.limit {
        // Most-recent-N semantics = "most recent limit by ts" → grab the tail
        // of the full order. SQL has no trivial suffix-take; we fetch all and
        // slice, matching the JSONL backend's behaviour.
        // For the > 10k case it's still fine because SQL indexes the scan.
        let _ = limit;
    }

    let mut stmt = conn.prepare(&sql)?;
    let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec
        .iter()
        .map(|s| s as &dyn rusqlite::ToSql)
        .collect();
    let rows = stmt.query_map(rusqlite::params_from_iter(params_refs), |row| {
        let ts: String = row.get(0)?;
        let source: String = row.get(1)?;
        let signal: String = row.get(2)?;
        let name: Option<String> = row.get(3)?;
        let feature_dir: Option<String> = row.get(4)?;
        let attrs_json: String = row.get(5)?;
        let attrs: std::collections::BTreeMap<String, String> =
            serde_json::from_str(&attrs_json).unwrap_or_default();
        Ok(Event {
            ts,
            source,
            signal,
            name,
            feature_dir,
            attrs,
        })
    })?;

    let mut events: Vec<Event> = rows.collect::<Result<Vec<_>, _>>()?;

    if let Some(n) = filters.limit {
        if events.len() > n {
            let skip = events.len() - n;
            events = events.into_iter().skip(skip).collect();
        }
    }
    Ok(events)
}

/// Force a WAL checkpoint that truncates the WAL file. Called by
/// `specere serve` (issue #30) on graceful shutdown; callable manually too.
pub fn checkpoint_truncate(conn: &Connection) -> anyhow::Result<()> {
    conn.pragma_update(None, "wal_checkpoint", "TRUNCATE")?;
    Ok(())
}

/// Backfill SQLite from an existing JSONL mirror — used once after the #29
/// upgrade on repos that had only JSONL from #28. Skips if SQLite already
/// has rows (defensive — never overwrites).
pub fn backfill_from_jsonl(conn: &Connection, jsonl_path: &Path) -> anyhow::Result<usize> {
    let existing: i64 = conn
        .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
        .unwrap_or(0);
    if existing > 0 || !jsonl_path.exists() {
        return Ok(0);
    }
    let text = std::fs::read_to_string(jsonl_path)?;
    let mut n = 0usize;
    let tx = conn.unchecked_transaction()?;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(e) = serde_json::from_str::<Event>(line) {
            append(&tx, &e)?;
            n += 1;
        }
    }
    tx.commit()?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_creates_db_and_wal() {
        let tmp = tempfile::TempDir::new().unwrap();
        let conn = open(tmp.path()).unwrap();
        assert!(tmp.path().join(".specere/events.sqlite").exists());
        // WAL mode — after the first write, the -wal file appears.
        append(
            &conn,
            &Event {
                ts: "2026-04-18T15:00:00Z".into(),
                source: "x".into(),
                signal: "traces".into(),
                name: None,
                feature_dir: None,
                attrs: Default::default(),
            },
        )
        .unwrap();
        assert!(tmp.path().join(".specere/events.sqlite-wal").exists());
    }

    #[test]
    fn round_trip_10k_events() {
        let tmp = tempfile::TempDir::new().unwrap();
        let conn = open(tmp.path()).unwrap();
        // Batch insert inside a single transaction — matches real-world
        // `specere serve` behaviour (batched processor in otel-config.yml).
        let tx = conn.unchecked_transaction().unwrap();
        for i in 0..10_000 {
            append(
                &tx,
                &Event {
                    ts: format!("2026-04-18T15:00:{:02}Z", i % 60),
                    source: if i % 2 == 0 { "implement" } else { "plan" }.into(),
                    signal: "traces".into(),
                    name: Some(format!("step-{i}")),
                    feature_dir: Some("specs/999".into()),
                    attrs: Default::default(),
                },
            )
            .unwrap();
        }
        tx.commit().unwrap();

        // Scan-by-source uses the index.
        let start = std::time::Instant::now();
        let got = query(
            &conn,
            &QueryFilters {
                source: Some("implement".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let elapsed = start.elapsed();
        assert_eq!(got.len(), 5000, "half of 10k events should match");
        // p50-ish check — give test-mode generous headroom; FR-P3-004 is 500ms
        // and this runs in sub-second on a laptop.
        assert!(
            elapsed < std::time::Duration::from_millis(1500),
            "query over 10k events took {elapsed:?} — above the 1500ms ceiling for a debug build"
        );
    }

    #[test]
    fn backfill_imports_jsonl() {
        let tmp = tempfile::TempDir::new().unwrap();
        // Pre-populate .specere/events.jsonl with 3 records.
        std::fs::create_dir_all(tmp.path().join(".specere")).unwrap();
        let jsonl = tmp.path().join(".specere/events.jsonl");
        let mut text = String::new();
        for i in 0..3 {
            let e = Event {
                ts: format!("2026-04-18T15:00:0{i}Z"),
                source: "seed".into(),
                signal: "traces".into(),
                name: None,
                feature_dir: None,
                attrs: Default::default(),
            };
            text.push_str(&serde_json::to_string(&e).unwrap());
            text.push('\n');
        }
        std::fs::write(&jsonl, text).unwrap();

        let conn = open(tmp.path()).unwrap();
        let n = backfill_from_jsonl(&conn, &jsonl).unwrap();
        assert_eq!(n, 3);

        let got = query(&conn, &QueryFilters::default()).unwrap();
        assert_eq!(got.len(), 3);
    }

    #[test]
    fn backfill_is_noop_when_sqlite_already_has_rows() {
        let tmp = tempfile::TempDir::new().unwrap();
        let conn = open(tmp.path()).unwrap();
        append(
            &conn,
            &Event {
                ts: "2026-04-18T15:00:00Z".into(),
                source: "existing".into(),
                signal: "traces".into(),
                name: None,
                feature_dir: None,
                attrs: Default::default(),
            },
        )
        .unwrap();
        // Pretend the JSONL has more.
        let jsonl = tmp.path().join(".specere/events.jsonl");
        std::fs::write(
            &jsonl,
            "{\"ts\":\"2020-01-01T00:00:00Z\",\"source\":\"old\",\"signal\":\"traces\",\"attrs\":{}}\n",
        )
        .unwrap();
        let n = backfill_from_jsonl(&conn, &jsonl).unwrap();
        assert_eq!(n, 0, "existing rows must block backfill");
    }

    #[test]
    fn query_with_limit_returns_most_recent() {
        let tmp = tempfile::TempDir::new().unwrap();
        let conn = open(tmp.path()).unwrap();
        for i in 0..5 {
            append(
                &conn,
                &Event {
                    ts: format!("2026-04-18T15:00:0{i}Z"),
                    source: format!("s{i}"),
                    signal: "traces".into(),
                    name: None,
                    feature_dir: None,
                    attrs: Default::default(),
                },
            )
            .unwrap();
        }
        let got = query(
            &conn,
            &QueryFilters {
                limit: Some(2),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].source, "s3");
        assert_eq!(got[1].source, "s4");
    }
}
