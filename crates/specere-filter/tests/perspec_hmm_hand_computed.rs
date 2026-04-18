//! Hand-computed 2-event fixture. The expected numbers are reproduced with
//! `numpy` on the same `Motion::prototype_defaults()` matrices so this test
//! is the exact parity anchor for #40. If any of these numbers drift, the
//! motion matrices or the log-domain renormalisation has changed.

use approx::assert_abs_diff_eq;
use ndarray::{array, Array1};
use specere_filter::hmm::SpecDescriptor;
use specere_filter::{Motion, PerSpecHMM, TestSensor};

/// Deterministic test sensor: `outcome="pass"` → log-likelihood [ln 0.10, ln 0.80, ln 0.10];
/// `outcome="fail"` → log-likelihood [ln 0.30, ln 0.10, ln 0.60]. Matches a
/// typical "clean test pass pushes belief toward SAT, failure toward VIO"
/// shape. These numbers are baked into the hand-computed expected posterior.
struct DemoSensor;
impl TestSensor for DemoSensor {
    fn log_likelihood(&self, _spec_id: &str, outcome: &str) -> Array1<f64> {
        match outcome {
            "pass" => array![0.10_f64.ln(), 0.80_f64.ln(), 0.10_f64.ln()],
            "fail" => array![0.30_f64.ln(), 0.10_f64.ln(), 0.60_f64.ln()],
            other => panic!("unexpected outcome: {other}"),
        }
    }
}

fn one_spec() -> Vec<SpecDescriptor> {
    vec![SpecDescriptor {
        id: "FR-001".into(),
        support: vec!["src/foo.rs".into()],
    }]
}

#[test]
fn uniform_prior_plus_pass_matches_bayes_closed_form() {
    // With a uniform (1/3, 1/3, 1/3) prior and sensor row [0.10, 0.80, 0.10],
    // the posterior is [0.10, 0.80, 0.10] (uniform prior cancels).
    let mut f = PerSpecHMM::new(one_spec(), Motion::prototype_defaults());
    f.update_test("FR-001", "pass", &DemoSensor).unwrap();
    let m = f.marginal("FR-001").unwrap();
    let expected = array![0.10, 0.80, 0.10];
    assert_abs_diff_eq!(m[0], expected[0], epsilon = 1e-9);
    assert_abs_diff_eq!(m[1], expected[1], epsilon = 1e-9);
    assert_abs_diff_eq!(m[2], expected[2], epsilon = 1e-9);
}

#[test]
fn predict_then_pass_matches_hand_computed() {
    // Step 1: predict on a touched file. Uniform prior (1/3, 1/3, 1/3) times
    // t_mix = 0.7·t_good + 0.3·t_bad (prototype-aligned matrices).
    // Row-stochastic matrices → the row-sum-over-columns is (col-sum)/3.
    //
    // Prototype t_good col sums: [0.17, 2.47, 0.36]
    // Prototype t_bad  col sums: [0.17, 0.48, 2.35]
    // t_mix col sums = 0.7·t_good + 0.3·t_bad:
    //   col 0 (UNK): 0.7·0.17 + 0.3·0.17 = 0.170
    //   col 1 (SAT): 0.7·2.47 + 0.3·0.48 = 1.729 + 0.144 = 1.873
    //   col 2 (VIO): 0.7·0.36 + 0.3·2.35 = 0.252 + 0.705 = 0.957
    // Divide by 3 for the uniform-row contraction:
    //   post-predict prior ≈ [0.05667, 0.62433, 0.31900]
    let mut f = PerSpecHMM::new(one_spec(), Motion::prototype_defaults());
    f.predict(&["src/foo.rs"]);
    let after_predict = f.marginal("FR-001").unwrap();
    assert_abs_diff_eq!(after_predict[0], 0.170 / 3.0, epsilon = 1e-9);
    assert_abs_diff_eq!(after_predict[1], 1.873 / 3.0, epsilon = 1e-9);
    assert_abs_diff_eq!(after_predict[2], 0.957 / 3.0, epsilon = 1e-9);

    // Step 2: update on "pass" — log-domain Bayes with DemoSensor's local
    // pass row [0.10, 0.80, 0.10] (not DefaultTestSensor; this test isolates
    // motion-matrix arithmetic from sensor calibration).
    //   Un-norm: [0.05667·0.10, 0.62433·0.80, 0.31900·0.10]
    //          = [0.005667,     0.499467,     0.031900]
    //   total  = 0.537033 → posterior ≈ [0.01055, 0.93004, 0.05940]
    f.update_test("FR-001", "pass", &DemoSensor).unwrap();
    let post = f.marginal("FR-001").unwrap();
    let un_norm = [
        (0.170 / 3.0) * 0.10,
        (1.873 / 3.0) * 0.80,
        (0.957 / 3.0) * 0.10,
    ];
    let total: f64 = un_norm.iter().sum();
    assert_abs_diff_eq!(post[0], un_norm[0] / total, epsilon = 1e-9);
    assert_abs_diff_eq!(post[1], un_norm[1] / total, epsilon = 1e-9);
    assert_abs_diff_eq!(post[2], un_norm[2] / total, epsilon = 1e-9);
    // Sanity: the posterior must be a valid simplex.
    assert_abs_diff_eq!(post.sum(), 1.0, epsilon = 1e-9);
}

#[test]
fn update_test_rejects_unknown_spec() {
    let mut f = PerSpecHMM::new(one_spec(), Motion::prototype_defaults());
    let err = f.update_test("FR-999", "pass", &DemoSensor);
    assert!(err.is_err(), "expected error for unknown spec");
}

#[test]
fn hundred_event_stream_has_no_nan_and_sums_to_one() {
    // FR-P4 smoke: no NaN/Inf, every row normalised within 1e-9. 100 events
    // alternate predict + test on a single spec.
    let mut f = PerSpecHMM::new(one_spec(), Motion::prototype_defaults());
    for i in 0..100 {
        f.predict(&["src/foo.rs"]);
        let outcome = if i % 2 == 0 { "pass" } else { "fail" };
        f.update_test("FR-001", outcome, &DemoSensor).unwrap();
        let m = f.marginal("FR-001").unwrap();
        for v in m.iter() {
            assert!(v.is_finite(), "non-finite at step {i}: {v}");
            assert!((0.0..=1.0).contains(v), "off simplex at step {i}: {v}");
        }
        assert_abs_diff_eq!(m.sum(), 1.0, epsilon = 1e-9);
    }
}
