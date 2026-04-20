//! FR-HM-050..052 — `specere harness cluster` end-to-end.
//!
//! Verifies:
//! 1. Running on a graph with coupled + disjoint edges produces clusters
//!    that reflect the underlying community structure.
//! 2. The summary table prints cluster count + modularity.
//! 3. `--emit-to-sensor-map` prints a pasteable TOML snippet.
//! 4. Running twice with the same seed yields byte-identical output.
//! 5. Friendly message when no scan has run.

mod common;

use common::TempRepo;

/// Seed a harness-graph.toml directly (bypass scan — we want specific
/// edges on specific paths for deterministic clustering).
fn seed_graph_with_edges(repo: &TempRepo) {
    let toml = r#"
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

[[nodes]]
id = "cccccccccccccccc"
path = "tests/c.rs"
category = "integration"
category_confidence = 1.0

[[nodes]]
id = "dddddddddddddddd"
path = "tests/d.rs"
category = "integration"
category_confidence = 1.0

[[cov_cooccur_edges]]
from = "aaaaaaaaaaaaaaaa"
to = "bbbbbbbbbbbbbbbb"
from_path = "tests/a.rs"
to_path = "tests/b.rs"
jaccard = 0.9
intersection_size = 9

[[cov_cooccur_edges]]
from = "cccccccccccccccc"
to = "dddddddddddddddd"
from_path = "tests/c.rs"
to_path = "tests/d.rs"
jaccard = 0.9
intersection_size = 9
"#;
    repo.write(".specere/harness-graph.toml", toml);
}

#[test]
fn cluster_groups_coupled_pairs() {
    let repo = TempRepo::new();
    seed_graph_with_edges(&repo);

    let out = repo
        .run_specere(&["harness", "cluster"])
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "cluster failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let raw = std::fs::read_to_string(repo.abs(".specere/harness-graph.toml")).unwrap();
    let val: toml::Value = toml::from_str(&raw).expect("valid TOML");
    let nodes = val["nodes"].as_array().unwrap();
    let cluster_of = |p: &str| -> Option<String> {
        nodes
            .iter()
            .find(|n| n["path"].as_str() == Some(p))
            .and_then(|n| n.get("cluster_id"))
            .and_then(|v| v.as_str())
            .map(String::from)
    };
    let a = cluster_of("tests/a.rs");
    let b = cluster_of("tests/b.rs");
    let c = cluster_of("tests/c.rs");
    let d = cluster_of("tests/d.rs");
    assert!(a.is_some() && b.is_some());
    assert_eq!(a, b, "tests/a and tests/b must share a cluster");
    assert_eq!(c, d, "tests/c and tests/d must share a cluster");
    assert_ne!(a, c, "a-b cluster must differ from c-d cluster");
}

#[test]
fn cluster_summary_prints_modularity_line() {
    let repo = TempRepo::new();
    seed_graph_with_edges(&repo);
    let out = repo.run_specere(&["harness", "cluster"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("cluster(s)"),
        "expected cluster count; got:\n{stdout}"
    );
    assert!(
        stdout.contains("modularity"),
        "expected modularity number; got:\n{stdout}"
    );
}

#[test]
fn cluster_emit_to_sensor_map_prints_snippet() {
    let repo = TempRepo::new();
    seed_graph_with_edges(&repo);
    let out = repo
        .run_specere(&["harness", "cluster", "--emit-to-sensor-map"])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("[harness_cluster]") && stdout.contains("[harness_cluster.clusters."),
        "expected snippet block; got:\n{stdout}"
    );
}

#[test]
fn cluster_deterministic_across_runs() {
    let repo = TempRepo::new();
    seed_graph_with_edges(&repo);
    let _ = repo
        .run_specere(&["harness", "cluster", "--seed", "7"])
        .output()
        .unwrap();
    let first = std::fs::read(repo.abs(".specere/harness-graph.toml")).unwrap();
    let _ = repo
        .run_specere(&["harness", "cluster", "--seed", "7"])
        .output()
        .unwrap();
    let second = std::fs::read(repo.abs(".specere/harness-graph.toml")).unwrap();
    assert_eq!(
        first, second,
        "harness-graph.toml must be byte-identical across runs with the same seed"
    );
}

#[test]
fn cluster_without_scan_prints_friendly_message() {
    let repo = TempRepo::new();
    let out = repo.run_specere(&["harness", "cluster"]).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("run `specere harness scan` first"),
        "expected guidance; got:\n{stdout}"
    );
}
