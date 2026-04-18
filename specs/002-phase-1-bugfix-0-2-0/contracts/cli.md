# Contract: `specere` CLI surface — Phase 1 delta

Every new / changed flag and exit code introduced by v0.2.0. Flags not listed are unchanged from 0.1.0-dev.

## `specere add <unit>`

| Flag | Type | Default | Semantics |
|---|---|---|---|
| `<unit>` | positional String | — | Unit id (registry-resolved). v0.2.0 ships `speckit` and `claude-code-deploy`. |
| `--adopt-edits` | Boolean | false | If set, and an owned file has a content SHA mismatch, accept the on-disk content as the new owner baseline (update manifest). Refuses if a file is entirely missing — see `--help` text. |
| `--branch <name>` | String | `"000-baseline"` | **`speckit` unit only.** Override the auto-created feature branch name. Also respects `$SPECERE_FEATURE_BRANCH` env var (CLI flag wins). No-op on non-git targets. |
| `--force` | Boolean | false | *Reserved.* In 0.2.0 `--force` only bypasses the SHA-diff gate **when combined with** `--adopt-edits`. Standalone `--force` still refuses (FR-P1-003) to prevent regressions. |
| `--dry-run` | Boolean | false | Pre-existing. Prints the `Plan` and exits 0. |

**Exit codes**:
- `0` — install succeeded (or was a no-op idempotent).
- `1` — generic installer error.
- `2` — `Error::AlreadyInstalledMismatch` (SHA-diff gate). Stderr names the file(s) and cites `--adopt-edits`.
- `3` — `Error::ParseFailure` (malformed YAML / TOML / JSON / text in a file we own part of). Stderr names the file and format.
- `4` — `Error::DeletedOwnedFile` (owned file missing, `--adopt-edits` refused). Stderr directs to `specere remove && specere add`.
- `5` — `Error::SpecifyCliMissing` (pre-existing) — `specify` not on `$PATH`.

**Stderr format** for user-visible errors:

```
specere: error: <one-line summary>
  help: <actionable next step, naming concrete paths or flags>
```

Example for exit 2:
```
specere: error: cannot re-install `claude-code-deploy`; 1 owned file has been edited
  help: run `specere add claude-code-deploy --adopt-edits` to accept your changes
  affected: .gitignore (sha256 changed since last install)
```

## `specere remove <unit>`

| Flag | Type | Default | Semantics |
|---|---|---|---|
| `<unit>` | positional String | — | Unit id. |
| `--delete-branch` | Boolean | false | **`speckit` unit only.** If the manifest records `branch_was_created_by_specere = true` for this unit, delete the branch. Refuses if the branch is dirty (uncommitted changes) or currently checked out. Never deletes a branch where `branch_was_created_by_specere = false`. |
| `--force` | Boolean | false | Bypass the "user-edited file preservation" rule for this remove only. Deletes even user-edited files. Dangerous — documented as such in `--help`. |
| `--dry-run` | Boolean | false | Pre-existing. |

**Exit codes**:
- `0` — remove succeeded.
- `1` — generic remove error.
- `6` — `Error::BranchDirty` (only with `--delete-branch` + dirty working tree). Stderr: `git stash` suggestion.
- `7` — `Error::BranchNotOurs` (only with `--delete-branch` when `branch_was_created_by_specere = false`).

## Interaction with `uvx specify-cli`

The `speckit` unit's `install` function shells out to `uvx --from git+https://github.com/github/spec-kit@v0.7.3 specify init . --integration claude --ai-skills --force` **iff** `$PATH` contains a working `specify` binary at the correct version — otherwise it falls back to invoking `specify` directly (if the binary was installed via `uv tool install` previously). No new flag semantics — only the `--no-git` drop is behavioral.

## Backward compatibility

A 0.1.x user who runs v0.2.0's `specere add speckit` sees:
1. The manifest's `install_config` gains `branch_name` + `branch_was_created_by_specere`.
2. The working tree ends on `000-baseline` (or the user's override).
3. A new section appears in `.specify/extensions.yml` inside a `specere:begin claude-code-deploy` fenced block (only if `claude-code-deploy` is also added).

No 0.1.x artifacts are removed; user can still `specere remove speckit` on a 0.1.x manifest (branch ops are just no-ops).
