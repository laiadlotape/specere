//! Drive a filter from an event stream. Issue #43 / FR-P4-001 + FR-P4-005.
//!
//! Event-attr contract the hook authors populate:
//!
//! - `attrs.spec_id` = `"FR-NNN"` — which spec this event concerns.
//! - `attrs.event_kind` = `"test_outcome"` | `"files_touched"`.
//! - For `test_outcome`: `attrs.outcome` = `"pass"` | `"fail"`.
//! - For `files_touched`: `attrs.paths` = comma-separated absolute-or-repo-
//!   relative file paths.
//!
//! Events without the required attrs are silently skipped — the filter
//! shouldn't crash on malformed hook data, and skipped events show up in
//! the `EventOutcome::skipped` count for the CLI to report.
//!
//! `DefaultTestSensor` provides a prototype-ported emission model so the
//! CLI doesn't force every caller to wire their own sensor. Override by
//! constructing a [`crate::TestSensor`] impl directly and using the filter
//! methods.

use ndarray::{array, Array1};

use crate::state::TestSensor;

/// Prototype-default emission model. Treat the `pass` row as a peaked SAT
/// observation and `fail` as a peaked VIO one. Calibrated to match the
/// `prototype/mini_specs/sensors.py` TestSensor.strong-signal case.
pub struct DefaultTestSensor;

impl TestSensor for DefaultTestSensor {
    fn log_likelihood(&self, _spec_id: &str, outcome: &str) -> Array1<f64> {
        match outcome {
            "pass" => array![0.20_f64.ln(), 0.70_f64.ln(), 0.10_f64.ln()],
            "fail" => array![0.30_f64.ln(), 0.05_f64.ln(), 0.65_f64.ln()],
            // Unknown outcomes: return a flat (uninformative) row so the
            // posterior doesn't drift on bad data.
            _ => {
                let u = (1.0_f64 / 3.0).ln();
                array![u, u, u]
            }
        }
    }
}

/// Summary of how many events drove what update.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DriveStats {
    pub processed: usize,
    pub skipped: usize,
    pub predicts: usize,
    pub test_updates: usize,
    /// Last-event timestamp observed — suitable for storing as the cursor.
    pub latest_ts: Option<String>,
}

/// Parse a `files_touched` event's `attrs.paths` into a Vec of path strs.
/// Empty-string → empty vec. Extra whitespace around commas is trimmed.
pub fn parse_paths(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_sensor_pass_peaks_sat() {
        let s = DefaultTestSensor;
        let v = s.log_likelihood("FR-001", "pass");
        // SAT index (1) must be the largest log-likelihood.
        let mx = v.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        assert!((v[1] - mx).abs() < 1e-12);
    }

    #[test]
    fn default_sensor_fail_peaks_vio() {
        let s = DefaultTestSensor;
        let v = s.log_likelihood("FR-001", "fail");
        let mx = v.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        assert!((v[2] - mx).abs() < 1e-12);
    }

    #[test]
    fn default_sensor_unknown_outcome_is_flat() {
        let s = DefaultTestSensor;
        let v = s.log_likelihood("FR-001", "bogus");
        let expected = (1.0_f64 / 3.0).ln();
        for k in 0..3 {
            assert!((v[k] - expected).abs() < 1e-12);
        }
    }

    #[test]
    fn parse_paths_splits_and_trims() {
        assert_eq!(
            parse_paths("a.rs, b.rs , c.rs"),
            vec!["a.rs", "b.rs", "c.rs"]
        );
        assert_eq!(parse_paths(""), Vec::<String>::new());
        assert_eq!(parse_paths("   "), Vec::<String>::new());
    }
}
