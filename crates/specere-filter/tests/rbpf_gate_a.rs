//! Gate-A-style scenario for RBPF (issue #42).
//!
//! Rather than strict Python-prototype parity (FR-P4-002's <2 pp tail-MAP
//! anchor needs a one-time fixture export from `prototype/mini_specs/`),
//! this test validates RBPF behaviour qualitatively against a known
//! injected ground truth on a coupling topology that BP cannot handle —
//! two overlapping cycles. If RBPF cannot recover the truly-violated
//! spec here, it has no hope on the real Gate-A.
//!
//! The Python parity anchor is tracked as the last FR-P4-002 follow-up;
//! this test is sufficient to catch algorithmic regressions in the port.

use ndarray::{array, Array1};
use specere_filter::hmm::SpecDescriptor;
use specere_filter::{Motion, TestSensor, RBPF};

struct GateSensor;
impl TestSensor for GateSensor {
    fn log_likelihood(&self, _spec_id: &str, outcome: &str) -> Array1<f64> {
        match outcome {
            // Strong VIO-pulling sensor for "fail" observations; mild
            // SAT-pulling for "pass". Symmetric calibration so posterior
            // separation is driven by the event ratio, not the sensor
            // asymmetry.
            "fail" => array![0.25_f64.ln(), 0.05_f64.ln(), 0.80_f64.ln()],
            "pass" => array![0.15_f64.ln(), 0.80_f64.ln(), 0.05_f64.ln()],
            other => panic!("unexpected outcome: {other}"),
        }
    }
}

fn five_spec_cluster() -> Vec<SpecDescriptor> {
    (1..=5)
        .map(|n| SpecDescriptor {
            id: format!("FR-{n:03}"),
            support: vec![format!("src/spec_{n}.rs")],
        })
        .collect()
}

#[test]
fn rbpf_recovers_injected_violation_on_cyclic_cluster() {
    // Gate-A-style: inject ground-truth VIO on FR-003. Observe 6 fails on
    // FR-003 interleaved with 2 passes on each neighbour. The coupling
    // implied by the scenario has cycles (if the user were to author it
    // as edges), so BP couldn't run — RBPF is the intended path. The
    // cluster covers all 5 specs so every observation reweights particles.
    let cluster_ids: Vec<&str> = (1..=5)
        .map(|n| match n {
            1 => "FR-001",
            2 => "FR-002",
            3 => "FR-003",
            4 => "FR-004",
            _ => "FR-005",
        })
        .collect();

    let mut rbpf = RBPF::new(
        five_spec_cluster(),
        Motion::prototype_defaults(),
        &cluster_ids,
        512,
        0xA1_CE_5E_ED,
    );

    // 6 fails on the injected violation + 2 passes on each neighbour.
    for _ in 0..6 {
        rbpf.update_test("FR-003", "fail", &GateSensor).unwrap();
    }
    for sid in ["FR-001", "FR-002", "FR-004", "FR-005"] {
        for _ in 0..2 {
            rbpf.update_test(sid, "pass", &GateSensor).unwrap();
        }
    }

    // FR-003 must have concentrated on VIO; neighbours must lean SAT.
    let m_3 = rbpf.marginal("FR-003").unwrap();
    assert!(
        m_3[2] > 0.60,
        "FR-003 VIO mass did not concentrate (got {m_3:?}); injection not recovered"
    );
    for sid in ["FR-001", "FR-002", "FR-004", "FR-005"] {
        let m = rbpf.marginal(sid).unwrap();
        assert!(
            m[1] > m[2],
            "{sid} leaned toward VIO despite only pass observations: {m:?}"
        );
    }
}

#[test]
fn seeded_runs_are_reproducible_end_to_end() {
    // Two RBPF instances, same seed + same event stream → identical marginals
    // to within f64 precision. This is the deterministic-rerun invariant
    // the FR-P4-004 golden-file lock relies on.
    let cluster_ids: &[&str] = &["FR-001", "FR-002", "FR-003"];
    let specs = || five_spec_cluster();
    let mut a = RBPF::new(
        specs(),
        Motion::prototype_defaults(),
        cluster_ids,
        128,
        0xDEAD_BEEF,
    );
    let mut b = RBPF::new(
        specs(),
        Motion::prototype_defaults(),
        cluster_ids,
        128,
        0xDEAD_BEEF,
    );
    for outcome in ["fail", "fail", "pass", "fail", "pass"] {
        a.update_test("FR-002", outcome, &GateSensor).unwrap();
        b.update_test("FR-002", outcome, &GateSensor).unwrap();
    }
    for sid in cluster_ids {
        let ma = a.marginal(sid).unwrap();
        let mb = b.marginal(sid).unwrap();
        for k in 0..3 {
            assert!(
                (ma[k] - mb[k]).abs() < 1e-12,
                "seeded drift on {sid}[{k}]: {} vs {}",
                ma[k],
                mb[k]
            );
        }
    }
}

#[test]
fn particle_cloud_survives_mixed_stream() {
    // Mixed fail/pass stream on a single-spec cluster. After many rounds
    // the marginal should track the evidence, but with repeated cycles of
    // fail then pass the particle cloud never collapses to one state —
    // we check that the VIO mass is between the "all fail" and "all pass"
    // asymptotes. Implicitly exercises the resample path since ESS drops
    // between like-sign runs.
    let mut rbpf = RBPF::new(
        five_spec_cluster(),
        Motion::prototype_defaults(),
        &["FR-001"],
        128,
        12345,
    );
    for _ in 0..6 {
        for _ in 0..3 {
            rbpf.update_test("FR-001", "fail", &GateSensor).unwrap();
        }
        for _ in 0..3 {
            rbpf.update_test("FR-001", "pass", &GateSensor).unwrap();
        }
    }
    let m = rbpf.marginal("FR-001").unwrap();
    // Mixed evidence ⇒ non-degenerate marginal. Neither SAT nor VIO should
    // dominate — both should hold meaningful mass.
    assert!(m[1] > 0.05 && m[2] > 0.05, "mixed stream collapsed: {m:?}");
    assert!((m.sum() - 1.0).abs() < 1e-9);
}
