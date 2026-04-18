# Quickstart — Phase 1 Bugfix Release (0.2.0)

This walkthrough exercises every Phase 1 FR end-to-end against a fresh fixture repository. It doubles as the SC-008 aspirational script (the usability session, if one is performed, follows this flow).

## Prerequisites

- `specere` v0.2.0 binary on `$PATH`
- `specify` (from `uv tool install git+https://github.com/github/spec-kit@v0.7.3`) on `$PATH`
- `git` ≥ 2.30
- `claude` CLI (Claude Code) — optional for the `/speckit-clarify` step at the end

## Step 1 — fresh fixture repo

```sh
mkdir /tmp/speceret-qs && cd /tmp/speceret-qs
git init
git commit --allow-empty -m "initial"
```

## Step 2 — install `speckit`

```sh
specere add speckit
# Expected:
#   ✓ detected git repo; dropping --no-git
#   ✓ scaffolded .specify/ tree (v0.7.3)
#   ✓ created feature branch 000-baseline
#   ✓ recorded install in .specere/manifest.toml
```

**Exercise FR-P1-001/002/007.** Verify:

```sh
git branch --show-current    # → 000-baseline
grep -A2 'id = "speckit"' .specere/manifest.toml
# → install_config = { ..., branch_name = "000-baseline", branch_was_created_by_specere = true }
```

**Exercise FR-P1-001 negative.** Re-run `specere add speckit` on the same repo:

```sh
specere add speckit
# Expected: ✓ already installed (no-op, exit 0)
```

**Exercise FR-P1-003.** Hand-edit an owned file and re-install:

```sh
echo "oops" >> .specify/memory/constitution.md
specere add speckit
# Expected stderr:
#   specere: error: cannot re-install `speckit`; 1 owned file has been edited
#     help: run `specere add speckit --adopt-edits` to accept your changes
#     affected: .specify/memory/constitution.md
# Exit code 2.

specere add speckit --adopt-edits
# Expected: ✓ adopted 1 edit; manifest updated
```

## Step 3 — install `claude-code-deploy`

```sh
specere add claude-code-deploy
# Expected:
#   ✓ appended .claude/settings.local.json to .gitignore (marker-fenced)
#   ✓ registered after_implement hook in .specify/extensions.yml
#   ✓ installed 3 skills under .claude/skills/specere-*/SKILL.md
```

**Exercise FR-P1-004/005.** Verify:

```sh
grep -A1 "specere:begin claude-code-deploy" .gitignore
# → .claude/settings.local.json

grep -B1 -A6 "specere.observe.implement" .specify/extensions.yml
# → extension: specere, command: specere.observe.implement, optional: false
```

## Step 4 — round-trip remove

```sh
sha256sum .gitignore .specify/extensions.yml > /tmp/pre-snapshot.sha256
specere remove claude-code-deploy
sha256sum .gitignore .specify/extensions.yml > /tmp/post-snapshot.sha256
diff /tmp/pre-snapshot.sha256 /tmp/post-snapshot.sha256
```

**Empty diff** = FR-P1-006 satisfied (SC-004).

Then re-install to verify idempotence across the cycle:

```sh
specere add claude-code-deploy
# Expected: ✓ installed
```

## Step 5 — optional: exercise the real workflow

If Claude Code is installed:

```sh
claude -p "/speckit-clarify test feature"
# Expected: no "Not on a feature branch" error.
# This is the SC-002 headline — the bug that motivated Phase 1.
```

## Step 6 — remove both units (fresh-repo inverse)

```sh
specere remove claude-code-deploy
specere remove speckit --delete-branch   # deletes 000-baseline because branch_was_created_by_specere=true
# Expected: working tree on main, no .specify/, no .specere/, .gitignore is empty-or-untouched.
```

## What this quickstart does NOT cover

- FR-P1-008 (malformed-file refuse) — exercised by `crates/specere-units/tests/fr_p1_008_malformed_file_refuse.rs`, not by this walkthrough. Manual reproduction requires hand-corrupting `.specify/extensions.yml`, which is out of scope for a "happy path" quickstart.
- FR-P1-009 (regression-test discipline) — this is a CI property, not a user-facing flow.
- Phase 3's `specere observe record` body — the hook fires but the observer stub exits with a friendly "coming in v0.4.0" message. This is correct for v0.2.0.
