//! FR-P4-005 — filter engine must sustain ≥ 1000 events/s on laptop hardware.
//!
//! This test is `#[ignore]`-gated so CI doesn't pay the benchmarking cost
//! every run. Invoke locally with:
//!
//!     cargo test --test fr_p4_005_throughput -- --ignored
//!
//! Generates a 10 000-event JSONL stream alternating `test_outcome` and
//! `files_touched` events, runs `specere filter run`, and asserts the
//! observed throughput is ≥ 1000 events/s (i.e. 10k events under 10 s).
//!
//! The generated events bypass `specere observe record` (which would
//! spawn the binary 10 000 times) and are written directly to
//! `.specere/events.jsonl` in the fixture format the driver expects.

mod common;

use std::fs::OpenOptions;
use std::io::Write;
use std::time::Instant;

use common::TempRepo;

const N_EVENTS: usize = 10_000;

#[test]
#[ignore]
fn filter_run_sustains_at_least_1000_events_per_second() {
    let repo = TempRepo::new();

    // Sensor-map with 10 specs — enough to make per-spec marginal work
    // non-trivial without dominating total runtime.
    let mut sm = String::from("schema_version = 1\n\n[specs]\n");
    for n in 1..=10 {
        sm.push_str(&format!(
            "\"FR-{n:03}\" = {{ support = [\"src/spec_{n}.rs\"] }}\n"
        ));
    }
    repo.write(".specere/sensor-map.toml", &sm);

    // Pre-seed `.specere/events.jsonl` directly in the JSONL format the
    // `event_store::query` path reads. Alternating files_touched +
    // test_outcome events keeps both hot paths (predict, update_test) hit.
    let jsonl_path = repo.abs(".specere/events.jsonl");
    std::fs::create_dir_all(jsonl_path.parent().unwrap()).unwrap();
    let mut jsonl = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&jsonl_path)
        .unwrap();

    // Use strictly-monotonic fake timestamps so the cursor-gated replay
    // consumes every event. RFC3339 lexicographic comparison on same-TZ
    // strings is equivalent to numeric time comparison.
    for i in 0..N_EVENTS {
        let ts = format!(
            "2026-04-18T00:00:{:02}.{:06}Z",
            i / 1_000_000,
            i % 1_000_000
        );
        let spec_n = (i % 10) + 1;
        let line = if i % 2 == 0 {
            format!(
                r#"{{"ts":"{ts}","source":"bench","signal":"traces","attrs":{{"event_kind":"files_touched","paths":"src/spec_{spec_n}.rs"}}}}"#
            )
        } else {
            let outcome = if i % 4 == 1 { "pass" } else { "fail" };
            format!(
                r#"{{"ts":"{ts}","source":"bench","signal":"traces","attrs":{{"event_kind":"test_outcome","spec_id":"FR-{spec_n:03}","outcome":"{outcome}"}}}}"#
            )
        };
        writeln!(jsonl, "{line}").unwrap();
    }
    jsonl.flush().unwrap();
    drop(jsonl);

    // Confirm the JSONL is the only event source — SQLite is backfilled
    // on first query.
    let start = Instant::now();
    let output = repo
        .run_specere(&["filter", "run"])
        .output()
        .expect("filter run failed to spawn");
    let elapsed = start.elapsed();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "filter run exited non-zero.\nstdout: {stdout}\nstderr: {stderr}"
    );

    let secs = elapsed.as_secs_f64();
    let events_per_sec = N_EVENTS as f64 / secs;
    eprintln!(
        "FR-P4-005 throughput: {N_EVENTS} events in {secs:.3}s = {events_per_sec:.0} events/s"
    );

    assert!(
        events_per_sec >= 1000.0,
        "FR-P4-005 breach: {events_per_sec:.0} events/s < 1000 (elapsed {secs:.3}s)"
    );
}
