# Feature Specification: Release Infrastructure (v0.2.0 cut)

**Feature Branch**: `005-release-infra`
**Created**: 2026-04-18
**Status**: Draft
**Input**: User description: "Wire cargo-dist + release.yml so the pending Phase 1 merge (ae7b4c4) can be cut as v0.2.0. Plan.md's 'cargo-dist already configured' assumption was false; this spec makes the release infrastructure real."

> **Source.** `docs/upcoming.md` priority 1 ("release-infra"); closes the release-engineering deferrals in `docs/specere_v1.md` §5 Phase 1.
> **Governance.** `.specify/memory/constitution.md` — rules 1-10 apply but scope is infra-only; no FR changes, no crate-surface changes. Principle III (reversibility) still applies to the release process: a bad tag must be deletable without corrupting the repo.

## User Scenarios & Testing *(mandatory)*

### User Story 1 — Maintainer cuts v0.2.0 with one command (Priority: P1)

The repo owner has just merged a release-worthy PR to `main`. They want to produce a versioned release — git tag, GitHub Release entry, cross-platform pre-built binaries, crates.io publish (opt-in later) — without hand-running `cargo build --target x86_64-pc-windows-msvc && ...` five times or manually uploading asset files. After this story, a tag push triggers the release workflow and every artifact lands on the GitHub Release page automatically.

**Why this priority**: This is the single remaining blocker for cutting v0.2.0. Without it, the Phase 1 work on `main` is frozen at "merged but not released." All of `docs/upcoming.md` priorities 2+ are implicitly blocked by an unreleased v0.2.0 because users have no way to install the Phase-1 fixes without building from source.

**Independent Test**: On a disposable branch, bump version, tag `v0.2.0-testrelease`, push. The release workflow runs, cross-platform binaries are built, GitHub Release entry is created with the binaries attached. Delete the tag + Release afterward to clean up.

**Acceptance Scenarios**:

1. **Given** `main` is at a commit with `workspace.package.version = "0.2.0"`, **When** a maintainer pushes a `v0.2.0` tag pointing at that commit, **Then** the release workflow runs and produces GitHub Release assets for at least `{x86_64-unknown-linux-gnu, aarch64-unknown-linux-gnu, x86_64-apple-darwin, aarch64-apple-darwin, x86_64-pc-windows-msvc}`.
2. **Given** the same tag push, **When** the workflow completes, **Then** a GitHub Release entry exists at `https://github.com/laiadlotape/specere/releases/tag/v0.2.0` with the CHANGELOG `## [0.2.0]` section as the body.
3. **Given** a tag push on a commit where `Cargo.toml` version does **not** match the tag name, **When** the release workflow validates, **Then** it fails fast with a clear error before publishing any asset.
4. **Given** a mis-cut tag (e.g. `v0.2.0-typo`), **When** the maintainer deletes the tag + Release, **Then** the repo is bit-identical to its pre-tag state (principle III applied to releases).

---

### User Story 2 — Contributors see the release plumbing and can reproduce a release locally (Priority: P2)

A contributor wants to understand what the release workflow does before proposing a change to it. They read `docs/release.md` (new) and `cargo-dist`'s generated config in `Cargo.toml`. They can run `dist plan` and `dist build` locally on their own fork for any single target and inspect the output. Release-engineering is not a black box.

**Why this priority**: Constitution rule 10 ("parse narrowly") applies in spirit — every piece of release plumbing must be explicit and inspectable; no hidden magic.

**Acceptance Scenarios**:

1. **Given** a fresh clone of the repo, **When** a contributor runs `dist plan` locally, **Then** they see the target list + artifact list that the CI would produce, without any network call or external config lookup.
2. **Given** a contributor who reads `docs/release.md`, **When** they finish the doc, **Then** they know (a) how to run the release locally, (b) what `[workspace.metadata.dist]` fields govern the build, (c) how to bump the version, (d) how to move the CHANGELOG section.

---

### Edge Cases

- What if the release workflow runs against a tag on a branch *other than* `main`? The workflow refuses — releases are cut from `main` only.
- What if `cargo-dist` upstream has a breaking change between this spec and the next release? Version-pin the `cargo-dist` version in `release.yml`; upgrades are their own PR.
- What if a target fails to build (e.g., the Windows builder goes stale)? The other targets still publish; the failed target's CI log is the actionable record.
- What if the CHANGELOG `## [0.2.0]` section is missing at tag time? Release workflow refuses — same class of gate as FR-P1-008 (parse narrowly).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-RI-001**: The repository MUST contain a `.github/workflows/release.yml` file that triggers on tag push matching the pattern `v*.*.*` and produces a GitHub Release with cross-platform binary assets.
- **FR-RI-002**: The `Cargo.toml` workspace MUST contain a `[workspace.metadata.dist]` section declaring at least the five target triples listed in `docs/roadmap/31_specere_scaffolding.md §2` (two Linux, two macOS, one Windows).
- **FR-RI-003**: The release workflow MUST refuse to publish if `workspace.package.version` in `Cargo.toml` does not match the tag name (stripping the leading `v`).
- **FR-RI-004**: The release workflow MUST refuse to publish if `CHANGELOG.md` does not contain a `## [<version>]` section for the tagged version.
- **FR-RI-005**: The release workflow MUST refuse to run on a tag whose commit is not reachable from `main`.
- **FR-RI-006**: A `docs/release.md` file MUST document the tag-cut procedure (version bump, CHANGELOG move, tag push, tag delete for rollback) and the local reproduction procedure (`dist plan`, `dist build`).
- **FR-RI-007**: This feature's merge MUST bump `workspace.package.version` from `0.2.0-dev` to `0.2.0` and move the `[Unreleased]` CHANGELOG entries to `[0.2.0] - 2026-MM-DD`.

### Key Entities

- **Release tag**: an annotated git tag matching `v<semver>`; the only trigger for `release.yml`.
- **`[workspace.metadata.dist]`**: the cargo-dist config block in `Cargo.toml`; source of truth for build targets, installer scripts, and artifact naming.
- **CHANGELOG section**: a `## [<version>] - <date>` block in `CHANGELOG.md`; its body becomes the GitHub Release notes.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A maintainer can cut v0.2.0 in under 10 minutes of their own time from a clean `main`: bump version, move CHANGELOG section, commit, push tag, wait for CI. No hand-assembling of binaries.
- **SC-002**: The release workflow produces exactly 5 binary artifacts (one per target triple) for any `v*.*.*` tag push on `main`. Zero artifacts for tag pushes on other branches.
- **SC-003**: The tag-cut procedure is documented in `docs/release.md` at a depth sufficient for a first-time contributor to reproduce it.
- **SC-004**: A mis-cut tag can be fully rolled back (`git tag -d`, `git push --delete origin v...`, GitHub Release deletion) with zero residue in the repo.
- **SC-005**: `dist plan` in a fresh clone prints the same target + artifact list the CI would produce, with no network access required.

## Assumptions

- `cargo-dist` is installed at a pinned version via `cargo install cargo-dist@<pin>` in the workflow; drift between local and CI versions is out of scope for this feature.
- GitHub provides the runners for the five target triples; self-hosted runners are out of scope.
- crates.io publishing is deferred to a follow-up spec (`release-infra-crates-io`); this feature only covers GitHub Releases + binaries.
- SC-001's "10 minutes" is wall-clock for the maintainer; CI run time is not counted.
