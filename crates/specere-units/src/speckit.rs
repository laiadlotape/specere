//! `specere add speckit` — a **wrapper unit** for the github/spec-kit
//! scaffolder.
//!
//! Wrapper-unit semantics (see `docs/roadmap/31_specere_scaffolding.md` §11.1):
//!
//! * The manifest entry is **minimal**: `(unit_id, version, install_config,
//!   installed_at)`. No file list — SpecKit already tracks its own integration
//!   state in `.specify/integrations/integration.json`. Duplicating it is
//!   wheel-reinvention and drifts every time SpecKit upgrades.
//! * `install` shells out to `uvx ... specify init`. We record only that we
//!   invoked it, and with which pin.
//! * `remove` delegates first to SpecKit's own removal verbs (when they exist),
//!   then falls back to a directory wipe behind confirmation semantics.
//!
//! If the user wants file-level tracking of what SpecKit produced, they read
//! SpecKit's own manifest — it is authoritative, not ours.

use std::path::PathBuf;
use std::process::Command;

use specere_core::{AddUnit, Ctx, Plan, PlanOp, Record, Result};

/// Pinned SpecKit upstream tag. Bump in lockstep with our CHANGELOG.
pub const PINNED_SPECKIT_TAG: &str = "v0.7.3";

/// Default agent integration. Overridable via `specere add speckit -- --integration=<agent>` (flag plumbing in a later release).
pub const DEFAULT_INTEGRATION: &str = "claude";

pub struct Speckit;

impl AddUnit for Speckit {
    fn id(&self) -> &'static str {
        "speckit"
    }

    fn pinned_version(&self) -> &'static str {
        PINNED_SPECKIT_TAG
    }

    fn preflight(&self, _ctx: &Ctx) -> Result<Plan> {
        let mut plan = Plan::default();
        if !command_exists("uvx") {
            return Err(specere_core::Error::Preflight(
                "`uvx` not found in PATH; install `uv` (https://github.com/astral-sh/uv) to use `specere add speckit`".into(),
            ));
        }
        plan.ops.push(PlanOp::RunCommand {
            program: "uvx".into(),
            args: uvx_init_args(),
        });
        Ok(plan)
    }

    fn install(&self, ctx: &Ctx, _plan: &Plan) -> Result<Record> {
        let status = Command::new("uvx")
            .args(uvx_init_args())
            .current_dir(ctx.repo())
            .status()
            .map_err(|e| specere_core::Error::Install(format!("failed to invoke uvx: {e}")))?;
        if !status.success() {
            return Err(specere_core::Error::Install(format!(
                "upstream `specify init` exited with {:?}",
                status.code()
            )));
        }

        let mut record = Record::default();
        record.notes.push(format!(
            "scaffolded via uvx @ {PINNED_SPECKIT_TAG}; file-level tracking is SpecKit's responsibility at `.specify/integrations/integration.json`"
        ));
        Ok(record)
    }

    fn remove(&self, ctx: &Ctx, _record: &Record) -> Result<()> {
        // 1) Prefer SpecKit's own removal verb if it exists. This is the
        //    fine-grained path that respects any user edits inside `.specify/`.
        let integration_removed = try_upstream_integration_uninstall(ctx);

        // 2) For the `.specify/` and `specs/` directories SpecKit leaves
        //    behind, fall back to wholesale removal.
        let specify_dir = ctx.repo().join(".specify");
        let specs_dir = ctx.repo().join("specs");
        let claude_md = ctx.repo().join("CLAUDE.md");

        for path in [&specify_dir, &specs_dir] {
            if path.exists() {
                std::fs::remove_dir_all(path).map_err(|e| {
                    specere_core::Error::Remove(format!("wholesale remove {}: {e}", path.display()))
                })?;
            }
        }
        // Only remove CLAUDE.md if it was created by SpecKit (heuristic:
        // contains a SpecKit-generated marker). User-authored files stay.
        if claude_md.exists() {
            let content = std::fs::read_to_string(&claude_md).unwrap_or_default();
            if content.contains("spec-kit") || content.contains("/speckit.") {
                std::fs::remove_file(&claude_md)
                    .map_err(|e| specere_core::Error::Remove(format!("remove CLAUDE.md: {e}")))?;
            } else {
                tracing::warn!("CLAUDE.md present but does not look SpecKit-generated; preserving");
            }
        }
        tracing::info!(
            "speckit removed (upstream integration uninstall: {})",
            if integration_removed {
                "ok"
            } else {
                "n/a or failed — fell back to wipe"
            }
        );
        Ok(())
    }
}

fn uvx_init_args() -> Vec<String> {
    vec![
        "--from".into(),
        format!(
            "git+https://github.com/github/spec-kit.git@{}",
            PINNED_SPECKIT_TAG
        ),
        "specify".into(),
        "init".into(),
        ".".into(),
        "--integration".into(),
        DEFAULT_INTEGRATION.into(),
        "--force".into(),
        "--no-git".into(),
    ]
}

fn command_exists(program: &str) -> bool {
    match Command::new(program).arg("--version").output() {
        Ok(o) => o.status.success(),
        Err(_) => false,
    }
}

/// Attempt `specify integration uninstall <agent>` via `uvx`. Returns true on
/// success; false on any failure (missing binary, non-zero exit, …) so the
/// caller can fall back to directory wipe.
fn try_upstream_integration_uninstall(ctx: &Ctx) -> bool {
    let specify_dir: PathBuf = ctx.repo().join(".specify");
    if !specify_dir.exists() {
        return false;
    }
    let out = Command::new("uvx")
        .args([
            "--from",
            &format!(
                "git+https://github.com/github/spec-kit.git@{}",
                PINNED_SPECKIT_TAG
            ),
            "specify",
            "integration",
            "uninstall",
            DEFAULT_INTEGRATION,
        ])
        .current_dir(ctx.repo())
        .output();
    match out {
        Ok(o) => o.status.success(),
        Err(_) => false,
    }
}
