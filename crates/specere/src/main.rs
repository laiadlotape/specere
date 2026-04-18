//! SpecERE — Spec Entropy Regulation Engine.
//!
//! Composable, reversible Repo-SLAM scaffolding. Each capability is an
//! `AddUnit` (see `specere-core`); each `add` has a manifest-backed `remove`.

use std::path::PathBuf;

use anyhow::Result;
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
        eprintln!("specere: error: {e}");
        std::process::exit(1);
    }
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
