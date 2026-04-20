//! FR-HM-030..033 — `specere harness coverage` end-to-end.
//!
//! Drives the CLI via the hidden `--from-lcov-dir` flag (mirrors the
//! FR-EQ-001 `--from-outcomes` pattern). Tests here do NOT require
//! `cargo-llvm-cov` to be installed — LCOV fixtures on disk are enough.

mod common;

use common::TempRepo;

const LCOV_A: &str = "\
SF:src/lib.rs
DA:1,1
DA:2,1
DA:3,1
end_of_record
";

const LCOV_B_OVERLAP: &str = "\
SF:src/lib.rs
DA:2,1
DA:3,1
DA:4,1
end_of_record
";

const LCOV_DISJOINT: &str = "\
SF:src/other.rs
DA:10,1
DA:11,1
end_of_record
";

fn seed_scan(repo: &TempRepo) {
    repo.write(
        "Cargo.toml",
        "[package]\nname = \"demo\"\nversion=\"0.1\"\n",
    );
    repo.write("src/lib.rs", "pub fn f(){}");
    repo.write("tests/a.rs", "#[test] fn a(){}");
    repo.write("tests/b.rs", "#[test] fn b(){}");
    repo.write("tests/c.rs", "#[test] fn c(){}");
    let out = repo
        .run_specere(&["harness", "scan"])
        .output()
        .expect("spawn");
    assert!(out.status.success(), "seed scan failed");
}

fn seed_lcov_dir(repo: &TempRepo) -> std::path::PathBuf {
    let dir = repo.abs(".specere/lcov");
    std::fs::create_dir_all(&dir).unwrap();
    // Filename convention: path-with-slashes-as-`__`. So `tests/a.rs`
    // becomes `tests__a.rs.lcov` — the `.rs` is preserved because we
    // only strip `.lcov` as the file extension.
    std::fs::write(dir.join("tests__a.rs.lcov"), LCOV_A).unwrap();
    std::fs::write(dir.join("tests__b.rs.lcov"), LCOV_B_OVERLAP).unwrap();
    std::fs::write(dir.join("tests__c.rs.lcov"), LCOV_DISJOINT).unwrap();
    dir
}

#[test]
fn coverage_emits_cov_cooccur_edge_for_overlapping_tests() {
    let repo = TempRepo::new();
    seed_scan(&repo);
    let lcov_dir = seed_lcov_dir(&repo);

    let out = repo
        .run_specere(&[
            "harness",
            "coverage",
            "--from-lcov-dir",
            lcov_dir.to_str().unwrap(),
            "--threshold",
            "0.1",
        ])
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "coverage failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let raw = std::fs::read_to_string(repo.abs(".specere/harness-graph.toml")).unwrap();
    let val: toml::Value = toml::from_str(&raw).expect("valid TOML");
    let edges = val
        .get("cov_cooccur_edges")
        .and_then(|v| v.as_array())
        .expect("cov_cooccur_edges array present");
    // tests/a ↔ tests/b share 2 of 4 lines → Jaccard = 0.5 → edge.
    // tests/a ↔ tests/c are disjoint → no edge.
    let ab = edges.iter().find(|e| {
        let p1 = e["from_path"].as_str().unwrap_or("");
        let p2 = e["to_path"].as_str().unwrap_or("");
        (p1 == "tests/a.rs" && p2 == "tests/b.rs") || (p1 == "tests/b.rs" && p2 == "tests/a.rs")
    });
    assert!(ab.is_some(), "expected tests/a ↔ tests/b edge in {edges:?}");
    let ac = edges.iter().find(|e| {
        let p1 = e["from_path"].as_str().unwrap_or("");
        let p2 = e["to_path"].as_str().unwrap_or("");
        (p1 == "tests/a.rs" && p2 == "tests/c.rs") || (p1 == "tests/c.rs" && p2 == "tests/a.rs")
    });
    assert!(
        ac.is_none(),
        "tests/a and tests/c are disjoint — no edge expected"
    );

    // Per-node coverage_hash is populated for tests with fixture data.
    let nodes = val["nodes"].as_array().unwrap();
    let a = nodes
        .iter()
        .find(|n| n["path"].as_str() == Some("tests/a.rs"))
        .unwrap();
    assert!(
        a.get("coverage_hash").is_some(),
        "a should have coverage_hash"
    );
}

#[test]
fn coverage_threshold_filters_low_similarity_pairs() {
    let repo = TempRepo::new();
    seed_scan(&repo);
    let lcov_dir = seed_lcov_dir(&repo);

    // With threshold 0.9, the 0.5-Jaccard a↔b pair should NOT emit.
    let out = repo
        .run_specere(&[
            "harness",
            "coverage",
            "--from-lcov-dir",
            lcov_dir.to_str().unwrap(),
            "--threshold",
            "0.9",
        ])
        .output()
        .expect("spawn");
    assert!(out.status.success());

    let raw = std::fs::read_to_string(repo.abs(".specere/harness-graph.toml")).unwrap();
    let val: toml::Value = toml::from_str(&raw).unwrap();
    let edges = val
        .get("cov_cooccur_edges")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        edges.is_empty(),
        "with threshold=0.9, no pair should emit; got {edges:?}"
    );
}

#[test]
fn coverage_without_scan_prints_friendly_message() {
    let repo = TempRepo::new();
    let out = repo
        .run_specere(&["harness", "coverage"])
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
fn coverage_handles_missing_lcov_dir_gracefully() {
    let repo = TempRepo::new();
    seed_scan(&repo);
    let fake = repo.abs(".specere/nonexistent-lcov");
    let out = repo
        .run_specere(&[
            "harness",
            "coverage",
            "--from-lcov-dir",
            fake.to_str().unwrap(),
        ])
        .output()
        .expect("spawn");
    // A missing fixture dir should surface as a clean error, not panic.
    assert!(
        !out.status.success() || {
            let stderr = String::from_utf8_lossy(&out.stderr);
            stderr.contains("No such")
                || stderr.contains("not found")
                || stderr.contains("load lcov")
        },
        "expected clean error:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn coverage_output_is_deterministic_across_runs() {
    let repo = TempRepo::new();
    seed_scan(&repo);
    let lcov_dir = seed_lcov_dir(&repo);
    let _ = repo
        .run_specere(&[
            "harness",
            "coverage",
            "--from-lcov-dir",
            lcov_dir.to_str().unwrap(),
        ])
        .output()
        .expect("spawn");
    let first = std::fs::read(repo.abs(".specere/harness-graph.toml")).unwrap();
    let _ = repo
        .run_specere(&[
            "harness",
            "coverage",
            "--from-lcov-dir",
            lcov_dir.to_str().unwrap(),
        ])
        .output()
        .expect("spawn");
    let second = std::fs::read(repo.abs(".specere/harness-graph.toml")).unwrap();
    assert_eq!(first, second, "repeated coverage must be byte-identical");
}
