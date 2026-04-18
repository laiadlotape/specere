//! The day-one set of `AddUnit` implementations plus the top-level command
//! dispatcher (`add`, `remove`, `status`, `verify`, `doctor`).

use anyhow::{anyhow, Context};
use specere_core::{AddUnit, Ctx, Owner};
use specere_manifest::{record_to_unit_entry, sha256_file, Manifest};

pub mod deploy;
pub mod ears_linter;
pub mod filter_state;
pub mod orphan;
pub mod otel_collector;
pub mod speckit;

pub const SPECERE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Flags passed by the top-level CLI through `add` into per-unit constructors.
#[derive(Debug, Default, Clone)]
pub struct AddFlags {
    /// `--branch <name>` — only consumed by the `speckit` unit (Phase 1).
    pub branch: Option<String>,
    /// `--adopt-edits` — drives the SHA-diff gate's fallback path.
    pub adopt_edits: bool,
    /// `--service` — only consumed by the `otel-collector` unit (Phase 2 / #13).
    pub with_service: bool,
}

/// Return the `AddUnit` trait object for a given unit id + flags.
pub fn lookup(id: &str, flags: &AddFlags) -> Option<Box<dyn AddUnit>> {
    match id {
        "speckit" => Some(Box::new(speckit::Speckit::with_flags(
            speckit::SpeckitFlags {
                branch: flags.branch.clone(),
            },
        ))),
        "claude-code-deploy" => Some(Box::new(deploy::claude_code::ClaudeCodeDeploy)),
        "filter-state" => Some(Box::new(filter_state::FilterState)),
        "otel-collector" => Some(Box::new(otel_collector::OtelCollector::new(
            flags.with_service,
        ))),
        "ears-linter" => Some(Box::new(ears_linter::EarsLinter)),
        _ => None,
    }
}

pub fn add(ctx: &Ctx, unit_id: &str, flags: &AddFlags) -> anyhow::Result<()> {
    let unit = lookup(unit_id, flags).ok_or_else(|| {
        anyhow!("unknown unit `{unit_id}`; run `specere status` to list installed ones")
    })?;

    let mut manifest = Manifest::load_or_init(&ctx.manifest_path(), SPECERE_VERSION)?;

    // FR-P1-003: SHA-diff gate on re-install. Only applies to native units
    // with a `files` list recorded in the manifest.
    if let Some(existing) = manifest.get(unit.id()) {
        let diverged = check_sha_divergence(ctx, existing);
        if diverged.is_empty() {
            tracing::info!("unit `{}` already installed — no-op", unit.id());
            return Ok(());
        }
        if !flags.adopt_edits {
            return Err(anyhow!(specere_core::Error::AlreadyInstalledMismatch {
                unit: unit.id().to_string(),
                files: diverged.into_iter().map(|(_, p)| p).collect(),
            }));
        }
        // Clarified: --adopt-edits refuses deletions.
        for (_, path) in &diverged {
            if !ctx.repo().join(path).exists() {
                return Err(anyhow!(specere_core::Error::DeletedOwnedFile {
                    unit: unit.id().to_string(),
                    path: path.clone(),
                }));
            }
        }
        // Adopt path: flip `owner` to user-edited for the diverged files
        // without rewriting them.
        let mut updated = existing.clone();
        for (idx, _path) in diverged {
            updated.files[idx].owner = Owner::UserEditedAfterInstall;
            updated.files[idx].sha256_post =
                sha256_file(&ctx.repo().join(&updated.files[idx].path))
                    .unwrap_or_else(|_| updated.files[idx].sha256_post.clone());
        }
        manifest.upsert(updated);
        manifest.save(&ctx.manifest_path())?;
        tracing::info!("adopted user edits for `{}`", unit.id());
        return Ok(());
    }

    let plan = unit.preflight(ctx).context("preflight failed")?;
    if ctx.dry_run() {
        print_plan(&plan);
        return Ok(());
    }

    let record = unit.install(ctx, &plan).context("install failed")?;

    // Extract branch hints from record.notes (set by Speckit::install) and
    // lift them into install_config.
    let mut install_config = toml::Table::new();
    let mut passthrough_notes = Vec::with_capacity(record.notes.len());
    for note in &record.notes {
        if let Some(v) = note.strip_prefix("branch_name=") {
            install_config.insert("branch_name".into(), toml::Value::String(v.to_string()));
        } else if let Some(v) = note.strip_prefix("branch_was_created_by_specere=") {
            let b = v == "true";
            install_config.insert(
                "branch_was_created_by_specere".into(),
                toml::Value::Boolean(b),
            );
        } else {
            passthrough_notes.push(note.clone());
        }
    }
    let mut trimmed_record = record;
    trimmed_record.notes = passthrough_notes;

    let entry = record_to_unit_entry(
        unit.id(),
        unit.pinned_version(),
        install_config,
        trimmed_record,
    );
    manifest.upsert(entry);
    manifest.save(&ctx.manifest_path())?;

    unit.postflight(ctx, &manifest.get(unit.id()).unwrap().clone_record())
        .context("postflight failed")?;

    tracing::info!("installed `{}` @ {}", unit.id(), unit.pinned_version());
    Ok(())
}

/// Return (index-in-files-list, repo-relative path) for every file whose
/// on-disk SHA256 disagrees with the manifest. Empty = clean.
fn check_sha_divergence(
    ctx: &Ctx,
    entry: &specere_manifest::UnitEntry,
) -> Vec<(usize, std::path::PathBuf)> {
    let mut out = Vec::new();
    for (i, f) in entry.files.iter().enumerate() {
        if f.owner == Owner::UserEditedAfterInstall {
            continue; // already adopted; not a divergence
        }
        let abs = ctx.repo().join(&f.path);
        if !abs.exists() {
            // deletion — reported separately via Error::DeletedOwnedFile in future tests.
            out.push((i, f.path.clone()));
            continue;
        }
        match sha256_file(&abs) {
            Ok(actual) if actual != f.sha256_post => {
                out.push((i, f.path.clone()));
            }
            _ => {}
        }
    }
    out
}

pub fn remove(
    ctx: &Ctx,
    unit_id: &str,
    dry_run: bool,
    _force: bool,
    delete_branch: bool,
) -> anyhow::Result<()> {
    let unit =
        lookup(unit_id, &AddFlags::default()).ok_or_else(|| anyhow!("unknown unit `{unit_id}`"))?;
    let mut manifest = Manifest::load_or_init(&ctx.manifest_path(), SPECERE_VERSION)?;
    let Some(entry) = manifest.get(unit.id()).cloned() else {
        return Err(anyhow!("unit `{}` is not installed", unit.id()));
    };

    if dry_run {
        println!(
            "Would remove unit `{}` (installed {}):",
            entry.id, entry.installed_at
        );
        for f in &entry.files {
            println!("  file   {}", f.path.display());
        }
        for m in &entry.markers {
            println!("  marker {} [{}]", m.path.display(), m.unit_id);
        }
        for d in &entry.dirs {
            println!("  dir    {}", d.display());
        }
        if entry.files.is_empty() && entry.dirs.is_empty() && entry.markers.is_empty() {
            println!("  (wrapper unit — delegates to upstream on remove)");
        }
        return Ok(());
    }

    let record = entry.clone_record();
    unit.remove(ctx, &record).context("remove failed")?;

    // FR-P1-007 / contracts/cli.md §Remove: speckit + --delete-branch.
    if delete_branch && unit_id == "speckit" {
        let branch_name = entry
            .install_config
            .get("branch_name")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let was_ours = entry
            .install_config
            .get("branch_was_created_by_specere")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if let Some(branch) = branch_name {
            if !was_ours {
                return Err(anyhow!(specere_core::Error::BranchNotOurs { branch }));
            }
            if git_working_tree_dirty(ctx) {
                return Err(anyhow!(specere_core::Error::BranchDirty { branch }));
            }
            // Switch off the branch first (can't delete the branch we're on).
            let _ = std::process::Command::new("git")
                .args(["checkout", "main"])
                .current_dir(ctx.repo())
                .status();
            let st = std::process::Command::new("git")
                .args(["branch", "-D", &branch])
                .current_dir(ctx.repo())
                .status()
                .map_err(|e| anyhow!("git branch -D failed: {e}"))?;
            if !st.success() {
                return Err(anyhow!("git branch -D {branch} exited non-zero"));
            }
        }
    }

    manifest.remove(unit.id());
    // If the manifest has no units left, garbage-collect the .specere/ directory
    // entirely so the repo returns to its pre-install state.
    if manifest.units.is_empty() {
        let _ = std::fs::remove_file(ctx.manifest_path());
        let _ = std::fs::remove_dir(ctx.specere_dir());
    } else {
        manifest.save(&ctx.manifest_path())?;
    }

    tracing::info!("removed `{}`", unit.id());
    Ok(())
}

pub fn status(ctx: &Ctx) -> anyhow::Result<()> {
    let manifest = Manifest::load_or_init(&ctx.manifest_path(), SPECERE_VERSION)?;
    if manifest.units.is_empty() {
        println!("No SpecERE units installed in {}", ctx.repo().display());
        return Ok(());
    }
    println!("SpecERE units installed in {}:", ctx.repo().display());
    for u in &manifest.units {
        let shape = if u.files.is_empty() && u.markers.is_empty() {
            "wrapper"
        } else {
            "native"
        };
        println!(
            "  {} @ {} [{}] ({} files, {} markers)",
            u.id,
            u.version,
            shape,
            u.files.len(),
            u.markers.len()
        );
    }
    Ok(())
}

pub fn verify(ctx: &Ctx) -> anyhow::Result<()> {
    let manifest = Manifest::load_or_init(&ctx.manifest_path(), SPECERE_VERSION)?;
    let mut drift = 0usize;
    for u in &manifest.units {
        for f in &u.files {
            let abs = ctx.repo().join(&f.path);
            if !abs.exists() {
                println!("MISSING  [{}] {}", u.id, f.path.display());
                drift += 1;
                continue;
            }
            let actual = sha256_file(&abs)?;
            if actual != f.sha256_post && f.owner == Owner::Specere {
                println!("DRIFTED  [{}] {}", u.id, f.path.display());
                drift += 1;
            }
        }
    }
    if drift == 0 {
        println!("No drift.");
    } else {
        println!("{drift} drift entries.");
    }
    Ok(())
}

/// Sweep orphan `.specify/` state (issue #16). Non-destructive if no orphan
/// is detected. Returns the number of orphan artifact groups cleaned.
pub fn clean_orphans(ctx: &Ctx) -> anyhow::Result<usize> {
    match orphan::detect(ctx.repo()) {
        Some(state) => {
            let n = 1 + state.orphan_runs.len();
            orphan::clean(ctx.repo(), &state)?;
            tracing::info!(
                "cleaned orphan feature dir at `{}` + {} workflow-run artifact(s)",
                state.feature_dir.display(),
                state.orphan_runs.len()
            );
            Ok(n)
        }
        None => Ok(0),
    }
}

pub fn doctor(ctx: &Ctx) -> anyhow::Result<()> {
    println!("SpecERE doctor — target: {}", ctx.repo().display());
    check("git", &["--version"]);
    check("uvx", &["--version"]);
    check("cargo", &["--version"]);
    let manifest_exists = ctx.manifest_path().exists();
    println!(
        "  manifest   {}",
        if manifest_exists { "present" } else { "absent" }
    );
    Ok(())
}

fn git_working_tree_dirty(ctx: &Ctx) -> bool {
    match std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(ctx.repo())
        .output()
    {
        Ok(o) => !o.stdout.is_empty(),
        Err(_) => false,
    }
}

fn check(program: &str, args: &[&str]) {
    match std::process::Command::new(program).args(args).output() {
        Ok(out) if out.status.success() => {
            let v = String::from_utf8_lossy(&out.stdout);
            println!("  {:10} OK  {}", program, v.lines().next().unwrap_or(""));
        }
        Ok(_) => println!("  {:10} FAIL", program),
        Err(_) => println!("  {:10} MISSING", program),
    }
}

fn print_plan(plan: &specere_core::Plan) {
    println!("Plan:");
    for op in &plan.ops {
        use specere_core::PlanOp::*;
        match op {
            WriteFile { path, summary } => println!("  + write   {} ({})", path.display(), summary),
            UpsertMarker { path, block_id } => {
                println!("  ~ marker  {} [{}]", path.display(), block_id)
            }
            RunCommand { program, args } => println!("  $ {} {}", program, args.join(" ")),
            AppendLines { path, lines } => {
                println!("  >> append {} ({} lines)", path.display(), lines.len())
            }
            CreateDir { path } => println!("  + mkdir   {}", path.display()),
        }
    }
}

/// Helper on `UnitEntry` to project back to a `Record` for `remove`.
trait UnitEntryExt {
    fn clone_record(&self) -> specere_core::Record;
}

impl UnitEntryExt for specere_manifest::UnitEntry {
    fn clone_record(&self) -> specere_core::Record {
        specere_core::Record {
            files: self
                .files
                .iter()
                .map(|f| specere_core::FileEntry {
                    path: f.path.clone(),
                    sha256_post: f.sha256_post.clone(),
                    owner: f.owner,
                    role: f.role.clone(),
                })
                .collect(),
            markers: self
                .markers
                .iter()
                .map(|m| specere_core::MarkerEntry {
                    path: m.path.clone(),
                    unit_id: m.unit_id.clone(),
                    block_id: m.block_id.clone(),
                    sha256: m.sha256.clone(),
                })
                .collect(),
            dirs: self.dirs.clone(),
            notes: self.notes.clone(),
        }
    }
}
