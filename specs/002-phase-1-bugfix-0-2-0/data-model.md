# Data Model — Phase 1 Bugfix Release (0.2.0)

Two schemas are extended by this feature: the SpecERE-native `.specere/manifest.toml` and the SpecKit-owned `.specify/extensions.yml` (mutated only inside marker-fenced blocks).

## 1. `.specere/manifest.toml` — extended schema v1

Schema version stays at `1`; Phase 1 adds fields to existing tables without breaking older manifests. A 0.1.x manifest parses cleanly under 0.2.0 (missing fields default).

### `[meta]` table (unchanged)

| Field | Type | Required | Notes |
|---|---|---|---|
| `specere_version` | String | yes | semver of the writing binary |
| `schema_version` | Integer | yes | currently `1` |
| `created_at` | RFC3339 String | yes | set at first install |
| `branch_at_init` | String | no | *existing field, deprecated by this phase* — observational only, replaced by unit-level `install_config.branch_name` + `branch_was_created_by_specere`. Retained for backwards-read; not written by 0.2.0+ |

### `[[units]]` array (extended)

Shape unchanged. Each entry is a table.

| Field | Type | Required | Notes |
|---|---|---|---|
| `id` | String | yes | e.g. `"speckit"`, `"claude-code-deploy"` |
| `kind` | String | yes | `"wrapper"` or `"native"` |
| `version` | String | yes | semver or upstream tag |
| `installed_at` | RFC3339 String | yes | |
| `install_config` | Inline Table | yes | **extended** — see below |
| `files` | Array of Tables | yes (native only) | per §1.3 |
| `markers` | Array of Tables | yes (native only) | per §1.4 |

### `[[units]].install_config` — inline table, **extended in Phase 1**

| Field | Type | Required | Units | Notes |
|---|---|---|---|---|
| `integration` | String | no | speckit only | `"claude"` for v1 |
| `script` | String | no | speckit only | `"sh"` or `"ps"` |
| `ai_skills` | Boolean | no | speckit only | true by default |
| `offline` | Boolean | no | speckit only | whether `--offline` was passed |
| **`branch_name`** | String | no | speckit only | **Phase 1 NEW.** The feature branch name the installer finished on. May be the auto-created default (`000-baseline`) or a user override. Absent iff the target was non-git. |
| **`branch_was_created_by_specere`** | Boolean | no | speckit only | **Phase 1 NEW.** `true` iff the installer created this branch from scratch; `false` iff the branch pre-existed and SpecERE only switched to it. Used by `specere remove speckit --delete-branch` to refuse deletion of user-created branches. |

### `[[units.files]]` (unchanged)

Per-file ownership record for native units only.

| Field | Type | Required | Notes |
|---|---|---|---|
| `path` | String | yes | repo-relative |
| `sha256_post` | String | yes | SHA256 of post-install content, or `"SELF"` for manifest, or `"PENDING"` for template-authored |
| `owner` | String | yes | `"specere"`, `"speckit"`, `"user-edited-after-install"` |
| `role` | String | yes | freeform tag — `"constitution"`, `"skill"`, `"workflow"`, etc. |

### `[[units.markers]]` (unchanged)

Marker-fenced block records for native units.

| Field | Type | Required | Notes |
|---|---|---|---|
| `path` | String | yes | file the unit owns a fenced block in |
| `begin` | String | yes | exact begin marker text |
| `end` | String | yes | exact end marker text |

### Backwards compatibility

A 0.1.x manifest (no `branch_name` / `branch_was_created_by_specere`) is read by 0.2.0 as if both fields were absent: `remove` falls back to "do not attempt to touch any git branch" behavior, matching 0.1.x semantics. An upgrade pass (`specere migrate`) is out of scope for v0.2.0 — the next `specere add speckit --force --adopt-edits` will re-record the missing fields.

---

## 2. `.specify/extensions.yml` — safe-mutation contract

SpecERE does **not** own this file. SpecKit owns it; the git extension already writes 17 entries. SpecERE mutates only inside a marker-fenced block under each `hooks.<verb>:` list.

### Marker convention

Because YAML block-sequence items cannot contain HTML comments outside the item body, we fence at the **list-item comment level** using YAML line comments (not HTML):

```yaml
  after_implement:
  - extension: git
    command: speckit.git.commit
    …
  # >>> specere:begin claude-code-deploy
  - extension: specere
    command: specere.observe.implement
    enabled: true
    optional: false
    prompt: …
    description: …
    condition: null
  # <<< specere:end claude-code-deploy
```

`specere-markers::yaml_block_fence` parses `# >>> specere:begin <unit-id>` and `# <<< specere:end <unit-id>` as line-comment markers. The `.specere/manifest.toml → [[units.markers]]` entry for `claude-code-deploy` records these exact strings.

### Invariants

1. **Add**: insert the block at the tail of the relevant `hooks.<verb>:` list. If the verb key is absent, create it. If `hooks:` itself is absent, create it.
2. **Remove**: locate the marker pair by unit-id, excise the enclosed lines *and the marker lines themselves*, collapse adjacent blank lines to one. Preserve every other byte.
3. **Refuse**: if `serde_yaml::from_str` fails on the pre-mutation file, exit non-zero with `Error::ParseFailure`. Never attempt to repair.
4. **Idempotent add**: if a specere block with the same unit-id already exists, the add is a no-op (zero bytes changed).

---

## 3. `.gitignore` — safe-mutation contract

Plain text; fence with HTML-style comments:

```
# <!-- specere:begin claude-code-deploy -->
.claude/settings.local.json
# <!-- specere:end claude-code-deploy -->
```

Invariants identical to §2 but line-oriented: marker lines are whole lines; enclosed content is zero or more lines between them.

---

## 4. Entity relationships

```text
Unit ───────────────────────────────────────────────────────────────
  id, kind, version, installed_at, install_config                   │
  │                                                                 │
  ├─(native only)─► [[units.files]] ─► path, sha256_post, owner     │
  │                                                                 │
  └─(native only)─► [[units.markers]] ─► path, begin, end ──┐       │
                                                             │       │
                                                             ▼       │
                                                    MarkerFence ─── co-owns ───► Shared file
                                                    (CLAUDE.md, .gitignore, extensions.yml)
```

SpecKit's own manifests (`.specify/integrations/*.manifest.json`) are **disjoint** — SpecERE does not read or modify them; the `speckit` wrapper unit delegates to `specify integration uninstall claude` for that half.

## 5. State transitions

The `Unit` lifecycle has five states tracked via manifest presence + SHA match:

1. **Uninstalled** — no manifest entry.
2. **Installed-clean** — manifest entry present; every `files.path` hashes to `sha256_post`.
3. **Installed-with-user-edits** — manifest entry present; ≥ 1 file hashes differently. Files with mismatch are flipped to `owner = user-edited-after-install` by the next `--adopt-edits` pass.
4. **Installed-with-deletions** — manifest entry present; ≥ 1 file is missing from disk. `--adopt-edits` refuses (clarified); user runs `remove && add` to re-enter state 2.
5. **Installed-orphan** — manifest entry present; unit ID no longer in the binary's registry. `specere verify` flags; `specere remove` still works because `files`/`markers` tables are self-describing.

Transitions and their guards:

| From | Via | To |
|---|---|---|
| Uninstalled | `specere add <unit>` | Installed-clean |
| Installed-clean | user edits a file | Installed-with-user-edits |
| Installed-clean | user deletes a file | Installed-with-deletions |
| Installed-clean | `specere add <unit>` (idempotent no-op) | Installed-clean |
| Installed-with-user-edits | `specere add <unit>` (no flag) | **REFUSE** (FR-P1-003) |
| Installed-with-user-edits | `specere add <unit> --adopt-edits` | Installed-clean (with user content recorded) |
| Installed-with-deletions | `specere add <unit> --adopt-edits` | **REFUSE** (clarified) |
| Any Installed-* | `specere remove <unit>` | Uninstalled |
