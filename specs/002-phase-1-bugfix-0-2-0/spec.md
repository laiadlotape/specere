# Feature Specification: Phase 1 Bugfix Release (0.2.0)

**Feature Branch**: `002-phase-1-bugfix-0-2-0`
**Created**: 2026-04-18
**Status**: Draft
**Input**: User description: "Phase 1 bugfix release (0.2.0): close all six FRs — P1-001..P1-006 — for the speckit wrapper unit and the claude-code-deploy unit."

## Clarifications

### Session 2026-04-18
- Q: Where in `.specere/manifest.toml` does FR-P1-007 record the auto-created branch name? → A: Unit-level, as `units[id=speckit].install_config.branch_name` alongside a boolean `branch_was_created_by_specere`.
- Q: What does `specere add <unit> --adopt-edits` do when an owned file has been outright DELETED (not just edited)? → A: Refuse. `--adopt-edits` is scoped to content changes; deletion is a structural change — the user runs `specere remove` then `specere add` instead.
- Q: Is SC-008 (one-shot external-developer usability check before v0.2.0) blocking or aspirational? → A: Aspirational. The CHANGELOG documents the outcome if performed; the tag does not block on it. The other success criteria remain blocking.
- Q: Does FR-P1-008's "refuse-on-parse-failure" rule cover TOML and JSON files, or only YAML and plain text? → A: All declared formats the harness parses — YAML (`extensions.yml`), TOML (`.specere/*.toml`), JSON (`.specify/workflows/workflow-registry.json`), and plain text (`.gitignore`).

## User Scenarios & Testing *(mandatory)*

The four user stories below cover the four confirmed bugs from the first dogfood pass (recorded in `docs/specere_v1.md` §4 and the project-memory entry `feedback_adoption_bugs.md`). Each story is an independently testable vertical slice and each ships alongside regression tests that pin the bug.

### User Story 1 - Install on a git-backed repo produces a working feature branch (Priority: P1)

A developer runs the SpecERE installer on an existing git repository and expects the scaffolded SpecKit state to be immediately usable by Claude Code's `/speckit-*` slash commands — most notably `/speckit-clarify`, which fails today because the installer scaffolds specs outside a feature branch.

**Why this priority**: This is the headline bug from the first `/specere-adopt` dogfood pass. Until it is fixed, the SpecERE install is effectively broken on every real-world repository (git-backed is the default case). Nothing else in Phase 1 matters if users hit a "Not on a feature branch" error the first time they try to clarify a spec.

**Independent Test**: On a clean clone of a git-backed fixture repo, run the `speckit` unit installer, then run `/speckit-clarify` against a minimal spec. The slash command must proceed past the branch check. Testable without any other unit installed.

**Acceptance Scenarios**:

1. **Given** a target repository that contains a `.git/` directory, **When** the `speckit` unit is installed, **Then** the installer does not request that the underlying scaffolder operate in no-git mode.
2. **Given** a target repository that contains a `.git/` directory and a user who did not override the branch name, **When** the `speckit` unit install completes, **Then** the working tree is checked out on a feature branch named `000-baseline`.
3. **Given** a target repository that contains a `.git/` directory and a user who set the branch-name override to `my-branch`, **When** the `speckit` unit install completes, **Then** the working tree is checked out on a feature branch named `my-branch`.
4. **Given** a target repository that does not contain a `.git/` directory, **When** the `speckit` unit is installed, **Then** the installer falls back to no-git mode and does not attempt any branch operation.
5. **Given** a SpecERE-scaffolded repository on its newly created feature branch, **When** a `/speckit-clarify` equivalent is invoked, **Then** the command does not fail with a branch-check error.
6. **Given** a completed install that auto-created a feature branch, **When** the install manifest is read, **Then** the manifest records the branch name so a later remove operation can reference it.

---

### User Story 2 - Re-installing a unit never silently overwrites the user's local edits (Priority: P1)

A developer who has hand-edited a file that SpecERE previously installed (for example, a template or workflow) re-runs the installer. Today, re-install overwrites their edits without warning. The user expects the installer to detect the divergence, refuse to proceed, and point at an explicit opt-in to adopt the user's edits.

**Why this priority**: Silent data loss is the most damaging class of harness bug. A single occurrence destroys the user's trust in the tool. This is P1 alongside Story 1 because any harness that can eat uncommitted work is worse than no harness at all.

**Independent Test**: Install any unit on a fixture repo, hand-edit one of the files the unit owns, re-run the installer without the adopt-edits flag; assert the installer refuses and the file on disk is unchanged. Then re-run with the adopt-edits flag; assert the installer accepts the file as-is and updates the manifest.

**Acceptance Scenarios**:

1. **Given** a unit has been installed and one of its owned files has since been hand-edited by the user, **When** the same unit is re-installed without the adopt-edits override, **Then** the installer refuses to proceed and names the diverged file in its error message.
2. **Given** the same scenario as above, **When** the same unit is re-installed with the adopt-edits override, **Then** the installer accepts the file's current content as the new owner record and does not overwrite it.
3. **Given** a unit has been installed and none of its owned files have diverged from the manifest record, **When** the same unit is re-installed, **Then** the install completes without requiring the adopt-edits override.
4. **Given** the installer refuses because of a divergence, **When** the user reads the error message, **Then** the message states which files diverged and how to invoke the adopt-edits path.

---

### User Story 3 - `claude-code-deploy` unit leaves a clean git surface and a live hook contract (Priority: P2)

A developer installs the `claude-code-deploy` unit on top of an already-scaffolded SpecKit repo. The unit exists to make Claude Code a first-class deployer: it owns the `.claude/` tree and owns the hook registration in `.specify/extensions.yml`. The install must not leak Claude Code's local settings into git history, and it must register exactly one `after_implement` hook that points at the SpecERE observer (even before the observer binary itself ships in Phase 3).

**Why this priority**: This closes the two remaining bugs from the first dogfood pass (`.claude/settings.local.json` not gitignored; `.specify/extensions.yml` never produced). Without these fixes, any team using Claude Code will accidentally commit machine-local settings, and Phase 3's observer will have no hook to latch onto. P2 rather than P1 because the failure modes are recoverable (users can add their own gitignore entry), whereas Story 1 and Story 2 block first use.

**Independent Test**: On a fixture repo that already has the `speckit` unit installed, install the `claude-code-deploy` unit. Verify that `.gitignore` now contains an entry for the Claude Code local-settings file inside a marker-fenced block, and that `.specify/extensions.yml` contains exactly one enabled `after_implement` hook naming the SpecERE implement-observer command.

**Acceptance Scenarios**:

1. **Given** a target repository with no pre-existing gitignore entry for the Claude Code local-settings file, **When** the `claude-code-deploy` unit is installed, **Then** the `.gitignore` contains an entry for `.claude/settings.local.json` inside a SpecERE marker-fenced block owned by the `claude-code-deploy` unit.
2. **Given** a target repository whose `.gitignore` already contains content unrelated to SpecERE, **When** the `claude-code-deploy` unit is installed, **Then** the pre-existing `.gitignore` content is preserved verbatim and only the marker-fenced block is added.
3. **Given** a target repository with no pre-existing `.specify/extensions.yml`, **When** the `claude-code-deploy` unit is installed, **Then** the file is created and contains exactly one entry under the `after_implement` hook list, pointing at the SpecERE implement-observer command.
4. **Given** a target repository whose `.specify/extensions.yml` already contains unrelated hooks authored by the user, **When** the `claude-code-deploy` unit is installed, **Then** the pre-existing hooks are preserved and exactly one new `after_implement` entry is added for the SpecERE implement-observer.
5. **Given** the hook entry is written by the installer, **When** the entry is inspected, **Then** it declares a `description` field, an explicit `enabled: true`, and is marked mandatory (not optional).

---

### User Story 4 - Removing `claude-code-deploy` reverses every change it made, and only those (Priority: P2)

A developer decides to uninstall the `claude-code-deploy` unit. They expect the removal to strip exactly the artifacts the unit owned — the hook entry in `.specify/extensions.yml`, the marker-fenced block in `.gitignore`, and any skills installed under `.claude/skills/` — and to leave every other line in those shared files untouched.

**Why this priority**: This is the uninstall side of Story 3, and together they form the round-trip invariant the harness constitution requires: every install must be reversible to a bit-identical state modulo user edits. Without a working uninstall, the "bug backlog" from the first dogfood pass is not really closed — users would just be unable to recover.

**Independent Test**: Run the round-trip: install `claude-code-deploy`, then remove it. Assert that `.specify/extensions.yml` and `.gitignore` are bit-identical to their pre-install content (any unrelated lines added by the user between install and remove are preserved).

**Acceptance Scenarios**:

1. **Given** the `claude-code-deploy` unit has been installed and nothing else has changed, **When** the unit is removed, **Then** the hook entry written at install time is gone from `.specify/extensions.yml`.
2. **Given** the same scenario, **When** the unit is removed, **Then** the marker-fenced block the unit added to `.gitignore` is gone, including the marker lines themselves.
3. **Given** a user added unrelated hooks to `.specify/extensions.yml` after the unit was installed, **When** the unit is removed, **Then** those user-added hooks are preserved verbatim.
4. **Given** a user added unrelated gitignore lines outside the marker-fenced block, **When** the unit is removed, **Then** those lines are preserved verbatim.
5. **Given** an install followed by a remove with no user edits in between, **When** the two shared files are compared against their pre-install snapshots, **Then** they are bit-identical.

---

### Edge Cases

- What happens if the target repository is a git repo but the HEAD is detached at install time? The installer must fail with an actionable error instead of silently leaving the user in an unnamed state, and it must not proceed to scaffold anything.
- What happens if the user has an uncommitted working tree when the installer would create a feature branch? The branch creation still proceeds (standard git semantics carry the dirty changes onto the new branch), but the installer must surface the fact before committing manifest changes.
- What happens when `.specify/extensions.yml` exists but is syntactically invalid? The installer must refuse to proceed on that file rather than rewrite it blindly, and must not overwrite the user's (probably in-progress) edit.
- What happens when `.gitignore` already contains an entry for `.claude/settings.local.json` outside a SpecERE marker block? The installer must not add a duplicate entry; it may either leave the existing line in place (preferred) or warn and skip.
- What happens when the `--adopt-edits` override is used on a file that has **not** diverged? It must be a no-op: no manifest rewrite, no content change.
- What happens when the user removes `claude-code-deploy` on a repo where the marker block or hook entry has been hand-deleted before uninstall? The remove path must treat the missing owned content as already-removed rather than erroring, and must still leave the rest of each file intact.
- What happens when the install is interrupted mid-way (user-initiated Ctrl-C between the two shared-file edits)? Whatever partial state was written must either be fully visible to the next remove invocation (so uninstall cleans up) or not persisted at all. No "half-installed" state that the manifest cannot describe.

## Requirements *(mandatory)*

### Functional Requirements

Six phase-prefixed FRs, one per line item of `docs/specere_v1.md` §5 Phase 1. IDs are pinned for traceability with the master list in §6 of that plan and are referenced by downstream `/speckit-tasks` issues.

- **FR-P1-001**: The `speckit` unit installer MUST NOT request no-git behaviour from the underlying SpecKit scaffolder when the target directory contains a `.git/` directory.
- **FR-P1-002**: When the `speckit` unit installer runs against a git-backed target, it MUST finish with the working tree on a feature branch whose name is `000-baseline` by default, or the value of the `SPECERE_FEATURE_BRANCH` override when that override is provided.
- **FR-P1-003**: Every unit installer MUST refuse to re-write any owned file whose current on-disk content hash differs from the hash the manifest recorded at the previous install, unless the user explicitly passes the `--adopt-edits` override — in which case the installer MUST update the manifest to record the user's current content as the new owner baseline, without overwriting it. If an owned file is missing entirely (deletion, not edit), `--adopt-edits` MUST refuse with a message directing the user to `specere remove <unit>` followed by `specere add <unit>` — deletion is a structural change and is out of `--adopt-edits` scope.
- **FR-P1-004**: The `claude-code-deploy` unit installer MUST append an entry for `.claude/settings.local.json` to the target repository's `.gitignore`, placed inside a SpecERE marker-fenced block owned by the `claude-code-deploy` unit; any pre-existing content in `.gitignore` MUST be preserved verbatim.
- **FR-P1-005**: The `claude-code-deploy` unit installer MUST register exactly one `after_implement` hook in `.specify/extensions.yml` that names the SpecERE implement-observer command; any pre-existing hooks in that file MUST be preserved verbatim.
- **FR-P1-006**: The `claude-code-deploy` unit remover MUST strip exactly the `after_implement` hook entry it registered at install time and the marker-fenced block it added to `.gitignore`; all other content in both shared files MUST remain bit-identical to the pre-remove state.

Supporting requirements, derived from the edge cases above:

- **FR-P1-007**: When the `speckit` unit installer runs against a git-backed target and records the feature branch it created, the manifest entry for that install MUST include the branch name and a boolean flag distinguishing "branch created by SpecERE" from "branch pre-existed" — both fields live on the unit's record as `install_config.branch_name` and `install_config.branch_was_created_by_specere`, so `specere remove <unit>` can read both from the same record.
- **FR-P1-008**: When any unit installer encounters a shared file whose existing content is syntactically invalid for its declared format, the installer MUST refuse to rewrite that file and MUST surface an actionable error naming the file and the parse failure. Declared formats covered: YAML (`.specify/extensions.yml`), TOML (`.specere/manifest.toml`, `.specere/sensor-map.toml`), JSON (`.specify/workflows/workflow-registry.json`), and plain text (`.gitignore`). Adding a new declared format to the harness implies extending this rule to cover it.
- **FR-P1-009**: Every bug enumerated in `docs/specere_v1.md` §4 that falls under Phase 1 (rows 1, 2, 3, 4) MUST have at least one regression test that fails against the pre-fix codebase and passes against the post-fix codebase.

### Key Entities

- **Unit**: A SpecERE-installable component with an install plan and a remove plan. Phase 1 touches two concrete units: `speckit` (wrapper over the upstream scaffolder) and `claude-code-deploy` (owner of the `.claude/` tree and of the SpecERE implement-observer hook).
- **Manifest record**: The per-unit entry in `.specere/manifest.toml` that names every file the unit installed, records a content hash per file, and — for the `speckit` unit after this phase — records the feature branch the install created.
- **Owned file**: A file whose content is under unit stewardship, recorded in the manifest with a hash. Divergence between the on-disk hash and the manifest hash is the trigger for the `--adopt-edits` gate.
- **Marker-fenced block**: A contiguous range of lines inside a shared file (e.g. `.gitignore`, `.specify/extensions.yml`) delimited by `specere:begin <unit>` / `specere:end <unit>` markers. The unit installer owns only the content between the markers; the rest of the file belongs to the user.
- **Hook entry**: A YAML mapping under the `hooks.after_implement` list in `.specify/extensions.yml` naming a command to run when a SpecKit `/speckit-implement` run concludes. The `claude-code-deploy` unit adds exactly one such entry pointing at the SpecERE implement-observer.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On a freshly cloned git-backed fixture repository, installing the `speckit` unit and then invoking the next slash command in the SpecKit workflow succeeds without a "not on a feature branch" error in 100% of runs across ten consecutive trials.
- **SC-002**: On the same repository, running `/speckit-clarify` after the install succeeds without manual intervention in 100% of runs across ten consecutive trials.
- **SC-003**: Re-running the same unit installer on a repository where the user has hand-edited one owned file results in zero bytes of user content being overwritten across 100% of trials without the adopt-edits override; 100% of the time the installer surfaces an error naming the diverged file.
- **SC-004**: Installing and then removing the `claude-code-deploy` unit on a fixture repository leaves `.gitignore` and `.specify/extensions.yml` bit-identical to their pre-install content in 100% of trials, measured by a byte-level diff against snapshots captured before the install.
- **SC-005**: After `claude-code-deploy` is installed on a fresh repository, exactly one entry appears under the `after_implement` hook list in `.specify/extensions.yml`; zero duplicate entries appear on any re-install.
- **SC-006**: After `claude-code-deploy` is installed, the `.gitignore` file contains exactly one marker-fenced block owned by that unit, and that block contains an entry for `.claude/settings.local.json`; zero duplicate blocks appear on re-install.
- **SC-007**: The integration test suite covering these user stories runs to green on a standard developer laptop in under five minutes total wall-clock time.
- **SC-008** (aspirational, non-blocking): A developer who has not seen SpecERE before can, by reading only the 0.2.0 release notes and `specere --help`, successfully install the two units on a fixture repository and reach a working `/speckit-clarify` invocation without external guidance. This is a usability signal captured in the 0.2.0 CHANGELOG if a session is performed, but it does **not** block the release tag. The blocking criteria for v0.2.0 are SC-001 through SC-007.

## Assumptions

- Phase 0 documentation work (README, CONTRIBUTING, CHANGELOG entries describing the pivot) has landed on `main` and is not part of this feature's scope; the Phase 1 release notes build on that baseline.
- The harness scaffold work on branch `000-harness` — including the constitution, the `specere-observe` workflow, and the empty `.specere/review-queue.md` — has landed on `main` before this feature is implemented, so the `after_implement` hook has a real observer command to reference even before that observer's body is implemented in Phase 3.
- The target repositories exercised in tests are git-backed by default. A separate regression test covers the non-git fallback for FR-P1-001, but the primary acceptance scenarios assume a git working tree.
- The default feature-branch name `000-baseline` matches the plan in `docs/specere_v1.md` §5 Phase 1. If a user overrides it via `SPECERE_FEATURE_BRANCH`, the override wins and no further branch-name transformation is applied.
- The implement-observer command referenced by the `after_implement` hook — `specere.observe.implement` — is expected to exist as a registered SpecERE slash command by the time Phase 3 ships. In Phase 1 the hook is registered but its invocation is a no-op stub; this is explicitly by design, per `docs/specere_v1.md` §5 Phase 1's "even if `specere observe` itself isn't implemented yet" clause.
- The SpecKit version pin (`0.7.3` per `.specify/init-options.json`) does not change within the life of the 0.2.0 release. A minor SpecKit bump is out of scope and deferred to a later patch release.
- "Bit-identical" in FR-P1-006 and SC-004 refers to the content of shared files, not to file metadata (mtime, mode bits), and does not apply to files whose owner is recorded as `UserEditedAfterInstall` — those are preserved with a warning, not byte-equal to pre-install state.
- `cargo-dist` is already configured for the repository, so the 0.2.0 release itself is a tag push plus CHANGELOG entry; release engineering work is not part of this spec.
