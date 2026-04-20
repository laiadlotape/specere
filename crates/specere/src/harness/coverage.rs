//! Coverage co-execution (FR-HM-030..033, S4).
//!
//! For each test (`#[test]` / `#[bench]` / property / fuzz), we collect a
//! *coverage bitvector* — the set of production-source lines the test
//! exercised. Two tests with highly-overlapping bitvectors likely exercise
//! the same code path; their Jaccard similarity is the `cov_cooccur` edge
//! weight.
//!
//! Pipeline:
//!
//! 1. `cargo-llvm-cov nextest --no-report --lcov --output-path <path>`
//!    aggregates coverage across all tests. Per-test bitvectors require
//!    one LCOV per test — either `cargo llvm-cov run --test <name>`
//!    iteratively, or post-hoc split of profraw files. We ship both
//!    paths today: **fixture-driven** via `--from-lcov-dir <dir>` (one
//!    `.lcov` per test, named `<test>.lcov`) for CI + unit tests, and
//!    **live** via the subprocess wrapper for repos with llvm tools.
//!
//! 2. [`parse_lcov`] reads one LCOV file → `BTreeMap<source_file,
//!    BitSet<line>>`.
//!
//! 3. [`compute_bitvector_hash`] derives the `coverage_hash` stored on
//!    the harness node (blake-style digest of the sorted line-hit set).
//!
//! 4. [`jaccard`] pairs all tests and emits `cov_cooccur` edges when
//!    `J >= --threshold` (default 0.1 — below that, noise dominates).
//!
//! We deliberately do **not** support profraw merging in this slice —
//! LCOV is the stable interchange format, and any post-llvm-cov
//! pipeline can emit it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::harness::node::{HarnessFile, HarnessGraph};

/// Default Jaccard threshold for emitting a `cov_cooccur` edge.
#[allow(dead_code)]
pub const DEFAULT_J_THRESHOLD: f64 = 0.1;

/// Per-test coverage footprint: which production-source lines the test
/// touched, keyed by repo-relative path.
#[derive(Debug, Clone, Default)]
pub struct TestCoverage {
    /// Test identifier — a harness-file path, or for more granular
    /// callers, `<file>::<test_name>`. Stored raw; the caller decides.
    #[allow(dead_code)]
    pub test_id: String,
    /// Line-hit sets per source file.
    pub hits: BTreeMap<String, BTreeSet<u32>>,
}

impl TestCoverage {
    /// Total number of line hits across all source files (the bitvector's
    /// cardinality). Used as the Jaccard denominator.
    #[allow(dead_code)]
    pub fn cardinality(&self) -> usize {
        self.hits.values().map(|s| s.len()).sum()
    }

    /// Deterministic SHA-256 hex digest of the sorted `(file, line)` list.
    /// Stored in the harness node so downstream slices (S6 clustering)
    /// can check whether coverage has changed without re-running tests.
    pub fn bitvector_hash(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        for (file, lines) in &self.hits {
            h.update(file.as_bytes());
            h.update(b":");
            for line in lines {
                h.update(line.to_le_bytes());
            }
            h.update(b"\n");
        }
        hex::encode(&h.finalize()[..16])
    }

    /// Union of all covered `(file, line)` pairs. Used to compute Jaccard
    /// intersections with another TestCoverage.
    pub fn covered_pairs(&self) -> BTreeSet<(String, u32)> {
        let mut s = BTreeSet::new();
        for (file, lines) in &self.hits {
            for line in lines {
                s.insert((file.clone(), *line));
            }
        }
        s
    }
}

/// `cov_cooccur` edge between two tests (undirected; stored with
/// `from < to` by harness-node id).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CovCooccurEdge {
    pub from: String,
    pub to: String,
    pub from_path: String,
    pub to_path: String,
    /// `|A ∩ B| / |A ∪ B|` over covered `(file, line)` pairs.
    pub jaccard: f64,
    /// Number of distinct `(file, line)` pairs in the intersection.
    pub intersection_size: u32,
}

/// Parse one LCOV file into a [`TestCoverage`]. We only honour the
/// `SF:` (source file) and `DA:<line>,<count>` (per-line hit count) keys
/// — the other LCOV fields (`FN`, `FNF`, `BRDA`, …) are ignored. Lines
/// with `<count> == 0` are dropped; we only record *hits*, not misses.
pub fn parse_lcov(test_id: &str, raw: &str) -> TestCoverage {
    let mut hits: BTreeMap<String, BTreeSet<u32>> = BTreeMap::new();
    let mut cur_file: Option<String> = None;
    for line in raw.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("SF:") {
            cur_file = Some(rest.to_string());
        } else if let Some(rest) = line.strip_prefix("DA:") {
            // Format: DA:<line>,<count>[,<checksum>]
            let mut parts = rest.split(',');
            let line_no: u32 = match parts.next().and_then(|s| s.parse().ok()) {
                Some(n) => n,
                None => continue,
            };
            let count: u64 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            if count == 0 {
                continue;
            }
            if let Some(f) = &cur_file {
                hits.entry(f.clone()).or_default().insert(line_no);
            }
        } else if line == "end_of_record" {
            cur_file = None;
        }
    }
    TestCoverage {
        test_id: test_id.to_string(),
        hits,
    }
}

/// Jaccard similarity over covered `(file, line)` pairs.
/// `0.0` for disjoint; `1.0` for identical; undefined (reported as `0.0`)
/// for two empty coverages.
pub fn jaccard(a: &TestCoverage, b: &TestCoverage) -> (f64, u32) {
    let pa = a.covered_pairs();
    let pb = b.covered_pairs();
    let inter: usize = pa.intersection(&pb).count();
    let union: usize = pa.len() + pb.len() - inter;
    if union == 0 {
        return (0.0, 0);
    }
    (inter as f64 / union as f64, inter as u32)
}

/// Enrich `graph` with `cov_cooccur_edges` + per-node `coverage_hash`
/// attributes. `coverages` is the pre-loaded set of one TestCoverage
/// per harness node (loaded from LCOV fixtures, or from a live run).
pub fn enrich(
    graph: &mut HarnessGraph,
    coverages: &BTreeMap<String, TestCoverage>,
    threshold: f64,
) -> CoverageReport {
    let mut report = CoverageReport::default();

    // First pass: write coverage_hash onto each node whose test_id matches
    // the node's path. For tests with multiple fixtures (one test = one
    // LCOV file) this is one-to-one.
    for node in &mut graph.nodes {
        if let Some(cov) = coverages.get(&node.path) {
            node.coverage_hash = Some(cov.bitvector_hash());
            report.nodes_enriched += 1;
        }
    }

    // Second pass: pairwise Jaccard.
    let path_to_id: BTreeMap<&str, &str> = graph
        .nodes
        .iter()
        .map(|n| (n.path.as_str(), n.id.as_str()))
        .collect();
    let keys: Vec<&String> = coverages.keys().collect();
    for i in 0..keys.len() {
        for j in (i + 1)..keys.len() {
            let a_path = keys[i].as_str();
            let b_path = keys[j].as_str();
            let a_id = match path_to_id.get(a_path) {
                Some(id) => *id,
                None => continue,
            };
            let b_id = match path_to_id.get(b_path) {
                Some(id) => *id,
                None => continue,
            };
            let cov_a = &coverages[keys[i]];
            let cov_b = &coverages[keys[j]];
            let (j_score, inter_size) = jaccard(cov_a, cov_b);
            if j_score < threshold {
                continue;
            }
            let (from, to, from_path, to_path) = if a_id < b_id {
                (a_id, b_id, a_path, b_path)
            } else {
                (b_id, a_id, b_path, a_path)
            };
            graph.cov_cooccur_edges.push(CovCooccurEdge {
                from: from.to_string(),
                to: to.to_string(),
                from_path: from_path.to_string(),
                to_path: to_path.to_string(),
                jaccard: (j_score * 1000.0).round() / 1000.0,
                intersection_size: inter_size,
            });
            report.edges_emitted += 1;
        }
    }
    // Dedupe + deterministic sort.
    graph
        .cov_cooccur_edges
        .sort_by(|a, b| a.from.cmp(&b.from).then_with(|| a.to.cmp(&b.to)));
    graph.cov_cooccur_edges.dedup();

    report
}

/// Load a directory of per-test `.lcov` files. Each file is named
/// `<test_rel_path>.lcov` (with `/` replaced by `__`, since filenames
/// can't contain `/`). Returns `test_path → TestCoverage`.
pub fn load_lcov_dir(dir: &Path) -> Result<BTreeMap<String, TestCoverage>> {
    let mut out = BTreeMap::new();
    for entry in std::fs::read_dir(dir).with_context(|| format!("read_dir {}", dir.display()))? {
        let entry = entry?;
        if entry.path().extension().and_then(|e| e.to_str()) != Some("lcov") {
            continue;
        }
        let raw = std::fs::read_to_string(entry.path())
            .with_context(|| format!("read {}", entry.path().display()))?;
        // Filename pattern: <escaped-path>.lcov → restore `/` and drop `.lcov`.
        let stem = entry
            .path()
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let test_path = stem.replace("__", "/");
        out.insert(test_path.clone(), parse_lcov(&test_path, &raw));
    }
    Ok(out)
}

/// Live path: run `cargo llvm-cov nextest --no-report --lcov
/// --output-path <dir>/coverage.lcov` once, parse the aggregate, and
/// return a single-test `TestCoverage` keyed to the repo root. Per-test
/// granularity requires the fixture path (see [`load_lcov_dir`]).
///
/// Best-effort: if `cargo-llvm-cov` isn't on `$PATH`, returns
/// `Err` with a friendly message.
pub fn run_live_coverage(repo: &Path) -> Result<TestCoverage> {
    use std::process::Command;
    let tmp = repo.join(".specere").join("coverage.lcov");
    if let Some(parent) = tmp.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let out = Command::new("cargo")
        .current_dir(repo)
        .args([
            "llvm-cov",
            "nextest",
            "--no-report",
            "--lcov",
            "--output-path",
        ])
        .arg(&tmp)
        .output()
        .context(
            "failed to spawn `cargo llvm-cov` — install with `cargo install cargo-llvm-cov`",
        )?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        anyhow::bail!(
            "cargo llvm-cov failed:\n{stderr}\n\
             (install with `cargo install cargo-llvm-cov`; also needs `rustup component add llvm-tools-preview`)"
        );
    }
    let raw = std::fs::read_to_string(&tmp).with_context(|| format!("read {}", tmp.display()))?;
    Ok(parse_lcov("aggregate", &raw))
}

/// Summary of the enrich pass.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct CoverageReport {
    pub nodes_enriched: usize,
    pub edges_emitted: usize,
}

/// Read `[specere.coverage] enabled = true` from sensor-map.toml.
/// Default: `false`. The flag gates *automatic* coverage collection on
/// other CLI verbs (e.g. `specere harness scan` could opt-in to run S4
/// at the end); on-demand invocation via `specere harness coverage`
/// always runs regardless.
#[allow(dead_code)]
pub fn coverage_enabled(sensor_map_path: &Path) -> bool {
    let raw = match std::fs::read_to_string(sensor_map_path) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let val: toml::Value = match toml::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return false,
    };
    val.get("specere")
        .and_then(|s| s.get("coverage"))
        .and_then(|c| c.get("enabled"))
        .and_then(|e| e.as_bool())
        .unwrap_or(false)
}

// ──────────────────────────────────────────────────────────────────────
//  Node-graph plumbing
// ──────────────────────────────────────────────────────────────────────

/// Convenience: add `coverage_hash` to existing `HarnessFile` via an
/// accessor that tolerates the older (S1-shaped) TOML schemas where the
/// field is absent.
#[allow(dead_code)]
pub fn coverage_hash_for(file: &HarnessFile) -> Option<&str> {
    file.coverage_hash.as_deref()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cov(test_id: &str, pairs: &[(&str, u32)]) -> TestCoverage {
        let mut hits: BTreeMap<String, BTreeSet<u32>> = BTreeMap::new();
        for (file, line) in pairs {
            hits.entry((*file).to_string()).or_default().insert(*line);
        }
        TestCoverage {
            test_id: test_id.to_string(),
            hits,
        }
    }

    #[test]
    fn parse_lcov_minimal() {
        let raw = "SF:src/lib.rs\n\
                   DA:10,1\n\
                   DA:20,3\n\
                   DA:30,0\n\
                   end_of_record\n";
        let c = parse_lcov("t1", raw);
        let lines = c.hits.get("src/lib.rs").unwrap();
        assert!(lines.contains(&10));
        assert!(lines.contains(&20));
        assert!(!lines.contains(&30), "zero-hit lines dropped");
        assert_eq!(c.cardinality(), 2);
    }

    #[test]
    fn parse_lcov_multiple_files() {
        let raw = "SF:src/a.rs\n\
                   DA:1,1\n\
                   end_of_record\n\
                   SF:src/b.rs\n\
                   DA:5,2\n\
                   DA:6,1\n\
                   end_of_record\n";
        let c = parse_lcov("t", raw);
        assert_eq!(c.hits.len(), 2);
        assert_eq!(c.hits["src/a.rs"].len(), 1);
        assert_eq!(c.hits["src/b.rs"].len(), 2);
    }

    #[test]
    fn parse_lcov_with_checksum() {
        // Some LCOV writers append a checksum as a third field.
        let raw = "SF:x.rs\nDA:1,4,abc123\nDA:2,1\nend_of_record\n";
        let c = parse_lcov("t", raw);
        assert_eq!(c.hits["x.rs"].len(), 2);
    }

    #[test]
    fn jaccard_disjoint_is_zero() {
        let a = cov("a", &[("x.rs", 1), ("x.rs", 2)]);
        let b = cov("b", &[("y.rs", 1), ("y.rs", 2)]);
        let (j, inter) = jaccard(&a, &b);
        assert_eq!(j, 0.0);
        assert_eq!(inter, 0);
    }

    #[test]
    fn jaccard_identical_is_one() {
        let a = cov("a", &[("x.rs", 1), ("x.rs", 2), ("y.rs", 5)]);
        let b = cov("b", &[("x.rs", 1), ("x.rs", 2), ("y.rs", 5)]);
        let (j, inter) = jaccard(&a, &b);
        assert!((j - 1.0).abs() < 1e-9);
        assert_eq!(inter, 3);
    }

    #[test]
    fn jaccard_partial_overlap() {
        let a = cov("a", &[("x.rs", 1), ("x.rs", 2), ("x.rs", 3)]);
        let b = cov("b", &[("x.rs", 2), ("x.rs", 3), ("x.rs", 4)]);
        // |A ∩ B| = 2, |A ∪ B| = 4 → 0.5
        let (j, inter) = jaccard(&a, &b);
        assert!((j - 0.5).abs() < 1e-9, "got {j}");
        assert_eq!(inter, 2);
    }

    #[test]
    fn jaccard_empty_both() {
        let a = cov("a", &[]);
        let b = cov("b", &[]);
        let (j, inter) = jaccard(&a, &b);
        assert_eq!(j, 0.0);
        assert_eq!(inter, 0);
    }

    #[test]
    fn bitvector_hash_is_deterministic() {
        let a = cov("a", &[("x.rs", 1), ("y.rs", 10)]);
        let b = cov("a", &[("y.rs", 10), ("x.rs", 1)]); // same pairs, inserted in different order
        assert_eq!(a.bitvector_hash(), b.bitvector_hash());
        let c = cov("c", &[("x.rs", 1)]);
        assert_ne!(a.bitvector_hash(), c.bitvector_hash());
    }

    #[test]
    fn coverage_enabled_defaults_false() {
        let dir = tempfile::TempDir::new().unwrap();
        let sm = dir.path().join("sensor-map.toml");
        std::fs::write(&sm, "schema_version = 1\n[specs]\n").unwrap();
        assert!(!coverage_enabled(&sm));
    }

    #[test]
    fn coverage_enabled_true_when_set() {
        let dir = tempfile::TempDir::new().unwrap();
        let sm = dir.path().join("sensor-map.toml");
        std::fs::write(
            &sm,
            "schema_version = 1\n[specs]\n\n[specere.coverage]\nenabled = true\n",
        )
        .unwrap();
        assert!(coverage_enabled(&sm));
    }

    #[test]
    fn load_lcov_dir_reads_all_files() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("tests__it.lcov"),
            "SF:src/lib.rs\nDA:1,1\nend_of_record\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("tests__other.lcov"),
            "SF:src/lib.rs\nDA:2,1\nend_of_record\n",
        )
        .unwrap();
        let loaded = load_lcov_dir(dir.path()).unwrap();
        assert!(loaded.contains_key("tests/it"));
        assert!(loaded.contains_key("tests/other"));
    }
}
