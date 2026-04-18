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

    let result = match cli.command {
        Command::Add {
            unit,
            adopt_edits,
            branch,
        } => {
            let flags = specere_units::AddFlags {
                branch,
                adopt_edits,
            };
            specere_units::add(&ctx, &unit, &flags)
        }
        Command::Remove {
            unit,
            force,
            delete_branch,
        } => specere_units::remove(&ctx, &unit, ctx.dry_run(), force, delete_branch),
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
        Command::Observe => specere_telemetry::observe(&ctx),
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
