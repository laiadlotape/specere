//! FR-HM-020..022 — `specere harness history` end-to-end.
//!
//! Seeds a temp repo with a realistic history (churn + co-modification +
//! a solo-committed file), drives the CLI, and asserts that:
//! 1. Each node gets `version_metrics` (commits, authors, churn_rate,
//!    age_days, hotspot_score, last_touched).
//! 2. A strongly co-coupled pair emits a `comod` edge with PPMI > 0
//!    and `co_commits >= min_commits`.
//! 3. Without a prior `harness scan`, the CLI prints friendly guidance.

mod common;

use common::TempRepo;

fn git(repo: &TempRepo, args: &[&str]) {
    std::process::Command::new("git")
        .args(args)
        .current_dir(repo.path())
        .status()
        .expect("git");
}

fn seed_scan_with_two_coupled_files(repo: &TempRepo) {
    repo.write(
        "Cargo.toml",
        "[package]\nname = \"demo\"\nversion=\"0.1\"\n",
    );
    repo.write("src/lib.rs", "#[cfg(test)] mod t { #[test] fn a(){} }");
    repo.write("tests/a_it.rs", "#[test] fn aa(){}");
    repo.write("tests/b_it.rs", "#[test] fn bb(){}");
    // Scan establishes the graph.
    let out = repo.run_specere(&["harness", "scan"]).output().unwrap();
    assert!(out.status.success(), "seed scan failed");

    // Now evolve history: 4 joint commits across a_it + b_it, plus one
    // solo commit on lib.rs (to give the PPMI denominator something
    // that doesn't contain both).
    for i in 0..4 {
        repo.write("tests/a_it.rs", &format!("#[test] fn aa{i}(){{}}"));
        repo.write("tests/b_it.rs", &format!("#[test] fn bb{i}(){{}}"));
        git(repo, &["add", "."]);
        git(repo, &["commit", "-q", "-m", "coupled edit"]);
    }
    repo.write("src/lib.rs", "pub fn solo(){}");
    git(repo, &["add", "src/lib.rs"]);
    git(repo, &["commit", "-q", "-m", "solo"]);
}

#[test]
fn history_enriches_every_node_with_version_metrics() {
    let repo = TempRepo::new();
    seed_scan_with_two_coupled_files(&repo);

    let out = repo
        .run_specere(&["harness", "history"])
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "history failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let raw = std::fs::read_to_string(repo.abs(".specere/harness-graph.toml")).unwrap();
    let val: toml::Value = toml::from_str(&raw).expect("valid TOML");
    let nodes = val["nodes"].as_array().unwrap();
    let a = nodes
        .iter()
        .find(|n| n["path"].as_str() == Some("tests/a_it.rs"))
        .unwrap();
    let vm = a.get("version_metrics").expect("version_metrics populated");
    // a_it appears in the 4 joint coupled-edit commits (first of those
    // is its add; the other 3 are modifications).
    let commits = vm["commits"].as_integer().unwrap();
    assert_eq!(
        commits, 4,
        "expected 4 commits on tests/a_it.rs, got {commits}"
    );
    assert!(vm["hotspot_score"].as_float().unwrap() > 0.0);
    assert!(vm.get("last_touched").is_some());
}

#[test]
fn history_emits_comod_edge_for_coupled_pair() {
    let repo = TempRepo::new();
    seed_scan_with_two_coupled_files(&repo);

    let out = repo
        .run_specere(&["harness", "history", "--min-commits", "3"])
        .output()
        .expect("spawn");
    assert!(out.status.success());

    let raw = std::fs::read_to_string(repo.abs(".specere/harness-graph.toml")).unwrap();
    let val: toml::Value = toml::from_str(&raw).unwrap();
    let comod = val
        .get("comod_edges")
        .and_then(|v| v.as_array())
        .expect("comod_edges table must be present");
    // We seeded 4 joint commits on a_it + b_it, one solo on lib.rs.
    // That gives PPMI = log2((4/5) / (4/5 × 4/5)) = log2(5/4) > 0.
    let edge = comod
        .iter()
        .find(|e| {
            let p1 = e["from_path"].as_str().unwrap_or("");
            let p2 = e["to_path"].as_str().unwrap_or("");
            (p1 == "tests/a_it.rs" && p2 == "tests/b_it.rs")
                || (p1 == "tests/b_it.rs" && p2 == "tests/a_it.rs")
        })
        .expect("expected a_it ↔ b_it comod edge");
    let co = edge["co_commits"].as_integer().unwrap();
    assert!(co >= 3, "co_commits should be >= 3; got {co}");
    let ppmi = edge["ppmi"].as_float().unwrap();
    assert!(ppmi > 0.0, "ppmi must be > 0 for coupled pair; got {ppmi}");
}

#[test]
fn history_prints_top_hotspots_in_summary() {
    let repo = TempRepo::new();
    seed_scan_with_two_coupled_files(&repo);
    let out = repo.run_specere(&["harness", "history"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("top hotspots"),
        "expected hotspot list; got:\n{stdout}"
    );
}

#[test]
fn history_without_scan_prints_friendly_message() {
    let repo = TempRepo::new();
    let out = repo.run_specere(&["harness", "history"]).output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("run `specere harness scan` first"),
        "expected guidance; got:\n{stdout}"
    );
}
