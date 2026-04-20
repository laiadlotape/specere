//! `RBPF` — Rao-Blackwellised particle filter for coupling clusters BP can
//! neither converge on nor route through a DAG. Issue #42 / pre-FR-P4-002.
//!
//! Each particle samples a joint discrete assignment over a designated
//! `cluster` subset of specs; non-cluster specs use the per-spec HMM
//! backbone (the "Rao-Blackwellised" part — exact per-spec marginalisation
//! conditional on the particle's discrete hypothesis). Weights update by
//! the full measurement likelihood conditional on each particle's cluster
//! state; resampling triggers when the effective sample size drops below
//! `resample_ess_frac × N`.
//!
//! Determinism: a seeded `StdRng` drives every stochastic step. Same seed
//! and same event stream ⇒ bit-identical posterior, which `#43` relies on
//! for the FR-P4-004 golden-file lock.
//!
//! **Gate-A Python-prototype parity (FR-P4-002, <2 pp).** Deferred — the
//! parity anchor needs a one-time export of the Python prototype on a
//! fixed fixture. Tracked as the last follow-up before #42 closes; for
//! now, [`rbpf_gate_a.rs`] validates behaviour qualitatively against a
//! known-injected ground truth.

use std::collections::HashMap;

use anyhow::{anyhow, Result};
use ndarray::{Array1, Array2};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use crate::hmm::{PerSpecHMM, SpecDescriptor};
use crate::motion::Motion;
use crate::state::{Belief, TestSensor};

const EPS: f64 = 1e-12;

/// Prototype default from `prototype/mini_specs/filter.py`.
pub const DEFAULT_N_PARTICLES: usize = 512;
pub const DEFAULT_RESAMPLE_ESS_FRAC: f64 = 0.3;
pub const DEFAULT_SEED: u64 = 0x5E_5E_5E_5E;

pub struct RBPF {
    hmm: PerSpecHMM,
    cluster: Vec<usize>,
    cluster_id_to_pos: HashMap<String, usize>,
    /// Particles: `(n_particles, cluster_len)` — each row is a sampled
    /// joint-assignment over the cluster. Values ∈ {0, 1, 2}.
    particles: Array2<u8>,
    /// Log-weights, renormalised lazily via log-sum-exp.
    log_weights: Array1<f64>,
    resample_ess_frac: f64,
    rng: StdRng,
}

impl RBPF {
    pub fn new(
        specs: Vec<SpecDescriptor>,
        motion: Motion,
        cluster_spec_ids: &[&str],
        n_particles: usize,
        seed: u64,
    ) -> Self {
        let hmm = PerSpecHMM::new(specs, motion);
        let mut cluster: Vec<usize> = Vec::new();
        let mut cluster_id_to_pos: HashMap<String, usize> = HashMap::new();
        for sid in cluster_spec_ids {
            if let Some(i) = hmm.idx.get(*sid) {
                cluster_id_to_pos.insert((*sid).to_string(), cluster.len());
                cluster.push(*i);
            }
        }
        let mut rng = StdRng::seed_from_u64(seed);
        let mut particles = Array2::<u8>::zeros((n_particles, cluster.len()));
        for row in 0..n_particles {
            for col in 0..cluster.len() {
                particles[[row, col]] = rng.gen_range(0..3);
            }
        }
        let log_weights = Array1::from_elem(n_particles, -(n_particles as f64).ln());
        Self {
            hmm,
            cluster,
            cluster_id_to_pos,
            particles,
            log_weights,
            resample_ess_frac: DEFAULT_RESAMPLE_ESS_FRAC,
            rng,
        }
    }

    pub fn num_specs(&self) -> usize {
        self.hmm.num_specs()
    }

    pub fn n_particles(&self) -> usize {
        self.particles.nrows()
    }

    pub fn cluster_len(&self) -> usize {
        self.cluster.len()
    }

    /// Overwrite one spec's belief — mirrors `PerSpecHMM::set_belief`.
    /// For non-cluster specs this forwards directly to the HMM. For
    /// cluster specs we re-sample each particle's state for this spec
    /// from the supplied marginal. Joint structure is *not* preserved
    /// (particles become independent across the specs being set) — this
    /// is adequate for cross-session resume (FR-P6) where the saved
    /// posterior is already marginalised anyway.
    pub fn set_belief(&mut self, spec_id: &str, belief: &[f64]) {
        assert_eq!(belief.len(), 3, "set_belief requires a length-3 vector");
        self.hmm.set_belief(spec_id, belief);
        if let Some(&pos) = self.cluster_id_to_pos.get(spec_id) {
            // Marginal-preserving particle re-seed: sample each row's
            // value for this cluster position from the given marginal.
            // Normalised defensively in case input drifted off-simplex.
            let arr = Array1::from_vec(belief.to_vec());
            let total: f64 = arr.iter().sum();
            let normed = if total > EPS {
                arr.mapv(|v| v / total)
            } else {
                Array1::from_elem(3, 1.0 / 3.0)
            };
            for n in 0..self.n_particles() {
                self.particles[[n, pos]] = sample_categorical(&normed, &mut self.rng);
            }
            // Reset weights to uniform — the old log-weights refer to the
            // pre-seed particle distribution and are stale after re-sampling.
            let n = self.n_particles() as f64;
            self.log_weights = Array1::from_elem(self.n_particles(), -(n).ln());
        }
    }

    pub fn predict(&mut self, files_touched: &[&str]) {
        self.hmm.predict(files_touched);
        // Step each particle's cluster-spec states by sampling from the row
        // of the transition matrix indexed by the particle's current value.
        let t_mix = self.hmm.motion.t_mix();
        let t_leak = &self.hmm.motion.t_leak;
        for (pos, &i) in self.cluster.iter().enumerate() {
            let touched = self.hmm.support[i]
                .iter()
                .any(|p| files_touched.iter().any(|f| f == p));
            let m = if touched { &t_mix } else { t_leak };
            for n in 0..self.n_particles() {
                let cur = self.particles[[n, pos]] as usize;
                let row = m.row(cur);
                self.particles[[n, pos]] = sample_categorical(&row.to_owned(), &mut self.rng);
            }
        }
    }

    pub fn update_test<S: TestSensor>(
        &mut self,
        spec_id: &str,
        outcome: &str,
        sensor: &S,
    ) -> Result<()> {
        let log_lik_vec = sensor.log_likelihood(spec_id, outcome);
        if log_lik_vec.len() != 3 {
            return Err(anyhow!("TestSensor must return a length-3 vector"));
        }
        if let Some(&pos) = self.cluster_id_to_pos.get(spec_id) {
            // Reweight particles by the likelihood conditional on their
            // sampled cluster state.
            for n in 0..self.n_particles() {
                let s = self.particles[[n, pos]] as usize;
                self.log_weights[n] += log_lik_vec[s];
            }
        }
        // Always run the RB backbone update for consistent non-cluster marginals.
        self.hmm.update_test(spec_id, outcome, sensor)?;
        if self.ess() < self.resample_ess_frac * self.n_particles() as f64 {
            self.resample();
        }
        Ok(())
    }

    pub fn marginal(&self, spec_id: &str) -> Result<Belief> {
        if let Some(&pos) = self.cluster_id_to_pos.get(spec_id) {
            let w = normalised_weights(&self.log_weights);
            let mut counts = Array1::<f64>::zeros(3);
            for n in 0..self.n_particles() {
                let s = self.particles[[n, pos]] as usize;
                counts[s] += w[n];
            }
            let total = counts.sum();
            if total > EPS {
                Ok(counts / total)
            } else {
                Ok(Array1::from_elem(3, 1.0 / 3.0))
            }
        } else {
            self.hmm.marginal(spec_id)
        }
    }

    pub fn all_marginals(&self) -> Result<Array2<f64>> {
        let mut out = Array2::<f64>::zeros((self.num_specs(), 3));
        for (sid, _) in self.hmm.idx.iter() {
            let i = self.hmm.idx[sid];
            out.row_mut(i).assign(&self.marginal(sid)?);
        }
        Ok(out)
    }

    fn ess(&self) -> f64 {
        let w = normalised_weights(&self.log_weights);
        1.0 / w.iter().map(|x| x * x).sum::<f64>().max(EPS)
    }

    fn resample(&mut self) {
        let w = normalised_weights(&self.log_weights);
        let n = self.n_particles();
        let mut new_particles = Array2::<u8>::zeros(self.particles.raw_dim());
        // Systematic categorical resampling via a cumulative-sum search.
        let mut cdf = Vec::with_capacity(n);
        let mut running = 0.0;
        for v in w.iter() {
            running += *v;
            cdf.push(running);
        }
        for m in 0..n {
            let u: f64 = self.rng.gen();
            let mut idx = cdf.iter().position(|c| *c >= u).unwrap_or(n - 1);
            if idx >= n {
                idx = n - 1;
            }
            for col in 0..self.cluster.len() {
                new_particles[[m, col]] = self.particles[[idx, col]];
            }
        }
        self.particles = new_particles;
        self.log_weights = Array1::from_elem(n, -(n as f64).ln());
    }
}

fn normalised_weights(log_w: &Array1<f64>) -> Array1<f64> {
    let m = log_w.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let exped: Array1<f64> = log_w.mapv(|x| (x - m).exp());
    let total = exped.sum();
    if total > EPS {
        exped / total
    } else {
        Array1::from_elem(log_w.len(), 1.0 / log_w.len() as f64)
    }
}

fn sample_categorical(probs: &Array1<f64>, rng: &mut StdRng) -> u8 {
    let u: f64 = rng.gen();
    let mut running = 0.0;
    for (i, p) in probs.iter().enumerate() {
        running += *p;
        if u <= running {
            return i as u8;
        }
    }
    (probs.len() - 1) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use ndarray::array;

    fn spec(id: &str) -> SpecDescriptor {
        SpecDescriptor {
            id: id.into(),
            support: vec![format!("src/{id}.rs")],
        }
    }

    struct FailSensor;
    impl TestSensor for FailSensor {
        fn log_likelihood(&self, _: &str, _: &str) -> Array1<f64> {
            array![0.30_f64.ln(), 0.05_f64.ln(), 0.85_f64.ln()]
        }
    }

    #[test]
    fn seeded_construction_is_deterministic() {
        let specs = vec![spec("A"), spec("B")];
        let motion = Motion::prototype_defaults();
        let rbpf_1 = RBPF::new(specs.clone(), motion.clone(), &["A"], 64, 42);
        let rbpf_2 = RBPF::new(specs, motion, &["A"], 64, 42);
        assert_eq!(rbpf_1.particles, rbpf_2.particles);
    }

    #[test]
    fn different_seeds_diverge() {
        let specs = vec![spec("A")];
        let motion = Motion::prototype_defaults();
        let rbpf_1 = RBPF::new(specs.clone(), motion.clone(), &["A"], 64, 1);
        let rbpf_2 = RBPF::new(specs, motion, &["A"], 64, 2);
        assert_ne!(rbpf_1.particles, rbpf_2.particles);
    }

    #[test]
    fn empty_cluster_tracks_backbone_on_non_cluster_spec() {
        // With an empty cluster, RBPF's marginal must equal the backbone
        // HMM's marginal exactly — all specs are "non-cluster".
        let specs = vec![spec("A")];
        let motion = Motion::prototype_defaults();
        let mut rbpf = RBPF::new(specs.clone(), motion.clone(), &[], 32, 7);
        let mut hmm = PerSpecHMM::new(specs, motion);
        for _ in 0..3 {
            rbpf.update_test("A", "fail", &FailSensor).unwrap();
            hmm.update_test("A", "fail", &FailSensor).unwrap();
        }
        let a = rbpf.marginal("A").unwrap();
        let b = hmm.marginal("A").unwrap();
        for k in 0..3 {
            assert!(
                (a[k] - b[k]).abs() < 1e-12,
                "empty-cluster RBPF diverged from HMM on A[{k}]: {} vs {}",
                a[k],
                b[k]
            );
        }
    }

    #[test]
    fn cluster_spec_under_fail_stream_concentrates_on_vio() {
        // Repeated fail events on a cluster spec must push the particle-
        // weighted marginal toward VIO. Seeded for stability.
        let specs = vec![spec("A"), spec("B")];
        let motion = Motion::prototype_defaults();
        let mut rbpf = RBPF::new(specs, motion, &["A"], 256, 13);
        for _ in 0..8 {
            rbpf.update_test("A", "fail", &FailSensor).unwrap();
        }
        let m = rbpf.marginal("A").unwrap();
        assert!(
            m[2] > 0.70,
            "VIO mass failed to concentrate after 8 fails: {m:?}"
        );
        // The simplex invariant must still hold.
        assert!((m.sum() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn rejects_length_mismatch_from_sensor() {
        struct BadSensor;
        impl TestSensor for BadSensor {
            fn log_likelihood(&self, _: &str, _: &str) -> Array1<f64> {
                array![0.5_f64.ln(), 0.5_f64.ln()]
            }
        }
        let specs = vec![spec("A")];
        let motion = Motion::prototype_defaults();
        let mut rbpf = RBPF::new(specs, motion, &["A"], 16, 99);
        let err = rbpf.update_test("A", "pass", &BadSensor);
        assert!(err.is_err(), "expected error for wrong-arity sensor");
    }
}
