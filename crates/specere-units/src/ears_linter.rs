//! `specere add ears-linter` — native unit that installs EARS-style lint
//! rules, the advisory `before_clarify` hook, and the `specere-lint-ears`
//! skill that executes the lint.
//!
//! Issue #14 / FR-P2-003. Advisory only — the hook registers as
//! `optional: true` so the lint never blocks a `/speckit-*` verb.

use std::path::PathBuf;

use specere_core::{AddUnit, Ctx, FileEntry, MarkerEntry, Owner, Plan, PlanOp, Record, Result};

const UNIT_ID: &str = "ears-linter";

const RULES_TOML: &str = include_str!("ears_linter/rules.toml");

const LINT_SKILL_CONTENTS: &str = include_str!("ears_linter/lint-ears-skill.md");

const BEFORE_CLARIFY_ENTRY: &str = concat!(
    "  - extension: specere\n",
    "    command: specere.lint.ears\n",
    "    enabled: true\n",
    "    optional: true\n",
    "    prompt: Run EARS-style lint over the active feature's spec.md? (advisory)\n",
    "    description: SpecERE EARS linter (FR-P2-003, advisory)\n",
    "    condition: null"
);

pub struct EarsLinter;

impl AddUnit for EarsLinter {
    fn id(&self) -> &'static str {
        UNIT_ID
    }

    fn pinned_version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn preflight(&self, _ctx: &Ctx) -> Result<Plan> {
        let mut plan = Plan::default();
        plan.ops.push(PlanOp::CreateDir {
            path: PathBuf::from(".specere/lint"),
        });
        plan.ops.push(PlanOp::WriteFile {
            path: PathBuf::from(".specere/lint/ears.toml"),
            summary: "EARS linter rules".into(),
        });
        plan.ops.push(PlanOp::CreateDir {
            path: PathBuf::from(".claude/skills/specere-lint-ears"),
        });
        plan.ops.push(PlanOp::WriteFile {
            path: PathBuf::from(".claude/skills/specere-lint-ears/SKILL.md"),
            summary: "specere-lint-ears skill".into(),
        });
        plan.ops.push(PlanOp::UpsertMarker {
            path: PathBuf::from(".specify/extensions.yml"),
            block_id: UNIT_ID.to_string(),
        });
        Ok(plan)
    }

    fn install(&self, ctx: &Ctx, _plan: &Plan) -> Result<Record> {
        let mut record = Record::default();

        // 1. Rules file under .specere/lint/
        let lint_dir = ctx.repo().join(".specere/lint");
        std::fs::create_dir_all(&lint_dir)
            .map_err(|e| specere_core::Error::Install(format!("create .specere/lint/: {e}")))?;
        record.dirs.push(PathBuf::from(".specere/lint"));

        let rules_path = lint_dir.join("ears.toml");
        std::fs::write(&rules_path, RULES_TOML)
            .map_err(|e| specere_core::Error::Install(format!("write rules: {e}")))?;
        record.files.push(FileEntry {
            path: PathBuf::from(".specere/lint/ears.toml"),
            sha256_post: specere_manifest::sha256_bytes(RULES_TOML.as_bytes()),
            owner: Owner::Specere,
            role: "ears-linter-rules".into(),
        });

        // 2. Skill file under .claude/skills/specere-lint-ears/
        let skill_dir = ctx.repo().join(".claude/skills/specere-lint-ears");
        std::fs::create_dir_all(&skill_dir)
            .map_err(|e| specere_core::Error::Install(format!("create skill dir: {e}")))?;
        record
            .dirs
            .push(PathBuf::from(".claude/skills/specere-lint-ears"));

        let skill_path = skill_dir.join("SKILL.md");
        std::fs::write(&skill_path, LINT_SKILL_CONTENTS)
            .map_err(|e| specere_core::Error::Install(format!("write skill: {e}")))?;
        record.files.push(FileEntry {
            path: PathBuf::from(".claude/skills/specere-lint-ears/SKILL.md"),
            sha256_post: specere_manifest::sha256_bytes(LINT_SKILL_CONTENTS.as_bytes()),
            owner: Owner::Specere,
            role: "ears-linter-skill".into(),
        });

        // 3. before_clarify hook in .specify/extensions.yml (advisory).
        let ext_path = ctx.repo().join(".specify/extensions.yml");
        if let Some(parent) = ext_path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| specere_core::Error::Install(format!("create .specify/: {e}")))?;
            }
        }
        let existing_yml = std::fs::read_to_string(&ext_path).unwrap_or_else(|_| {
            "installed: []\nsettings:\n  auto_execute_hooks: true\nhooks:\n".to_string()
        });
        if !existing_yml.is_empty() {
            if let Err(inner) = specere_markers::yaml_block_fence::is_valid_yaml(&existing_yml) {
                return Err(specere_core::Error::ParseFailure {
                    path: PathBuf::from(".specify/extensions.yml"),
                    format: "yaml",
                    inner,
                });
            }
        }
        let new_yml = specere_markers::yaml_block_fence::add(
            &existing_yml,
            UNIT_ID,
            "before_clarify",
            BEFORE_CLARIFY_ENTRY,
        )
        .map_err(|e| specere_core::Error::Install(format!("extensions.yml fence: {e}")))?;
        std::fs::write(&ext_path, &new_yml)
            .map_err(|e| specere_core::Error::Install(format!("write extensions.yml: {e}")))?;
        record.markers.push(MarkerEntry {
            path: PathBuf::from(".specify/extensions.yml"),
            unit_id: UNIT_ID.to_string(),
            block_id: Some("before_clarify".to_string()),
            sha256: specere_manifest::sha256_bytes(new_yml.as_bytes()),
        });

        record.notes.push(format!(
            "ears-linter installed ({} rule(s); advisory hook registered)",
            count_rules(RULES_TOML)
        ));
        Ok(record)
    }

    fn remove(&self, ctx: &Ctx, record: &Record) -> Result<()> {
        // 1. Strip the before_clarify hook block.
        let ext_path = ctx.repo().join(".specify/extensions.yml");
        if ext_path.exists() {
            let text = std::fs::read_to_string(&ext_path)
                .map_err(|e| specere_core::Error::Remove(format!("read extensions.yml: {e}")))?;
            if let Err(inner) = specere_markers::yaml_block_fence::is_valid_yaml(&text) {
                return Err(specere_core::Error::ParseFailure {
                    path: PathBuf::from(".specify/extensions.yml"),
                    format: "yaml",
                    inner,
                });
            }
            let stripped = specere_markers::yaml_block_fence::remove(&text, UNIT_ID)
                .map_err(|e| specere_core::Error::Remove(format!("extensions.yml strip: {e}")))?;
            std::fs::write(&ext_path, stripped)
                .map_err(|e| specere_core::Error::Remove(format!("write extensions.yml: {e}")))?;
        }

        // 2. Delete recorded skill + rules files (SHA-checked).
        for f in &record.files {
            let abs = ctx.repo().join(&f.path);
            if !abs.exists() {
                continue;
            }
            if f.owner == Owner::UserEditedAfterInstall {
                continue;
            }
            let actual = specere_manifest::sha256_file(&abs).map_err(|e| {
                specere_core::Error::Remove(format!("sha256 {}: {e}", abs.display()))
            })?;
            if actual != f.sha256_post {
                tracing::warn!("ears-linter: `{}` edited; preserving", f.path.display());
                continue;
            }
            std::fs::remove_file(&abs).map_err(|e| {
                specere_core::Error::Remove(format!("remove {}: {e}", abs.display()))
            })?;
        }

        // 3. GC empty dirs we created.
        for rel in [".specere/lint", ".claude/skills/specere-lint-ears"] {
            let abs = ctx.repo().join(rel);
            if abs.is_dir() {
                if let Ok(mut it) = std::fs::read_dir(&abs) {
                    if it.next().is_none() {
                        let _ = std::fs::remove_dir(&abs);
                    }
                }
            }
        }

        Ok(())
    }
}

fn count_rules(toml: &str) -> usize {
    toml.matches("\n[[rules]]").count()
}
