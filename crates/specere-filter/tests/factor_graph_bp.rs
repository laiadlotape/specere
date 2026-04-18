//! Integration tests for FactorGraphBP + the coupling loader.
//! Anchors FR-P4-006 (cycle rejection) and the qualitative coupling behaviour
//! the prototype validated on Gate-A.

use ndarray::{array, Array1};
use specere_filter::hmm::SpecDescriptor;
use specere_filter::{CouplingGraph, FactorGraphBP, Motion, PerSpecHMM, TestSensor};

struct FailSensor;
impl TestSensor for FailSensor {
    fn log_likelihood(&self, _spec_id: &str, _outcome: &str) -> Array1<f64> {
        array![0.30_f64.ln(), 0.05_f64.ln(), 0.85_f64.ln()]
    }
}

fn three_spec_fixture() -> Vec<SpecDescriptor> {
    vec![
        SpecDescriptor {
            id: "FR-001".into(),
            support: vec!["src/a.rs".into()],
        },
        SpecDescriptor {
            id: "FR-002".into(),
            support: vec!["src/b.rs".into()],
        },
        SpecDescriptor {
            id: "FR-003".into(),
            support: vec!["src/c.rs".into()],
        },
    ]
}

#[test]
fn chain_of_three_propagates_violated_belief_downstream() {
    // Tree: FR-001 → FR-002 → FR-003. Failing tests on FR-001 should lift
    // VIO mass on all three. Per FR-P4-006 the graph is a DAG so the loader
    // accepts; BP should converge (trees are exact).
    let coupling = CouplingGraph::from_toml_str(
        r#"
        [coupling]
        edges = [
          ["FR-001", "FR-002"],
          ["FR-002", "FR-003"],
        ]
        "#,
    )
    .unwrap();
    assert_eq!(coupling.edges.len(), 2);

    let mut bp = FactorGraphBP::new(
        three_spec_fixture(),
        Motion::prototype_defaults(),
        &coupling,
    )
    .with_n_iter(3);
    assert_eq!(bp.num_edges(), 2);

    for _ in 0..5 {
        bp.update_test("FR-001", "fail", &FailSensor).unwrap();
    }

    let vio_001 = bp.marginal("FR-001").unwrap()[2];
    let vio_002 = bp.marginal("FR-002").unwrap()[2];
    let vio_003 = bp.marginal("FR-003").unwrap()[2];

    // Strict downstream ordering: FR-001 (directly observed) > FR-002 (one
    // hop) > FR-003 (two hops) > 1/3 (uniform prior floor).
    assert!(vio_001 > vio_002, "{vio_001} ≤ {vio_002}");
    assert!(vio_002 > vio_003, "{vio_002} ≤ {vio_003}");
    assert!(
        vio_003 > 1.0 / 3.0,
        "two-hop downstream did not rise above uniform"
    );
}

#[test]
fn cycle_is_rejected_with_chain_in_error() {
    let err = CouplingGraph::from_toml_str(
        r#"
        [coupling]
        edges = [
          ["FR-001", "FR-002"],
          ["FR-002", "FR-003"],
          ["FR-003", "FR-001"],
        ]
        "#,
    )
    .unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("cycle"), "missing 'cycle' in {msg}");
    assert!(msg.contains("FR-001"), "chain missing FR-001: {msg}");
    assert!(
        msg.contains("RBPF"),
        "message should mention RBPF escape: {msg}"
    );
}

#[test]
fn unknown_edges_are_silently_dropped() {
    // Loader accepts the TOML (both edges are in a DAG), but only one
    // references a known spec. The filter should still construct cleanly
    // and report exactly one kept edge.
    let coupling = CouplingGraph::from_toml_str(
        r#"
        [coupling]
        edges = [
          ["FR-001", "FR-002"],
          ["FR-999", "FR-888"],
        ]
        "#,
    )
    .unwrap();
    let bp = FactorGraphBP::new(
        three_spec_fixture(),
        Motion::prototype_defaults(),
        &coupling,
    );
    assert_eq!(bp.num_edges(), 1, "unknown edges must be dropped");
}

#[test]
fn empty_coupling_matches_baseline() {
    let coupling = CouplingGraph::default();
    let mut bp = FactorGraphBP::new(
        three_spec_fixture(),
        Motion::prototype_defaults(),
        &coupling,
    );
    let mut hmm = PerSpecHMM::new(three_spec_fixture(), Motion::prototype_defaults());
    for _ in 0..3 {
        bp.update_test("FR-001", "fail", &FailSensor).unwrap();
        hmm.update_test("FR-001", "fail", &FailSensor).unwrap();
    }
    for id in ["FR-001", "FR-002", "FR-003"] {
        let a = bp.marginal(id).unwrap();
        let b = hmm.marginal(id).unwrap();
        for k in 0..3 {
            assert!(
                (a[k] - b[k]).abs() < 1e-12,
                "BP with no edges diverged from HMM on {id}[{k}]"
            );
        }
    }
}
