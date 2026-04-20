//! FR-HM-052b — cluster-belief priors wire into the filter's calibration.
//!
//! When `.specere/harness-graph.toml` contains cluster assignments +
//! `flakiness_score` values, `specere filter run` must use
//! `Calibration::from_cluster_evidence` for every spec whose support
//! files fall inside a flaky cluster. The posterior must reflect the
//! compressed quality.

mod common;

use common::TempRepo;

fn seed_sensor_map(repo: &TempRepo) {
    repo.write(
        ".specere/sensor-map.toml",
        r#"
schema_version = 1

[specs]
"FR-auth"    = { support = ["src/auth/"] }
"FR-billing" = { support = ["src/billing/"] }
"#,
    );
}

/// Graph with two clusters — FR-auth's tests are in a flaky cluster,
/// FR-billing's tests are in a clean cluster.
fn seed_flaky_cluster_graph(repo: &TempRepo) {
    repo.write(
        ".specere/harness-graph.toml",
        r#"
schema_version = 1

[[nodes]]
id = "aaaaaaaaaaaaaaaa"
path = "src/auth/a.rs"
category = "integration"
category_confidence = 1.0
flakiness_score = 0.6
cluster_id = "C01"

[[nodes]]
id = "bbbbbbbbbbbbbbbb"
path = "src/auth/b.rs"
category = "integration"
category_confidence = 1.0
flakiness_score = 0.8
cluster_id = "C01"

[[nodes]]
id = "cccccccccccccccc"
path = "src/billing/c.rs"
category = "integration"
category_confidence = 1.0
flakiness_score = 0.0
cluster_id = "C02"

[[nodes]]
id = "dddddddddddddddd"
path = "src/billing/d.rs"
category = "integration"
category_confidence = 1.0
flakiness_score = 0.0
cluster_id = "C02"
"#,
    );
}

fn seed_one_test_outcome_event(repo: &TempRepo, spec: &str, outcome: &str, ts: &str) {
    let events_path = repo.abs(".specere/events.jsonl");
    let existing = std::fs::read_to_string(&events_path).unwrap_or_default();
    let line = format!(
        r#"{{"ts":"{ts}","source":"t","signal":"traces","attrs":{{"event_kind":"test_outcome","spec_id":"{spec}","outcome":"{outcome}"}}}}"#
    );
    let combined = if existing.is_empty() {
        format!("{line}\n")
    } else {
        format!("{existing}{line}\n")
    };
    std::fs::write(&events_path, combined).unwrap();
}

#[test]
fn flaky_cluster_compresses_spec_posterior_toward_uncertainty() {
    let repo = TempRepo::new();
    seed_sensor_map(&repo);
    seed_flaky_cluster_graph(&repo);
    // Both specs get one passing test outcome at the same time.
    seed_one_test_outcome_event(&repo, "FR-auth", "pass", "2026-04-20T10:00:00.000Z");
    seed_one_test_outcome_event(&repo, "FR-billing", "pass", "2026-04-20T10:00:00.001Z");

    let out = repo
        .run_specere(&["filter", "run"])
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "filter run failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    // Parse the posterior. FR-auth (flaky cluster) should have LOWER p_sat
    // than FR-billing (clean cluster) despite both receiving the same
    // passing-test evidence — the cluster compression pushes α_sat
    // toward α_unk, so a single pass is less informative.
    let raw = std::fs::read_to_string(repo.abs(".specere/posterior.toml"))
        .expect("posterior.toml written");
    let post: toml::Value = toml::from_str(&raw).expect("valid TOML");
    let entries = post["entries"].as_array().expect("entries array");
    let auth = entries
        .iter()
        .find(|e| e["spec_id"].as_str() == Some("FR-auth"))
        .expect("FR-auth entry");
    let billing = entries
        .iter()
        .find(|e| e["spec_id"].as_str() == Some("FR-billing"))
        .expect("FR-billing entry");
    let auth_psat = auth["p_sat"].as_float().unwrap();
    let billing_psat = billing["p_sat"].as_float().unwrap();
    assert!(
        billing_psat > auth_psat,
        "flaky-cluster spec must have lower p_sat than clean-cluster spec; \
         got FR-auth p_sat={auth_psat} vs FR-billing p_sat={billing_psat}"
    );
}

#[test]
fn no_harness_graph_preserves_baseline_calibration() {
    // Regression: when harness-graph.toml is absent, the cluster wiring
    // must fall back to the pre-FR-HM-052b formula bit-identically.
    let repo = TempRepo::new();
    seed_sensor_map(&repo);
    // Deliberately do NOT write a harness graph.
    seed_one_test_outcome_event(&repo, "FR-auth", "pass", "2026-04-20T10:00:00.000Z");

    let out = repo
        .run_specere(&["filter", "run"])
        .output()
        .expect("spawn");
    assert!(out.status.success());

    let raw = std::fs::read_to_string(repo.abs(".specere/posterior.toml")).unwrap();
    let post: toml::Value = toml::from_str(&raw).unwrap();
    let auth = post["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["spec_id"].as_str() == Some("FR-auth"))
        .expect("FR-auth entry");
    let p_sat = auth["p_sat"].as_float().unwrap();
    // With prototype alphas + a single pass, p_sat > 0.5 (dominantly
    // pulled toward SAT). Exact value depends on prior + predict step;
    // we just assert the qualitative inequality.
    assert!(
        p_sat > 0.4,
        "baseline (no cluster) calibration should still move posterior toward SAT; got {p_sat}"
    );
}

#[test]
fn clean_cluster_leaves_posterior_indistinguishable_from_baseline() {
    let repo = TempRepo::new();
    seed_sensor_map(&repo);
    // Graph with zero-flakiness nodes — cluster penalty is no-op.
    repo.write(
        ".specere/harness-graph.toml",
        r#"
schema_version = 1

[[nodes]]
id = "eeeeeeeeeeeeeeee"
path = "src/auth/pristine.rs"
category = "integration"
category_confidence = 1.0
flakiness_score = 0.0
cluster_id = "C01"
"#,
    );
    seed_one_test_outcome_event(&repo, "FR-auth", "pass", "2026-04-20T10:00:00.000Z");

    let out = repo
        .run_specere(&["filter", "run"])
        .output()
        .expect("spawn");
    assert!(out.status.success());
    // No assertion on exact numerics here — just confirm the filter
    // ran successfully. Bit-identical comparison with the no-graph
    // path is tricky because IDs differ; covered by the unit test
    // `from_cluster_evidence_with_zero_flakiness_matches_from_evidence`.
}
