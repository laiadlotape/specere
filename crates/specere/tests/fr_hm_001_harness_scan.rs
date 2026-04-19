//! FR-HM-001..004 — `specere harness scan` end-to-end.
//!
//! Strategy: seed a TempRepo with a mix of unit + integration + bench +
//! workflow files, drive the CLI, then verify the classified node list
//! in `.specere/harness-graph.toml`.

mod common;

use common::TempRepo;

#[test]
fn scan_classifies_every_harness_category_we_can_detect_statically() {
    let repo = TempRepo::new();
    repo.write(
        "Cargo.toml",
        "[package]\nname = \"demo\"\nversion = \"0.1\"\n",
    );
    // Unit (#[cfg(test)] inline inside src/).
    repo.write(
        "src/lib.rs",
        "#[cfg(test)] mod t { #[test] fn a(){} #[tokio::test] async fn b(){} }",
    );
    // Integration.
    repo.write("tests/it.rs", "#[test] fn i1(){}");
    // Fixture shared helper.
    repo.write("tests/common/mod.rs", "pub fn h(){}");
    // Bench.
    repo.write(
        "benches/mybench.rs",
        "use criterion::{criterion_group, criterion_main};\nfn _bench(_c: &mut criterion::Criterion) {}\ncriterion_group!(g, _bench);\ncriterion_main!(g);\n",
    );
    // Property.
    repo.write(
        "src/props.rs",
        "proptest::proptest! { #[test] fn pp(a in 0..100i32) { assert!(a>=0); } }",
    );
    // Fuzz target.
    repo.write(
        "fuzz/fuzz_targets/round.rs",
        "libfuzzer_sys::fuzz_target!(|data: &[u8]| { let _ = data; });",
    );
    // Workflow.
    repo.write(".github/workflows/ci.yml", "name: CI\non: [push]\n");
    // justfile.
    repo.write("justfile", "test:\n\tcargo test\n");

    let out = repo
        .run_specere(&["harness", "scan"])
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "harness scan failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let raw = std::fs::read_to_string(repo.abs(".specere/harness-graph.toml"))
        .expect("harness-graph.toml should be written");
    // Parse as TOML.
    let val: toml::Value = toml::from_str(&raw).expect("valid TOML");
    let nodes = val["nodes"].as_array().expect("nodes array");

    // Collect (path → category) pairs.
    let mut cat_of: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for n in nodes {
        let p = n["path"].as_str().unwrap().to_string();
        let c = n["category"].as_str().unwrap().to_string();
        cat_of.insert(p, c);
    }

    assert_eq!(cat_of.get("src/lib.rs").map(String::as_str), Some("unit"));
    assert_eq!(
        cat_of.get("tests/it.rs").map(String::as_str),
        Some("integration")
    );
    assert_eq!(
        cat_of.get("tests/common/mod.rs").map(String::as_str),
        Some("fixture")
    );
    assert_eq!(
        cat_of.get("benches/mybench.rs").map(String::as_str),
        Some("bench")
    );
    assert_eq!(
        cat_of.get("src/props.rs").map(String::as_str),
        Some("property")
    );
    assert_eq!(
        cat_of.get("fuzz/fuzz_targets/round.rs").map(String::as_str),
        Some("fuzz")
    );
    assert_eq!(
        cat_of.get(".github/workflows/ci.yml").map(String::as_str),
        Some("workflow")
    );
    assert_eq!(cat_of.get("justfile").map(String::as_str), Some("workflow"));
}

#[test]
fn scan_output_is_byte_identical_on_repeated_runs() {
    let repo = TempRepo::new();
    repo.write(
        "Cargo.toml",
        "[package]\nname = \"demo\"\nversion=\"0.1\"\n",
    );
    repo.write("src/lib.rs", "pub fn a() {}");
    repo.write("tests/it.rs", "#[test] fn t(){}");

    let _ = repo
        .run_specere(&["harness", "scan"])
        .output()
        .expect("spawn");
    let first = std::fs::read(repo.abs(".specere/harness-graph.toml")).unwrap();

    let _ = repo
        .run_specere(&["harness", "scan"])
        .output()
        .expect("spawn");
    let second = std::fs::read(repo.abs(".specere/harness-graph.toml")).unwrap();

    assert_eq!(
        first, second,
        "harness-graph.toml must be byte-identical on re-scan"
    );
}

#[test]
fn scan_summary_reports_per_category_counts() {
    let repo = TempRepo::new();
    repo.write("Cargo.toml", "[package]\nname=\"demo\"\nversion=\"0.1\"\n");
    repo.write("src/a.rs", "#[cfg(test)] mod t { #[test] fn a(){} }");
    repo.write("tests/b.rs", "#[test] fn b(){}");
    repo.write("tests/c.rs", "#[test] fn c(){}");

    let out = repo
        .run_specere(&["harness", "scan"])
        .output()
        .expect("spawn");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("file(s) classified"),
        "missing header:\n{stdout}"
    );
    assert!(
        stdout.contains("integration"),
        "missing category row:\n{stdout}"
    );
}

#[test]
fn scan_format_json_emits_valid_json() {
    let repo = TempRepo::new();
    repo.write("Cargo.toml", "[package]\nname=\"demo\"\nversion=\"0.1\"\n");
    repo.write("src/lib.rs", "pub fn f(){}");

    let out = repo
        .run_specere(&["harness", "scan", "--format", "json"])
        .output()
        .expect("spawn");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let _: serde_json::Value = serde_json::from_str(&stdout).expect("stdout must be valid JSON");
}

#[test]
fn scan_skips_target_directory() {
    let repo = TempRepo::new();
    repo.write("Cargo.toml", "[package]\nname=\"demo\"\nversion=\"0.1\"\n");
    repo.write("src/lib.rs", "pub fn f(){}");
    // A spurious .rs file under target/ — must never be classified.
    repo.write("target/debug/build/spurious.rs", "fn x(){}");

    let _ = repo
        .run_specere(&["harness", "scan"])
        .output()
        .expect("spawn");
    let raw = std::fs::read_to_string(repo.abs(".specere/harness-graph.toml")).unwrap();
    assert!(!raw.contains("target/debug/build/spurious.rs"));
}
