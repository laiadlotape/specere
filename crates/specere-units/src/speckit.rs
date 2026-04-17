//! `specere add speckit` — thin wrapper over upstream `github/spec-kit`.
//!
//! Design decisions (see `docs/roadmap/31_specere_scaffolding.md`):
//!
//! * We pin a SpecKit release tag per SpecERE release. Never fork its output.
//! * We scaffold via `uvx --from git+https://github.com/github/spec-kit.git@<tag> specify init . --integration <agent> --force`.
//! * We record every file SpecKit created into the SpecERE manifest so `remove` can
//!   do an exact inverse, distinguishing "installed-by-us, unchanged" from
//!   "user-edited".
//! * Shared files (`CLAUDE.md`, `AGENTS.md`, …) are wrapped in marker-fenced
//!   SpecERE blocks so we never mutate user content outside our namespace.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use specere_core::{AddUnit, Ctx, FileEntry, Owner, Plan, PlanOp, Record, Result};
use specere_manifest::sha256_file;
use walkdir::WalkDir;

/// Pinned SpecKit upstream tag. Bump in lockstep with our CHANGELOG.
pub const PINNED_SPECKIT_TAG: &str = "v0.7.3";

/// Default agent integration. Can be overridden via `specere add speckit -- --integration=cursor` (flags plumbing lands in 0.1.1).
pub const DEFAULT_INTEGRATION: &str = "claude";

#[derive(Default)]
pub struct Speckit;

impl AddUnit for Speckit {
    fn id(&self) -> &'static str {
        "speckit"
    }

    fn pinned_version(&self) -> &'static str {
        PINNED_SPECKIT_TAG
    }

    fn preflight(&self, ctx: &Ctx) -> Result<Plan> {
        let mut plan = Plan::default();
        let uvx_available = command_exists("uvx");
        if !uvx_available {
            return Err(specere_core::Error::Preflight(
                "`uvx` not found in PATH; install `uv` (https://github.com/astral-sh/uv) to use `specere add speckit`".into(),
            ));
        }
        plan.ops.push(PlanOp::RunCommand {
            program: "uvx".into(),
            args: vec![
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
            ],
        });
        plan.ops.push(PlanOp::WriteFile {
            path: ctx.manifest_path(),
            summary: "record what SpecKit installed".into(),
        });
        Ok(plan)
    }

    fn install(&self, ctx: &Ctx, _plan: &Plan) -> Result<Record> {
        let snapshot_before = snapshot_tree(ctx.repo());
        let status = Command::new("uvx")
            .args([
                "--from",
                &format!(
                    "git+https://github.com/github/spec-kit.git@{}",
                    PINNED_SPECKIT_TAG
                ),
                "specify",
                "init",
                ".",
                "--integration",
                DEFAULT_INTEGRATION,
                "--force",
                "--no-git",
            ])
            .current_dir(ctx.repo())
            .status()
            .map_err(|e| specere_core::Error::Install(format!("failed to invoke uvx: {e}")))?;
        if !status.success() {
            return Err(specere_core::Error::Install(format!(
                "upstream `specify init` exited with {:?}",
                status.code()
            )));
        }

        let snapshot_after = snapshot_tree(ctx.repo());
        let new_paths: Vec<PathBuf> = snapshot_after
            .difference(&snapshot_before)
            .cloned()
            .collect();

        let mut record = Record::default();
        for path in new_paths {
            let abs = ctx.repo().join(&path);
            if abs.is_dir() {
                record.dirs.push(path);
                continue;
            }
            let sha = sha256_file(&abs).map_err(|e| {
                specere_core::Error::Install(format!("sha256 {}: {e}", abs.display()))
            })?;
            record.files.push(FileEntry {
                path,
                sha256_post: sha,
                owner: Owner::Upstream,
                role: "speckit-scaffold".into(),
            });
        }
        record
            .notes
            .push(format!("scaffolded via uvx @ {}", PINNED_SPECKIT_TAG));
        Ok(record)
    }

    fn remove(&self, ctx: &Ctx, record: &Record) -> Result<()> {
        let mut preserved = 0usize;
        let mut removed_files = 0usize;
        for f in &record.files {
            let abs = ctx.repo().join(&f.path);
            if !abs.exists() {
                continue;
            }
            let actual = sha256_file(&abs).map_err(|e| {
                specere_core::Error::Remove(format!("sha256 {}: {e}", abs.display()))
            })?;
            if actual != f.sha256_post {
                tracing::warn!(
                    "file `{}` was edited after install; preserving (use `remove --force` to delete)",
                    f.path.display()
                );
                preserved += 1;
                continue;
            }
            std::fs::remove_file(&abs).map_err(|e| {
                specere_core::Error::Remove(format!("remove {}: {e}", abs.display()))
            })?;
            removed_files += 1;
        }
        let mut dirs = record.dirs.clone();
        dirs.sort_by_key(|p| std::cmp::Reverse(p.components().count()));
        for d in dirs {
            let abs = ctx.repo().join(&d);
            if abs.exists() && is_dir_empty(&abs).unwrap_or(false) {
                let _ = std::fs::remove_dir(&abs);
            }
        }
        tracing::info!(
            "speckit remove: {removed_files} file(s) removed, {preserved} preserved (user-edited)"
        );
        Ok(())
    }
}

fn command_exists(program: &str) -> bool {
    match Command::new(program).arg("--version").output() {
        Ok(o) => o.status.success(),
        Err(_) => false,
    }
}

/// Walk the repo producing the set of relative paths (files + dirs), skipping
/// `.git` and `target/`. Used to diff the tree before and after upstream scaffolding.
fn snapshot_tree(root: &Path) -> HashSet<PathBuf> {
    let mut out = HashSet::new();
    let skip: [&str; 3] = [".git", "target", "node_modules"];
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            if e.depth() == 0 {
                return true;
            }
            !skip.iter().any(|s| name == *s)
        })
    {
        let Ok(entry) = entry else { continue };
        if let Ok(rel) = entry.path().strip_prefix(root) {
            out.insert(rel.to_path_buf());
        }
    }
    out.remove(Path::new(""));
    out
}

fn is_dir_empty(p: &Path) -> std::io::Result<bool> {
    Ok(std::fs::read_dir(p)?.next().is_none())
}
