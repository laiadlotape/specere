//! Transition matrices for the motion step. Ported from
//! `prototype/mini_specs/filter.py` + `prototype/mini_specs/world.py`.
//! These are Gate-A-validated starting values; do not re-tune without a new
//! parity export against the Python prototype.

use ndarray::{array, Array2};

/// Three 3×3 transition matrices + the `assumed_good_rate` that mixes the
/// "good write" and "bad write" models when a spec's support file is touched.
/// Untouched specs use the identity-leak matrix (nearly identity, small
/// drift toward UNK to model clock-independent uncertainty).
#[derive(Debug, Clone)]
pub struct Motion {
    /// Transition when a "good" write lands on a supporting file.
    pub t_good: Array2<f64>,
    /// Transition when a "bad" write lands on a supporting file.
    pub t_bad: Array2<f64>,
    /// Transition when no supporting file was touched (identity + leak).
    pub t_leak: Array2<f64>,
    /// Mixing weight on `t_good` for the touched case.
    /// Mix: `assumed_good * t_good + (1 - assumed_good) * t_bad`.
    pub assumed_good: f64,
}

impl Motion {
    /// Prototype defaults. Rows are current-status, columns next-status.
    /// Order is [UNK, SAT, VIO] — matches [`crate::Status::index`].
    pub fn prototype_defaults() -> Self {
        // Good write: strong pull toward SAT; small chance of a latent VIO.
        let t_good = array![[0.20, 0.75, 0.05], [0.05, 0.93, 0.02], [0.10, 0.70, 0.20],];
        // Bad write: pull toward VIO; UNK tends to stay or drift to VIO.
        let t_bad = array![[0.25, 0.15, 0.60], [0.05, 0.40, 0.55], [0.05, 0.05, 0.90],];
        // Identity-leak: nearly identity, a 1% drift toward UNK on Sat/Vio
        // so old beliefs age toward uncertainty.
        let t_leak = array![[0.98, 0.01, 0.01], [0.01, 0.98, 0.01], [0.01, 0.01, 0.98],];
        Self {
            t_good,
            t_bad,
            t_leak,
            assumed_good: 0.7,
        }
    }

    /// `assumed_good * t_good + (1 - assumed_good) * t_bad`.
    pub fn t_mix(&self) -> Array2<f64> {
        &self.t_good * self.assumed_good + &self.t_bad * (1.0 - self.assumed_good)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows_sum_to_one(m: &Array2<f64>) -> bool {
        m.rows().into_iter().all(|r| (r.sum() - 1.0).abs() < 1e-9)
    }

    #[test]
    fn prototype_matrices_are_row_stochastic() {
        let m = Motion::prototype_defaults();
        assert!(rows_sum_to_one(&m.t_good), "t_good rows must sum to 1");
        assert!(rows_sum_to_one(&m.t_bad), "t_bad rows must sum to 1");
        assert!(rows_sum_to_one(&m.t_leak), "t_leak rows must sum to 1");
        assert!(rows_sum_to_one(&m.t_mix()), "t_mix rows must sum to 1");
    }

    #[test]
    fn mix_at_full_good_equals_t_good() {
        let mut m = Motion::prototype_defaults();
        m.assumed_good = 1.0;
        let mix = m.t_mix();
        let diff = (&mix - &m.t_good)
            .mapv(f64::abs)
            .iter()
            .cloned()
            .fold(0.0_f64, f64::max);
        assert!(diff < 1e-12, "mix diverged from t_good by {diff}");
    }
}
