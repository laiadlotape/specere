//! Claude Code deployer — installs SpecERE skills under `.claude/skills/`,
//! gitignores Claude-local settings, and registers the `after_implement`
//! hook in `.specify/extensions.yml` (FR-P1-004/005).

use std::path::PathBuf;

use specere_core::{AddUnit, Ctx, MarkerEntry, Plan, Record, Result};

use super::{AgentBundle, Deploy, SkillBundle};

/// The adoption skill — translates an existing repo into SpecERE's SDD stack.
pub const SPECERE_ADOPT_SKILL: SkillBundle = SkillBundle {
    id: "specere-adopt",
    contents: include_str!("skills/specere-adopt.md"),
};

/// After-implement observer skill (fires from the `after_implement` hook).
pub const SPECERE_OBSERVE_IMPLEMENT_SKILL: SkillBundle = SkillBundle {
    id: "specere-observe-implement",
    contents: include_str!("skills/specere-observe-implement.md"),
};

/// Self-extension detector (fires from the `after_analyze` hook).
pub const SPECERE_REVIEW_CHECK_SKILL: SkillBundle = SkillBundle {
    id: "specere-review-check",
    contents: include_str!("skills/specere-review-check.md"),
};

/// Interactive drain of `.specere/review-queue.md`.
pub const SPECERE_REVIEW_DRAIN_SKILL: SkillBundle = SkillBundle {
    id: "specere-review-drain",
    contents: include_str!("skills/specere-review-drain.md"),
};

const ALL_SKILLS: &[SkillBundle] = &[
    SPECERE_ADOPT_SKILL,
    SPECERE_OBSERVE_IMPLEMENT_SKILL,
    SPECERE_REVIEW_CHECK_SKILL,
    SPECERE_REVIEW_DRAIN_SKILL,
];

/// First SpecERE-owned subagent — constitution-compliant PR / diff review.
/// Issue #7.
pub const SPECERE_REVIEWER_AGENT: AgentBundle = AgentBundle {
    id: "specere-reviewer",
    contents: include_str!("agents/specere-reviewer.md"),
};

const ALL_AGENTS: &[AgentBundle] = &[SPECERE_REVIEWER_AGENT];

/// The session-durable rules text for the CLAUDE.md `rules` marker-fenced
/// block. Issue #8. Sourced from a single file to avoid duplication with the
/// constitution.
const SPECERE_RULES_BODY: &str = include_str!("rules/specere-rules.md");

/// Unit id used for marker-fence blocks we own.
const UNIT_ID: &str = "claude-code-deploy";

const GITIGNORE_LINES: &[&str] = &[".claude/settings.local.json"];

/// The `after_implement` hook entry, exactly as contracts/extensions-mutation.md §Marker convention.
const AFTER_IMPLEMENT_ENTRY: &str = concat!(
    "  - extension: specere\n",
    "    command: specere.observe.implement\n",
    "    enabled: true\n",
    "    optional: false\n",
    "    prompt: Record Repo-SLAM observation from the just-completed implement run?\n",
    "    description: SpecERE telemetry + post-implement filter step (FR-P1-005)\n",
    "    condition: null"
);

/// The `claude-code-deploy` unit.
pub struct ClaudeCodeDeploy;

impl Deploy for ClaudeCodeDeploy {
    fn harness_id(&self) -> &'static str {
        "claude-code"
    }

    fn skills(&self) -> &'static [SkillBundle] {
        ALL_SKILLS
    }

    fn agents(&self) -> &'static [AgentBundle] {
        ALL_AGENTS
    }

    fn skill_dir(&self, ctx: &Ctx) -> PathBuf {
        ctx.repo().join(".claude").join("skills")
    }

    fn skill_rel_path(&self, skill_id: &str) -> PathBuf {
        PathBuf::from(".claude/skills")
            .join(skill_id)
            .join("SKILL.md")
    }

    fn agent_dir(&self, ctx: &Ctx) -> PathBuf {
        ctx.repo().join(".claude").join("agents")
    }

    fn agent_rel_path(&self, agent_id: &str) -> PathBuf {
        PathBuf::from(".claude/agents").join(format!("{agent_id}.md"))
    }
}

impl AddUnit for ClaudeCodeDeploy {
    fn id(&self) -> &'static str {
        UNIT_ID
    }

    fn pinned_version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn preflight(&self, ctx: &Ctx) -> Result<Plan> {
        super::plan(self, ctx)
    }

    fn install(&self, ctx: &Ctx, plan: &Plan) -> Result<Record> {
        // 1. Deploy the skill bundles (inherited generic path).
        let mut record = super::install(self, ctx, plan)?;

        // 2. FR-P1-004: gitignore the Claude Code local settings file,
        //    marker-fenced.
        let gitignore_path = ctx.repo().join(".gitignore");
        let existing = std::fs::read_to_string(&gitignore_path).unwrap_or_default();
        // FR-P1-008 guard: no parse check needed for plain text, but still
        // validate UTF-8 (std::fs::read_to_string already enforces this).
        let new_ignore =
            specere_markers::text_block_fence::add(&existing, UNIT_ID, GITIGNORE_LINES).map_err(
                |e| specere_core::Error::Install(format!("gitignore fence insert: {e}")),
            )?;
        std::fs::write(&gitignore_path, &new_ignore)
            .map_err(|e| specere_core::Error::Install(format!("write .gitignore: {e}")))?;
        record.markers.push(MarkerEntry {
            path: PathBuf::from(".gitignore"),
            unit_id: UNIT_ID.to_string(),
            block_id: None,
            sha256: specere_manifest::sha256_bytes(new_ignore.as_bytes()),
        });

        // 3. FR-P1-005: register after_implement hook in extensions.yml.
        let ext_path = ctx.repo().join(".specify").join("extensions.yml");
        // Ensure the parent dir exists.
        if let Some(parent) = ext_path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| specere_core::Error::Install(format!("create .specify/: {e}")))?;
            }
        }
        let existing_yml = std::fs::read_to_string(&ext_path).unwrap_or_else(|_| {
            // Minimal bootstrap if the file is missing (SpecKit would normally have created it).
            "installed: []\nsettings:\n  auto_execute_hooks: true\nhooks:\n".to_string()
        });
        // FR-P1-008 guard: if the existing file is corrupt YAML, refuse.
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
            "after_implement",
            AFTER_IMPLEMENT_ENTRY,
        )
        .map_err(|e| specere_core::Error::Install(format!("extensions.yml fence insert: {e}")))?;
        std::fs::write(&ext_path, &new_yml).map_err(|e| {
            specere_core::Error::Install(format!("write .specify/extensions.yml: {e}"))
        })?;
        record.markers.push(MarkerEntry {
            path: PathBuf::from(".specify/extensions.yml"),
            unit_id: UNIT_ID.to_string(),
            block_id: Some("after_implement".to_string()),
            sha256: specere_manifest::sha256_bytes(new_yml.as_bytes()),
        });

        // 4. Whole-file FileEntry records intentionally omitted for .gitignore
        //    and .specify/extensions.yml — both are multi-owner files (other
        //    units add their own fenced blocks), so a whole-file SHA on our
        //    record would drift and spuriously trip FR-P1-003's gate on the
        //    next `specere init` idempotent pass. The MarkerEntry records
        //    above are authoritative for our owned content.

        // 5. Issue #8: embed the session-durable rules block in CLAUDE.md via
        //    a second marker-fenced section, disjoint from the existing
        //    `harness` block the scaffold already writes there.
        let claude_md = ctx.repo().join("CLAUDE.md");
        let existing_cm = std::fs::read_to_string(&claude_md).unwrap_or_default();
        let new_cm = specere_markers::upsert_block(
            &existing_cm,
            "rules",
            None,
            SPECERE_RULES_BODY.trim_end_matches('\n'),
        )
        .map_err(|e| specere_core::Error::Install(format!("CLAUDE.md rules fence: {e}")))?;
        std::fs::write(&claude_md, &new_cm)
            .map_err(|e| specere_core::Error::Install(format!("write CLAUDE.md: {e}")))?;
        record.markers.push(MarkerEntry {
            path: PathBuf::from("CLAUDE.md"),
            unit_id: "rules".to_string(),
            block_id: None,
            sha256: specere_manifest::sha256_bytes(new_cm.as_bytes()),
        });

        Ok(record)
    }

    fn remove(&self, ctx: &Ctx, record: &Record) -> Result<()> {
        // 1. Strip .gitignore fenced block.
        let gi_path = ctx.repo().join(".gitignore");
        if gi_path.exists() {
            let text = std::fs::read_to_string(&gi_path)
                .map_err(|e| specere_core::Error::Remove(format!("read .gitignore: {e}")))?;
            let stripped = specere_markers::text_block_fence::remove(&text, UNIT_ID)
                .map_err(|e| specere_core::Error::Remove(format!("gitignore strip: {e}")))?;
            if stripped.is_empty() {
                // File is now empty of content → remove it entirely (matches pre-install state).
                let _ = std::fs::remove_file(&gi_path);
            } else {
                std::fs::write(&gi_path, stripped)
                    .map_err(|e| specere_core::Error::Remove(format!("write .gitignore: {e}")))?;
            }
        }

        // 2a. Strip CLAUDE.md rules fenced block (issue #8).
        let claude_md = ctx.repo().join("CLAUDE.md");
        if claude_md.exists() {
            let text = std::fs::read_to_string(&claude_md)
                .map_err(|e| specere_core::Error::Remove(format!("read CLAUDE.md: {e}")))?;
            let stripped = specere_markers::strip_block(&text, "rules", None)
                .map_err(|e| specere_core::Error::Remove(format!("CLAUDE.md rules strip: {e}")))?;
            if stripped.is_empty() {
                let _ = std::fs::remove_file(&claude_md);
            } else {
                std::fs::write(&claude_md, stripped)
                    .map_err(|e| specere_core::Error::Remove(format!("write CLAUDE.md: {e}")))?;
            }
        }

        // 2. Strip extensions.yml fenced block.
        let ext_path = ctx.repo().join(".specify").join("extensions.yml");
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

        // 3. Remove the skill files via the generic deploy::remove, but skip
        //    any files we know we didn't add as skill files (gitignore,
        //    extensions.yml — they're marker-stripped above, not deleted).
        let mut skill_record = record.clone();
        skill_record.files.retain(|f| {
            !matches!(
                f.path.as_os_str().to_str(),
                Some(".gitignore") | Some(".specify/extensions.yml")
            )
        });
        super::remove(self, ctx, &skill_record)
    }
}
