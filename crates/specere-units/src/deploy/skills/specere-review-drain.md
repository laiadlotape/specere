---
name: specere-review-drain
description: Walk .specere/review-queue.md interactively; for each open item, ask the user EXTEND / IGNORE / ALLOWLIST / ADJUDICATE, record the decision in .specere/decisions.log, and move the item to the Closed section.
argument-hint: "[--all | --category=<label>]"
user-invocable: true
disable-model-invocation: false
---

# specere-review-drain

Interactive dogfeed review of the self-extension queue.

## Procedure

1. Read `.specere/review-queue.md`; parse sections under `## Open items`.
2. For each open item, prompt the user with the four decision options
   (use the `ask_user_question` pattern — present the item's surface,
   sample, and suggested action as context):
   - **EXTEND** — add to `sensor-map.toml` (ask channel A/B/C/D) or file
     a unit-extension TODO in `docs/roadmap/`. Preferred when the surface is
     novel but informative.
   - **IGNORE** — add to a `sensor-map.toml` ignore-list. Use for
     genuinely ephemeral surfaces (e.g., OS caches outside repo scope).
   - **ALLOWLIST** — mark as "known-but-not-load-bearing"; still logged
     but never raises the review queue.
   - **ADJUDICATE** — for divergences (posterior disagrees with reality
     per core_theory §4); prompts for a labeled training sample and logs it.
3. Append one line to `.specere/decisions.log`:
   `<iso8601>\t<surface-label>\t<decision>\t<user-rationale>`
4. Move the item's section from `## Open items` to `## Closed items`
   with a `closed_at:` field added.
5. At completion, print a summary: `{n_extended, n_ignored, n_allowlisted, n_adjudicated}`.

## Invariants

- Never auto-decide. Always ask.
- Never silently drop items; closing requires a logged decision.
- Never modify files outside `.specere/` or `docs/roadmap/`.
- Per constitution principle IV: drain sessions happen at the
  divergence-adjudication layer. Do not invoke during per-tool-call flow.
