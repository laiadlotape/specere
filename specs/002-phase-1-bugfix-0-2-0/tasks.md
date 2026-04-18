# Tasks: Phase 1 Bugfix Release (0.2.0)

**Feature**: Phase 1 Bugfix Release (0.2.0)
**Branch**: `002-phase-1-bugfix-0-2-0`
**Spec**: [spec.md](./spec.md) · **Plan**: [plan.md](./plan.md) · **Data model**: [data-model.md](./data-model.md)
**Input**: 9 FRs (FR-P1-001 … FR-P1-009), 8 SCs, 4 user stories.

## Execution rules

- Tasks execute **top to bottom**; within a phase, `[P]` marks tasks that can run concurrently (different files, no cross-task dependencies on incomplete work).
- `[USN]` marks the user-story this task delivers (from `spec.md`). Setup, Foundational, and Polish phases carry no story label.
- Every task names exact file paths. No vague scope.
- TDD discipline: each FR's integration test is written **before** the implementation task it pins. The test starts red; implementation turns it green.
- Do not mark a task `[X]` until all of: (a) the code compiles with `cargo build`; (b) the test it targets passes; (c) `cargo clippy -- -D warnings` is clean on the changed files.

---

## Phase 1 — Setup

- [ ] T001 Add `serde_yaml = "0.9"` to workspace `Cargo.toml` `[workspace.dependencies]` block and re-export through `specere-core/Cargo.toml`.
- [ ] T002 [P] Add `sha2 = "0.10"` to workspace dependencies and wire it into `specere-manifest/Cargo.toml`.
- [ ] T003 [P] Add dev-dependencies `tempfile = "3"`, `assert_cmd = "2"`, `predicates = "3"` to workspace-level `[workspace.dependencies]` with the `optional = false` default, and reference them from `crates/specere-units/Cargo.toml` under `[dev-dependencies]`.
- [ ] T004 Create directory `crates/specere-units/tests/common/` with an empty `mod.rs` — stub for the shared `TempRepo` fixture (populated in T007).
- [ ] T005 Bump workspace package version to `0.2.0-dev` in `Cargo.toml` `[workspace.package].version`.

## Phase 2 — Foundational (blocks every user story)

- [ ] T006 Extend `specere_core::Error` in `crates/specere-core/src/lib.rs` with three variants: `AlreadyInstalledMismatch { unit: String, files: Vec<PathBuf> }`, `ParseFailure { path: PathBuf, format: String, inner: String }`, `DeletedOwnedFile { unit: String, path: PathBuf }`. Derive `Error`, `Debug`, wire into existing `Result<T>` alias. Map each variant to a stable exit code in `crates/specere/src/main.rs` following `contracts/cli.md` (2, 3, 4 respectively).
- [ ] T007 Implement the shared `TempRepo` fixture in `crates/specere-units/tests/common/mod.rs`: `TempRepo::new() -> Self` creates a `TempDir`, runs `git init`, makes an initial empty commit, returns a struct exposing `.path()`, `.run_specere(args: &[&str])` (spawning the `specere` binary via `assert_cmd::Command`), and `.sha256_of(path: &str)`. Also expose `.corrupt_file(path, bytes)` for FR-P1-008 tests.
- [ ] T008 Extend the manifest schema in `crates/specere-manifest/src/lib.rs` — `UnitInstallConfig` struct gains `branch_name: Option<String>` and `branch_was_created_by_specere: bool` (default `false`). Ensure `serde` tolerates missing fields in old manifests (add `#[serde(default)]`). Update `save()` to emit them only when `Some`/`true`.
- [ ] T009 [P] Add a `specere-markers::yaml_block_fence` module in `crates/specere-markers/src/lib.rs` implementing the add/remove protocol described in `contracts/extensions-mutation.md` §Add/§Remove. Inputs: file contents (`&str`), unit id, verb, entry text. Outputs: new contents (`String`) or `Error::MarkerUnpaired`. No YAML re-serialization.
- [ ] T010 [P] Add a `specere-markers::text_block_fence` module (sibling of T009) for plain-text files (`.gitignore`): marker lines are `# <!-- specere:begin <id> -->` / `# <!-- specere:end <id> -->`. Same add/remove semantics.

---

## Phase 3 — User Story 1: Install on a git-backed repo produces a working feature branch (P1)

**Goal**: Satisfy FR-P1-001, FR-P1-002, FR-P1-007. Independent test: after `specere add speckit` on a git fixture, `git branch --show-current` is `000-baseline` (or override).

- [ ] T011 [US1] Write integration test `crates/specere-units/tests/fr_p1_001_no_no_git.rs`: uses `TempRepo`, asserts that `specere add speckit --dry-run` produces a `Plan` whose speckit-init invocation does NOT contain `--no-git`. Expect test to fail red at this point.
- [ ] T012 [US1] Write integration test `crates/specere-units/tests/fr_p1_002_branch_auto_create.rs` covering the four acceptance scenarios from `spec.md §User Story 1`: default branch is `000-baseline`; `$SPECERE_FEATURE_BRANCH=alpha-baseline` override wins; non-git fallback; existing-branch reuse (no error, switches to it).
- [ ] T013 [US1] Write integration test `crates/specere-units/tests/fr_p1_007_manifest_branch_record.rs`: after `specere add speckit` on a git fixture, parse `.specere/manifest.toml` and assert `units[id=speckit].install_config.branch_name == "000-baseline"` and `branch_was_created_by_specere == true`. Assert the field is absent on a non-git target.
- [ ] T014 [US1] Implement the branch logic in `crates/specere-units/src/speckit.rs`: in `preflight`, detect `.git/` existence; compute `branch_name` from CLI `--branch` flag → `$SPECERE_FEATURE_BRANCH` → `"000-baseline"`; detect whether the branch already exists (`git rev-parse --verify $branch`). In `install`, drop `--no-git` iff git-detected; after `specify init` returns, invoke `git checkout -b $branch` iff `branch_was_created_by_specere`, else `git checkout $branch`. Record both fields in the manifest via T008's extended schema.
- [ ] T015 [US1] Add the CLI `--branch` flag to `crates/specere/src/main.rs` clap definition under the `add` subcommand. Flag is `speckit`-only; reject with usage error for other unit ids. Ensure T011, T012, T013 turn green.

**Checkpoint**: All three FR-P1-001/002/007 tests pass. User Story 1 deliverable complete; SC-001 becomes checkable by re-running the quickstart.

---

## Phase 4 — User Story 2: Re-install never silently overwrites user edits (P1)

**Goal**: Satisfy FR-P1-003. Independent test: installing then editing an owned file, then re-running `add` without flag must refuse with exit 2; with `--adopt-edits` must adopt.

- [ ] T016 [US2] Write integration test `crates/specere-units/tests/fr_p1_003_sha_diff_gate.rs` covering all three acceptance scenarios from `spec.md §User Story 2`: refuse on edit (exit 2, stderr naming file); accept on `--adopt-edits`; no-op on clean re-install. Plus the clarified deletion case: delete an owned file, run `specere add filter-state --adopt-edits`, expect exit 4 and a message citing `specere remove … && specere add …`.
- [ ] T017 [US2] Extend `AddUnit::preflight` default implementation (or a new helper `check_sha_gate` in `specere-core`) to: walk the manifest's `units[id].files`; for each, SHA256 the on-disk content; if any differ from `sha256_post`, return `Error::AlreadyInstalledMismatch`; if any file is missing and `--adopt-edits` is set, return `Error::DeletedOwnedFile`.
- [ ] T018 [US2] Add the `--adopt-edits` flag to `crates/specere/src/main.rs` clap `add` subcommand; thread into `Ctx`. When set and the preflight raises `AlreadyInstalledMismatch`, the install function replaces each matching file's `sha256_post` with the current on-disk hash and sets `owner = "user-edited-after-install"` instead of rewriting. Write the updated manifest.
- [ ] T019 [US2] [P] Update `specere add speckit` error message formatting in `crates/specere/src/main.rs` per `contracts/cli.md §Stderr format`: one-line summary, `help:` line with the `--adopt-edits` suggestion, `affected:` line naming the file(s).

**Checkpoint**: FR-P1-003 test green; SC-003 (refuse under 200 ms) benchmarked in the test body via `Instant::now`.

---

## Phase 5 — User Story 3: `claude-code-deploy` leaves a clean git surface and a live hook (P2)

**Goal**: Satisfy FR-P1-004, FR-P1-005. Independent test: after install, `.gitignore` has a marker-fenced block and `.specify/extensions.yml` has a single specere `after_implement` entry.

- [ ] T020 [US3] Write integration test `crates/specere-units/tests/fr_p1_004_gitignore_marker.rs` covering the four acceptance scenarios from `spec.md §User Story 3` (FR-P1-004-related rows): creates-when-absent; preserves pre-existing lines; strips on remove; refuses on user-edited fence.
- [ ] T021 [US3] Write integration test `crates/specere-units/tests/fr_p1_005_hook_registration.rs` covering the FR-P1-005 rows: writes exactly one entry; removes cleanly; does not disturb user-added hooks.
- [ ] T022 [US3] Create `crates/specere-units/src/deploy/mod.rs` (new module): the `ClaudeCodeDeploy` unit implementing `AddUnit`. `id() = "claude-code-deploy"`, `kind = native`. Wire into `crates/specere-units/src/lib.rs` registry.
- [ ] T023 [US3] [P] Implement `crates/specere-units/src/deploy/gitignore.rs`: `install_gitignore_entry(ctx, unit_id, line)` appends `.claude/settings.local.json` inside a `text_block_fence` from T010; `remove_gitignore_entry(ctx, unit_id)` strips it. Add the resulting manifest `markers` entry.
- [ ] T024 [US3] [P] Implement `crates/specere-units/src/deploy/extensions.rs`: `install_hook(ctx, unit_id, verb, entry)` uses `yaml_block_fence` from T009 to insert the hook entry under `hooks.after_implement`. `remove_hook(ctx, unit_id, verb)` strips it. The hook entry text matches `contracts/extensions-mutation.md §Marker convention`.
- [ ] T025 [US3] [P] Embed the three specere skill files (`specere-observe-implement`, `specere-review-check`, `specere-review-drain`) from `.claude/skills/specere-*/SKILL.md` into the `deploy` unit via `include_str!`, and write them to `.claude/skills/specere-*/SKILL.md` on install. Record each in `units.files` with role `"skill"`.
- [ ] T026 [US3] Wire everything in `ClaudeCodeDeploy::install` — gitignore entry + hook entry + three skill files — and record all files + markers in a single atomic manifest save.

**Checkpoint**: FR-P1-004 and FR-P1-005 tests green. Manual quickstart §Step 3 passes.

---

## Phase 6 — User Story 4: `remove` is a byte-identical inverse (P2)

**Goal**: Satisfy FR-P1-006. Also satisfies FR-P1-008 as a side-effect (remove must refuse on malformed input).

- [ ] T027 [US4] Write integration test `crates/specere-units/tests/fr_p1_006_remove_round_trip.rs`: snapshot SHA256 of `.gitignore` and `.specify/extensions.yml` pre-install; install `claude-code-deploy`; remove `claude-code-deploy`; re-SHA; assert equality. Also covers the "user added unrelated content in between" subscenarios from spec §User Story 4.
- [ ] T028 [US4] Write integration test `crates/specere-units/tests/fr_p1_008_malformed_file_refuse.rs`: corrupt `.specify/extensions.yml` (mismatched quote), attempt `specere add claude-code-deploy`, assert exit 3 and stderr names the file. Repeat for `.specere/manifest.toml` (corrupt TOML) and `.specify/workflows/workflow-registry.json` (corrupt JSON).
- [ ] T029 [US4] Implement `ClaudeCodeDeploy::remove` in `crates/specere-units/src/deploy/mod.rs`: invoke T010's `text_block_fence::remove` on `.gitignore`; invoke T009's `yaml_block_fence::remove` on `.specify/extensions.yml`; delete all `units.files` (respecting `owner` — `user-edited-after-install` is preserved with a warning); remove the `units` entry from the manifest.
- [ ] T030 [US4] [P] Extend `Speckit::remove` with the `--delete-branch` flag behavior from `contracts/cli.md §specere remove`: if flag is set AND `branch_was_created_by_specere == true` AND working tree is clean, run `git branch -D $branch`. If dirty, return `Error::BranchDirty` (exit 6). If `!branch_was_created_by_specere`, return `Error::BranchNotOurs` (exit 7).
- [ ] T031 [US4] [P] Add parse-safety checks at every file-read site: `.specify/extensions.yml` (serde_yaml), `.specere/manifest.toml` (toml), `.specere/sensor-map.toml` (toml), `.specify/workflows/workflow-registry.json` (serde_json). On parse failure, raise `Error::ParseFailure`. The plain-text `.gitignore` file is always parseable as UTF-8; if UTF-8 validation fails, also raise `ParseFailure` with `format = "utf-8"`.

**Checkpoint**: FR-P1-006 and FR-P1-008 tests green. SC-004 is measurable: run the test, assert zero-diff.

---

## Phase 7 — Polish & release engineering

- [ ] T032 [P] Update `CHANGELOG.md` under `## [Unreleased]` with one entry per FR — bugfix listings pointing at the GitHub issue number (create issue links if available, otherwise `docs/specere_v1.md §4` references). Move to a `## [0.2.0] - 2026-MM-DD` section once the tag is cut.
- [ ] T033 [P] Add a `--help` text block to `specere add --help` and `specere remove --help` that mentions the new flags (`--adopt-edits`, `--branch`, `--delete-branch`) and cites the corresponding FR id from this spec (for discoverability).
- [ ] T034 [P] Document the new manifest fields in `docs/` (either `docs/add-unit-contract.md` if present, or a new `docs/manifest-schema.md`) — the `contracts/manifest.md` text is the canonical source, `docs/` just links to it.
- [ ] T035 [P] Add a short `docs/lessons/0.2.0-usability.md` as a placeholder — filled in after the SC-008 aspirational usability session if one occurs; empty otherwise.
- [ ] T036 Run `cargo fmt --all` + `cargo clippy --workspace -- -D warnings` + `cargo test --workspace` — all must pass clean before the tag. CI already runs this; the task is to fix any local lints the CI surfaces.
- [ ] T037 Generate the v0.2.0 release via `cargo dist build` in dry-run mode; verify `Cargo.toml` workspace version is `0.2.0` (not `-dev`). Tag push + GitHub release are the user's decision (not automated here).

---

## Dependencies

```text
T001..T005  (Setup) ──────────────────────────────────────┐
                                                           ▼
T006..T010  (Foundational) ──► block every US phase below ─┐
                                                            ▼
Phase 3 (US1, P1): T011 [P] T012 [P] T013 [P] → T014 → T015
Phase 4 (US2, P1): T016 → T017 → T018 → T019 [P]
Phase 5 (US3, P2): T020 [P] T021 [P] → T022 → T023 [P] T024 [P] T025 [P] → T026
Phase 6 (US4, P2): T027 [P] T028 [P] → T029 → T030 [P] T031 [P]
Phase 7 (Polish):  T032 [P] T033 [P] T034 [P] T035 [P] → T036 → T037
```

User stories are intentionally independent at the integration-test level. US1 and US2 are P1 — MVP for v0.2.0 is just those two if timebox slips.

## Parallel execution examples

- **Setup burst**: T002 and T003 can run concurrently with T001 (distinct Cargo.toml sections).
- **Foundational burst**: T009 and T010 are independent (two different fence types in different files).
- **US1 test burst**: T011, T012, T013 write three independent test files — parallel-safe.
- **US3 implementation burst**: T023, T024, T025 each own a different file in `src/deploy/` — parallel-safe once T022 lands.
- **Polish burst**: T032–T035 all edit independent doc files.

## Independent test criteria per story

- **US1**: `cargo test -p specere-units fr_p1_001 fr_p1_002 fr_p1_007` passes against a clean fixture repo.
- **US2**: `cargo test -p specere-units fr_p1_003` passes including both edit and deletion paths.
- **US3**: `cargo test -p specere-units fr_p1_004 fr_p1_005` passes; quickstart §Step 3 green.
- **US4**: `cargo test -p specere-units fr_p1_006 fr_p1_008` passes; quickstart §Step 4 shows empty diff.

## Suggested MVP scope

**Strict MVP**: US1 + US2 (both P1) — closes the two most user-visible bugs (branch-check trap, silent overwrite). Releasable as v0.2.0-rc1 if the P2 work slips.

**Full v0.2.0**: all four user stories complete, including the round-trip round-trip guarantee (US4). This is the default target.

## Implementation strategy

TDD per story: red test first, then implementation, then refactor. Each phase ends with a cargo-test-green checkpoint. Do not start Phase N+1 until Phase N is fully green.

## Format validation

All 37 tasks follow the `- [ ] TNNN [P?] [USN?] description with file path` format. Every US3+ task carries a `[USN]` label; Setup / Foundational / Polish tasks do not. Every implementation task cites at least one concrete file path from `plan.md §Project Structure`.
