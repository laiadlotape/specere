# Tasks: Release Infrastructure (v0.2.0 cut)

**Feature**: 005-release-infra · **Spec**: [spec.md](./spec.md) · **Plan**: [plan.md](./plan.md)

## Execution rules

- Tasks execute top to bottom; `[P]` marks parallelizable.
- Every FR has at least one task that closes it; the task ID cites the FR.
- No cargo-dist invocation until the guard-job layer is drafted — otherwise cargo-dist's generator would overwrite the custom workflow on first re-run.

---

## Phase 1 — Setup

- [ ] T001 Install `cargo-dist@0.31` locally so `dist` is invokable: `cargo install cargo-dist@0.31 --locked`.
- [ ] T002 Verify the install: `dist --version` reports `0.31.x`.

## Phase 2 — Generate base config

- [ ] T003 Run `dist init --yes --installers sh,powershell --hosting github` from the workspace root. This writes `[workspace.metadata.dist]` into `Cargo.toml` and creates `.github/workflows/release.yml`.
- [ ] T004 In `[workspace.metadata.dist]`, override the generated target list to exactly the five triples from `plan.md § Technical Context`. Remove any macOS-x86_64-only legacy entries cargo-dist may have added.
- [ ] T005 In `[workspace.metadata.dist]`, set `pr-run-mode = "skip"` so cargo-dist's own PR-preview jobs don't interfere with our existing CI.
- [ ] T006 Pin cargo-dist in `.github/workflows/release.yml` to `0.31` (cargo-dist regenerates with whatever version is local; pin prevents drift).

## Phase 3 — Guard jobs (FR-RI-003, FR-RI-004, FR-RI-005)

- [ ] T007 Prepend a `guards` job in `release.yml` that runs before any cargo-dist job via `needs:`. Inside, three steps:
    - **tag-version match**: extract the tag name, strip `v`, grep `Cargo.toml` for `version = "$STRIPPED"` under `[workspace.package]`. Fail with an actionable message if mismatch.
    - **CHANGELOG section present**: grep `CHANGELOG.md` for `## \[<version>\]`. Fail with a pointer to `docs/release.md § Tag-cut procedure` on miss.
    - **Tag reachable from main**: `git fetch origin main --depth=0`; `git merge-base --is-ancestor $TAG_SHA origin/main`. Fail with a pointer to the reachable-from-main rule on miss.
- [ ] T008 Wire every cargo-dist job in `release.yml` to `needs: [guards]` so a failed guard blocks the build.

## Phase 4 — Docs (FR-RI-006)

- [ ] T009 Write `docs/release.md` covering: tag-cut procedure (version bump, CHANGELOG move, commit, tag, push); local reproduction (`dist plan`, `dist build --target <triple>`); rollback (tag delete + Release delete + nothing else); the three guard jobs' failure modes.
- [ ] T010 Update `docs/upcoming.md`: strike priority 1 (release-infra) from the active queue; add entry under `## Recently closed` with a one-line pointer to `v0.2.0` tag (when cut).

## Phase 5 — Version + CHANGELOG bump (FR-RI-007)

- [ ] T011 Bump `workspace.package.version` in `Cargo.toml` from `0.2.0-dev` to `0.2.0`.
- [ ] T012 Rename `[Unreleased]` in `CHANGELOG.md` to `[0.2.0] - 2026-04-18`; add a fresh empty `## [Unreleased]` at the top.
- [ ] T013 Update `Cargo.lock` by running `cargo build` (picks up the new version).

## Phase 6 — End-to-end dry run

- [ ] T014 `dist plan` in the workspace root. Expect: 5 target-triple artifacts listed, 2 installer scripts (`sh`/`powershell`). Capture the output in the PR description for review.
- [ ] T015 After PR merge (NOT automated from this feature): push a throwaway `v0.2.0-testrelease` tag on a disposable branch off `main`, confirm the workflow runs end-to-end, delete the test tag + Release. Separate from PR merge because tag pushes trigger the workflow only post-merge.
- [ ] T016 Cut the real `v0.2.0` tag on the merge commit. Wait for release.yml; confirm 5 binaries + a Release page with the `## [0.2.0]` body.

---

## Dependencies

```
T001 → T002 → T003 → T004 → T005 → T006 → T007 → T008 → T009 [P] T010 [P] → T011 → T012 → T013 → T014 → T015 → T016
```

T009 and T010 are the only parallelizable pair (independent doc files). Everything else must be sequential because cargo-dist's init writes files that later tasks edit.

## Independent verification per FR

| FR | Verified by | How |
|---|---|---|
| FR-RI-001 | T003, T007, T008 | `.github/workflows/release.yml` exists and runs on `v*.*.*` tags |
| FR-RI-002 | T004 | `[workspace.metadata.dist].targets` enumerates the five triples |
| FR-RI-003 | T007 step 1 | guard fails on tag/version mismatch |
| FR-RI-004 | T007 step 2 | guard fails on missing CHANGELOG section |
| FR-RI-005 | T007 step 3 | guard fails on unreachable-from-main tag |
| FR-RI-006 | T009 | `docs/release.md` covers all four procedures |
| FR-RI-007 | T011, T012 | version + CHANGELOG bumps in the same commit |

## Scope boundary

- **T015 and T016 are NOT part of the PR this feature produces.** They happen after the release-infra PR merges to `main`. The PR itself ships wiring + docs + version bump; the first real tag push is a follow-up maintainer action (explicit human authorization per core_theory §4 divergence-adjudication).
