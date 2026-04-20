//! `[rbpf]` section loader for `.specere/sensor-map.toml`.
//!
//! Format:
//!
//! ```toml
//! [rbpf]
//! # Spec ids whose joint distribution is tracked by the particle filter.
//! # Typically the set of FRs inside a cyclic coupling cluster where BP
//! # would reject the loader with `DAG required`.
//! cluster = ["FR-001", "FR-002", "FR-005"]
//! # Number of particles. Default 200.
//! n_particles = 200
//! # Seed. Default 42 — keep fixed for replay-determinism.
//! seed = 42
//! # ESS resample threshold as a fraction of n_particles (0..1). Default 0.5.
//! resample_ess_frac = 0.5
//! ```
//!
//! A missing file, missing `[rbpf]` section, or empty `cluster` all
//! yield `None` — the CLI then falls through to BP or HMM.

use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

const DEFAULT_N_PARTICLES: usize = 200;
const DEFAULT_SEED: u64 = 42;
const DEFAULT_RESAMPLE_ESS_FRAC: f64 = 0.5;

#[derive(Debug, Clone)]
pub struct RbpfConfig {
    pub cluster: Vec<String>,
    pub n_particles: usize,
    pub seed: u64,
    pub resample_ess_frac: f64,
}

impl RbpfConfig {
    /// Load from a sensor-map path. Returns `None` when the file is
    /// absent, the `[rbpf]` section is missing, or `cluster` is empty.
    pub fn load(path: &Path) -> Result<Option<Self>> {
        if !path.exists() {
            return Ok(None);
        }
        let raw =
            std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        Self::from_toml_str(&raw)
    }

    pub fn from_toml_str(raw: &str) -> Result<Option<Self>> {
        #[derive(Deserialize)]
        struct Root {
            rbpf: Option<Section>,
        }
        #[derive(Deserialize)]
        struct Section {
            #[serde(default)]
            cluster: Vec<String>,
            n_particles: Option<usize>,
            seed: Option<u64>,
            resample_ess_frac: Option<f64>,
        }
        let parsed: Root = toml::from_str(raw).context("parse sensor-map.toml for [rbpf]")?;
        let Some(section) = parsed.rbpf else {
            return Ok(None);
        };
        if section.cluster.is_empty() {
            return Ok(None);
        }
        Ok(Some(Self {
            cluster: section.cluster,
            n_particles: section.n_particles.unwrap_or(DEFAULT_N_PARTICLES),
            seed: section.seed.unwrap_or(DEFAULT_SEED),
            resample_ess_frac: section
                .resample_ess_frac
                .unwrap_or(DEFAULT_RESAMPLE_ESS_FRAC)
                .clamp(0.1, 1.0),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_or_missing_section_returns_none() {
        let none1 = RbpfConfig::from_toml_str("schema_version = 1\n[specs]\n").unwrap();
        assert!(none1.is_none());
        let none2 = RbpfConfig::from_toml_str("[rbpf]\ncluster = []\n").unwrap();
        assert!(none2.is_none());
    }

    #[test]
    fn populated_section_parses_defaults() {
        let cfg = RbpfConfig::from_toml_str("[rbpf]\ncluster = [\"FR-001\", \"FR-002\"]\n")
            .unwrap()
            .expect("section present");
        assert_eq!(cfg.cluster, vec!["FR-001", "FR-002"]);
        assert_eq!(cfg.n_particles, DEFAULT_N_PARTICLES);
        assert_eq!(cfg.seed, DEFAULT_SEED);
        assert!((cfg.resample_ess_frac - 0.5).abs() < 1e-9);
    }

    #[test]
    fn explicit_values_override_defaults() {
        let cfg = RbpfConfig::from_toml_str(
            "[rbpf]\ncluster = [\"FR-001\"]\nn_particles = 512\nseed = 7\nresample_ess_frac = 0.25\n",
        )
        .unwrap()
        .expect("section present");
        assert_eq!(cfg.n_particles, 512);
        assert_eq!(cfg.seed, 7);
        assert!((cfg.resample_ess_frac - 0.25).abs() < 1e-9);
    }

    #[test]
    fn resample_frac_clamped_to_safe_range() {
        let cfg = RbpfConfig::from_toml_str(
            "[rbpf]\ncluster = [\"FR-001\"]\nresample_ess_frac = 0.001\n",
        )
        .unwrap()
        .unwrap();
        assert!(cfg.resample_ess_frac >= 0.1, "too-small frac clamped up");
        let cfg2 =
            RbpfConfig::from_toml_str("[rbpf]\ncluster = [\"FR-001\"]\nresample_ess_frac = 5.0\n")
                .unwrap()
                .unwrap();
        assert!(cfg2.resample_ess_frac <= 1.0, "too-large frac clamped down");
    }

    #[test]
    fn missing_file_returns_none_not_error() {
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        let nonexistent = dir.path().join("sensor-map.toml");
        let r = RbpfConfig::load(&nonexistent).unwrap();
        assert!(r.is_none());
    }
}
