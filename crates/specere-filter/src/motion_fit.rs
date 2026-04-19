//! `specere calibrate motion-from-evidence` — fit per-spec transition
//! matrices from event history (FR-EQ-004).
//!
//! Algorithm:
//! 1. For each spec, collect `mutation_result` + `test_outcome` events
//!    chronologically.
//! 2. Map each event to a `(write_class, observed_state)` pair:
//!    - `mutation_result outcome=caught` → (Good, SAT)
//!    - `mutation_result outcome=missed|timeout` → (Bad, VIO)
//!    - `test_outcome outcome=pass` → (Good, SAT)
//!    - `test_outcome outcome=fail` → (Bad, VIO)
//!    - otherwise → (Leak, UNK)
//! 3. For every consecutive pair `(e_prev, e_curr)`, increment
//!    `count[class(e_curr)][state(e_prev)][state(e_curr)]`.
//! 4. Laplace-smooth per row: `t[c][i][j] = (count[i][j] + 1) / (Σ_k count[i][k] + 3)`.
//! 5. If a class has zero observations, fall back to the prototype
//!    default for that class (we don't fabricate rows from noise).
//! 6. Require ≥ `min_events` total events per spec (default 20) to emit a
//!    fit; otherwise report `InsufficientHistory`.

use std::collections::BTreeMap;

use ndarray::{array, Array2};

use crate::motion::Motion;
use crate::state::Status;

/// 3×3 counts for each write class. Rows = prev status; cols = curr status.
#[derive(Debug, Clone, Default)]
struct ClassCounts {
    m: [[u32; 3]; 3],
    total: u32,
}

impl ClassCounts {
    fn bump(&mut self, from: Status, to: Status) {
        self.m[from.index()][to.index()] += 1;
        self.total += 1;
    }

    /// Laplace-smoothed row-stochastic transition matrix. Returns `None` if
    /// `total == 0` — the caller should fall back to the prototype row.
    fn to_matrix(&self) -> Option<Array2<f64>> {
        if self.total == 0 {
            return None;
        }
        let mut out = Array2::<f64>::zeros((3, 3));
        for i in 0..3 {
            let row_sum: u32 = self.m[i].iter().sum();
            let denom = (row_sum + 3) as f64;
            for j in 0..3 {
                out[[i, j]] = (self.m[i][j] as f64 + 1.0) / denom;
            }
        }
        Some(out)
    }
}

/// Result of fitting one spec. Either a full `Motion` (fit succeeded) or an
/// explicit `InsufficientHistory` reason the CLI can surface. `Motion` is
/// boxed because it owns three 3×3 `Array2<f64>` matrices — large compared
/// to the insufficient-history variant.
#[derive(Debug, Clone)]
pub enum SpecFit {
    Fitted {
        motion: Box<Motion>,
        quality: f64,
        n_events: u32,
        kill_rate: Option<f64>,
        classes_observed: u32,
    },
    InsufficientHistory {
        n_events: u32,
        min_required: u32,
    },
}

/// Per-spec fit report.
#[derive(Debug, Clone)]
pub struct FitReport {
    pub per_spec: BTreeMap<String, SpecFit>,
    pub min_events: u32,
}

impl FitReport {
    /// Ready-to-paste TOML snippet. For fitted specs emits
    /// `[motion."<id>"]` + `[calibration."<id>"]`; for insufficient specs
    /// emits a `# insufficient history: N events (need M)` comment.
    pub fn to_toml_snippet(&self) -> String {
        let mut s = String::new();
        s.push_str("# Per-spec motion fit — auto-proposed by\n");
        s.push_str("# `specere calibrate motion-from-evidence`.\n");
        s.push_str(
            "# Paste the [motion.*] + [calibration.*] tables into .specere/sensor-map.toml.\n",
        );
        s.push_str(&format!(
            "# {} spec(s) analysed; minimum {} events required to fit.\n\n",
            self.per_spec.len(),
            self.min_events
        ));
        for (sid, fit) in &self.per_spec {
            match fit {
                SpecFit::Fitted {
                    motion,
                    quality,
                    n_events,
                    kill_rate,
                    classes_observed,
                } => {
                    s.push_str(&format!(
                        "# {sid}: fit from {n_events} event(s), {classes_observed}/3 classes observed"
                    ));
                    if let Some(k) = kill_rate {
                        s.push_str(&format!(", kill_rate={k:.3}"));
                    }
                    s.push('\n');
                    s.push_str(&format!("[motion.\"{sid}\"]\n"));
                    s.push_str(&format!("t_good = {}\n", fmt_matrix(&motion.t_good)));
                    s.push_str(&format!("t_bad  = {}\n", fmt_matrix(&motion.t_bad)));
                    s.push_str(&format!("t_leak = {}\n", fmt_matrix(&motion.t_leak)));
                    s.push_str(&format!("assumed_good = {:.3}\n", motion.assumed_good));
                    s.push_str(&format!("[calibration.\"{sid}\"]\n"));
                    s.push_str(&format!("quality = {quality:.3}\n\n"));
                }
                SpecFit::InsufficientHistory {
                    n_events,
                    min_required,
                } => {
                    s.push_str(&format!(
                        "# {sid}: insufficient history — {n_events} event(s), need {min_required}\n\n"
                    ));
                }
            }
        }
        s
    }
}

fn fmt_matrix(m: &Array2<f64>) -> String {
    let mut s = String::from("[");
    for i in 0..m.nrows() {
        if i > 0 {
            s.push_str(", ");
        }
        s.push('[');
        for j in 0..m.ncols() {
            if j > 0 {
                s.push_str(", ");
            }
            s.push_str(&format!("{:.4}", m[[i, j]]));
        }
        s.push(']');
    }
    s.push(']');
    s
}

/// Per-spec observation extracted from one event: which class drove the
/// update, and which status the event evidences.
#[derive(Debug, Clone, Copy)]
struct Obs {
    class: WriteClass,
    state: Status,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WriteClass {
    Good = 0,
    Bad = 1,
    // Leak class is inferred from gaps between evidence events; fit() doesn't
    // synthesise them today (we'd need files_touched/mutation_result gap
    // analysis). `t_leak` currently falls back to the prototype default.
}

/// Classify a single event into `(class, state)`. Returns `None` for events
/// that carry no evidence (e.g. `files_touched` alone, or unknown kinds).
fn classify_event(kind: &str, outcome: &str) -> Option<Obs> {
    match (kind, outcome) {
        ("mutation_result", "caught") => Some(Obs {
            class: WriteClass::Good,
            state: Status::Sat,
        }),
        ("mutation_result", "missed" | "timeout") => Some(Obs {
            class: WriteClass::Bad,
            state: Status::Vio,
        }),
        ("test_outcome", "pass") => Some(Obs {
            class: WriteClass::Good,
            state: Status::Sat,
        }),
        ("test_outcome", "fail") => Some(Obs {
            class: WriteClass::Bad,
            state: Status::Vio,
        }),
        // `mutation_result outcome=unviable` carries no state signal; skip.
        // Same for any other event kind or unrecognised outcome.
        _ => None,
    }
}

/// One (spec_id, kind, outcome) triple pre-extracted from an event.
#[derive(Debug, Clone)]
pub struct FitInput {
    pub spec_id: String,
    pub kind: String,
    pub outcome: String,
}

/// Fit per-spec motions from a chronologically-ordered event stream.
///
/// `spec_ids` constrains the fit to declared specs; events whose `spec_id`
/// doesn't appear in the list are silently ignored. `min_events` is the
/// threshold below which a spec emits `InsufficientHistory` instead of a
/// fit (per FR-EQ-004: default 20).
pub fn fit(spec_ids: &[String], events: &[FitInput], min_events: u32) -> FitReport {
    // Per-spec event buckets, preserving input order.
    let mut per_spec: BTreeMap<String, Vec<Obs>> = BTreeMap::new();
    for sid in spec_ids {
        per_spec.insert(sid.clone(), Vec::new());
    }
    for e in events {
        if let Some(bucket) = per_spec.get_mut(&e.spec_id) {
            if let Some(obs) = classify_event(&e.kind, &e.outcome) {
                bucket.push(obs);
            }
        }
    }

    let mut out = BTreeMap::new();
    for (sid, obs_list) in per_spec {
        let n_events = obs_list.len() as u32;
        if n_events < min_events {
            out.insert(
                sid,
                SpecFit::InsufficientHistory {
                    n_events,
                    min_required: min_events,
                },
            );
            continue;
        }

        // Build per-class count matrices from consecutive pairs. Only Good
        // and Bad are fitted; t_leak always falls back to the prototype.
        let mut good = ClassCounts::default();
        let mut bad = ClassCounts::default();
        for win in obs_list.windows(2) {
            let (prev, curr) = (win[0], win[1]);
            let bucket = match curr.class {
                WriteClass::Good => &mut good,
                WriteClass::Bad => &mut bad,
            };
            bucket.bump(prev.state, curr.state);
        }

        let proto = Motion::prototype_defaults();
        let mut classes_observed = 0;
        let t_good = match good.to_matrix() {
            Some(m) => {
                classes_observed += 1;
                m
            }
            None => proto.t_good.clone(),
        };
        let t_bad = match bad.to_matrix() {
            Some(m) => {
                classes_observed += 1;
                m
            }
            None => proto.t_bad.clone(),
        };
        let t_leak = proto.t_leak.clone();

        // Kill rate from the mutation subset (same formula as FR-EQ-002).
        let mut caught = 0u32;
        let mut missed = 0u32;
        for o in &obs_list {
            match (o.class, o.state) {
                (WriteClass::Good, Status::Sat) => caught += 1,
                (WriteClass::Bad, Status::Vio) => missed += 1,
                _ => {}
            }
        }
        let kill_rate = if caught + missed > 0 {
            Some(caught as f64 / (caught + missed) as f64)
        } else {
            None
        };
        // Quality follows the FR-EQ-002 formula; no smell penalty here —
        // smells are layered on at filter-run time (FR-EQ-005).
        let quality = kill_rate.unwrap_or(1.0).clamp(0.3, 1.0);

        out.insert(
            sid,
            SpecFit::Fitted {
                motion: Box::new(Motion {
                    t_good,
                    t_bad,
                    t_leak,
                    assumed_good: proto.assumed_good,
                }),
                quality,
                n_events,
                kill_rate,
                classes_observed,
            },
        );
    }

    FitReport {
        per_spec: out,
        min_events,
    }
}

#[allow(dead_code)]
fn _status_matrix_example() {
    // Keep the compiler from stripping `array!` used only in tests.
    let _: Array2<f64> = array![[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(spec: &str, kind: &str, outcome: &str) -> FitInput {
        FitInput {
            spec_id: spec.into(),
            kind: kind.into(),
            outcome: outcome.into(),
        }
    }

    #[test]
    fn insufficient_history_under_threshold() {
        let specs = vec!["FR-001".to_string()];
        let events: Vec<FitInput> = (0..10)
            .map(|_| ev("FR-001", "test_outcome", "pass"))
            .collect();
        let report = fit(&specs, &events, 20);
        match &report.per_spec["FR-001"] {
            SpecFit::InsufficientHistory {
                n_events,
                min_required,
            } => {
                assert_eq!(*n_events, 10);
                assert_eq!(*min_required, 20);
            }
            _ => panic!("expected InsufficientHistory"),
        }
    }

    #[test]
    fn fit_emitted_with_enough_history() {
        let specs = vec!["FR-001".to_string()];
        // 20 pass events → all (Good, SAT). Consecutive pairs:
        // (SAT → SAT) × 19 in the good-class bucket.
        let events: Vec<FitInput> = (0..20)
            .map(|_| ev("FR-001", "test_outcome", "pass"))
            .collect();
        let report = fit(&specs, &events, 20);
        match &report.per_spec["FR-001"] {
            SpecFit::Fitted {
                motion,
                quality,
                classes_observed,
                kill_rate,
                ..
            } => {
                // Kill-rate = 20/20 = 1.0 → quality 1.0.
                assert_eq!(*kill_rate, Some(1.0));
                assert!((*quality - 1.0).abs() < 1e-12);
                assert_eq!(*classes_observed, 1, "only Good class observed");
                // t_good SAT row (index 1) must be peaked at SAT (j=1) since
                // 19/19 Laplace-smoothed: (19+1)/(19+3) = 20/22 ≈ 0.909.
                let sat_row = motion.t_good.row(1);
                assert!(
                    (sat_row[1] - 20.0 / 22.0).abs() < 1e-9,
                    "SAT→SAT should dominate: {}",
                    sat_row[1]
                );
                // Rows sum to 1 (Laplace-smoothed is row-stochastic).
                for i in 0..3 {
                    assert!(
                        (motion.t_good.row(i).sum() - 1.0).abs() < 1e-9,
                        "t_good row {} not stochastic",
                        i
                    );
                }
                // t_bad / t_leak must equal prototype (no Bad or Leak obs).
                let proto = Motion::prototype_defaults();
                assert!(matrix_eq(&motion.t_bad, &proto.t_bad));
                assert!(matrix_eq(&motion.t_leak, &proto.t_leak));
            }
            _ => panic!("expected Fitted"),
        }
    }

    #[test]
    fn mixed_outcomes_produce_both_classes() {
        let specs = vec!["FR-002".to_string()];
        // 25 alternating pass/fail events → both Good and Bad classes observed.
        let events: Vec<FitInput> = (0..25)
            .map(|i| {
                let o = if i % 2 == 0 { "pass" } else { "fail" };
                ev("FR-002", "test_outcome", o)
            })
            .collect();
        let report = fit(&specs, &events, 20);
        match &report.per_spec["FR-002"] {
            SpecFit::Fitted {
                classes_observed,
                kill_rate,
                ..
            } => {
                assert_eq!(*classes_observed, 2);
                // 13 pass, 12 fail → kill_rate = 13/25 = 0.52
                let k = kill_rate.expect("kill_rate set when pass+fail > 0");
                assert!((k - 13.0 / 25.0).abs() < 1e-9, "kill_rate mismatch: {}", k);
            }
            _ => panic!("expected Fitted"),
        }
    }

    #[test]
    fn unknown_specs_ignored() {
        let specs = vec!["FR-001".to_string()];
        let events: Vec<FitInput> = (0..20)
            .map(|_| ev("FR-999", "test_outcome", "pass"))
            .collect();
        let report = fit(&specs, &events, 20);
        match &report.per_spec["FR-001"] {
            SpecFit::InsufficientHistory { n_events, .. } => assert_eq!(*n_events, 0),
            _ => panic!("expected InsufficientHistory (no FR-001 events)"),
        }
    }

    #[test]
    fn rows_stochastic_under_laplace() {
        // Synthetic: arbitrary count matrix; verify Laplace smoothing keeps
        // rows row-stochastic even when some rows have zero observations.
        let mut c = ClassCounts::default();
        c.bump(Status::Unk, Status::Sat);
        c.bump(Status::Sat, Status::Sat);
        c.bump(Status::Sat, Status::Vio);
        let m = c.to_matrix().unwrap();
        for i in 0..3 {
            let s = m.row(i).sum();
            assert!((s - 1.0).abs() < 1e-9, "row {i} not stochastic: {s}");
        }
    }

    #[test]
    fn mutation_events_drive_fit() {
        let specs = vec!["FR-003".to_string()];
        let events: Vec<FitInput> = (0..22)
            .map(|i| {
                let out = if i < 18 { "caught" } else { "missed" };
                ev("FR-003", "mutation_result", out)
            })
            .collect();
        let report = fit(&specs, &events, 20);
        match &report.per_spec["FR-003"] {
            SpecFit::Fitted {
                kill_rate,
                n_events,
                ..
            } => {
                assert_eq!(*n_events, 22);
                let k = kill_rate.expect("kill_rate set");
                assert!((k - 18.0 / 22.0).abs() < 1e-9, "kill_rate: {}", k);
            }
            _ => panic!("expected Fitted"),
        }
    }

    #[test]
    fn snippet_contains_motion_and_calibration_tables() {
        let specs = vec!["FR-alpha".to_string()];
        let events: Vec<FitInput> = (0..20)
            .map(|_| ev("FR-alpha", "test_outcome", "pass"))
            .collect();
        let report = fit(&specs, &events, 20);
        let snip = report.to_toml_snippet();
        assert!(snip.contains("[motion.\"FR-alpha\"]"), "snippet: {snip}");
        assert!(snip.contains("t_good ="));
        assert!(snip.contains("t_bad"));
        assert!(snip.contains("t_leak"));
        assert!(snip.contains("[calibration.\"FR-alpha\"]"));
        assert!(snip.contains("quality ="));
    }

    fn matrix_eq(a: &Array2<f64>, b: &Array2<f64>) -> bool {
        if a.shape() != b.shape() {
            return false;
        }
        a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() < 1e-12)
    }
}
