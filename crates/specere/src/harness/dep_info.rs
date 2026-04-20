//! `rustc --emit=dep-info` parser (FR-HM-003).
//!
//! rustc writes `.d` files alongside compiled artefacts under
//! `target/debug/deps/`. Each file is a Make-style dependency list:
//!
//! ```text
//! <path-to-artefact>.rmeta: foo.rs bar.rs baz.rs
//! ```
//!
//! Multi-line continuations use `\` at end-of-line; paths containing
//! spaces are escaped with a backslash (`\ `). We convert each entry to
//! repo-relative form and emit `direct_use` edges whose endpoints exist
//! in the already-classified node set.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::harness::node::{DirectEdge, HarnessFile};

/// Walk `target/debug/deps/*.d`, parse each, and emit direct-use edges
/// whose endpoints are both nodes in the scan graph.
pub fn collect_edges(dep_dir: &Path, nodes: &[HarnessFile]) -> Result<Vec<DirectEdge>> {
    // Build a path → id index once.
    let idx: std::collections::BTreeMap<&str, &str> = nodes
        .iter()
        .map(|n| (n.path.as_str(), n.id.as_str()))
        .collect();

    let mut edges: Vec<DirectEdge> = Vec::new();
    // repo root = two levels above `target/debug/deps/`.
    let repo = dep_dir
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .context("dep-info dir must live under <repo>/target/debug/deps")?;

    for entry in
        std::fs::read_dir(dep_dir).with_context(|| format!("read_dir {}", dep_dir.display()))?
    {
        let entry = entry?;
        if entry.path().extension().and_then(|e| e.to_str()) != Some("d") {
            continue;
        }
        let raw = match std::fs::read_to_string(entry.path()) {
            Ok(s) => s,
            Err(_) => continue,
        };
        for (target, sources) in parse_dep_file(&raw) {
            let target_rel = match to_repo_rel(repo, &target) {
                Some(s) => s,
                None => continue,
            };
            let from_id = match idx.get(target_rel.as_str()) {
                Some(id) => *id,
                None => continue,
            };
            for src in &sources {
                let src_rel = match to_repo_rel(repo, src) {
                    Some(s) => s,
                    None => continue,
                };
                if let Some(to_id) = idx.get(src_rel.as_str()) {
                    if from_id == *to_id {
                        continue; // self-loop
                    }
                    edges.push(DirectEdge {
                        from: from_id.to_string(),
                        to: (*to_id).to_string(),
                        from_path: target_rel.clone(),
                        to_path: src_rel.clone(),
                    });
                }
            }
        }
    }
    // Dedupe.
    edges.sort_by(|a, b| a.from.cmp(&b.from).then_with(|| a.to.cmp(&b.to)));
    edges.dedup();
    Ok(edges)
}

/// Parse a single `.d` file body. Returns (target, sources) per rule.
/// dep-info is simple enough that a hand-rolled tokeniser is cheaper
/// than pulling a makefile parser crate.
fn parse_dep_file(raw: &str) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    // Collapse line continuations: any `\<newline>` folds into a single space.
    let collapsed = raw.replace("\\\n", " ").replace("\\\r\n", " ");
    for line in collapsed.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // Split on the *first* unescaped `:` — anything after is the source list.
        let (lhs, rhs) = match split_on_unescaped_colon(line) {
            Some(p) => p,
            None => continue,
        };
        let target = unescape_make_path(lhs.trim());
        let sources: Vec<String> = tokenise_make_paths(rhs.trim());
        if !target.is_empty() {
            out.push((target, sources));
        }
    }
    out
}

fn split_on_unescaped_colon(s: &str) -> Option<(&str, &str)> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            i += 2;
            continue;
        }
        if bytes[i] == b':' {
            return Some((&s[..i], &s[i + 1..]));
        }
        i += 1;
    }
    None
}

fn tokenise_make_paths(s: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c == '\\' && i + 1 < bytes.len() {
            // Escaped space or colon → literal.
            cur.push(bytes[i + 1] as char);
            i += 2;
            continue;
        }
        if c.is_whitespace() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            i += 1;
            continue;
        }
        cur.push(c);
        i += 1;
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn unescape_make_path(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            out.push(bytes[i + 1] as char);
            i += 2;
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn to_repo_rel(repo: &Path, path: &str) -> Option<String> {
    let p = PathBuf::from(path);
    let abs = if p.is_absolute() { p } else { repo.join(&p) };
    let rel = abs.strip_prefix(repo).ok()?;
    let mut s = rel.to_string_lossy().to_string();
    if std::path::MAIN_SEPARATOR != '/' {
        s = s.replace(std::path::MAIN_SEPARATOR, "/");
    }
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::node::{path_id, Category, HarnessFile};
    use tempfile::TempDir;

    fn node(path: &str, cat: Category) -> HarnessFile {
        HarnessFile {
            id: path_id(path),
            path: path.to_string(),
            category: cat,
            category_confidence: 1.0,
            crate_name: None,
            test_names: Vec::new(),
            provenance: None,
            version_metrics: None,
            coverage_hash: None,
            flakiness_score: None,
        }
    }

    #[test]
    fn simple_single_line() {
        let raw = "target/debug/foo.rmeta: src/lib.rs src/foo.rs\n";
        let p = parse_dep_file(raw);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].0, "target/debug/foo.rmeta");
        assert_eq!(p[0].1, vec!["src/lib.rs", "src/foo.rs"]);
    }

    #[test]
    fn multi_line_continuation() {
        let raw = "target/a.rmeta: src/a.rs \\\n  src/b.rs \\\n  src/c.rs\n";
        let p = parse_dep_file(raw);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].1, vec!["src/a.rs", "src/b.rs", "src/c.rs"]);
    }

    #[test]
    fn escaped_space_in_path() {
        let raw = "target/x.rmeta: src/has\\ space.rs src/ok.rs\n";
        let p = parse_dep_file(raw);
        assert_eq!(p[0].1, vec!["src/has space.rs", "src/ok.rs"]);
    }

    #[test]
    fn empty_and_comment_lines_skipped() {
        let raw = "\n# comment\n\ntarget/y.rmeta: src/y.rs\n";
        let p = parse_dep_file(raw);
        assert_eq!(p.len(), 1);
    }

    #[test]
    fn collect_edges_builds_from_real_dir() {
        let dir = TempDir::new().unwrap();
        let repo = dir.path();
        let deps = repo.join("target").join("debug").join("deps");
        std::fs::create_dir_all(&deps).unwrap();
        std::fs::write(
            deps.join("demo-abc.d"),
            "tests/it.rs: tests/common/mod.rs src/lib.rs\n",
        )
        .unwrap();

        let nodes = vec![
            node("tests/it.rs", Category::Integration),
            node("tests/common/mod.rs", Category::Fixture),
            node("src/lib.rs", Category::Production),
        ];
        let edges = collect_edges(&deps, &nodes).unwrap();
        // Two edges: it → common, it → src/lib.
        assert_eq!(edges.len(), 2);
        let targets: Vec<&str> = edges.iter().map(|e| e.to_path.as_str()).collect();
        assert!(targets.contains(&"tests/common/mod.rs"));
        assert!(targets.contains(&"src/lib.rs"));
    }

    #[test]
    fn edges_outside_node_set_are_dropped() {
        let dir = TempDir::new().unwrap();
        let deps = dir.path().join("target").join("debug").join("deps");
        std::fs::create_dir_all(&deps).unwrap();
        std::fs::write(
            deps.join("x.d"),
            "tests/it.rs: /abs/external/crate/lib.rs\n",
        )
        .unwrap();
        let nodes = vec![node("tests/it.rs", Category::Integration)];
        let edges = collect_edges(&deps, &nodes).unwrap();
        // External path not in the node set → no edge emitted.
        assert!(edges.is_empty());
    }
}
