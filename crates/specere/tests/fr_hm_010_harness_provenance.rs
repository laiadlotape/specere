//! FR-HM-010..012 — `specere harness provenance` end-to-end.
//!
//! Three scenarios:
//! 1. A workflow-span event in .specere/events.jsonl correctly claims a
//!    harness file → provenance populated with creator_verb / agent / spec.
//! 2. No workflow span → falls back to git log creation commit + author.
//! 3. Divergence heuristic flags agent-created files that also have a
//!    human git commit (conservative; advisory only).

mod common;

use common::TempRepo;

fn seed_scan(repo: &TempRepo) {
    repo.write(
        "Cargo.toml",
        "[package]\nname = \"demo\"\nversion=\"0.1\"\n",
    );
    repo.write("src/lib.rs", "#[cfg(test)] mod t { #[test] fn a(){} }");
    repo.write("tests/it.rs", "#[test] fn i1(){}");
    // Scan populates the graph first.
    let out = repo
        .run_specere(&["harness", "scan"])
        .output()
        .expect("spawn");
    assert!(out.status.success(), "seed scan failed");
}

#[test]
fn provenance_pulls_creator_verb_from_workflow_span() {
    let repo = TempRepo::new();
    seed_scan(&repo);
    // Also commit the files so git log has something to find.
    std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(repo.path())
        .status()
        .unwrap();
    std::process::Command::new("git")
        .args(["commit", "-q", "-m", "harness seed"])
        .current_dir(repo.path())
        .status()
        .unwrap();

    // Seed one workflow span claiming tests/it.rs.
    let events_line = r#"{"ts":"2026-04-19T10:00:00Z","span_id":"span-abc","source":"claude-code","signal":"traces","attrs":{"event_kind":"workflow_span","specere.workflow_step":"implement","files_created":"tests/it.rs","gen_ai.system":"claude-code","specere.fr_ids":"FR-HM-001"}}"#;
    let events_path = repo.abs(".specere/events.jsonl");
    std::fs::write(&events_path, format!("{events_line}\n")).unwrap();

    let out = repo
        .run_specere(&["harness", "provenance"])
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "provenance failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("1 via workflow span"),
        "expected 1 span-attributed; got:\n{stdout}"
    );

    // Verify TOML round-trip — the integration test's authoritative
    // check is on the on-disk graph, not just stdout.
    let raw = std::fs::read_to_string(repo.abs(".specere/harness-graph.toml")).unwrap();
    let val: toml::Value = toml::from_str(&raw).expect("valid TOML");
    let nodes = val["nodes"].as_array().unwrap();
    let it = nodes
        .iter()
        .find(|n| n["path"].as_str() == Some("tests/it.rs"))
        .expect("tests/it.rs node present");
    let prov = it.get("provenance").expect("provenance populated");
    assert_eq!(prov["creator_verb"].as_str(), Some("implement"));
    assert_eq!(prov["creator_agent"].as_str(), Some("claude-code"));
    assert_eq!(prov["creator_spec"].as_str(), Some("FR-HM-001"));
    assert_eq!(prov["creator_span_id"].as_str(), Some("span-abc"));
}

#[test]
fn provenance_falls_back_to_git_when_no_span() {
    let repo = TempRepo::new();
    seed_scan(&repo);
    std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(repo.path())
        .status()
        .unwrap();
    std::process::Command::new("git")
        .args(["commit", "-q", "-m", "pure human seed"])
        .current_dir(repo.path())
        .status()
        .unwrap();

    let out = repo
        .run_specere(&["harness", "provenance"])
        .output()
        .expect("spawn");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("0 via workflow span"), "got:\n{stdout}");
    assert!(stdout.contains("via git log"), "got:\n{stdout}");

    let raw = std::fs::read_to_string(repo.abs(".specere/harness-graph.toml")).unwrap();
    let val: toml::Value = toml::from_str(&raw).unwrap();
    let it = val["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["path"].as_str() == Some("tests/it.rs"))
        .expect("tests/it.rs present");
    let prov = it.get("provenance").expect("git-based provenance present");
    assert!(prov.get("creator_commit").is_some());
    assert!(prov.get("creator_human").is_some());
    assert!(prov.get("creator_verb").is_none(), "no span → no verb");
}

#[test]
fn provenance_without_scan_prints_friendly_message() {
    let repo = TempRepo::new();
    // Deliberately do NOT run scan.
    let out = repo
        .run_specere(&["harness", "provenance"])
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
fn divergence_flag_set_when_agent_created_and_human_committed() {
    let repo = TempRepo::new();
    seed_scan(&repo);
    // Commit as a human.
    std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(repo.path())
        .status()
        .unwrap();
    std::process::Command::new("git")
        .args(["commit", "-q", "-m", "seed"])
        .current_dir(repo.path())
        .status()
        .unwrap();
    // Span claims tests/it.rs was created by claude-code.
    std::fs::write(
        repo.abs(".specere/events.jsonl"),
        r#"{"ts":"2026-04-19T10:00:00Z","span_id":"s1","source":"claude-code","signal":"traces","attrs":{"event_kind":"workflow_span","specere.workflow_step":"implement","files_created":"tests/it.rs","gen_ai.system":"claude-code"}}"#,
    )
    .unwrap();

    let out = repo
        .run_specere(&["harness", "provenance"])
        .output()
        .expect("spawn");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("flagged") && stdout.contains("agent-created, human-modified"),
        "divergence banner expected:\n{stdout}"
    );
}
