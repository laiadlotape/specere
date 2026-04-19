//! `specere evaluate <kind>` — run external evaluators and emit their
//! results as evidence events in `.specere/events.jsonl`.
//!
//! FR-EQ-001: mutation testing via `cargo-mutants`. The evaluator is
//! **advisory** — a low kill rate is an evidence event, not an error.
//! The filter's sensor-calibration formula (FR-EQ-002) later turns the
//! aggregated kill rate into per-spec `α_sat` / `α_vio` adjustments.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

/// One entry in `mutants.out/outcomes.json` — cargo-mutants v25–v27+.
///
/// The actual schema is tolerant-friendly: the baseline has
/// `{"scenario": "Baseline", "summary": "Success", ...}` (scenario is a
/// plain string), and each mutant has
/// `{"scenario": {"Mutant": {...}}, "summary": "CaughtMutant|MissedMutant|...", ...}`
/// (scenario is a tagged object with a single `Mutant` key). `serde_json::Value`
/// absorbs the polymorphism; `merged_mutant()` pulls out the descriptor
/// when present. See `crates/specere/tests/issue_070_evaluate_mutations.rs`
/// for fixture snapshots of both shapes.
#[derive(Debug, Clone, Deserialize)]
pub struct MutationOutcome {
    #[serde(default)]
    pub scenario: Option<serde_json::Value>,
    #[serde(default)]
    pub summary: Option<String>,
}

/// Mutant descriptor pulled from `scenario.Mutant` when the scenario is a
/// mutant (not the baseline). All fields optional — cargo-mutants schema
/// drift across versions is handled one field at a time.
#[derive(Debug, Clone, Default)]
pub struct MutantDescriptor {
    pub file: Option<String>,
    pub function_name: Option<String>,
    pub line: Option<u64>,
    pub genre: Option<String>,
    pub description: Option<String>,
}

impl MutationOutcome {
    /// Extract the `scenario.Mutant` descriptor if this outcome is for a
    /// mutant (vs the baseline). Returns `None` for baseline / unparseable
    /// scenarios.
    pub fn merged_mutant(&self) -> Option<MutantDescriptor> {
        let scenario = self.scenario.as_ref()?;
        let mutant = scenario.get("Mutant")?.as_object()?;
        let file = mutant
            .get("file")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let function_name = mutant
            .get("function")
            .and_then(|f| f.get("function_name"))
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let line = mutant
            .get("span")
            .and_then(|s| s.get("start"))
            .and_then(|st| st.get("line"))
            .and_then(|l| l.as_u64());
        let genre = mutant
            .get("genre")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        // `name` is cargo-mutants' human-readable summary of the mutation.
        let description = mutant
            .get("name")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        Some(MutantDescriptor {
            file,
            function_name,
            line,
            genre,
            description,
        })
    }
}

/// The whole `outcomes.json` — current layout is `{"outcomes": [...]}`.
/// Older/alt layouts (bare array) are also accepted for forward-compat.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum OutcomesFile {
    Wrapped { outcomes: Vec<MutationOutcome> },
    BareList(Vec<MutationOutcome>),
}

impl OutcomesFile {
    pub fn into_outcomes(self) -> Vec<MutationOutcome> {
        match self {
            Self::Wrapped { outcomes } => outcomes,
            Self::BareList(v) => v,
        }
    }
}

/// Normalised outcome class. We collapse cargo-mutants' string labels to
/// the four-state signal the filter consumes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutcomeClass {
    /// Mutant was detected by a failing test — good. Contributes to kill rate.
    Caught,
    /// Mutant passed all tests — bad. The test suite can't discriminate.
    Missed,
    /// Mutant caused a hang beyond the per-mutant timeout.
    Timeout,
    /// Mutant failed to build — excluded from the kill-rate denominator.
    Unviable,
}

impl OutcomeClass {
    /// Map cargo-mutants' summary strings to our normalised classification.
    /// v27 emits `"CaughtMutant"`/`"MissedMutant"`/`"Success"` (baseline) /
    /// `"Timeout"` / `"Unviable"`. Earlier versions used `"Caught"`/`"Missed"`.
    /// We accept both. Baseline's `"Success"` maps to None (skip — it's the
    /// control run, not a mutant).
    pub fn from_summary(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "caughtmutant" | "caught" => Some(Self::Caught),
            "missedmutant" | "missed" | "failure" => Some(Self::Missed),
            "timeout" => Some(Self::Timeout),
            "unviable" => Some(Self::Unviable),
            // "success" = baseline run (no mutation applied). Not a mutant
            // outcome, so skip. Any other unknown summary is also skipped.
            _ => None,
        }
    }
    pub fn as_attr(self) -> &'static str {
        match self {
            Self::Caught => "caught",
            Self::Missed => "missed",
            Self::Timeout => "timeout",
            Self::Unviable => "unviable",
        }
    }
}

/// Parse `outcomes.json` from a `cargo-mutants` run.
pub fn parse_outcomes_json(raw: &str) -> Result<Vec<MutationOutcome>> {
    let file: OutcomesFile =
        serde_json::from_str(raw).context("parse cargo-mutants outcomes.json")?;
    Ok(file.into_outcomes())
}

/// Attribute each outcome to at most one spec by intersecting its source
/// path with the `[specs]` support sets in sensor-map. Directory-boundary
/// semantics match `calibrate::compute_report` (fix in v1.0.1).
pub fn attribute_to_spec<'a>(
    source_path: &str,
    specs: &'a [specere_filter::hmm::SpecDescriptor],
) -> Option<&'a str> {
    for spec in specs {
        for sup in &spec.support {
            let bare = sup.trim_end_matches('/');
            let dir = format!("{bare}/");
            if source_path == bare || source_path.starts_with(dir.as_str()) {
                return Some(&spec.id);
            }
        }
    }
    None
}

/// CLI entry — `specere evaluate mutations`. Run `cargo-mutants` (or skip
/// and parse a pre-existing outcomes.json when `from_outcomes` is set, for
/// tests), then emit one `mutation_result` event per parsed outcome.
pub fn run_mutations(
    ctx: &specere_core::Ctx,
    sensor_map: Option<PathBuf>,
    scope: Option<String>,
    in_diff: Option<String>,
    jobs: usize,
    from_outcomes: Option<PathBuf>,
) -> Result<()> {
    let sensor_map_path = sensor_map.unwrap_or_else(|| ctx.repo().join(".specere/sensor-map.toml"));
    let specs = specere_filter::load_specs(&sensor_map_path)?;

    // Scope filter — if --scope FR-001, only mutate files in that spec's support.
    let scoped_files: Option<Vec<String>> = scope.as_deref().map(|fr| {
        specs
            .iter()
            .find(|s| s.id == fr)
            .map(|s| s.support.clone())
            .unwrap_or_default()
    });
    if scope.is_some() && scoped_files.as_ref().is_some_and(|v| v.is_empty()) {
        return Err(anyhow!(
            "--scope FR-ID {:?} not found in [specs] of {}",
            scope,
            sensor_map_path.display()
        ));
    }

    let outcomes_path = match &from_outcomes {
        Some(p) => p.clone(),
        None => {
            let out_dir = ctx.repo().join(".specere/mutants.out");
            run_cargo_mutants(
                ctx.repo(),
                &out_dir,
                scoped_files.as_deref(),
                in_diff.as_deref(),
                jobs,
            )?;
            out_dir.join("outcomes.json")
        }
    };

    let raw = std::fs::read_to_string(&outcomes_path)
        .with_context(|| format!("read {}", outcomes_path.display()))?;
    let outcomes = parse_outcomes_json(&raw)?;

    let mut emitted = 0usize;
    let mut unattributed = 0usize;
    for o in &outcomes {
        let Some(summary) = o.summary.as_deref().and_then(OutcomeClass::from_summary) else {
            // Baseline scenario, or unknown summary — skip.
            continue;
        };
        let mutant = o.merged_mutant().unwrap_or_default();
        let source = mutant.file.clone();
        let spec_id = source
            .as_deref()
            .and_then(|p| attribute_to_spec(p, &specs))
            .map(String::from);

        if spec_id.is_none() && summary != OutcomeClass::Unviable {
            unattributed += 1;
        }

        let mut attrs = std::collections::BTreeMap::new();
        attrs.insert("event_kind".to_string(), "mutation_result".to_string());
        if let Some(sid) = &spec_id {
            attrs.insert("spec_id".to_string(), sid.clone());
        }
        attrs.insert("outcome".to_string(), summary.as_attr().to_string());
        if let Some(s) = source.as_deref() {
            attrs.insert("file".to_string(), s.to_string());
        }
        if let Some(fn_name) = &mutant.function_name {
            attrs.insert("function".to_string(), fn_name.clone());
        }
        if let Some(line) = mutant.line {
            attrs.insert("line".to_string(), line.to_string());
        }
        if let Some(g) = &mutant.genre {
            attrs.insert("operator".to_string(), g.clone());
        }

        let event = specere_telemetry::Event {
            ts: specere_telemetry::event_store::now_rfc3339(),
            source: "cargo-mutants".into(),
            signal: "traces".into(),
            name: mutant.description.clone(),
            feature_dir: None,
            attrs,
        };
        specere_telemetry::record(ctx, event)?;
        emitted += 1;
    }

    println!(
        "specere evaluate mutations: {emitted} mutation event(s) emitted \
         ({unattributed} unattributed to any [specs] entry)"
    );
    Ok(())
}

fn run_cargo_mutants(
    repo: &Path,
    out_dir: &Path,
    scoped_files: Option<&[String]>,
    in_diff: Option<&str>,
    jobs: usize,
) -> Result<()> {
    // cargo-mutants is a cargo subcommand, invoked as `cargo mutants`.
    let mut cmd = Command::new("cargo");
    cmd.current_dir(repo)
        .args(["mutants", "--json", "--output"])
        .arg(out_dir)
        .arg(format!("--jobs={jobs}"));
    if let Some(files) = scoped_files {
        for f in files {
            cmd.arg("--file").arg(f);
        }
    }
    if let Some(d) = in_diff {
        cmd.arg("--in-diff").arg(d);
    }
    let out = cmd.output().with_context(|| {
        "spawn `cargo mutants` — is it installed? `cargo install cargo-mutants`"
    })?;
    // cargo-mutants exits non-zero when mutants are missed (by design).
    // We don't propagate that as an error — the missed mutants are the
    // point of the evidence signal.
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        // A missing binary error is worth surfacing; a "some mutants
        // missed" exit is not. Heuristic: if stderr mentions "error: ..."
        // AND outcomes.json doesn't exist afterwards, it's a real error.
        if !out_dir.join("outcomes.json").exists() {
            return Err(anyhow!(
                "cargo-mutants failed without producing outcomes.json.\nstdout: {stdout}\nstderr: {stderr}"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use specere_filter::hmm::SpecDescriptor;

    fn spec(id: &str, support: &[&str]) -> SpecDescriptor {
        SpecDescriptor {
            id: id.into(),
            support: support.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn parse_v27_wrapped_layout_with_baseline_and_mutant() {
        // Matches what `cargo mutants --json` v27 actually emits: a baseline
        // entry with `scenario: "Baseline"` (string) and mutant entries with
        // `scenario: {"Mutant": {...}}` (tagged object).
        let raw = r#"{
            "outcomes": [
                {"scenario": "Baseline", "summary": "Success"},
                {"scenario": {"Mutant": {
                    "name": "replace add -> i32 with 0",
                    "file": "src/lib.rs",
                    "function": {"function_name": "add"},
                    "span": {"start": {"line": 1, "column": 37}},
                    "genre": "FnValue"
                }}, "summary": "CaughtMutant"},
                {"scenario": {"Mutant": {
                    "name": "replace > with >= in is_positive",
                    "file": "src/lib.rs",
                    "function": {"function_name": "is_positive"},
                    "span": {"start": {"line": 2, "column": 40}},
                    "genre": "BinaryOp"
                }}, "summary": "MissedMutant"}
            ]
        }"#;
        let out = parse_outcomes_json(raw).unwrap();
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].summary.as_deref(), Some("Success"));
        assert!(out[0].merged_mutant().is_none(), "baseline has no Mutant");

        let m1 = out[1].merged_mutant().unwrap();
        assert_eq!(m1.file.as_deref(), Some("src/lib.rs"));
        assert_eq!(m1.function_name.as_deref(), Some("add"));
        assert_eq!(m1.line, Some(1));
        assert_eq!(m1.genre.as_deref(), Some("FnValue"));

        let m2 = out[2].merged_mutant().unwrap();
        assert_eq!(m2.function_name.as_deref(), Some("is_positive"));
        assert_eq!(m2.line, Some(2));
    }

    #[test]
    fn parse_handles_bare_list_layout() {
        let raw = r#"[{"scenario": {"Mutant": {"file":"src/b.rs","span":{"start":{"line":7}}}}, "summary":"MissedMutant"}]"#;
        let out = parse_outcomes_json(raw).unwrap();
        assert_eq!(out[0].summary.as_deref(), Some("MissedMutant"));
        let m = out[0].merged_mutant().unwrap();
        assert_eq!(m.file.as_deref(), Some("src/b.rs"));
        assert_eq!(m.line, Some(7));
    }

    #[test]
    fn outcome_class_maps_v27_and_case_insensitive() {
        assert_eq!(
            OutcomeClass::from_summary("CaughtMutant"),
            Some(OutcomeClass::Caught)
        );
        assert_eq!(
            OutcomeClass::from_summary("MissedMutant"),
            Some(OutcomeClass::Missed)
        );
        assert_eq!(
            OutcomeClass::from_summary("Caught"),
            Some(OutcomeClass::Caught)
        );
        assert_eq!(
            OutcomeClass::from_summary("missed"),
            Some(OutcomeClass::Missed)
        );
        assert_eq!(
            OutcomeClass::from_summary("TIMEOUT"),
            Some(OutcomeClass::Timeout)
        );
        assert_eq!(
            OutcomeClass::from_summary("Unviable"),
            Some(OutcomeClass::Unviable)
        );
        // Baseline's "Success" is intentionally None — baseline is not a mutant.
        assert_eq!(OutcomeClass::from_summary("Success"), None);
        assert_eq!(OutcomeClass::from_summary("unknown"), None);
    }

    #[test]
    fn attribute_directory_match_is_boundary_safe() {
        // Regression anchor from v1.0.1 — `src/auth` must NOT match
        // `src/auth_helpers/x.rs`.
        let specs = vec![
            spec("auth", &["src/auth/"]),
            spec("helpers", &["src/auth_helpers/"]),
        ];
        assert_eq!(attribute_to_spec("src/auth/login.rs", &specs), Some("auth"));
        assert_eq!(
            attribute_to_spec("src/auth_helpers/h.rs", &specs),
            Some("helpers")
        );
        assert_eq!(attribute_to_spec("src/unrelated.rs", &specs), None);
    }

    #[test]
    fn attribute_exact_file_match_works() {
        let specs = vec![spec("main", &["src/main.rs"])];
        assert_eq!(attribute_to_spec("src/main.rs", &specs), Some("main"));
        // Sibling file with matching prefix must NOT match.
        assert_eq!(attribute_to_spec("src/mainframe.rs", &specs), None);
    }

    #[test]
    fn parse_tolerates_missing_scenario() {
        let raw = r#"[{"summary":"CaughtMutant"}]"#;
        let out = parse_outcomes_json(raw).unwrap();
        assert_eq!(out.len(), 1);
        // No scenario/Mutant — merged_mutant returns None; consumers treat
        // as unattributed.
        assert!(out[0].merged_mutant().is_none());
    }
}
