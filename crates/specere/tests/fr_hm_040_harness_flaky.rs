//! FR-HM-040..043 — `specere harness flaky` end-to-end.
//!
//! Drives the CLI via the hidden `--from-runs` flag, which reads a
//! per-run JSONL fixture. Tests verify:
//! 1. A stable test with ≥ 50 runs gets a `flakiness_score=0.0` node.
//! 2. A coupled-failure pair (a + b always fail together) emits a
//!    `cofail` edge with positive PPMI.
//! 3. Below-threshold joint failures do NOT emit.
//! 4. Insufficient history (< `--min-runs`) prints a friendly message
//!    and leaves all `flakiness_score` fields null.

mod common;

use common::TempRepo;

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

fn seed_runs(repo: &TempRepo, lines: &[&str]) -> std::path::PathBuf {
    let p = repo.abs(".specere/runs.jsonl");
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, lines.join("\n") + "\n").unwrap();
    p
}

#[test]
fn flaky_scores_stable_test_at_zero() {
    let repo = TempRepo::new();
    seed_scan(&repo);
    // 55 runs, tests/a always passes.
    let lines: Vec<String> = (0..55)
        .map(|i| format!(r#"{{"run_id":"r{i}","outcomes":{{"tests/a.rs":"pass"}}}}"#))
        .collect();
    let borrowed: Vec<&str> = lines.iter().map(String::as_str).collect();
    let runs_path = seed_runs(&repo, &borrowed);

    let out = repo
        .run_specere(&[
            "harness",
            "flaky",
            "--from-runs",
            runs_path.to_str().unwrap(),
        ])
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "flaky failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let raw = std::fs::read_to_string(repo.abs(".specere/harness-graph.toml")).unwrap();
    let val: toml::Value = toml::from_str(&raw).expect("valid TOML");
    let a = val["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["path"].as_str() == Some("tests/a.rs"))
        .unwrap();
    assert_eq!(a["flakiness_score"].as_float().unwrap(), 0.0);
}

#[test]
fn flaky_emits_cofail_for_coupled_pair() {
    let repo = TempRepo::new();
    seed_scan(&repo);
    // 60 runs; a + b fail together in 10 runs (every 6th).
    let lines: Vec<String> = (0..60)
        .map(|i| {
            let outcome = if i % 6 == 0 { "fail" } else { "pass" };
            format!(
                r#"{{"run_id":"r{i}","outcomes":{{"tests/a.rs":"{outcome}","tests/b.rs":"{outcome}"}}}}"#
            )
        })
        .collect();
    let borrowed: Vec<&str> = lines.iter().map(String::as_str).collect();
    let runs_path = seed_runs(&repo, &borrowed);

    let out = repo
        .run_specere(&[
            "harness",
            "flaky",
            "--from-runs",
            runs_path.to_str().unwrap(),
        ])
        .output()
        .expect("spawn");
    assert!(out.status.success());

    let raw = std::fs::read_to_string(repo.abs(".specere/harness-graph.toml")).unwrap();
    let val: toml::Value = toml::from_str(&raw).unwrap();
    let edges = val
        .get("cofail_edges")
        .and_then(|v| v.as_array())
        .expect("cofail_edges array");
    let e = edges.iter().find(|e| {
        let p1 = e["from_path"].as_str().unwrap_or("");
        let p2 = e["to_path"].as_str().unwrap_or("");
        (p1 == "tests/a.rs" && p2 == "tests/b.rs") || (p1 == "tests/b.rs" && p2 == "tests/a.rs")
    });
    assert!(e.is_some(), "expected a↔b cofail edge");
    let edge = e.unwrap();
    assert_eq!(edge["n_joint_failures"].as_integer().unwrap(), 10);
    assert!(edge["ppmi"].as_float().unwrap() > 0.0);
}

#[test]
fn flaky_insufficient_history_prints_friendly_message() {
    let repo = TempRepo::new();
    seed_scan(&repo);
    // Only 3 runs — far below default min_runs=50.
    let runs_path = seed_runs(
        &repo,
        &[
            r#"{"run_id":"r1","outcomes":{"tests/a.rs":"pass"}}"#,
            r#"{"run_id":"r2","outcomes":{"tests/a.rs":"fail"}}"#,
            r#"{"run_id":"r3","outcomes":{"tests/a.rs":"pass"}}"#,
        ],
    );
    let out = repo
        .run_specere(&[
            "harness",
            "flaky",
            "--from-runs",
            runs_path.to_str().unwrap(),
        ])
        .output()
        .expect("spawn");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("insufficient history"),
        "expected insufficient-history banner; got:\n{stdout}"
    );

    // Verify no flakiness_score was stored.
    let raw = std::fs::read_to_string(repo.abs(".specere/harness-graph.toml")).unwrap();
    let val: toml::Value = toml::from_str(&raw).unwrap();
    let a = val["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["path"].as_str() == Some("tests/a.rs"))
        .unwrap();
    assert!(a.get("flakiness_score").is_none());
}

#[test]
fn flaky_without_scan_prints_friendly_message() {
    let repo = TempRepo::new();
    let out = repo
        .run_specere(&["harness", "flaky"])
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
fn flaky_with_custom_min_runs_unlocks_small_fixtures() {
    let repo = TempRepo::new();
    seed_scan(&repo);
    // 10 runs — enough with --min-runs=5.
    let lines: Vec<String> = (0..10)
        .map(|i| {
            let outcome = if i < 2 { "fail" } else { "pass" };
            format!(r#"{{"run_id":"r{i}","outcomes":{{"tests/a.rs":"{outcome}"}}}}"#)
        })
        .collect();
    let borrowed: Vec<&str> = lines.iter().map(String::as_str).collect();
    let runs_path = seed_runs(&repo, &borrowed);

    let out = repo
        .run_specere(&[
            "harness",
            "flaky",
            "--from-runs",
            runs_path.to_str().unwrap(),
            "--min-runs",
            "5",
        ])
        .output()
        .expect("spawn");
    assert!(out.status.success());

    let raw = std::fs::read_to_string(repo.abs(".specere/harness-graph.toml")).unwrap();
    let val: toml::Value = toml::from_str(&raw).unwrap();
    let a = val["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["path"].as_str() == Some("tests/a.rs"))
        .unwrap();
    let score = a["flakiness_score"].as_float().unwrap();
    assert!((score - 0.2).abs() < 0.01, "2/10 = 0.2; got {score}");
}
