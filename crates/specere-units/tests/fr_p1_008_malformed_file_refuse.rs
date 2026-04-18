//! FR-P1-008. Installers must refuse when a declared-format file is
//! syntactically invalid, surfacing exit code 3 and naming the file.

mod common;

use common::TempRepo;

#[test]
fn refuse_on_malformed_extensions_yml_during_install() {
    let repo = TempRepo::new();
    std::fs::create_dir_all(repo.abs(".specify")).unwrap();
    // Mismatched bracket = invalid YAML.
    repo.write(".specify/extensions.yml", "hooks: [after_implement: BROKEN");

    let out = repo
        .run_specere(&["add", "claude-code-deploy"])
        .output()
        .expect("spawn");
    assert_eq!(
        out.status.code(),
        Some(3),
        "expected ParseFailure exit 3; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("extensions.yml"),
        "stderr should name the file; got:\n{stderr}"
    );
}

#[test]
fn refuse_on_malformed_extensions_yml_during_remove() {
    let repo = TempRepo::new();
    std::fs::create_dir_all(repo.abs(".specify")).unwrap();
    let base =
        "installed: []\nsettings:\n  auto_execute_hooks: true\nhooks:\n  after_implement: []\n";
    repo.write(".specify/extensions.yml", base);

    assert!(repo
        .run_specere(&["add", "claude-code-deploy"])
        .output()
        .unwrap()
        .status
        .success());

    // User corrupts the YAML file AFTER install.
    repo.corrupt_file(".specify/extensions.yml", b"not: valid: yaml:::");

    let out = repo
        .run_specere(&["remove", "claude-code-deploy"])
        .output()
        .expect("spawn");
    assert!(
        !out.status.success(),
        "remove should refuse on corrupt YAML"
    );
}
