//! Deploy abstraction: a SpecERE *Deploy unit* installs SpecERE-owned
//! capabilities (skills, hooks, commands) into one specific coding-agent
//! harness. The trait below is the extension point; each harness gets a
//! concrete implementation.
//!
//! Current implementations:
//!
//! - [`claude_code::ClaudeCodeDeploy`] — `.claude/skills/` + `.claude/settings.json` hooks
//!
//! Planned (see `docs/roadmap/31_specere_scaffolding.md` §11.2):
//!
//! - `cursor_deploy::CursorDeploy` — `.cursor/commands/`
//! - `aider_deploy::AiderDeploy` — `CONVENTIONS.md` + `.aider.conf.yml`
//! - `opencode_deploy::OpenCodeDeploy` — OpenCode agent config

use std::path::{Path, PathBuf};

use specere_core::{Ctx, FileEntry, Owner, Plan, PlanOp, Record, Result};
use specere_manifest::{sha256_bytes, sha256_file};

pub mod claude_code;

/// A skill bundle SpecERE ships and deploys into agent harnesses.
#[derive(Debug, Clone, Copy)]
pub struct SkillBundle {
    pub id: &'static str,
    pub contents: &'static str,
}

/// The harness-specific contract. Every deployer describes *where* it puts
/// skills in the target repo; the generic install/remove logic below does the
/// rest.
pub trait Deploy {
    fn harness_id(&self) -> &'static str;

    fn skills(&self) -> &'static [SkillBundle];

    /// Absolute directory into which a given skill's `SKILL.md` is written.
    /// Convention: `<repo>/<harness-skill-dir>/<skill.id>/SKILL.md`.
    fn skill_dir(&self, ctx: &Ctx) -> PathBuf;

    /// Relative path of the skill file inside the target repo, given the
    /// skill's id. Used by manifest persistence.
    fn skill_rel_path(&self, skill_id: &str) -> PathBuf;
}

/// Generic `preflight` for any deployer.
pub fn plan<D: Deploy + ?Sized>(deployer: &D, ctx: &Ctx) -> Result<Plan> {
    let mut plan = Plan::default();
    plan.ops.push(PlanOp::CreateDir {
        path: deployer.skill_dir(ctx),
    });
    for skill in deployer.skills() {
        let rel = deployer.skill_rel_path(skill.id);
        plan.ops.push(PlanOp::WriteFile {
            path: rel,
            summary: format!("{} skill bundle", skill.id),
        });
    }
    Ok(plan)
}

/// Generic install. Writes every skill bundle under
/// `<skill_dir>/<skill.id>/SKILL.md` and records each entry in the returned
/// `Record` with `owner = Specere`.
pub fn install<D: Deploy + ?Sized>(deployer: &D, ctx: &Ctx, _plan: &Plan) -> Result<Record> {
    let mut record = Record::default();
    let skill_dir = deployer.skill_dir(ctx);
    std::fs::create_dir_all(&skill_dir).map_err(|e| {
        specere_core::Error::Install(format!("create {}: {e}", skill_dir.display()))
    })?;
    record.dirs.push(rel_to_repo(ctx.repo(), &skill_dir));

    for skill in deployer.skills() {
        let skill_subdir = skill_dir.join(skill.id);
        std::fs::create_dir_all(&skill_subdir).map_err(|e| {
            specere_core::Error::Install(format!("create {}: {e}", skill_subdir.display()))
        })?;
        record.dirs.push(rel_to_repo(ctx.repo(), &skill_subdir));

        let skill_file = skill_subdir.join("SKILL.md");
        std::fs::write(&skill_file, skill.contents).map_err(|e| {
            specere_core::Error::Install(format!("write {}: {e}", skill_file.display()))
        })?;

        let sha = sha256_bytes(skill.contents.as_bytes());
        record.files.push(FileEntry {
            path: rel_to_repo(ctx.repo(), &skill_file),
            sha256_post: sha,
            owner: Owner::Specere,
            role: format!("{}-skill-{}", deployer.harness_id(), skill.id),
        });
    }

    record.notes.push(format!(
        "{} deployer installed {} skill(s)",
        deployer.harness_id(),
        deployer.skills().len()
    ));
    Ok(record)
}

/// Generic remove. Walks the recorded `files` list, SHA-checks each entry, and
/// removes only the ones we installed and that haven't been user-edited.
pub fn remove<D: Deploy + ?Sized>(_deployer: &D, ctx: &Ctx, record: &Record) -> Result<()> {
    let mut preserved = 0usize;
    let mut removed = 0usize;
    for f in &record.files {
        let abs = ctx.repo().join(&f.path);
        if !abs.exists() {
            continue;
        }
        let actual = sha256_file(&abs)
            .map_err(|e| specere_core::Error::Remove(format!("sha256 {}: {e}", abs.display())))?;
        if actual != f.sha256_post {
            tracing::warn!(
                "file `{}` edited after install; preserving",
                f.path.display()
            );
            preserved += 1;
            continue;
        }
        std::fs::remove_file(&abs)
            .map_err(|e| specere_core::Error::Remove(format!("remove {}: {e}", abs.display())))?;
        removed += 1;
    }
    let mut dirs = record.dirs.clone();
    dirs.sort_by_key(|p| std::cmp::Reverse(p.components().count()));
    for d in dirs {
        let abs = ctx.repo().join(&d);
        if abs.exists() && is_dir_empty(&abs).unwrap_or(false) {
            let _ = std::fs::remove_dir(&abs);
        }
    }
    tracing::info!("deploy remove: {removed} file(s) removed, {preserved} preserved (user-edited)");
    Ok(())
}

fn rel_to_repo(repo: &Path, abs: &Path) -> PathBuf {
    abs.strip_prefix(repo)
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|_| abs.to_path_buf())
}

fn is_dir_empty(p: &Path) -> std::io::Result<bool> {
    Ok(std::fs::read_dir(p)?.next().is_none())
}
