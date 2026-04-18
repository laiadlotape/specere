# Releasing SpecERE

`specere` releases are driven by [`cargo-dist`](https://github.com/axodotdev/cargo-dist) (pinned to `0.31.x` via `dist-workspace.toml`) plus a hand-written guard workflow that validates the tag before cargo-dist uploads any artifact. This document is for maintainers cutting a release.

## What produces a release

A single git tag push matching `v<major>.<minor>.<patch>` on `main` (e.g. `v0.2.0`) triggers two workflows in parallel:

1. **`release-guards.yml`** (SpecERE-owned, hand-written) — validates the tag is sane *before* any build work. Runs in seconds.
2. **`release.yml`** (cargo-dist-generated — do not hand-edit) — cross-compiles the `specere` binary for five target triples, bundles them, writes the `shell` and `powershell` installer scripts, and uploads everything to the GitHub Release page for the tag.

A red X on `release-guards.yml` with a green checkmark on `release.yml` is a mistake state — delete the tag, fix the mismatch, retag. See § Guards failure modes.

## Tag-cut procedure

Cut from a clean `main` at the commit you want to release.

1. **Bump the workspace version.** Edit `Cargo.toml` top-level `[workspace.package]` — change `version = "X.Y.Z-dev"` to `version = "X.Y.Z"`.
2. **Move the CHANGELOG section.** Rename `## [Unreleased]` to `## [X.Y.Z] - YYYY-MM-DD`. Add a fresh empty `## [Unreleased]` above it. The `[X.Y.Z]` body becomes the GitHub Release notes.
3. **Update `Cargo.lock`.** Run `cargo build` — picks up the version bump without rebuilding everything.
4. **Commit + PR + merge** to `main`. The PR title should be `release: vX.Y.Z`. Normal CI gates apply (rustfmt, clippy, test matrix, docs-sync, review).
5. **Tag the merge commit.**
   ```sh
   git checkout main && git pull --ff-only
   git tag -a vX.Y.Z -m "release: vX.Y.Z"
   git push origin vX.Y.Z
   ```
6. **Wait.** `release-guards.yml` completes in under a minute; `release.yml` completes in 10–20 minutes. The GitHub Release page will be auto-created with your CHANGELOG body and five binaries + two installers attached.

Total maintainer-interactive time: < 10 minutes (SC-001). CI wall-clock for the release build: ~15–20 min.

## Guards failure modes

If `release-guards.yml` posts a red X on the tag commit, *do not continue* — rollback and fix. The three guards are:

### Guard 1 — tag name matches `Cargo.toml` version (FR-RI-003)

> `Tag 'vX.Y.Z' does not match Cargo.toml version 'A.B.C'. Delete the tag, bump version (or fix the tag), and retag.`

Cause: you pushed a tag before step 1 of the tag-cut procedure (or the two drifted). Fix: `git tag -d vX.Y.Z && git push --delete origin vX.Y.Z`; rerun the procedure from step 1.

### Guard 2 — CHANGELOG section exists (FR-RI-004)

> `CHANGELOG.md is missing '## [X.Y.Z]' section. …`

Cause: you skipped step 2 of the tag-cut procedure. Fix: delete the tag, add the CHANGELOG section via a follow-up PR, retag.

### Guard 3 — tag commit is reachable from main (FR-RI-005)

> `Tag commit <sha> is not reachable from origin/main. Releases are cut from main only.`

Cause: you tagged a commit on a feature branch instead of on the merged `main`. Fix: delete the tag, rebase onto main (if the work hasn't merged), or tag the actual merge commit.

## Local reproduction

Before cutting a real release, run the release plan locally:

```sh
dist plan                                   # list targets, artifacts, installers
dist build --target x86_64-unknown-linux-gnu # build just your host's binary
```

No network access is required for `dist plan` (SC-005). `dist build` only fetches Rust dependencies, not cargo-dist itself.

## Rollback

A mis-cut tag can be fully unwound:

```sh
# 1. delete the local tag
git tag -d vX.Y.Z
# 2. delete the remote tag
git push --delete origin vX.Y.Z
# 3. delete the auto-created GitHub Release (UI or gh CLI)
gh release delete vX.Y.Z --yes
```

No `main` rewrite needed; the tag push is the only mutation. The repo's post-rollback state is bit-identical to its pre-tag state (SC-004, principle III).

## Deferred

Not handled by this spec; follow-up specs track them:

- **crates.io publish** — deferred to a separate `release-infra-crates-io` spec. Today, `specere` is install-from-git-or-binary only.
- **Homebrew tap / Scoop / .msi / .deb / .rpm** — deferred; cargo-dist supports them via `installers = [...]` but Phase 1 scope is shell + powershell only.
- **Update-channel / self-updater** — deferred; `axoupdater` integration is a future spec.
