//! FR-P1-005. `claude-code-deploy` install registers exactly one `after_implement`
//! hook in `.specify/extensions.yml` pointing at `specere.observe.implement`.

mod common;

use common::TempRepo;

fn install_deploy(repo: &TempRepo) {
    let out = repo
        .run_specere(&["add", "claude-code-deploy"])
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "install failed — stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn creates_extensions_yml_when_absent() {
    let repo = TempRepo::new();
    // Simulate a SpecKit-init'd repo: .specify/ dir exists but no extensions.yml.
    std::fs::create_dir_all(repo.abs(".specify")).unwrap();
    install_deploy(&repo);

    let yml = std::fs::read_to_string(repo.abs(".specify/extensions.yml")).unwrap();
    assert!(yml.contains("specere:begin claude-code-deploy"));
    assert!(yml.contains("specere.observe.implement"));
    assert!(yml.contains("after_implement"));
}

#[test]
fn preserves_preexisting_hooks() {
    let repo = TempRepo::new();
    // A realistic extensions.yml with git's hooks already present.
    let base = "installed: []\nsettings:\n  auto_execute_hooks: true\nhooks:\n  after_implement:\n  - extension: git\n    command: speckit.git.commit\n    enabled: true\n    optional: true\n    prompt: Commit implementation changes?\n    description: Auto-commit after implementation\n    condition: null\n";
    std::fs::create_dir_all(repo.abs(".specify")).unwrap();
    repo.write(".specify/extensions.yml", base);

    install_deploy(&repo);

    let yml = std::fs::read_to_string(repo.abs(".specify/extensions.yml")).unwrap();
    // git entry still present.
    assert!(
        yml.contains("speckit.git.commit"),
        "git hook entry clobbered:\n{yml}"
    );
    // specere entry appended.
    assert!(
        yml.contains("specere.observe.implement"),
        "specere hook missing:\n{yml}"
    );
    // Exactly one specere entry.
    let count = yml.matches("specere.observe.implement").count();
    assert_eq!(
        count, 1,
        "expected exactly one specere.observe.implement entry"
    );
}

#[test]
fn specere_hook_has_required_fields() {
    let repo = TempRepo::new();
    std::fs::create_dir_all(repo.abs(".specify")).unwrap();
    install_deploy(&repo);
    let yml = std::fs::read_to_string(repo.abs(".specify/extensions.yml")).unwrap();

    // Extract the fenced block.
    let begin = "specere:begin claude-code-deploy";
    let start = yml.find(begin).expect("begin marker present");
    let block = &yml[start..];

    assert!(
        block.contains("extension: specere"),
        "missing extension: specere\n{block}"
    );
    assert!(
        block.contains("enabled: true"),
        "missing enabled: true\n{block}"
    );
    assert!(
        block.contains("optional: false"),
        "hook must be mandatory (optional: false)\n{block}"
    );
}
