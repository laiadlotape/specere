//! Git-history metrics + co-modification PPMI (FR-HM-020..022).
//!
//! For each node in the harness graph we compute:
//!
//! - **`age_days`**: whole days between the introducing commit and HEAD.
//! - **`commits`**: number of commits that touched the file (renames
//!   followed via `--follow`, copies via `-M -C`).
//! - **`authors`**: distinct committer emails across those commits.
//! - **`churn_rate`**: `(lines_added + lines_deleted) / commits` — a
//!   mean-lines-per-commit score; zero for add-once-never-touched files.
//! - **`last_touched`**: most-recent commit timestamp (RFC-3339).
//! - **`bus_factor`**: number of authors who own ≥ 20 % of the lines each.
//!   A bus_factor of 1 is the high-risk case.
//! - **`hotspot_score`**: `(churn_rate × ln(commits + 1)) / (age_days + 1)`
//!   (Adam Tornhill-style). Sort harness files by this to surface
//!   test-rot candidates in S6 and beyond.
//!
//! Alongside per-node metrics, we compute **pairwise PPMI** on the
//! commit matrix: two files that co-change consistently get a `comod`
//! edge with PPMI > 0 and `co_commits` over the min-threshold.
//!
//! Reuses git via subprocess (`git log --numstat --follow -M -C`).
//! Falls back cleanly on shallow clones / fresh `git init` repos.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::process::Command;

use anyhow::Result;

use crate::harness::node::{ComodEdge, HarnessGraph, VersionMetrics};

/// Minimum `co_commits` count for a pair to emit a `comod` edge.
/// Same 3-commit floor as `specere calibrate from-git` (v1.0.1 audit).
#[allow(dead_code)]
pub const DEFAULT_MIN_COMOD_COMMITS: u32 = 3;

/// Enrich `graph` with per-node `version_metrics` + pairwise `comod_edges`.
///
/// `min_comod_commits` controls the PPMI reporting threshold; counts
/// below this are dropped.
pub fn enrich(
    graph: &mut HarnessGraph,
    repo: &Path,
    min_comod_commits: u32,
) -> Result<HistoryReport> {
    let mut report = HistoryReport::default();

    // Pass 1: for each node, run `git log --numstat --follow -M -C -- <path>`
    // and accumulate VersionMetrics.
    let now_ymd = current_ymd();
    for node in &mut graph.nodes {
        if let Some(vm) = compute_metrics(repo, &node.path, now_ymd) {
            node.version_metrics = Some(vm);
            report.nodes_enriched += 1;
        }
    }

    // Pass 2: per-file sets of commit SHAs (for PPMI). We run `git log
    // --name-only --pretty=format:%H -M -C --follow -- <path>` once per
    // node. This is O(nodes × git_log_cost); acceptable for typical
    // ~100-1000 harness files, and avoidable only by a full repo-wide
    // log with post-hoc path join (deferred optimisation).
    let n_commits_total = repo_total_commits(repo).unwrap_or(0);
    if n_commits_total == 0 {
        // No git history → can't compute PPMI. Return the report early.
        return Ok(report);
    }

    let mut commit_sets: HashMap<String, BTreeSet<String>> = HashMap::new();
    for node in &graph.nodes {
        let commits = commits_for_path(repo, &node.path).unwrap_or_default();
        if !commits.is_empty() {
            commit_sets.insert(node.id.clone(), commits);
        }
    }

    let path_of: HashMap<String, String> = graph
        .nodes
        .iter()
        .map(|n| (n.id.clone(), n.path.clone()))
        .collect();

    // Pairwise PPMI.
    let ids: Vec<&String> = commit_sets.keys().collect();
    let n_commits_f = n_commits_total as f64;
    for i in 0..ids.len() {
        for j in (i + 1)..ids.len() {
            let a = ids[i];
            let b = ids[j];
            let set_a = &commit_sets[a];
            let set_b = &commit_sets[b];
            let joint: usize = set_a.intersection(set_b).count();
            if (joint as u32) < min_comod_commits {
                continue;
            }
            let p_a = set_a.len() as f64 / n_commits_f;
            let p_b = set_b.len() as f64 / n_commits_f;
            let p_ab = joint as f64 / n_commits_f;
            let denom = p_a * p_b;
            let ppmi = if denom > 0.0 {
                (p_ab / denom).log2().max(0.0)
            } else {
                0.0
            };
            if ppmi <= 0.0 {
                continue;
            }
            let (from, to, from_path, to_path) = if a < b {
                (a.clone(), b.clone(), path_of[a].clone(), path_of[b].clone())
            } else {
                (b.clone(), a.clone(), path_of[b].clone(), path_of[a].clone())
            };
            graph.comod_edges.push(ComodEdge {
                from,
                to,
                from_path,
                to_path,
                co_commits: joint as u32,
                ppmi,
            });
            report.comod_edges_emitted += 1;
        }
    }
    // Dedupe + deterministic sort.
    graph
        .comod_edges
        .sort_by(|a, b| a.from.cmp(&b.from).then_with(|| a.to.cmp(&b.to)));
    graph.comod_edges.dedup();

    Ok(report)
}

/// Report summary — printed by the CLI.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct HistoryReport {
    pub nodes_enriched: usize,
    pub comod_edges_emitted: usize,
}

fn compute_metrics(repo: &Path, rel_path: &str, now_ymd: [u32; 3]) -> Option<VersionMetrics> {
    // `git log --numstat --follow -M -C --pretty=format:%H%x09%ae%x09%aI -- <path>`
    // yields interleaved lines:
    //   <sha>\t<author_email>\t<author_iso>
    //   <add>\t<del>\t<path>  (one line per file in the commit; --follow filters to this path)
    let out = Command::new("git")
        .current_dir(repo)
        .args([
            "log",
            "--numstat",
            "--follow",
            "-M",
            "-C",
            "--pretty=format:%H%x09%ae%x09%aI",
            "--",
            rel_path,
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let raw = String::from_utf8_lossy(&out.stdout).into_owned();
    if raw.trim().is_empty() {
        return None;
    }

    let mut commits = 0u32;
    let mut authors: BTreeSet<String> = BTreeSet::new();
    let mut author_lines: BTreeMap<String, u64> = BTreeMap::new();
    let mut total_add: u64 = 0;
    let mut total_del: u64 = 0;
    let mut oldest_date: Option<String> = None;
    let mut newest_date: Option<String> = None;
    let mut cur_author: Option<String> = None;

    for line in raw.lines() {
        if line.is_empty() {
            continue;
        }
        // Both header and numstat lines carry 2 tabs; disambiguate by first
        // token: header → 40-char hex SHA; numstat → digits or "-" for
        // binaries. `--follow` may emit a rename notation like "a => b" in
        // the third numstat field — we ignore the path entirely since
        // `--follow` already filtered to our file.
        let mut parts = line.splitn(3, '\t');
        let first = parts.next().unwrap_or("");
        let second = parts.next().unwrap_or("");
        let third = parts.next().unwrap_or("");
        if is_sha(first) && !second.is_empty() && !third.is_empty() {
            commits += 1;
            cur_author = Some(second.to_string());
            authors.insert(second.to_string());
            if newest_date.is_none() {
                newest_date = Some(third.to_string());
            }
            oldest_date = Some(third.to_string());
        } else if (first == "-" || first.chars().all(|c| c.is_ascii_digit()))
            && (second == "-" || second.chars().all(|c| c.is_ascii_digit()))
        {
            let add_n: u64 = first.parse().unwrap_or(0);
            let del_n: u64 = second.parse().unwrap_or(0);
            total_add += add_n;
            total_del += del_n;
            if let Some(a) = &cur_author {
                *author_lines.entry(a.clone()).or_insert(0) += add_n;
            }
        }
    }
    if commits == 0 {
        return None;
    }

    let age_days = oldest_date
        .as_deref()
        .and_then(parse_iso_ymd)
        .map(|d| days_between(d, now_ymd))
        .unwrap_or(0);

    let churn_rate = if commits > 0 {
        ((total_add + total_del) as f64 / commits as f64 * 100.0).round() / 100.0
    } else {
        0.0
    };

    let total_lines: u64 = author_lines.values().sum();
    let bus_factor = if total_lines > 0 {
        let threshold = (total_lines as f64 * 0.2) as u64;
        author_lines.values().filter(|v| **v >= threshold).count() as u32
    } else {
        0
    };

    let hotspot_score = {
        let n = commits as f64;
        let a = age_days as f64;
        churn_rate * (n + 1.0).ln() / (a + 1.0)
    };

    Some(VersionMetrics {
        age_days,
        commits,
        authors: authors.len() as u32,
        churn_rate,
        last_touched: newest_date,
        bus_factor,
        hotspot_score: (hotspot_score * 100.0).round() / 100.0,
    })
}

fn commits_for_path(repo: &Path, rel_path: &str) -> Option<BTreeSet<String>> {
    let out = Command::new("git")
        .current_dir(repo)
        .args([
            "log",
            "--follow",
            "-M",
            "-C",
            "--pretty=format:%H",
            "--",
            rel_path,
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s: BTreeSet<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    Some(s)
}

fn repo_total_commits(repo: &Path) -> Option<u32> {
    let out = Command::new("git")
        .current_dir(repo)
        .args(["rev-list", "--count", "HEAD"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

/// Very small ISO-8601-date parser for the `%aI` format's YYYY-MM-DD prefix.
fn is_sha(s: &str) -> bool {
    s.len() >= 7 && s.len() <= 64 && s.chars().all(|c| c.is_ascii_hexdigit())
}

fn parse_iso_ymd(iso: &str) -> Option<[u32; 3]> {
    if iso.len() < 10 {
        return None;
    }
    let y: u32 = iso.get(0..4)?.parse().ok()?;
    let m: u32 = iso.get(5..7)?.parse().ok()?;
    let d: u32 = iso.get(8..10)?.parse().ok()?;
    Some([y, m, d])
}

fn current_ymd() -> [u32; 3] {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Days since epoch.
    let days_since_epoch = secs / 86_400;
    // Convert to Y-M-D via the classic civil algorithm (Howard Hinnant).
    let z = days_since_epoch as i64 + 719_468;
    let era = if z >= 0 {
        z / 146_097
    } else {
        (z - 146_096) / 146_097
    };
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy as i64 - (153 * mp as i64 + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as i64;
    let y = y + if m <= 2 { 1 } else { 0 };
    [y as u32, m as u32, d as u32]
}

fn days_between(older: [u32; 3], newer: [u32; 3]) -> u32 {
    fn to_days([y, m, d]: [u32; 3]) -> i64 {
        let y = y as i64 - if m <= 2 { 1 } else { 0 };
        let era = y.div_euclid(400);
        let yoe = (y - era * 400) as u64;
        let m = m as u64;
        let d = d as u64;
        let m_adj = if m > 2 { m - 3 } else { m + 9 };
        let doy = (153 * m_adj + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe as i64 - 719_468
    }
    let a = to_days(older);
    let b = to_days(newer);
    (b - a).max(0) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::node::{path_id, Category, HarnessFile};
    use tempfile::TempDir;

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
                })
                .collect(),
            edges: Vec::new(),
            comod_edges: Vec::new(),
        }
    }

    fn git(dir: &Path, args: &[&str]) -> bool {
        Command::new("git")
            .args(args)
            .current_dir(dir)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    fn init_git(dir: &Path) {
        assert!(git(dir, &["init", "-q", "-b", "main"]));
        assert!(git(dir, &["config", "user.email", "t@x.y"]));
        assert!(git(dir, &["config", "user.name", "T"]));
    }

    #[test]
    fn parse_iso_ymd_handles_standard_format() {
        assert_eq!(parse_iso_ymd("2026-04-19T10:00:00Z"), Some([2026, 4, 19]));
        assert_eq!(parse_iso_ymd("2025-12-31"), Some([2025, 12, 31]));
        assert_eq!(parse_iso_ymd("bad"), None);
    }

    #[test]
    fn days_between_is_monotonic() {
        let a = days_between([2026, 4, 19], [2026, 4, 20]);
        assert_eq!(a, 1);
        let b = days_between([2026, 4, 1], [2026, 4, 19]);
        assert_eq!(b, 18);
        let c = days_between([2026, 4, 19], [2026, 4, 19]);
        assert_eq!(c, 0);
    }

    #[test]
    fn single_file_two_commits_metrics() {
        let dir = TempDir::new().unwrap();
        let repo = dir.path();
        init_git(repo);
        std::fs::write(repo.join("foo.rs"), "fn f(){}\n").unwrap();
        git(repo, &["add", "foo.rs"]);
        git(repo, &["commit", "-q", "-m", "add"]);
        std::fs::write(repo.join("foo.rs"), "fn f(){}\nfn g(){}\n").unwrap();
        git(repo, &["add", "foo.rs"]);
        git(repo, &["commit", "-q", "-m", "edit"]);

        let mut g = graph_with(&["foo.rs"]);
        let report = enrich(&mut g, repo, 3).unwrap();
        assert_eq!(report.nodes_enriched, 1);
        let vm = g.nodes[0].version_metrics.as_ref().unwrap();
        assert_eq!(vm.commits, 2);
        assert_eq!(vm.authors, 1);
        assert!(vm.churn_rate > 0.0, "some churn expected: {vm:?}");
        assert!(vm.last_touched.is_some());
    }

    #[test]
    fn two_files_co_modification_ppmi_positive() {
        let dir = TempDir::new().unwrap();
        let repo = dir.path();
        init_git(repo);
        // Five joint commits → strong coupling.
        for i in 0..5 {
            std::fs::write(repo.join("a.rs"), format!("{i}")).unwrap();
            std::fs::write(repo.join("b.rs"), format!("{i}")).unwrap();
            git(repo, &["add", "."]);
            git(repo, &["commit", "-q", "-m", "pair"]);
        }
        // An unrelated commit on `c.rs` so p(a)·p(b) < p(a,b) — without
        // this, every commit has both files and PPMI collapses to zero.
        std::fs::write(repo.join("c.rs"), "unrelated").unwrap();
        git(repo, &["add", "c.rs"]);
        git(repo, &["commit", "-q", "-m", "solo c"]);

        let mut g = graph_with(&["a.rs", "b.rs", "c.rs"]);
        let report = enrich(&mut g, repo, 3).unwrap();
        assert_eq!(report.comod_edges_emitted, 1);
        let e = &g.comod_edges[0];
        assert_eq!(e.co_commits, 5);
        assert!(
            e.ppmi > 0.0,
            "PPMI must be positive for coupled pair: {e:?}"
        );
    }

    #[test]
    fn below_threshold_pair_is_dropped() {
        let dir = TempDir::new().unwrap();
        let repo = dir.path();
        init_git(repo);
        // Only 2 joint commits — below the default 3.
        for i in 0..2 {
            std::fs::write(repo.join("a.rs"), format!("{i}")).unwrap();
            std::fs::write(repo.join("b.rs"), format!("{i}")).unwrap();
            git(repo, &["add", "."]);
            git(repo, &["commit", "-q", "-m", "pair"]);
        }
        let mut g = graph_with(&["a.rs", "b.rs"]);
        let report = enrich(&mut g, repo, 3).unwrap();
        assert_eq!(
            report.comod_edges_emitted, 0,
            "below threshold pair must be dropped"
        );
    }

    #[test]
    fn missing_git_repo_returns_zero_enrichment() {
        let dir = TempDir::new().unwrap();
        let mut g = graph_with(&["tests/lonely.rs"]);
        // No git init at all.
        let report = enrich(&mut g, dir.path(), 3).unwrap();
        assert_eq!(report.nodes_enriched, 0);
        assert_eq!(report.comod_edges_emitted, 0);
    }

    #[test]
    fn hotspot_score_is_higher_for_churny_files() {
        let dir = TempDir::new().unwrap();
        let repo = dir.path();
        init_git(repo);
        // `hot.rs` changes a lot, `cold.rs` is added once.
        for i in 0..10 {
            std::fs::write(repo.join("hot.rs"), format!("fn x_{i}() {{}}\n")).unwrap();
            git(repo, &["add", "hot.rs"]);
            git(repo, &["commit", "-q", "-m", "churn"]);
        }
        std::fs::write(repo.join("cold.rs"), "fn c(){}").unwrap();
        git(repo, &["add", "cold.rs"]);
        git(repo, &["commit", "-q", "-m", "cold add"]);

        let mut g = graph_with(&["hot.rs", "cold.rs"]);
        enrich(&mut g, repo, 3).unwrap();
        let hot_score = g
            .nodes
            .iter()
            .find(|n| n.path == "hot.rs")
            .unwrap()
            .version_metrics
            .as_ref()
            .unwrap()
            .hotspot_score;
        let cold_score = g
            .nodes
            .iter()
            .find(|n| n.path == "cold.rs")
            .unwrap()
            .version_metrics
            .as_ref()
            .unwrap()
            .hotspot_score;
        assert!(
            hot_score > cold_score,
            "hot hotspot_score={hot_score} should exceed cold={cold_score}"
        );
    }
}
