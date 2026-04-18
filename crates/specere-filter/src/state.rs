//! 3-state simplex over spec statuses + the per-spec [`Belief`] vector + the
//! [`TestSensor`] trait that callers implement to feed observation-likelihoods
//! into [`crate::PerSpecHMM::update_test`].

use ndarray::Array1;

/// Spec status space. Matches the prototype's indexing — `Unk = 0`, `Sat = 1`,
/// `Vio = 2`. Do not reorder without updating every hand-computed fixture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Unk = 0,
    Sat = 1,
    Vio = 2,
}

impl Status {
    pub fn index(self) -> usize {
        self as usize
    }
}

/// Per-spec belief — length-3 simplex normalised to sum to 1. Order follows
/// [`Status::index`].
pub type Belief = Array1<f64>;

/// Uniform 1/3-1/3-1/3 prior.
pub fn uniform_belief() -> Belief {
    Array1::from_elem(3, 1.0 / 3.0)
}

/// Emission model for a test outcome. Implementations return the log-likelihood
/// of the observed outcome conditional on each spec status (length-3 vector
/// indexed by [`Status`]). Keep it cheap — this is called once per
/// [`crate::PerSpecHMM::update_test`] call.
pub trait TestSensor {
    /// `log p(outcome | status)` for the three spec statuses.
    fn log_likelihood(&self, spec_id: &str, outcome: &str) -> Array1<f64>;
}
