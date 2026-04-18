//! `specere calibrate from-git` — learn coupling edges from commit history.
//!
//! Phase 5 partial (coupling-edge suggester). Full per-spec motion-matrix
//! fit (`t_good` / `t_bad` per spec via (diff, test-delta) pairs) needs a
//! durable test-history source that v0.5.0 doesn't yet carry — deferred to
//! a later phase.
//!
//! Algorithm:
//! 1. Read `[specs]` from sensor-map.toml (each spec has a file `support`).
//! 2. Walk `git log --name-only` from a caller-controlled depth.
//! 3. For each commit, compute the set of *touched specs* = specs whose
//!    support intersects the commit's modified files.
//! 4. Tally a co-modification count per unordered spec pair.
//! 5. Emit directed edges `[src, dst]` where `co(src, dst) >= min_commits`.
//!    Direction resolves by the prototype's convention: the alphabetically
//!    smaller spec id becomes `src` — the caller can flip any edge they
//!    want before pasting into sensor-map.toml.
//! 6. Emit a DAG: if the alphabetical direction produces a cycle the
//!    lower-confidence edge is dropped.
//!
//! Output is a stable TOML snippet the caller pastes into
//! `.specere/sensor-map.toml`'s `[coupling]` section.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::process::Command;

use anyhow::{anyhow, Context, Result};

use crate::hmm::SpecDescriptor;

/// Configurable knobs for the coupling suggester.
#[derive(Debug, Clone)]
pub struct CalibrateOpts {
    /// How many most-recent commits to analyse. None = unlimited.
    pub max_commits: Option<usize>,
    /// Only propose edges where the co-modification count is at least this.
    /// Default 3 — filters out coincidences but keeps real correlations.
    pub min_commits: usize,
}

impl Default for CalibrateOpts {
    fn default() -> Self {
        Self {
            max_commits: Some(500),
            min_commits: 3,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CoEdge {
    pub src: String,
    pub dst: String,
    pub co_commits: usize,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CalibrationReport {
    /// Commits walked.
    pub commits_analysed: usize,
    /// Commits with ≥ 1 spec touched.
    pub commits_with_spec_activity: usize,
    /// Per-spec commit count.
    pub spec_activity: BTreeMap<String, usize>,
    /// Proposed coupling edges, sorted by `co_commits` descending.
    pub edges: Vec<CoEdge>,
    /// Edges dropped to keep the graph acyclic.
    pub dropped_cycle_edges: Vec<CoEdge>,
}

impl CalibrationReport {
    /// Render a ready-to-paste TOML snippet for `.specere/sensor-map.toml`'s
    /// `[coupling]` table. Empty edges yield a header-only comment.
    pub fn to_toml_snippet(&self) -> String {
        let mut s = String::new();
        s.push_str("# Suggested coupling edges — auto-proposed by\n");
        s.push_str("# `specere calibrate from-git` based on co-modification counts.\n");
        s.push_str(&format!(
            "# Analysed {} commits ({} touched a tracked spec).\n",
            self.commits_analysed, self.commits_with_spec_activity
        ));
        if self.edges.is_empty() {
            s.push_str("# No pairs exceeded the min-commits threshold.\n");
            s.push_str("[coupling]\nedges = []\n");
            return s;
        }
        s.push_str("[coupling]\nedges = [\n");
        for e in &self.edges {
            s.push_str(&format!(
                "  [\"{}\", \"{}\"],  # {} co-commits\n",
                e.src, e.dst, e.co_commits
            ));
        }
        s.push_str("]\n");
        if !self.dropped_cycle_edges.is_empty() {
            s.push_str("\n# Dropped to keep the graph acyclic (loopy BP requires a DAG):\n");
            for e in &self.dropped_cycle_edges {
                s.push_str(&format!(
                    "#   [\"{}\", \"{}\"] ({} co-commits) — would have closed a cycle.\n",
                    e.src, e.dst, e.co_commits
                ));
            }
        }
        s
    }
}

/// Top-level entry. Shells out to `git` (must be on PATH) inside `repo`.
pub fn calibrate_from_git(
    repo: &Path,
    specs: &[SpecDescriptor],
    opts: &CalibrateOpts,
) -> Result<CalibrationReport> {
    if specs.is_empty() {
        return Err(anyhow!(
            "calibrate: no specs provided — sensor-map.toml [specs] is empty"
        ));
    }
    let raw = run_git_log_names(repo, opts.max_commits)?;
    let commits = parse_git_log(&raw);
    compute_report(specs, &commits, opts)
}

fn run_git_log_names(repo: &Path, max_commits: Option<usize>) -> Result<String> {
    // `--pretty=format:---COMMIT---` + `--name-only` gives us one marker per
    // commit followed by its file list. Simpler than JSON but unambiguous
    // because `---COMMIT---` can't legally appear as a filename prefix
    // under git semantics.
    let mut cmd = Command::new("git");
    cmd.current_dir(repo)
        .args(["log", "--pretty=format:---COMMIT---", "--name-only"]);
    if let Some(n) = max_commits {
        cmd.arg(format!("-n{n}"));
    }
    let out = cmd
        .output()
        .with_context(|| format!("spawn `git log` at {}", repo.display()))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        // Friendlier messages for the two common setup errors (audit C-02 /
        // C-11). Fall through to the raw git output for everything else.
        if stderr.contains("does not have any commits") {
            return Err(anyhow!(
                "calibrate: {} has no commits yet — make at least one commit before running `specere calibrate`",
                repo.display()
            ));
        }
        if stderr.contains("not a git repository") {
            return Err(anyhow!(
                "calibrate: {} is not a git repository — run `git init` first",
                repo.display()
            ));
        }
        return Err(anyhow!(
            "`git log` failed at {}: {}",
            repo.display(),
            stderr
        ));
    }
    String::from_utf8(out.stdout).context("git log output was not UTF-8")
}

fn parse_git_log(raw: &str) -> Vec<Vec<String>> {
    let mut commits: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    for line in raw.lines() {
        if line == "---COMMIT---" {
            if !current.is_empty() {
                commits.push(std::mem::take(&mut current));
            }
        } else if !line.trim().is_empty() {
            current.push(line.to_string());
        }
    }
    if !current.is_empty() {
        commits.push(current);
    }
    commits
}

fn compute_report(
    specs: &[SpecDescriptor],
    commits: &[Vec<String>],
    opts: &CalibrateOpts,
) -> Result<CalibrationReport> {
    // Match a commit file against a spec support entry. A file `f` is under
    // a support entry `sup` iff it's an exact file-path match, or it lives
    // inside the directory that `sup` names. We pre-normalise each support
    // entry into a `(sup_no_trailing_slash, sup_with_trailing_slash)` pair
    // and test against both — `f == sup` for exact matches, `f.starts_with(sup_slash)`
    // for directory-prefix matches. This prevents the false-positive where
    // support `src/widget` matches `src/widgetry/x.rs` (audit finding C-13
    // / C-01: `starts_with` without a separator bleeds across sibling paths).
    let spec_ids: Vec<&str> = specs.iter().map(|s| s.id.as_str()).collect();
    let supports_normalised: Vec<Vec<(String, String)>> = specs
        .iter()
        .map(|s| {
            s.support
                .iter()
                .map(|sup| {
                    let bare = sup.trim_end_matches('/').to_string();
                    let dir = format!("{bare}/");
                    (bare, dir)
                })
                .collect()
        })
        .collect();

    let mut spec_activity: BTreeMap<String, usize> = BTreeMap::new();
    // Co-occurrence count keyed by the sorted (src, dst) pair.
    let mut co: HashMap<(String, String), usize> = HashMap::new();
    let mut commits_with_activity = 0usize;

    for files in commits {
        let touched: BTreeSet<&str> = spec_ids
            .iter()
            .enumerate()
            .filter(|(i, _)| {
                supports_normalised[*i].iter().any(|(bare, dir)| {
                    files
                        .iter()
                        .any(|f| f == bare || f.starts_with(dir.as_str()))
                })
            })
            .map(|(_, sid)| *sid)
            .collect();

        if touched.is_empty() {
            continue;
        }
        commits_with_activity += 1;
        for sid in &touched {
            *spec_activity.entry((*sid).to_string()).or_default() += 1;
        }
        // All unordered pairs within `touched`.
        let touched_vec: Vec<&str> = touched.iter().copied().collect();
        for i in 0..touched_vec.len() {
            for j in (i + 1)..touched_vec.len() {
                let (a, b) = (touched_vec[i], touched_vec[j]);
                // Alphabetical direction convention.
                let (src, dst) = if a < b { (a, b) } else { (b, a) };
                *co.entry((src.to_string(), dst.to_string())).or_default() += 1;
            }
        }
    }

    // Sort edges meeting the threshold by co-commits desc, then lex.
    let mut proposed: Vec<CoEdge> = co
        .into_iter()
        .filter(|(_, n)| *n >= opts.min_commits)
        .map(|((src, dst), co_commits)| CoEdge {
            src,
            dst,
            co_commits,
        })
        .collect();
    proposed.sort_by(|a, b| {
        b.co_commits
            .cmp(&a.co_commits)
            .then_with(|| a.src.cmp(&b.src).then_with(|| a.dst.cmp(&b.dst)))
    });

    // Greedy DAG filter: add edges in priority order; drop any edge that
    // would create a cycle. Since we always direct alphabetically this is
    // rare, but double-direction edges in user-authored extensions could
    // trigger it.
    let mut kept: Vec<CoEdge> = Vec::new();
    let mut dropped: Vec<CoEdge> = Vec::new();
    for e in proposed {
        if would_create_cycle(&kept, &e.src, &e.dst) {
            dropped.push(e);
        } else {
            kept.push(e);
        }
    }

    Ok(CalibrationReport {
        commits_analysed: commits.len(),
        commits_with_spec_activity: commits_with_activity,
        spec_activity,
        edges: kept,
        dropped_cycle_edges: dropped,
    })
}

fn would_create_cycle(existing: &[CoEdge], new_src: &str, new_dst: &str) -> bool {
    // Reachability: does `new_dst` already reach `new_src`? Then adding
    // `new_src -> new_dst` would close a cycle.
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for e in existing {
        adj.entry(e.src.as_str()).or_default().push(e.dst.as_str());
    }
    let mut stack: Vec<&str> = vec![new_dst];
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    while let Some(n) = stack.pop() {
        if n == new_src {
            return true;
        }
        if !seen.insert(n) {
            continue;
        }
        if let Some(children) = adj.get(n) {
            stack.extend(children.iter().copied());
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(id: &str, support: &[&str]) -> SpecDescriptor {
        SpecDescriptor {
            id: id.into(),
            support: support.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn commits(spec_sets: &[&[&str]], supports: &[&SpecDescriptor]) -> Vec<Vec<String>> {
        spec_sets
            .iter()
            .map(|set| {
                set.iter()
                    .flat_map(|sid| {
                        supports
                            .iter()
                            .find(|s| s.id == *sid)
                            .map(|s| s.support.clone())
                            .unwrap_or_default()
                    })
                    .collect()
            })
            .collect()
    }

    #[test]
    fn parse_git_log_handles_multi_file_commits() {
        let raw = "---COMMIT---\nsrc/a.rs\nsrc/b.rs\n---COMMIT---\nsrc/c.rs\n";
        let cs = parse_git_log(raw);
        assert_eq!(cs.len(), 2);
        assert_eq!(cs[0], vec!["src/a.rs", "src/b.rs"]);
        assert_eq!(cs[1], vec!["src/c.rs"]);
    }

    #[test]
    fn compute_report_proposes_high_cooccurrence_pairs() {
        let specs = [
            spec("auth_login", &["src/auth.rs"]),
            spec("billing", &["src/billing.rs"]),
            spec("api", &["src/api.rs"]),
        ];
        let spec_refs: Vec<&SpecDescriptor> = specs.iter().collect();
        // 5 commits where auth_login + billing co-modify; 1 isolated api commit.
        let sets: &[&[&str]] = &[
            &["auth_login", "billing"],
            &["auth_login", "billing"],
            &["auth_login", "billing"],
            &["auth_login", "billing"],
            &["auth_login", "billing"],
            &["api"],
        ];
        let cs = commits(sets, &spec_refs);
        let report = compute_report(&specs, &cs, &CalibrateOpts::default()).unwrap();
        assert_eq!(report.commits_analysed, 6);
        assert_eq!(report.commits_with_spec_activity, 6);
        assert_eq!(report.edges.len(), 1, "expected a single coupling edge");
        assert_eq!(report.edges[0].src, "auth_login");
        assert_eq!(report.edges[0].dst, "billing");
        assert_eq!(report.edges[0].co_commits, 5);
    }

    #[test]
    fn compute_report_respects_min_commits() {
        let specs = [spec("a", &["src/a.rs"]), spec("b", &["src/b.rs"])];
        let spec_refs: Vec<&SpecDescriptor> = specs.iter().collect();
        let sets: &[&[&str]] = &[&["a", "b"], &["a", "b"]];
        let cs = commits(sets, &spec_refs);
        let opts = CalibrateOpts {
            min_commits: 3,
            ..Default::default()
        };
        let report = compute_report(&specs, &cs, &opts).unwrap();
        assert!(
            report.edges.is_empty(),
            "expected no edges below threshold, got {:?}",
            report.edges
        );
    }

    #[test]
    fn compute_report_directs_edges_alphabetically() {
        let specs = [spec("zulu", &["src/z.rs"]), spec("alpha", &["src/a.rs"])];
        let spec_refs: Vec<&SpecDescriptor> = specs.iter().collect();
        let pair: &[&str] = &["zulu", "alpha"];
        let sets: Vec<&[&str]> = vec![pair; 10];
        let cs = commits(&sets, &spec_refs);
        let report = compute_report(&specs, &cs, &CalibrateOpts::default()).unwrap();
        assert_eq!(report.edges[0].src, "alpha");
        assert_eq!(report.edges[0].dst, "zulu");
    }

    #[test]
    fn report_snippet_contains_count_annotations() {
        let report = CalibrationReport {
            commits_analysed: 42,
            commits_with_spec_activity: 30,
            spec_activity: BTreeMap::new(),
            edges: vec![CoEdge {
                src: "auth".into(),
                dst: "billing".into(),
                co_commits: 17,
            }],
            dropped_cycle_edges: vec![],
        };
        let toml = report.to_toml_snippet();
        assert!(toml.contains("42 commits"));
        assert!(toml.contains("17 co-commits"));
        assert!(toml.contains("[\"auth\", \"billing\"]"));
    }

    #[test]
    fn would_create_cycle_detects_back_edge() {
        // a -> b -> c already. Proposing c -> a would close a cycle.
        let existing = vec![
            CoEdge {
                src: "a".into(),
                dst: "b".into(),
                co_commits: 5,
            },
            CoEdge {
                src: "b".into(),
                dst: "c".into(),
                co_commits: 5,
            },
        ];
        assert!(would_create_cycle(&existing, "c", "a"));
        assert!(!would_create_cycle(&existing, "a", "c"));
    }

    #[test]
    fn sibling_directories_do_not_false_match() {
        // Audit finding C-01 / C-13: support `src/auth` must NOT match a
        // commit that touches only `src/auth_helpers/*`. Pre-fix the
        // `starts_with` call bled across sibling path prefixes.
        let specs = [
            spec("auth", &["src/auth"]),
            spec("helpers", &["src/auth_helpers"]),
        ];
        let cs = vec![
            vec!["src/auth_helpers/h.rs".to_string()],
            vec!["src/auth_helpers/h.rs".to_string()],
            vec!["src/auth_helpers/h.rs".to_string()],
            vec![
                "src/auth/a.rs".to_string(),
                "src/auth_helpers/h.rs".to_string(),
            ],
        ];
        let report = compute_report(&specs, &cs, &CalibrateOpts::default()).unwrap();
        // `auth` must only be counted in the 4th commit.
        assert_eq!(report.spec_activity.get("auth").copied(), Some(1));
        assert_eq!(report.spec_activity.get("helpers").copied(), Some(4));
        // No coupling edge meets the default min_commits=3 threshold for
        // the auth<->helpers pair (only 1 co-commit).
        assert!(
            report.edges.is_empty(),
            "expected no edges; found {:?}",
            report.edges
        );
    }

    #[test]
    fn trailing_slash_support_is_equivalent_to_bare() {
        // Audit C-13 tail: `src/widget/` and `src/widget` should match the
        // same files so users don't have to guess the convention.
        let specs_with_slash = [spec("w", &["src/widget/"])];
        let specs_bare = [spec("w", &["src/widget"])];
        let cs = vec![
            vec!["src/widget/a.rs".to_string()],
            vec!["src/widget/sub/b.rs".to_string()],
            vec!["src/widgetry/x.rs".to_string()], // must NOT match either
        ];
        let r1 = compute_report(&specs_with_slash, &cs, &CalibrateOpts::default()).unwrap();
        let r2 = compute_report(&specs_bare, &cs, &CalibrateOpts::default()).unwrap();
        assert_eq!(r1.spec_activity, r2.spec_activity);
        assert_eq!(r1.spec_activity.get("w").copied(), Some(2));
    }

    #[test]
    fn exact_file_match_works() {
        // Audit C-14: a support entry that names a file directly (not a
        // directory) should match that file and only that file. Sibling
        // filenames that share a prefix must not be matched.
        let specs = [spec("main", &["src/main.rs"])];
        let cs = vec![
            vec!["src/main.rs".to_string()],
            vec!["src/mainframe.rs".to_string()], // no match
            vec!["src/main.rs.bak".to_string()],  // no match
        ];
        let report = compute_report(&specs, &cs, &CalibrateOpts::default()).unwrap();
        assert_eq!(report.spec_activity.get("main").copied(), Some(1));
    }

    #[test]
    fn directory_prefix_support_matches_nested_files() {
        // Support "src/auth/" matches any commit file under src/auth/.
        let specs = [spec("auth", &["src/auth/"])];
        let spec_refs: Vec<&SpecDescriptor> = specs.iter().collect();
        let sets: &[&[&str]] = &[&[]];
        // Hand-build a commit list that names files but no spec.
        let _ = commits(sets, &spec_refs);
        // Actually test the real path:
        let cs = vec![
            vec!["src/auth/login.rs".to_string()],
            vec!["src/unrelated.rs".to_string()],
        ];
        let report = compute_report(&specs, &cs, &CalibrateOpts::default()).unwrap();
        assert_eq!(report.spec_activity.get("auth").copied(), Some(1));
    }
}
