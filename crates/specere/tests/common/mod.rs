//! Shared test fixture for SpecERE integration tests.
//!
//! `TempRepo` creates an ephemeral `TempDir`, runs `git init`, makes one empty
//! commit, and exposes helpers to spawn the `specere` binary via `assert_cmd`
//! and hash files with SHA256. See `specs/002-phase-1-bugfix-0-2-0/plan.md`
//! prior #2 for the "live git, no pre-baked tarball" decision.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

use assert_cmd::Command as AssertCommand;
use tempfile::TempDir;

pub struct TempRepo {
    tmp: TempDir,
}

impl TempRepo {
    /// Create a fresh git-initialised temp directory with a single empty commit
    /// on `main`. The resulting tree is the standard fixture for Phase 1 tests.
    pub fn new() -> Self {
        let tmp = TempDir::new().expect("tempdir");
        run(tmp.path(), "git", &["init", "-q", "-b", "main"]);
        // Ensure git identity is set for the test's commits (CI may have none).
        run(
            tmp.path(),
            "git",
            &["config", "user.email", "test@specere.local"],
        );
        run(tmp.path(), "git", &["config", "user.name", "Specere Tests"]);
        run(
            tmp.path(),
            "git",
            &["commit", "--allow-empty", "-q", "-m", "initial"],
        );
        Self { tmp }
    }

    /// Create a temp dir that is NOT a git repo (for the non-git fallback path
    /// in FR-P1-001).
    pub fn new_non_git() -> Self {
        let tmp = TempDir::new().expect("tempdir");
        Self { tmp }
    }

    pub fn path(&self) -> &Path {
        self.tmp.path()
    }

    /// Run `specere <args>` inside the temp repo. Returns the `assert_cmd`
    /// wrapper so tests can chain `.assert()` calls.
    pub fn run_specere(&self, args: &[&str]) -> AssertCommand {
        let mut cmd = AssertCommand::cargo_bin("specere").expect("specere bin");
        cmd.current_dir(self.tmp.path());
        cmd.args(args);
        cmd
    }

    /// Hex-encoded SHA256 of a file inside the repo.
    pub fn sha256_of(&self, rel: &str) -> String {
        let path = self.tmp.path().join(rel);
        let bytes = std::fs::read(&path).unwrap_or_else(|_| Vec::new());
        hex_sha256(&bytes)
    }

    /// Write `content` to `rel` inside the repo, overwriting if it exists.
    pub fn write(&self, rel: &str, content: &str) {
        let path = self.tmp.path().join(rel);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(&path, content).expect("write temp file");
    }

    /// Delete a file inside the repo; panic if it doesn't exist.
    pub fn delete(&self, rel: &str) {
        std::fs::remove_file(self.tmp.path().join(rel)).expect("delete temp file");
    }

    /// Corrupt a file — replace its content with arbitrary bytes (used by
    /// FR-P1-008 tests).
    pub fn corrupt_file(&self, rel: &str, bytes: &[u8]) {
        let path = self.tmp.path().join(rel);
        std::fs::write(&path, bytes).expect("corrupt temp file");
    }

    /// Return the current git branch (helper for US1 tests).
    pub fn current_branch(&self) -> String {
        let out = Command::new("git")
            .args(["branch", "--show-current"])
            .current_dir(self.tmp.path())
            .output()
            .expect("git branch");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// Create a git branch inside the repo (without switching).
    pub fn create_branch(&self, name: &str) {
        run(self.tmp.path(), "git", &["branch", name]);
    }

    /// Return the absolute path of an entry relative to the repo root.
    pub fn abs(&self, rel: &str) -> PathBuf {
        self.tmp.path().join(rel)
    }
}

fn run(dir: &Path, prog: &str, args: &[&str]) {
    let status = Command::new(prog)
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap_or_else(|e| panic!("failed to spawn {prog}: {e}"));
    assert!(status.success(), "{prog} {:?} failed", args);
}

fn hex_sha256(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}
