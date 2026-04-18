//! Issue #31 / FR-P3-002 / FR-P3-006 — claude-code-deploy registers
//! before_<verb> + after_<verb> hooks for every SpecKit workflow verb, all
//! pointing at the generic `specere.observe.step` command.

mod common;

use common::TempRepo;

const ALL_VERBS: &[&str] = &[
    "specify",
    "clarify",
    "plan",
    "tasks",
    "analyze",
    "checklist",
    "implement",
];

fn install(repo: &TempRepo) {
    std::fs::create_dir_all(repo.abs(".specify")).unwrap();
    // Seed a minimal extensions.yml so the hook adds have a target to splice into.
    repo.write(
        ".specify/extensions.yml",
        "installed: []\nsettings:\n  auto_execute_hooks: true\nhooks:\n",
    );
    let out = repo
        .run_specere(&["add", "claude-code-deploy"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "install failed — stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn install_registers_before_and_after_hooks_for_every_verb() {
    let repo = TempRepo::new();
    install(&repo);

    let yml = std::fs::read_to_string(repo.abs(".specify/extensions.yml")).unwrap();
    // `before_<verb>` for every verb, `after_<verb>` for every verb except
    // `implement` (still handled by the pre-existing single hook with
    // `command: specere.observe.implement` under the main claude-code-deploy
    // block — preserves FR-P1-005).
    for verb in ALL_VERBS {
        let before_id = format!("workflow-spans-before-{verb}");
        assert!(
            yml.contains(&before_id),
            "missing before-{verb} fenced block; yml:\n{yml}"
        );
        if *verb == "implement" {
            continue;
        }
        let after_id = format!("workflow-spans-after-{verb}");
        assert!(
            yml.contains(&after_id),
            "missing after-{verb} fenced block; yml:\n{yml}"
        );
    }

    // Count specere.observe.step entries — expect exactly 13 (7 before + 6 after;
    // after_implement is the pre-existing bespoke hook).
    let step_count = yml.matches("specere.observe.step").count();
    assert_eq!(
        step_count, 13,
        "expected 13 specere.observe.step hook entries; got {step_count}"
    );

    // Each specere.observe.step entry is advisory.
    // Count `optional: true` on specere.observe.step blocks (proxy via full
    // text match — extensions.yml may have other optional: true from git-ext).
    // Instead verify there are no `optional: false` specere.observe.step entries.
    assert_eq!(
        yml.matches("optional: false\n    prompt: \"Record ")
            .count(),
        0,
        "specere.observe.step hooks must all be advisory"
    );
}

#[test]
fn install_writes_specere_observe_step_skill() {
    let repo = TempRepo::new();
    install(&repo);
    let skill = repo.abs(".claude/skills/specere-observe-step/SKILL.md");
    assert!(skill.exists(), "specere-observe-step skill not shipped");
    let body = std::fs::read_to_string(&skill).unwrap();
    assert!(body.contains("name: specere-observe-step"));
    assert!(body.contains("specere observe record"));
}

#[test]
fn remove_strips_workflow_spans_block_cleanly() {
    let repo = TempRepo::new();
    std::fs::create_dir_all(repo.abs(".specify")).unwrap();
    let original =
        "installed: []\nsettings:\n  auto_execute_hooks: true\nhooks:\n  before_plan:\n  - extension: git\n    command: speckit.git.commit\n    enabled: true\n    optional: true\n    prompt: Commit?\n    description: Auto-commit\n    condition: null\n";
    repo.write(".specify/extensions.yml", original);

    assert!(repo
        .run_specere(&["add", "claude-code-deploy"])
        .output()
        .unwrap()
        .status
        .success());
    assert!(repo
        .run_specere(&["remove", "claude-code-deploy"])
        .output()
        .unwrap()
        .status
        .success());

    let after = std::fs::read_to_string(repo.abs(".specify/extensions.yml")).unwrap();
    assert_eq!(
        original, after,
        "extensions.yml should round-trip byte-identical"
    );
}
