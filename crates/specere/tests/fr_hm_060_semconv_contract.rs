//! FR-HM-060..061 — `specere.harness.*` OpenTelemetry Semantic Convention
//! contract tests.
//!
//! Every harness-manager verb emits a `harness_*_completed` event to
//! `.specere/events.jsonl` after running. This suite spawns each verb in
//! a TempRepo, then validates the emitted events against the attribute
//! schema defined in `docs/otel-specere-semconv.md` §5.3.

mod common;

use common::TempRepo;

fn seed_scan_ready(repo: &TempRepo) {
    repo.write(
        "Cargo.toml",
        "[package]\nname = \"demo\"\nversion=\"0.1\"\n",
    );
    repo.write("src/lib.rs", "pub fn f(){}");
    repo.write("tests/it.rs", "#[test] fn a(){}");
}

fn read_events(repo: &TempRepo) -> Vec<serde_json::Value> {
    let raw = std::fs::read_to_string(repo.abs(".specere/events.jsonl")).unwrap_or_default();
    raw.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("valid JSON event"))
        .collect()
}

/// Every event MUST carry schema_version + cli_version + verb.
fn assert_common_attrs(event: &serde_json::Value, expected_kind: &str, expected_verb: &str) {
    let attrs = event["attrs"]
        .as_object()
        .expect("every event has an attrs object");
    assert_eq!(
        attrs.get("event_kind").and_then(|v| v.as_str()),
        Some(expected_kind),
        "event_kind mismatch: {event:?}"
    );
    assert_eq!(
        attrs.get("specere.schema_version").and_then(|v| v.as_str()),
        Some("1"),
        "specere.schema_version must be 1"
    );
    assert!(
        attrs.get("specere.cli_version").is_some(),
        "specere.cli_version missing"
    );
    assert_eq!(
        attrs.get("specere.harness.verb").and_then(|v| v.as_str()),
        Some(expected_verb),
        "specere.harness.verb mismatch"
    );
}

#[test]
fn scan_emits_harness_scan_completed_event() {
    let repo = TempRepo::new();
    seed_scan_ready(&repo);
    let _ = repo.run_specere(&["harness", "scan"]).output().unwrap();

    let events = read_events(&repo);
    let scan = events
        .iter()
        .find(|e| e["attrs"]["event_kind"].as_str() == Some("harness_scan_completed"))
        .expect("harness_scan_completed event present");
    assert_common_attrs(scan, "harness_scan_completed", "scan");
    assert!(
        scan["attrs"]["specere.harness.n_files"].as_str().is_some(),
        "n_files attr missing"
    );
}

#[test]
fn provenance_emits_completion_event() {
    let repo = TempRepo::new();
    seed_scan_ready(&repo);
    let _ = repo.run_specere(&["harness", "scan"]).output().unwrap();
    let _ = repo
        .run_specere(&["harness", "provenance"])
        .output()
        .unwrap();

    let events = read_events(&repo);
    let prov = events
        .iter()
        .find(|e| e["attrs"]["event_kind"].as_str() == Some("harness_provenance_completed"))
        .expect("harness_provenance_completed event present");
    assert_common_attrs(prov, "harness_provenance_completed", "provenance");
    for k in [
        "specere.harness.n_files",
        "specere.harness.n_files_enriched",
        "specere.harness.n_span_attributed",
        "specere.harness.n_git_attributed",
    ] {
        assert!(
            prov["attrs"][k].as_str().is_some(),
            "{k} attr missing: {prov:?}"
        );
    }
}

#[test]
fn cluster_emits_completion_event_with_modularity_and_seed() {
    let repo = TempRepo::new();
    // Seed a graph directly — cluster doesn't need scan output when the
    // graph already exists.
    repo.write(
        ".specere/harness-graph.toml",
        r#"
schema_version = 1

[[nodes]]
id = "aaaaaaaaaaaaaaaa"
path = "tests/a.rs"
category = "integration"
category_confidence = 1.0

[[nodes]]
id = "bbbbbbbbbbbbbbbb"
path = "tests/b.rs"
category = "integration"
category_confidence = 1.0

[[cov_cooccur_edges]]
from = "aaaaaaaaaaaaaaaa"
to = "bbbbbbbbbbbbbbbb"
from_path = "tests/a.rs"
to_path = "tests/b.rs"
jaccard = 0.9
intersection_size = 5
"#,
    );
    let _ = repo
        .run_specere(&["harness", "cluster", "--seed", "7"])
        .output()
        .unwrap();
    let events = read_events(&repo);
    let c = events
        .iter()
        .find(|e| e["attrs"]["event_kind"].as_str() == Some("harness_cluster_completed"))
        .expect("harness_cluster_completed event present");
    assert_common_attrs(c, "harness_cluster_completed", "cluster");
    // Required §5.3 attrs for cluster events.
    for k in [
        "specere.harness.n_clusters",
        "specere.harness.total_modularity",
        "specere.harness.cluster_seed",
    ] {
        assert!(c["attrs"][k].as_str().is_some(), "{k} missing: {c:?}");
    }
    assert_eq!(
        c["attrs"]["specere.harness.cluster_seed"].as_str(),
        Some("7"),
        "cluster_seed must match --seed"
    );
}

#[test]
fn coverage_emits_completion_event() {
    let repo = TempRepo::new();
    seed_scan_ready(&repo);
    let _ = repo.run_specere(&["harness", "scan"]).output().unwrap();

    let lcov_dir = repo.abs(".specere/lcov");
    std::fs::create_dir_all(&lcov_dir).unwrap();
    std::fs::write(
        lcov_dir.join("tests__it.rs.lcov"),
        "SF:src/lib.rs\nDA:1,1\nend_of_record\n",
    )
    .unwrap();

    let _ = repo
        .run_specere(&[
            "harness",
            "coverage",
            "--from-lcov-dir",
            lcov_dir.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    let events = read_events(&repo);
    let c = events
        .iter()
        .find(|e| e["attrs"]["event_kind"].as_str() == Some("harness_coverage_completed"))
        .expect("harness_coverage_completed event present");
    assert_common_attrs(c, "harness_coverage_completed", "coverage");
    assert!(c["attrs"]["specere.harness.jaccard_threshold"]
        .as_str()
        .is_some());
}

#[test]
fn flaky_emits_completion_event_with_insufficient_history_flag() {
    let repo = TempRepo::new();
    seed_scan_ready(&repo);
    let _ = repo.run_specere(&["harness", "scan"]).output().unwrap();

    // Write a runs fixture with only 3 runs — below the default 50 threshold.
    repo.write(
        ".specere/runs.jsonl",
        r#"{"run_id":"r1","outcomes":{"tests/it.rs":"pass"}}
{"run_id":"r2","outcomes":{"tests/it.rs":"fail"}}
{"run_id":"r3","outcomes":{"tests/it.rs":"pass"}}
"#,
    );
    let runs_path = repo.abs(".specere/runs.jsonl");
    let _ = repo
        .run_specere(&[
            "harness",
            "flaky",
            "--from-runs",
            runs_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    let events = read_events(&repo);
    let f = events
        .iter()
        .find(|e| e["attrs"]["event_kind"].as_str() == Some("harness_flaky_completed"))
        .expect("harness_flaky_completed event present");
    assert_common_attrs(f, "harness_flaky_completed", "flaky");
    assert_eq!(
        f["attrs"]["specere.harness.insufficient_history"].as_str(),
        Some("true"),
        "insufficient_history flag must be set when n_runs < min_runs"
    );
}
