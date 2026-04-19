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
//! Phase 1 additions (FR-P1-001/002/007):
//! * Detects ambient git-kind; drops `--no-git` iff `.git/` exists.
//! * Auto-creates a feature branch (`000-baseline` default, overridable via
//!   `--branch` CLI flag or `$SPECERE_FEATURE_BRANCH` env var).
//! * Records the resulting branch name + whether SpecERE created it in
//!   `install_config` so `remove --delete-branch` can read both fields.
//!
//! Test mode: setting `SPECERE_TEST_SKIP_UVX=1` bypasses the actual `uvx`
//! subprocess so integration tests can exercise branch logic offline.

use std::path::PathBuf;
use std::process::Command;

use specere_core::{AddUnit, Ctx, Plan, PlanOp, Record, Result};

/// Pinned SpecKit upstream tag. Bump in lockstep with our CHANGELOG.
pub const PINNED_SPECKIT_TAG: &str = "v0.7.3";

/// Default agent integration. Overridable via `specere add speckit -- --integration=<agent>` (flag plumbing in a later release).
pub const DEFAULT_INTEGRATION: &str = "claude";

/// Default feature-branch name created on a git target.
pub const DEFAULT_FEATURE_BRANCH: &str = "000-baseline";

/// Env var override for the auto-created feature branch name.
pub const ENV_FEATURE_BRANCH: &str = "SPECERE_FEATURE_BRANCH";

/// Test-only: when set to `1`, `install` skips the `uvx specify init`
/// subprocess call. The branch-create path still runs so integration tests
/// can exercise it offline.
pub const ENV_TEST_SKIP_UVX: &str = "SPECERE_TEST_SKIP_UVX";

#[derive(Debug, Default, Clone)]
pub struct SpeckitFlags {
    /// CLI `--branch <name>` override; wins over env var when set.
    pub branch: Option<String>,
}

pub struct Speckit {
    pub flags: SpeckitFlags,
}

impl Speckit {
    pub fn new() -> Self {
        Self {
            flags: SpeckitFlags::default(),
        }
    }

    pub fn with_flags(flags: SpeckitFlags) -> Self {
        Self { flags }
    }

    /// Resolve the target branch name per priority: CLI flag → env var →
    /// default. Returns `None` for non-git targets (caller skips branch ops).
    fn resolved_branch(&self, ctx: &Ctx) -> Option<String> {
        if !is_git_repo(ctx) {
            return None;
        }
        if let Some(b) = self.flags.branch.clone() {
            return Some(b);
        }
        if let Ok(b) = std::env::var(ENV_FEATURE_BRANCH) {
            if !b.is_empty() {
                return Some(b);
            }
        }
        Some(DEFAULT_FEATURE_BRANCH.to_string())
    }
}

impl Default for Speckit {
    fn default() -> Self {
        Self::new()
    }
}

impl AddUnit for Speckit {
    fn id(&self) -> &'static str {
        "speckit"
    }

    fn pinned_version(&self) -> &'static str {
        PINNED_SPECKIT_TAG
    }

    fn preflight(&self, ctx: &Ctx) -> Result<Plan> {
        // Issue #16: refuse on orphan .specify/ state before any other work.
        if let Some(state) = crate::orphan::detect(ctx.repo()) {
            return Err(specere_core::Error::OrphanFeatureDir {
                feature_dir: state
                    .feature_dir
                    .strip_prefix(ctx.repo())
                    .map(|p| p.to_path_buf())
                    .unwrap_or(state.feature_dir),
            });
        }

        let mut plan = Plan::default();
        if !test_skip_uvx() && !command_exists("uvx") && !command_exists("specify") {
            return Err(specere_core::Error::Preflight(
                "`uvx` (or an installed `specify`) not found in PATH; install `uv` \
                 (https://github.com/astral-sh/uv) or run `uv tool install \
                 git+https://github.com/github/spec-kit` to use `specere add speckit`"
                    .into(),
            ));
        }
        plan.ops.push(PlanOp::RunCommand {
            program: "uvx".into(),
            args: uvx_init_args(is_git_repo(ctx)),
        });
        if let Some(branch) = self.resolved_branch(ctx) {
            plan.ops.push(PlanOp::RunCommand {
                program: "git".into(),
                args: vec!["checkout".into(), "-b".into(), branch],
            });
        }
        Ok(plan)
    }

    fn install(&self, ctx: &Ctx, _plan: &Plan) -> Result<Record> {
        let is_git = is_git_repo(ctx);
        let branch = self.resolved_branch(ctx);

        // 1. Run `uvx specify init` unless the test env bypasses it.
        if !test_skip_uvx() {
            let status = Command::new("uvx")
                .args(uvx_init_args(is_git))
                .current_dir(ctx.repo())
                .status()
                .map_err(|e| specere_core::Error::Install(format!("failed to invoke uvx: {e}")))?;
            if !status.success() {
                return Err(specere_core::Error::Install(format!(
                    "upstream `specify init` exited with {:?}",
                    status.code()
                )));
            }
        }

        // 2. Create or switch to the feature branch on git targets.
        let mut branch_was_created_by_specere = false;
        if let Some(b) = branch.as_deref() {
            if branch_exists(ctx, b) {
                git_checkout(ctx, b)?;
            } else {
                git_checkout_new(ctx, b)?;
                branch_was_created_by_specere = true;
            }
        }

        let mut record = Record::default();
        record.notes.push(format!(
            "scaffolded via uvx @ {PINNED_SPECKIT_TAG}; file-level tracking is SpecKit's responsibility at `.specify/integrations/integration.json`"
        ));

        // 3. Stash branch info on the Record so the dispatcher can push it
        //    into install_config before saving the manifest.
        if let Some(b) = branch {
            record.notes.push(format!("branch_name={b}"));
            record.notes.push(format!(
                "branch_was_created_by_specere={branch_was_created_by_specere}"
            ));
        }

        Ok(record)
    }

    fn remove(&self, ctx: &Ctx, _record: &Record) -> Result<()> {
        // 1) Prefer SpecKit's own removal verb if it exists.
        let integration_removed = try_upstream_integration_uninstall(ctx);

        // 2) Fall back to wholesale removal of the directories SpecKit leaves.
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
        if claude_md.exists() {
            let content = std::fs::read_to_string(&claude_md).unwrap_or_default();
            if content.contains("spec-kit") || content.contains("/speckit.") {
                std::fs::remove_file(&claude_md)
                    .map_err(|e| specere_core::Error::Remove(format!("remove CLAUDE.md: {e}")))?;
            } else {
                tracing::warn!("CLAUDE.md present but does not look SpecKit-generated; preserving");
            }
        }

        // 3) Sweep orphan skill directories the upstream SpecKit integration
        //    drops but doesn't enumerate on uninstall. Issue #64 — observed:
        //    `speckit-git-{commit,feature,initialize,remote,validate}`
        //    remain after `specere remove speckit` because speckit is a
        //    wrapper unit with "0 files, 0 markers" in our manifest, and
        //    the upstream `specify integration uninstall` hook can miss them.
        //
        //    Limited to `speckit-git-*` so we DON'T touch the other
        //    `speckit-*` skills that `claude-code-deploy` installs (`speckit-plan`,
        //    `speckit-implement`, etc.) — those are that unit's responsibility
        //    and have their own manifest tracking.
        let claude_skills = ctx.repo().join(".claude").join("skills");
        if claude_skills.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&claude_skills) {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    let name_str = name.to_string_lossy();
                    if name_str.starts_with("speckit-git-") {
                        let path = entry.path();
                        if path.is_dir() {
                            let _ = std::fs::remove_dir_all(&path);
                        } else {
                            let _ = std::fs::remove_file(&path);
                        }
                    }
                }
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

fn uvx_init_args(is_git: bool) -> Vec<String> {
    let mut args = vec![
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
    ];
    // FR-P1-001: only pass `--no-git` on non-git targets.
    if !is_git {
        args.push("--no-git".into());
    }
    args
}

fn is_git_repo(ctx: &Ctx) -> bool {
    ctx.repo().join(".git").exists()
}

fn branch_exists(ctx: &Ctx, branch: &str) -> bool {
    Command::new("git")
        .args(["rev-parse", "--verify", "--quiet", branch])
        .current_dir(ctx.repo())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn git_checkout(ctx: &Ctx, branch: &str) -> Result<()> {
    let status = Command::new("git")
        .args(["checkout", branch])
        .current_dir(ctx.repo())
        .status()
        .map_err(|e| specere_core::Error::Install(format!("git checkout failed: {e}")))?;
    if !status.success() {
        return Err(specere_core::Error::Install(format!(
            "git checkout {branch} exited non-zero"
        )));
    }
    Ok(())
}

fn git_checkout_new(ctx: &Ctx, branch: &str) -> Result<()> {
    let status = Command::new("git")
        .args(["checkout", "-b", branch])
        .current_dir(ctx.repo())
        .status()
        .map_err(|e| specere_core::Error::Install(format!("git checkout -b failed: {e}")))?;
    if !status.success() {
        return Err(specere_core::Error::Install(format!(
            "git checkout -b {branch} exited non-zero"
        )));
    }
    Ok(())
}

fn test_skip_uvx() -> bool {
    matches!(std::env::var(ENV_TEST_SKIP_UVX).as_deref(), Ok("1"))
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
