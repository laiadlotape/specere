//! `.specere/posterior.toml` — serialised per-spec belief surface.
//!
//! Format (sorted by spec_id for FR-P4-004 determinism):
//!
//! ```toml
//! cursor = "2026-04-18T15:00:00Z"   # last event ts processed
//! schema_version = 1
//!
//! [[entries]]
//! spec_id = "FR-001"
//! p_unk = 0.120
//! p_sat = 0.680
//! p_vio = 0.200
//! entropy = 0.874
//! last_updated = "2026-04-18T15:00:00Z"
//! ```
//!
//! Write is atomic: serialise → write to a sibling `.specere/posterior.toml.tmp`
//! → rename over the real path. Avoids partial files after a crash.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use ndarray::Array1;
use serde::{Deserialize, Serialize};

const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Entry {
    pub spec_id: String,
    pub p_unk: f64,
    pub p_sat: f64,
    pub p_vio: f64,
    pub entropy: f64,
    pub last_updated: String,
}

impl Entry {
    pub fn from_belief(spec_id: &str, belief: &Array1<f64>, ts: &str) -> Self {
        let p_unk = belief[0];
        let p_sat = belief[1];
        let p_vio = belief[2];
        Self {
            spec_id: spec_id.to_string(),
            p_unk,
            p_sat,
            p_vio,
            entropy: shannon_entropy(&[p_unk, p_sat, p_vio]),
            last_updated: ts.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Posterior {
    pub cursor: Option<String>,
    pub schema_version: u32,
    pub entries: Vec<Entry>,
}

impl Default for Posterior {
    fn default() -> Self {
        Self {
            cursor: None,
            schema_version: SCHEMA_VERSION,
            entries: Vec::new(),
        }
    }
}

impl Posterior {
    pub fn default_path(repo: &Path) -> PathBuf {
        repo.join(".specere").join("posterior.toml")
    }

    /// Load posterior or return a fresh default if the file is absent.
    pub fn load_or_default(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw =
            std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        let p: Self = toml::from_str(&raw).context("parse posterior.toml")?;
        Ok(p)
    }

    /// Atomic write: serialise, write to `path.tmp`, rename over `path`.
    pub fn write_atomic(&mut self, path: &Path) -> Result<()> {
        // Sort entries by spec_id for deterministic TOML output.
        self.entries.sort_by(|a, b| a.spec_id.cmp(&b.spec_id));
        self.schema_version = SCHEMA_VERSION;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let serialised = toml::to_string(self).context("serialise posterior")?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, serialised).with_context(|| format!("write {}", tmp.display()))?;
        std::fs::rename(&tmp, path)
            .with_context(|| format!("rename {} -> {}", tmp.display(), path.display()))?;
        Ok(())
    }
}

fn shannon_entropy(probs: &[f64]) -> f64 {
    const EPS: f64 = 1e-12;
    -probs.iter().map(|p| p * (p.max(EPS).ln())).sum::<f64>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ndarray::array;

    #[test]
    fn entropy_is_max_at_uniform() {
        let h_uniform = shannon_entropy(&[1.0 / 3.0; 3]);
        let h_concentrated = shannon_entropy(&[0.01, 0.98, 0.01]);
        assert!(h_uniform > h_concentrated);
        // ln 3 ≈ 1.0986
        assert!((h_uniform - 3.0_f64.ln()).abs() < 1e-9);
    }

    #[test]
    fn entry_from_belief_roundtrips() {
        let e = Entry::from_belief("FR-001", &array![0.10, 0.70, 0.20], "2026-04-18T12:00:00Z");
        assert_eq!(e.spec_id, "FR-001");
        assert!((e.p_unk - 0.10).abs() < 1e-12);
        assert!((e.p_sat - 0.70).abs() < 1e-12);
        assert!((e.p_vio - 0.20).abs() < 1e-12);
        assert!(e.entropy > 0.0);
    }

    #[test]
    fn default_is_empty() {
        let p = Posterior::default();
        assert!(p.cursor.is_none());
        assert_eq!(p.schema_version, SCHEMA_VERSION);
        assert!(p.entries.is_empty());
    }

    #[test]
    fn write_is_sorted_by_spec_id() {
        let mut p = Posterior {
            cursor: Some("2026-04-18T12:00:00Z".into()),
            schema_version: SCHEMA_VERSION,
            entries: vec![
                Entry::from_belief("FR-002", &array![0.1, 0.8, 0.1], "2026-04-18T12:00:00Z"),
                Entry::from_belief("FR-001", &array![0.1, 0.1, 0.8], "2026-04-18T12:00:00Z"),
            ],
        };
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("posterior.toml");
        p.write_atomic(&path).unwrap();
        let reloaded = Posterior::load_or_default(&path).unwrap();
        assert_eq!(reloaded.entries[0].spec_id, "FR-001");
        assert_eq!(reloaded.entries[1].spec_id, "FR-002");
    }
}
