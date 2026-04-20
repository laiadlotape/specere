//! 3-state simplex over spec statuses + the per-spec [`Belief`] vector + the
//! [`TestSensor`] trait that callers implement to feed observation-likelihoods
//! into [`crate::PerSpecHMM::update_test`].
//!
//! **Calibration (FR-EQ-002).** v1.0.4's fixed `α_sat = 0.92` / `α_vio = 0.90`
//! assumed test suites always discriminate SAT from VIO well. That's false
//! when tests are tautological, over-mocked, or happy-path-only. v1.0.5
//! introduces [`Calibration`] — a per-spec quality multiplier, derived from
//! mutation kill rate and test-smell penalty, that compresses alphas toward
//! uninformative when the evidence is weak. At `quality = 1.0` the sensor
//! behaves exactly as before (Gate-A parity preserved).

use ndarray::Array1;

const EPS: f64 = 1e-6;

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

/// Per-spec evidence-quality calibration (FR-EQ-002). Derived from mutation
/// kill rate × test-smell penalty. Callers build one [`Calibration`] per
/// spec and feed a matching [`CalibratedTestSensor`] into the filter.
///
/// The `quality` field is the single knob that compresses the sensor's
/// discriminative power. At `1.0`, alphas match the prototype defaults
/// (`α_sat = 0.92`, `α_vio = 0.90`, `α_unk = 0.55`) — so Gate-A parity
/// (FR-P4-002) is preserved bit-for-bit when no evidence has arrived.
/// At the floor (`0.3`) alphas compress halfway toward uninformative.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Calibration {
    pub quality: f64,
    pub alpha_sat: f64,
    pub alpha_vio: f64,
    pub alpha_unk: f64,
}

/// Prototype constants — kept private; all external callers go through
/// [`Calibration::prototype`] or [`Calibration::from_evidence`].
const ALPHA_SAT_PROTO: f64 = 0.92;
const ALPHA_VIO_PROTO: f64 = 0.90;
const ALPHA_UNK_PROTO: f64 = 0.55;

impl Calibration {
    /// Prototype-default alphas — exactly what v1.0.4 used unconditionally.
    pub const fn prototype() -> Self {
        Self {
            quality: 1.0,
            alpha_sat: ALPHA_SAT_PROTO,
            alpha_vio: ALPHA_VIO_PROTO,
            alpha_unk: ALPHA_UNK_PROTO,
        }
    }

    /// Construct from per-spec evidence signals (FR-EQ-002 § 5 formula):
    ///
    /// ```text
    /// q       = clamp(0.3, kill_rate × smell_penalty, 1.0)
    /// α_sat   = 0.55 + q × 0.37       → 0.92 at q=1, ~0.66 at q=0.3
    /// α_vio   = 0.45 + q × 0.45       → 0.90 at q=1, ~0.58 at q=0.3
    /// α_unk   = 0.55                  (unchanged)
    /// ```
    ///
    /// `kill_rate` is `caught / (caught + missed)` with timeouts counted as
    /// missed and unviable excluded — zero evidence (no mutations run) is
    /// signalled by `kill_rate = 1.0`, which returns prototype alphas
    /// (back-compat: repos that never run `specere evaluate mutations`
    /// behave exactly as v1.0.4).
    ///
    /// `smell_penalty` is `clamp(0.3, 1.0 - 0.15×n_smells, 1.0)` upstream
    /// of this function.
    pub fn from_evidence(kill_rate: f64, smell_penalty: f64) -> Self {
        let q = (kill_rate * smell_penalty).clamp(0.3, 1.0);
        Self {
            quality: q,
            alpha_sat: ALPHA_UNK_PROTO + q * (ALPHA_SAT_PROTO - ALPHA_UNK_PROTO),
            alpha_vio: (1.0 - ALPHA_UNK_PROTO) + q * (ALPHA_VIO_PROTO - (1.0 - ALPHA_UNK_PROTO)),
            alpha_unk: ALPHA_UNK_PROTO,
        }
    }

    /// Extended constructor for FR-HM-052b: compresses quality further
    /// when the spec's harness-cluster peers show systematic flakiness.
    ///
    /// ```text
    /// q_base    = from_evidence formula
    /// q_cluster = q_base × (1 − 0.5 × cluster_flakiness_score)
    /// q_final   = clamp(0.3, q_cluster, 1.0)
    /// ```
    ///
    /// `cluster_flakiness_score ∈ [0, 1]` is the mean flakiness across
    /// the harness files that share a cluster with this spec's tests.
    /// When the cluster is pristine (score = 0), output is bit-identical
    /// to [`Self::from_evidence`]. When the cluster is maximally flaky
    /// (score ≥ 0.5), quality is roughly halved — alphas compress hard
    /// toward `α_unk`.
    pub fn from_cluster_evidence(
        kill_rate: f64,
        smell_penalty: f64,
        cluster_flakiness_score: f64,
    ) -> Self {
        let base = (kill_rate * smell_penalty).clamp(0.3, 1.0);
        let cluster_penalty = (1.0 - 0.5 * cluster_flakiness_score.clamp(0.0, 1.0)).max(0.3);
        let q = (base * cluster_penalty).clamp(0.3, 1.0);
        Self {
            quality: q,
            alpha_sat: ALPHA_UNK_PROTO + q * (ALPHA_SAT_PROTO - ALPHA_UNK_PROTO),
            alpha_vio: (1.0 - ALPHA_UNK_PROTO) + q * (ALPHA_VIO_PROTO - (1.0 - ALPHA_UNK_PROTO)),
            alpha_unk: ALPHA_UNK_PROTO,
        }
    }

    /// Likelihood table for a test outcome under this calibration. Shape:
    /// log-probability for each status (`Unk, Sat, Vio`) given the outcome.
    /// Matches the prototype's numerical output at `quality = 1.0`.
    pub fn log_likelihood(&self, outcome: &str) -> Array1<f64> {
        let clip = |x: f64| x.max(EPS).ln();
        match outcome {
            "pass" => ndarray::array![
                clip(self.alpha_unk),
                clip(self.alpha_sat),
                clip(1.0 - self.alpha_vio)
            ],
            "fail" => ndarray::array![
                clip(1.0 - self.alpha_unk),
                clip(1.0 - self.alpha_sat),
                clip(self.alpha_vio)
            ],
            _ => {
                let u = (1.0_f64 / 3.0).ln();
                ndarray::array![u, u, u]
            }
        }
    }
}

impl Default for Calibration {
    fn default() -> Self {
        Self::prototype()
    }
}

/// A [`TestSensor`] that uses a static [`Calibration`] for every spec. The
/// CLI wraps this in a map keyed by spec_id when per-spec calibration is
/// in play — `DefaultTestSensor` is the "same alphas for every spec"
/// adapter that callers wanting v1.0.4 behaviour can use unchanged.
pub struct CalibratedTestSensor {
    calibration: Calibration,
}

impl CalibratedTestSensor {
    pub fn new(calibration: Calibration) -> Self {
        Self { calibration }
    }

    pub fn calibration(&self) -> &Calibration {
        &self.calibration
    }
}

impl TestSensor for CalibratedTestSensor {
    fn log_likelihood(&self, _spec_id: &str, outcome: &str) -> Array1<f64> {
        self.calibration.log_likelihood(outcome)
    }
}

/// Per-spec sensor — one `Calibration` per spec_id. Unknown specs fall
/// back to prototype defaults, matching v1.0.4 behaviour.
pub struct PerSpecTestSensor {
    map: std::collections::HashMap<String, Calibration>,
    fallback: Calibration,
}

impl PerSpecTestSensor {
    pub fn new() -> Self {
        Self {
            map: std::collections::HashMap::new(),
            fallback: Calibration::prototype(),
        }
    }

    pub fn insert(&mut self, spec_id: impl Into<String>, calibration: Calibration) {
        self.map.insert(spec_id.into(), calibration);
    }

    pub fn calibration_for(&self, spec_id: &str) -> &Calibration {
        self.map.get(spec_id).unwrap_or(&self.fallback)
    }
}

impl Default for PerSpecTestSensor {
    fn default() -> Self {
        Self::new()
    }
}

impl TestSensor for PerSpecTestSensor {
    fn log_likelihood(&self, spec_id: &str, outcome: &str) -> Array1<f64> {
        self.calibration_for(spec_id).log_likelihood(outcome)
    }
}

#[cfg(test)]
mod calibration_tests {
    use super::*;
    use approx::assert_abs_diff_eq;

    #[test]
    fn prototype_matches_v104_constants() {
        let c = Calibration::prototype();
        assert_abs_diff_eq!(c.quality, 1.0, epsilon = 1e-12);
        assert_abs_diff_eq!(c.alpha_sat, 0.92, epsilon = 1e-12);
        assert_abs_diff_eq!(c.alpha_vio, 0.90, epsilon = 1e-12);
        assert_abs_diff_eq!(c.alpha_unk, 0.55, epsilon = 1e-12);
    }

    #[test]
    fn from_evidence_at_full_quality_equals_prototype() {
        let c = Calibration::from_evidence(1.0, 1.0);
        let p = Calibration::prototype();
        // Formula gives α_sat = 0.55 + 1.0*(0.92-0.55) = 0.92 exactly. Must
        // match prototype at `q=1` so Gate-A parity (FR-P4-002) is preserved.
        assert_abs_diff_eq!(c.alpha_sat, p.alpha_sat, epsilon = 1e-12);
        assert_abs_diff_eq!(c.alpha_vio, p.alpha_vio, epsilon = 1e-12);
        assert_abs_diff_eq!(c.alpha_unk, p.alpha_unk, epsilon = 1e-12);
    }

    #[test]
    fn from_evidence_clamps_at_0_3_floor() {
        // kill_rate=0, smell_penalty=0.5 → q would be 0, clamps to 0.3.
        let c = Calibration::from_evidence(0.0, 0.5);
        assert_abs_diff_eq!(c.quality, 0.3, epsilon = 1e-12);
        // α_sat = 0.55 + 0.3*(0.92-0.55) = 0.661
        assert_abs_diff_eq!(c.alpha_sat, 0.661, epsilon = 1e-9);
        // α_vio = 0.45 + 0.3*(0.90-0.45) = 0.585
        assert_abs_diff_eq!(c.alpha_vio, 0.585, epsilon = 1e-9);
    }

    #[test]
    fn from_evidence_mid_quality() {
        // kill_rate=0.6, smell_penalty=1.0 → q=0.6.
        let c = Calibration::from_evidence(0.6, 1.0);
        assert_abs_diff_eq!(c.quality, 0.6, epsilon = 1e-12);
        // α_sat = 0.55 + 0.6*0.37 = 0.772
        assert_abs_diff_eq!(c.alpha_sat, 0.772, epsilon = 1e-9);
    }

    #[test]
    fn from_cluster_evidence_with_zero_flakiness_matches_from_evidence() {
        // Cluster is pristine → identical to baseline formula.
        let a = Calibration::from_evidence(0.8, 1.0);
        let b = Calibration::from_cluster_evidence(0.8, 1.0, 0.0);
        assert_abs_diff_eq!(a.quality, b.quality, epsilon = 1e-12);
        assert_abs_diff_eq!(a.alpha_sat, b.alpha_sat, epsilon = 1e-12);
        assert_abs_diff_eq!(a.alpha_vio, b.alpha_vio, epsilon = 1e-12);
    }

    #[test]
    fn from_cluster_evidence_compresses_quality_on_flaky_cluster() {
        // cluster_flakiness_score = 0.4 → cluster_penalty = 1.0 − 0.5×0.4 = 0.8
        // Base q = 0.8 × 1.0 = 0.8. Final q = 0.8 × 0.8 = 0.64.
        let c = Calibration::from_cluster_evidence(0.8, 1.0, 0.4);
        assert_abs_diff_eq!(c.quality, 0.64, epsilon = 1e-9);
        // α_sat = 0.55 + 0.64 × 0.37 = 0.7868
        assert_abs_diff_eq!(c.alpha_sat, 0.7868, epsilon = 1e-9);
    }

    #[test]
    fn from_cluster_evidence_clamps_at_floor() {
        // Worst-case: low kill_rate × max flakiness → clamps at 0.3.
        let c = Calibration::from_cluster_evidence(0.1, 0.5, 1.0);
        assert_abs_diff_eq!(c.quality, 0.3, epsilon = 1e-9);
    }

    #[test]
    fn from_cluster_evidence_tolerates_out_of_range_flakiness() {
        // Negative input should saturate at 0 (no penalty);
        // > 1 should saturate at 1.
        let a = Calibration::from_cluster_evidence(1.0, 1.0, -0.5);
        let b = Calibration::from_cluster_evidence(1.0, 1.0, 0.0);
        assert_abs_diff_eq!(a.quality, b.quality, epsilon = 1e-12);
        let c = Calibration::from_cluster_evidence(1.0, 1.0, 2.0);
        let d = Calibration::from_cluster_evidence(1.0, 1.0, 1.0);
        assert_abs_diff_eq!(c.quality, d.quality, epsilon = 1e-12);
    }

    #[test]
    fn calibrated_sensor_matches_prototype_default_sensor() {
        // A CalibratedTestSensor(prototype) must return the same log-
        // likelihoods as the legacy DefaultTestSensor — this is the
        // backbone of bit-identical Gate-A parity under the new code path.
        use crate::drive::DefaultTestSensor;
        let sensor = CalibratedTestSensor::new(Calibration::prototype());
        let default = DefaultTestSensor;
        for outcome in ["pass", "fail", "unknown-fallback"] {
            let a = sensor.log_likelihood("FR-001", outcome);
            let b = default.log_likelihood("FR-001", outcome);
            for k in 0..3 {
                assert_abs_diff_eq!(a[k], b[k], epsilon = 1e-12);
            }
        }
    }

    #[test]
    fn per_spec_sensor_falls_back_to_prototype_on_unknown() {
        let mut per_spec = PerSpecTestSensor::new();
        per_spec.insert("FR-001", Calibration::from_evidence(0.3, 1.0));
        // FR-001 has a calibration, FR-999 doesn't — falls back to prototype.
        let a = per_spec.calibration_for("FR-001");
        let b = per_spec.calibration_for("FR-999");
        assert!(a.quality < 1.0);
        assert_eq!(b, &Calibration::prototype());
    }
}
