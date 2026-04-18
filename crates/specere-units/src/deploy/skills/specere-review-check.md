---
name: specere-review-check
description: Scan recent observations for un-mapped write surfaces, hook verbs, and OTel spans. Append pending items to .specere/review-queue.md. Invoked as the review-queue-check step of the specere-observe workflow.
argument-hint: "[scope: post-implement | post-analyze | adhoc]"
user-invocable: true
disable-model-invocation: false
---

# specere-review-check

Scan what the harness has observed in the current iteration and detect
anything the harness does **not yet cover**. This is the self-extension
detector required by constitution principle V.

## Procedure

1. Read `.specere/sensor-map.toml` and `.specere/manifest.toml` — the
   authoritative coverage sets.
2. Read the recent span store (once Phase 3 lands — until then, inspect the
   git diff of `.specify/` + the current `$FEATURE_DIR` + `.claude/skills/`).
3. For each observed surface NOT in the coverage sets, append a section to
   `.specere/review-queue.md` with:
   - `### [YYYY-MM-DD] <surface-label>`
   - `first_seen: <iso8601>`
   - `last_seen:  <iso8601>`
   - `sample: <short payload>`
   - `suggested_action: EXTEND | IGNORE | ALLOWLIST | ADJUDICATE`
   - `rationale: <why the detector flagged this>`
4. Collapse repeats: if a section with identical `<surface-label>` already
   exists, update `last_seen` + increment a `seen_count` field instead of
   creating a new section.
5. Summarize: if items were appended, print "⚠ N new review items — run
   `/specere-review-drain` or see `.specere/review-queue.md`". If no new
   items, print "✓ harness up-to-date".

## Coverage rules

The detector MUST flag:
- Hook verbs invoked with no `extension: specere` entry registered.
- OTel spans with `gen_ai.*` attributes not matched by any `sensor-map.toml` entry.
- Files written outside any unit's manifest file-list (native units only).
- Test outcomes not linked to any FR-NNN via `sensor-map.toml` coupling.

The detector MUST NOT flag:
- SpecKit-owned files (templates, scripts, skills under `speckit-*`).
- Files under `.git/`, `target/`, and other user-owned ignores.
- User-edited files already marked `Owner::UserEditedAfterInstall`.
