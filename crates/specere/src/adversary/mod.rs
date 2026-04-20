//! FR-EQ-020..024 — LLM adversary agent.
//!
//! `specere adversary run --spec FR-NNN` iteratively asks an LLM for a
//! shell script that would falsify the spec, runs it in a sandbox, and
//! on a failing + reproducible outcome after ≥ 3 iterations emits a
//! `counterexample_found` evidence event. Budget is enforced per-month
//! at $20 USD (configurable). Minimization via delta-debug.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

pub mod minimize;
pub mod provider;
pub mod sandbox;
pub mod spend;

pub use provider::Provider;

/// Parameters for one `adversary run` invocation.
pub struct RunParams {
    pub spec_id: String,
    pub provider: String,
    pub max_iterations: u32,
    pub sandbox_mode: sandbox::Mode,
    pub fixture_dir: Option<PathBuf>,
    pub cap_usd: Option<f64>,
    /// Minimum iterations that must execute before we are allowed to
    /// emit a `counterexample_found` event (FR-EQ-022).
    pub min_iterations_for_found: u32,
    /// Time-box for delta-debug (FR-EQ-023). 30 s default.
    pub minimize_timeout: Duration,
}

impl RunParams {
    pub fn new(spec_id: String) -> Self {
        RunParams {
            spec_id,
            provider: "mock".into(),
            max_iterations: 5,
            sandbox_mode: sandbox::Mode::Rlimit,
            fixture_dir: None,
            cap_usd: None,
            min_iterations_for_found: 3,
            minimize_timeout: Duration::from_secs(30),
        }
    }
}

pub struct Summary {
    pub spec_id: String,
    pub iterations_run: u32,
    pub counterexample_found: bool,
    pub counterexample_candidate: bool,
    pub budget_exceeded: bool,
    pub spent_this_run_usd: f64,
}

pub fn run(ctx: &specere_core::Ctx, params: RunParams) -> Result<Summary> {
    let repo = ctx.repo();
    let (spec_text, support, tests) = load_spec_bundle(repo, &params.spec_id)?;
    let provider_impl = provider::build(&params.provider, repo, params.fixture_dir.clone())?;

    let ledger_path = spend::ledger_path(repo);
    let mut ledger = spend::load_or_init(&ledger_path, params.cap_usd)?;

    let scratch = sandbox::default_scratch(repo);
    let mut summary = Summary {
        spec_id: params.spec_id.clone(),
        iterations_run: 0,
        counterexample_found: false,
        counterexample_candidate: false,
        budget_exceeded: false,
        spent_this_run_usd: 0.0,
    };

    let mut first_failure: Option<(u32, String, String)> = None;

    for iter in 1..=params.max_iterations {
        // Budget check before the expensive LLM call.
        if ledger.spent_usd >= ledger.cap_usd - 1e-9 {
            summary.budget_exceeded = true;
            emit_event(
                ctx,
                &params.spec_id,
                "adversary_budget_exceeded",
                &[
                    ("iteration", iter.to_string()),
                    ("spent_usd", format!("{:.4}", ledger.spent_usd)),
                    ("cap_usd", format!("{:.4}", ledger.cap_usd)),
                ],
            )?;
            break;
        }

        let sug = match provider_impl.ask(&spec_text, &support, &tests, iter) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("provider ask failed on iter {iter}: {e:#}");
                continue;
            }
        };
        summary.spent_this_run_usd += sug.cost_usd;

        // Charge the ledger post-call so a failure doesn't bill the user.
        if sug.cost_usd > 0.0 {
            if let Err(e) = spend::charge(&ledger_path, &mut ledger, sug.cost_usd) {
                if let Some(spend::SpendError::CapExceeded { .. }) = e.downcast_ref() {
                    summary.budget_exceeded = true;
                    emit_event(
                        ctx,
                        &params.spec_id,
                        "adversary_budget_exceeded",
                        &[
                            ("iteration", iter.to_string()),
                            ("spent_usd", format!("{:.4}", ledger.spent_usd)),
                            ("cap_usd", format!("{:.4}", ledger.cap_usd)),
                        ],
                    )?;
                    break;
                }
                return Err(e);
            }
        }

        summary.iterations_run = iter;

        let outcome = sandbox::run(
            params.sandbox_mode,
            repo,
            &scratch,
            &sug.script,
            Duration::from_secs(30),
        )?;

        emit_event(
            ctx,
            &params.spec_id,
            "adversary_iteration_complete",
            &[
                ("iteration", iter.to_string()),
                ("provider", provider_impl.name().to_string()),
                ("rationale", sug.rationale.clone()),
                ("status", outcome.status.to_string()),
                (
                    "timed_out",
                    if outcome.timed_out { "true" } else { "false" }.to_string(),
                ),
                ("cost_usd", format!("{:.4}", sug.cost_usd)),
            ],
        )?;

        if outcome.failed() && first_failure.is_none() {
            first_failure = Some((iter, sug.script.clone(), sug.rationale.clone()));
        }

        // FR-EQ-022: require ≥ min_iterations_for_found iterations before
        // emitting counterexample_found. We still complete the loop to
        // probe for additional angles, but break early once we've hit
        // the minimum and have a failure in hand.
        if iter >= params.min_iterations_for_found && first_failure.is_some() {
            break;
        }
    }

    if let Some((found_iter, script, rationale)) = first_failure {
        // FR-EQ-022: the loop must have RUN ≥ min iterations BEFORE
        // producing the finding — i.e. the finding's iteration index
        // must be ≥ min_iterations_for_found. A fail on iter 1 is a
        // "one-shot" (suspect per Liu '24) and downgrades to candidate.
        let eligible_for_found = found_iter >= params.min_iterations_for_found;
        if eligible_for_found {
            // Determinism check: re-run the same script; if it still fails,
            // emit counterexample_found after minimization.
            let second = sandbox::run(
                params.sandbox_mode,
                repo,
                &scratch,
                &script,
                Duration::from_secs(30),
            )?;
            if second.failed() {
                let deadline = Instant::now() + params.minimize_timeout;
                let mode = params.sandbox_mode;
                let repo_owned = repo.to_path_buf();
                let scratch_owned = scratch.clone();
                let min = minimize::minimize(
                    &script,
                    |candidate| match sandbox::run(
                        mode,
                        &repo_owned,
                        &scratch_owned,
                        candidate,
                        Duration::from_secs(15),
                    ) {
                        Ok(o) => o.failed(),
                        Err(_) => false,
                    },
                    deadline,
                );
                emit_event(
                    ctx,
                    &params.spec_id,
                    "counterexample_found",
                    &[
                        ("iteration_found", found_iter.to_string()),
                        ("iterations_total", summary.iterations_run.to_string()),
                        ("provider", provider_impl.name().to_string()),
                        ("rationale", rationale),
                        ("original_len", script.len().to_string()),
                        ("minimized_len", min.len().to_string()),
                        ("minimized", min),
                    ],
                )?;
                summary.counterexample_found = true;
            } else {
                // Non-deterministic — downgrade to candidate.
                emit_event(
                    ctx,
                    &params.spec_id,
                    "counterexample_candidate",
                    &[
                        ("iteration_found", found_iter.to_string()),
                        ("reason", "non_deterministic".into()),
                        ("provider", provider_impl.name().to_string()),
                        ("rationale", rationale),
                        ("script", script),
                    ],
                )?;
                summary.counterexample_candidate = true;
            }
        } else {
            // FR-EQ-022: one-shot finding — route to review queue as
            // candidate, not posterior-update.
            emit_event(
                ctx,
                &params.spec_id,
                "counterexample_candidate",
                &[
                    ("iteration_found", found_iter.to_string()),
                    ("reason", "below_min_iterations".into()),
                    (
                        "min_iterations_required",
                        params.min_iterations_for_found.to_string(),
                    ),
                    ("provider", provider_impl.name().to_string()),
                    ("rationale", rationale),
                    ("script", script),
                ],
            )?;
            summary.counterexample_candidate = true;
        }
    }

    Ok(summary)
}

fn emit_event(
    ctx: &specere_core::Ctx,
    spec_id: &str,
    event_kind: &str,
    extra_attrs: &[(&str, String)],
) -> Result<()> {
    let mut attrs = std::collections::BTreeMap::new();
    attrs.insert("event_kind".to_string(), event_kind.to_string());
    attrs.insert("spec_id".to_string(), spec_id.to_string());
    for (k, v) in extra_attrs {
        attrs.insert((*k).to_string(), v.clone());
    }
    let event = specere_telemetry::Event {
        ts: specere_telemetry::event_store::now_rfc3339(),
        source: "adversary".into(),
        signal: "traces".into(),
        name: Some(event_kind.to_string()),
        feature_dir: None,
        attrs,
    };
    specere_telemetry::record(ctx, event)?;
    Ok(())
}

/// Load spec.md text + support + tests for the given FR id. Support &
/// tests are looked up from `.specere/sensor-map.toml`. Spec text is
/// best-effort — if the FR is not in a `specs/*/spec.md`, we synthesize
/// a short placeholder so the provider has *some* context.
fn load_spec_bundle(repo: &Path, spec_id: &str) -> Result<(String, Vec<String>, Vec<String>)> {
    let sensor_map_path = repo.join(".specere/sensor-map.toml");
    let specs = if sensor_map_path.exists() {
        specere_filter::load_specs(&sensor_map_path).unwrap_or_default()
    } else {
        Vec::new()
    };
    let support: Vec<String> = specs
        .iter()
        .find(|s| s.id == spec_id)
        .map(|s| s.support.clone())
        .unwrap_or_default();
    let tests: Vec<String> = Vec::new();
    let spec_text = read_spec_text(repo, spec_id)
        .unwrap_or_else(|| format!("(no spec.md section found for {spec_id})"));
    Ok((spec_text, support, tests))
}

fn read_spec_text(repo: &Path, spec_id: &str) -> Option<String> {
    let specs_dir = repo.join("specs");
    if !specs_dir.exists() {
        return None;
    }
    let mut found: Option<String> = None;
    let walker = walkdir::WalkDir::new(&specs_dir)
        .max_depth(3)
        .into_iter()
        .filter_map(|e| e.ok());
    for entry in walker {
        if entry.file_name() == "spec.md" {
            if let Ok(body) = std::fs::read_to_string(entry.path()) {
                if body.contains(spec_id) {
                    // Pull the first 2 KB around the first occurrence.
                    let idx = body.find(spec_id).unwrap_or(0);
                    let start = idx.saturating_sub(200);
                    let end = (idx + 2048).min(body.len());
                    found = Some(body[start..end].to_string());
                    break;
                }
            }
        }
    }
    found
}

pub fn run_cli(
    ctx: &specere_core::Ctx,
    spec: String,
    provider: String,
    max_iterations: u32,
    sandbox_mode: String,
    from_fixture: Option<PathBuf>,
    cap_usd: Option<f64>,
) -> Result<()> {
    let mode = sandbox::Mode::parse(&sandbox_mode)?;
    let mut params = RunParams::new(spec);
    params.provider = provider;
    params.max_iterations = max_iterations;
    params.sandbox_mode = mode;
    params.fixture_dir = from_fixture;
    params.cap_usd = cap_usd;
    let summary = run(ctx, params).with_context(|| "adversary run failed")?;
    let status = if summary.counterexample_found {
        "found"
    } else if summary.counterexample_candidate {
        "candidate"
    } else if summary.budget_exceeded {
        "budget_exceeded"
    } else {
        "no_counterexample"
    };
    println!(
        "specere adversary run: spec={} iterations={} status={} spent=${:.4}",
        summary.spec_id, summary.iterations_run, status, summary.spent_this_run_usd,
    );
    Ok(())
}
