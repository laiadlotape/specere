//! SpecERE — Spec Entropy Regulation Engine.
//!
//! Composable, reversible Repo-SLAM scaffolding. Each capability is an
//! `AddUnit` (see `specere-core`); each `add` has a manifest-backed `remove`.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

mod adversary;
mod evaluate;
mod harness;
mod smells;

/// SpecERE — Spec Entropy Regulation Engine.
#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Target repository (defaults to current directory).
    #[arg(long, global = true)]
    repo: Option<PathBuf>,

    /// Print what would be done without touching the filesystem.
    #[arg(long, global = true)]
    dry_run: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Install one add-unit into the target repository.
    Add {
        /// Unit id (e.g. `speckit`).
        unit: String,

        /// Accept on-disk content of owned files as the new baseline
        /// when SHA-diff detects user edits (FR-P1-003).
        #[arg(long)]
        adopt_edits: bool,

        /// Override the auto-created feature branch name (speckit only;
        /// FR-P1-002). Wins over `$SPECERE_FEATURE_BRANCH`.
        #[arg(long, value_name = "NAME")]
        branch: Option<String>,

        /// Write a platform-specific service artifact (otel-collector
        /// only; issue #13 / FR-P2-002).
        #[arg(long)]
        service: bool,
    },
    /// Remove one add-unit from the target repository.
    Remove {
        /// Unit id (e.g. `speckit`).
        unit: String,

        /// Remove user-edited files too (off by default).
        #[arg(long)]
        force: bool,

        /// Delete the auto-created feature branch, if
        /// `branch_was_created_by_specere = true` and working tree is
        /// clean (speckit only; FR-P1-007).
        #[arg(long)]
        delete_branch: bool,
    },
    /// Install the five day-one units in one idempotent pass
    /// (speckit → filter-state → claude-code-deploy → otel-collector → ears-linter).
    /// FR-P2-005 / issue #15.
    Init,
    /// Run one of the shipped linters. Issue #25.
    Lint {
        #[command(subcommand)]
        kind: LintKind,
    },
    /// List installed units and flag drift.
    Status,
    /// Re-hash every manifest entry and report drift.
    Verify,
    /// Diagnose the target repo (installed units, tool prerequisites).
    Doctor {
        /// Sweep orphan `.specify/` state left by aborted
        /// `specify workflow run` subprocesses (issue #16).
        #[arg(long)]
        clean_orphans: bool,
        /// Scan posterior + evidence calibration for specs where the filter
        /// reports high confidence in SAT despite low-quality evidence —
        /// write a review-queue entry for each (FR-EQ-006).
        #[arg(long)]
        suspicious: bool,
    },
    /// Record or query telemetry events. Issue #28 / FR-P3-004.
    Observe {
        #[command(subcommand)]
        kind: ObserveKind,
    },
    /// Start the embedded OTLP/HTTP + gRPC receivers. Blocks until SIGINT.
    /// Issue #30 (HTTP) + #34 (gRPC) / FR-P3-001 + FR-P3-005.
    Serve {
        /// Path to the otel-config.yml (default: `.specere/otel-config.yml`).
        #[arg(long)]
        config: Option<PathBuf>,
        /// Override the HTTP bind address (host:port). Wins over the YAML.
        #[arg(long)]
        bind: Option<String>,
        /// Override the gRPC bind address (host:port). Wins over the YAML.
        #[arg(long)]
        grpc_bind: Option<String>,
    },
    /// Per-spec Bayesian filter over the event store. Issue #43 / FR-P4-001..005.
    Filter {
        #[command(subcommand)]
        kind: FilterKind,
    },
    /// Learn filter parameters from repo history. Phase 5.
    Calibrate {
        #[command(subcommand)]
        kind: CalibrateKind,
    },
    /// Run external evaluators (mutation testing, property checkers) and
    /// emit the results as evidence events. FR-EQ-001..005.
    Evaluate {
        #[command(subcommand)]
        kind: EvaluateKind,
    },
    /// Harness manager & inspector — enumerate, categorise, and relate
    /// test/bench/fuzz/workflow files. FR-HM-001..085.
    Harness {
        #[command(subcommand)]
        kind: HarnessKind,
    },
    /// LLM adversary agent — propose spec-violating shell scripts,
    /// run them in a sandbox, minimize counter-examples. FR-EQ-020..024.
    Adversary {
        #[command(subcommand)]
        kind: AdversaryKind,
    },
}

#[derive(Subcommand)]
enum AdversaryKind {
    /// One iterative falsification run for a single spec. FR-EQ-020.
    Run {
        /// Target spec id (e.g. `FR-EQ-007`).
        #[arg(long)]
        spec: String,
        /// LLM provider. `mock` is deterministic + free (CI-safe); the
        /// real providers require `ANTHROPIC_API_KEY` or `OPENAI_API_KEY`.
        #[arg(long, default_value = "mock")]
        provider: String,
        /// Max iterations of (ask LLM → sandbox run → minimize). FR-EQ-022
        /// requires ≥ 3 for `counterexample_found`; one-shot findings are
        /// recorded as `counterexample_candidate` only.
        #[arg(long, default_value_t = 5)]
        max_iterations: u32,
        /// Sandbox mode: `none` (trust-provider; only with `mock` in CI),
        /// `rlimit` (default; CPU + vmem caps), `bubblewrap` (full
        /// namespace isolation). FR-EQ-024.
        #[arg(long, default_value = "rlimit")]
        sandbox: String,
        /// Override the per-month USD cap. Default 20.0.
        #[arg(long)]
        cap_usd: Option<f64>,
        /// Test-only — load canned `iter_<N>.sh` files from this dir
        /// instead of making real LLM calls. Implies `--provider mock`.
        #[arg(long, value_name = "PATH", hide = true)]
        from_fixture: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum HarnessKind {
    /// Walk the repo, classify every harness file into one of nine
    /// categories, extract test names, and emit direct-use edges parsed
    /// from `rustc --emit=dep-info` output. Writes
    /// `.specere/harness-graph.toml`. FR-HM-001..004.
    Scan {
        /// Output format for stdout summary. The TOML file is always
        /// written; this flag controls what the CLI prints.
        #[arg(long, default_value = "summary")]
        format: String,
    },
    /// Enrich `.specere/harness-graph.toml` with per-file provenance —
    /// which `/speckit-*` verb created the file (if any) plus the
    /// introducing git commit + author. FR-HM-010..012.
    Provenance,
    /// Enrich harness nodes with git-history metrics (age, commits,
    /// churn, authors, hotspot score) and emit pairwise co-modification
    /// edges via PPMI. FR-HM-020..022.
    History {
        /// Minimum co-modification count for a pair to emit an edge.
        /// Default 3 — same floor as `specere calibrate from-git`.
        #[arg(long, default_value_t = 3)]
        min_commits: u32,
    },
    /// Compute per-test coverage bitvectors via `cargo-llvm-cov`, then
    /// Jaccard similarity → `cov_cooccur` edges. FR-HM-030..033.
    Coverage {
        /// Test-only — read per-test LCOV files from this dir instead of
        /// running `cargo llvm-cov`. One file per test named
        /// `<path-with-slashes-as-__>.lcov` (e.g. `tests__it.lcov`).
        #[arg(long, value_name = "PATH", hide = true)]
        from_lcov_dir: Option<PathBuf>,
        /// Jaccard threshold for emitting a `cov_cooccur` edge. Default
        /// 0.1 — below that, noise dominates.
        #[arg(long, default_value_t = 0.1)]
        threshold: f64,
    },
    /// Interactive terminal UI for the harness inspector — file tree,
    /// detail pane, relation inspector overlay, event timeline footer.
    /// FR-HM-070..072.
    Tui {
        /// Test-only — render `N` frames to a TestBackend then exit,
        /// without touching the real terminal. CI smokes the widget tree.
        #[arg(long, value_name = "N", hide = true, default_value_t = 0)]
        headless_frames: u32,
    },
    /// Run Louvain community detection on the combined edge graph
    /// (direct + comod + cov_cooccur + cofail), write per-node
    /// `cluster_id`s + a cluster-summary table. FR-HM-050..052.
    Cluster {
        /// Deterministic seed for node-visitation order. Default 42.
        #[arg(long, default_value_t = 42)]
        seed: u64,
        /// Also print a `[harness_cluster]` TOML snippet to stdout for
        /// pasting into `.specere/sensor-map.toml`.
        #[arg(long)]
        emit_to_sensor_map: bool,
    },
    /// Compute per-test flakiness scores + pairwise co-failure PPMI
    /// edges from CI history. FR-HM-040..043.
    Flaky {
        /// Test-only — read pre-built `<run_id>` JSONL file instead of
        /// looking at the event store. One JSON object per line:
        /// `{"run_id":"<id>","outcomes":{"<path>":"pass|fail|skip"}}`.
        #[arg(long, value_name = "PATH", hide = true)]
        from_runs: Option<PathBuf>,
        /// Minimum joint-failure count for a `cofail` edge (Hoeffding
        /// floor). Default 5.
        #[arg(long, default_value_t = 5)]
        min_co_fail: u32,
        /// Flakiness-score cutoff. Tests above this are `probable_flake`
        /// and their `cofail` contributions get dampened. Default 0.01.
        #[arg(long, default_value_t = 0.01)]
        flake_threshold: f64,
        /// Minimum number of runs before reporting any score. Default
        /// 50 — below this, the CLI prints "insufficient history" (same
        /// pattern as FR-EQ-004 motion-from-evidence).
        #[arg(long, default_value_t = 50)]
        min_runs: u32,
    },
}

#[derive(Subcommand)]
enum EvaluateKind {
    /// Run `cargo-mutants` and emit one `mutation_result` event per mutant
    /// into `.specere/events.jsonl`. Events carry `spec_id` attributed via
    /// sensor-map support-set intersection. Advisory — a low kill rate is
    /// recorded, not an error. FR-EQ-001.
    Mutations {
        /// Override the sensor-map path (default: `.specere/sensor-map.toml`).
        #[arg(long)]
        sensor_map: Option<PathBuf>,
        /// Restrict mutation to files supporting this FR only.
        #[arg(long, value_name = "FR-ID")]
        scope: Option<String>,
        /// Pass through to `cargo-mutants --in-diff <REF>` for PR-scoped runs.
        #[arg(long)]
        in_diff: Option<String>,
        /// Parallelism. Forwarded to `cargo-mutants --jobs N`.
        #[arg(long, default_value_t = 1)]
        jobs: usize,
        /// Test-only — parse this existing `outcomes.json` instead of running
        /// `cargo-mutants`. The CLI otherwise always invokes the tool fresh.
        #[arg(long, value_name = "PATH", hide = true)]
        from_outcomes: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum CalibrateKind {
    /// Walk `git log` and propose `[coupling]` edges based on co-modification
    /// counts. Reads `[specs]` from sensor-map.toml; prints a TOML snippet to
    /// stdout that the user can paste into `.specere/sensor-map.toml`.
    FromGit {
        /// Override the sensor-map path (default: `.specere/sensor-map.toml`).
        #[arg(long)]
        sensor_map: Option<PathBuf>,
        /// How many most-recent commits to analyse. Default 500.
        #[arg(long, default_value_t = 500)]
        max_commits: usize,
        /// Minimum co-modification count for an edge to be proposed. Default 3.
        #[arg(long, default_value_t = 3)]
        min_commits: usize,
    },
    /// Fit per-spec motion matrices from the event store's
    /// `mutation_result` + `test_outcome` history (FR-EQ-004). Emits a
    /// TOML snippet with `[motion."<id>"]` + `[calibration."<id>"]`
    /// tables for the caller to paste into `.specere/sensor-map.toml`.
    MotionFromEvidence {
        /// Override the sensor-map path (default: `.specere/sensor-map.toml`).
        #[arg(long)]
        sensor_map: Option<PathBuf>,
        /// Minimum events per spec to emit a fit (else reports
        /// `insufficient history`). Default 20.
        #[arg(long, default_value_t = 20)]
        min_events: u32,
    },
}

#[derive(Subcommand)]
enum FilterKind {
    /// Consume new events since the last cursor, advance the posterior, write
    /// atomically to `.specere/posterior.toml`. Idempotent under no new events.
    Run {
        /// Override the sensor-map path (default: `.specere/sensor-map.toml`).
        #[arg(long)]
        sensor_map: Option<PathBuf>,
        /// Override the posterior path (default: `.specere/posterior.toml`).
        #[arg(long)]
        posterior: Option<PathBuf>,
    },
    /// Read the posterior and print a per-spec belief table.
    Status {
        /// Sort column + direction. One of `entropy,desc` (default), `p_sat,asc`,
        /// `p_sat,desc`, `p_vio,asc`, `p_vio,desc`.
        #[arg(long, default_value = "entropy,desc")]
        sort: String,
        /// Output format: `table` (default) or `json`.
        #[arg(long, default_value = "table")]
        format: String,
        /// Override the posterior path.
        #[arg(long)]
        posterior: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum ObserveKind {
    /// Append one event to `.specere/events.jsonl`. Typically invoked from a
    /// hook in `.specify/extensions.yml` (e.g. `after_implement`).
    Record {
        /// Slash-command verb or CLI source name (e.g. `implement`).
        #[arg(long)]
        source: String,
        /// Feature directory the event belongs to (from SpecKit).
        #[arg(long)]
        feature_dir: Option<PathBuf>,
        /// OTLP signal class: `traces` (default), `logs`, or `metrics`.
        #[arg(long, default_value = "traces")]
        signal: String,
        /// Human-readable name for the span / record. Defaults to `source`.
        #[arg(long)]
        name: Option<String>,
        /// Repeatable `key=value` attribute pairs (gen_ai.*, specere.*, ...).
        #[arg(long = "attr", value_name = "KEY=VALUE", num_args = 0..)]
        attrs: Vec<String>,
    },
    /// Read events back. Prints to stdout in the requested format.
    Query {
        /// Only events at or after this RFC3339 timestamp.
        #[arg(long)]
        since: Option<String>,
        /// Only events with this signal class.
        #[arg(long)]
        signal: Option<String>,
        /// Only events with this source.
        #[arg(long)]
        source: Option<String>,
        /// Cap results to the most recent N.
        #[arg(long)]
        limit: Option<usize>,
        /// Output format.
        #[arg(long, default_value = "table")]
        format: String,
    },
}

#[derive(Subcommand)]
enum LintKind {
    /// EARS-style lint over the active feature's spec.md (FR-P2-003).
    /// Advisory only — always exits 0.
    Ears,
    /// Static analysis of test files for smells that degrade sensor
    /// calibration (FR-EQ-003). Emits `test_smell_detected` events.
    /// Advisory — always exits 0.
    Tests {
        /// Override the sensor-map path (default: `.specere/sensor-map.toml`).
        #[arg(long)]
        sensor_map: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    let cli = Cli::parse();
    let repo = cli
        .repo
        .unwrap_or_else(|| std::env::current_dir().expect("cwd available"));
    let ctx = specere_core::Ctx::new(repo).with_dry_run(cli.dry_run);

    let result = match cli.command {
        Command::Add {
            unit,
            adopt_edits,
            branch,
            service,
        } => {
            let flags = specere_units::AddFlags {
                branch,
                adopt_edits,
                with_service: service,
            };
            specere_units::add(&ctx, &unit, &flags)
        }
        Command::Remove {
            unit,
            force,
            delete_branch,
        } => specere_units::remove(&ctx, &unit, ctx.dry_run(), force, delete_branch),
        Command::Init => specere_units::init(&ctx),
        Command::Lint { kind } => match kind {
            LintKind::Ears => specere_units::run_ears_lint(&ctx),
            LintKind::Tests { sensor_map } => smells::run_lint_tests(&ctx, sensor_map),
        },
        Command::Status => specere_units::status(&ctx),
        Command::Verify => specere_units::verify(&ctx),
        Command::Doctor {
            clean_orphans,
            suspicious,
        } => {
            if suspicious {
                run_doctor_suspicious(&ctx)
            } else if clean_orphans {
                match specere_units::clean_orphans(&ctx) {
                    Ok(0) => {
                        println!("No orphan .specify/ state detected.");
                        Ok(())
                    }
                    Ok(n) => {
                        println!("Cleaned {n} orphan artifact group(s).");
                        Ok(())
                    }
                    Err(e) => Err(e),
                }
            } else {
                specere_units::doctor(&ctx)
            }
        }
        Command::Observe { kind } => match kind {
            ObserveKind::Record {
                source,
                feature_dir,
                signal,
                name,
                attrs,
            } => run_observe_record(&ctx, source, feature_dir, signal, name, attrs),
            ObserveKind::Query {
                since,
                signal,
                source,
                limit,
                format,
            } => run_observe_query(&ctx, since, signal, source, limit, &format),
        },
        Command::Serve {
            config,
            bind,
            grpc_bind,
        } => run_serve(&ctx, config, bind, grpc_bind),
        Command::Filter { kind } => match kind {
            FilterKind::Run {
                sensor_map,
                posterior,
            } => run_filter_run(&ctx, sensor_map, posterior),
            FilterKind::Status {
                sort,
                format,
                posterior,
            } => run_filter_status(&ctx, &sort, &format, posterior),
        },
        Command::Calibrate { kind } => match kind {
            CalibrateKind::FromGit {
                sensor_map,
                max_commits,
                min_commits,
            } => run_calibrate_from_git(&ctx, sensor_map, max_commits, min_commits),
            CalibrateKind::MotionFromEvidence {
                sensor_map,
                min_events,
            } => run_calibrate_motion_from_evidence(&ctx, sensor_map, min_events),
        },
        Command::Evaluate { kind } => match kind {
            EvaluateKind::Mutations {
                sensor_map,
                scope,
                in_diff,
                jobs,
                from_outcomes,
            } => evaluate::run_mutations(&ctx, sensor_map, scope, in_diff, jobs, from_outcomes),
        },
        Command::Harness { kind } => match kind {
            HarnessKind::Scan { format } => harness::run_scan(&ctx, &format),
            HarnessKind::Provenance => harness::run_provenance(&ctx),
            HarnessKind::History { min_commits } => harness::run_history(&ctx, min_commits),
            HarnessKind::Coverage {
                from_lcov_dir,
                threshold,
            } => harness::run_coverage(&ctx, from_lcov_dir, threshold),
            HarnessKind::Flaky {
                from_runs,
                min_co_fail,
                flake_threshold,
                min_runs,
            } => harness::run_flaky(&ctx, from_runs, min_co_fail, flake_threshold, min_runs),
            HarnessKind::Cluster {
                seed,
                emit_to_sensor_map,
            } => harness::run_cluster(&ctx, seed, emit_to_sensor_map),
            HarnessKind::Tui { headless_frames } => harness::run_tui(&ctx, headless_frames),
        },
        Command::Adversary { kind } => match kind {
            AdversaryKind::Run {
                spec,
                provider,
                max_iterations,
                sandbox,
                cap_usd,
                from_fixture,
            } => adversary::run_cli(
                &ctx,
                spec,
                provider,
                max_iterations,
                sandbox,
                from_fixture,
                cap_usd,
            ),
        },
    };

    if let Err(e) = result {
        // Look for a specere_core::Error in the error chain for exit-code
        // + message formatting (contracts/cli.md §Stderr format).
        let root = e.root_cause();
        if let Some(specere_err) = root.downcast_ref::<specere_core::Error>() {
            eprintln!("specere: error: {specere_err}");
            print_help_hint(specere_err);
            std::process::exit(specere_err.exit_code());
        }
        // Print the full anyhow context chain so parse failures, file-system
        // errors, etc. surface their inner cause (manual-test M-04 / M-19 —
        // previously only the top-level `.context()` was shown).
        eprintln!("specere: error: {e:#}");
        std::process::exit(1);
    }
    Ok(())
}

fn run_serve(
    ctx: &specere_core::Ctx,
    config: Option<PathBuf>,
    bind: Option<String>,
    grpc_bind: Option<String>,
) -> Result<()> {
    let config_path = config.unwrap_or_else(|| ctx.repo().join(".specere/otel-config.yml"));
    let cfg = specere_telemetry::serve::load_config(&config_path);
    let http_bind = match bind {
        Some(addr) => addr.parse()?,
        None => cfg.http_bind,
    };
    let grpc_bind = match grpc_bind {
        Some(addr) => addr.parse()?,
        None => specere_telemetry::load_grpc_endpoint(&config_path)
            .unwrap_or_else(specere_telemetry::default_grpc_bind),
    };

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(async move {
        let repo = ctx.repo().to_path_buf();
        let (tx, rx) = tokio::sync::watch::channel(false);
        let signal = tokio::spawn(async move {
            if let Err(e) = tokio::signal::ctrl_c().await {
                tracing::warn!("failed to install SIGINT handler: {e}");
            }
            let _ = tx.send(true);
        });
        specere_telemetry::serve_both(repo, http_bind, grpc_bind, rx).await?;
        let _ = signal.await;
        Ok::<_, anyhow::Error>(())
    })?;
    Ok(())
}

fn run_observe_record(
    ctx: &specere_core::Ctx,
    source: String,
    feature_dir: Option<PathBuf>,
    signal: String,
    name: Option<String>,
    attrs: Vec<String>,
) -> Result<()> {
    let mut attrs_map = std::collections::BTreeMap::new();
    for pair in &attrs {
        match pair.split_once('=') {
            Some((k, v)) => {
                attrs_map.insert(k.to_string(), v.to_string());
            }
            None => anyhow::bail!(
                "invalid --attr `{pair}`; expected `KEY=VALUE` (e.g. --attr gen_ai.system=claude-code)"
            ),
        }
    }
    let event = specere_telemetry::Event {
        ts: String::new(), // filled in by record()
        source: source.clone(),
        signal,
        name: name.or_else(|| Some(source.clone())),
        feature_dir: feature_dir.map(|p| p.to_string_lossy().to_string()),
        attrs: attrs_map,
    };
    specere_telemetry::record(ctx, event)?;
    println!("specere observe record: 1 event appended to .specere/events.jsonl");
    Ok(())
}

fn run_observe_query(
    ctx: &specere_core::Ctx,
    since: Option<String>,
    signal: Option<String>,
    source: Option<String>,
    limit: Option<usize>,
    format: &str,
) -> Result<()> {
    let filters = specere_telemetry::QueryFilters {
        since,
        signal,
        source,
        limit,
    };
    let events = specere_telemetry::query(ctx, &filters)?;
    let fmt = match format {
        "json" => specere_telemetry::QueryFormat::Json,
        "toml" => specere_telemetry::QueryFormat::Toml,
        "table" => specere_telemetry::QueryFormat::Table,
        other => anyhow::bail!("unknown --format `{other}`; expected json|toml|table"),
    };
    let out = specere_telemetry::format_events(&events, fmt)?;
    println!("{out}");
    Ok(())
}

fn run_filter_run(
    ctx: &specere_core::Ctx,
    sensor_map: Option<PathBuf>,
    posterior: Option<PathBuf>,
) -> Result<()> {
    let sensor_map_path = sensor_map.unwrap_or_else(|| ctx.repo().join(".specere/sensor-map.toml"));
    let posterior_path =
        posterior.unwrap_or_else(|| specere_filter::Posterior::default_path(ctx.repo()));

    let specs = specere_filter::load_specs(&sensor_map_path)?;
    let coupling = specere_filter::CouplingGraph::load(&sensor_map_path)?;
    let motion = specere_filter::Motion::prototype_defaults();

    // Issue #50 — advisory exclusive lock on `.specere/filter.lock` so concurrent
    // `filter run` invocations queue instead of racing the atomic-write path.
    // `fs2::FileExt::lock_exclusive` blocks until the lock is acquired; the
    // lock releases automatically when the file handle drops.
    let lock_path = posterior_path
        .parent()
        .map(|p| p.join("filter.lock"))
        .unwrap_or_else(|| std::path::PathBuf::from(".specere/filter.lock"));
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let lock_file = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .with_context(|| format!("open advisory lock at {}", lock_path.display()))?;
    fs2::FileExt::lock_exclusive(&lock_file)
        .with_context(|| format!("acquire advisory lock at {}", lock_path.display()))?;

    let mut existing = specere_filter::Posterior::load_or_default(&posterior_path)?;
    let cursor = existing.cursor.clone();

    // Query events strictly *after* the cursor so a re-run with no new events
    // is a no-op (FR-P4-001). The event_store query uses `>= since` so we
    // compare-and-skip ourselves.
    let all_events = specere_telemetry::event_store::query(
        ctx.repo(),
        &specere_telemetry::event_store::QueryFilters::default(),
    )?;
    let new_events: Vec<_> = all_events
        .into_iter()
        .filter(|e| match &cursor {
            Some(c) => e.ts.as_str() > c.as_str(),
            None => true,
        })
        .collect();

    if new_events.is_empty() {
        // Idempotent re-run: posterior must stay byte-identical (FR-P4-001).
        // Still write to re-sort entries deterministically in case the file
        // was hand-edited out of order — the sort is a no-op on already-
        // sorted data.
        existing.write_atomic(&posterior_path)?;
        println!(
            "specere filter: no new events since {}",
            cursor.as_deref().unwrap_or("start")
        );
        return Ok(());
    }

    // Branch: RBPF when `[rbpf]` is configured (escape valve for cyclic
    // coupling clusters); FactorGraphBP when `[coupling]` edges form a
    // DAG; plain PerSpecHMM otherwise. Precedence documented on
    // `FilterBackend`.
    let rbpf_config = specere_filter::RbpfConfig::load(&sensor_map_path)
        .with_context(|| format!("parse [rbpf] from {}", sensor_map_path.display()))?;
    let mut hmm = match rbpf_config {
        Some(cfg) => {
            let cluster_refs: Vec<&str> = cfg.cluster.iter().map(String::as_str).collect();
            FilterBackend::Rbpf(Box::new(specere_filter::RBPF::new(
                specs.clone(),
                motion,
                &cluster_refs,
                cfg.n_particles,
                cfg.seed,
            )))
        }
        None if coupling.edges.is_empty() => {
            FilterBackend::Hmm(specere_filter::PerSpecHMM::new(specs.clone(), motion))
        }
        None => FilterBackend::Bp(specere_filter::FactorGraphBP::new(
            specs.clone(),
            motion,
            &coupling,
        )),
    };

    // FR-P6 cross-session resume: seed the backend's belief buffer from the
    // persisted posterior so repeated `filter run` invocations don't lose
    // accumulated belief. Entries for specs no longer in `[specs]` are
    // silently dropped by `set_belief`.
    for entry in &existing.entries {
        hmm.set_belief(&entry.spec_id, &[entry.p_unk, entry.p_sat, entry.p_vio]);
    }

    // FR-EQ-005 — compute per-spec calibration from aggregated mutation +
    // smell events across ALL events (not just new ones), then build a
    // PerSpecTestSensor keyed by spec_id. At quality=1.0 for every spec,
    // the filter's numerical output is bit-identical to v1.0.4 — so
    // repos that don't run `specere evaluate mutations` see no change.
    let all_events_for_calibration = specere_telemetry::event_store::query(
        ctx.repo(),
        &specere_telemetry::event_store::QueryFilters::default(),
    )?;
    // FR-HM-052b — if a harness-graph exists on disk, pull cluster-level
    // flakiness into the calibration formula. Absent graph → behaviour is
    // bit-identical to pre-FR-HM-052b (no cluster compression).
    let harness_graph_path = ctx.repo().join(".specere").join("harness-graph.toml");
    let harness_graph = harness::node::HarnessGraph::load_or_default(&harness_graph_path)
        .unwrap_or_else(|_| harness::node::HarnessGraph {
            schema_version: 1,
            nodes: Vec::new(),
            edges: Vec::new(),
            comod_edges: Vec::new(),
            cov_cooccur_edges: Vec::new(),
            cofail_edges: Vec::new(),
            cluster_report: None,
        });
    let calibrations = compute_per_spec_calibrations_with_clusters(
        &specs,
        &all_events_for_calibration,
        &harness_graph,
    );
    let mut sensor = specere_filter::PerSpecTestSensor::new();
    for (sid, cal) in &calibrations {
        sensor.insert(sid, *cal);
    }
    let mut processed = 0usize;
    let mut skipped = 0usize;
    // Cursor advances to the **max** observed ts, not the last-processed one —
    // JSONL appends can arrive out of order (backfills, post-hoc late events),
    // and taking the last-iterated ts breaks FR-P4-001 on a subsequent re-run.
    let mut latest_ts: Option<String> = None;
    for e in new_events {
        match &latest_ts {
            Some(cur) if e.ts.as_str() <= cur.as_str() => {}
            _ => latest_ts = Some(e.ts.clone()),
        }
        let kind = e.attrs.get("event_kind").map(String::as_str);
        let spec_id = e.attrs.get("spec_id").map(String::as_str);
        match (kind, spec_id) {
            (Some("test_outcome"), Some(sid)) => {
                let outcome = e.attrs.get("outcome").map(String::as_str).unwrap_or("");
                match hmm.update_test(sid, outcome, &sensor) {
                    Ok(()) => processed += 1,
                    Err(_) => skipped += 1,
                }
            }
            (Some("files_touched"), _) => {
                let raw = e.attrs.get("paths").map(String::as_str).unwrap_or("");
                let paths = specere_filter::parse_paths(raw);
                let refs: Vec<&str> = paths.iter().map(|s| s.as_str()).collect();
                hmm.predict(&refs);
                processed += 1;
            }
            _ => skipped += 1,
        }
    }

    // Snapshot marginals into a fresh Posterior.
    let write_ts = latest_ts
        .clone()
        .unwrap_or_else(specere_telemetry::event_store::now_rfc3339);
    let entries = specs
        .iter()
        .map(|s| {
            hmm.marginal(&s.id)
                .map(|b| specere_filter::Entry::from_belief(&s.id, &b, &write_ts))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut out = specere_filter::Posterior {
        cursor: latest_ts,
        schema_version: 1,
        entries,
    };
    out.write_atomic(&posterior_path)?;

    println!(
        "specere filter: processed {processed} event(s), skipped {skipped}; cursor -> {}",
        out.cursor.as_deref().unwrap_or("<unchanged>")
    );
    // FR-EQ-005 — print calibration summary. Only shows specs that have
    // any non-default calibration (i.e., evidence events were processed).
    // Suppressed entirely when every spec is at q=1.0 to avoid noise on
    // v1.0.4-style repos.
    let non_default: Vec<_> = specs
        .iter()
        .filter_map(|s| {
            calibrations
                .get(&s.id)
                .filter(|c| (c.quality - 1.0).abs() > 1e-9)
                .map(|c| (s.id.clone(), *c))
        })
        .collect();
    if !non_default.is_empty() {
        println!("  calibration (per spec, from mutation + smell evidence):");
        for (sid, c) in &non_default {
            let suspicious = if c.quality < 0.5 {
                " ← low evidence"
            } else {
                ""
            };
            println!(
                "    {:<24} q={:.2} α_sat={:.2} α_vio={:.2}{}",
                sid, c.quality, c.alpha_sat, c.alpha_vio, suspicious
            );
        }
    }
    Ok(())
}

/// FR-HM-052b extension — same as [`compute_per_spec_calibrations`] but
/// also threads cluster-level flakiness (from `harness-graph.toml`) into
/// `Calibration::from_cluster_evidence`. When the harness graph is
/// empty, this falls back to the old per-spec-only formula so existing
/// repos see bit-identical output.
fn compute_per_spec_calibrations_with_clusters(
    specs: &[specere_filter::hmm::SpecDescriptor],
    events: &[specere_telemetry::Event],
    harness_graph: &harness::node::HarnessGraph,
) -> std::collections::HashMap<String, specere_filter::Calibration> {
    use std::collections::HashMap;
    // Short-circuit when there's no harness-graph data.
    if harness_graph.nodes.is_empty() {
        return compute_per_spec_calibrations(specs, events);
    }

    // Build per-cluster flakiness_score mean.
    let mut cluster_flake_totals: HashMap<String, (f64, u32)> = HashMap::new();
    for node in &harness_graph.nodes {
        if let (Some(cid), Some(fs)) = (&node.cluster_id, node.flakiness_score) {
            let entry = cluster_flake_totals.entry(cid.clone()).or_insert((0.0, 0));
            entry.0 += fs;
            entry.1 += 1;
        }
    }
    let cluster_flake_mean: HashMap<String, f64> = cluster_flake_totals
        .into_iter()
        .filter(|(_, (_, n))| *n > 0)
        .map(|(cid, (total, n))| (cid, total / n as f64))
        .collect();

    // Re-run per-spec mutation + smell aggregation (same as the baseline).
    let mut caught: HashMap<String, u32> = HashMap::new();
    let mut missed: HashMap<String, u32> = HashMap::new();
    let mut smells: HashMap<String, std::collections::HashSet<String>> = HashMap::new();
    for e in events {
        let kind = e.attrs.get("event_kind").map(String::as_str);
        let spec_id = e.attrs.get("spec_id").map(String::as_str);
        match (kind, spec_id) {
            (Some("mutation_result"), Some(sid)) => {
                let outcome = e.attrs.get("outcome").map(String::as_str).unwrap_or("");
                match outcome {
                    "caught" => *caught.entry(sid.to_string()).or_insert(0) += 1,
                    "missed" | "timeout" => *missed.entry(sid.to_string()).or_insert(0) += 1,
                    _ => {}
                }
            }
            (Some("test_smell_detected"), Some(sid)) => {
                let fn_name = e.attrs.get("test_fn").map(String::as_str).unwrap_or("");
                let smell = e.attrs.get("smell_kind").map(String::as_str).unwrap_or("");
                smells
                    .entry(sid.to_string())
                    .or_default()
                    .insert(format!("{fn_name}|{smell}"));
            }
            _ => {}
        }
    }

    // Per-spec cluster flakiness = mean of flakiness_scores across every
    // harness node whose path is a member of any support entry for that
    // spec. If those nodes belong to clusters, we use the cluster-mean
    // flakiness, not the raw node flakiness — captures "my tests share
    // a cluster with known-flaky peers" even when the spec's own tests
    // have no history yet.
    let mut out: HashMap<String, specere_filter::Calibration> = HashMap::new();
    for spec in specs {
        let c = caught.get(&spec.id).copied().unwrap_or(0);
        let m = missed.get(&spec.id).copied().unwrap_or(0);
        let kill_rate = if c + m == 0 {
            1.0
        } else {
            c as f64 / (c + m) as f64
        };
        let n_smells = smells.get(&spec.id).map(|s| s.len()).unwrap_or(0) as f64;
        let smell_penalty = (1.0 - 0.15 * n_smells).clamp(0.3, 1.0);

        // Find harness nodes whose path starts with any of this spec's
        // support prefixes. Reuse the same directory-boundary semantics
        // as `calibrate from-git` (v1.0.1 fix).
        let mut peer_cluster_scores: Vec<f64> = Vec::new();
        for node in &harness_graph.nodes {
            let matches = spec.support.iter().any(|sup| {
                let bare = sup.trim_end_matches('/');
                let dir = format!("{bare}/");
                node.path == bare || node.path.starts_with(dir.as_str())
            });
            if !matches {
                continue;
            }
            if let Some(cid) = &node.cluster_id {
                if let Some(mean) = cluster_flake_mean.get(cid) {
                    peer_cluster_scores.push(*mean);
                }
            }
        }
        let cluster_flakiness = if peer_cluster_scores.is_empty() {
            0.0
        } else {
            peer_cluster_scores.iter().sum::<f64>() / peer_cluster_scores.len() as f64
        };

        out.insert(
            spec.id.clone(),
            specere_filter::Calibration::from_cluster_evidence(
                kill_rate,
                smell_penalty,
                cluster_flakiness,
            ),
        );
    }
    out
}

/// Aggregate `mutation_result` + `test_smell_detected` events per spec
/// into a [`Calibration`] via FR-EQ-002's formula. Specs with no evidence
/// receive [`Calibration::prototype`] — bit-identical to v1.0.4 behaviour.
fn compute_per_spec_calibrations(
    specs: &[specere_filter::hmm::SpecDescriptor],
    events: &[specere_telemetry::Event],
) -> std::collections::HashMap<String, specere_filter::Calibration> {
    use std::collections::HashMap;
    // Kill-rate numerator + denominator per spec.
    let mut caught: HashMap<String, u32> = HashMap::new();
    let mut missed: HashMap<String, u32> = HashMap::new();
    // Distinct test_fn × smell_kind pairs per spec (dedupe repeated lint runs).
    let mut smells: HashMap<String, std::collections::HashSet<String>> = HashMap::new();

    for e in events {
        let kind = e.attrs.get("event_kind").map(String::as_str);
        let spec_id = e.attrs.get("spec_id").map(String::as_str);
        match (kind, spec_id) {
            (Some("mutation_result"), Some(sid)) => {
                let outcome = e.attrs.get("outcome").map(String::as_str).unwrap_or("");
                match outcome {
                    "caught" => *caught.entry(sid.to_string()).or_insert(0) += 1,
                    "missed" | "timeout" => *missed.entry(sid.to_string()).or_insert(0) += 1,
                    // "unviable" is excluded from the denominator.
                    _ => {}
                }
            }
            (Some("test_smell_detected"), Some(sid)) => {
                let fn_name = e.attrs.get("test_fn").map(String::as_str).unwrap_or("");
                let smell = e.attrs.get("smell_kind").map(String::as_str).unwrap_or("");
                let key = format!("{fn_name}|{smell}");
                smells.entry(sid.to_string()).or_default().insert(key);
            }
            _ => {}
        }
    }

    let mut out: std::collections::HashMap<String, specere_filter::Calibration> =
        std::collections::HashMap::new();
    for spec in specs {
        let c = caught.get(&spec.id).copied().unwrap_or(0);
        let m = missed.get(&spec.id).copied().unwrap_or(0);
        let kill_rate = if c + m == 0 {
            // No mutation evidence — treat as "perfect" so prototype alphas hold.
            1.0
        } else {
            c as f64 / (c + m) as f64
        };
        let n_smells = smells.get(&spec.id).map(|s| s.len()).unwrap_or(0) as f64;
        let smell_penalty = (1.0 - 0.15 * n_smells).clamp(0.3, 1.0);
        out.insert(
            spec.id.clone(),
            specere_filter::Calibration::from_evidence(kill_rate, smell_penalty),
        );
    }
    out
}

/// Thin enum so `run_filter_run` can dispatch to HMM, BP, or RBPF
/// without trait objects. Routing precedence (highest first):
///
/// 1. `[rbpf] cluster = [...]` in sensor-map → RBPF.
/// 2. `[coupling] edges = [...]` (non-empty DAG) → FactorGraphBP.
/// 3. Otherwise → plain PerSpecHMM.
enum FilterBackend {
    Hmm(specere_filter::PerSpecHMM),
    Bp(specere_filter::FactorGraphBP),
    /// Boxed because RBPF's particle buffer is the largest variant
    /// (~880 bytes); keeping the enum size bounded by the non-RBPF
    /// variants avoids wasted memory on every dispatch.
    Rbpf(Box<specere_filter::RBPF>),
}

impl FilterBackend {
    fn predict(&mut self, files: &[&str]) {
        match self {
            Self::Hmm(f) => f.predict(files),
            Self::Bp(f) => f.predict(files),
            Self::Rbpf(f) => f.predict(files),
        }
    }
    fn update_test<S: specere_filter::TestSensor>(
        &mut self,
        spec_id: &str,
        outcome: &str,
        sensor: &S,
    ) -> Result<()> {
        match self {
            Self::Hmm(f) => f.update_test(spec_id, outcome, sensor),
            Self::Bp(f) => f.update_test(spec_id, outcome, sensor),
            Self::Rbpf(f) => f.update_test(spec_id, outcome, sensor),
        }
    }
    fn marginal(&self, spec_id: &str) -> Result<specere_filter::Belief> {
        match self {
            Self::Hmm(f) => f.marginal(spec_id),
            Self::Bp(f) => f.marginal(spec_id),
            Self::Rbpf(f) => f.marginal(spec_id),
        }
    }
    /// Seed one spec's belief — delegates into the backend's HMM buffer.
    fn set_belief(&mut self, spec_id: &str, belief: &[f64]) {
        match self {
            Self::Hmm(f) => f.set_belief(spec_id, belief),
            Self::Bp(f) => f.set_belief(spec_id, belief),
            Self::Rbpf(f) => f.set_belief(spec_id, belief),
        }
    }
}

fn run_filter_status(
    ctx: &specere_core::Ctx,
    sort: &str,
    format: &str,
    posterior: Option<PathBuf>,
) -> Result<()> {
    let posterior_path =
        posterior.unwrap_or_else(|| specere_filter::Posterior::default_path(ctx.repo()));
    if !posterior_path.exists() {
        println!("no posterior yet — run `specere filter run` first");
        return Ok(());
    }
    let p = specere_filter::Posterior::load_or_default(&posterior_path)?;

    // Empty-posterior hint — the file exists (so the "no posterior yet"
    // branch above didn't fire) but has zero entries. Surfaced by manual-
    // test M-07-B.
    if p.entries.is_empty() {
        println!(
            "posterior has no entries — no events processed yet. \
             Add `[specs]` + seed events, then `specere filter run`."
        );
        return Ok(());
    }

    let mut entries = p.entries.clone();
    sort_entries(&mut entries, sort)?;

    match format {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&entries)?);
        }
        "table" => {
            // Spec-id column width = max(header, longest spec_id), capped
            // at 64 to avoid hostile-input blowups. Previously fixed at
            // 11, which truncated domain-prefixed FR ids (`FR-auth-001`,
            // `FR-EQ-004`). See docs/upcoming.md §4 closure.
            const HEADER: &str = "spec_id";
            let id_width = entries
                .iter()
                .map(|e| e.spec_id.len())
                .chain(std::iter::once(HEADER.len()))
                .max()
                .unwrap_or(HEADER.len())
                .clamp(HEADER.len(), 64);
            let id_dashes: String = "-".repeat(id_width);
            println!(
                "{:<id_width$}  p_unk   p_sat   p_vio   entropy  last_updated",
                HEADER
            );
            println!("{id_dashes}  ------  ------  ------  -------  --------------------");
            for e in &entries {
                println!(
                    "{:<id_width$}  {:>6.3}  {:>6.3}  {:>6.3}  {:>7.4}  {}",
                    e.spec_id, e.p_unk, e.p_sat, e.p_vio, e.entropy, e.last_updated
                );
            }
        }
        other => anyhow::bail!("unknown --format `{other}`; one of `table` (default) or `json`"),
    }
    Ok(())
}

fn sort_entries(entries: &mut [specere_filter::Entry], sort: &str) -> Result<()> {
    use std::cmp::Ordering;
    let (field, dir) = sort
        .split_once(',')
        .ok_or_else(|| anyhow::anyhow!("--sort expects `field,asc|desc` (got `{sort}`)"))?;
    // Validate direction explicitly — previously any non-"asc" string silently
    // became `desc`. Surfaced by manual-test M-15-B.
    let ascending = match dir {
        "asc" => true,
        "desc" => false,
        other => anyhow::bail!("--sort direction must be `asc` or `desc` (got `{other}`)"),
    };
    let cmp: fn(&specere_filter::Entry, &specere_filter::Entry) -> Ordering = match field {
        "entropy" => |a, b| a.entropy.partial_cmp(&b.entropy).unwrap_or(Ordering::Equal),
        "p_sat" => |a, b| a.p_sat.partial_cmp(&b.p_sat).unwrap_or(Ordering::Equal),
        "p_vio" => |a, b| a.p_vio.partial_cmp(&b.p_vio).unwrap_or(Ordering::Equal),
        "p_unk" => |a, b| a.p_unk.partial_cmp(&b.p_unk).unwrap_or(Ordering::Equal),
        "spec_id" => |a, b| a.spec_id.cmp(&b.spec_id),
        _ => anyhow::bail!(
            "unknown --sort field `{field}`; one of entropy, p_sat, p_vio, p_unk, spec_id"
        ),
    };
    entries.sort_by(|a, b| {
        let o = cmp(a, b);
        if ascending {
            o
        } else {
            o.reverse()
        }
    });
    Ok(())
}

fn run_calibrate_from_git(
    ctx: &specere_core::Ctx,
    sensor_map: Option<PathBuf>,
    max_commits: usize,
    min_commits: usize,
) -> Result<()> {
    let sensor_map_path = sensor_map.unwrap_or_else(|| ctx.repo().join(".specere/sensor-map.toml"));
    let specs = specere_filter::load_specs(&sensor_map_path)?;
    let opts = specere_filter::CalibrateOpts {
        max_commits: Some(max_commits),
        min_commits,
    };
    let report = specere_filter::calibrate_from_git(ctx.repo(), &specs, &opts)?;

    // Write the TOML snippet to stdout; summary + hints to stderr so the
    // caller can redirect stdout straight into their sensor-map.toml edit.
    eprintln!(
        "specere calibrate: analysed {} commit(s); {} touched a tracked spec",
        report.commits_analysed, report.commits_with_spec_activity
    );
    if !report.spec_activity.is_empty() {
        eprintln!("  per-spec touch counts:");
        for (sid, n) in &report.spec_activity {
            eprintln!("    {sid:<32} {n}");
        }
    }
    if report.edges.is_empty() {
        eprintln!(
            "  no coupling edges proposed (raise --max-commits or lower --min-commits to soften the threshold)"
        );
    } else {
        eprintln!(
            "  {} edge(s) proposed; paste the snippet below into `.specere/sensor-map.toml`",
            report.edges.len()
        );
    }
    if !report.dropped_cycle_edges.is_empty() {
        eprintln!(
            "  {} edge(s) dropped because they would have closed a cycle (see snippet for detail)",
            report.dropped_cycle_edges.len()
        );
    }
    println!("{}", report.to_toml_snippet());
    Ok(())
}

/// FR-EQ-004 — fit per-spec motion matrices from the event store.
///
/// Prints a TOML snippet of `[motion."<id>"]` + `[calibration."<id>"]`
/// tables the caller pastes into `.specere/sensor-map.toml`. Specs with
/// too few events emit an `insufficient history` comment instead.
fn run_calibrate_motion_from_evidence(
    ctx: &specere_core::Ctx,
    sensor_map: Option<PathBuf>,
    min_events: u32,
) -> Result<()> {
    let sensor_map_path = sensor_map.unwrap_or_else(|| ctx.repo().join(".specere/sensor-map.toml"));
    let specs = specere_filter::load_specs(&sensor_map_path)?;
    let spec_ids: Vec<String> = specs.iter().map(|s| s.id.clone()).collect();

    let events = specere_telemetry::event_store::query(
        ctx.repo(),
        &specere_telemetry::event_store::QueryFilters::default(),
    )?;
    let inputs: Vec<specere_filter::FitInput> = events
        .iter()
        .filter_map(|e| {
            let spec_id = e.attrs.get("spec_id").cloned()?;
            let kind = e.attrs.get("event_kind").cloned().unwrap_or_default();
            let outcome = e.attrs.get("outcome").cloned().unwrap_or_default();
            Some(specere_filter::FitInput {
                spec_id,
                kind,
                outcome,
            })
        })
        .collect();

    let report = specere_filter::fit_motion_from_evidence(&spec_ids, &inputs, min_events);

    let mut fitted = 0usize;
    let mut insufficient = 0usize;
    for fit in report.per_spec.values() {
        match fit {
            specere_filter::SpecFit::Fitted { .. } => fitted += 1,
            specere_filter::SpecFit::InsufficientHistory { .. } => insufficient += 1,
        }
    }
    eprintln!(
        "specere calibrate motion-from-evidence: {fitted} spec(s) fitted, {insufficient} with insufficient history (threshold: {min_events})"
    );
    println!("{}", report.to_toml_snippet());
    Ok(())
}

/// FR-EQ-006 — review-queue flagging of suspicious high-confidence SAT.
///
/// Reads `.specere/posterior.toml` and the event store's calibration.
/// For each spec where `p_sat > suspicious_p_sat_min` (default 0.95) AND
/// `quality < suspicious_quality_max` (default 0.50), appends a
/// human-review entry to `.specere/review-queue.md`. Thresholds are
/// configurable via `[review]` in sensor-map.toml. Never removes
/// entries — manual review is the adjudication (constitution V).
fn run_doctor_suspicious(ctx: &specere_core::Ctx) -> Result<()> {
    let sensor_map_path = ctx.repo().join(".specere/sensor-map.toml");
    let specs = specere_filter::load_specs(&sensor_map_path).unwrap_or_default();
    let (p_sat_min, quality_max) = load_suspicious_thresholds(&sensor_map_path);

    let posterior_path = specere_filter::Posterior::default_path(ctx.repo());
    let posterior = specere_filter::Posterior::load_or_default(&posterior_path)
        .context("load posterior.toml for --suspicious scan")?;
    if posterior.entries.is_empty() {
        println!(
            "specere doctor --suspicious: posterior is empty — run `specere filter run` first"
        );
        return Ok(());
    }

    let all_events = specere_telemetry::event_store::query(
        ctx.repo(),
        &specere_telemetry::event_store::QueryFilters::default(),
    )?;
    let calibrations = compute_per_spec_calibrations(&specs, &all_events);

    let today = specere_telemetry::event_store::now_rfc3339()
        .split('T')
        .next()
        .unwrap_or("")
        .to_string();

    let mut flagged: Vec<String> = Vec::new();
    for entry in &posterior.entries {
        let cal = calibrations
            .get(&entry.spec_id)
            .copied()
            .unwrap_or(specere_filter::Calibration::prototype());
        if entry.p_sat > p_sat_min && cal.quality < quality_max {
            let (kill, smells) = kill_and_smells(&entry.spec_id, &all_events);
            flagged.push(format_review_entry(entry, &cal, kill, smells, &today));
        }
    }

    if flagged.is_empty() {
        println!(
            "specere doctor --suspicious: no suspicious specs (p_sat > {p_sat_min:.2} ∧ quality < {quality_max:.2})"
        );
        return Ok(());
    }

    let queue_path = ctx.repo().join(".specere").join("review-queue.md");
    if let Some(parent) = queue_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let mut existing = std::fs::read_to_string(&queue_path).unwrap_or_default();
    if !existing.is_empty() && !existing.ends_with("\n\n") {
        if existing.ends_with('\n') {
            existing.push('\n');
        } else {
            existing.push_str("\n\n");
        }
    }
    for entry in &flagged {
        existing.push_str(entry);
        existing.push('\n');
    }
    std::fs::write(&queue_path, existing)
        .with_context(|| format!("write {}", queue_path.display()))?;

    println!(
        "specere doctor --suspicious: flagged {} spec(s); appended to {}",
        flagged.len(),
        queue_path.display()
    );
    Ok(())
}

fn load_suspicious_thresholds(sensor_map_path: &std::path::Path) -> (f64, f64) {
    const DEFAULT_P_SAT_MIN: f64 = 0.95;
    const DEFAULT_QUALITY_MAX: f64 = 0.50;
    let raw = match std::fs::read_to_string(sensor_map_path) {
        Ok(s) => s,
        Err(_) => return (DEFAULT_P_SAT_MIN, DEFAULT_QUALITY_MAX),
    };
    let val: toml::Value = match toml::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return (DEFAULT_P_SAT_MIN, DEFAULT_QUALITY_MAX),
    };
    let review = val.get("review").and_then(|v| v.as_table());
    let p_sat_min = review
        .and_then(|t| t.get("suspicious_p_sat_min"))
        .and_then(|v| v.as_float())
        .unwrap_or(DEFAULT_P_SAT_MIN);
    let quality_max = review
        .and_then(|t| t.get("suspicious_quality_max"))
        .and_then(|v| v.as_float())
        .unwrap_or(DEFAULT_QUALITY_MAX);
    (p_sat_min, quality_max)
}

fn kill_and_smells(spec_id: &str, events: &[specere_telemetry::Event]) -> (Option<f64>, usize) {
    let mut caught = 0u32;
    let mut missed = 0u32;
    let mut smell_keys: std::collections::HashSet<String> = std::collections::HashSet::new();
    for e in events {
        let kind = e.attrs.get("event_kind").map(String::as_str);
        let sid = e.attrs.get("spec_id").map(String::as_str);
        if sid != Some(spec_id) {
            continue;
        }
        match kind {
            Some("mutation_result") => {
                match e.attrs.get("outcome").map(String::as_str).unwrap_or("") {
                    "caught" => caught += 1,
                    "missed" | "timeout" => missed += 1,
                    _ => {}
                }
            }
            Some("test_smell_detected") => {
                let fn_name = e.attrs.get("test_fn").map(String::as_str).unwrap_or("");
                let smell = e.attrs.get("smell_kind").map(String::as_str).unwrap_or("");
                smell_keys.insert(format!("{fn_name}|{smell}"));
            }
            _ => {}
        }
    }
    let kill = if caught + missed > 0 {
        Some(caught as f64 / (caught + missed) as f64)
    } else {
        None
    };
    (kill, smell_keys.len())
}

fn format_review_entry(
    entry: &specere_filter::Entry,
    cal: &specere_filter::Calibration,
    kill_rate: Option<f64>,
    n_smells: usize,
    today: &str,
) -> String {
    let kill_str = kill_rate
        .map(|k| format!("{k:.2}"))
        .unwrap_or_else(|| "n/a".to_string());
    let smell_penalty = (1.0 - 0.15 * n_smells as f64).clamp(0.3, 1.0);
    format!(
        "## Suspicious high-confidence SAT — {sid} (auto-flagged {today})\n\
         \n\
         - **Posterior**: p_sat = {psat:.2}, p_vio = {pvio:.2}, p_unk = {punk:.2}\n\
         - **Calibration**: quality = {q:.2} (mutation kill {k}, smell penalty {sp:.2})\n\
         - **Recommendation**: Human sanity-check before trusting this SAT — either the test suite is too weak to discriminate, or smells are dragging calibration down.\n",
        sid = entry.spec_id,
        today = today,
        psat = entry.p_sat,
        pvio = entry.p_vio,
        punk = entry.p_unk,
        q = cal.quality,
        k = kill_str,
        sp = smell_penalty,
    )
}

fn print_help_hint(e: &specere_core::Error) {
    use specere_core::Error::*;
    match e {
        AlreadyInstalledMismatch { unit, files } => {
            eprintln!("  help: run `specere add {unit} --adopt-edits` to accept your changes");
            for f in files {
                eprintln!("  affected: {}", f.display());
            }
        }
        DeletedOwnedFile { unit, .. } => {
            eprintln!("  help: run `specere remove {unit}` then `specere add {unit}` instead");
        }
        ParseFailure { path, .. } => {
            eprintln!("  affected: {}", path.display());
        }
        BranchDirty { .. } => {
            eprintln!("  help: stash or commit your changes first (`git stash`)");
        }
        _ => {}
    }
}
