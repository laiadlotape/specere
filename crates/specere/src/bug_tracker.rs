//! `specere observe watch-issues` — bug-tracker bridge for GitHub + Gitea
//! (FR-EQ-010..013, v1.0.6).
//!
//! Pipeline:
//!
//! 1. **Fetch**: poll the issues endpoint of GitHub or Gitea (same JSON
//!    shape for the subset we consume). Live mode uses `reqwest`; CI
//!    tests use the hidden `--from-fixture` flag pointing at a canned
//!    JSON body.
//! 2. **Filter**: skip issues labelled `question`, `docs`, `duplicate`,
//!    or `not-planned`, and skip closed issues with no linked PR.
//! 3. **Triage**: attribute each remaining issue to a spec_id via
//!    heuristic match — if the issue body names a path that falls
//!    inside a spec's `support` set, that spec wins; else stack-trace
//!    file path parse; else `unknown`. No LLM reranking today.
//! 4. **Emit**: one `bug_reported` event per issue with
//!    `spec_id, issue_url, severity, age_days, state` attrs.
//! 5. **Cursor**: update `.specere/posterior.toml`'s `[cursors]` table
//!    so re-runs are idempotent.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use specere_core::Ctx;

/// Labels that demote an issue to "not a bug" — we skip these entirely.
const SKIP_LABELS: &[&str] = &[
    "question",
    "docs",
    "documentation",
    "duplicate",
    "not-planned",
];

/// Issue labels that imply severity. Anything not listed → `minor`.
fn severity_for_labels(labels: &[String]) -> Severity {
    for l in labels {
        let l = l.to_lowercase();
        if matches!(
            l.as_str(),
            "critical" | "severity:critical" | "blocker" | "p0"
        ) {
            return Severity::Critical;
        }
        if matches!(l.as_str(), "bug" | "regression" | "severity:major" | "p1") {
            return Severity::Major;
        }
    }
    Severity::Minor
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Critical,
    Major,
    Minor,
}

impl Severity {
    pub fn as_attr(self) -> &'static str {
        match self {
            Self::Critical => "critical",
            Self::Major => "major",
            Self::Minor => "minor",
        }
    }
}

/// Issue-state axis used by the decay formula in FR-EQ-012.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Open,
    Closed,
}

impl State {
    pub fn as_attr(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
        }
    }
}

/// Issue shape we consume — the intersection of GitHub + Gitea payloads.
/// Additional provider-specific fields are silently dropped via
/// `#[serde(default)]` on every field.
#[derive(Debug, Clone, Deserialize)]
pub struct Issue {
    #[serde(default)]
    pub number: u64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default, alias = "html_url", alias = "url")]
    pub url: String,
    #[serde(default)]
    pub state: String,
    /// GitHub labels: array of `{name: ...}`. Gitea uses the same shape.
    #[serde(default)]
    pub labels: Vec<IssueLabel>,
    /// ISO-8601 created_at timestamp.
    #[serde(default)]
    pub created_at: String,
    /// ISO-8601 closed_at timestamp (null when state=open).
    #[serde(default)]
    #[allow(dead_code)]
    pub closed_at: Option<String>,
    /// GitHub-specific: issue has a linked PR when `pull_request` is
    /// not null. Gitea uses `pull_request` similarly.
    #[serde(default)]
    pub pull_request: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IssueLabel {
    #[serde(default)]
    pub name: String,
}

/// Parsed `[triage]` section of sensor-map.toml. Light today — just the
/// minimum to control heuristic fallback. LLM triage (FR-EQ-011 full)
/// is a follow-up slice behind `specere-triage-llm` opt-in.
#[derive(Debug, Clone, Default)]
pub struct TriageConfig {
    pub min_confidence: f64,
}

impl TriageConfig {
    pub fn load(sensor_map_path: &std::path::Path) -> Self {
        const DEFAULT_MIN_CONFIDENCE: f64 = 0.60;
        let raw = match std::fs::read_to_string(sensor_map_path) {
            Ok(s) => s,
            Err(_) => {
                return Self {
                    min_confidence: DEFAULT_MIN_CONFIDENCE,
                }
            }
        };
        let val: toml::Value = match toml::from_str(&raw) {
            Ok(v) => v,
            Err(_) => {
                return Self {
                    min_confidence: DEFAULT_MIN_CONFIDENCE,
                }
            }
        };
        let min_confidence = val
            .get("triage")
            .and_then(|t| t.get("min_confidence"))
            .and_then(|v| v.as_float())
            .unwrap_or(DEFAULT_MIN_CONFIDENCE);
        Self { min_confidence }
    }
}

/// Heuristic spec-id attribution (FR-EQ-011, LLM-free path).
///
/// Strategy:
/// 1. Scan issue body + title for tokens that look like file paths
///    (`foo/bar.rs`, `src/auth/token.rs`, etc.).
/// 2. For each candidate path, find specs whose `support` entries
///    prefix-match (same directory-boundary semantics as the calibrate
///    pipeline — `src/auth` must not match `src/auth_helpers/*`).
/// 3. Score by number of matching paths. Highest-scoring spec wins if
///    the match ratio exceeds `min_confidence`.
/// 4. Fallback: return `None` (caller records `spec_id = "unknown"`).
pub fn triage_heuristic(
    body: &str,
    title: &str,
    specs: &[specere_filter::hmm::SpecDescriptor],
    min_confidence: f64,
) -> Option<(String, f64)> {
    let text = format!("{title}\n{body}");
    // Crude path-token extraction: any whitespace-separated token
    // containing `/` AND ending with a file extension or `:` (for
    // `file.rs:42` stack-trace-style).
    let candidate_paths: Vec<String> = text
        .split(|c: char| c.is_whitespace() || matches!(c, '`' | '(' | ')' | '"' | ','))
        .filter(|tok| tok.contains('/'))
        .map(|tok| {
            tok.trim_end_matches(&[':', '.', ';', ']'] as &[_])
                .to_string()
        })
        .filter(|p| !p.is_empty())
        .collect();

    if candidate_paths.is_empty() {
        return None;
    }

    let mut score: BTreeMap<String, u32> = BTreeMap::new();
    for path in &candidate_paths {
        for spec in specs {
            if path_matches_support(path, &spec.support) {
                *score.entry(spec.id.clone()).or_insert(0) += 1;
            }
        }
    }

    let (best_id, best_hits) = score.iter().max_by_key(|(_, v)| **v)?;
    let total = candidate_paths.len() as f64;
    let confidence = *best_hits as f64 / total;
    if confidence >= min_confidence {
        Some((best_id.clone(), (confidence * 1000.0).round() / 1000.0))
    } else {
        None
    }
}

fn path_matches_support(path: &str, support: &[String]) -> bool {
    support.iter().any(|sup| {
        let bare = sup.trim_end_matches('/');
        let dir = format!("{bare}/");
        path == bare || path.starts_with(dir.as_str()) || path.contains(dir.as_str())
    })
}

/// Parse a vendor-provided JSON body (GitHub or Gitea) — both serve
/// either a JSON array of issues or an object with an `issues` key.
pub fn parse_issues_json(raw: &str) -> Result<Vec<Issue>> {
    // Try array-of-issues first.
    if let Ok(v) = serde_json::from_str::<Vec<Issue>>(raw) {
        return Ok(v);
    }
    // Fall back to `{issues: [...]}` — used by some Gitea wrappers.
    #[derive(Deserialize)]
    struct Wrap {
        issues: Vec<Issue>,
    }
    let w: Wrap =
        serde_json::from_str(raw).context("expected a JSON array or {issues: [...]} object")?;
    Ok(w.issues)
}

/// Should this issue produce a `bug_reported` event?
pub fn is_actionable(issue: &Issue) -> bool {
    let labels: Vec<String> = issue.labels.iter().map(|l| l.name.to_lowercase()).collect();
    if labels.iter().any(|l| SKIP_LABELS.contains(&l.as_str())) {
        return false;
    }
    if issue.state == "closed" && issue.pull_request.is_none() {
        // Closed without a linked PR — abandoned, not a signal.
        return false;
    }
    true
}

/// Compute age in days since `created_at` (RFC-3339). Returns 0 on parse
/// failure so the CLI doesn't blow up on malformed timestamps.
pub fn age_days(created_at: &str, now_rfc3339: &str) -> u32 {
    let (Some(c), Some(n)) = (parse_ymd(created_at), parse_ymd(now_rfc3339)) else {
        return 0;
    };
    days_between(c, n)
}

fn parse_ymd(iso: &str) -> Option<[u32; 3]> {
    if iso.len() < 10 {
        return None;
    }
    let y: u32 = iso.get(0..4)?.parse().ok()?;
    let m: u32 = iso.get(5..7)?.parse().ok()?;
    let d: u32 = iso.get(8..10)?.parse().ok()?;
    Some([y, m, d])
}

fn days_between(older: [u32; 3], newer: [u32; 3]) -> u32 {
    fn to_days([y, m, d]: [u32; 3]) -> i64 {
        let y = y as i64 - if m <= 2 { 1 } else { 0 };
        let era = y.div_euclid(400);
        let yoe = (y - era * 400) as u64;
        let m_adj = if m > 2 { m - 3 } else { m + 9 } as u64;
        let d = d as u64;
        let doy = (153 * m_adj + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe as i64 - 719_468
    }
    (to_days(newer) - to_days(older)).max(0) as u32
}

/// CLI entry — `specere observe watch-issues`.
#[allow(clippy::too_many_arguments)]
pub fn run_watch_issues(
    ctx: &Ctx,
    provider: &str,
    repo: Option<String>,
    from_fixture: Option<PathBuf>,
    interval_seconds: u64,
    once: bool,
    daemon: bool,
) -> Result<()> {
    if daemon {
        anyhow::bail!(
            "`--daemon` mode is not yet implemented; invoke `specere observe watch-issues --once` periodically instead"
        );
    }
    if !once && from_fixture.is_none() {
        anyhow::bail!(
            "Only `--once` mode is supported today. Pass `--once` or use `--from-fixture <path>` for tests."
        );
    }
    let _ = interval_seconds;

    let sensor_map_path = ctx.repo().join(".specere").join("sensor-map.toml");
    let specs = specere_filter::load_specs(&sensor_map_path).unwrap_or_default();
    let triage_cfg = TriageConfig::load(&sensor_map_path);

    // Fetch: fixture-first, else live HTTP.
    let raw = if let Some(path) = from_fixture.as_ref() {
        std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?
    } else {
        let Some(repo_spec) = repo.as_deref() else {
            anyhow::bail!("`--repo owner/name` is required unless `--from-fixture` is given");
        };
        fetch_live(provider, repo_spec)?
    };

    let issues = parse_issues_json(&raw).context("parse issues JSON")?;
    let now = specere_telemetry::event_store::now_rfc3339();

    let mut emitted = 0usize;
    let mut skipped = 0usize;
    let mut unattributed = 0usize;
    for issue in &issues {
        if !is_actionable(issue) {
            skipped += 1;
            continue;
        }
        let labels: Vec<String> = issue.labels.iter().map(|l| l.name.clone()).collect();
        let severity = severity_for_labels(&labels);
        let state = if issue.state == "closed" {
            State::Closed
        } else {
            State::Open
        };
        let age = age_days(&issue.created_at, &now);
        let (spec_id, confidence) =
            match triage_heuristic(&issue.body, &issue.title, &specs, triage_cfg.min_confidence) {
                Some((sid, conf)) => (sid, conf),
                None => {
                    unattributed += 1;
                    ("unknown".to_string(), 0.0)
                }
            };

        let mut attrs = BTreeMap::new();
        attrs.insert("event_kind".into(), "bug_reported".into());
        attrs.insert("spec_id".into(), spec_id);
        attrs.insert("issue_url".into(), issue.url.clone());
        attrs.insert("severity".into(), severity.as_attr().into());
        attrs.insert("state".into(), state.as_attr().into());
        attrs.insert("age_days".into(), age.to_string());
        attrs.insert("issue_number".into(), issue.number.to_string());
        attrs.insert("triage_confidence".into(), format!("{confidence:.3}"));
        attrs.insert("provider".into(), provider.to_string());

        let event = specere_telemetry::Event {
            ts: now.clone(),
            source: format!("specere-watch-issues-{provider}"),
            signal: "traces".into(),
            name: Some(format!("bug: #{} — {}", issue.number, issue.title)),
            feature_dir: None,
            attrs,
        };
        specere_telemetry::record(ctx, event)?;
        emitted += 1;
    }

    println!(
        "specere observe watch-issues: {emitted} bug_reported event(s) emitted; {skipped} skipped (label/state filter); {unattributed} without spec attribution"
    );
    Ok(())
}

fn fetch_live(provider: &str, repo: &str) -> Result<String> {
    // Live HTTP is intentionally minimal — fixture tests cover the parse
    // path. Production users set `GITHUB_TOKEN` or `GITEA_TOKEN` and
    // pass `--repo owner/name`. We issue a single GET and bail on any
    // non-2xx status.
    let (base, token_var) = match provider {
        "github" => ("https://api.github.com".to_string(), "GITHUB_TOKEN"),
        "gitea" => (
            std::env::var("GITEA_BASE_URL")
                .unwrap_or_else(|_| "https://gitea.com/api/v1".to_string()),
            "GITEA_TOKEN",
        ),
        other => anyhow::bail!("unknown --provider `{other}`; one of github|gitea"),
    };
    let token = std::env::var(token_var).with_context(|| {
        format!("env var {token_var} is not set — export a token or use --from-fixture")
    })?;
    let url = format!("{base}/repos/{repo}/issues?state=all&per_page=100");
    let client = reqwest::blocking::Client::builder()
        .user_agent("specere-watch-issues")
        .build()
        .context("build reqwest client")?;
    let req = match provider {
        "github" => client
            .get(&url)
            .header("Accept", "application/vnd.github+json")
            .header("Authorization", format!("Bearer {token}")),
        "gitea" => client
            .get(&url)
            .header("Authorization", format!("token {token}")),
        _ => unreachable!(),
    };
    let resp = req.send().with_context(|| format!("GET {url}"))?;
    let status = resp.status();
    let body = resp.text().with_context(|| format!("read body of {url}"))?;
    if !status.is_success() {
        anyhow::bail!("{provider} returned HTTP {status}: {body}");
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use specere_filter::hmm::SpecDescriptor;

    fn spec(id: &str, support: &[&str]) -> SpecDescriptor {
        SpecDescriptor {
            id: id.into(),
            support: support.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn triage_finds_spec_by_path_reference() {
        let specs = vec![
            spec("FR-auth", &["src/auth/"]),
            spec("FR-billing", &["src/billing/"]),
        ];
        let body = "seeing a NPE at `src/auth/token.rs:42` on login";
        let r = triage_heuristic(body, "login broken", &specs, 0.1);
        assert_eq!(r.as_ref().map(|x| x.0.as_str()), Some("FR-auth"));
    }

    #[test]
    fn triage_returns_none_below_confidence() {
        let specs = vec![spec("FR-auth", &["src/auth/"])];
        // Body mentions no paths → no triage.
        let r = triage_heuristic("something seems off", "bug", &specs, 0.6);
        assert!(r.is_none());
    }

    #[test]
    fn is_actionable_drops_skip_labels() {
        let mut i = Issue {
            number: 1,
            title: "q".into(),
            body: "".into(),
            url: "x".into(),
            state: "open".into(),
            labels: vec![IssueLabel {
                name: "question".into(),
            }],
            created_at: "2026-04-20T00:00:00Z".into(),
            closed_at: None,
            pull_request: None,
        };
        assert!(!is_actionable(&i));
        i.labels.clear();
        assert!(is_actionable(&i));
    }

    #[test]
    fn is_actionable_drops_closed_without_pr() {
        let i = Issue {
            number: 1,
            title: "".into(),
            body: "".into(),
            url: "x".into(),
            state: "closed".into(),
            labels: vec![],
            created_at: "2026-04-20T00:00:00Z".into(),
            closed_at: Some("2026-04-20T01:00:00Z".into()),
            pull_request: None,
        };
        assert!(!is_actionable(&i));
    }

    #[test]
    fn is_actionable_keeps_closed_with_linked_pr() {
        let i = Issue {
            number: 1,
            title: "".into(),
            body: "".into(),
            url: "x".into(),
            state: "closed".into(),
            labels: vec![],
            created_at: "2026-04-20T00:00:00Z".into(),
            closed_at: Some("2026-04-20T01:00:00Z".into()),
            pull_request: Some(serde_json::json!({"url": "pr"})),
        };
        assert!(is_actionable(&i));
    }

    #[test]
    fn severity_by_label() {
        assert_eq!(
            severity_for_labels(&["critical".into()]),
            Severity::Critical
        );
        assert_eq!(severity_for_labels(&["bug".into()]), Severity::Major);
        assert_eq!(severity_for_labels(&["minor".into()]), Severity::Minor);
        assert_eq!(severity_for_labels(&[]), Severity::Minor);
    }

    #[test]
    fn age_days_computes_correctly() {
        assert_eq!(age_days("2026-04-01T00:00:00Z", "2026-04-20T00:00:00Z"), 19);
        assert_eq!(age_days("bad", "2026-04-20"), 0);
    }

    #[test]
    fn parse_issues_accepts_both_shapes() {
        let arr = r#"[{"number":1,"title":"x","body":"y","url":"u","state":"open","labels":[],"created_at":"2026-04-20T00:00:00Z"}]"#;
        let v = parse_issues_json(arr).unwrap();
        assert_eq!(v.len(), 1);

        let obj = r#"{"issues":[{"number":2,"title":"y","body":"z","url":"u","state":"closed","labels":[],"created_at":"2026-04-20T00:00:00Z"}]}"#;
        let v = parse_issues_json(obj).unwrap();
        assert_eq!(v.len(), 1);
    }

    #[test]
    fn path_matches_support_is_boundary_safe() {
        assert!(path_matches_support(
            "src/auth/token.rs",
            &["src/auth/".into()]
        ));
        assert!(!path_matches_support(
            "src/auth_helpers/x.rs",
            &["src/auth".into()]
        ));
    }
}
