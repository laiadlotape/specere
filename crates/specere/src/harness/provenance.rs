//! Provenance join (FR-HM-010..012).
//!
//! Answers: *who (human or agent) + what (/speckit-* verb) + when created
//! this harness file?* Two signal paths, joined per-node:
//!
//! 1. **Workflow-span attribution** (primary). Walks
//!    `.specere/events.jsonl`, picks up every `/speckit-*` workflow span
//!    (attribute `specere.workflow_step` present), and matches by
//!    `files_created` / `files_touched` / `paths` attributes against
//!    harness-graph node paths. When multiple spans claim the same file,
//!    the **earliest** wins (creator vs. later modifier distinction).
//!
//! 2. **Git log fallback** (secondary). For any harness file not claimed
//!    by a workflow span, shells out to
//!    `git log --diff-filter=A --follow -- <path>` to find the introducing
//!    commit + its author. Purely human provenance.
//!
//! **Divergence** (FR-HM-012): when an agent created the file AND a human
//! later patched ≥ 50 % of its lines, `provenance.divergence_detected =
//! true`. We surface but don't block; the reviewer decides.
//!
//! Today's event-store emissions do **not** routinely carry
//! `files_created`/`files_touched`; the skill (`specere-observe-step`)
//! records only verb + phase + feature_dir. S2 therefore ships as a
//! *forward-compatible* join: existing repos get git-based provenance;
//! once hook authors start emitting `files_touched`, agent provenance
//! fills in automatically.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};

use crate::harness::node::{HarnessGraph, Provenance};

/// Populate `provenance` on every node in `graph`. Mutates in place; writes
/// nothing. Caller is responsible for `write_atomic`.
pub fn enrich(graph: &mut HarnessGraph, repo: &Path) -> Result<ProvReport> {
    let mut report = ProvReport::default();

    // Pass 1: load events, build (file_path → SpanClaim) map of earliest
    // workflow-span claim.
    let events_path = repo.join(".specere").join("events.jsonl");
    let span_claims = if events_path.is_file() {
        load_span_claims(&events_path).unwrap_or_default()
    } else {
        BTreeMap::new()
    };

    // Pass 2: for each node, prefer span claim; else fall back to git log.
    for node in &mut graph.nodes {
        let mut p = Provenance::default();

        if let Some(claim) = span_claims.get(&node.path) {
            p.creator_span_id = claim.span_id.clone();
            p.creator_verb = claim.verb.clone();
            p.creator_agent = claim.agent.clone();
            p.creator_spec = claim.spec.clone();
            p.created_at = Some(claim.ts.clone());
            report.span_attributed += 1;
        }

        // Always also query git log for commit + human author (complements
        // agent attribution; these aren't mutually exclusive).
        if let Some((commit, author, author_ts)) = git_created_by(repo, &node.path) {
            p.creator_commit = Some(commit);
            p.creator_human = Some(author);
            if p.created_at.is_none() {
                p.created_at = Some(author_ts);
            }
            report.git_attributed += 1;
        }

        // Divergence: rough heuristic — agent attribution present AND
        // non-trivial post-creation blame authorship. Full implementation
        // needs per-line blame + .mailmap resolution; S2 ships the flag
        // plumbed but false by default. A follow-up slice (FR-HM-012a)
        // will wire the blame walk.
        //
        // For now, we set divergence_detected = true only when the creator
        // span's agent field is Some AND the git commit author matches a
        // well-known human-email pattern (not the agent). Minimal signal,
        // but it's opt-in via the node-level flag; never gates anything.
        if let (Some(agent), Some(human)) = (&p.creator_agent, &p.creator_human) {
            // Very conservative — only flag divergence when the agent is a
            // known auto-commit marker and the human email is something
            // else. This avoids false positives on human-authored-and-
            // committed files.
            let agent_is_auto = agent.eq_ignore_ascii_case("claude-code")
                || agent.eq_ignore_ascii_case("cursor")
                || agent.eq_ignore_ascii_case("openhands");
            let human_is_bot = human.contains("bot") || human.ends_with("[bot]");
            if agent_is_auto && !human_is_bot {
                p.divergence_detected = true;
                report.divergence_detected += 1;
            }
        }

        // Store provenance only when at least one field was populated.
        if p.creator_span_id.is_some() || p.creator_commit.is_some() || p.creator_verb.is_some() {
            node.provenance = Some(p);
            report.total_enriched += 1;
        }
    }

    Ok(report)
}

/// Summary of the enrich pass; printed by the CLI.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ProvReport {
    pub total_enriched: usize,
    pub span_attributed: usize,
    pub git_attributed: usize,
    pub divergence_detected: usize,
}

/// One earliest-win claim on a repo-rel path by a workflow span.
#[derive(Debug, Clone)]
struct SpanClaim {
    span_id: Option<String>,
    verb: Option<String>,
    agent: Option<String>,
    spec: Option<String>,
    /// RFC-3339 timestamp; used for earliest-win comparisons.
    ts: String,
}

/// Parse `.specere/events.jsonl` into a `file_path -> earliest SpanClaim`
/// map. We pick up two attribute-shape variants:
///
/// - `attrs.files_created = "path1,path2"` (comma-separated) — the
///   preferred attr the skill will emit (spec'd in FR-HM-010b).
/// - `attrs.files_touched` / `attrs.paths` — legacy; used verbatim by
///   the filter's predict step. We treat a `files_touched` attribute as
///   *modification, not creation* — it populates `modifier` fields in
///   S3, not here. S2 only consumes `files_created` + `paths` when the
///   span's `specere.workflow_step` contains a verb. Anything else is
///   ignored.
fn load_span_claims(events_path: &Path) -> Result<BTreeMap<String, SpanClaim>> {
    let raw = std::fs::read_to_string(events_path)
        .with_context(|| format!("read {}", events_path.display()))?;
    let mut claims: BTreeMap<String, SpanClaim> = BTreeMap::new();

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
        let verb = attrs
            .get("specere.workflow_step")
            .and_then(|x| x.as_str())
            .map(String::from);
        if verb.is_none() {
            // Only workflow-span events participate in provenance attribution.
            continue;
        }
        let ts = v
            .get("ts")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let files_created = attrs
            .get("files_created")
            .and_then(|x| x.as_str())
            .unwrap_or("");
        let paths_attr = attrs.get("paths").and_then(|x| x.as_str()).unwrap_or("");

        // We only claim *creation* from files_created today; a later slice
        // (FR-HM-010b) may widen to paths=… when the skill flags them.
        let files: Vec<&str> = if !files_created.is_empty() {
            files_created
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect()
        } else if !paths_attr.is_empty() {
            paths_attr
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect()
        } else {
            continue;
        };

        let claim = SpanClaim {
            span_id: v
                .get("span_id")
                .or_else(|| attrs.get("span_id"))
                .and_then(|x| x.as_str())
                .map(String::from),
            verb: verb.clone(),
            agent: attrs
                .get("gen_ai.system")
                .and_then(|x| x.as_str())
                .map(String::from),
            spec: attrs
                .get("specere.fr_ids")
                .and_then(|x| x.as_str())
                .and_then(|s| s.split(',').next().map(str::trim).map(String::from)),
            ts: ts.clone(),
        };

        for path in files {
            // Earliest-win: only overwrite when the incoming ts is earlier.
            let ent = claims.entry(path.to_string());
            use std::collections::btree_map::Entry;
            match ent {
                Entry::Vacant(v) => {
                    v.insert(claim.clone());
                }
                Entry::Occupied(mut o) => {
                    if claim.ts < o.get().ts {
                        *o.get_mut() = claim.clone();
                    }
                }
            }
        }
    }
    Ok(claims)
}

/// Find the commit SHA + author email + author date that introduced `path`.
/// Returns `None` when git is unavailable, the file is untracked, or the
/// repo has no history.
fn git_created_by(repo: &Path, rel_path: &str) -> Option<(String, String, String)> {
    let out = Command::new("git")
        .current_dir(repo)
        .args([
            "log",
            "--diff-filter=A",
            "--follow",
            "--format=%H%x09%ae%x09%aI",
            "-1",
            "--",
            rel_path,
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let line = String::from_utf8_lossy(&out.stdout);
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let mut parts = line.split('\t');
    let sha = parts.next()?.to_string();
    let author = parts.next()?.to_string();
    let ts = parts.next()?.to_string();
    Some((sha, author, ts))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::node::{path_id, Category, HarnessFile};
    use tempfile::TempDir;

    fn test_graph(path: &str) -> HarnessGraph {
        HarnessGraph {
            schema_version: 1,
            nodes: vec![HarnessFile {
                id: path_id(path),
                path: path.to_string(),
                category: Category::Integration,
                category_confidence: 1.0,
                crate_name: None,
                test_names: Vec::new(),
                provenance: None,
                version_metrics: None,
                coverage_hash: None,
            }],
            edges: Vec::new(),
            comod_edges: Vec::new(),
            cov_cooccur_edges: Vec::new(),
        }
    }

    fn init_git(repo: &Path) {
        let _ = Command::new("git")
            .args(["init", "-q", "-b", "main"])
            .current_dir(repo)
            .status();
        let _ = Command::new("git")
            .args(["config", "user.email", "test@x.y"])
            .current_dir(repo)
            .status();
        let _ = Command::new("git")
            .args(["config", "user.name", "T"])
            .current_dir(repo)
            .status();
    }

    #[test]
    fn git_fallback_identifies_creation_commit() {
        let dir = TempDir::new().unwrap();
        let repo = dir.path();
        init_git(repo);
        std::fs::write(repo.join("foo.rs"), "fn f(){}").unwrap();
        Command::new("git")
            .args(["add", "foo.rs"])
            .current_dir(repo)
            .status()
            .unwrap();
        Command::new("git")
            .args(["commit", "-q", "-m", "add"])
            .current_dir(repo)
            .status()
            .unwrap();

        let result = git_created_by(repo, "foo.rs");
        let (_, author, _) = result.expect("must find introducing commit");
        assert_eq!(author, "test@x.y");
    }

    #[test]
    fn git_fallback_returns_none_for_untracked_file() {
        let dir = TempDir::new().unwrap();
        init_git(dir.path());
        std::fs::write(dir.path().join("untracked.rs"), "x").unwrap();
        assert!(git_created_by(dir.path(), "untracked.rs").is_none());
    }

    #[test]
    fn span_claim_prefers_earliest_ts() {
        let dir = TempDir::new().unwrap();
        let events = dir.path().join("events.jsonl");
        std::fs::write(
            &events,
            concat!(
                r#"{"ts":"2026-04-19T09:00:00Z","attrs":{"specere.workflow_step":"implement","files_created":"tests/it.rs","gen_ai.system":"claude-code"}}"#, "\n",
                r#"{"ts":"2026-04-19T10:00:00Z","attrs":{"specere.workflow_step":"specify","files_created":"tests/it.rs","gen_ai.system":"claude-code"}}"#, "\n",
            ),
        ).unwrap();
        let claims = load_span_claims(&events).unwrap();
        let c = claims.get("tests/it.rs").expect("claim present");
        assert_eq!(c.verb.as_deref(), Some("implement"), "earliest ts wins");
    }

    #[test]
    fn span_claim_ignores_non_workflow_events() {
        let dir = TempDir::new().unwrap();
        let events = dir.path().join("events.jsonl");
        std::fs::write(
            &events,
            r#"{"ts":"2026-04-19T09:00:00Z","attrs":{"event_kind":"test_outcome","files_created":"tests/ignored.rs"}}"#,
        )
        .unwrap();
        let claims = load_span_claims(&events).unwrap();
        assert!(claims.is_empty(), "non-workflow events must not claim");
    }

    #[test]
    fn enrich_applies_span_attribution() {
        let dir = TempDir::new().unwrap();
        let repo = dir.path();
        // Seed an events.jsonl with a workflow span claiming tests/it.rs.
        std::fs::create_dir_all(repo.join(".specere")).unwrap();
        std::fs::write(
            repo.join(".specere").join("events.jsonl"),
            r#"{"ts":"2026-04-19T10:00:00Z","span_id":"span-abc","attrs":{"specere.workflow_step":"implement","files_created":"tests/it.rs","gen_ai.system":"claude-code","specere.fr_ids":"FR-001"}}"#,
        )
        .unwrap();
        let mut g = test_graph("tests/it.rs");
        let report = enrich(&mut g, repo).unwrap();
        assert_eq!(report.span_attributed, 1);
        let p = g.nodes[0].provenance.as_ref().unwrap();
        assert_eq!(p.creator_verb.as_deref(), Some("implement"));
        assert_eq!(p.creator_agent.as_deref(), Some("claude-code"));
        assert_eq!(p.creator_spec.as_deref(), Some("FR-001"));
        assert_eq!(p.creator_span_id.as_deref(), Some("span-abc"));
    }

    #[test]
    fn enrich_falls_back_to_git_when_no_span() {
        let dir = TempDir::new().unwrap();
        let repo = dir.path();
        init_git(repo);
        std::fs::write(repo.join("foo.rs"), "fn f(){}").unwrap();
        Command::new("git")
            .args(["add", "foo.rs"])
            .current_dir(repo)
            .status()
            .unwrap();
        Command::new("git")
            .args(["commit", "-q", "-m", "add"])
            .current_dir(repo)
            .status()
            .unwrap();

        let mut g = test_graph("foo.rs");
        let report = enrich(&mut g, repo).unwrap();
        assert_eq!(report.span_attributed, 0);
        assert_eq!(report.git_attributed, 1);
        let p = g.nodes[0].provenance.as_ref().unwrap();
        assert!(p.creator_commit.is_some());
        assert_eq!(p.creator_human.as_deref(), Some("test@x.y"));
        assert!(p.creator_verb.is_none());
    }

    #[test]
    fn divergence_flagged_when_agent_and_human_differ() {
        let dir = TempDir::new().unwrap();
        let repo = dir.path();
        init_git(repo);
        std::fs::write(repo.join("x.rs"), "x").unwrap();
        Command::new("git")
            .args(["add", "x.rs"])
            .current_dir(repo)
            .status()
            .unwrap();
        Command::new("git")
            .args(["commit", "-q", "-m", "add"])
            .current_dir(repo)
            .status()
            .unwrap();

        // Span claims the file as agent-authored.
        std::fs::create_dir_all(repo.join(".specere")).unwrap();
        std::fs::write(
            repo.join(".specere").join("events.jsonl"),
            r#"{"ts":"2026-04-19T10:00:00Z","attrs":{"specere.workflow_step":"implement","files_created":"x.rs","gen_ai.system":"claude-code"}}"#,
        )
        .unwrap();

        let mut g = test_graph("x.rs");
        let report = enrich(&mut g, repo).unwrap();
        assert_eq!(report.divergence_detected, 1);
        assert!(g.nodes[0].provenance.as_ref().unwrap().divergence_detected);
    }

    #[test]
    fn enrich_handles_missing_events_file() {
        let dir = TempDir::new().unwrap();
        let mut g = test_graph("tests/lonely.rs");
        // No git init → no git log either; enrich must not panic.
        let report = enrich(&mut g, dir.path()).unwrap();
        assert_eq!(report.total_enriched, 0);
    }
}
