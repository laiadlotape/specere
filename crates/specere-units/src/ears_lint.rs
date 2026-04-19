//! `specere lint ears` runtime — reads `.specere/lint/ears.toml`, applies each
//! rule to the active feature's `spec.md` Functional Requirements section, and
//! prints findings. Exits 0 regardless of findings (advisory per FR-P2-003).
//!
//! Issue #25.

use std::path::{Path, PathBuf};

use regex::Regex;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct RulesFile {
    #[serde(default, rename = "rules")]
    rules: Vec<RawRule>,
}

#[derive(Debug, Deserialize)]
struct RawRule {
    id: String,
    severity: String,
    #[serde(default)]
    #[allow(dead_code)]
    description: String,
    #[allow(dead_code)]
    scope: String,
    pattern: String,
    #[serde(default)]
    bad_match: bool,
    #[serde(default)]
    condition_only: bool,
}

/// A single lint finding.
#[derive(Debug)]
pub struct Finding {
    pub rule_id: String,
    pub severity: String,
    pub excerpt: String,
}

/// Run the lint given a repo path. Never errors on missing state — returns
/// empty findings + a skip reason for the caller to print. Errors only on
/// malformed rules.toml (which is a real bug worth surfacing).
pub fn run(repo: &Path) -> anyhow::Result<LintOutcome> {
    let feature_json = repo.join(".specify").join("feature.json");
    let rules_path = repo.join(".specere/lint/ears.toml");

    if !rules_path.is_file() {
        return Ok(LintOutcome::Skipped(format!(
            "rules file missing at {} — run `specere add ears-linter`",
            rules_path.display()
        )));
    }
    if !feature_json.is_file() {
        return Ok(LintOutcome::Skipped(
            "no active feature — skipping ears lint (`.specify/feature.json` absent)".into(),
        ));
    }

    let feat_raw = std::fs::read_to_string(&feature_json)?;
    let feature_dir_rel = parse_feature_directory(&feat_raw).map_err(|e| {
        anyhow::anyhow!(
            "parsing {}: {e} — expected a JSON object with key `feature_directory` (or alias `feature_dir`)",
            feature_json.display()
        )
    })?;
    let spec_md = repo.join(&feature_dir_rel).join("spec.md");
    if !spec_md.is_file() {
        return Ok(LintOutcome::Skipped(format!(
            "spec.md missing at {}",
            spec_md.display()
        )));
    }

    let rules_raw = std::fs::read_to_string(&rules_path)?;
    let rules: RulesFile = toml::from_str(&rules_raw)
        .map_err(|e| anyhow::anyhow!("parsing {}: {e}", rules_path.display()))?;
    let compiled: Vec<CompiledRule> = rules
        .rules
        .into_iter()
        .map(CompiledRule::compile)
        .collect::<Result<_, _>>()?;

    let spec_body = std::fs::read_to_string(&spec_md)?;
    let bullets = extract_fr_bullets(&spec_body);

    let mut findings = Vec::new();
    for bullet in &bullets {
        for rule in &compiled {
            let pattern_matches = rule.pattern.is_match(bullet);
            let rule_fires = if rule.bad_match {
                // bad_match=true: warn when pattern DOES match
                pattern_matches
            } else if rule.condition_only {
                // condition_only=true + bad_match=false: rule was designed to
                // catch condition-casing drift; removed in #25 because as
                // specified it could never fire. Kept here for backward compat
                // if a future rules.toml reintroduces it with a real design —
                // for now: no-op.
                false
            } else {
                // Default: warn when pattern does NOT match.
                !pattern_matches
            };
            if rule_fires {
                let excerpt = bullet.lines().next().unwrap_or(bullet).trim().to_string();
                let excerpt = truncate(&excerpt, 120);
                findings.push(Finding {
                    rule_id: rule.id.clone(),
                    severity: rule.severity.clone(),
                    excerpt,
                });
            }
        }
    }

    Ok(LintOutcome::Ran {
        feature_dir: feature_dir_rel.into(),
        bullet_count: bullets.len(),
        findings,
    })
}

pub enum LintOutcome {
    /// Rules + feature present; ran successfully. May have zero findings.
    Ran {
        feature_dir: PathBuf,
        bullet_count: usize,
        findings: Vec<Finding>,
    },
    /// Ran as no-op with a human-readable reason.
    Skipped(String),
}

struct CompiledRule {
    id: String,
    severity: String,
    pattern: Regex,
    bad_match: bool,
    condition_only: bool,
}

impl CompiledRule {
    fn compile(raw: RawRule) -> anyhow::Result<Self> {
        let pattern = Regex::new(&raw.pattern)
            .map_err(|e| anyhow::anyhow!("invalid regex for rule `{}`: {e}", raw.id))?;
        Ok(Self {
            id: raw.id,
            severity: raw.severity,
            pattern,
            bad_match: raw.bad_match,
            condition_only: raw.condition_only,
        })
    }
}

/// Pull the Functional Requirements section's bullet lines out of spec.md.
/// Heuristic: everything between `### Functional Requirements` and the next
/// `### ` heading (or EOF), keeping only lines that start with `-`.
fn extract_fr_bullets(spec: &str) -> Vec<String> {
    let mut in_fr = false;
    let mut bullets = Vec::new();
    for line in spec.lines() {
        let trimmed = line.trim_start();
        if let Some(heading) = line.strip_prefix("### ") {
            if heading
                .trim()
                .eq_ignore_ascii_case("Functional Requirements")
            {
                in_fr = true;
                continue;
            }
            // Different subsection — leave FR.
            if in_fr {
                break;
            }
        }
        if line.starts_with("## ") && in_fr {
            break;
        }
        if in_fr && trimmed.starts_with('-') {
            bullets.push(line.to_string());
        }
    }
    bullets
}

/// Parse `.specify/feature.json`. Accepts `feature_directory` (the speckit
/// convention) or the shorter `feature_dir` alias (what hook authors
/// commonly reach for). Issue #61 — previously a hand-rolled string search
/// that was both fragile (broke on any JSON whitespace reshuffle) and
/// rigid (only the exact key name would match).
fn parse_feature_directory(raw: &str) -> anyhow::Result<String> {
    #[derive(serde::Deserialize)]
    struct FeatureJson {
        #[serde(alias = "feature_dir")]
        feature_directory: String,
    }
    let parsed: FeatureJson =
        serde_json::from_str(raw).map_err(|e| anyhow::anyhow!("JSON parse error: {e}"))?;
    if parsed.feature_directory.trim().is_empty() {
        anyhow::bail!("`feature_directory` is empty");
    }
    Ok(parsed.feature_directory)
}

/// Truncate a string to at most `max` BYTES, snapping to the nearest char
/// boundary at or below `max` so we never slice mid-codepoint. Appends `…`
/// when truncation actually happened.
///
/// Issue #63: the previous `&s[..max]` slice panicked on multi-byte UTF-8
/// (`≥`, `→`, `€`, smart-quotes, em-dashes) because `max` landed inside a
/// codepoint. Common in technical specs.
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }
    let mut boundary = max;
    while boundary > 0 && !s.is_char_boundary(boundary) {
        boundary -= 1;
    }
    format!("{}…", &s[..boundary])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_under_max_is_noop() {
        assert_eq!(truncate("short", 10), "short");
    }

    #[test]
    fn truncate_at_char_boundary_snaps_ascii() {
        // "hello world" → max=5 gives "hello…"
        let out = truncate("hello world", 5);
        assert_eq!(out, "hello…");
    }

    #[test]
    fn truncate_snaps_to_char_boundary_on_multibyte() {
        // Issue #63 repro — `≥` is 3 bytes (e2 89 a5). Any max landing inside
        // those 3 bytes must snap back to the byte before the char.
        let s = "rate ≥ 60 Hz sustained";
        // Try several `max` values that would have panicked pre-fix.
        for bad in [6, 7] {
            let out = truncate(s, bad);
            // Must not panic and must end with the ellipsis marker.
            assert!(out.ends_with('…'), "output should signal truncation: {out}");
            // Must be a valid UTF-8 string — `to_string()` above already
            // enforces this, but confirm semantically: the output should NOT
            // contain the full `≥` (truncation happens before it).
            assert!(
                !out.contains('≥'),
                "output should have truncated before the multi-byte char: {out}"
            );
        }
    }

    #[test]
    fn truncate_preserves_multibyte_chars_when_boundary_permits() {
        let s = "a≥b"; // bytes: 1 + 3 + 1 = 5
                       // max=4 snaps to byte 4 which IS a char boundary (start of 'b'),
                       // so the output keeps `≥` and truncates before `b`.
        let out = truncate(s, 4);
        assert_eq!(out, "a≥…");
    }
}
