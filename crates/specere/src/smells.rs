//! `specere lint tests` — static analysis of test files for smells that
//! degrade the filter's sensor calibration (FR-EQ-003).
//!
//! Walks `src/**/*.rs` + `tests/**/*.rs`, parses each file with `syn`,
//! identifies `#[test]` functions, and applies a rule set:
//!
//! - `tautological-assert` — `assert_eq!(x, x)`, `assert!(true)`,
//!   `assert_ne!(x, y)` where `x` and `y` are the same token stream.
//! - `no-assertion` — a `#[test]` function body contains no `assert*!`
//!   macro call, no `.unwrap_err()`, no `Result`-returning body with a
//!   matching `?`. Purely effectful tests that don't check anything.
//! - `mock-only` — the test body's non-trivial statements are ≥ 90 %
//!   mock-builder calls (`mock_*`, `Mock*::new()`) with no call into
//!   the actual subject under test.
//! - `single-fixture` — multiple `#[test]` fns against the same fn
//!   exercise only one input constant (happy-path-only).
//!
//! Every smell emits a `test_smell_detected` event at INFO severity
//! (per the v1 questionnaire answer — advisory, never blocks). Events
//! are consumed by `run_filter_run`'s calibration computation
//! (FR-EQ-005).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use quote::ToTokens;
use syn::visit::Visit;
use syn::{Expr, ExprMacro, ItemFn};

/// One detected smell with source location and severity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Smell {
    pub kind: SmellKind,
    pub test_fn: String,
    pub file: PathBuf,
    pub line: usize,
    pub severity: Severity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SmellKind {
    TautologicalAssert,
    NoAssertion,
    MockOnly,
    // SingleFixture needs cross-function analysis; deferred.
}

impl SmellKind {
    pub fn as_attr(self) -> &'static str {
        match self {
            Self::TautologicalAssert => "tautological-assert",
            Self::NoAssertion => "no-assertion",
            Self::MockOnly => "mock-only",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Info,
}

impl Severity {
    pub fn as_attr(self) -> &'static str {
        "info"
    }
}

/// Analyse a single Rust source file, returning detected smells.
pub fn analyse_file(path: &Path, src: &str) -> Vec<Smell> {
    let parsed = match syn::parse_file(src) {
        Ok(f) => f,
        Err(_) => return Vec::new(), // bad Rust → skip rather than error
    };
    let mut v = SmellVisitor {
        path: path.to_path_buf(),
        smells: Vec::new(),
        current_fn: None,
        current_fn_line: 0,
        in_test_fn: false,
    };
    v.visit_file(&parsed);
    v.smells
}

/// Walk a repo, analyse every `.rs` file under `src/` and `tests/`, return
/// the collected smells. Used by `specere lint tests` to drive event emission.
pub fn analyse_repo(repo: &Path) -> Result<Vec<Smell>> {
    let mut out = Vec::new();
    for root in ["src", "tests"] {
        let dir = repo.join(root);
        if !dir.is_dir() {
            continue;
        }
        for entry in walkdir::WalkDir::new(&dir) {
            let entry = entry.with_context(|| format!("walk {}", dir.display()))?;
            if !entry.file_type().is_file() {
                continue;
            }
            if entry.path().extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let src = match std::fs::read_to_string(entry.path()) {
                Ok(s) => s,
                Err(_) => continue,
            };
            out.extend(analyse_file(entry.path(), &src));
        }
    }
    Ok(out)
}

struct SmellVisitor {
    path: PathBuf,
    smells: Vec<Smell>,
    current_fn: Option<String>,
    current_fn_line: usize,
    in_test_fn: bool,
}

impl<'ast> Visit<'ast> for SmellVisitor {
    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        let is_test = has_test_attr(&node.attrs);
        let prior_fn = self.current_fn.take();
        let prior_line = self.current_fn_line;
        let prior_in_test = self.in_test_fn;

        self.current_fn = Some(node.sig.ident.to_string());
        self.current_fn_line = node.sig.fn_token.span.start().line;
        self.in_test_fn = is_test;

        if is_test {
            self.check_test_fn(node);
        }
        syn::visit::visit_item_fn(self, node);

        self.current_fn = prior_fn;
        self.current_fn_line = prior_line;
        self.in_test_fn = prior_in_test;
    }

    fn visit_expr_macro(&mut self, node: &'ast ExprMacro) {
        self.check_macro(&node.mac);
        syn::visit::visit_expr_macro(self, node);
    }

    fn visit_stmt_macro(&mut self, node: &'ast syn::StmtMacro) {
        // Stmt-position macros (`assert_eq!(...);` at the top of a test
        // body) are NOT wrapped in Expr::Macro in syn 2 — they're
        // StmtMacro. Visit explicitly or you'll miss every assert.
        self.check_macro(&node.mac);
        syn::visit::visit_stmt_macro(self, node);
    }
}

impl SmellVisitor {
    /// Called from both `visit_expr_macro` and `visit_stmt_macro` — a
    /// tautological assert is the same shape either way.
    fn check_macro(&mut self, mac: &syn::Macro) {
        if !self.in_test_fn {
            return;
        }
        if let Some(smell) = detect_tautological_mac(mac) {
            let fn_name = self.current_fn.clone().unwrap_or_else(|| "<anon>".into());
            let line = mac
                .path
                .segments
                .first()
                .map(|s| s.ident.span().start().line)
                .unwrap_or(self.current_fn_line);
            self.smells.push(Smell {
                kind: smell,
                test_fn: fn_name,
                file: self.path.clone(),
                line,
                severity: Severity::Info,
            });
        }
    }

    fn check_test_fn(&mut self, node: &ItemFn) {
        // 1. No-assertion: scan the function body for any assert*! macro
        //    OR .unwrap_err() OR `?` returning Err-path.
        let body_src = node.block.to_token_stream().to_string();
        let has_assertion = has_assertion_macro(&node.block)
            || body_src.contains(". unwrap_err")
            || body_src.contains(".expect_err")
            // `#[should_panic]` attribute means the panic itself is the assertion.
            || has_should_panic_attr(&node.attrs)
            // Tests returning Result with `?` — the ? is the assertion surface.
            || (is_result_returning(&node.sig) && body_src.contains('?'));
        if !has_assertion {
            let fn_name = node.sig.ident.to_string();
            self.smells.push(Smell {
                kind: SmellKind::NoAssertion,
                test_fn: fn_name,
                file: self.path.clone(),
                line: node.sig.fn_token.span.start().line,
                severity: Severity::Info,
            });
        }

        // Mock-only count across this test fn's body.
        let mut mock_counter = MockCounter::default();
        mock_counter.visit_block(&node.block);
        let fn_name = node.sig.ident.to_string();
        if mock_counter.total >= 3 && mock_counter.mock_only * 10 >= mock_counter.total * 9 {
            self.smells.push(Smell {
                kind: SmellKind::MockOnly,
                test_fn: fn_name,
                file: self.path.clone(),
                line: node.sig.fn_token.span.start().line,
                severity: Severity::Info,
            });
        }
    }
}

/// Returns Some(TautologicalAssert) iff the macro call is a recognised
/// tautology. Takes `&syn::Macro` so it works for both `ExprMacro` and
/// `StmtMacro`.
fn detect_tautological_mac(mac: &syn::Macro) -> Option<SmellKind> {
    let name = mac.path.segments.last()?.ident.to_string();
    match name.as_str() {
        "assert" => {
            let tokens = mac.tokens.to_string();
            let head = normalise_tokens(&tokens);
            // `assert!(true)` or `assert!(!false)` — both tokens normalise
            // to `"true"` / `"!false"` respectively.
            if head == "true" || head == "!false" {
                return Some(SmellKind::TautologicalAssert);
            }
        }
        "assert_eq" | "assert_ne" => {
            // `assert_eq!(x, x)` — both args identical after tokenising.
            if let Ok(args) = syn::parse2::<EqAssertArgs>(mac.tokens.clone()) {
                let left = args.left.to_token_stream().to_string();
                let right = args.right.to_token_stream().to_string();
                if normalise_tokens(&left) == normalise_tokens(&right) {
                    return Some(SmellKind::TautologicalAssert);
                }
            }
        }
        _ => {}
    }
    None
}

fn normalise_tokens(s: &str) -> String {
    s.split_whitespace().collect()
}

struct EqAssertArgs {
    left: Expr,
    right: Expr,
}

impl syn::parse::Parse for EqAssertArgs {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let left = input.parse()?;
        input.parse::<syn::Token![,]>()?;
        let right = input.parse()?;
        // Consume any trailing tokens (message format args in assert_eq!).
        let _: proc_macro2::TokenStream = input.parse()?;
        Ok(Self { left, right })
    }
}

/// Body-scan helper: is there at least one assert*! invocation anywhere
/// inside? Walks both expr-position AND stmt-position macros — in syn 2,
/// a top-level `assert_eq!(...)` in a test body is `Stmt::Macro`, not
/// wrapped in `Expr::Macro`.
fn has_assertion_macro(block: &syn::Block) -> bool {
    struct Finder {
        found: bool,
    }
    fn is_assertion(mac: &syn::Macro) -> bool {
        if let Some(last) = mac.path.segments.last() {
            let n = last.ident.to_string();
            return n.starts_with("assert") || n == "panic" || n == "unreachable";
        }
        false
    }
    impl<'ast> Visit<'ast> for Finder {
        fn visit_expr_macro(&mut self, node: &'ast ExprMacro) {
            if is_assertion(&node.mac) {
                self.found = true;
            }
            syn::visit::visit_expr_macro(self, node);
        }
        fn visit_stmt_macro(&mut self, node: &'ast syn::StmtMacro) {
            if is_assertion(&node.mac) {
                self.found = true;
            }
            syn::visit::visit_stmt_macro(self, node);
        }
    }
    let mut f = Finder { found: false };
    f.visit_block(block);
    f.found
}

fn has_test_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        // `#[test]` — path segment is just `test`.
        if a.path().is_ident("test") {
            return true;
        }
        // `#[tokio::test]`, `#[async_std::test]`, `#[actix_rt::test]`,
        // `#[rstest]` → last segment is `test`.
        let segs = &a.path().segments;
        if let Some(last) = segs.last() {
            let name = last.ident.to_string();
            return name == "test" || name == "rstest";
        }
        false
    })
}

fn has_should_panic_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| a.path().is_ident("should_panic"))
}

fn is_result_returning(sig: &syn::Signature) -> bool {
    match &sig.output {
        syn::ReturnType::Default => false,
        syn::ReturnType::Type(_, ty) => {
            let s = quote::ToTokens::to_token_stream(&**ty).to_string();
            s.contains("Result")
        }
    }
}

#[derive(Default)]
struct MockCounter {
    total: usize,
    mock_only: usize,
}

impl<'ast> Visit<'ast> for MockCounter {
    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        self.total += 1;
        // Path starts with `mock_` or uses `Mock` type.
        let s = quote::ToTokens::to_token_stream(&node.func).to_string();
        if s.contains("mock_") || s.contains("Mock") {
            self.mock_only += 1;
        }
        syn::visit::visit_expr_call(self, node);
    }
    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        self.total += 1;
        let method = node.method.to_string();
        // Mockall's API conventions: expect_*, return_const, returning,
        // with, times, in_sequence. We match the common prefixes/names
        // that appear ≥2× in a mock setup.
        if method.starts_with("mock_")
            || method.starts_with("expect")
            || method == "returning"
            || method == "return_const"
            || method == "with"
            || method == "times"
            || method == "in_sequence"
        {
            self.mock_only += 1;
        }
        syn::visit::visit_expr_method_call(self, node);
    }
}

/// CLI entry — `specere lint tests`. Walks the repo, emits one
/// `test_smell_detected` event per detected smell, prints a summary.
pub fn run_lint_tests(ctx: &specere_core::Ctx, sensor_map: Option<PathBuf>) -> Result<()> {
    let sensor_map_path = sensor_map.unwrap_or_else(|| ctx.repo().join(".specere/sensor-map.toml"));
    // Load specs for path-based attribution; empty [specs] is OK (we'll
    // emit unattributed events).
    let specs = specere_filter::load_specs(&sensor_map_path).unwrap_or_default();

    let smells = analyse_repo(ctx.repo())?;
    let mut emitted = 0;
    let mut per_spec_counts: BTreeMap<String, usize> = BTreeMap::new();

    for smell in &smells {
        // Attribute via intersection of smell.file with spec support sets.
        let file_rel = smell.file.strip_prefix(ctx.repo()).unwrap_or(&smell.file);
        let file_str = file_rel.to_string_lossy();
        let spec_id = specs
            .iter()
            .find(|s| {
                s.support.iter().any(|sup| {
                    let bare = sup.trim_end_matches('/');
                    let dir = format!("{bare}/");
                    file_str == bare || file_str.starts_with(dir.as_str())
                })
            })
            .map(|s| s.id.clone());

        let mut attrs = std::collections::BTreeMap::new();
        attrs.insert("event_kind".into(), "test_smell_detected".into());
        if let Some(sid) = &spec_id {
            attrs.insert("spec_id".into(), sid.clone());
            *per_spec_counts.entry(sid.clone()).or_insert(0) += 1;
        }
        attrs.insert("smell_kind".into(), smell.kind.as_attr().into());
        attrs.insert("severity".into(), smell.severity.as_attr().into());
        attrs.insert("test_fn".into(), smell.test_fn.clone());
        attrs.insert("file".into(), file_str.to_string());
        attrs.insert("line".into(), smell.line.to_string());

        let event = specere_telemetry::Event {
            ts: specere_telemetry::event_store::now_rfc3339(),
            source: "specere-lint-tests".into(),
            signal: "traces".into(),
            name: Some(format!(
                "{}: {} in {}",
                smell.kind.as_attr(),
                smell.test_fn,
                file_str
            )),
            feature_dir: None,
            attrs,
        };
        specere_telemetry::record(ctx, event)?;
        emitted += 1;
    }

    println!("specere lint tests: {emitted} smell(s) detected");
    for (sid, n) in &per_spec_counts {
        println!("  {sid}: {n}");
    }
    if smells.is_empty() {
        println!("  clean — no smells detected in `src/` or `tests/`");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path() -> PathBuf {
        PathBuf::from("test.rs")
    }

    #[test]
    fn detects_assert_eq_with_identical_args() {
        let src = r#"
        #[test]
        fn bad() {
            let x = 42;
            assert_eq!(x, x);
        }
        "#;
        let smells = analyse_file(&path(), src);
        assert_eq!(smells.len(), 1);
        assert_eq!(smells[0].kind, SmellKind::TautologicalAssert);
    }

    #[test]
    fn detects_assert_true() {
        let src = r#"
        #[test]
        fn bad() { assert!(true); }
        "#;
        let smells = analyse_file(&path(), src);
        assert!(smells
            .iter()
            .any(|s| s.kind == SmellKind::TautologicalAssert));
    }

    #[test]
    fn detects_no_assertion() {
        let src = r#"
        #[test]
        fn bad() {
            let _x = 1 + 1;
        }
        "#;
        let smells = analyse_file(&path(), src);
        assert!(smells.iter().any(|s| s.kind == SmellKind::NoAssertion));
    }

    #[test]
    fn respects_should_panic() {
        let src = r#"
        #[test]
        #[should_panic]
        fn intentionally_panics() {
            panic!("expected");
        }
        "#;
        let smells = analyse_file(&path(), src);
        // `#[should_panic]` means the panic IS the assertion — no NoAssertion.
        assert!(!smells.iter().any(|s| s.kind == SmellKind::NoAssertion));
    }

    #[test]
    fn result_returning_with_question_mark_counts_as_asserted() {
        let src = r#"
        #[test]
        fn ok() -> Result<(), String> {
            maybe_fail()?;
            Ok(())
        }
        "#;
        let smells = analyse_file(&path(), src);
        // The `?` is the assertion surface on a Result-returning test.
        assert!(!smells.iter().any(|s| s.kind == SmellKind::NoAssertion));
    }

    #[test]
    fn genuine_test_with_assert_is_clean() {
        let src = r#"
        #[test]
        fn good() {
            let result = 2 + 2;
            assert_eq!(result, 4);
        }
        "#;
        let smells = analyse_file(&path(), src);
        assert!(smells.is_empty(), "unexpected smells: {smells:?}");
    }

    #[test]
    fn non_test_fn_ignored() {
        let src = r#"
        fn helper() {
            assert_eq!(1, 1);
        }
        "#;
        let smells = analyse_file(&path(), src);
        assert!(
            smells.is_empty(),
            "non-#[test] fn should not be analysed: {smells:?}"
        );
    }

    #[test]
    fn tokio_test_detected() {
        let src = r#"
        #[tokio::test]
        async fn async_bad() {
            assert!(true);
        }
        "#;
        let smells = analyse_file(&path(), src);
        assert!(smells
            .iter()
            .any(|s| s.kind == SmellKind::TautologicalAssert));
    }

    #[test]
    fn detects_mock_only_test() {
        let src = r#"
        #[test]
        fn mocky() {
            let mock_svc = Mock::new();
            mock_svc.expect_foo().returning(|| 1);
            mock_svc.expect_bar().returning(|| 2);
            mock_svc.expect_baz().returning(|| 3);
        }
        "#;
        let smells = analyse_file(&path(), src);
        assert!(
            smells.iter().any(|s| s.kind == SmellKind::MockOnly),
            "expected mock-only smell; got {smells:?}"
        );
    }

    #[test]
    fn parse_errors_do_not_crash() {
        let src = "this is not valid rust %%%";
        let smells = analyse_file(&path(), src);
        assert!(smells.is_empty());
    }
}
