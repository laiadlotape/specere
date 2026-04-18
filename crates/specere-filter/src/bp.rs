//! `FactorGraphBP` — per-spec HMM plus loopy belief propagation over the
//! directed coupling graph. Issue #41 / FR-P4-006.
//!
//! After each motion or observation step, `n_iter` BP sweeps propagate a
//! small damped message along every edge. The pair factor is a 3×3 log
//! matrix that is zero except for the `(Vio, Vio)` corner, which carries
//! `ln(kappa)` — "if src is VIOLATED, push dst toward VIOLATED." Messages
//! are log-sum-exp combined, mean-centred so a uniform src produces no
//! bias, and added to the destination belief with the `damp` factor.
//!
//! The graph must be a DAG (enforced in [`crate::CouplingGraph::require_dag`]).
//! True cycles escape to RBPF (#42).

use anyhow::Result;
use ndarray::{Array1, Array2};

use crate::coupling::CouplingGraph;
use crate::hmm::{PerSpecHMM, SpecDescriptor};
use crate::motion::Motion;
use crate::state::{Belief, TestSensor};

const EPS: f64 = 1e-12;

/// Prototype defaults, pulled from `prototype/mini_specs/filter.py`.
pub const DEFAULT_KAPPA: f64 = 1.4;
pub const DEFAULT_DAMP: f64 = 0.3;
pub const DEFAULT_N_ITER: usize = 1;

pub struct FactorGraphBP {
    hmm: PerSpecHMM,
    /// Resolved edges as `(src_idx, dst_idx)` — unknown spec ids are silently
    /// dropped at construction. The loader already guaranteed no cycles.
    edges: Vec<(usize, usize)>,
    kappa: f64,
    damp: f64,
    n_iter: usize,
}

impl FactorGraphBP {
    pub fn new(specs: Vec<SpecDescriptor>, motion: Motion, coupling: &CouplingGraph) -> Self {
        let hmm = PerSpecHMM::new(specs, motion);
        let edges = coupling
            .edges
            .iter()
            .filter_map(|(src, dst)| {
                let i = hmm.idx.get(src)?;
                let j = hmm.idx.get(dst)?;
                Some((*i, *j))
            })
            .collect();
        Self {
            hmm,
            edges,
            kappa: DEFAULT_KAPPA,
            damp: DEFAULT_DAMP,
            n_iter: DEFAULT_N_ITER,
        }
    }

    pub fn with_kappa(mut self, kappa: f64) -> Self {
        self.kappa = kappa;
        self
    }
    pub fn with_damp(mut self, damp: f64) -> Self {
        self.damp = damp;
        self
    }
    pub fn with_n_iter(mut self, n_iter: usize) -> Self {
        self.n_iter = n_iter;
        self
    }

    pub fn num_specs(&self) -> usize {
        self.hmm.num_specs()
    }
    pub fn spec_ids(&self) -> &[String] {
        self.hmm.spec_ids()
    }
    pub fn marginal(&self, spec_id: &str) -> Result<Belief> {
        self.hmm.marginal(spec_id)
    }
    pub fn all_marginals(&self) -> Array2<f64> {
        self.hmm.all_marginals()
    }

    /// Number of coupling edges resolved against known spec ids. Edges that
    /// reference unknown specs are dropped at construction; this returns
    /// the *kept* count.
    pub fn num_edges(&self) -> usize {
        self.edges.len()
    }

    pub fn predict(&mut self, files_touched: &[&str]) {
        self.hmm.predict(files_touched);
        self.run_bp();
    }

    pub fn update_test<S: TestSensor>(
        &mut self,
        spec_id: &str,
        outcome: &str,
        sensor: &S,
    ) -> Result<()> {
        self.hmm.update_test(spec_id, outcome, sensor)?;
        self.run_bp();
        Ok(())
    }

    fn run_bp(&mut self) {
        for _ in 0..self.n_iter {
            self.bp_step();
        }
    }

    /// One sweep over every directed edge. For each `(src, dst)` edge:
    ///   1. Build `M[s, d] = log_src[s] + log_phi[s, d]` — the pair factor
    ///      combined with the src-side log-belief.
    ///   2. Marginalise out `s` via log-sum-exp → length-3 log-message.
    ///   3. Mean-centre so uniform sources produce no bias.
    ///   4. Accumulate `damp * msg` into `dst`'s log-belief.
    ///
    /// After all edges, renormalise each row that received a message.
    fn bp_step(&mut self) {
        if self.edges.is_empty() {
            return;
        }
        let log_phi = pair_factor(self.kappa);
        let log_snapshot: Array2<f64> = self.hmm.belief.mapv(|p| (p + EPS).ln());
        let mut new_log = log_snapshot.clone();
        let mut touched = vec![false; self.hmm.num_specs()];

        for &(i, j) in &self.edges {
            let log_src = log_snapshot.row(i);
            let mut m_matrix = Array2::<f64>::zeros((3, 3));
            for s in 0..3 {
                for d in 0..3 {
                    m_matrix[[s, d]] = log_src[s] + log_phi[[s, d]];
                }
            }
            let mut msg = Array1::<f64>::zeros(3);
            for d in 0..3 {
                let col = m_matrix.column(d);
                let m_max = col.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                let lse = m_max + col.iter().map(|x| (x - m_max).exp()).sum::<f64>().ln();
                msg[d] = lse;
            }
            let mean = msg.sum() / 3.0;
            msg.mapv_inplace(|x| x - mean);
            let mut dst_row = new_log.row_mut(j);
            for d in 0..3 {
                dst_row[d] += self.damp * msg[d];
            }
            touched[j] = true;
        }

        // Only renormalise rows that actually received a message — untouched
        // rows already held normalised beliefs and the EPS floor in
        // `log_snapshot` would otherwise introduce a sub-1e-12 drift.
        for (i, was_touched) in touched.iter().enumerate() {
            if !*was_touched {
                continue;
            }
            let row = new_log.row(i);
            let m_max = row.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let exped: Array1<f64> = row.mapv(|x| (x - m_max).exp());
            let total = exped.sum();
            let normed = if total > EPS {
                exped / total
            } else {
                Array1::from_elem(3, 1.0 / 3.0)
            };
            self.hmm.belief.row_mut(i).assign(&normed);
        }
    }
}

/// 3×3 log-pair-factor. Zero everywhere except `(Vio, Vio) = ln(kappa)`.
fn pair_factor(kappa: f64) -> Array2<f64> {
    let mut m = Array2::<f64>::zeros((3, 3));
    m[[2, 2]] = kappa.ln();
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::motion::Motion;

    fn spec(id: &str) -> SpecDescriptor {
        SpecDescriptor {
            id: id.into(),
            support: vec![format!("src/{id}.rs")],
        }
    }

    #[test]
    fn empty_coupling_matches_per_spec_hmm() {
        // With no edges, BP is a no-op — FactorGraphBP ≡ PerSpecHMM.
        let specs = vec![spec("FR-001"), spec("FR-002")];
        let motion = Motion::prototype_defaults();
        let mut bp = FactorGraphBP::new(specs.clone(), motion.clone(), &CouplingGraph::default());
        let mut hmm = PerSpecHMM::new(specs, motion);
        bp.predict(&["src/FR-001.rs"]);
        hmm.predict(&["src/FR-001.rs"]);
        for id in ["FR-001", "FR-002"] {
            let a = bp.marginal(id).unwrap();
            let b = hmm.marginal(id).unwrap();
            for k in 0..3 {
                assert!((a[k] - b[k]).abs() < 1e-12, "diverged on {id}[{k}]");
            }
        }
    }

    #[test]
    fn kappa_one_is_no_op() {
        // `kappa = 1.0` → `ln(kappa) = 0` everywhere in the pair factor, so
        // BP should leave beliefs exactly as PerSpecHMM would have. This is
        // the flat-out sanity check on the BP code path: no coupling bias
        // leaks in from the mechanics.
        let specs = vec![spec("A"), spec("B")];
        let motion = Motion::prototype_defaults();
        let coupling = CouplingGraph {
            edges: vec![("A".into(), "B".into())],
        };
        let mut bp = FactorGraphBP::new(specs.clone(), motion.clone(), &coupling)
            .with_kappa(1.0)
            .with_n_iter(5);
        let mut hmm = PerSpecHMM::new(specs, motion);
        bp.predict(&[]);
        hmm.predict(&[]);
        for id in ["A", "B"] {
            let a = bp.marginal(id).unwrap();
            let b = hmm.marginal(id).unwrap();
            for k in 0..3 {
                assert!(
                    (a[k] - b[k]).abs() < 1e-12,
                    "kappa=1 leaked coupling bias on {id}[{k}]: bp={} hmm={}",
                    a[k],
                    b[k],
                );
            }
        }
    }

    #[test]
    fn violated_src_pushes_dst_toward_violated() {
        // Anchor A's belief at VIO (via repeated "fail" test updates) and
        // confirm B's VIO mass rises relative to the no-coupling baseline.
        use ndarray::array;

        struct FailSensor;
        impl TestSensor for FailSensor {
            fn log_likelihood(&self, _spec_id: &str, _outcome: &str) -> Array1<f64> {
                // Strong VIO-pulling sensor.
                array![0.30_f64.ln(), 0.05_f64.ln(), 0.85_f64.ln()]
            }
        }

        let specs = || vec![spec("A"), spec("B")];
        let motion = Motion::prototype_defaults();
        let coupling = CouplingGraph {
            edges: vec![("A".into(), "B".into())],
        };
        let mut bp = FactorGraphBP::new(specs(), motion.clone(), &coupling).with_n_iter(3);
        let mut hmm = PerSpecHMM::new(specs(), motion);

        for _ in 0..5 {
            bp.update_test("A", "fail", &FailSensor).unwrap();
            hmm.update_test("A", "fail", &FailSensor).unwrap();
        }
        let bp_b_vio = bp.marginal("B").unwrap()[2];
        let hmm_b_vio = hmm.marginal("B").unwrap()[2];
        assert!(
            bp_b_vio > hmm_b_vio,
            "coupling failed to lift B's VIO mass: bp={bp_b_vio:.6}, hmm={hmm_b_vio:.6}"
        );
    }
}
