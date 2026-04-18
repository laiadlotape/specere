//! SpecERE — Spec Entropy Regulation Engine.
//!
//! Composable, reversible Repo-SLAM scaffolding. Each capability is an
//! `AddUnit` (see `specere-core`); each `add` has a manifest-backed `remove`.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

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
        },
        Command::Status => specere_units::status(&ctx),
        Command::Verify => specere_units::verify(&ctx),
        Command::Doctor { clean_orphans } => {
            if clean_orphans {
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

    // Branch: use FactorGraphBP when the sensor-map has edges, else plain
    // PerSpecHMM. RBPF routing from the CLI is out-of-scope for #43.
    let mut hmm = if coupling.edges.is_empty() {
        FilterBackend::Hmm(specere_filter::PerSpecHMM::new(specs.clone(), motion))
    } else {
        FilterBackend::Bp(specere_filter::FactorGraphBP::new(
            specs.clone(),
            motion,
            &coupling,
        ))
    };

    let sensor = specere_filter::DefaultTestSensor;
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
    Ok(())
}

/// Thin enum so `run_filter_run` can dispatch to HMM or BP without trait
/// objects. RBPF needs explicit cluster config → not CLI-wired in #43.
enum FilterBackend {
    Hmm(specere_filter::PerSpecHMM),
    Bp(specere_filter::FactorGraphBP),
}

impl FilterBackend {
    fn predict(&mut self, files: &[&str]) {
        match self {
            Self::Hmm(f) => f.predict(files),
            Self::Bp(f) => f.predict(files),
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
        }
    }
    fn marginal(&self, spec_id: &str) -> Result<specere_filter::Belief> {
        match self {
            Self::Hmm(f) => f.marginal(spec_id),
            Self::Bp(f) => f.marginal(spec_id),
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
            println!("spec_id      p_unk   p_sat   p_vio   entropy  last_updated");
            println!("-----------  ------  ------  ------  -------  --------------------");
            for e in &entries {
                println!(
                    "{:<11}  {:>6.3}  {:>6.3}  {:>6.3}  {:>7.4}  {}",
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
