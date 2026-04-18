//! FR-P2-003 / issue #14 — `ears-linter` unit:
//! - writes `.specere/lint/ears.toml` rules
//! - registers `before_clarify` hook in extensions.yml with `optional: true`
//! - installs a `specere-lint-ears` skill
//! - advisory only — never blocks a `/speckit-*` command

mod common;

use common::TempRepo;

fn install(repo: &TempRepo) {
    // Seed a minimal .specify/extensions.yml so the hook add can splice in.
    std::fs::create_dir_all(repo.abs(".specify")).unwrap();
    repo.write(
        ".specify/extensions.yml",
        "installed: []\nsettings:\n  auto_execute_hooks: true\nhooks:\n",
    );
    let out = repo
        .run_specere(&["add", "ears-linter"])
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "install failed — exit {:?}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn install_writes_rules_toml() {
    let repo = TempRepo::new();
    install(&repo);

    let rules = repo.abs(".specere/lint/ears.toml");
    assert!(rules.exists(), "rules toml not written");
    let text = std::fs::read_to_string(&rules).unwrap();
    assert!(text.contains("schema_version"));
    assert!(text.contains("ears-fr-prefix"));
    assert!(text.contains("ears-must-should"));

    // Must parse as valid TOML.
    let _: toml::Value = toml::from_str(&text).expect("rules.toml must parse as TOML");
}

#[test]
fn install_registers_before_clarify_hook_as_advisory() {
    let repo = TempRepo::new();
    install(&repo);

    let yml = std::fs::read_to_string(repo.abs(".specify/extensions.yml")).unwrap();
    // Find the fenced block.
    let begin = "specere:begin ears-linter";
    let start = yml.find(begin).expect("ears-linter fence missing");
    let block = &yml[start..];

    assert!(
        block.contains("specere.lint.ears"),
        "hook command not specere.lint.ears:\n{block}"
    );
    // Advisory = optional: true. Blocking = optional: false.
    assert!(
        block.contains("optional: true"),
        "ears-linter hook must be advisory (optional: true); got:\n{block}"
    );
    // Scope must be before_clarify.
    let before = &yml[..start];
    assert!(
        before.rfind("before_clarify:").is_some(),
        "ears-linter hook must sit under hooks.before_clarify; context:\n{before}"
    );
}

#[test]
fn install_writes_skill_file() {
    let repo = TempRepo::new();
    install(&repo);
    let skill = repo.abs(".claude/skills/specere-lint-ears/SKILL.md");
    assert!(skill.exists(), "skill file not written");
    let body = std::fs::read_to_string(&skill).unwrap();
    assert!(body.contains("name: specere-lint-ears"));
    assert!(body.contains("Never block"));
}

#[test]
fn round_trip_is_byte_identical() {
    let repo = TempRepo::new();
    std::fs::create_dir_all(repo.abs(".specify")).unwrap();
    let yml_before =
        "installed: []\nsettings:\n  auto_execute_hooks: true\nhooks:\n  before_clarify:\n  - extension: git\n    command: speckit.git.commit\n    enabled: true\n    optional: true\n    prompt: Commit?\n    description: Auto-commit\n    condition: null\n";
    repo.write(".specify/extensions.yml", yml_before);

    assert!(repo
        .run_specere(&["add", "ears-linter"])
        .output()
        .unwrap()
        .status
        .success());
    assert!(repo
        .run_specere(&["remove", "ears-linter"])
        .output()
        .unwrap()
        .status
        .success());

    let yml_after = std::fs::read_to_string(repo.abs(".specify/extensions.yml")).unwrap();
    assert_eq!(
        yml_before, yml_after,
        "extensions.yml not byte-identical after round-trip"
    );
    assert!(
        !repo.abs(".specere/lint/ears.toml").exists(),
        "rules file leaked on remove"
    );
    assert!(
        !repo
            .abs(".claude/skills/specere-lint-ears/SKILL.md")
            .exists(),
        "skill file leaked on remove"
    );
}
