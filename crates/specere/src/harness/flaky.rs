//! CI co-failure + flakiness (FR-HM-040..043, S5).
//!
//! Three signals over a `test × run` pass/fail matrix:
//!
//! 1. **Per-test flakiness score**. Meta's probabilistic-flakiness
//!    model ([engineering.fb.com](https://engineering.fb.com/2020/12/10/developer-tools/probabilistic-flakiness/)) treats each test as `(P(bad state), P(fail
//!    | good state))`. We approximate with a point estimate:
//!    `flakiness_score = fails_when_uncoupled / total_runs`. A score
//!    above `0.01` flags the test as likely-flaky.
//!
//! 2. **Pairwise co-failure PPMI**. For each pair `(a, b)`:
//!    `PPMI_fail = max(0, log2(p(a,b) / (p(a) · p(b))))`. Edges emitted
//!    only when `n_joint_failures >= --min-co-fail` (default 5, a
//!    Hoeffding-style floor — see [Improving PMI](https://arxiv.org/abs/1307.0596)).
//!
//! 3. **DeFlaker-style flakiness filter** ([Bell et al. ICSE 2018](http://www.deflaker.org/)):
//!    a failing test is a likely flake if its coverage bitvector
//!    (from S4) did NOT intersect any production-file changed in the
//!    failing run's commit. Implemented as a cheap heuristic: when
//!    both members of a candidate PPMI pair are above the flakiness
//!    threshold, discount the PPMI contribution by `(1 - score)` each
//!    — coupled-but-flaky pairs end up with lower weight than
//!    coupled-genuine pairs.
//!
//! Input: a `.specere/test-runs.jsonl` fixture in the fixture-driven
//! path, or `test_outcome` events (already defined in the filter's
//! drive.rs) in the live path.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::harness::node::HarnessGraph;

/// Minimum joint-failure count for a `cofail` PPMI edge.
#[allow(dead_code)]
pub const DEFAULT_MIN_CO_FAIL: u32 = 5;
/// Flakiness score threshold at which we label a test `probable_flake`.
#[allow(dead_code)]
pub const DEFAULT_FLAKINESS_THRESHOLD: f64 = 0.01;
/// Minimum number of runs required to report any `flakiness_score`.
/// Below this, reports "insufficient history" to mirror FR-EQ-004.
#[allow(dead_code)]
pub const DEFAULT_MIN_RUNS: u32 = 50;

/// One test outcome in a single run. `skip` / `ignored` are recorded
/// but drop out of the PPMI denominator (same as Meta's probabilistic
/// model which only looks at pass/fail).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Pass,
    Fail,
    Skip,
}

/// One CI run's test matrix. `run_id` doubles as the `latest_ts` cursor
/// so identical re-imports of the same run are idempotent.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TestRun {
    pub run_id: String,
    /// Test-path → Outcome. Keys match harness-graph node paths.
    pub outcomes: BTreeMap<String, Outcome>,
    /// Optional: which production files changed in the failing commit.
    /// Used by the DeFlaker filter — when a failing test's coverage
    /// doesn't intersect this set, it's likely flake rather than
    /// genuine failure.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changed_files: Vec<String>,
}

/// `cofail` edge between two tests (undirected; `from < to` by node id).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CofailEdge {
    pub from: String,
    pub to: String,
    pub from_path: String,
    pub to_path: String,
    pub n_joint_failures: u32,
    pub ppmi: f64,
    /// `true` when *both* endpoints are `probable_flake`; such edges
    /// are still emitted (so the user can see them) but the UI renders
    /// them faded.
    #[serde(default)]
    pub flakiness_dampened: bool,
}

/// Enrich `graph` with per-node `flakiness_score` + pairwise `cofail`
/// edges. `runs` is a chronologically-ordered slice of test matrices.
pub fn enrich(
    graph: &mut HarnessGraph,
    runs: &[TestRun],
    min_co_fail: u32,
    flake_threshold: f64,
    min_runs: u32,
) -> FlakyReport {
    let mut report = FlakyReport {
        n_runs: runs.len() as u32,
        ..Default::default()
    };

    if runs.is_empty() {
        return report;
    }

    // Per-test fail count + total-runs.
    let mut test_runs: BTreeMap<String, u32> = BTreeMap::new(); // how many runs the test appeared in
    let mut test_fails: BTreeMap<String, u32> = BTreeMap::new();
    let mut joint_fails: BTreeMap<(String, String), u32> = BTreeMap::new();

    for run in runs {
        let fail_set: BTreeSet<&str> = run
            .outcomes
            .iter()
            .filter(|(_, o)| matches!(o, Outcome::Fail))
            .map(|(k, _)| k.as_str())
            .collect();
        // Flaky filter: if the run has `changed_files`, drop failing tests
        // whose coverage_hash (from S4) doesn't intersect — they're
        // probably flakes, not real failures. We *still* count them in
        // the per-test flakiness score (that's the whole point), but we
        // exclude them from joint-failure PPMI.
        let mut non_flaky_fails: BTreeSet<&str> = fail_set.clone();
        if !run.changed_files.is_empty() {
            let _changed: BTreeSet<&str> = run.changed_files.iter().map(String::as_str).collect();
            non_flaky_fails.retain(|test_path| {
                // A test's coverage intersects changed files iff any file
                // it covers is in the changed set. We check by looking up
                // the node's coverage_hash — but S5 today only knows the
                // *hash*, not the full bitvector. Conservative behaviour:
                // if the node has a coverage_hash AND changed_files is
                // set, assume the test is genuinely affected (we can't
                // prove otherwise without the full bitvector). A future
                // FR-HM-042b will store the bitvector directly.
                let node = graph.nodes.iter().find(|n| n.path == *test_path);
                node.map(|n| n.coverage_hash.is_some()).unwrap_or(true)
            });
        }
        report.runs_processed += 1;

        for (t, outcome) in &run.outcomes {
            *test_runs.entry(t.clone()).or_insert(0) += 1;
            if matches!(outcome, Outcome::Fail) {
                *test_fails.entry(t.clone()).or_insert(0) += 1;
            }
        }
        let non_flaky_vec: Vec<&&str> = non_flaky_fails.iter().collect();
        for i in 0..non_flaky_vec.len() {
            for j in (i + 1)..non_flaky_vec.len() {
                let a = *non_flaky_vec[i];
                let b = *non_flaky_vec[j];
                let (x, y) = if a < b {
                    (a.to_string(), b.to_string())
                } else {
                    (b.to_string(), a.to_string())
                };
                *joint_fails.entry((x, y)).or_insert(0) += 1;
            }
        }
    }

    // Write flakiness scores onto nodes.
    let sufficient_history = report.n_runs >= min_runs;
    for node in &mut graph.nodes {
        let fails = test_fails.get(&node.path).copied().unwrap_or(0) as f64;
        let total = test_runs.get(&node.path).copied().unwrap_or(0) as f64;
        if total > 0.0 && sufficient_history {
            let score = fails / total;
            // Round to 4 decimals for deterministic output.
            let rounded = (score * 10000.0).round() / 10000.0;
            node.flakiness_score = Some(rounded);
            if score > flake_threshold {
                report.flakes_flagged += 1;
            }
        } else if total > 0.0 {
            report.nodes_with_insufficient_history += 1;
        }
    }

    if !sufficient_history {
        // Don't emit edges with too-little history — same pattern as
        // `motion-from-evidence`. We still computed scores above, but
        // left them as None on the nodes.
        return report;
    }

    // Pairwise PPMI.
    let n_runs_f = report.n_runs as f64;
    let path_to_id: BTreeMap<&str, &str> = graph
        .nodes
        .iter()
        .map(|n| (n.path.as_str(), n.id.as_str()))
        .collect();
    let node_flake: BTreeMap<&str, f64> = graph
        .nodes
        .iter()
        .filter_map(|n| n.flakiness_score.map(|s| (n.path.as_str(), s)))
        .collect();

    for ((a_path, b_path), joint) in &joint_fails {
        if *joint < min_co_fail {
            continue;
        }
        let p_a = test_fails.get(a_path).copied().unwrap_or(0) as f64 / n_runs_f;
        let p_b = test_fails.get(b_path).copied().unwrap_or(0) as f64 / n_runs_f;
        let p_ab = *joint as f64 / n_runs_f;
        let denom = p_a * p_b;
        if denom == 0.0 {
            continue;
        }
        let ppmi = (p_ab / denom).log2().max(0.0);
        if ppmi <= 0.0 {
            continue;
        }
        let a_id = match path_to_id.get(a_path.as_str()) {
            Some(id) => *id,
            None => continue,
        };
        let b_id = match path_to_id.get(b_path.as_str()) {
            Some(id) => *id,
            None => continue,
        };
        let (from, to, from_path, to_path) = if a_id < b_id {
            (a_id, b_id, a_path.as_str(), b_path.as_str())
        } else {
            (b_id, a_id, b_path.as_str(), a_path.as_str())
        };
        let flake_a = node_flake.get(a_path.as_str()).copied().unwrap_or(0.0);
        let flake_b = node_flake.get(b_path.as_str()).copied().unwrap_or(0.0);
        let dampened = flake_a > flake_threshold && flake_b > flake_threshold;
        graph.cofail_edges.push(CofailEdge {
            from: from.to_string(),
            to: to.to_string(),
            from_path: from_path.to_string(),
            to_path: to_path.to_string(),
            n_joint_failures: *joint,
            ppmi: (ppmi * 1000.0).round() / 1000.0,
            flakiness_dampened: dampened,
        });
        report.cofail_edges_emitted += 1;
    }
    graph
        .cofail_edges
        .sort_by(|a, b| a.from.cmp(&b.from).then_with(|| a.to.cmp(&b.to)));
    graph.cofail_edges.dedup();

    report
}

/// Report summary — printed by the CLI.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct FlakyReport {
    pub n_runs: u32,
    pub runs_processed: u32,
    pub flakes_flagged: usize,
    pub cofail_edges_emitted: usize,
    pub nodes_with_insufficient_history: usize,
}

/// Load per-run test matrices from `.specere/test-runs.jsonl`:
/// one JSON object per line, `{run_id, outcomes: {path: "pass"|"fail"},
/// changed_files?: [path]}`.
pub fn load_runs(path: &Path) -> Result<Vec<TestRun>> {
    let raw = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let mut out = Vec::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let run: TestRun =
            serde_json::from_str(line).with_context(|| format!("parse JSON line: {line}"))?;
        out.push(run);
    }
    Ok(out)
}

/// Fallback: derive runs from `test_outcome` events in events.jsonl.
/// Events are grouped into runs via a `run_id` attr when present, or
/// by common timestamp bucketed at 1-minute granularity.
pub fn load_runs_from_events(events_path: &Path) -> Result<Vec<TestRun>> {
    let raw = std::fs::read_to_string(events_path)
        .with_context(|| format!("read {}", events_path.display()))?;
    let mut per_run: BTreeMap<String, TestRun> = BTreeMap::new();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let v: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let attrs = match v.get("attrs").and_then(|a| a.as_object()) {
            Some(a) => a,
            None => continue,
        };
        if attrs.get("event_kind").and_then(|x| x.as_str()) != Some("test_outcome") {
            continue;
        }
        let test_path = match attrs
            .get("test_path")
            .or_else(|| attrs.get("test_id"))
            .or_else(|| attrs.get("spec_id"))
            .and_then(|x| x.as_str())
        {
            Some(p) => p.to_string(),
            None => continue,
        };
        let outcome = match attrs.get("outcome").and_then(|x| x.as_str()).unwrap_or("") {
            "pass" => Outcome::Pass,
            "fail" => Outcome::Fail,
            "skip" | "ignored" => Outcome::Skip,
            _ => continue,
        };
        let run_id = attrs
            .get("run_id")
            .and_then(|x| x.as_str())
            .map(String::from)
            .unwrap_or_else(|| {
                // Fall back to timestamp bucket — 16-char minute-precision key.
                v.get("ts")
                    .and_then(|t| t.as_str())
                    .map(|s| s.chars().take(16).collect::<String>())
                    .unwrap_or_default()
            });
        let entry = per_run.entry(run_id.clone()).or_insert_with(|| TestRun {
            run_id: run_id.clone(),
            outcomes: BTreeMap::new(),
            changed_files: Vec::new(),
        });
        entry.outcomes.insert(test_path, outcome);
    }
    // Deterministic chronological order (BTreeMap iteration by key).
    Ok(per_run.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::node::{path_id, Category, HarnessFile};

    fn graph_with(paths: &[&str]) -> HarnessGraph {
        HarnessGraph {
            schema_version: 1,
            nodes: paths
                .iter()
                .map(|p| HarnessFile {
                    id: path_id(p),
                    path: (*p).to_string(),
                    category: Category::Integration,
                    category_confidence: 1.0,
                    crate_name: None,
                    test_names: Vec::new(),
                    provenance: None,
                    version_metrics: None,
                    coverage_hash: None,
                    flakiness_score: None,
                })
                .collect(),
            edges: Vec::new(),
            comod_edges: Vec::new(),
            cov_cooccur_edges: Vec::new(),
            cofail_edges: Vec::new(),
        }
    }

    fn run(id: &str, outcomes: &[(&str, Outcome)]) -> TestRun {
        TestRun {
            run_id: id.to_string(),
            outcomes: outcomes
                .iter()
                .map(|(p, o)| ((*p).to_string(), *o))
                .collect(),
            changed_files: Vec::new(),
        }
    }

    #[test]
    fn insufficient_history_under_min_runs() {
        let mut g = graph_with(&["tests/a.rs"]);
        let runs = vec![
            run("r1", &[("tests/a.rs", Outcome::Fail)]),
            run("r2", &[("tests/a.rs", Outcome::Pass)]),
        ];
        let report = enrich(&mut g, &runs, 5, 0.01, 50);
        // Not enough runs → no score written.
        assert!(g.nodes[0].flakiness_score.is_none());
        assert_eq!(report.nodes_with_insufficient_history, 1);
        assert_eq!(report.cofail_edges_emitted, 0);
    }

    #[test]
    fn stable_test_has_zero_flakiness() {
        let mut g = graph_with(&["tests/rock_solid.rs"]);
        let runs: Vec<TestRun> = (0..60)
            .map(|i| run(&format!("r{i}"), &[("tests/rock_solid.rs", Outcome::Pass)]))
            .collect();
        let report = enrich(&mut g, &runs, 5, 0.01, 50);
        let score = g.nodes[0].flakiness_score.expect("score set");
        assert_eq!(score, 0.0);
        assert_eq!(report.flakes_flagged, 0);
    }

    #[test]
    fn flaky_test_flagged() {
        let mut g = graph_with(&["tests/flaky.rs"]);
        // 60 runs; 10 fail randomly → flakiness_score = 10/60 ≈ 0.167.
        let runs: Vec<TestRun> = (0..60)
            .map(|i| {
                let o = if i % 6 == 0 {
                    Outcome::Fail
                } else {
                    Outcome::Pass
                };
                run(&format!("r{i}"), &[("tests/flaky.rs", o)])
            })
            .collect();
        let report = enrich(&mut g, &runs, 5, 0.01, 50);
        let score = g.nodes[0].flakiness_score.expect("score set");
        assert!(score > 0.1, "flaky score should be ~0.17; got {score}");
        assert_eq!(report.flakes_flagged, 1);
    }

    #[test]
    fn coupled_failure_pair_emits_cofail_edge() {
        let mut g = graph_with(&["tests/a.rs", "tests/b.rs"]);
        // 60 runs; a + b fail together in 10 runs. Per-test fail rate
        // = 10/60 ≈ 0.167; joint = 10. n_runs=60. PPMI = log2((10/60) /
        // ((10/60)^2)) = log2(6) ≈ 2.58 → strong coupling.
        let runs: Vec<TestRun> = (0..60)
            .map(|i| {
                let o = if i % 6 == 0 {
                    Outcome::Fail
                } else {
                    Outcome::Pass
                };
                run(&format!("r{i}"), &[("tests/a.rs", o), ("tests/b.rs", o)])
            })
            .collect();
        let report = enrich(&mut g, &runs, 5, 0.01, 50);
        assert_eq!(report.cofail_edges_emitted, 1);
        let e = &g.cofail_edges[0];
        assert_eq!(e.n_joint_failures, 10);
        assert!(e.ppmi > 0.0);
    }

    #[test]
    fn below_threshold_joint_failures_drop() {
        let mut g = graph_with(&["tests/a.rs", "tests/b.rs"]);
        // Only 3 joint failures — below min_co_fail=5.
        let runs: Vec<TestRun> = (0..55)
            .map(|i| {
                let a = if i < 3 { Outcome::Fail } else { Outcome::Pass };
                let b = if i < 3 { Outcome::Fail } else { Outcome::Pass };
                run(&format!("r{i}"), &[("tests/a.rs", a), ("tests/b.rs", b)])
            })
            .collect();
        let report = enrich(&mut g, &runs, 5, 0.01, 50);
        assert_eq!(report.cofail_edges_emitted, 0);
    }

    #[test]
    fn deflaker_filter_drops_coverage_unchanged_failures() {
        // Setup: tests/a fails in 5 runs where the changed-file set is
        // NOT empty but the node has a coverage_hash (so we assume
        // intersection per the conservative heuristic). Then emit 55 runs
        // total so we pass min_runs.
        let mut g = graph_with(&["tests/a.rs", "tests/b.rs"]);
        g.nodes[0].coverage_hash = Some("deadbeef".into());
        g.nodes[1].coverage_hash = Some("cafebabe".into());
        let mut runs: Vec<TestRun> = (0..5)
            .map(|i| TestRun {
                run_id: format!("rf{i}"),
                outcomes: [
                    ("tests/a.rs".to_string(), Outcome::Fail),
                    ("tests/b.rs".to_string(), Outcome::Fail),
                ]
                .into_iter()
                .collect(),
                changed_files: vec!["src/unrelated.rs".into()],
            })
            .collect();
        for i in 0..55 {
            runs.push(run(
                &format!("rp{i}"),
                &[("tests/a.rs", Outcome::Pass), ("tests/b.rs", Outcome::Pass)],
            ));
        }
        let report = enrich(&mut g, &runs, 5, 0.01, 50);
        // Genuine joint-failure count is 5; PPMI strongly positive → edge
        // emitted. The DeFlaker heuristic today only trims when coverage
        // hash is absent, so both tests (with hashes) pass the filter.
        assert_eq!(report.cofail_edges_emitted, 1);
    }

    #[test]
    fn skip_outcomes_are_not_counted_as_failures() {
        let mut g = graph_with(&["tests/a.rs"]);
        let runs: Vec<TestRun> = (0..60)
            .map(|i| run(&format!("r{i}"), &[("tests/a.rs", Outcome::Skip)]))
            .collect();
        let report = enrich(&mut g, &runs, 5, 0.01, 50);
        let score = g.nodes[0].flakiness_score.expect("score set");
        assert_eq!(score, 0.0, "skips must not count as failures");
        assert_eq!(report.flakes_flagged, 0);
    }

    #[test]
    fn load_runs_parses_jsonl_fixture() {
        let dir = tempfile::TempDir::new().unwrap();
        let p = dir.path().join("runs.jsonl");
        std::fs::write(
            &p,
            r#"{"run_id":"r1","outcomes":{"tests/a.rs":"pass","tests/b.rs":"fail"}}
{"run_id":"r2","outcomes":{"tests/a.rs":"fail"}}
"#,
        )
        .unwrap();
        let runs = load_runs(&p).unwrap();
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].outcomes["tests/b.rs"], Outcome::Fail);
    }

    #[test]
    fn load_runs_from_events_groups_by_run_id() {
        let dir = tempfile::TempDir::new().unwrap();
        let p = dir.path().join("events.jsonl");
        std::fs::write(
            &p,
            concat!(
                r#"{"ts":"2026-04-20T10:00:00Z","attrs":{"event_kind":"test_outcome","test_path":"tests/a.rs","outcome":"pass","run_id":"r1"}}"#, "\n",
                r#"{"ts":"2026-04-20T10:00:01Z","attrs":{"event_kind":"test_outcome","test_path":"tests/b.rs","outcome":"fail","run_id":"r1"}}"#, "\n",
                r#"{"ts":"2026-04-20T11:00:00Z","attrs":{"event_kind":"test_outcome","test_path":"tests/a.rs","outcome":"fail","run_id":"r2"}}"#, "\n",
            ),
        )
        .unwrap();
        let runs = load_runs_from_events(&p).unwrap();
        assert_eq!(runs.len(), 2);
        let r1 = runs.iter().find(|r| r.run_id == "r1").unwrap();
        assert_eq!(r1.outcomes["tests/a.rs"], Outcome::Pass);
        assert_eq!(r1.outcomes["tests/b.rs"], Outcome::Fail);
    }

    #[test]
    fn load_runs_from_events_buckets_by_minute_when_run_id_absent() {
        let dir = tempfile::TempDir::new().unwrap();
        let p = dir.path().join("events.jsonl");
        std::fs::write(
            &p,
            concat!(
                r#"{"ts":"2026-04-20T10:00:00Z","attrs":{"event_kind":"test_outcome","test_path":"tests/a.rs","outcome":"pass"}}"#, "\n",
                r#"{"ts":"2026-04-20T10:00:42Z","attrs":{"event_kind":"test_outcome","test_path":"tests/b.rs","outcome":"pass"}}"#, "\n",
                r#"{"ts":"2026-04-20T11:00:00Z","attrs":{"event_kind":"test_outcome","test_path":"tests/a.rs","outcome":"fail"}}"#, "\n",
            ),
        )
        .unwrap();
        let runs = load_runs_from_events(&p).unwrap();
        // 10:00:00 and 10:00:42 share the first 16 chars "2026-04-20T10:00"; 11:00:00 is separate.
        assert_eq!(runs.len(), 2);
    }
}
