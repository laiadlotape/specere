//! FR-P1-007. The `speckit` unit's manifest entry records the feature branch
//! name and whether SpecERE created it.

mod common;

use common::TempRepo;

#[test]
fn manifest_records_branch_fields_on_git_target() {
    let repo = TempRepo::new();
    let out = repo
        .run_specere(&["add", "speckit"])
        .env("SPECERE_TEST_SKIP_UVX", "1")
        .output()
        .expect("spawn");
    assert!(
        out.status.success(),
        "exit {:?}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );

    let manifest_text =
        std::fs::read_to_string(repo.abs(".specere/manifest.toml")).expect("manifest present");
    let m: toml::Value = toml::from_str(&manifest_text).expect("manifest parses");

    let speckit = m
        .get("units")
        .and_then(|u| u.as_array())
        .and_then(|a| {
            a.iter()
                .find(|t| t.get("id").and_then(|i| i.as_str()) == Some("speckit"))
        })
        .expect("speckit unit in manifest");
    let cfg = speckit
        .get("install_config")
        .expect("install_config present");

    assert_eq!(
        cfg.get("branch_name").and_then(|v| v.as_str()),
        Some("000-baseline"),
        "manifest missing branch_name or wrong value:\n{manifest_text}"
    );
    assert_eq!(
        cfg.get("branch_was_created_by_specere")
            .and_then(|v| v.as_bool()),
        Some(true),
        "manifest missing branch_was_created_by_specere or wrong value:\n{manifest_text}"
    );
}

#[test]
fn manifest_records_false_when_branch_preexisted() {
    let repo = TempRepo::new();
    repo.create_branch("000-baseline");
    let out = repo
        .run_specere(&["add", "speckit"])
        .env("SPECERE_TEST_SKIP_UVX", "1")
        .output()
        .expect("spawn");
    assert!(out.status.success());

    let m: toml::Value =
        toml::from_str(&std::fs::read_to_string(repo.abs(".specere/manifest.toml")).unwrap())
            .unwrap();
    let cfg = m["units"][0]["install_config"].clone();
    assert_eq!(cfg["branch_name"].as_str(), Some("000-baseline"));
    assert_eq!(cfg["branch_was_created_by_specere"].as_bool(), Some(false));
}

#[test]
fn manifest_omits_branch_fields_on_non_git_target() {
    let repo = TempRepo::new_non_git();
    let out = repo
        .run_specere(&["add", "speckit"])
        .env("SPECERE_TEST_SKIP_UVX", "1")
        .output()
        .expect("spawn");
    assert!(out.status.success());

    let m: toml::Value =
        toml::from_str(&std::fs::read_to_string(repo.abs(".specere/manifest.toml")).unwrap())
            .unwrap();
    let cfg = m["units"][0]["install_config"]
        .as_table()
        .expect("config table");
    assert!(
        cfg.get("branch_name").is_none(),
        "branch_name should be absent on non-git target; got: {cfg:?}"
    );
    assert!(cfg.get("branch_was_created_by_specere").is_none());
}
