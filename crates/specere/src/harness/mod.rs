//! Harness manager & inspector (FR-HM-001..085).
//!
//! The harness manager treats every test/bench/fuzz/mock/fixture/workflow
//! file as a first-class typed node in a graph, alongside the production
//! code it exercises. Over six implementation slices (S1–S6; see
//! `docs/harness-manager-plan.md`), SpecERE builds up:
//!
//! - **S1** (this module today): enumerate + categorise into nine classes;
//!   extract test names; emit direct-use edges from `rustc --emit=dep-info`.
//! - **S2** (upcoming): provenance join — link each harness file to the
//!   `/speckit-*` verb that created it, via existing workflow_span events.
//! - **S3**: git version metrics + co-modification PPMI edges.
//! - **S4**: per-test coverage bitvectors via `cargo-llvm-cov` → Jaccard
//!   → `cov_cooccur` edges.
//! - **S5**: CI co-failure + Meta-style probabilistic flakiness.
//! - **S6**: Leiden community detection on the combined edge graph →
//!   cluster-belief priors wired into the BBN.
//!
//! This file orchestrates; the heavy lifting lives in submodules.

pub mod classify;
pub mod cluster;
pub mod coverage;
pub mod dep_info;
pub mod flaky;
pub mod history;
pub mod node;
pub mod provenance;
pub mod scan;

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result};

/// FR-HM-061 — emit a `harness_*_completed` summary event to the event
/// store after each CLI verb. Attributes follow
/// `docs/otel-specere-semconv.md` §5.3. Failures to emit are non-fatal —
/// we never fail the user's CLI because telemetry couldn't write.
fn emit_completion_event(
    ctx: &specere_core::Ctx,
    event_kind: &'static str,
    verb: &'static str,
    mut attrs: BTreeMap<String, String>,
) {
    attrs.insert("event_kind".into(), event_kind.into());
    attrs.insert("specere.schema_version".into(), "1".into());
    attrs.insert(
        "specere.cli_version".into(),
        env!("CARGO_PKG_VERSION").into(),
    );
    attrs.insert("specere.harness.verb".into(), verb.into());
    let event = specere_telemetry::Event {
        ts: specere_telemetry::event_store::now_rfc3339(),
        source: format!("specere-harness-{verb}"),
        signal: "traces".into(),
        name: Some(format!("harness {verb} completed")),
        feature_dir: None,
        attrs,
    };
    let _ = specere_telemetry::record(ctx, event);
}

/// CLI entry — `specere harness scan [--format summary|json|toml]`.
/// Writes `.specere/harness-graph.toml` with classified nodes + any
/// direct-use edges discovered via `rustc --emit=dep-info`. Prints a
/// stdout summary in the requested format.
pub fn run_scan(ctx: &specere_core::Ctx, format: &str) -> Result<()> {
    let repo = ctx.repo();
    let nodes = scan::scan_repo(repo).with_context(|| format!("scan {}", repo.display()))?;

    // Collect direct-use edges from target/debug/deps/*.d, if present.
    // No edges when dep-info is absent — the S1 contract is that a fresh
    // clone yields nodes-only; edges fill in after the first build.
    let dep_dir = repo.join("target").join("debug").join("deps");
    let edges = if dep_dir.is_dir() {
        dep_info::collect_edges(&dep_dir, &nodes).unwrap_or_default()
    } else {
        Vec::new()
    };

    let mut graph = node::HarnessGraph {
        schema_version: 1,
        nodes,
        edges,
        comod_edges: Vec::new(),
        cov_cooccur_edges: Vec::new(),
        cofail_edges: Vec::new(),
        cluster_report: None,
    };

    let out_path = output_path(repo);
    graph
        .write_atomic(&out_path)
        .with_context(|| format!("write {}", out_path.display()))?;

    match format {
        "json" => {
            let j = serde_json::to_string_pretty(&graph)?;
            println!("{j}");
        }
        "toml" => {
            // Emit the same on-disk content to stdout for piping.
            let t = toml::to_string_pretty(&graph)?;
            println!("{t}");
        }
        _ => {
            // "summary" (default): terse per-category counts.
            print_summary(&graph);
        }
    }

    // FR-HM-061 — summary event.
    let mut attrs = BTreeMap::new();
    attrs.insert(
        "specere.harness.n_files".into(),
        graph.nodes.len().to_string(),
    );
    attrs.insert(
        "specere.harness.n_edges.direct_use".into(),
        graph.edges.len().to_string(),
    );
    emit_completion_event(ctx, "harness_scan_completed", "scan", attrs);
    Ok(())
}

fn output_path(repo: &std::path::Path) -> PathBuf {
    repo.join(".specere").join("harness-graph.toml")
}

/// CLI entry — `specere harness provenance`. Reads the existing
/// `.specere/harness-graph.toml`, enriches every node with a provenance
/// record, and writes the result back. Prints a terse summary of
/// span-attributed + git-attributed + divergence-detected counts.
pub fn run_provenance(ctx: &specere_core::Ctx) -> Result<()> {
    let repo = ctx.repo();
    let out_path = output_path(repo);
    let mut graph = node::HarnessGraph::load_or_default(&out_path)
        .with_context(|| format!("read {}", out_path.display()))?;
    if graph.nodes.is_empty() {
        println!(
            "specere harness provenance: no harness-graph.toml found — run `specere harness scan` first"
        );
        return Ok(());
    }
    let report = provenance::enrich(&mut graph, repo)
        .with_context(|| format!("enrich {} provenance", repo.display()))?;
    graph
        .write_atomic(&out_path)
        .with_context(|| format!("write {}", out_path.display()))?;

    println!(
        "specere harness provenance: enriched {}/{} node(s); {} via workflow span, {} via git log",
        report.total_enriched,
        graph.nodes.len(),
        report.span_attributed,
        report.git_attributed,
    );
    if report.divergence_detected > 0 {
        println!(
            "  {} file(s) flagged: agent-created, human-modified (advisory — no block)",
            report.divergence_detected
        );
    }
    let mut attrs = BTreeMap::new();
    attrs.insert(
        "specere.harness.n_files".into(),
        graph.nodes.len().to_string(),
    );
    attrs.insert(
        "specere.harness.n_files_enriched".into(),
        report.total_enriched.to_string(),
    );
    attrs.insert(
        "specere.harness.n_span_attributed".into(),
        report.span_attributed.to_string(),
    );
    attrs.insert(
        "specere.harness.n_git_attributed".into(),
        report.git_attributed.to_string(),
    );
    emit_completion_event(ctx, "harness_provenance_completed", "provenance", attrs);
    Ok(())
}

/// CLI entry — `specere harness cluster`. Runs Louvain on the combined
/// edge graph, writes per-node `cluster_id`s and a cluster-summary
/// table back to `harness-graph.toml`. Optionally emits a
/// `[harness_cluster]` sensor-map snippet to stdout for the user to
/// paste into `.specere/sensor-map.toml` — this is how downstream
/// filters pick up cluster-belief priors (FR-HM-052).
pub fn run_cluster(ctx: &specere_core::Ctx, seed: u64, emit_to_sensor_map: bool) -> Result<()> {
    let repo = ctx.repo();
    let out_path = output_path(repo);
    let mut graph = node::HarnessGraph::load_or_default(&out_path)
        .with_context(|| format!("read {}", out_path.display()))?;
    if graph.nodes.is_empty() {
        println!(
            "specere harness cluster: no harness-graph.toml found — run `specere harness scan` first"
        );
        return Ok(());
    }

    let mix = cluster::EdgeMix::default();
    let report = cluster::enrich(&mut graph, mix, seed);
    let node_total = graph.nodes.len();
    graph.cluster_report = Some(report.clone());
    graph
        .write_atomic(&out_path)
        .with_context(|| format!("write {}", out_path.display()))?;

    println!(
        "specere harness cluster: {} cluster(s) over {} node(s); total modularity = {:.3} (algo={}, seed={})",
        report.n_clusters, node_total, report.total_modularity, report.algo, report.seed,
    );
    for c in &report.clusters {
        println!(
            "  {}  members={}  internal_weight={:.3}  modularity={:.3}",
            c.id,
            c.members.len(),
            c.internal_weight,
            c.modularity
        );
    }
    if emit_to_sensor_map {
        println!();
        println!("{}", cluster::to_sensor_map_snippet(&report));
    }
    let mut attrs = BTreeMap::new();
    attrs.insert("specere.harness.n_files".into(), node_total.to_string());
    attrs.insert(
        "specere.harness.n_clusters".into(),
        report.n_clusters.to_string(),
    );
    attrs.insert(
        "specere.harness.total_modularity".into(),
        format!("{:.4}", report.total_modularity),
    );
    attrs.insert("specere.harness.cluster_seed".into(), seed.to_string());
    attrs.insert(
        "specere.harness.n_edges.cov_cooccur".into(),
        graph.cov_cooccur_edges.len().to_string(),
    );
    attrs.insert(
        "specere.harness.n_edges.comod".into(),
        graph.comod_edges.len().to_string(),
    );
    attrs.insert(
        "specere.harness.n_edges.cofail".into(),
        graph.cofail_edges.len().to_string(),
    );
    emit_completion_event(ctx, "harness_cluster_completed", "cluster", attrs);
    Ok(())
}

/// CLI entry — `specere harness flaky`. Reads
/// `.specere/harness-graph.toml` + per-run test matrices (from
/// `.specere/test-runs.jsonl`, or from `test_outcome` events in
/// `.specere/events.jsonl` as a fallback, or from a fixture dir via
/// `--from-runs`), then computes per-node `flakiness_score` and
/// pairwise `cofail_edges` via PPMI. Reports `insufficient history`
/// when fewer than `--min-runs` (default 50) runs have accumulated.
pub fn run_flaky(
    ctx: &specere_core::Ctx,
    from_runs: Option<PathBuf>,
    min_co_fail: u32,
    flake_threshold: f64,
    min_runs: u32,
) -> Result<()> {
    let repo = ctx.repo();
    let out_path = output_path(repo);
    let mut graph = node::HarnessGraph::load_or_default(&out_path)
        .with_context(|| format!("read {}", out_path.display()))?;
    if graph.nodes.is_empty() {
        println!(
            "specere harness flaky: no harness-graph.toml found — run `specere harness scan` first"
        );
        return Ok(());
    }

    let runs = if let Some(p) = from_runs {
        flaky::load_runs(&p).with_context(|| format!("load {}", p.display()))?
    } else {
        // Live path: read from the event store.
        let events = repo.join(".specere").join("events.jsonl");
        if events.is_file() {
            flaky::load_runs_from_events(&events)
                .with_context(|| format!("load {}", events.display()))?
        } else {
            Vec::new()
        }
    };

    let report = flaky::enrich(&mut graph, &runs, min_co_fail, flake_threshold, min_runs);
    let node_total = graph.nodes.len();
    graph
        .write_atomic(&out_path)
        .with_context(|| format!("write {}", out_path.display()))?;

    let scored = graph
        .nodes
        .iter()
        .filter(|n| n.flakiness_score.is_some())
        .count();
    let mut attrs = BTreeMap::new();
    attrs.insert("specere.harness.n_files".into(), node_total.to_string());
    attrs.insert(
        "specere.harness.n_files_enriched".into(),
        scored.to_string(),
    );
    attrs.insert("specere.harness.n_runs".into(), report.n_runs.to_string());
    attrs.insert(
        "specere.harness.flakes_flagged".into(),
        report.flakes_flagged.to_string(),
    );
    attrs.insert(
        "specere.harness.n_edges.cofail".into(),
        report.cofail_edges_emitted.to_string(),
    );
    attrs.insert(
        "specere.harness.insufficient_history".into(),
        (report.n_runs < min_runs).to_string(),
    );
    emit_completion_event(ctx, "harness_flaky_completed", "flaky", attrs);

    if report.n_runs < min_runs {
        println!(
            "specere harness flaky: {} run(s) in history — need ≥ {} for PPMI (insufficient history)",
            report.n_runs, min_runs
        );
        return Ok(());
    }
    println!(
        "specere harness flaky: processed {} run(s); {}/{} node(s) scored; {} probable flake(s); {} cofail edge(s) (min_co_fail={})",
        report.n_runs,
        scored,
        node_total,
        report.flakes_flagged,
        report.cofail_edges_emitted,
        min_co_fail,
    );
    Ok(())
}

/// CLI entry — `specere harness coverage`. Reads
/// `.specere/harness-graph.toml`, loads per-test LCOV files (either from
/// a test-supplied fixture directory via `--from-lcov-dir`, or from a
/// live `cargo llvm-cov` run), computes per-test bitvectors + Jaccard
/// similarity, and emits `cov_cooccur` edges. Writes back to the graph.
pub fn run_coverage(
    ctx: &specere_core::Ctx,
    from_lcov_dir: Option<PathBuf>,
    threshold: f64,
) -> Result<()> {
    let repo = ctx.repo();
    let out_path = output_path(repo);
    let mut graph = node::HarnessGraph::load_or_default(&out_path)
        .with_context(|| format!("read {}", out_path.display()))?;
    if graph.nodes.is_empty() {
        println!(
            "specere harness coverage: no harness-graph.toml found — run `specere harness scan` first"
        );
        return Ok(());
    }

    let coverages = if let Some(dir) = from_lcov_dir {
        coverage::load_lcov_dir(&dir)
            .with_context(|| format!("load lcov from {}", dir.display()))?
    } else {
        // Live run: single aggregate LCOV for now; per-test granularity
        // is a follow-up in S4b once we want to pay the wall-clock cost.
        match coverage::run_live_coverage(repo) {
            Ok(agg) => {
                let mut m = std::collections::BTreeMap::new();
                m.insert("aggregate".to_string(), agg);
                m
            }
            Err(e) => {
                eprintln!("specere harness coverage: {e:#}");
                return Ok(());
            }
        }
    };

    let report = coverage::enrich(&mut graph, &coverages, threshold);
    let node_total = graph.nodes.len();
    graph
        .write_atomic(&out_path)
        .with_context(|| format!("write {}", out_path.display()))?;

    println!(
        "specere harness coverage: enriched {}/{} node(s); {} cov_cooccur edge(s) (threshold={:.2})",
        report.nodes_enriched, node_total, report.edges_emitted, threshold
    );
    let mut attrs = BTreeMap::new();
    attrs.insert("specere.harness.n_files".into(), node_total.to_string());
    attrs.insert(
        "specere.harness.n_files_enriched".into(),
        report.nodes_enriched.to_string(),
    );
    attrs.insert(
        "specere.harness.n_edges.cov_cooccur".into(),
        report.edges_emitted.to_string(),
    );
    attrs.insert(
        "specere.harness.jaccard_threshold".into(),
        format!("{threshold:.2}"),
    );
    emit_completion_event(ctx, "harness_coverage_completed", "coverage", attrs);
    Ok(())
}

/// CLI entry — `specere harness history`. Reads
/// `.specere/harness-graph.toml`, enriches every node with
/// `version_metrics`, emits `comod_edges` over the min-count threshold,
/// and writes the result back. Prints a top-5 hotspot list.
pub fn run_history(ctx: &specere_core::Ctx, min_comod_commits: u32) -> Result<()> {
    let repo = ctx.repo();
    let out_path = output_path(repo);
    let mut graph = node::HarnessGraph::load_or_default(&out_path)
        .with_context(|| format!("read {}", out_path.display()))?;
    if graph.nodes.is_empty() {
        println!(
            "specere harness history: no harness-graph.toml found — run `specere harness scan` first"
        );
        return Ok(());
    }
    let report = history::enrich(&mut graph, repo, min_comod_commits)
        .with_context(|| format!("history walk over {}", repo.display()))?;
    // Snapshot hotspot list before the mutable write_atomic call borrows graph.
    let mut hotspots: Vec<(String, node::VersionMetrics)> = graph
        .nodes
        .iter()
        .filter_map(|n| {
            n.version_metrics
                .as_ref()
                .map(|vm| (n.path.clone(), vm.clone()))
        })
        .collect();
    hotspots.sort_by(|a, b| {
        b.1.hotspot_score
            .partial_cmp(&a.1.hotspot_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let node_total = graph.nodes.len();

    graph
        .write_atomic(&out_path)
        .with_context(|| format!("write {}", out_path.display()))?;

    println!(
        "specere harness history: enriched {}/{} node(s); {} comod edge(s) (min_co_commits={})",
        report.nodes_enriched, node_total, report.comod_edges_emitted, min_comod_commits
    );
    if !hotspots.is_empty() {
        println!("  top hotspots (hotspot_score, path):");
        for (path, vm) in hotspots.iter().take(5) {
            println!(
                "    {score:>7.2}  {path}  (commits={c}, churn={ch}, age={a}d, authors={au})",
                score = vm.hotspot_score,
                path = path,
                c = vm.commits,
                ch = vm.churn_rate,
                a = vm.age_days,
                au = vm.authors
            );
        }
    }
    let mut attrs = BTreeMap::new();
    attrs.insert("specere.harness.n_files".into(), node_total.to_string());
    attrs.insert(
        "specere.harness.n_files_enriched".into(),
        report.nodes_enriched.to_string(),
    );
    attrs.insert(
        "specere.harness.n_edges.comod".into(),
        report.comod_edges_emitted.to_string(),
    );
    attrs.insert(
        "specere.harness.min_co_commits".into(),
        min_comod_commits.to_string(),
    );
    emit_completion_event(ctx, "harness_history_completed", "history", attrs);
    Ok(())
}

fn print_summary(graph: &node::HarnessGraph) {
    let mut counts: std::collections::BTreeMap<node::Category, usize> =
        std::collections::BTreeMap::new();
    for n in &graph.nodes {
        *counts.entry(n.category).or_insert(0) += 1;
    }
    println!(
        "specere harness scan: {} file(s) classified",
        graph.nodes.len()
    );
    for (cat, n) in &counts {
        println!("  {:<12} {n}", cat.as_str());
    }
    println!("  direct_use edges: {}", graph.edges.len());
}
