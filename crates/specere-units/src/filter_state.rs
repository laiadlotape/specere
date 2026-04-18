//! `specere add filter-state` — native unit that lays down the
//! `.specere/` skeleton and the `.gitignore` allowlist that lets the filter
//! engine (Phase 4) and observe pipeline (Phase 3) write runtime state
//! without leaking it into git.
//!
//! Issue #12 / FR-P2-001.

use std::path::PathBuf;

use specere_core::{AddUnit, Ctx, FileEntry, MarkerEntry, Owner, Plan, PlanOp, Record, Result};

const UNIT_ID: &str = "filter-state";

const EVENTS_SQLITE_CONTENT: &[u8] = b"";

const POSTERIOR_TOML_CONTENT: &str = concat!(
    "# SpecERE filter posterior. Phase 4 populates per-spec entries below.\n",
    "schema_version = 1\n",
);

const SENSOR_MAP_TOML_CONTENT: &str = concat!(
    "# SpecERE sensor map — Repo-SLAM sensor channel registry.\n",
    "#\n",
    "# SpecERE-native (10-rule #5). Populated by `specere-adopt` or by hand.\n",
    "#\n",
    "# Channels (see docs/analysis/core_theory.md §3 in ReSearch):\n",
    "#   A = test / contract measurements\n",
    "#   B = read-tool observations\n",
    "#   C = harness-intrinsic signals\n",
    "#   D = invariants / PBT / mutation\n",
    "\n",
    "schema_version = 1\n",
    "\n",
    "[channels]\n",
    "# (empty — populated per-spec as the sensor array grows)\n",
);

const GITIGNORE_BODY_LINES: &[&str] = &[
    ".specere/*",
    "!.specere/manifest.toml",
    "!.specere/sensor-map.toml",
    "!.specere/review-queue.md",
    "!.specere/decisions.log",
    "!.specere/posterior.toml",
];

pub struct FilterState;

impl AddUnit for FilterState {
    fn id(&self) -> &'static str {
        UNIT_ID
    }

    fn pinned_version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn preflight(&self, _ctx: &Ctx) -> Result<Plan> {
        let mut plan = Plan::default();
        plan.ops.push(PlanOp::CreateDir {
            path: PathBuf::from(".specere"),
        });
        for (name, _) in skeleton_files() {
            plan.ops.push(PlanOp::WriteFile {
                path: PathBuf::from(".specere").join(name),
                summary: format!("filter-state skeleton: {name}"),
            });
        }
        plan.ops.push(PlanOp::UpsertMarker {
            path: PathBuf::from(".gitignore"),
            block_id: UNIT_ID.to_string(),
        });
        Ok(plan)
    }

    fn install(&self, ctx: &Ctx, _plan: &Plan) -> Result<Record> {
        let specere_dir = ctx.repo().join(".specere");
        std::fs::create_dir_all(&specere_dir).map_err(|e| {
            specere_core::Error::Install(format!("create {}: {e}", specere_dir.display()))
        })?;

        let mut record = Record::default();
        record.dirs.push(PathBuf::from(".specere"));

        for (name, content) in skeleton_files() {
            let abs = specere_dir.join(name);
            // Only write if absent. If the target already has this file (e.g.
            // from the session harness on specere's own repo), adopt the
            // existing content and record its SHA.
            let wrote = if !abs.exists() {
                std::fs::write(&abs, content).map_err(|e| {
                    specere_core::Error::Install(format!("write {}: {e}", abs.display()))
                })?;
                true
            } else {
                false
            };
            let on_disk = std::fs::read(&abs).map_err(|e| {
                specere_core::Error::Install(format!("re-read {}: {e}", abs.display()))
            })?;
            let sha = specere_manifest::sha256_bytes(&on_disk);
            record.files.push(FileEntry {
                path: PathBuf::from(".specere").join(name),
                sha256_post: sha,
                owner: if wrote {
                    Owner::Specere
                } else {
                    Owner::UserEditedAfterInstall
                },
                role: format!(
                    "filter-state-{}",
                    name.trim_end_matches(".toml").trim_end_matches(".sqlite")
                ),
            });
        }

        // .gitignore marker block. We record only a MarkerEntry (not a
        // whole-file FileEntry) because .gitignore is a multi-owner file —
        // other units (claude-code-deploy, etc) may write their own fenced
        // blocks, and a whole-file SHA on this record would drift and trip
        // FR-P1-003's SHA-diff gate on every re-install.
        let gi_path = ctx.repo().join(".gitignore");
        let existing = std::fs::read_to_string(&gi_path).unwrap_or_default();
        let new_ign =
            specere_markers::text_block_fence::add(&existing, UNIT_ID, GITIGNORE_BODY_LINES)
                .map_err(|e| specere_core::Error::Install(format!("gitignore fence: {e}")))?;
        std::fs::write(&gi_path, &new_ign)
            .map_err(|e| specere_core::Error::Install(format!("write .gitignore: {e}")))?;
        record.markers.push(MarkerEntry {
            path: PathBuf::from(".gitignore"),
            unit_id: UNIT_ID.to_string(),
            block_id: None,
            sha256: specere_manifest::sha256_bytes(new_ign.as_bytes()),
        });

        record.notes.push(format!(
            "filter-state installed skeleton ({} file(s))",
            skeleton_files().len()
        ));
        Ok(record)
    }

    fn remove(&self, ctx: &Ctx, record: &Record) -> Result<()> {
        // 1) Strip the .gitignore fenced block; if the file becomes empty,
        //    delete it (matches pre-install state on a fresh fixture).
        let gi_path = ctx.repo().join(".gitignore");
        if gi_path.exists() {
            let text = std::fs::read_to_string(&gi_path)
                .map_err(|e| specere_core::Error::Remove(format!("read .gitignore: {e}")))?;
            let stripped = specere_markers::text_block_fence::remove(&text, UNIT_ID)
                .map_err(|e| specere_core::Error::Remove(format!("gitignore strip: {e}")))?;
            if stripped.is_empty() {
                let _ = std::fs::remove_file(&gi_path);
            } else {
                std::fs::write(&gi_path, stripped)
                    .map_err(|e| specere_core::Error::Remove(format!("write .gitignore: {e}")))?;
            }
        }

        // 2) Delete skeleton files whose on-disk SHA still matches what the
        //    manifest recorded. User-edited files are preserved with a warning.
        for f in &record.files {
            if f.path.as_os_str() == std::ffi::OsStr::new(".gitignore") {
                continue;
            }
            let abs = ctx.repo().join(&f.path);
            if !abs.exists() {
                continue;
            }
            if f.owner == Owner::UserEditedAfterInstall {
                tracing::warn!(
                    "filter-state: `{}` marked user-edited at install; preserving on remove",
                    f.path.display()
                );
                continue;
            }
            let actual = specere_manifest::sha256_file(&abs).map_err(|e| {
                specere_core::Error::Remove(format!("sha256 {}: {e}", abs.display()))
            })?;
            if actual != f.sha256_post {
                tracing::warn!(
                    "filter-state: `{}` edited after install; preserving",
                    f.path.display()
                );
                continue;
            }
            std::fs::remove_file(&abs).map_err(|e| {
                specere_core::Error::Remove(format!("remove {}: {e}", abs.display()))
            })?;
        }

        Ok(())
    }
}

fn skeleton_files() -> [(&'static str, &'static [u8]); 3] {
    [
        ("events.sqlite", EVENTS_SQLITE_CONTENT),
        ("posterior.toml", POSTERIOR_TOML_CONTENT.as_bytes()),
        ("sensor-map.toml", SENSOR_MAP_TOML_CONTENT.as_bytes()),
    ]
}
