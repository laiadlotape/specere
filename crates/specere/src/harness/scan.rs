//! Repo walker (FR-HM-001) — enumerates candidate harness files.
//!
//! Walks `src/`, `tests/`, `benches/`, `fuzz/`, `xtask/`,
//! `.github/workflows/`, and root-level `justfile`/`.justfile`. Every
//! file found is classified via `classify::classify`; files classified
//! as `Production` are *still* recorded (we need them as edge targets
//! for dep-info attribution), just with that category.
//!
//! Crate attribution: for each file under `crates/<name>/src/**` or
//! `crates/<name>/tests/**`, `crate_name = "<name>"`. For single-crate
//! repos, falls back to reading the top-level `Cargo.toml` package name.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::harness::classify;
use crate::harness::node::{path_id, repo_rel, HarnessFile};

/// Walk `repo` and return classified harness-file nodes sorted by id.
pub fn scan_repo(repo: &Path) -> Result<Vec<HarnessFile>> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut out: Vec<HarnessFile> = Vec::new();

    for root in candidate_roots(repo) {
        if !root.is_dir() {
            continue;
        }
        walk_dir(repo, &root, &mut seen, &mut out)
            .with_context(|| format!("walk {}", root.display()))?;
    }

    // Individual files: justfile, root-level workflow YAML variants.
    for fname in [".justfile", "justfile", "Justfile"] {
        let p = repo.join(fname);
        if p.is_file() {
            if let Some(rel) = repo_rel(repo, &p) {
                if !seen.contains(&rel) {
                    let c = classify::classify(&rel, None);
                    out.push(HarnessFile {
                        id: path_id(&rel),
                        path: rel.clone(),
                        category: c.category,
                        category_confidence: c.confidence,
                        crate_name: None,
                        test_names: c.test_names,
                        provenance: None,
                        version_metrics: None,
                    });
                    seen.insert(rel);
                }
            }
        }
    }

    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

fn candidate_roots(repo: &Path) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = vec![repo.join(".github").join("workflows"), repo.join("xtask")];
    // Per-crate roots: find every crate in cargo workspace.
    if repo.join("crates").is_dir() {
        if let Ok(entries) = std::fs::read_dir(repo.join("crates")) {
            for e in entries.flatten() {
                if e.path().is_dir() {
                    roots.push(e.path().join("src"));
                    roots.push(e.path().join("tests"));
                    roots.push(e.path().join("benches"));
                    roots.push(e.path().join("fuzz"));
                }
            }
        }
    }
    // Single-crate layout.
    roots.push(repo.join("src"));
    roots.push(repo.join("tests"));
    roots.push(repo.join("benches"));
    roots.push(repo.join("fuzz"));
    roots
}

fn walk_dir(
    repo: &Path,
    root: &Path,
    seen: &mut BTreeSet<String>,
    out: &mut Vec<HarnessFile>,
) -> Result<()> {
    for entry in walkdir::WalkDir::new(root).follow_links(false) {
        let entry = entry.with_context(|| format!("walkdir {}", root.display()))?;
        if !entry.file_type().is_file() {
            continue;
        }
        // Skip build artifacts — we never want to classify generated files
        // under `target/`, `.git/`, or `node_modules/`.
        let path = entry.path();
        if path_excluded(path) {
            continue;
        }
        let rel = match repo_rel(repo, path) {
            Some(r) => r,
            None => continue,
        };
        if seen.contains(&rel) {
            continue;
        }
        // Only classify .rs + .yml/.yaml + justfile; skip the rest (.lock,
        // .md, .toml within crates/*/src/ stays classified as Production
        // but we short-circuit unknown-ext to avoid reading huge binaries).
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let classify_src: Option<String> = match ext {
            "rs" => std::fs::read_to_string(path).ok(),
            "yml" | "yaml" => None, // classify by path only
            _ => continue,
        };
        let c = classify::classify(&rel, classify_src.as_deref());
        let crate_name = detect_crate_name(repo, &rel);
        out.push(HarnessFile {
            id: path_id(&rel),
            path: rel.clone(),
            category: c.category,
            category_confidence: c.confidence,
            crate_name,
            test_names: c.test_names,
            provenance: None,
            version_metrics: None,
        });
        seen.insert(rel);
    }
    Ok(())
}

fn path_excluded(path: &Path) -> bool {
    for seg in path.iter() {
        let s = seg.to_string_lossy();
        if s == "target" || s == ".git" || s == "node_modules" || s == ".specere" || s == ".specify"
        {
            return true;
        }
    }
    false
}

fn detect_crate_name(repo: &Path, rel: &str) -> Option<String> {
    // crates/<name>/... pattern.
    if let Some(after) = rel.strip_prefix("crates/") {
        let name = after.split('/').next()?;
        if !name.is_empty() {
            return Some(name.to_string());
        }
    }
    // Single-crate fallback: top-level Cargo.toml [package] name.
    let cargo = repo.join("Cargo.toml");
    if cargo.is_file() {
        if let Ok(raw) = std::fs::read_to_string(&cargo) {
            if let Ok(val) = toml::from_str::<toml::Value>(&raw) {
                if let Some(name) = val
                    .get("package")
                    .and_then(|p| p.get("name"))
                    .and_then(|n| n.as_str())
                {
                    return Some(name.to_string());
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::node::Category;
    use tempfile::TempDir;

    fn write(dir: &Path, rel: &str, content: &str) {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }

    #[test]
    fn finds_integration_and_unit_tests_in_single_crate() {
        let dir = TempDir::new().unwrap();
        write(
            dir.path(),
            "Cargo.toml",
            "[package]\nname = \"demo\"\nversion = \"0.1\"\n",
        );
        write(
            dir.path(),
            "src/lib.rs",
            "#[cfg(test)] mod t { #[test] fn a() {} }",
        );
        write(dir.path(), "tests/it.rs", "#[test] fn b() {}");
        let nodes = scan_repo(dir.path()).unwrap();
        let paths: Vec<&str> = nodes.iter().map(|n| n.path.as_str()).collect();
        assert!(paths.contains(&"src/lib.rs"));
        assert!(paths.contains(&"tests/it.rs"));
        // crate_name populated for both.
        assert!(nodes
            .iter()
            .all(|n| n.crate_name.as_deref() == Some("demo")));
    }

    #[test]
    fn workspace_crates_get_named() {
        let dir = TempDir::new().unwrap();
        write(
            dir.path(),
            "Cargo.toml",
            "[workspace]\nmembers = [\"crates/a\"]\n",
        );
        write(
            dir.path(),
            "crates/a/Cargo.toml",
            "[package]\nname = \"a\"\nversion=\"0.1\"\n",
        );
        write(dir.path(), "crates/a/src/lib.rs", "pub fn f() {}");
        let nodes = scan_repo(dir.path()).unwrap();
        let a = nodes
            .iter()
            .find(|n| n.path == "crates/a/src/lib.rs")
            .unwrap();
        assert_eq!(a.crate_name.as_deref(), Some("a"));
    }

    #[test]
    fn workflow_yaml_included() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), ".github/workflows/ci.yml", "name: CI\n");
        let nodes = scan_repo(dir.path()).unwrap();
        let ci = nodes
            .iter()
            .find(|n| n.path == ".github/workflows/ci.yml")
            .unwrap();
        assert_eq!(ci.category, Category::Workflow);
    }

    #[test]
    fn justfile_included() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "justfile", "test:\n\tcargo test\n");
        let nodes = scan_repo(dir.path()).unwrap();
        assert!(nodes
            .iter()
            .any(|n| n.path == "justfile" && n.category == Category::Workflow));
    }

    #[test]
    fn target_dir_excluded() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "src/lib.rs", "fn main(){}");
        write(dir.path(), "target/debug/foo.rs", "fn f(){}");
        let nodes = scan_repo(dir.path()).unwrap();
        assert!(!nodes.iter().any(|n| n.path.starts_with("target/")));
    }

    #[test]
    fn empty_repo_yields_empty_graph() {
        let dir = TempDir::new().unwrap();
        let nodes = scan_repo(dir.path()).unwrap();
        assert!(nodes.is_empty());
    }

    #[test]
    fn same_file_never_double_counted() {
        // Arrange for both `src/` and `crates/x/src/` to exist; scan must
        // still record each real file once.
        let dir = TempDir::new().unwrap();
        write(
            dir.path(),
            "crates/x/Cargo.toml",
            "[package]\nname=\"x\"\nversion=\"0.1\"\n",
        );
        write(dir.path(), "crates/x/src/lib.rs", "pub fn f(){}");
        let nodes = scan_repo(dir.path()).unwrap();
        let matches: Vec<_> = nodes
            .iter()
            .filter(|n| n.path == "crates/x/src/lib.rs")
            .collect();
        assert_eq!(matches.len(), 1);
    }
}
