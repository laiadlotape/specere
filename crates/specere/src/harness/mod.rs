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
pub mod dep_info;
pub mod node;
pub mod provenance;
pub mod scan;

use std::path::PathBuf;

use anyhow::{Context, Result};

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
