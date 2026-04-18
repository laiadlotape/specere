//! Claude Code deployer — installs SpecERE skills under `.claude/skills/` in
//! the target repo.

use std::path::PathBuf;

use specere_core::{AddUnit, Ctx, Plan, Record, Result};

use super::{Deploy, SkillBundle};

/// The adoption skill — translates an existing repo into SpecERE's SDD stack.
/// Embedded at compile time.
pub const SPECERE_ADOPT_SKILL: SkillBundle = SkillBundle {
    id: "specere-adopt",
    contents: include_str!("skills/specere-adopt.md"),
};

const ALL_SKILLS: &[SkillBundle] = &[SPECERE_ADOPT_SKILL];

/// The `claude-code-deploy` unit.
pub struct ClaudeCodeDeploy;

impl Deploy for ClaudeCodeDeploy {
    fn harness_id(&self) -> &'static str {
        "claude-code"
    }

    fn skills(&self) -> &'static [SkillBundle] {
        ALL_SKILLS
    }

    fn skill_dir(&self, ctx: &Ctx) -> PathBuf {
        ctx.repo().join(".claude").join("skills")
    }

    fn skill_rel_path(&self, skill_id: &str) -> PathBuf {
        PathBuf::from(".claude/skills")
            .join(skill_id)
            .join("SKILL.md")
    }
}

impl AddUnit for ClaudeCodeDeploy {
    fn id(&self) -> &'static str {
        "claude-code-deploy"
    }

    fn pinned_version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn preflight(&self, ctx: &Ctx) -> Result<Plan> {
        super::plan(self, ctx)
    }

    fn install(&self, ctx: &Ctx, plan: &Plan) -> Result<Record> {
        super::install(self, ctx, plan)
    }

    fn remove(&self, ctx: &Ctx, record: &Record) -> Result<()> {
        super::remove(self, ctx, record)
    }
}
