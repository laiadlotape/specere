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

        /// Extra flags forwarded to the unit (key=value).
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        flags: Vec<String>,
    },
    /// Remove one add-unit from the target repository.
    Remove {
        /// Unit id (e.g. `speckit`).
        unit: String,

        /// Remove user-edited files too (off by default).
        #[arg(long)]
        force: bool,
    },
    /// List installed units and flag drift.
    Status,
    /// Re-hash every manifest entry and report drift.
    Verify,
    /// Diagnose the target repo (installed units, tool prerequisites).
    Doctor,
    /// Emit telemetry records from a hook invocation.
    Observe,
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

    match cli.command {
        Command::Add { unit, flags } => specere_units::add(&ctx, &unit, &flags),
        Command::Remove { unit, force } => specere_units::remove(&ctx, &unit, ctx.dry_run(), force),
        Command::Status => specere_units::status(&ctx),
        Command::Verify => specere_units::verify(&ctx),
        Command::Doctor => specere_units::doctor(&ctx),
        Command::Observe => specere_telemetry::observe(&ctx),
    }
}
