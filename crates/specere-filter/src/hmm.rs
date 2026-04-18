//! `PerSpecHMM` — independent per-spec forward recursion. Mirrors the
//! prototype's baseline filter (no cross-spec coupling; that lands in #41).
//!
//! Belief state: one length-3 simplex per spec, stored row-wise in an
//! `Array2<f64>` indexed by [`crate::Status`] columns. All updates are
//! log-domain-safe — `update_test` runs Bayes in log space with a stable
//! log-sum-exp normalisation, and `predict` normalises rows defensively
//! because numerical drift across long motion chains can push row-sums
//! slightly off unity.

use std::collections::HashMap;

use anyhow::{anyhow, Result};
use ndarray::{Array1, Array2};

use crate::motion::Motion;
use crate::state::{Belief, TestSensor};

const EPS: f64 = 1e-12;

/// A single spec plus the set of files whose edits should advance its belief.
#[derive(Debug, Clone)]
pub struct SpecDescriptor {
    pub id: String,
    /// File paths (or prefixes — caller's contract) that imply this spec
    /// when touched by an agent write.
    pub support: Vec<String>,
}

/// Baseline per-spec Bayesian filter. Beliefs are held in an `Array2<f64>`
/// of shape `(n_specs, 3)`; each row is a simplex over [`crate::Status`].
pub struct PerSpecHMM {
    pub(crate) spec_ids: Vec<String>,
    pub(crate) idx: HashMap<String, usize>,
    pub(crate) support: Vec<Vec<String>>,
    pub(crate) motion: Motion,
    pub(crate) belief: Array2<f64>,
}

impl PerSpecHMM {
    /// Build with uniform 1/3-1/3-1/3 priors on every spec.
    pub fn new(specs: Vec<SpecDescriptor>, motion: Motion) -> Self {
        let n = specs.len();
        let mut spec_ids = Vec::with_capacity(n);
        let mut idx = HashMap::with_capacity(n);
        let mut support = Vec::with_capacity(n);
        for (i, s) in specs.into_iter().enumerate() {
            idx.insert(s.id.clone(), i);
            spec_ids.push(s.id);
            support.push(s.support);
        }
        let belief = Array2::from_elem((n, 3), 1.0 / 3.0);
        Self {
            spec_ids,
            idx,
            support,
            motion,
            belief,
        }
    }

    pub fn spec_ids(&self) -> &[String] {
        &self.spec_ids
    }

    pub fn num_specs(&self) -> usize {
        self.spec_ids.len()
    }

    /// Return the current belief for one spec as a length-3 vector
    /// `[p(Unk), p(Sat), p(Vio)]`. Errors if the id is unknown.
    pub fn marginal(&self, spec_id: &str) -> Result<Belief> {
        let i = self
            .idx
            .get(spec_id)
            .ok_or_else(|| anyhow!("unknown spec id: {spec_id}"))?;
        Ok(self.belief.row(*i).to_owned())
    }

    /// Return the full belief matrix (n_specs × 3) as a fresh owned copy.
    pub fn all_marginals(&self) -> Array2<f64> {
        self.belief.clone()
    }

    /// Motion step. For each spec, if any of its support files intersects
    /// `files_touched`, advance its row by the mixture transition
    /// `t_mix = α·t_good + (1-α)·t_bad`; otherwise by the identity-leak
    /// `t_leak`. Each row is renormalised defensively — the underlying
    /// matrices are row-stochastic so the sum should already be 1, but
    /// long motion chains accumulate float drift.
    pub fn predict(&mut self, files_touched: &[&str]) {
        let t_mix = self.motion.t_mix();
        let t_leak = &self.motion.t_leak;
        for i in 0..self.spec_ids.len() {
            let touched = self.support[i]
                .iter()
                .any(|p| files_touched.iter().any(|f| f == p));
            let m = if touched { &t_mix } else { t_leak };
            let row = self.belief.row(i).to_owned();
            let next = row.dot(m);
            self.belief.row_mut(i).assign(&normalise(&next));
        }
    }

    /// Observation step for a test outcome. Multiplies the current belief
    /// by the sensor's log-likelihood in log space, then renormalises via
    /// log-sum-exp. Unknown spec ids return an error — callers should
    /// either ignore stray events or declare the spec first.
    pub fn update_test<S: TestSensor>(
        &mut self,
        spec_id: &str,
        outcome: &str,
        sensor: &S,
    ) -> Result<()> {
        let i = self
            .idx
            .get(spec_id)
            .ok_or_else(|| anyhow!("unknown spec id: {spec_id}"))?;
        let prior = self.belief.row(*i).to_owned();
        let log_lik = sensor.log_likelihood(spec_id, outcome);
        debug_assert_eq!(log_lik.len(), 3, "TestSensor must return a length-3 vector");
        let mut log_post: Array1<f64> = prior.mapv(|p| (p + EPS).ln()) + &log_lik;
        let m = log_post.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        log_post.mapv_inplace(|x| x - m);
        let post = log_post.mapv(f64::exp);
        let total = post.sum();
        let next = if total > EPS {
            post / total
        } else {
            Array1::from_elem(3, 1.0 / 3.0)
        };
        self.belief.row_mut(*i).assign(&next);
        Ok(())
    }
}

fn normalise(v: &Array1<f64>) -> Array1<f64> {
    let total = v.sum();
    if total > EPS {
        v / total
    } else {
        Array1::from_elem(v.len(), 1.0 / v.len() as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniform_prior_sums_to_one_on_construction() {
        let specs = vec![SpecDescriptor {
            id: "FR-001".into(),
            support: vec!["src/foo.rs".into()],
        }];
        let f = PerSpecHMM::new(specs, Motion::prototype_defaults());
        let m = f.marginal("FR-001").unwrap();
        assert!((m.sum() - 1.0).abs() < 1e-12);
        assert_eq!(m.len(), 3);
    }

    #[test]
    fn predict_leaves_rows_on_simplex() {
        let specs = vec![
            SpecDescriptor {
                id: "FR-001".into(),
                support: vec!["src/foo.rs".into()],
            },
            SpecDescriptor {
                id: "FR-002".into(),
                support: vec!["src/bar.rs".into()],
            },
        ];
        let mut f = PerSpecHMM::new(specs, Motion::prototype_defaults());
        f.predict(&["src/foo.rs"]);
        for i in 0..f.num_specs() {
            let row = f.all_marginals().row(i).to_owned();
            assert!(
                (row.sum() - 1.0).abs() < 1e-9,
                "row {i} off simplex: sum = {}",
                row.sum()
            );
        }
    }

    #[test]
    fn predict_only_moves_touched_specs() {
        let specs = vec![
            SpecDescriptor {
                id: "FR-001".into(),
                support: vec!["src/foo.rs".into()],
            },
            SpecDescriptor {
                id: "FR-002".into(),
                support: vec!["src/bar.rs".into()],
            },
        ];
        let mut f = PerSpecHMM::new(specs, Motion::prototype_defaults());
        let before = f.marginal("FR-002").unwrap();
        f.predict(&["src/foo.rs"]);
        let after_foo = f.marginal("FR-001").unwrap();
        let after_bar = f.marginal("FR-002").unwrap();
        // FR-002 wasn't touched — only the tiny identity-leak moved it. Its
        // deviation from the uniform prior must be strictly smaller than
        // FR-001's (which got the full mix transition).
        let delta_bar = (&after_bar - &before).mapv(f64::abs).sum();
        let delta_foo = (&after_foo - &before).mapv(f64::abs).sum();
        assert!(
            delta_foo > 10.0 * delta_bar,
            "touched/untouched ratio too small: foo Δ={delta_foo}, bar Δ={delta_bar}"
        );
    }
}
