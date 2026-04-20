//! Harness-file node model + on-disk TOML serialisation (FR-HM-004).
//!
//! The node is a **path-stable, hash-derived ID** for S1 (renames break
//! it; S3 adds `git log --follow` lineage and `prior_paths`). The TOML
//! writer sorts deterministically so repeated scans of the same repo
//! state produce byte-identical output — a prerequisite for diffing
//! graph changes over time and for Gate-A-style bit-parity tests.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Nine harness categories (see `docs/proposals/v3-harness-manager.md` §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Unit,
    Integration,
    Property,
    Fuzz,
    Bench,
    Snapshot,
    Golden,
    Mock,
    Fixture,
    Workflow,
    /// Non-harness file that still shows up in walks (we record it so the
    /// direct-use edge target list is closed under the walk).
    Production,
}

impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unit => "unit",
            Self::Integration => "integration",
            Self::Property => "property",
            Self::Fuzz => "fuzz",
            Self::Bench => "bench",
            Self::Snapshot => "snapshot",
            Self::Golden => "golden",
            Self::Mock => "mock",
            Self::Fixture => "fixture",
            Self::Workflow => "workflow",
            Self::Production => "production",
        }
    }
}

/// Stable 16-char hex ID derived from the repo-relative path. Used as a
/// deterministic primary key; consumers that care about rename tracking
/// use `path` + the `prior_paths` list (populated in S3, not S1).
pub fn path_id(rel_path: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(rel_path.as_bytes());
    let full = h.finalize();
    hex::encode(&full[..8])
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HarnessFile {
    /// Stable 16-char hex ID; `blake-free` — SHA-256 first 8 bytes of path.
    pub id: String,
    /// Repo-relative, forward-slash path (Windows backslashes are normalised).
    pub path: String,
    pub category: Category,
    /// 0..=1 — 1.0 for path-convention matches, lower when classification
    /// falls back to AST heuristics on ambiguous files.
    pub category_confidence: f64,
    /// Cargo crate owning this file; `None` for non-cargo files (workflows,
    /// justfile, root-level docs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub crate_name: Option<String>,
    /// Extracted `#[test]` / `#[bench]` / `proptest!{}` / `fuzz_target!` names.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub test_names: Vec<String>,
    /// Populated by `specere harness provenance` (S2). `None` on first scan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Provenance>,
    /// Populated by `specere harness history` (S3). `None` on first scan or
    /// when the repo has no git history (shallow clone, fresh init).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_metrics: Option<VersionMetrics>,
    /// Populated by `specere harness coverage` (S4). `None` on first scan
    /// or when coverage collection is disabled (`[specere.coverage]
    /// enabled = false`). 16-char hex digest of the test's bitvector.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage_hash: Option<String>,
    /// Populated by `specere harness flaky` (S5). `P(fail)` across the
    /// collected `test × run` matrix. `None` until ≥ `--min-runs` (default
    /// 50) runs have accumulated — same insufficient-history pattern as
    /// FR-EQ-004.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flakiness_score: Option<f64>,
}

/// Per-file git-history metrics (FR-HM-020). Computed by
/// `specere harness history` from `git log --numstat --follow`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct VersionMetrics {
    /// Number of days since the file's introducing commit.
    pub age_days: u32,
    /// Total commits that touched the file (follows renames).
    pub commits: u32,
    /// Distinct author emails in the commit history.
    pub authors: u32,
    /// Normalised churn = (lines_added + lines_deleted) / commits, clamped at
    /// 2 decimals. A churn of 0.0 means the file has been added once and
    /// never edited; 50.0+ is a high-volatility hotspot candidate.
    pub churn_rate: f64,
    /// Most-recent commit timestamp (RFC-3339).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_touched: Option<String>,
    /// `1` when one author wrote ≥ 80% of the lines; higher when ownership is
    /// distributed. Rough proxy for bus-factor (FR-HM-022 hotspot scoring).
    pub bus_factor: u32,
    /// `hotspot_score = (churn_rate × log(commits + 1)) / (age_days + 1)`.
    /// Surfaces files that are both volatile AND old enough to have earned
    /// test-rot debt. Used for the top-N list in the CLI output.
    pub hotspot_score: f64,
}

/// Who/what created this file, and when. Dual fields handle the common
/// case of an agent-authored-file committed by a human.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Provenance {
    /// Workflow-span event that first claimed `files_created` on this file
    /// (or `files_touched` without a prior span, as a fallback).
    /// Empty when only git-log attribution is available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creator_span_id: Option<String>,
    /// `specere.workflow_step` attribute of the claiming span, e.g.
    /// `implement`, `plan`, `specify`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creator_verb: Option<String>,
    /// `gen_ai.system` from the claiming span (e.g. `claude-code`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creator_agent: Option<String>,
    /// First FR-id from the claiming span's `specere.fr_ids` attr.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creator_spec: Option<String>,
    /// Git commit SHA that introduced the file (`git log --follow --diff-filter=A`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creator_commit: Option<String>,
    /// Human committer email from the introducing commit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creator_human: Option<String>,
    /// ISO-8601 creation timestamp (RFC3339). Source is either the claiming
    /// span or the introducing commit's author date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// When `true`, the file has human-authored lines that materially
    /// diverge from agent-authored (see FR-HM-012).
    #[serde(default)]
    pub divergence_detected: bool,
}

/// Direct-use edge produced by parsing a `.d` file (FR-HM-003).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DirectEdge {
    /// Source node id.
    pub from: String,
    /// Destination node id.
    pub to: String,
    /// Repo-relative paths the edge maps to (for debugging / display only;
    /// the ids are the authoritative keys).
    pub from_path: String,
    pub to_path: String,
}

/// Co-modification edge (FR-HM-021). Undirected; stored once per pair
/// with `from < to` by node id.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComodEdge {
    pub from: String,
    pub to: String,
    pub from_path: String,
    pub to_path: String,
    /// Number of commits where both files changed together.
    pub co_commits: u32,
    /// PPMI score — `log2(p(a,b) / (p(a) · p(b)))`, truncated at 0.
    pub ppmi: f64,
}

/// Schema-versioned graph container — one TOML file at
/// `.specere/harness-graph.toml`. Nodes sorted by id; edges by `(from, to)`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HarnessGraph {
    pub schema_version: u32,
    #[serde(default)]
    pub nodes: Vec<HarnessFile>,
    #[serde(default)]
    pub edges: Vec<DirectEdge>,
    /// Populated by `specere harness history` (FR-HM-021).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub comod_edges: Vec<ComodEdge>,
    /// Populated by `specere harness coverage` (FR-HM-032).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cov_cooccur_edges: Vec<crate::harness::coverage::CovCooccurEdge>,
    /// Populated by `specere harness flaky` (FR-HM-041).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cofail_edges: Vec<crate::harness::flaky::CofailEdge>,
}

impl HarnessGraph {
    /// Deterministic write: sort entries then serialise via `toml`.
    pub fn write_atomic(&mut self, path: &Path) -> Result<()> {
        self.nodes.sort_by(|a, b| a.id.cmp(&b.id));
        self.edges
            .sort_by(|a, b| a.from.cmp(&b.from).then_with(|| a.to.cmp(&b.to)));
        self.comod_edges
            .sort_by(|a, b| a.from.cmp(&b.from).then_with(|| a.to.cmp(&b.to)));
        self.cov_cooccur_edges
            .sort_by(|a, b| a.from.cmp(&b.from).then_with(|| a.to.cmp(&b.to)));
        self.cofail_edges
            .sort_by(|a, b| a.from.cmp(&b.from).then_with(|| a.to.cmp(&b.to)));
        let serialised = toml::to_string_pretty(self).context("serialise harness graph")?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, serialised.as_bytes())
            .with_context(|| format!("write {}", tmp.display()))?;
        std::fs::rename(&tmp, path)
            .with_context(|| format!("rename {} -> {}", tmp.display(), path.display()))?;
        Ok(())
    }

    /// Map of path → id for edge-attribution joins in dep-info parsing.
    #[allow(dead_code)]
    pub fn path_index(&self) -> BTreeMap<String, String> {
        self.nodes
            .iter()
            .map(|n| (n.path.clone(), n.id.clone()))
            .collect()
    }

    /// Re-load from an on-disk file; returns an empty graph if missing.
    #[allow(dead_code)]
    pub fn load_or_default(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self {
                schema_version: 1,
                nodes: Vec::new(),
                edges: Vec::new(),
                comod_edges: Vec::new(),
                cov_cooccur_edges: Vec::new(),
                cofail_edges: Vec::new(),
            });
        }
        let raw =
            std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        let g: Self = toml::from_str(&raw).context("parse harness-graph.toml")?;
        Ok(g)
    }
}

/// Normalise a filesystem path to repo-relative, forward-slash form.
pub fn repo_rel(repo: &Path, abs: &Path) -> Option<String> {
    let rel = abs.strip_prefix(repo).ok()?;
    let mut s = rel.to_string_lossy().to_string();
    if std::path::MAIN_SEPARATOR != '/' {
        s = s.replace(std::path::MAIN_SEPARATOR, "/");
    }
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn path_id_is_deterministic() {
        let a = path_id("tests/foo.rs");
        let b = path_id("tests/foo.rs");
        assert_eq!(a, b, "same path must hash to same id");
        let c = path_id("tests/bar.rs");
        assert_ne!(a, c, "different paths must hash differently");
        assert_eq!(a.len(), 16, "id is 16 hex chars (8 bytes of SHA-256)");
    }

    #[test]
    fn round_trip_preserves_content() {
        let mut g = HarnessGraph {
            schema_version: 1,
            nodes: vec![HarnessFile {
                id: path_id("tests/a.rs"),
                path: "tests/a.rs".into(),
                category: Category::Integration,
                category_confidence: 1.0,
                crate_name: Some("specere".into()),
                test_names: vec!["test_a".into(), "test_b".into()],
                provenance: None,
                version_metrics: None,
                coverage_hash: None,
                flakiness_score: None,
            }],
            edges: vec![],
            comod_edges: vec![],
            cov_cooccur_edges: vec![],
            cofail_edges: vec![],
        };
        let tmp = tempfile::NamedTempFile::new().unwrap();
        g.write_atomic(tmp.path()).unwrap();
        let loaded = HarnessGraph::load_or_default(tmp.path()).unwrap();
        assert_eq!(g, loaded);
    }

    #[test]
    fn sort_is_stable_across_runs() {
        let nodes: Vec<HarnessFile> = ["zz", "aa", "mm"]
            .iter()
            .map(|p| HarnessFile {
                id: path_id(p),
                path: (*p).into(),
                category: Category::Unit,
                category_confidence: 1.0,
                crate_name: None,
                test_names: Vec::new(),
                provenance: None,
                version_metrics: None,
                coverage_hash: None,
                flakiness_score: None,
            })
            .collect();
        let mut g1 = HarnessGraph {
            schema_version: 1,
            nodes: nodes.clone(),
            edges: Vec::new(),
            comod_edges: Vec::new(),
            cov_cooccur_edges: Vec::new(),
            cofail_edges: Vec::new(),
        };
        let mut g2 = HarnessGraph {
            schema_version: 1,
            nodes: nodes.into_iter().rev().collect(),
            edges: Vec::new(),
            comod_edges: Vec::new(),
            cov_cooccur_edges: Vec::new(),
            cofail_edges: Vec::new(),
        };
        let t1 = tempfile::NamedTempFile::new().unwrap();
        let t2 = tempfile::NamedTempFile::new().unwrap();
        g1.write_atomic(t1.path()).unwrap();
        g2.write_atomic(t2.path()).unwrap();
        let b1 = std::fs::read(t1.path()).unwrap();
        let b2 = std::fs::read(t2.path()).unwrap();
        assert_eq!(b1, b2, "write_atomic must be order-independent");
    }

    #[test]
    fn repo_rel_normalises_separators() {
        let repo = PathBuf::from("/a/b");
        let abs = PathBuf::from("/a/b/c/d.rs");
        assert_eq!(repo_rel(&repo, &abs).unwrap(), "c/d.rs");
    }

    #[test]
    fn category_as_str_covers_all_variants() {
        // Future-proof: if someone adds a variant they must also update
        // `as_str`. Stays cheap because there are only 11 variants.
        for c in [
            Category::Unit,
            Category::Integration,
            Category::Property,
            Category::Fuzz,
            Category::Bench,
            Category::Snapshot,
            Category::Golden,
            Category::Mock,
            Category::Fixture,
            Category::Workflow,
            Category::Production,
        ] {
            assert!(!c.as_str().is_empty());
        }
    }
}
