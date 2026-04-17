//! Core types shared across every SpecERE crate.
//!
//! `Ctx` is the per-invocation context (target repo path, dry-run flag).
//! `AddUnit` is the six-tuple contract every scaffolded capability implements:
//! `preflight -> install -> postflight` and a reverse `remove`, all bound to a
//! `Plan` and `Record` for idempotence and auditability.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("unit `{0}` not found in registry")]
    UnknownUnit(String),
    #[error("unit `{unit}` already installed (manifest entry present); pass --force to reinstall")]
    AlreadyInstalled { unit: String },
    #[error("unit `{unit}` not installed")]
    NotInstalled { unit: String },
    #[error("preflight failed: {0}")]
    Preflight(String),
    #[error("install failed: {0}")]
    Install(String),
    #[error("remove failed: {0}")]
    Remove(String),
    #[error("other: {0}")]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Per-invocation execution context.
#[derive(Debug, Clone)]
pub struct Ctx {
    repo: PathBuf,
    dry_run: bool,
}

impl Ctx {
    pub fn new(repo: PathBuf) -> Self {
        Self {
            repo,
            dry_run: false,
        }
    }

    pub fn with_dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    pub fn repo(&self) -> &Path {
        &self.repo
    }

    pub fn dry_run(&self) -> bool {
        self.dry_run
    }

    /// Return `.specere/` under the target repo.
    pub fn specere_dir(&self) -> PathBuf {
        self.repo.join(".specere")
    }

    /// Return the manifest path `.specere/manifest.toml`.
    pub fn manifest_path(&self) -> PathBuf {
        self.specere_dir().join("manifest.toml")
    }
}

/// A declarative plan an `AddUnit` produces during preflight. `install` executes
/// it; `--dry-run` prints it.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Plan {
    pub ops: Vec<PlanOp>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum PlanOp {
    /// Create a file with the given content.
    WriteFile { path: PathBuf, summary: String },
    /// Insert/replace a marker-fenced block in an existing file.
    UpsertMarker { path: PathBuf, block_id: String },
    /// Run a command; captured purely for user visibility in dry-run.
    RunCommand { program: String, args: Vec<String> },
    /// Append entries to an existing file (e.g. `.gitignore`).
    AppendLines { path: PathBuf, lines: Vec<String> },
    /// Create a directory.
    CreateDir { path: PathBuf },
}

/// The outcome record of a successful `install`. Persisted to the manifest so
/// `remove` can do an exact inverse.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Record {
    pub files: Vec<FileEntry>,
    pub markers: Vec<MarkerEntry>,
    pub dirs: Vec<PathBuf>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: PathBuf,
    pub sha256_post: String,
    pub owner: Owner,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkerEntry {
    pub path: PathBuf,
    pub unit_id: String,
    pub block_id: Option<String>,
    pub sha256: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Owner {
    Specere,
    UserEditedAfterInstall,
    Upstream,
}

/// The contract every scaffolded capability implements. Keep it small.
pub trait AddUnit {
    /// Stable id (`"speckit"`, `"otel-collector"`…).
    fn id(&self) -> &'static str;

    /// Pinned upstream version or internal semver; persisted to the manifest.
    fn pinned_version(&self) -> &'static str;

    /// Read-only detection pass. Returns a `Plan` describing what `install`
    /// would do. Called by `--dry-run`.
    fn preflight(&self, ctx: &Ctx) -> Result<Plan>;

    /// Execute the plan; return a `Record` for the manifest.
    fn install(&self, ctx: &Ctx, plan: &Plan) -> Result<Record>;

    /// Effects that are not rolled back on reinstall (e.g. opening an editor).
    fn postflight(&self, _ctx: &Ctx, _record: &Record) -> Result<()> {
        Ok(())
    }

    /// Reverse `install` using the persisted `Record`. Must leave the repo in
    /// its pre-install state (modulo documented postflight effects).
    fn remove(&self, ctx: &Ctx, record: &Record) -> Result<()>;
}
