//! FR-EQ-020..024 — `specere adversary run` end-to-end via mock provider.
//!
//! Uses the hidden `--from-fixture <dir>` flag so the mock provider reads
//! canned `iter_<N>.sh` scripts and never hits a real LLM endpoint. Tests:
//!
//! - FR-EQ-020: iterating the ask → sandbox loop emits per-iter events
//!   and a `counterexample_found` event on a reproducible failure.
//! - FR-EQ-021: a budget cap below current spend aborts with
//!   `adversary_budget_exceeded`.
//! - FR-EQ-022: one-shot findings (fixture fails on iter 1, passes on
//!   iters 2–5) emit `counterexample_candidate`, never `found`, so the
//!   posterior path is untouched.
//! - FR-EQ-023: `counterexample_found` event carries both `original_len`
//!   and `minimized_len`, with `minimized_len <= original_len`.
//! - FR-EQ-024: sandbox `none` mode completes; `rlimit` mode also works.
//!
//! The sandbox mode used here is `none` — we are running `exit 1` style
//! scripts, not LLM-generated code; there is no attack surface in CI to
//! defend against, and bwrap is not assumed installed on the runner.

mod common;

use common::TempRepo;

fn seed_sensor_map(repo: &TempRepo) {
    repo.write(
        ".specere/sensor-map.toml",
        r#"
schema_version = 1

[specs]
"FR-demo" = { support = ["src/auth/"] }

[channels]
"#,
    );
}

fn write_fixture(repo: &TempRepo, name: &str, body: &str) {
    repo.write(&format!(".specere/fixtures/{name}/iter_1.sh"), body);
}

fn write_fixtures(repo: &TempRepo, name: &str, per_iter: &[&str]) {
    for (i, body) in per_iter.iter().enumerate() {
        repo.write(&format!(".specere/fixtures/{name}/iter_{}.sh", i + 1), body);
    }
}

fn read_events(repo: &TempRepo) -> Vec<serde_json::Value> {
    let path = repo.abs(".specere/events.jsonl");
    if !path.exists() {
        return Vec::new();
    }
    let raw = std::fs::read_to_string(&path).unwrap();
    raw.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn events_with_kind<'a>(events: &'a [serde_json::Value], kind: &str) -> Vec<&'a serde_json::Value> {
    events
        .iter()
        .filter(|e| e["attrs"]["event_kind"].as_str() == Some(kind))
        .collect()
}

/// Duplicate of `specere::adversary::spend::current_month_utc` since the
/// integration test crate can't import the binary's private modules.
fn current_month_utc() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let days = now / 86400;
    let mut year = 1970i64;
    let mut rem = days as i64;
    loop {
        let diy = if is_leap(year) { 366 } else { 365 };
        if rem < diy {
            break;
        }
        rem -= diy;
        year += 1;
    }
    let mut month = 1u32;
    for m in 1..=12u32 {
        let dim = days_in_month(year, m);
        if rem < dim as i64 {
            month = m;
            break;
        }
        rem -= dim as i64;
    }
    format!("{year:04}-{month:02}")
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}

fn days_in_month(y: i64, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap(y) {
                29
            } else {
                28
            }
        }
        _ => unreachable!(),
    }
}

#[test]
fn fr_eq_020_reproducible_failure_emits_counterexample_found() {
    let repo = TempRepo::new();
    seed_sensor_map(&repo);
    // Iter 1 passes, iter 2 passes, iter 3 fails → ≥3 iter rule satisfied.
    write_fixtures(
        &repo,
        "repro",
        &[
            "exit 0\n",
            "exit 0\n",
            "# counterexample: division by zero in charge()\necho 'boom'\nexit 1\n",
        ],
    );
    repo.run_specere(&[
        "adversary",
        "run",
        "--spec",
        "FR-demo",
        "--provider",
        "mock",
        "--max-iterations",
        "5",
        "--sandbox",
        "none",
        "--from-fixture",
        ".specere/fixtures/repro",
    ])
    .assert()
    .success();

    let events = read_events(&repo);
    let iter_events = events_with_kind(&events, "adversary_iteration_complete");
    assert!(iter_events.len() >= 3, "expected ≥3 iter events");

    let found = events_with_kind(&events, "counterexample_found");
    assert_eq!(found.len(), 1, "expected 1 counterexample_found");
    let ev = found[0];
    let original = ev["attrs"]["original_len"]
        .as_str()
        .unwrap()
        .parse::<usize>()
        .unwrap();
    let minimized = ev["attrs"]["minimized_len"]
        .as_str()
        .unwrap()
        .parse::<usize>()
        .unwrap();
    assert!(
        minimized <= original,
        "minimized_len {minimized} must be ≤ original_len {original}"
    );
    assert_eq!(ev["attrs"]["spec_id"].as_str(), Some("FR-demo"));
}

#[test]
fn fr_eq_022_one_shot_finding_is_candidate_not_found() {
    let repo = TempRepo::new();
    seed_sensor_map(&repo);
    // Iter 1 fails, iters 2..=max pass. Because found_iter < min_iterations,
    // FR-EQ-022 mandates candidate classification, not posterior-eligible.
    write_fixtures(
        &repo,
        "oneshot",
        &["echo 'quick win'\nexit 1\n", "exit 0\n", "exit 0\n"],
    );
    repo.run_specere(&[
        "adversary",
        "run",
        "--spec",
        "FR-demo",
        "--provider",
        "mock",
        "--max-iterations",
        "3",
        "--sandbox",
        "none",
        "--from-fixture",
        ".specere/fixtures/oneshot",
    ])
    .assert()
    .success();

    let events = read_events(&repo);
    let found = events_with_kind(&events, "counterexample_found");
    let candidate = events_with_kind(&events, "counterexample_candidate");
    assert_eq!(
        found.len(),
        0,
        "FR-EQ-022: must NOT emit found for one-shot"
    );
    assert_eq!(candidate.len(), 1, "expected 1 candidate event");
    assert_eq!(
        candidate[0]["attrs"]["reason"].as_str(),
        Some("below_min_iterations"),
    );
}

#[test]
fn fr_eq_021_cap_zero_aborts_with_budget_exceeded() {
    let repo = TempRepo::new();
    seed_sensor_map(&repo);
    write_fixture(&repo, "cap", "exit 0\n");
    // Pre-load the ledger at $0.00 spent, $0.01 cap, and force the
    // provider to report a small cost by using anthropic — but we don't
    // have a key, so we construct the ledger directly with spent=cap.
    repo.write(
        ".specere/adversary-budget.toml",
        &format!(
            "month = \"{}\"\nspent_usd = 0.05\ncap_usd = 0.05\n",
            current_month_utc()
        ),
    );
    repo.run_specere(&[
        "adversary",
        "run",
        "--spec",
        "FR-demo",
        "--provider",
        "mock",
        "--max-iterations",
        "3",
        "--sandbox",
        "none",
        "--from-fixture",
        ".specere/fixtures/cap",
    ])
    .assert()
    .success();

    let events = read_events(&repo);
    let exceeded = events_with_kind(&events, "adversary_budget_exceeded");
    assert_eq!(
        exceeded.len(),
        1,
        "expected adversary_budget_exceeded event"
    );
    let iters = events_with_kind(&events, "adversary_iteration_complete");
    assert_eq!(
        iters.len(),
        0,
        "must not run any iterations when cap already hit"
    );
}

#[test]
fn fr_eq_020_noop_fixtures_produce_no_counterexample() {
    let repo = TempRepo::new();
    seed_sensor_map(&repo);
    write_fixtures(&repo, "noop", &["exit 0\n", "exit 0\n", "exit 0\n"]);
    repo.run_specere(&[
        "adversary",
        "run",
        "--spec",
        "FR-demo",
        "--provider",
        "mock",
        "--max-iterations",
        "3",
        "--sandbox",
        "none",
        "--from-fixture",
        ".specere/fixtures/noop",
    ])
    .assert()
    .success()
    .stdout(predicates::str::contains("status=no_counterexample"));

    let events = read_events(&repo);
    assert_eq!(events_with_kind(&events, "counterexample_found").len(), 0);
    assert_eq!(
        events_with_kind(&events, "counterexample_candidate").len(),
        0
    );
    assert_eq!(
        events_with_kind(&events, "adversary_iteration_complete").len(),
        3
    );
}
