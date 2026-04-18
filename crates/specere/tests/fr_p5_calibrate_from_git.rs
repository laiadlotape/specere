//! Phase 5 — `specere calibrate from-git` end-to-end.
//!
//! Builds a throwaway repo with scripted commits that co-modify known
//! file prefixes, then runs the CLI and parses the emitted TOML snippet
//! to verify the coupling-edge suggester produced the expected edges.

mod common;

use common::TempRepo;

fn seed_sensor_map_three_specs(repo: &TempRepo) {
    repo.write(
        ".specere/sensor-map.toml",
        r#"
schema_version = 1

[specs]
"auth_login"     = { support = ["src/auth/"] }
"billing_charge" = { support = ["src/billing/"] }
"api_health"     = { support = ["src/api/"] }
"#,
    );
}

fn git_commit_with_files(repo: &TempRepo, files: &[&str], msg: &str) {
    for f in files {
        let abs = repo.abs(f);
        std::fs::create_dir_all(abs.parent().unwrap()).unwrap();
        std::fs::write(&abs, format!("// {msg}\n")).unwrap();
    }
    std::process::Command::new("git")
        .current_dir(repo.path())
        .arg("add")
        .arg(".")
        .status()
        .unwrap();
    std::process::Command::new("git")
        .current_dir(repo.path())
        .args(["commit", "-q", "-m", msg])
        .status()
        .unwrap();
}

#[test]
fn calibrate_from_git_proposes_coupling_for_co_modified_specs() {
    let repo = TempRepo::new();
    seed_sensor_map_three_specs(&repo);

    // 5 commits that touch auth + billing together.
    for i in 0..5 {
        git_commit_with_files(
            &repo,
            &["src/auth/login.rs", "src/billing/charge.rs"],
            &format!("refactor auth+billing {i}"),
        );
    }
    // 1 isolated api commit.
    git_commit_with_files(&repo, &["src/api/health.rs"], "api healthcheck");
    // 1 commit touching a file no spec claims (noise).
    git_commit_with_files(&repo, &["README.md"], "docs only");

    let output = repo
        .run_specere(&["calibrate", "from-git"])
        .output()
        .expect("calibrate failed to spawn");
    assert!(
        output.status.success(),
        "calibrate exited non-zero.\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();

    // Snippet must contain the proposed edge with the right count.
    assert!(
        stdout.contains("[\"auth_login\", \"billing_charge\"]"),
        "expected auth_login -> billing_charge edge in:\n{stdout}"
    );
    assert!(stdout.contains("5 co-commits"));

    // api_health was isolated — no edge involving it.
    assert!(
        !stdout.contains("\"api_health\""),
        "api_health should not appear in edges:\n{stdout}"
    );

    // Summary on stderr: walked 7 commits, 6 touched a spec.
    assert!(
        stderr.contains("analysed 7 commit"),
        "unexpected stderr:\n{stderr}"
    );
    assert!(stderr.contains("6 touched"));
}

#[test]
fn calibrate_from_git_rejects_empty_specs_section() {
    let repo = TempRepo::new();
    repo.write(".specere/sensor-map.toml", "schema_version = 1\n[specs]\n");
    // Make at least one commit so git log is non-empty.
    git_commit_with_files(&repo, &["src/a.rs"], "seed");
    let output = repo
        .run_specere(&["calibrate", "from-git"])
        .output()
        .expect("spawn failed");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("[specs] section empty"),
        "expected empty-specs error, got:\n{stderr}"
    );
}

#[test]
fn calibrate_from_git_respects_min_commits_flag() {
    let repo = TempRepo::new();
    seed_sensor_map_three_specs(&repo);
    // Only 2 co-commits — below the default 3 threshold.
    for i in 0..2 {
        git_commit_with_files(
            &repo,
            &["src/auth/login.rs", "src/billing/charge.rs"],
            &format!("co-edit {i}"),
        );
    }
    let output = repo
        .run_specere(&["calibrate", "from-git", "--min-commits", "5"])
        .output()
        .expect("spawn failed");
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("[\"auth_login\""),
        "edge should be below threshold:\n{stdout}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no coupling edges proposed"));
}
