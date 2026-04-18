# Contract: `.specere/manifest.toml` — schema v1, Phase 1 delta

Full schema spec lives in `../data-model.md §1`; this file states the **stability contract** and the **migration path**.

## Schema version

- **Written by v0.2.0**: `schema_version = 1` (unchanged from v0.1.x).
- The field additions (`install_config.branch_name`, `install_config.branch_was_created_by_specere`) are **additive** only. Optional on read; required on write *iff* the target is a git repo.
- A v0.1.x manifest read by v0.2.0 succeeds; missing fields default as described in `../data-model.md §1` backwards-compatibility note.
- A v0.2.0 manifest read by v0.1.x will (per the toml crate's behavior) ignore unknown fields. v0.1.x cannot act on the branch-record, but also does not corrupt it.

## Field-stability guarantees

The following field names and semantics are stable across any v0.2.x release:

- `meta.specere_version`, `meta.schema_version`, `meta.created_at`
- `units[].id`, `units[].kind`, `units[].version`, `units[].installed_at`
- `units[].install_config.integration` (for `speckit`)
- `units[].install_config.branch_name` (new)
- `units[].install_config.branch_was_created_by_specere` (new)
- `units[].files[].path`, `units[].files[].sha256_post`, `units[].files[].owner`, `units[].files[].role`
- `units[].markers[].path`, `units[].markers[].begin`, `units[].markers[].end`

The `owner` value set `{"specere", "speckit", "user-edited-after-install"}` is closed; a new value (e.g. `"user-deleted-after-install"` proposed in v0.3) would bump `schema_version` to 2.

## Deprecations

- `meta.branch_at_init` — written by v0.1.x, retained by v0.2.0 reads, **not written** by v0.2.0. Field is informational. v1.0.0 will remove it.

## Example — v0.2.0 manifest after `specere init` on a git repo

```toml
[meta]
specere_version = "0.2.0"
schema_version  = 1
created_at      = "2026-04-18T10:00:00Z"

[[units]]
id           = "speckit"
kind         = "wrapper"
version      = "v0.7.3"
installed_at = "2026-04-18T10:00:01Z"
install_config = { integration = "claude", script = "sh", ai_skills = true, offline = false, branch_name = "000-baseline", branch_was_created_by_specere = true }

[[units]]
id           = "claude-code-deploy"
kind         = "native"
version      = "0.2.0"
installed_at = "2026-04-18T10:00:02Z"
install_config = {}

  [[units.files]]
  path        = ".claude/skills/specere-observe-implement/SKILL.md"
  sha256_post = "abc123…"
  owner       = "specere"
  role        = "skill"

  [[units.markers]]
  path  = ".gitignore"
  begin = "# <!-- specere:begin claude-code-deploy -->"
  end   = "# <!-- specere:end claude-code-deploy -->"

  [[units.markers]]
  path  = ".specify/extensions.yml"
  begin = "# >>> specere:begin claude-code-deploy"
  end   = "# <<< specere:end claude-code-deploy"
```
