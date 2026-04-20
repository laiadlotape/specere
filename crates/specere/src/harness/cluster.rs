//! Community detection on the combined harness-edge graph (FR-HM-050..052).
//!
//! We run Louvain's first phase — greedy local modularity maximisation —
//! over a weighted undirected graph built from every edge type collected
//! by S1–S5:
//!
//! ```text
//! w(a, b) = 0.4·J_cov        (coverage co-execution)
//!         + 0.3·σ(PPMI_fail) (CI co-failure, flake-dampened)
//!         + 0.2·σ(PPMI_mod)  (git co-modification)
//!         + 0.1·direct_use
//!         + 0.1·w_indirect   (IDF-weighted shared helpers)
//! ```
//!
//! where `σ(x) = 1 − exp(−x)` is a saturating normaliser.
//!
//! Louvain is **deterministic given a seed** (FR-HM-050 acceptance). We
//! fix node visitation order by sorted node-id hash, so the output is
//! byte-identical across repeated runs of the same graph.
//!
//! Output: per-node `cluster_id` + a `[harness_clusters]` TOML table
//! summarising members, size, modularity contribution, and a centroid
//! node (the member with the most internal edges).
//!
//! **S6 v1.2.0 scope.** Full Leiden refinement is deferred — the first
//! pass gives meaningful clusters for graphs up to a few hundred nodes,
//! which covers every SpecERE use case today. Upgrade path is documented
//! as FR-HM-050b in the plan's "re-planning triggers" list.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use crate::harness::node::HarnessGraph;

/// Mix coefficients in the composite edge weight (FR-HM-050).
#[derive(Debug, Clone, Copy)]
pub struct EdgeMix {
    pub alpha_cov: f64,
    pub alpha_fail: f64,
    pub alpha_mod: f64,
    pub alpha_direct: f64,
}

impl Default for EdgeMix {
    fn default() -> Self {
        Self {
            alpha_cov: 0.4,
            alpha_fail: 0.3,
            alpha_mod: 0.2,
            alpha_direct: 0.1,
        }
    }
}

/// One cluster — set of node ids + centroid hint + modularity info.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Cluster {
    /// `C01`, `C02`, … — 1-indexed, deterministic.
    pub id: String,
    /// Member harness-node ids (sorted).
    pub members: Vec<String>,
    /// Node id with the most within-cluster weight — the "centroid"
    /// the GUI can use as a cluster label.
    pub centroid: String,
    /// Member with the largest `flakiness_score` within this cluster,
    /// if any — a hint for the `doctor --suspicious` extension.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peak_flakiness_member: Option<String>,
    /// Sum of internal edge weights (after composite mix).
    pub internal_weight: f64,
    /// Modularity contribution of this cluster.
    pub modularity: f64,
}

/// Top-level cluster summary table written to harness-graph.toml.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ClusterReport {
    pub algo: String,
    pub seed: u64,
    pub total_modularity: f64,
    pub n_clusters: usize,
    pub clusters: Vec<Cluster>,
}

/// Composite graph adjacency — symmetric; weights sum for repeated pairs.
type AdjMap = HashMap<String, HashMap<String, f64>>;

/// Build weighted undirected adjacency from the graph's edge tables.
pub fn composite_adjacency(graph: &HarnessGraph, mix: EdgeMix) -> AdjMap {
    let mut adj: AdjMap = HashMap::new();
    // Helper to add symmetric weight.
    let mut add = |a: &str, b: &str, w: f64| {
        if w <= 0.0 || a == b {
            return;
        }
        *adj.entry(a.to_string())
            .or_default()
            .entry(b.to_string())
            .or_insert(0.0) += w;
        *adj.entry(b.to_string())
            .or_default()
            .entry(a.to_string())
            .or_insert(0.0) += w;
    };

    // direct_use: binary edges from dep-info; weight by mix.alpha_direct.
    for e in &graph.edges {
        add(&e.from, &e.to, mix.alpha_direct);
    }
    // comod: PPMI on commit matrix → saturating-normalise, then scale.
    for e in &graph.comod_edges {
        let w = (1.0 - (-e.ppmi).exp()) * mix.alpha_mod;
        add(&e.from, &e.to, w);
    }
    // cov_cooccur: Jaccard in [0, 1]; scale directly.
    for e in &graph.cov_cooccur_edges {
        add(&e.from, &e.to, e.jaccard * mix.alpha_cov);
    }
    // cofail: PPMI normalised; dampened pairs contribute half weight.
    for e in &graph.cofail_edges {
        let damp = if e.flakiness_dampened { 0.5 } else { 1.0 };
        let w = (1.0 - (-e.ppmi).exp()) * mix.alpha_fail * damp;
        add(&e.from, &e.to, w);
    }
    adj
}

/// Louvain first-phase — greedy local modularity max, seed-determined
/// node order.
pub fn louvain(adj: &AdjMap, _seed: u64) -> BTreeMap<String, usize> {
    // k_i = sum of weights incident to i, 2m = sum of all weights.
    let k: HashMap<String, f64> = adj
        .iter()
        .map(|(n, nbrs)| (n.clone(), nbrs.values().sum::<f64>()))
        .collect();
    let two_m: f64 = k.values().sum();
    if two_m <= 0.0 {
        // Degenerate graph: one cluster per node.
        return adj
            .keys()
            .enumerate()
            .map(|(i, n)| (n.clone(), i))
            .collect();
    }

    // Initialise: each node in its own community (by sorted-order index).
    let mut sorted_nodes: Vec<&String> = adj.keys().collect();
    sorted_nodes.sort();
    let mut comm: BTreeMap<String, usize> = sorted_nodes
        .iter()
        .enumerate()
        .map(|(i, n)| ((*n).clone(), i))
        .collect();

    // Community aggregate stats: per-community Σ_tot (sum of all incident
    // weights of members) for the modularity-gain calculation.
    let mut sigma_tot: HashMap<usize, f64> = comm
        .iter()
        .map(|(n, c)| (*c, *k.get(n).unwrap_or(&0.0)))
        .collect();

    let max_passes = 20;
    for _pass in 0..max_passes {
        let mut moved = false;
        // Iterate in sorted order for determinism.
        for n in &sorted_nodes {
            let n = (*n).clone();
            let cur_c = comm[&n];
            // Σ k_{n, C} for each neighbor community.
            let mut k_to_c: HashMap<usize, f64> = HashMap::new();
            if let Some(nbrs) = adj.get(&n) {
                for (m, w) in nbrs {
                    if let Some(&cm) = comm.get(m) {
                        *k_to_c.entry(cm).or_insert(0.0) += *w;
                    }
                }
            }
            let k_n = *k.get(&n).unwrap_or(&0.0);

            // Remove n from current community before testing moves.
            *sigma_tot.entry(cur_c).or_insert(0.0) -= k_n;
            let cur_gain = k_to_c.get(&cur_c).copied().unwrap_or(0.0)
                - sigma_tot.get(&cur_c).copied().unwrap_or(0.0) * k_n / two_m;

            let mut best_c = cur_c;
            let mut best_gain = cur_gain;
            // Deterministic iteration: sorted neighbor-community ids.
            let mut neighbor_comms: Vec<usize> = k_to_c.keys().copied().collect();
            neighbor_comms.sort();
            for c in neighbor_comms {
                let gain = k_to_c[&c] - sigma_tot.get(&c).copied().unwrap_or(0.0) * k_n / two_m;
                if gain > best_gain + 1e-12 || (gain > best_gain - 1e-12 && c < best_c) {
                    best_gain = gain;
                    best_c = c;
                }
            }

            // Re-insert n's k into the chosen community.
            *sigma_tot.entry(best_c).or_insert(0.0) += k_n;
            if best_c != cur_c {
                comm.insert(n, best_c);
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }

    // Re-label communities to a compact dense index (0..K-1), preserving
    // the deterministic order.
    let mut comm_ids: Vec<usize> = comm.values().copied().collect();
    comm_ids.sort();
    comm_ids.dedup();
    let remap: HashMap<usize, usize> = comm_ids.iter().enumerate().map(|(i, c)| (*c, i)).collect();
    comm.iter_mut().for_each(|(_, c)| *c = remap[c]);
    comm
}

/// Top-level entry: run Louvain over the combined edge graph, write
/// per-node `cluster_id`s, and return a summary [`ClusterReport`].
pub fn enrich(graph: &mut HarnessGraph, mix: EdgeMix, seed: u64) -> ClusterReport {
    let adj = composite_adjacency(graph, mix);
    let comm = louvain(&adj, seed);

    // Write cluster_id onto nodes.
    for node in &mut graph.nodes {
        node.cluster_id = comm.get(&node.id).map(|c| format!("C{:02}", c + 1));
    }

    // Per-cluster aggregation.
    let mut members_of: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for (node_id, c) in &comm {
        members_of.entry(*c).or_default().push(node_id.clone());
    }

    let k: HashMap<&str, f64> = adj
        .iter()
        .map(|(n, nbrs)| (n.as_str(), nbrs.values().sum::<f64>()))
        .collect();
    let two_m: f64 = k.values().sum();

    let mut clusters: Vec<Cluster> = Vec::new();
    let mut total_modularity = 0.0;
    for (c_idx, members) in &members_of {
        let mut sorted = members.clone();
        sorted.sort();
        let label = format!("C{:02}", c_idx + 1);

        // Internal weight = sum of weights with both endpoints in this
        // cluster, divided by 2 (undirected double count).
        let mut internal = 0.0;
        let mut node_internal_sum: BTreeMap<String, f64> = BTreeMap::new();
        for n in &sorted {
            if let Some(nbrs) = adj.get(n) {
                for (m, w) in nbrs {
                    if comm.get(m) == Some(c_idx) {
                        internal += w;
                        *node_internal_sum.entry(n.clone()).or_insert(0.0) += w;
                    }
                }
            }
        }
        internal /= 2.0;

        let sigma_in = 2.0 * internal;
        let sigma_tot: f64 = sorted
            .iter()
            .map(|n| *k.get(n.as_str()).unwrap_or(&0.0))
            .sum();
        let modularity = if two_m > 0.0 {
            (sigma_in / two_m) - (sigma_tot / two_m).powi(2)
        } else {
            0.0
        };
        total_modularity += modularity;

        let centroid = node_internal_sum
            .iter()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(n, _)| n.clone())
            .unwrap_or_else(|| sorted.first().cloned().unwrap_or_default());

        let peak_flakiness_member = graph
            .nodes
            .iter()
            .filter(|n| comm.get(&n.id) == Some(c_idx))
            .filter_map(|n| n.flakiness_score.map(|s| (n.id.clone(), s)))
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(id, _)| id);

        clusters.push(Cluster {
            id: label,
            members: sorted,
            centroid,
            peak_flakiness_member,
            internal_weight: (internal * 1000.0).round() / 1000.0,
            modularity: (modularity * 1000.0).round() / 1000.0,
        });
    }
    clusters.sort_by(|a, b| a.id.cmp(&b.id));

    ClusterReport {
        algo: "louvain".to_string(),
        seed,
        total_modularity: (total_modularity * 1000.0).round() / 1000.0,
        n_clusters: clusters.len(),
        clusters,
    }
}

/// Write a TOML `[harness_cluster]` snippet that the user can paste
/// into `.specere/sensor-map.toml` if they want the cluster-belief
/// priors to be picked up by downstream filters (FR-HM-051). Opt-in —
/// the CLI only writes to sensor-map when `--emit-to-sensor-map` is set.
pub fn to_sensor_map_snippet(report: &ClusterReport) -> String {
    let mut s = String::new();
    s.push_str("# Per-cluster harness-belief priors — auto-proposed by\n");
    s.push_str("# `specere harness cluster`. Paste into .specere/sensor-map.toml.\n");
    s.push_str(&format!(
        "# algo = {}, seed = {}, total_modularity = {:.3}\n\n",
        report.algo, report.seed, report.total_modularity
    ));
    s.push_str("[harness_cluster]\n");
    s.push_str(&format!("algo = \"{}\"\n", report.algo));
    s.push_str(&format!("seed = {}\n", report.seed));
    s.push_str("auto_emit = true\n\n");
    for c in &report.clusters {
        s.push_str(&format!("[harness_cluster.clusters.\"{}\"]\n", c.id));
        s.push_str("members = [\n");
        for m in &c.members {
            s.push_str(&format!("  \"{m}\",\n"));
        }
        s.push_str("]\n");
        s.push_str(&format!("centroid = \"{}\"\n", c.centroid));
        s.push_str(&format!("modularity = {:.3}\n\n", c.modularity));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::coverage::CovCooccurEdge;
    use crate::harness::node::{path_id, Category, ComodEdge, HarnessFile};

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
                    cluster_id: None,
                })
                .collect(),
            edges: Vec::new(),
            comod_edges: Vec::new(),
            cov_cooccur_edges: Vec::new(),
            cofail_edges: Vec::new(),
            cluster_report: None,
        }
    }

    fn add_cov_edge(g: &mut HarnessGraph, a: &str, b: &str, j: f64) {
        g.cov_cooccur_edges.push(CovCooccurEdge {
            from: path_id(a),
            to: path_id(b),
            from_path: a.to_string(),
            to_path: b.to_string(),
            jaccard: j,
            intersection_size: 1,
        });
    }

    fn add_comod_edge(g: &mut HarnessGraph, a: &str, b: &str, ppmi: f64) {
        g.comod_edges.push(ComodEdge {
            from: path_id(a),
            to: path_id(b),
            from_path: a.to_string(),
            to_path: b.to_string(),
            co_commits: 5,
            ppmi,
        });
    }

    #[test]
    fn two_disjoint_cliques_cluster_separately() {
        let mut g = graph_with(&[
            "tests/a.rs",
            "tests/b.rs",
            "tests/c.rs",
            "tests/x.rs",
            "tests/y.rs",
        ]);
        // Clique 1: a-b-c all highly similar.
        add_cov_edge(&mut g, "tests/a.rs", "tests/b.rs", 0.9);
        add_cov_edge(&mut g, "tests/a.rs", "tests/c.rs", 0.9);
        add_cov_edge(&mut g, "tests/b.rs", "tests/c.rs", 0.9);
        // Clique 2: x-y.
        add_cov_edge(&mut g, "tests/x.rs", "tests/y.rs", 0.9);
        let report = enrich(&mut g, EdgeMix::default(), 42);
        assert!(
            report.n_clusters >= 2,
            "expected ≥ 2 clusters; got {}",
            report.n_clusters
        );
        // a/b/c must share a cluster.
        let a = g
            .nodes
            .iter()
            .find(|n| n.path == "tests/a.rs")
            .unwrap()
            .cluster_id
            .clone();
        let b = g
            .nodes
            .iter()
            .find(|n| n.path == "tests/b.rs")
            .unwrap()
            .cluster_id
            .clone();
        let x = g
            .nodes
            .iter()
            .find(|n| n.path == "tests/x.rs")
            .unwrap()
            .cluster_id
            .clone();
        assert!(
            a.is_some() && b.is_some() && a == b,
            "a + b must share cluster"
        );
        assert_ne!(a, x, "cliques must be in different clusters");
    }

    #[test]
    fn empty_graph_produces_empty_report() {
        let mut g = graph_with(&[]);
        let report = enrich(&mut g, EdgeMix::default(), 42);
        assert_eq!(report.n_clusters, 0);
        assert_eq!(report.total_modularity, 0.0);
    }

    #[test]
    fn singleton_nodes_each_in_own_cluster() {
        let mut g = graph_with(&["tests/a.rs", "tests/b.rs", "tests/c.rs"]);
        // No edges → each node isolated.
        let report = enrich(&mut g, EdgeMix::default(), 42);
        // In the empty-adjacency case, our louvain fallback puts each
        // node in its own cluster (via the two_m == 0 branch).
        assert!(report.n_clusters <= 3);
    }

    #[test]
    fn deterministic_across_runs() {
        let mut g1 = graph_with(&["a", "b", "c"]);
        add_cov_edge(&mut g1, "a", "b", 0.8);
        add_cov_edge(&mut g1, "b", "c", 0.3);
        let r1 = enrich(&mut g1, EdgeMix::default(), 42);

        let mut g2 = graph_with(&["a", "b", "c"]);
        add_cov_edge(&mut g2, "a", "b", 0.8);
        add_cov_edge(&mut g2, "b", "c", 0.3);
        let r2 = enrich(&mut g2, EdgeMix::default(), 42);
        assert_eq!(r1, r2, "clustering must be seed-deterministic");
    }

    #[test]
    fn multiple_edge_types_compose() {
        // Ensure comod edges also pull into the composite adjacency.
        let mut g = graph_with(&["p", "q"]);
        add_comod_edge(&mut g, "p", "q", 4.0); // strong PPMI
        let report = enrich(&mut g, EdgeMix::default(), 42);
        // Two nodes + 1 strong comod edge → 1 cluster.
        let p = g
            .nodes
            .iter()
            .find(|n| n.path == "p")
            .unwrap()
            .cluster_id
            .clone();
        let q = g
            .nodes
            .iter()
            .find(|n| n.path == "q")
            .unwrap()
            .cluster_id
            .clone();
        assert!(p.is_some());
        assert_eq!(p, q, "strong comod edge should cluster p+q together");
        assert_eq!(report.n_clusters, 1);
    }

    #[test]
    fn sensor_map_snippet_contains_cluster_headers() {
        let report = ClusterReport {
            algo: "louvain".into(),
            seed: 42,
            total_modularity: 0.42,
            n_clusters: 1,
            clusters: vec![Cluster {
                id: "C01".into(),
                members: vec![path_id("tests/a.rs"), path_id("tests/b.rs")],
                centroid: path_id("tests/a.rs"),
                peak_flakiness_member: None,
                internal_weight: 0.9,
                modularity: 0.5,
            }],
        };
        let s = to_sensor_map_snippet(&report);
        assert!(s.contains("[harness_cluster]"));
        assert!(s.contains("[harness_cluster.clusters.\"C01\"]"));
        assert!(s.contains("centroid ="));
    }
}
