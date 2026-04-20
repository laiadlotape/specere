//! 9-category harness classifier (FR-HM-001..002).
//!
//! Two-tier strategy: path conventions first (fast, exact), AST inspection
//! second (catches inline `#[cfg(test)]` modules and macro-based fuzz/prop
//! targets). Path-convention matches get `confidence = 1.0`; AST-based
//! classifications get `0.8` unless the macro idiom is unambiguous.
//!
//! A file that satisfies **multiple** idioms (e.g. a fixture under
//! `tests/common/` that also contains a `proptest!{}` block) is
//! classified by the *most-specific* category; the order below encodes
//! that specificity (property > fuzz > bench > snapshot > mock > fixture > test kind).

use std::path::{Path, PathBuf};

use syn::visit::Visit;

use crate::harness::node::Category;

/// Classification result — category + confidence + extracted test names.
#[derive(Debug, Clone, PartialEq)]
pub struct Classification {
    pub category: Category,
    pub confidence: f64,
    pub test_names: Vec<String>,
}

/// Classify a single file by path + (optionally) its source text.
/// `source` may be `None` for non-Rust files (workflow YAML, justfile);
/// in that case we rely on path conventions alone.
pub fn classify(repo_rel_path: &str, source: Option<&str>) -> Classification {
    // Path-convention matches first — these are authoritative.
    if let Some(cat) = path_convention(repo_rel_path) {
        // Workflows/goldens/snapshots aren't parseable Rust; return early.
        match cat {
            Category::Workflow | Category::Golden | Category::Snapshot => {
                return Classification {
                    category: cat,
                    confidence: 1.0,
                    test_names: Vec::new(),
                };
            }
            _ => {}
        }
    }

    // For Rust files, AST inspection refines the path-based guess.
    if is_rust(repo_rel_path) {
        if let Some(src) = source {
            return classify_rust(repo_rel_path, src);
        }
    }

    // Fallback: use path-convention result, or Production when nothing matches.
    let cat = path_convention(repo_rel_path).unwrap_or(Category::Production);
    Classification {
        category: cat,
        confidence: if matches!(cat, Category::Production) {
            0.5
        } else {
            1.0
        },
        test_names: Vec::new(),
    }
}

fn is_rust(path: &str) -> bool {
    PathBuf::from(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e == "rs")
        .unwrap_or(false)
}

/// Match the file path against directory + filename conventions.
/// Returns `None` when no convention applies.
fn path_convention(path: &str) -> Option<Category> {
    // Workflow files — GitHub Actions, GitLab CI, justfile, xtask.
    if path.starts_with(".github/workflows/") {
        return Some(Category::Workflow);
    }
    if path == ".gitlab-ci.yml" || path == "justfile" || path == ".justfile" {
        return Some(Category::Workflow);
    }
    if path.starts_with("xtask/") {
        return Some(Category::Workflow);
    }

    // Golden/snapshot data files.
    if path.ends_with(".snap") || path.ends_with(".snap.new") {
        return Some(Category::Snapshot);
    }
    if (path.contains("/fixtures/") || path.starts_with("fixtures/")) && !is_rust(path) {
        return Some(Category::Golden);
    }
    if path.contains(".expected.") || path.ends_with(".expected") {
        return Some(Category::Golden);
    }

    // Cargo standard harness directories.
    if path.starts_with("fuzz/fuzz_targets/") {
        return Some(Category::Fuzz);
    }
    if path.starts_with("benches/") {
        return Some(Category::Bench);
    }
    if path.starts_with("tests/common/") || path.starts_with("tests/fixtures/") {
        return Some(Category::Fixture);
    }
    if path.starts_with("tests/") && is_rust(path) {
        return Some(Category::Integration);
    }

    // Naming conventions for mocks — only applies to .rs files.
    if is_rust(path) {
        let fname = Path::new(path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        if fname.starts_with("mock_") || fname == "mocks.rs" || path.contains("/mocks/") {
            return Some(Category::Mock);
        }
    }

    None
}

/// Parse Rust source and use AST features to refine the category.
/// Handles inline `#[cfg(test)] mod tests` (→ Unit), `proptest!{}` or
/// `#[quickcheck]` (→ Property), `fuzz_target!` / `libfuzzer_sys` (→ Fuzz),
/// `criterion_group!` or `#[bench]` (→ Bench). Extracts test names.
fn classify_rust(path: &str, src: &str) -> Classification {
    let parsed = match syn::parse_file(src) {
        Ok(f) => f,
        Err(_) => {
            // Unparseable — fall back to path conventions only.
            let cat = path_convention(path).unwrap_or(Category::Production);
            return Classification {
                category: cat,
                confidence: 0.6,
                test_names: Vec::new(),
            };
        }
    };

    let mut v = AstInspector::default();
    v.visit_file(&parsed);

    // Rank AST signals in specificity order: fuzz > bench > property > snapshot > mock > unit.
    // Path signal wins for integration/fixture directories; otherwise AST wins.
    let ast_category = if v.saw_fuzz_target {
        Some(Category::Fuzz)
    } else if v.saw_bench || v.saw_criterion {
        Some(Category::Bench)
    } else if v.saw_proptest || v.saw_quickcheck {
        Some(Category::Property)
    } else if v.saw_insta_snapshot {
        // .rs file wrapping insta assertions is still a test file; treat as
        // whatever the path says but ensure we note it. Snapshot is the path
        // convention for `.snap` files, not `.rs`.
        None
    } else if v.saw_test_attr {
        Some(Category::Unit)
    } else {
        None
    };

    let path_cat = path_convention(path);
    let (category, confidence) = match (ast_category, path_cat) {
        (Some(a), Some(p)) => {
            // Prefer the more specific AST call when both exist;
            // exception: path says `integration` and AST says `unit` →
            // integration wins (the file lives in tests/*).
            if matches!(
                p,
                Category::Integration | Category::Fixture | Category::Mock
            ) && matches!(a, Category::Unit)
            {
                (p, 1.0)
            } else {
                (a, 0.9)
            }
        }
        (Some(a), None) => (a, 0.8),
        (None, Some(p)) => (p, 1.0),
        (None, None) => (Category::Production, 0.9),
    };

    Classification {
        category,
        confidence,
        test_names: v.test_names,
    }
}

#[derive(Default)]
struct AstInspector {
    saw_test_attr: bool,
    saw_proptest: bool,
    saw_quickcheck: bool,
    saw_fuzz_target: bool,
    saw_criterion: bool,
    saw_bench: bool,
    saw_insta_snapshot: bool,
    test_names: Vec<String>,
}

impl<'ast> Visit<'ast> for AstInspector {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        let mut is_test = false;
        let mut is_bench = false;
        for attr in &node.attrs {
            if attr.path().is_ident("test") {
                is_test = true;
            }
            if attr.path().is_ident("bench") {
                is_bench = true;
            }
            if let Some(last) = attr.path().segments.last() {
                let name = last.ident.to_string();
                if name == "test" || name == "rstest" {
                    is_test = true;
                }
                if name == "bench" {
                    is_bench = true;
                }
                if name == "quickcheck" {
                    self.saw_quickcheck = true;
                    is_test = true;
                }
            }
        }
        if is_test {
            self.saw_test_attr = true;
            self.test_names.push(node.sig.ident.to_string());
        }
        if is_bench {
            self.saw_bench = true;
            self.test_names.push(node.sig.ident.to_string());
        }
        syn::visit::visit_item_fn(self, node);
    }

    fn visit_stmt_macro(&mut self, node: &'ast syn::StmtMacro) {
        self.observe_macro(&node.mac);
        syn::visit::visit_stmt_macro(self, node);
    }

    fn visit_expr_macro(&mut self, node: &'ast syn::ExprMacro) {
        self.observe_macro(&node.mac);
        syn::visit::visit_expr_macro(self, node);
    }

    fn visit_item_macro(&mut self, node: &'ast syn::ItemMacro) {
        self.observe_macro(&node.mac);
        syn::visit::visit_item_macro(self, node);
    }
}

impl AstInspector {
    fn observe_macro(&mut self, mac: &syn::Macro) {
        let path: Vec<String> = mac
            .path
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect();
        let last = path.last().map(|s| s.as_str()).unwrap_or("");
        let joined = path.join("::");
        if last == "proptest" || joined == "proptest::proptest" {
            self.saw_proptest = true;
        }
        if last == "fuzz_target" || joined.starts_with("libfuzzer_sys::fuzz_target") {
            self.saw_fuzz_target = true;
        }
        if last == "criterion_group" || last == "criterion_main" {
            self.saw_criterion = true;
        }
        if last.starts_with("assert_snapshot")
            || last.starts_with("assert_debug_snapshot")
            || last.starts_with("assert_yaml_snapshot")
            || last.starts_with("assert_json_snapshot")
            || joined.starts_with("insta::assert_")
        {
            self.saw_insta_snapshot = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integration_test_by_path() {
        let c = classify("tests/fr_eq_003.rs", Some("#[test] fn foo(){}"));
        assert_eq!(c.category, Category::Integration);
        assert_eq!(c.test_names, vec!["foo"]);
    }

    #[test]
    fn fixture_path_beats_inline_test() {
        // A fixture file that happens to contain a helper `#[test]`
        // still belongs under `fixture` because of its directory.
        let c = classify("tests/common/mod.rs", Some("#[test] fn helper(){}"));
        assert_eq!(c.category, Category::Fixture);
    }

    #[test]
    fn unit_test_inside_src() {
        let src = r#"
            fn add(a: i32, b: i32) -> i32 { a + b }
            #[cfg(test)]
            mod tests {
                use super::*;
                #[test]
                fn adds_two_and_two() { assert_eq!(add(2,2), 4); }
            }
        "#;
        let c = classify("src/math.rs", Some(src));
        assert_eq!(c.category, Category::Unit);
        assert!(c.test_names.contains(&"adds_two_and_two".to_string()));
    }

    #[test]
    fn src_without_tests_is_production() {
        let src = "pub fn add(a: i32, b: i32) -> i32 { a + b }";
        let c = classify("src/math.rs", Some(src));
        assert_eq!(c.category, Category::Production);
    }

    #[test]
    fn benches_dir_detected() {
        let c = classify("benches/my_bench.rs", Some("criterion_group!(g, foo);"));
        assert_eq!(c.category, Category::Bench);
    }

    #[test]
    fn fuzz_target_detected() {
        let src = "libfuzzer_sys::fuzz_target!(|data: &[u8]| { let _ = data; });";
        let c = classify("fuzz/fuzz_targets/round_trip.rs", Some(src));
        assert_eq!(c.category, Category::Fuzz);
    }

    #[test]
    fn property_via_proptest_macro() {
        let src = r#"
            proptest! {
                #[test]
                fn addition_is_commutative(a in 0i32..1000, b in 0i32..1000) {
                    assert_eq!(a + b, b + a);
                }
            }
        "#;
        let c = classify("src/props.rs", Some(src));
        assert_eq!(c.category, Category::Property);
    }

    #[test]
    fn quickcheck_attribute_detected() {
        let src = r#"
            #[cfg(test)]
            mod tests {
                #[quickcheck]
                fn commutes(a: i32, b: i32) -> bool { a + b == b + a }
            }
        "#;
        let c = classify("src/lib.rs", Some(src));
        assert_eq!(c.category, Category::Property);
    }

    #[test]
    fn tokio_test_still_classified_as_unit_in_src() {
        let src = r#"
            #[cfg(test)]
            mod t {
                #[tokio::test] async fn async_thing() {}
            }
        "#;
        let c = classify("src/lib.rs", Some(src));
        assert_eq!(c.category, Category::Unit);
        assert!(c.test_names.contains(&"async_thing".to_string()));
    }

    #[test]
    fn workflow_yaml_detected() {
        let c = classify(".github/workflows/ci.yml", None);
        assert_eq!(c.category, Category::Workflow);
    }

    #[test]
    fn snapshot_file_detected() {
        let c = classify("tests/snapshots/foo.snap", None);
        assert_eq!(c.category, Category::Snapshot);
    }

    #[test]
    fn mock_file_detected() {
        let c = classify("src/mocks/mock_service.rs", Some("fn mk() {}"));
        assert_eq!(c.category, Category::Mock);
    }

    #[test]
    fn golden_fixture_json_detected() {
        let c = classify("tests/fixtures/sample.json", None);
        assert_eq!(c.category, Category::Golden);
    }

    #[test]
    fn unparseable_rust_falls_back_to_path() {
        let c = classify("tests/broken.rs", Some("not valid rust $$$"));
        // Integration from path; confidence dampened.
        assert_eq!(c.category, Category::Integration);
        assert!(c.confidence < 1.0);
    }
}
