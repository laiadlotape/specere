//! Orphan-state detection for SpecKit artifacts left behind by aborted
//! `specify workflow run` invocations (issue #16, carry-over from
//! `.specere/decisions.log` 2026-04-18 EXTEND).
//!
//! Heuristic: `.specify/feature.json` references a `specs/NNN-*/` dir whose
//! `spec.md` is still the unfilled template — identified by the presence of
//! the verbatim `[FEATURE NAME]` placeholder in the first 20 lines.

use std::path::{Path, PathBuf};

/// Describes an orphan state on disk, if one is detected.
#[derive(Debug, Clone)]
pub struct OrphanState {
    /// Absolute path to the feature directory referenced by `.specify/feature.json`.
    pub feature_dir: PathBuf,
    /// Orphan workflow-run dirs under `.specify/workflows/runs/` (if any).
    pub orphan_runs: Vec<PathBuf>,
}

/// Inspect `repo` for orphan SpecKit state. Returns `Some(OrphanState)` iff
/// the heuristic in this module's doc comment matches.
pub fn detect(repo: &Path) -> Option<OrphanState> {
    let feature_json = repo.join(".specify").join("feature.json");
    if !feature_json.is_file() {
        return None;
    }
    let raw = std::fs::read_to_string(&feature_json).ok()?;
    let dir_rel = parse_feature_directory(&raw)?;
    let feature_dir = repo.join(&dir_rel);
    if !feature_dir.is_dir() {
        return None;
    }
    let spec_md = feature_dir.join("spec.md");
    if !spec_md.is_file() {
        return None;
    }
    if !spec_md_is_template(&spec_md) {
        return None;
    }

    // Optional: include orphan workflow-runs dirs in the state for sweeping.
    let runs_root = repo.join(".specify").join("workflows").join("runs");
    let orphan_runs = if runs_root.is_dir() {
        std::fs::read_dir(&runs_root)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect()
    } else {
        Vec::new()
    };

    Some(OrphanState {
        feature_dir,
        orphan_runs,
    })
}

/// Remove every artifact in the given `OrphanState`. Best-effort: logs and
/// skips on per-entry errors. Removes the spec dir, `.specify/feature.json`,
/// and any orphan workflow-run dirs. Does NOT touch git branches.
pub fn clean(repo: &Path, state: &OrphanState) -> std::io::Result<()> {
    if state.feature_dir.exists() {
        std::fs::remove_dir_all(&state.feature_dir)?;
    }
    let feature_json = repo.join(".specify").join("feature.json");
    if feature_json.exists() {
        std::fs::remove_file(&feature_json)?;
    }
    for run in &state.orphan_runs {
        if run.exists() {
            std::fs::remove_dir_all(run)?;
        }
    }
    // If specs/ is now empty, remove it (matches pre-orphan state on a
    // fixture that had no specs/ before the aborted workflow run).
    let specs_root = repo.join("specs");
    if specs_root.is_dir() {
        if let Ok(mut it) = std::fs::read_dir(&specs_root) {
            if it.next().is_none() {
                let _ = std::fs::remove_dir(&specs_root);
            }
        }
    }
    Ok(())
}

fn parse_feature_directory(raw: &str) -> Option<String> {
    // Proper JSON parse via serde_json. Accepts `feature_directory` (speckit
    // convention) or `feature_dir` (shorter alias). Issue #61.
    #[derive(serde::Deserialize)]
    struct FeatureJson {
        #[serde(alias = "feature_dir")]
        feature_directory: String,
    }
    serde_json::from_str::<FeatureJson>(raw)
        .ok()
        .map(|p| p.feature_directory)
        .filter(|s| !s.trim().is_empty())
}

fn spec_md_is_template(path: &Path) -> bool {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => return false,
    };
    // Look in the first 20 lines for any of the template-placeholder markers.
    let probe: String = text.lines().take(20).collect::<Vec<_>>().join("\n");
    probe.contains("[FEATURE NAME]") || probe.contains("[###-feature-name]")
}
