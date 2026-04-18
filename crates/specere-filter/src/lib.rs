//! Per-spec Bayesian filter over agent-telemetry events.
//!
//! Phase 4 / issue #40 — the baseline `PerSpecHMM` forward recursion. Takes
//! a [`Motion`] model (transition matrices ported verbatim from the ReSearch
//! prototype) and a per-spec belief over the three-state simplex
//! [`Status::Unk`], [`Status::Sat`], [`Status::Vio`], and advances it in two
//! ways:
//!
//! - [`PerSpecHMM::predict`] — motion step from an agent write; specs whose
//!   support files were touched get the mixture transition, the rest get
//!   the identity-leak.
//! - [`PerSpecHMM::update_test`] — observation step from a test outcome;
//!   Bayes in log-domain against a caller-supplied emission likelihood.
//!
//! Issues #41 (FactorGraphBP) and #42 (RBPF) extend this baseline; issue #43
//! wires the CLI. Hyperparameters match `prototype/mini_specs/filter.py`.

pub mod bp;
pub mod coupling;
pub mod hmm;
pub mod motion;
pub mod rbpf;
pub mod state;

pub use bp::FactorGraphBP;
pub use coupling::CouplingGraph;
pub use hmm::PerSpecHMM;
pub use motion::Motion;
pub use rbpf::RBPF;
pub use state::{Belief, Status, TestSensor};
