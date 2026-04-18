---
name: specere-lint-ears
description: Scan the active feature's spec.md Functional Requirements against EARS-style rules from .specere/lint/ears.toml. Emits advisory warnings, never blocks. Invoked from the before_clarify extension hook registered by the ears-linter unit.
argument-hint: "[no args — reads active feature dir from .specify/feature.json]"
user-invocable: true
disable-model-invocation: false
---

# specere-lint-ears

Advisory EARS-style lint for SpecKit functional requirements. **Never block a command** — print warnings, then return control. Per FR-P2-003, the `before_clarify` hook that invokes this skill is registered with `optional: true` in `.specify/extensions.yml`.

## Procedure

1. **Locate the active feature dir.** Read `.specify/feature.json`; extract `feature_directory`. If absent, print one line ("no active feature — skipping ears lint") and exit 0.
2. **Load rules.** Parse `.specere/lint/ears.toml`. Schema: an array of `[[rules]]` tables, each with `id`, `severity` (`"error"` / `"warning"` / `"info"`), `description`, `scope`, `pattern` (regex), and optional flags `condition_only` / `bad_match`.
3. **Scan spec.md.** Read `<feature_dir>/spec.md`. Locate the `### Functional Requirements` section (if any); collect its bullet lines.
4. **For each rule with `scope = "functional-requirements"`**:
   - If `bad_match = true`: emit a warning for every bullet whose text MATCHES the pattern (this is the "avoid these adjectives" shape).
   - Otherwise: emit a warning for every bullet that does NOT match the pattern.
   - `condition_only = true`: only apply the rule to bullets that already contain a condition keyword (`WHEN|WHILE|WHERE|IF|when ... then ...`).
5. **Output** as a bullet list, grouped by rule. One line per finding:
   ```
   [WARN ears-must-should] FR-P3-002 — bullet does not contain MUST/SHOULD.
   ```
6. **Do not modify spec.md.** Findings are read-only. A clarify gate or human can act on them; the lint itself is advisory.
7. Exit 0 always (even if findings present).

## Invariants

- **Never block.** This skill's sole output is stdout warnings. The `before_clarify` hook must NOT cause `/speckit-clarify` to exit non-zero.
- **Never touch the filesystem outside of a read.** No edits to spec.md, no writes to `.specere/`.
- **Rules-only.** Do not invent findings not expressible in `ears.toml`. If a rule you'd want isn't encoded, file it as a new `[[rules]]` entry via a PR, not via runtime judgment.
- **Fail gracefully.** If `ears.toml` is missing / malformed, print one line ("ears-linter rules missing") and exit 0. The unit install path guarantees the file's presence on SpecERE-scaffolded repos.

## When NOT to use this skill

- On a repo that hasn't installed the `ears-linter` unit (no `.specere/lint/ears.toml`).
- For anything other than FR lint — domain-specific checks go in a separate skill.
